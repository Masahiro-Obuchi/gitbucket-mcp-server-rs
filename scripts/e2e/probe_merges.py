#!/usr/bin/env python3
"""P12 merge contract checks on an explicitly disposable loopback GitBucket."""
import argparse
from contextlib import ExitStack
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import tempfile
from urllib.parse import quote, urlsplit

from mcp_review_client import McpReviewClient
from probe_inline_comments import Http, Parser, loopback_url


def options_form(page, target):
    forms = [n for n in Parser(page).root.nodes()
             if n.tag == "form" and urlsplit(n.attrs.get("action", "")).path == target]
    if len(forms) != 1:
        raise ValueError("expected one repository options form")
    fields = []
    for node in forms[0].nodes():
        name = node.attrs.get("name")
        if not name or "disabled" in node.attrs:
            continue
        if node.tag == "input":
            kind = node.attrs.get("type", "text")
            if kind in {"submit", "button", "file"}:
                continue
            if kind in {"checkbox", "radio"} and "checked" not in node.attrs:
                continue
            fields.append((name, node.attrs.get("value", "on" if kind == "checkbox" else "")))
        elif node.tag == "textarea":
            fields.append((name, node.text()))
        elif node.tag == "select":
            choices = [n for n in node.nodes() if n.tag == "option"]
            selected = [n for n in choices if "selected" in n.attrs] or choices[:1]
            fields.extend((name, n.attrs.get("value", n.text())) for n in selected)
    if not {"mergeOptions", "defaultMergeOption"}.issubset({k for k, _ in fields}):
        raise ValueError("repository options form omitted merge policy")
    return fields


def run(binary, output, version):
    base = loopback_url(os.environ["GITBUCKET_E2E_URL"])
    user = os.environ["GITBUCKET_E2E_GIT_USERNAME"]
    password = os.environ["GITBUCKET_E2E_GIT_PASSWORD"]
    token = os.environ["GITBUCKET_E2E_TOKEN"]
    api = Http(base, token=token)
    assert api.json("GET", "/api/v3/user")["login"] == user
    repo = "merge-probe-" + secrets.token_hex(5)
    repository = api.json("POST", "/api/v3/user/repos", {"name": repo, "private": True, "auto_init": True})
    owner_repo = quote(user, safe="") + "/" + quote(repo, safe="")
    rest, web = "/api/v3/repos/" + owner_repo, "/" + owner_repo
    owner_web = Http(base)
    owner_web.signin(user, password)
    result = {"version": version, "private_repository": True, "strategies": [], "rejections": []}
    output.mkdir(parents=True, exist_ok=False)

    def policy(methods):
        target = web + "/settings/options"
        status, html = owner_web.request("GET", target)
        assert status == 200
        fields = [(k, v) for k, v in options_form(html, target) if k not in {"mergeOptions", "defaultMergeOption"}]
        fields.extend(("mergeOptions", m) for m in methods)
        fields.append(("defaultMergeOption", methods[0]))
        status, _ = owner_web.request("POST", target, form=fields)
        assert status in {302, 303}
        status, html = owner_web.request("GET", target)
        assert status == 200
        assert {v for k, v in options_form(html, target) if k == "mergeOptions"} == set(methods)

    with ExitStack() as resources:
        work = Path(resources.enter_context(tempfile.TemporaryDirectory(prefix="gitbucket-merge-")))
        askpass = work / "askpass.sh"
        askpass.write_text('#!/bin/sh\ncase "$1" in *Username*) printf "%s\\n" "$MERGE_GIT_USER";; *) printf "%s\\n" "$MERGE_GIT_PASSWORD";; esac\n')
        askpass.chmod(0o700)
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        env.update({"GIT_ASKPASS": str(askpass), "GIT_TERMINAL_PROMPT": "0", "MERGE_GIT_USER": user, "MERGE_GIT_PASSWORD": password})
        checkout = work / "repo"

        def git(*args):
            done = subprocess.run(["git", "-c", "credential.helper=", *args], cwd=checkout if checkout.exists() else work,
                                  env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=90)
            if done.returncode:
                raise RuntimeError("disposable Git command failed: " + args[0])
            return done.stdout.rstrip("\n")

        git("clone", base + "/git/" + owner_repo + ".git", str(checkout))
        git("config", "user.name", "Merge probe")
        git("config", "user.email", "merge@example.test")
        default = repository["default_branch"]
        policy(["merge-commit", "squash", "rebase"])
        client = McpReviewClient(binary, base, token, user, password, read_only=False)
        resources.callback(client.close)

        def case(label, draft=False):
            target, source = "base/" + label, "head/" + label
            git("checkout", "-B", target, "origin/" + default)
            base_sha = git("rev-parse", "HEAD")
            git("checkout", "-b", source)
            for index in [1, 2]:
                (checkout / f"{label}-{index}.txt").write_text(f"change {index}\n")
                git("add", ".")
                git("commit", "-m", f"{label} commit {index}")
            head = git("rev-parse", "HEAD")
            git("push", "origin", target, source)
            pr = api.json("POST", rest + "/pulls", {"title": "P12 " + label, "head": source, "base": target, "draft": draft})
            return pr["number"], target, base_sha, head

        number, target, old, head = case("native-sha")
        raw = api.json("PUT", rest + f"/pulls/{number}/merge", {"sha": "0" * 40, "merge_method": "merge-commit"})
        assert raw["merged"] is True
        git("fetch", "origin")
        native_tip = git("rev-parse", "origin/" + target)
        assert native_tip in raw["sha"]
        result["native_response_sha"] = raw["sha"]
        result["native_wrong_sha_was_ignored"] = True
        (output / "observations.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")

        for method in ["merge", "squash", "rebase"]:
            number, target, old, head = case(method)
            args = {"owner": user, "repo": repo, "pull_number": number, "merge_method": method}
            if method != "squash":
                args.update({"head_guard": "preflight", "expected_head_sha": head})
            message = "P12 日本語\n\n    indented code\nhard break  \nend"
            if method != "rebase":
                args["commit_message"] = message
            merged = client.call("merge_pull_request", args)
            assert merged["merged"] is True and merged["requested_merge_method"] == method
            assert re.fullmatch("[0-9a-f]{40}", merged["sha"])
            verification = merged["verification"]
            assert verification["atomic_head_guard"] is False
            assert verification["mergeable_before"] is None
            assert verification["pr_state_verified"] and verification["base_ref_verified"] and verification["commit_parents_verified"]
            assert verification["merged_head_verified"] == (method == "merge")
            assert not verification["commit_message_verified"] and not verification["repository_policy_preflight_verified"]
            git("fetch", "origin")
            tip = git("rev-parse", "origin/" + target)
            assert tip == merged["sha"]
            parents = git("show", "-s", "--format=%P", tip).split()
            assert len(parents) == (2 if method == "merge" else 1)
            if method == "merge":
                assert parents == [old, head]
            if method == "squash":
                assert parents == [old]
            assert int(git("rev-list", "--count", old + ".." + tip)) == {"merge": 3, "squash": 1, "rebase": 2}[method]
            if method != "rebase":
                assert git("show", "-s", "--format=%B", tip).endswith(message)
            pr = api.json("GET", rest + f"/pulls/{number}")
            assert pr["merged"] is True and pr["state"] == "closed"
            result["strategies"].append({"method": method, "sha": tip, "parents": parents, "passed": True})
            (output / "observations.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")

        number, target, old, head = case("rejected")
        normal = {"owner": user, "repo": repo, "pull_number": number}
        for patch in [{"head_guard": "atomic", "expected_head_sha": head},
                      {"head_guard": "preflight", "expected_head_sha": old},
                      {"head_guard": "preflight"}, {"expected_head_sha": head},
                      {"merge_method": "rebase", "commit_message": "must not be ignored"}]:
            reply = client.rpc("tools/call", {"name": "merge_pull_request", "arguments": {**normal, **patch}})
            assert reply["isError"] is True
            result["rejections"].append(reply["structuredContent"]["kind"])
        policy(["merge-commit"])
        denied = client.rpc("tools/call", {"name": "merge_pull_request", "arguments": {**normal, "merge_method": "squash"}})
        assert denied["structuredContent"]["kind"] == "operation_not_applied"
        assert denied["structuredContent"]["cause"]["status"] == 405
        result["repository_policy_rejection"] = True
        git("fetch", "origin")
        assert git("rev-parse", "origin/" + target) == old
        assert api.json("GET", rest + f"/pulls/{number}")["merged"] is False

        number, _, _, _ = case("draft", draft=True)
        draft = client.rpc("tools/call", {"name": "merge_pull_request", "arguments": {**normal, "pull_number": number}})
        assert draft["structuredContent"]["kind"] == "unsupported_error"
        result["draft_rejection"] = True
        readonly = McpReviewClient(binary, base, token, user, password)
        resources.callback(readonly.close)
        blocked = readonly.rpc("tools/call", {"name": "merge_pull_request", "arguments": normal})
        assert blocked["structuredContent"]["kind"] == "read_only_error"
        result["readonly_rejection"] = True
    (output / "observations.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"version": version, "strategies": 3, "rejections": 8, "passed": True}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow-disposable-writes", action="store_true", required=True)
    parser.add_argument("--version-label", required=True)
    parser.add_argument("--mcp-binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    run(args.mcp_binary, args.output, args.version_label)


if __name__ == "__main__":
    main()
