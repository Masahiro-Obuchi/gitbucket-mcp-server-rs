"""Creation metadata through PAT-only stdio in the disposable changes fixture."""
from contextlib import ExitStack
import re
import secrets
from urllib.parse import quote

from mcp_review_client import McpReviewClient
from probe_inline_comments import Http


def verify_creation_metadata(binary, base, owner, token, repo, base_branch, git):
    api = Http(base, token=token)
    rest = "/api/v3/repos/" + quote(owner, safe="") + "/" + quote(repo, safe="")
    guest, secret = "creation-reader-" + secrets.token_hex(5), secrets.token_urlsafe(24)
    Http(base, basic=("root", "root")).json("POST", "/api/v3/admin/users", {
        "login": guest, "password": secret, "email": guest + "@example.test", "fullName": "Creation reader", "isAdmin": False})
    api.json("PUT", rest + "/collaborators/" + guest, {"permission": "pull"})
    assert api.json("GET", rest + "/collaborators/" + guest + "/permission")["permission"] == "GUEST"
    web = Http(base)
    web.signin(guest, secret)
    status, _ = web.request("POST", "/" + guest + "/_personalToken", form={"note": "disposable-creation-probe"})
    assert status in {302, 303}
    status, page = web.request("GET", "/" + guest + "/_application")
    match = re.search(r'data-clipboard-text="([0-9a-f]{40})"', page)
    assert status == 200 and match
    common = {"owner": owner, "repo": repo}
    raw = "  日本語 **Markdown**  \n\n    code\nlast  \n"
    exact_label = " creation 日本語 & label "

    def issue_ids():
        found = set()
        for number in range(1, 100):
            data = api.json("GET", rest + f"/issues?state=all&page={number}&per_page=25")
            if not data:
                return found
            assert not (found & {item["number"] for item in data})
            found.update(item["number"] for item in data)
        raise RuntimeError("Disposable creation issue enumeration did not terminate")

    with ExitStack() as resources:
        def connect(pat, readonly=False):
            client = McpReviewClient(binary, base, pat, "", "", web_auth=False, read_only=readonly, profile="rest-only")
            resources.callback(client.close)
            return client
        writer, reader, guest_client = connect(token), connect(token, True), connect(match.group(1))
        tools = {item["name"]: item["inputSchema"] for item in writer.rpc("tools/list", {})["tools"]}
        assert "milestone" in tools["create_issue"]["properties"] and "milestone" not in tools["create_issue"]["required"]
        assert "draft" in tools["create_pull_request"]["properties"] and "draft" not in tools["create_pull_request"]["required"]
        assert not ({"assignees", "milestone", "maintainer_can_modify"} & tools["create_pull_request"]["properties"].keys())
        assert "draft" not in tools["update_pull_request"]["properties"]
        assert not ({"assignees", "milestone"} & tools["update_issue"]["properties"].keys())
        seed = writer.call("create_issue", {**common, "title": "Creation label seed"})
        writer.call("add_issue_labels", {**common, "issue_number": seed["number"], "labels": [exact_label]})
        milestones = []
        for closed in (False, True):
            milestone = writer.call("create_milestone", {**common, "title": "creation-" + ("closed" if closed else "open")})
            if closed:
                milestone = writer.call("update_milestone", {**common, "milestone_number": milestone["number"], "state": "closed"})
            milestones.append(milestone["number"])
            created = writer.call("create_issue", {**common, "title": "Creation metadata", "body": raw,
                                  "labels": [exact_label], "assignees": [" " + owner + " ", guest], "milestone": milestone["number"]})
            for stored in (created, reader.call("get_issue", {**common, "issue_number": created["number"]})):
                assert stored["body"] == raw and stored["milestone"]["number"] == milestone["number"]
                assert stored["milestone"]["state"] == ("closed" if closed else "open")
                assert {item["login"] for item in stored["assignees"]} == {owner, guest}
                assert [item["name"] for item in stored["labels"]] == [exact_label]
        foreign_repo = api.json("POST", "/api/v3/user/repos", {"name": "creation-scope-" + secrets.token_hex(5), "private": True, "auto_init": True})
        foreign = api.json("POST", "/api/v3/repos/" + quote(owner, safe="") + "/" + quote(foreign_repo["name"], safe="") + "/milestones", {"title": "foreign"})
        before = issue_ids()
        for params, kind in [({"milestone": 2147483647}, "api_error"), ({"milestone": foreign["number"]}, "api_error"),
                             ({"labels": ["missing-label-" + secrets.token_hex(5)]}, "api_error"),
                             ({"assignees": ["missing-account-" + secrets.token_hex(5)]}, "api_error"),
                             ({"milestone": 0}, "validation_error"), ({"assignees": [owner, " " + owner]}, "validation_error")]:
            result = writer.rpc("tools/call", {"name": "create_issue", "arguments": {**common, "title": "Must not be created", **params}})
            assert result["isError"] and result["structuredContent"]["kind"] == kind
        assert issue_ids() == before
        # GitBucket lets GUEST create an issue but silently ignores nonempty
        # assignment metadata. The MCP error must locate that existing issue.
        result = guest_client.rpc("tools/call", {"name": "create_issue", "arguments": {
            **common, "title": "Ignored assignment", "body": raw, "assignees": [owner]}})
        assert result["isError"] and result["structuredContent"]["kind"] == "operation_not_verified"
        number = result["structuredContent"]["operation"]["target"]["number"]
        assert issue_ids() - before == {number}
        stored = reader.call("get_issue", {**common, "issue_number": number})
        assert stored["assignees"] == [] and stored["body"] == raw and stored["user"]["login"] == guest
        empty = guest_client.call("create_issue", {**common, "title": "Explicit empty assignments", "labels": [], "assignees": []})
        assert empty["labels"] == empty["assignees"] == []
        expected = {}
        for flag in (True, False, None):
            branch = "creation-meta/" + secrets.token_hex(5)
            git("push", "origin", "HEAD:refs/heads/" + branch)
            args = {**common, "title": "Creation draft contract", "head": branch, "base": base_branch, "body": raw}
            if flag is not None:
                args["draft"] = flag
            created = writer.call("create_pull_request", args)
            expected[created["number"]] = flag if flag is not None else False
            assert created["draft"] is expected[created["number"]] and created["body"] == raw
            stored = reader.call("get_pull_request", {**common, "pull_number": created["number"]})
            assert stored["draft"] is expected[created["number"]] and stored["body"] == raw
            if flag:
                blocked = writer.rpc("tools/call", {"name": "merge_pull_request", "arguments": {**common, "pull_number": created["number"]}})
                assert blocked["isError"] and blocked["structuredContent"]["kind"] == "unsupported_error"
                assert reader.call("get_pull_request", {**common, "pull_number": created["number"]})["state"] == "open"
            blocked = reader.rpc("tools/call", {"name": "create_pull_request", "arguments": args})
            assert blocked["isError"] and blocked["structuredContent"]["kind"] == "read_only_error"
        args = {**common, "state": "all", "limit": 2}
        listed = []
        while True:
            page = reader.call("list_pull_requests", args)
            listed.extend(page["pull_requests"])
            if not page["has_more"]:
                break
            args["cursor"] = page["next_cursor"]
        assert {item["number"]: item["draft"] for item in listed if item["number"] in expected} == expected
        blocked = reader.rpc("tools/call", {"name": "create_issue", "arguments": {
            **common, "title": "Read-only metadata must not be created", "milestone": milestones[0]}})
        assert blocked["isError"] and blocked["structuredContent"]["kind"] == "read_only_error"
