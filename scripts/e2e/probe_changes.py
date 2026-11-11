#!/usr/bin/env python3
"""P15 PR change reads through actual stdio against an opted-in disposable server."""
import argparse
from contextlib import ExitStack
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile
import time
from urllib.parse import quote

from mcp_review_client import McpReviewClient
from probe_inline_comments import Http, loopback_url


def page(client, tool, args):
    result = client.rpc("tools/call", {"name": tool, "arguments": args})
    assert len(json.dumps(result, ensure_ascii=False, separators=(",", ":")).encode()) <= 65536
    assert not result.get("isError"), "PR change read failed"
    return result["structuredContent"]


def collect(client, tool, args, first=None):
    field = {"list_pull_request_commits": "commits", "list_pull_request_files": "files", "get_pull_request_diff": "chunks"}[tool]
    data = first or page(client, tool, args)
    observed = data["range"]["observation_id"]
    comparison = data["comparison"]
    items = []
    while True:
        assert data["range"]["observation_id"] == observed and data["comparison"] == comparison
        items.extend(data[field])
        if not data["has_more"]:
            assert data["next_cursor"] is None
            return items, comparison
        data = page(client, tool, {**args, "cursor": data["next_cursor"]})


def run(binary):
    base = loopback_url(os.environ["GITBUCKET_E2E_URL"])
    user, password, token = (os.environ[k] for k in ("GITBUCKET_E2E_GIT_USERNAME", "GITBUCKET_E2E_GIT_PASSWORD", "GITBUCKET_E2E_TOKEN"))
    api = Http(base, token=token)
    assert api.json("GET", "/api/v3/user")["login"] == user
    repo = "changes-probe-" + secrets.token_hex(5)
    info = api.json("POST", "/api/v3/user/repos", {"name": repo, "private": True, "auto_init": True})
    target = quote(user, safe="") + "/" + quote(repo, safe="")
    rest = "/api/v3/repos/" + target
    web = Http(base)
    web.signin(user, password)
    with ExitStack() as resources:
        work = Path(resources.enter_context(tempfile.TemporaryDirectory(prefix="gitbucket-changes-")))
        askpass = work / "askpass.sh"
        askpass.write_text('#!/bin/sh\ncase "$1" in *Username*) printf "%s\\n" "$CHANGES_GIT_USER";; *) printf "%s\\n" "$CHANGES_GIT_PASSWORD";; esac\n')
        askpass.chmod(0o700)
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        env.update({"GIT_ASKPASS": str(askpass), "GIT_TERMINAL_PROMPT": "0", "CHANGES_GIT_USER": user, "CHANGES_GIT_PASSWORD": password})
        checkout = work / "repo"

        def git(*args, raw=False):
            done = subprocess.run(["git", "-c", "credential.helper=", *args], cwd=checkout if checkout.exists() else work,
                                  env=env, text=True, capture_output=True, timeout=90)
            if done.returncode:
                raise RuntimeError("disposable change probe Git command failed")
            return done.stdout if raw else done.stdout.rstrip("\n")

        git("clone", base + "/git/" + target + ".git", str(checkout))
        git("config", "user.name", "Changes author")
        git("config", "user.email", "changes@example.test")
        git("config", "commit.gpgsign", "false")
        git("config", "core.quotepath", "false")
        for name, body in {"review.txt": "alpha\nold\nomega\n", "gone.txt": "remove\n", "empty gone.txt": "", "old name.txt": "unchanged rename\n", "script.sh": "echo ok\n"}.items():
            (checkout / name).write_text(body)
        (checkout / "link").symlink_to("review.txt")
        (checkout / "image.bin").write_bytes(b"\x00old binary\xff")
        git("add", ".")
        git("commit", "-m", "Seed change fixtures")
        git("push", "origin", info["default_branch"])
        base_sha = git("rev-parse", "HEAD")
        git("checkout", "-b", "feature/changes")
        (checkout / "review.txt").write_text("alpha\nnew\nomega\n")
        (checkout / "gone.txt").unlink()
        (checkout / "empty gone.txt").unlink()
        (checkout / "empty new.txt").touch()
        (checkout / "link").unlink()
        (checkout / "link").symlink_to("new name.txt")
        (checkout / "old name.txt").rename(checkout / "new name.txt")
        (checkout / "script.sh").chmod(0o755)
        (checkout / "image.bin").write_bytes(b"\x00new binary\xfe")
        (checkout / "日本 & file.txt").write_text("日本語\n")
        (checkout / "tab\tfile.txt").write_text("tab path\n")
        (checkout / "large.txt").write_text("".join("日本語 line " + str(i) + "\n" for i in range(2500)))
        git("add", ".")
        (checkout / "submodule").mkdir()
        git("update-index", "--add", "--cacheinfo", "160000," + base_sha + ",submodule")
        git("commit", "-m", "Changes 日本語\n\n    code\nhard break  \nend")
        for i in range(26):
            (checkout / "tick.txt").write_text(str(i) + "\n")
            git("add", ".")
            git("commit", "-m", "Tick " + str(i))
        old_head = git("rev-parse", "HEAD")
        expected_old = git("rev-list", base_sha + ".." + old_head).splitlines()
        git("push", "origin", "feature/changes")
        pr = api.json("POST", rest + "/pulls", {"title": "P15 changes", "head": "feature/changes", "base": info["default_branch"]})
        number = pr["number"]
        pat = McpReviewClient(binary, base, token, "", "", web_auth=False)
        resources.callback(pat.close)
        args = {"owner": user, "repo": repo, "pull_number": number, "limit": 5, "view": "full"}
        first = page(pat, "list_pull_request_commits", args)
        assert first["has_more"] and first["comparison"]["head_sha"] == old_head
        (checkout / "tick.txt").write_text("new head\n")
        git("add", ".")
        git("commit", "-m", "Advance after cached observation")
        head = git("rev-parse", "HEAD")
        git("push", "origin", "feature/changes")
        for _ in range(100):
            if api.json("GET", rest + f"/pulls/{number}")["head"]["sha"] == head:
                break
            time.sleep(0.1)
        else:
            raise RuntimeError("PR metadata did not advance")
        cached, comparison = collect(pat, "list_pull_request_commits", args, first)
        assert [c["sha"] for c in cached] == expected_old and comparison["head_sha"] == old_head
        fresh, comparison = collect(pat, "list_pull_request_commits", args)
        assert len(fresh) == 28 and comparison["head_sha"] == head and not comparison["atomic"]
        assert [c["sha"] for c in fresh] == git("rev-list", base_sha + ".." + head).splitlines()
        for c in fresh:
            assert c["commit"]["message"] == git("cat-file", "commit", c["sha"], raw=True).partition("\n\n")[2]
            assert c["author"] is None and c["committer"] is None
        denied = pat.rpc("tools/call", {"name": "list_pull_request_files", "arguments": args})
        assert denied["isError"]
        client = McpReviewClient(binary, base, token, user, password)
        resources.callback(client.close)
        files, comparison = collect(client, "list_pull_request_files", {**args, "limit": 2})
        assert comparison["stored_base_sha"] == base_sha and comparison["head_sha"] == head
        counts = {}
        for row in git("diff", "--numstat", "--no-renames", "-z", base_sha, head, raw=True).split("\0"):
            if row:
                added, deleted, path = row.split("\t", 2)
                counts[path] = (None, None) if added == "-" else (int(added), int(deleted))
        renamed = sum(f["status"] == "renamed" for f in files)
        assert len(files) == len(counts) - renamed
        for f in files:
            if f["status"] == "renamed":
                assert git("show", base_sha + ":" + f["old_path"], raw=True) == git("show", head + ":" + f["new_path"], raw=True)
                assert (f["additions"], f["deletions"]) == (0, 0)
            else:
                assert (f["additions"], f["deletions"]) == counts[f["new_path"] or f["old_path"]]
        assert next(f for f in files if f["new_path"] == "image.bin")["binary"]
        assert next(f for f in files if f["old_path"] == "gone.txt")["new_path"] is None
        assert next(f for f in files if f["new_path"] == "script.sh")["new_mode"] == "100755"
        for path, side, expected_mode in [("empty gone.txt", "old", "100644"), ("empty new.txt", "new", "100644"),
                                          ("link", "new", "120000"), ("submodule", "new", "160000")]:
            observed_file = next(f for f in files if f[side + "_path"] == path)
            assert observed_file[side + "_mode"] == expected_mode
            if path.startswith("empty "):
                assert observed_file["hunks"] == 0 and observed_file["additions"] == observed_file["deletions"] == 0
        status, native = web.request("GET", "/" + target + "/patch/" + base_sha + "..." + head)
        assert status == 200
        diff_args = {"owner": user, "repo": repo, "pull_number": number, "limit": 1}
        chunks, observed = collect(client, "get_pull_request_diff", diff_args)
        assert len(chunks) > 10 and observed["head_sha"] == head
        assert "".join(c["text"] for c in chunks) == native
        offsets = {}
        for c in chunks:
            index = c["file_index"]
            assert c["start_byte"] == offsets.get(index, 0)
            assert len(c["text"].encode()) == c["end_byte"] - c["start_byte"] <= 4096
            offsets[index] = c["end_byte"]
        chosen, _ = collect(client, "get_pull_request_diff", {**diff_args, "path": "日本 & file.txt"})
        assert len({c["file_index"] for c in chosen}) == 1
        assert all(c["new_path"] == "日本 & file.txt" for c in chosen)
        from probe_comment_items import verify_comment_items
        verify_comment_items(binary, base, user, token, repo, number)
        merged = api.json("PUT", rest + f"/pulls/{number}/merge", {"merge_method": "merge-commit"})
        assert merged["merged"]
        after, observed = collect(client, "list_pull_request_files", {**args, "limit": 2})
        assert after == files and observed["stored_base_sha"] == base_sha and observed["head_sha"] == head
    print(json.dumps({"passed": True, "commits": 28, "files": len(files), "diff_chunks": len(chunks),
                      "cached_before_head_advance": True, "merged_pr_reads": True}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow-disposable-writes", action="store_true", required=True)
    parser.add_argument("--mcp-binary", type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    run(args.mcp_binary)


if __name__ == "__main__":
    main()
