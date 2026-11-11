"""Ordinary comment checks using the disposable changes probe's private PR fixture."""
from contextlib import ExitStack
import re
import secrets
from urllib.parse import quote

from mcp_review_client import McpReviewClient
from probe_inline_comments import Http


def verify_comment_items(binary, base, owner, token, repo, pull_number):
    api = Http(base, token=token)
    rest = "/api/v3/repos/" + quote(owner, safe="") + "/" + quote(repo, safe="")
    guest = "comment-reader-" + secrets.token_hex(5)
    secret = secrets.token_urlsafe(24)
    admin = Http(base, basic=("root", "root"))
    admin.json("POST", "/api/v3/admin/users", {"login": guest, "password": secret,
             "email": guest + "@example.test", "fullName": "Comment reader", "isAdmin": False})
    api.json("PUT", rest + "/collaborators/" + guest, {"permission": "pull"})
    permission = api.json("GET", rest + "/collaborators/" + guest + "/permission")
    assert permission["permission"] == "GUEST"
    guest_web = Http(base)
    guest_web.signin(guest, secret)
    status, _ = guest_web.request("POST", "/" + guest + "/_personalToken", form={"note": "disposable-comment-probe"})
    assert status in {302, 303}
    status, page = guest_web.request("GET", "/" + guest + "/_application")
    match = re.search(r'data-clipboard-text="([0-9a-f]{40})"', page)
    assert status == 200 and match
    issue = api.json("POST", rest + "/issues", {"title": "Ordinary comment contract"})
    common = {"owner": owner, "repo": repo}
    raw = "  日本語 **Markdown**  \n\n    code\n" + "content  \n" * 200

    with ExitStack() as resources:
        def client(pat, readonly=False):
            result = McpReviewClient(binary, base, pat, "", "", web_auth=False,
                                     read_only=readonly, profile="rest-only")
            resources.callback(result.close)
            return result
        writer, reader, guest_client = client(token), client(token, True), client(match.group(1))

        def error(client, tool, args, kind, status=None):
            result = client.rpc("tools/call", {"name": tool, "arguments": args})
            assert result["isError"] and result["structuredContent"]["kind"] == kind
            if status is not None:
                assert result["structuredContent"]["status"] == status

        ordinary = writer.call("add_issue_comment", {**common, "issue_number": issue["number"], "body": raw})
        pr_comment = writer.call("add_pull_request_comment", {**common, "pull_number": pull_number, "body": raw})
        guest_comment = guest_client.call("add_issue_comment", {**common, "issue_number": pull_number, "body": raw})
        for created, parent, author in [(ordinary, issue["number"], owner), (pr_comment, pull_number, owner), (guest_comment, pull_number, guest)]:
            args = {**common, "comment_id": created["id"]}
            got = reader.call("get_issue_comment", args)
            assert got["body"] == raw and got["user"]["login"] == author
            assert got["created_at"] and got["updated_at"]
            parent_kind = "issues" if parent == issue["number"] else "pull"
            assert got["html_url"].endswith("/" + parent_kind + "/" + str(parent) + "#comment-" + str(got["id"]))
            listing = writer.call("list_issue_comments", {**common, "issue_number": parent, "view": "full"})
            assert next(c for c in listing["comments"] if c["id"] == got["id"])["body"] == raw
        foreign_args = {**common, "comment_id": ordinary["id"]}
        for tool in ["update_issue_comment", "delete_issue_comment"]:
            args = {**foreign_args, **({"body": "forbidden replacement"} if tool.startswith("update") else {})}
            error(guest_client, tool, args, "operation_not_applied", 401)
            error(reader, tool, args, "read_only_error")
        assert reader.call("get_issue_comment", foreign_args)["body"] == raw
        for client_, target, author in [(writer, ordinary, owner), (writer, guest_comment, guest), (guest_client, guest_comment, guest)]:
            text = raw + "edited by " + (owner if client_ is writer else guest) + "  \n"
            got = client_.call("update_issue_comment", {**common, "comment_id": target["id"], "body": text})
            assert got["id"] == target["id"] and got["body"] == text and got["user"]["login"] == author
        for tool in ["get_issue_comment", "update_issue_comment", "delete_issue_comment"]:
            error(writer, tool, {**common, "comment_id": 0, **({"body": raw} if tool.startswith("update") else {})}, "validation_error")
        error(writer, "update_issue_comment", {**foreign_args, "body": " \n\t"}, "validation_error")
        other = api.json("POST", "/api/v3/user/repos", {"name": "comment-scope-" + secrets.token_hex(5), "private": True})
        error(reader, "get_issue_comment", {**foreign_args, "repo": other["name"]}, "api_error", 404)
        for client_, target in [(writer, ordinary), (writer, pr_comment), (guest_client, guest_comment)]:
            args = {**common, "comment_id": target["id"]}
            assert client_.call("delete_issue_comment", args) == {"deleted": True, "comment_id": target["id"]}
            error(reader, "get_issue_comment", args, "api_error", 404)
            error(client_, "delete_issue_comment", args, "api_error", 404)
