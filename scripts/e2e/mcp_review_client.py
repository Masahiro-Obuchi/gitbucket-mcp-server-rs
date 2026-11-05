"""Minimal stdio MCP client for the live review-read contract probe only."""
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time


class McpReviewClient:
    def __init__(self, binary, url, token, username, password):
        self.config = tempfile.TemporaryDirectory(prefix="gitbucket-mcp-review-")
        env = {k: v for k, v in os.environ.items() if not k.startswith("GITBUCKET_")}
        env.update({
            "GITBUCKET_URL": url, "GITBUCKET_TOKEN": token,
            "GITBUCKET_USERNAME": username, "GITBUCKET_PASSWORD": password,
            "GITBUCKET_API_PROFILE": "standard", "GITBUCKET_READ_ONLY": "true",
            "GITBUCKET_MCP_CONFIG_DIR": self.config.name,
        })
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
            assert len(tools) == 16
            for name in ["list_pull_request_review_comments", "get_pull_request_review_comment"]:
                tool = next(t for t in tools if t["name"] == name)
                assert tool["annotations"]["readOnlyHint"] is True
                assert tool["inputSchema"]["additionalProperties"] is False
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
        result = self.rpc("tools/call", {"name": name, "arguments": args})
        if result.get("isError"):
            kind = result.get("structuredContent", {}).get("kind", "unknown")
            raise RuntimeError("MCP review tool failed: " + kind)
        return result["structuredContent"]

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
