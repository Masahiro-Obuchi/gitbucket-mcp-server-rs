#!/usr/bin/env python3
"""Run contract checks only against a fresh, disposable official Docker image.

Private child logs/runtime/probe observations are discarded. The only public
artifact is a constructed allowlist of stage statuses and image/commit IDs.
"""
import argparse
import json
import os
from pathlib import Path
import re
import secrets
import signal
import subprocess
import sys
import tempfile
import time

from ci_plan import DOMAINS, VERSIONS
from mcp_review_client import McpReviewClient
from runtime import read_runtime

REPO = Path(__file__).resolve().parents[2]
PAGING_TESTS = ("test_e2e_list_paged_resources_over_25_items", "test_e2e_list_unpaged_resources_over_100_items")


class StageFailed(Exception):
    """No child output/argv in public exceptions."""


def execute(args, env, log, timeout=900):
    with log.open("wb") as stream:
        child = subprocess.Popen(args, cwd=REPO, env=env, stdout=stream,
                                 stderr=subprocess.STDOUT, start_new_session=True)
        try:
            if child.wait(timeout=timeout):
                raise StageFailed()
        finally:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait(timeout=5)


def diagnostics(binary, env, scratch):
    config = scratch / "config"
    config.mkdir(mode=0o700)
    (config / "config.toml").write_text("unused malformed TOML [")
    app = {k: v for k, v in env.items() if not k.startswith("GITBUCKET_")}
    app.update({"GITBUCKET_URL": env["GITBUCKET_E2E_URL"], "GITBUCKET_TOKEN": env["GITBUCKET_E2E_TOKEN"],
                "GITBUCKET_MCP_CONFIG_DIR": str(config), "GITBUCKET_API_PROFILE": "standard"})
    for command in ("--check-config", "--check-connection"):
        result = subprocess.run([str(binary), command], env=app, capture_output=True, timeout=30)
        if result.returncode:
            raise StageFailed()
        data = json.loads(result.stdout)
        assert data["status"] == "ok"
        assert data["connection_checked"] == (command == "--check-connection")
        assert data["web_session_checked"] is False
        assert data["web_credentials_configured"] is False
        assert env["GITBUCKET_E2E_TOKEN"].encode() not in result.stdout + result.stderr
    # Prove PAT-only stdio, schema registration and enforced read-only access.
    client = McpReviewClient(binary, app["GITBUCKET_URL"], app["GITBUCKET_TOKEN"], "", "", web_auth=False)
    try:
        user = client.call("get_authenticated_user", {})
        assert user["login"] == env["GITBUCKET_E2E_GIT_USERNAME"]
        blocked = client.rpc("tools/call", {"name": "create_issue", "arguments": {
            "owner": env["GITBUCKET_E2E_OWNER"], "repo": env["GITBUCKET_E2E_REPO"], "title": "must not be written"}})
        assert blocked["structuredContent"]["kind"] == "read_only_error"
    finally:
        client.close()


def image_identity(env):
    image = env["GITBUCKET_E2E_IMAGE"]
    data = json.loads(subprocess.check_output(["docker", "image", "inspect", image], stderr=subprocess.DEVNULL))[0]
    identity = data["Id"]
    digests = data.get("RepoDigests", [])
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", identity):
        raise StageFailed()
    official = [d for d in digests if re.fullmatch(r"ghcr.io/gitbucket/gitbucket@sha256:[0-9a-f]{64}", d)]
    if not official:
        raise StageFailed()
    return {"id": identity, "digests": official}


def source_identity():
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO).decode().strip()
    if not re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", revision):
        raise StageFailed()
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=REPO))
    return {"commit": revision, "worktree_dirty": dirty}


def run(version, domains, output, runtime_root, target, port, project):
    # Never borrow deployment credentials, URLs, image or repository overrides.
    env = {k: v for k, v in os.environ.items() if not k.startswith(("GITBUCKET_", "GIT_"))}
    env.update({"GITBUCKET_E2E_IMAGE": "ghcr.io/gitbucket/gitbucket:" + version,
                "GITBUCKET_E2E_COMPOSE_PROJECT": project, "GITBUCKET_E2E_ROOT": str(runtime_root),
                "GITBUCKET_E2E_ENV_FILE": str(runtime_root / "runtime.env"),
                "GITBUCKET_E2E_HTTP_PORT": str(port), "GITBUCKET_E2E_BASE_URL": f"http://127.0.0.1:{port}",
                "GITBUCKET_E2E_USER": "acceptance-owner", "GITBUCKET_E2E_PASSWORD": secrets.token_urlsafe(32),
                "GITBUCKET_E2E_API_PROFILE": "standard", "TOKIO_WORKER_THREADS": "2"})
    report = {"schema": 1, "version": version, "domains": domains, "passed": False, "stages": [],
              "transport": "loopback HTTP", "context_path": "/", "plugins": "official bundled image only"}
    stage_name = "setup"
    with tempfile.TemporaryDirectory(prefix="gitbucket-acceptance-private-") as private:
        scratch = Path(private)

        def stage(name, action):
            nonlocal stage_name
            stage_name = name
            start = time.monotonic()
            record = {"name": name, "passed": False}
            report["stages"].append(record)
            print(name + ": running", flush=True)
            try:
                action()
                record["passed"] = True
            finally:
                record["seconds"] = round(time.monotonic() - start, 2)

        def command(name, args, timeout=900, test_run=False):
            def action():
                log = scratch / (name + ".log")
                execute(args, env, log, timeout)
                if test_run:
                    contents = log.read_text(errors="replace")
                    counts = re.findall(r"test result: ok\. ([0-9]+) passed; 0 failed;", contents)
                    if len(counts) != 1 or int(counts[0]) == 0:
                        raise StageFailed()
                    if "paging" in domains and not all(test + " ... ok" in contents for test in PAGING_TESTS):
                        raise StageFailed()
                    report["stages"][-1]["tests_passed"] = int(counts[0])
            stage(name, action)

        try:
            stage("source", lambda: report.update(source=source_identity()))
            command("build", ["cargo", "build", "--locked", "--target-dir", str(target)])
            binary = target / "debug/gitbucket-mcp-server"
            command("bootstrap", ["bash", "scripts/e2e/bootstrap.sh"], 300)
            env.update(read_runtime(runtime_root / "runtime.env.json"))
            stage("image", lambda: report.update(image=image_identity(env)))
            if "startup" in domains:
                stage("startup", lambda: diagnostics(binary, env, scratch))
            cargo = ["cargo", "test", "--locked", "--target-dir", str(target), "--test", "e2e_test"]
            if {"core", "paging"}.issubset(domains):
                command("core-and-paging", cargo + ["--", "--ignored"], test_run=True)
            elif "core" in domains:
                command("core", cargo + ["--", "--ignored", *[item for name in PAGING_TESTS for item in ("--skip", name)]], test_run=True)
            elif "paging" in domains:
                command("paging", cargo + ["test_e2e_list_", "--", "--ignored"], test_run=True)
            if "review" in domains:
                command("review", [sys.executable, "scripts/e2e/probe_inline_comments.py", "--allow-disposable-writes",
                                   "--version-label", version, "--output", str(scratch / "reviews"),
                                   "--mcp-binary", str(binary), "--verify-posts"])
            if "merge" in domains:
                command("merge", [sys.executable, "scripts/e2e/probe_merges.py", "--allow-disposable-writes",
                                  "--version-label", version, "--output", str(scratch / "merges"), "--mcp-binary", str(binary)])
            report["passed"] = True
        except (Exception, KeyboardInterrupt):
            # Raw logs, exception text and upstream bodies remain private even
            # when a test, bootstrap, TLS request or cancellation fails.
            report["failed_stage"] = stage_name
            print(stage_name + ": failed (private diagnostics discarded)", flush=True)
        finally:
            try:
                command("cleanup", ["bash", "scripts/e2e/down.sh"], 120)
            except (Exception, KeyboardInterrupt):
                report["passed"] = False
                report["cleanup_failed"] = True
            output.mkdir(parents=True, exist_ok=True)
            (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    return report["passed"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow-disposable-writes", action="store_true", required=True)
    parser.add_argument("--version", choices=VERSIONS, required=True)
    parser.add_argument("--domains-json", default=json.dumps(DOMAINS))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runtime-root", type=Path, required=True)
    parser.add_argument("--target-dir", type=Path, default=Path(os.environ.get("CARGO_TARGET_DIR", REPO / "target")))
    parser.add_argument("--port", type=int, default=18080)
    parser.add_argument("--project", default="gitbucket-acceptance-" + secrets.token_hex(6))
    args = parser.parse_args()
    try:
        domains = json.loads(args.domains_json)
        if not isinstance(domains, list) or not domains or any(d not in DOMAINS for d in domains):
            raise ValueError()
        if len(set(domains)) != len(domains):
            raise ValueError()
        if not 1024 <= args.port <= 65535 or not re.fullmatch(r"[a-z0-9][a-z0-9_-]+", args.project):
            raise ValueError()
        if args.runtime_root.is_symlink():
            raise ValueError()
        root = args.runtime_root.resolve()
        # down.sh removes this directory. Only a new directory may be owned by
        # this runner, and never place the artifact/target/repo inside it.
        if root.exists() or root.is_symlink():
            raise ValueError()
        for protected in (args.output.resolve(), args.target_dir.resolve(), REPO):
            if root == protected or root in protected.parents or protected in root.parents:
                raise ValueError()
    except (ValueError, TypeError):
        parser.error("Invalid domains or disposable paths/project/port; choose a new runtime directory")
    os.umask(0o077)
    def interrupted(_signal, _frame):
        raise KeyboardInterrupt()
    signal.signal(signal.SIGTERM, interrupted)
    return 0 if run(args.version, domains, args.output.absolute(), root, args.target_dir.absolute(), args.port, args.project) else 1


if __name__ == "__main__":
    sys.exit(main())
