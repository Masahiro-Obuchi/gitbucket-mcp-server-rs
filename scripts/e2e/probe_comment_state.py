"""Comment plus close/reopen via actual stdio on disposable native GitBucket."""
from contextlib import ExitStack
import re
import secrets
from urllib.parse import quote

from mcp_review_client import McpReviewClient
from probe_inline_comments import Http


def verify_comment_state(binary, base, owner, token, repo, pull_number, base_branch, git):
    api = Http(base, token=token)
    common = {"owner": owner, "repo": repo}
    rest = "/api/v3/repos/" + quote(owner, safe="") + "/" + quote(repo, safe="")
    guest, secret = "state-author-" + secrets.token_hex(5), secrets.token_urlsafe(24)
    Http(base, basic=("root", "root")).json("POST", "/api/v3/admin/users", {
        "login": guest, "password": secret, "email": guest + "@example.test",
        "fullName": "Comment state author", "isAdmin": False})
    api.json("PUT", rest + "/collaborators/" + guest, {"permission": "pull"})
    assert api.json("GET", rest + "/collaborators/" + guest + "/permission")["permission"] == "GUEST"
    web = Http(base)
    web.signin(guest, secret)
    status, _ = web.request("POST", "/" + guest + "/_personalToken", form={"note": "disposable-state-probe"})
    assert status in {302, 303}
    status, page = web.request("GET", "/" + guest + "/_application")
    match = re.search(r'data-clipboard-text="([0-9a-f]{40})"', page)
    assert status == 200 and match
    raw = "  日本語 **Markdown**  \n\n    code\nlast  \n"

    with ExitStack() as resources:
        def connect(pat=token, profile="rest-only", readonly=False):
            client = McpReviewClient(binary, base, pat, "", "", web_auth=False,
                                     read_only=readonly, profile=profile)
            resources.callback(client.close)
            return client

        writer, standard, guest_writer, reader = connect(), connect(profile="standard"), connect(match.group(1)), connect(readonly=True)
        tools = {t["name"]: t for t in writer.rpc("tools/list", {})["tools"]}
        for name in ["close_issue_with_comment", "reopen_issue_with_comment"]:
            schema = tools[name]["inputSchema"]
            assert set(schema["required"]) == {"owner", "repo", "issue_number", "body"}
            assert set(schema["properties"]) == set(schema["required"])
            assert tools[name]["annotations"]["destructiveHint"] is True
            assert tools[name]["annotations"]["idempotentHint"] is False
        tip = api.json("GET", rest + "/branches/" + quote(base_branch, safe=""))["commit"]["sha"]

        def comments(number):
            return api.json("GET", rest + f"/issues/{number}/comments")

        def refused(client, name, number, kind, **extra):
            previous = comments(number)
            result = client.rpc("tools/call", {"name": name, "arguments": {**common, "issue_number": number, "body": raw, **extra}})
            assert result["isError"] and result["structuredContent"]["kind"] == kind
            assert comments(number) == previous

        def transition(client, close, number, author):
            before = reader.call("get_issue", {**common, "issue_number": number})
            prior = comments(number)
            is_pr = before["pull_request"] is not None
            pr_before = reader.call("get_pull_request", {**common, "pull_number": number}) if is_pr else None
            result = client.call("close_issue_with_comment" if close else "reopen_issue_with_comment", {
                **common, "issue_number": number, "body": raw})
            comment, issue = result["comment"], result["issue"]
            assert comment["body"] == raw and comment["user"]["login"] == author
            assert issue["state"] == ("closed" if close else "open") and issue["number"] == number
            for field in ("title", "body", "labels", "assignees", "pull_request"):
                assert issue[field] == before[field]
            if before["milestone"]:
                assert issue["milestone"]["number"] == before["milestone"]["number"]
            assert reader.call("get_issue_comment", {**common, "comment_id": comment["id"]}) == comment
            after = comments(number)
            assert len(after) == len(prior) + 1
            assert {c["id"] for c in after} - {c["id"] for c in prior} == {comment["id"]}
            assert next(c for c in after if c["id"] == comment["id"])["body"] == raw
            assert reader.call("get_issue", {**common, "issue_number": number})["state"] == issue["state"]
            if is_pr:
                observed = reader.call("get_pull_request", {**common, "pull_number": number})
                assert result["pull_request"] == observed and observed["state"] == issue["state"]
                for field in ("draft", "merged", "head", "base"):
                    assert observed[field] == pr_before[field]
                assert observed["merged"] is False
            else:
                assert result["pull_request"] is None
            return comment

        milestone = api.json("POST", rest + "/milestones", {"title": "state-preserved"})
        api.json("POST", rest + "/labels", {"name": "state label", "color": "abcdef"})
        # The new operation and metadata creation use PAT-only REST clients.
        issue = writer.call("create_issue", {**common, "title": "Comment and state metadata", "body": raw,
                                               "labels": ["state label"], "assignees": [owner, guest], "milestone": milestone["number"]})
        number = issue["number"]
        for close in [True, False]:
            if not close:
                transition(writer, True, number, owner)
            before = comments(number)
            ignored = guest_writer.rpc("tools/call", {"name": "close_issue_with_comment" if close else "reopen_issue_with_comment",
                                                       "arguments": {**common, "issue_number": number, "body": raw}})
            data = ignored["structuredContent"]
            assert ignored["isError"] and data["kind"] == "operation_partial"
            assert data["operation"]["target"]["parent_number"] == number
            created_id = data["operation"]["target"]["number"]
            assert len(comments(number)) == len(before) + 1
            assert reader.call("get_issue_comment", {**common, "comment_id": created_id})["user"]["login"] == guest
            assert reader.call("get_issue", {**common, "issue_number": number})["state"] == ("open" if close else "closed")
        transition(standard, False, number, owner)
        for client in [writer, standard]:
            transition(client, True, number, owner)
            refused(client, "close_issue_with_comment", number, "unsupported_error")
            transition(client, False, number, owner)
            refused(client, "reopen_issue_with_comment", number, "unsupported_error")
        for name in ["close_issue_with_comment", "reopen_issue_with_comment"]:
            refused(reader, name, number, "read_only_error")
            refused(writer, name, number, "validation_error", body=" \n\t")
        own = guest_writer.call("create_issue", {**common, "title": "Guest opener", "body": raw})
        transition(guest_writer, True, own["number"], guest)
        transition(guest_writer, False, own["number"], guest)
        transition(writer, True, pull_number, owner)
        transition(standard, False, pull_number, owner)
        branch = "state-guest-" + secrets.token_hex(5)
        git("push", "origin", "HEAD:refs/heads/" + branch)
        own_pr = guest_writer.call("create_pull_request", {**common, "title": "Guest draft opener", "body": raw,
                                                           "head": branch, "base": base_branch, "draft": True})
        transition(guest_writer, True, own_pr["number"], guest)
        transition(guest_writer, False, own_pr["number"], guest)
        assert api.json("GET", rest + "/branches/" + quote(base_branch, safe=""))["commit"]["sha"] == tip


def verify_merged_comment_state_rejection(binary, base, owner, token, repo, number):
    client = McpReviewClient(binary, base, token, "", "", web_auth=False, read_only=False, profile="rest-only")
    api = Http(base, token=token)
    path = "/api/v3/repos/" + quote(owner, safe="") + "/" + quote(repo, safe="") + f"/issues/{number}/comments"
    try:
        before = api.json("GET", path)
        for name in ["close_issue_with_comment", "reopen_issue_with_comment"]:
            result = client.rpc("tools/call", {"name": name, "arguments": {"owner": owner, "repo": repo,
                                "issue_number": number, "body": "Must not reopen merged PR"}})
            assert result["isError"] and result["structuredContent"]["kind"] == "unsupported_error"
        assert api.json("GET", path) == before
    finally:
        client.close()
