"""One-way PR readiness through actual stdio on disposable native GitBucket."""
from contextlib import ExitStack
import re
import secrets
import time
from urllib.parse import quote

from mcp_review_client import McpReviewClient
from probe_inline_comments import Http


def verify_pr_ready(binary, base, owner, password, token, repo, base_branch, git):
    api = Http(base, token=token)
    rest = "/api/v3/repos/" + quote(owner, safe="") + "/" + quote(repo, safe="")
    common = {"owner": owner, "repo": repo}
    guest, secret = "ready-head-" + secrets.token_hex(5), secrets.token_urlsafe(24)
    Http(base, basic=("root", "root")).json("POST", "/api/v3/admin/users", {
        "login": guest, "password": secret, "email": guest + "@example.test",
        "fullName": "Ready fork author", "isAdmin": False})
    api.json("PUT", rest + "/collaborators/" + guest, {"permission": "pull"})
    assert api.json("GET", rest + "/collaborators/" + guest + "/permission")["permission"] == "GUEST"
    web = Http(base)
    web.signin(guest, secret)
    status, _ = web.request("POST", "/" + guest + "/_personalToken", form={"note": "disposable-ready-probe"})
    assert status in {302, 303}
    status, page = web.request("GET", "/" + guest + "/_application")
    match = re.search(r'data-clipboard-text="([0-9a-f]{40})"', page)
    assert status == 200 and match
    guest_token = match.group(1)
    guest_api = Http(base, token=guest_token)
    raw = "  日本語 **Markdown**  \n\n    code\nlast  \n"

    with ExitStack() as resources:
        def connect(pat=token, username=owner, pwd=password, **options):
            client = McpReviewClient(binary, base, pat, username, pwd,
                                    read_only=options.pop("read_only", False), **options)
            resources.callback(client.close)
            return client

        writer = connect()
        reader = connect(read_only=True, web_auth=False)
        guest_writer = connect(guest_token, guest, secret)
        pat_only = connect(web_auth=False)
        rest_only = connect(profile="rest-only")
        guest_pat_only = connect(guest_token, guest, secret, web_auth=False)
        tools = {t["name"]: t for t in writer.rpc("tools/list", {})["tools"]}
        schema = tools["mark_pull_request_ready"]["inputSchema"]
        assert set(schema["required"]) == {"owner", "repo", "pull_number"}
        assert set(schema["properties"]) == {"owner", "repo", "pull_number"}
        assert tools["mark_pull_request_ready"]["annotations"]["idempotentHint"]
        assert "draft" not in tools["update_pull_request"]["inputSchema"]["properties"]
        tip_before = api.json("GET", rest + "/branches/" + quote(base_branch, safe=""))["commit"]["sha"]

        def create(branch, creator=writer, head=None):
            git("push", "origin", "HEAD:refs/heads/" + branch)
            return creator.call("create_pull_request", {
                **common, "title": "Native ready transition", "body": raw,
                "head": head or branch, "base": base_branch, "draft": True})

        branch = "ready-main-" + secrets.token_hex(5)
        created = create(branch)
        number = created["number"]
        args = {**common, "pull_number": number}
        issue_args = {**common, "issue_number": number}
        writer.call("add_issue_labels", {**issue_args, "labels": [" ready 日本語 & label "]})
        writer.call("add_issue_assignees", {**issue_args, "assignees": [owner, guest]})
        milestone = writer.call("create_milestone", {**common, "title": "ready-preserved-milestone"})
        writer.call("set_issue_milestone", {**issue_args, "milestone_number": milestone["number"]})
        before = reader.call("get_pull_request", args)
        issue_before = reader.call("get_issue", issue_args)
        assert before["draft"] is True and before["merged"] is False
        for client, kind in [(guest_writer, "operation_not_applied"), (pat_only, "authentication_error"),
                             (rest_only, "unsupported_error"), (reader, "read_only_error")]:
            result = client.rpc("tools/call", {"name": "mark_pull_request_ready", "arguments": args})
            assert result["isError"] and result["structuredContent"]["kind"] == kind
            assert reader.call("get_pull_request", args)["draft"] is True
        result = writer.rpc("tools/call", {"name": "merge_pull_request", "arguments": args})
        assert result["isError"] and result["structuredContent"]["kind"] == "unsupported_error"
        observed = writer.call("mark_pull_request_ready", args)
        assert observed["draft"] is False and observed["merged"] is False and observed["state"] == "open"
        for field in ("title", "body", "state", "merged", "head", "base", "labels", "assignees"):
            assert observed[field] == before[field]
        assert observed["body"] == raw
        issue_after = reader.call("get_issue", issue_args)
        for field in ("title", "body", "state", "labels", "assignees", "milestone"):
            assert issue_after[field] == issue_before[field]
        assert pat_only.call("mark_pull_request_ready", args)["draft"] is False
        assert guest_pat_only.call("mark_pull_request_ready", args)["draft"] is False
        assert reader.call("get_pull_request", args)["draft"] is False
        # Native PATCH ignores draft. Do not expose a reverse flag or use merge
        # as a substitute for an unsupported ready -> draft transition.
        native = api.json("PATCH", rest + f"/pulls/{number}", {"draft": True})
        assert native["draft"] is False and native["body"] == raw
        writer.call("update_pull_request", {**args, "state": "closed"})
        result = writer.rpc("tools/call", {"name": "mark_pull_request_ready", "arguments": args})
        assert result["isError"] and result["structuredContent"]["kind"] == "unsupported_error"
        assert reader.call("get_pull_request", args)["state"] == "closed"

        closed = create("ready-closed-" + secrets.token_hex(5))
        closed_args = {**common, "pull_number": closed["number"]}
        writer.call("update_pull_request", {**closed_args, "state": "closed"})
        result = writer.rpc("tools/call", {"name": "mark_pull_request_ready", "arguments": closed_args})
        assert result["isError"] and result["structuredContent"]["kind"] == "unsupported_error"
        assert reader.call("get_pull_request", closed_args)["draft"] is True

        # Private forks grant the source owner ADMIN by default. First verify
        # that inherited grant, then remove it to isolate head permission.
        fork_branch = "ready-fork-" + secrets.token_hex(5)
        denied_branch = fork_branch + "-no-head-role"
        git("push", "origin", "HEAD:refs/heads/" + fork_branch)
        git("push", "origin", "HEAD:refs/heads/" + denied_branch)
        # Fixture setup uses the common native form: 4.44.0 has no REST fork
        # creation endpoint. This does not add a product fork fallback.
        status, _ = web.request("POST", "/" + quote(owner, safe="") + "/" + quote(repo, safe="") + "/fork",
                                form={"account": guest})
        assert status in {302, 303}
        fork_rest = "/api/v3/repos/" + quote(guest, safe="") + "/" + quote(repo, safe="")
        for _ in range(120):
            status, _ = guest_api.request("GET", fork_rest + "/branches/" + fork_branch)
            if status == 200:
                break
            time.sleep(0.1)
        else:
            raise RuntimeError("Disposable fork branch did not become readable")
        assert guest_api.json("GET", fork_rest)["full_name"] == guest + "/" + repo
        fork_pr = guest_writer.call("create_pull_request", {
            **common, "title": "Fork ready permission", "body": raw,
            "head": guest + ":" + fork_branch, "base": base_branch, "draft": True})
        fork_args = {**common, "pull_number": fork_pr["number"]}
        permission = guest_api.json("GET", fork_rest + "/collaborators/" + quote(owner, safe="") + "/permission")
        assert permission["permission"] == "ADMIN"
        assert writer.call("mark_pull_request_ready", fork_args)["draft"] is False
        status, _ = guest_api.request("DELETE", fork_rest + "/collaborators/" + quote(owner, safe=""))
        assert status == 204
        status, _ = guest_api.request("GET", fork_rest + "/collaborators/" + quote(owner, safe="") + "/permission")
        assert status == 404
        fork_pr = guest_writer.call("create_pull_request", {
            **common, "title": "Fork head permission after grant removal", "body": raw,
            "head": guest + ":" + denied_branch, "base": base_branch, "draft": True})
        fork_args = {**common, "pull_number": fork_pr["number"]}
        fork_before = reader.call("get_pull_request", fork_args)
        assert fork_before["head"]["repo"]["full_name"] == guest + "/" + repo
        result = writer.rpc("tools/call", {"name": "mark_pull_request_ready", "arguments": fork_args})
        assert result["isError"] and result["structuredContent"]["kind"] == "operation_not_applied"
        assert result["structuredContent"]["cause"]["status"] == 404
        assert reader.call("get_pull_request", fork_args)["draft"] is True
        ready = guest_writer.call("mark_pull_request_ready", fork_args)
        assert ready["draft"] is False and ready["merged"] is False and ready["body"] == raw
        for field in ("title", "body", "state", "head", "base", "labels", "assignees"):
            assert ready[field] == fork_before[field]
        assert pat_only.call("mark_pull_request_ready", fork_args)["draft"] is False
        assert api.json("GET", rest + "/branches/" + quote(base_branch, safe=""))["commit"]["sha"] == tip_before
        # Native PR GET acknowledges a missing Option with HTTP 200 and no
        # resource body. Snapshot preparation must fail before a ready POST.
        status, missing_body = api.request("GET", rest + "/pulls/2147483647")
        assert status == 200 and not missing_body
        result = writer.rpc("tools/call", {"name": "mark_pull_request_ready", "arguments": {**common, "pull_number": 2147483647}})
        assert result["isError"] and result["structuredContent"]["kind"] == "other_error"


def verify_merged_ready_rejection(binary, base, owner, token, repo, number):
    client = McpReviewClient(binary, base, token, "", "", web_auth=False, read_only=False)
    try:
        args = {"owner": owner, "repo": repo, "pull_number": number}
        before = client.call("get_pull_request", args)
        assert before["merged"] is True and before["state"] == "closed"
        result = client.rpc("tools/call", {"name": "mark_pull_request_ready", "arguments": args})
        assert result["isError"] and result["structuredContent"]["kind"] == "unsupported_error"
        assert client.call("get_pull_request", args) == before
    finally:
        client.close()
