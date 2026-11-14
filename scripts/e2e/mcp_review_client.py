"""Minimal stdio MCP client for disposable review contract probes only."""
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time


class McpReviewClient:
    def __init__(self, binary, url, token, username, password, *, read_only=True, web_auth=True, profile="standard"):
        self.config = tempfile.TemporaryDirectory(prefix="gitbucket-mcp-review-")
        env = {k: v for k, v in os.environ.items() if not k.startswith("GITBUCKET_")}
        env.update({
            "GITBUCKET_URL": url, "GITBUCKET_TOKEN": token,
            "GITBUCKET_API_PROFILE": profile, "GITBUCKET_READ_ONLY": "true" if read_only else "false",
            "GITBUCKET_MCP_CONFIG_DIR": self.config.name,
        })
        if web_auth:
            env.update({"GITBUCKET_USERNAME": username, "GITBUCKET_PASSWORD": password})
        self.process = subprocess.Popen([str(Path(binary).resolve())], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                        text=True, encoding="utf-8", bufsize=1, env=env)
        self.messages = queue.Queue()
        self.sequence = 0
        threading.Thread(target=self._read, daemon=True).start()
        try:
            self.rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                    "clientInfo": {"name": "review-contract-probe", "version": "1"}})
            self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
            tools = self.rpc("tools/list", {})["tools"]
            assert len(tools) == (21 if read_only else 44)
            for name in ["list_pull_request_review_comments", "get_pull_request_review_comment"]:
                tool = next(t for t in tools if t["name"] == name)
                assert tool["annotations"]["readOnlyHint"] is True
                assert tool["inputSchema"]["additionalProperties"] is False
            if not read_only:
                post = next(t for t in tools if t["name"] == "add_pull_request_review_comment")
                assert post["annotations"]["readOnlyHint"] is False
                assert post["annotations"]["idempotentHint"] is False
                assert "head_guard" in post["inputSchema"]["required"]
        except BaseException:
            self.close()
            raise

    def _read(self):
        try:
            for line in self.process.stdout:
                self.messages.put(json.loads(line))
        finally:
            self.messages.put(None)

    def send(self, message):
        self.process.stdin.write(json.dumps(message, ensure_ascii=False) + "\n")
        self.process.stdin.flush()

    def rpc(self, method, params):
        self.sequence += 1
        identifier = self.sequence
        self.send({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params})
        deadline = time.monotonic() + 45
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise RuntimeError("MCP probe response deadline exceeded")
            try:
                message = self.messages.get(timeout=remaining)
            except queue.Empty:
                raise RuntimeError("MCP probe response deadline exceeded") from None
            if message is None:
                raise RuntimeError("MCP probe server closed its output")
            if message.get("id") != identifier:
                continue
            if "error" in message:
                raise RuntimeError("MCP probe received a protocol error")
            return message["result"]

    def call(self, name, args):
        args = dict(args)
        if name == "list_pull_request_review_comments":
            # This probe compares complete observed models, including rendered
            # bodies. Production list defaults deliberately return summaries.
            args["view"] = "full"
        result = self.rpc("tools/call", {"name": name, "arguments": args})
        if result.get("isError"):
            kind = result.get("structuredContent", {}).get("kind", "unknown")
            raise RuntimeError("MCP review tool failed: " + kind)
        data = result["structuredContent"]
        if name == "list_pull_request_review_comments":
            observed = data["range"]["observation_id"]
            while data["has_more"]:
                args["cursor"] = data["next_cursor"]
                page = self.rpc("tools/call", {"name": name, "arguments": args})
                assert not page.get("isError")
                page = page["structuredContent"]
                assert page["range"]["observation_id"] == observed
                assert page["range"]["source_complete"] is False
                assert page["coverage"] == data["coverage"]
                data["comments"].extend(page["comments"])
                data["has_more"], data["next_cursor"] = page["has_more"], page["next_cursor"]
        return data

    def close(self):
        if self.process.stdin and not self.process.stdin.closed:
            self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)
        if self.process.stdout:
            self.process.stdout.close()
        self.config.cleanup()
