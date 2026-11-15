"""Individual assignee mutations through actual stdio in disposable fixtures."""
from contextlib import ExitStack
import re
import secrets
from urllib.parse import quote

from mcp_review_client import McpReviewClient
from probe_inline_comments import Http


def verify_issue_assignees(binary, base, owner, password, token, repo, pull_number):
    api = Http(base, token=token)
    admin = Http(base, basic=("root", "root"))
    rest = "/api/v3/repos/" + quote(owner, safe="") + "/" + quote(repo, safe="")
    common = {"owner": owner, "repo": repo}
    accounts = []
    for label in ("guest", "external", "suspended"):
        login, secret = "assignee-" + label + "-" + secrets.token_hex(5), secrets.token_urlsafe(24)
        admin.json("POST", "/api/v3/admin/users", {"login": login, "password": secret,
                   "email": login + "@example.test", "fullName": "表示名 " + label, "isAdmin": False})
        accounts.append((login, secret))
    guest, guest_secret = accounts[0]
    outsider, disabled = accounts[1][0], accounts[2][0]
    api.json("PUT", rest + "/collaborators/" + guest, {"permission": "pull"})
    web = Http(base)
    web.signin(guest, guest_secret)
    status, _ = web.request("POST", "/" + guest + "/_personalToken", form={"note": "disposable-assignee-probe"})
    assert status in {302, 303}
    status, page = web.request("GET", "/" + guest + "/_application")
    match = re.search(r'data-clipboard-text="([0-9a-f]{40})"', page)
    assert status == 200 and match

    with ExitStack() as resources:
        def connect(pat=token, user=owner, secret=password, **options):
            client = McpReviewClient(binary, base, pat, user, secret,
                                    read_only=options.pop("read_only", False), **options)
            resources.callback(client.close)
            return client

        writer = connect()
        reader = connect(read_only=True, web_auth=False)
        guest_client = connect(match.group(1), guest, guest_secret)
        rest_only, pat_only = connect(profile="rest-only"), connect(web_auth=False)
        tools = {t["name"]: t["inputSchema"] for t in writer.rpc("tools/list", {})["tools"]}
        for name in ("add_issue_assignees", "replace_issue_assignees"):
            assert set(tools[name]["required"]) == {"owner", "repo", "issue_number", "assignees"}
        assert set(tools["remove_issue_assignee"]["required"]) == {"owner", "repo", "issue_number", "username"}

        raw = "  日本語 **Markdown**  \n\n    code\nlast  \n"
        milestone = writer.call("create_milestone", {**common, "title": "assignee-preserved-milestone"})
        seed = writer.call("create_issue", {**common, "title": "Assignee label seed"})
        exact_label = " assignee 日本語 & label "
        writer.call("add_issue_labels", {**common, "issue_number": seed["number"], "labels": [exact_label]})
        issue = writer.call("create_issue", {**common, "title": "Assignee fixture", "body": raw,
                            "milestone": milestone["number"], "labels": [exact_label], "assignees": [owner]})
        writer.call("update_issue", {**common, "issue_number": issue["number"], "state": "closed"})
        pr_original = reader.call("get_pull_request", {**common, "pull_number": pull_number})
        writer.call("update_pull_request", {**common, "pull_number": pull_number, "state": "closed"})
        pr_before = reader.call("get_pull_request", {**common, "pull_number": pull_number})

        def names(value):
            return {u["login"] for u in value["assignees"]}

        for number in (issue["number"], pull_number):
            target = {**common, "issue_number": number}
            before = reader.call("get_issue", target)
            preserved = {k: before[k] for k in ("title", "body", "state", "labels", "milestone")}

            def verify(value, expected):
                assert names(value) == set(expected)
                assert {k: value[k] for k in preserved} == preserved
                assert names(reader.call("get_issue", target)) == set(expected)
                return value

            initial = names(before)
            desired = initial | {owner, guest, outsider}
            for _ in range(2):
                verify(writer.call("add_issue_assignees", {**target, "assignees": [" " + guest + " ", owner, outsider]}), desired)
            # Native new/delete permits any existing individual account, even
            # a GUEST or a non-collaborator that the UI suggestion list omits.
            verify(guest_client.call("add_issue_assignees", {**target, "assignees": [owner]}), desired)
            for name, arguments in [
                ("remove_issue_assignee", {**target, "username": owner}),
                ("replace_issue_assignees", {**target, "assignees": []}),
            ]:
                result = guest_client.rpc("tools/call", {"name": name, "arguments": arguments})
                assert result["isError"]
                assert names(reader.call("get_issue", target)) == desired
            for client, kind in [(rest_only, "unsupported_error"), (pat_only, "authentication_error"), (reader, "read_only_error")]:
                for name, extra in [("add_issue_assignees", {"assignees": [owner]}),
                                    ("replace_issue_assignees", {"assignees": []}),
                                    ("remove_issue_assignee", {"username": owner})]:
                    result = client.rpc("tools/call", {"name": name, "arguments": {**target, **extra}})
                    assert result["isError"] and result["structuredContent"]["kind"] == kind
            for _ in range(2):
                verify(writer.call("remove_issue_assignee", {**target, "username": " " + guest + " "}), desired - {guest})
            for _ in range(2):
                verify(writer.call("replace_issue_assignees", {**target, "assignees": [guest, outsider]}), {guest, outsider})
            for _ in range(2):
                verify(writer.call("replace_issue_assignees", {**target, "assignees": []}), set())
            guest_client.call("replace_issue_assignees", {**target, "assignees": []})
            for extra, kind in [
                ({"assignees": [owner, "missing-" + secrets.token_hex(5)]}, "api_error"),
                ({"assignees": [owner, " " + owner]}, "validation_error"),
                ({"assignees": []}, "validation_error"),
                ({"assignees": ["../invalid"]}, "validation_error"),
            ]:
                result = writer.rpc("tools/call", {"name": "add_issue_assignees", "arguments": {**target, **extra}})
                assert result["isError"] and result["structuredContent"]["kind"] == kind
                assert names(reader.call("get_issue", target)) == set()

        pr_after = reader.call("get_pull_request", {**common, "pull_number": pull_number})
        for field in ("title", "body", "state", "draft", "head", "base", "labels"):
            assert pr_after[field] == pr_before[field]
        writer.call("update_pull_request", {**common, "pull_number": pull_number, "state": pr_original["state"]})
        assert reader.call("get_issue", {**common, "issue_number": issue["number"]})["body"] == raw
        assert reader.call("get_milestone", {**common, "milestone_number": milestone["number"]})["number"] == milestone["number"]
        for name, extra in [("add_issue_assignees", {"assignees": [owner]}),
                            ("replace_issue_assignees", {"assignees": []}),
                            ("remove_issue_assignee", {"username": owner})]:
            result = writer.rpc("tools/call", {"name": name, "arguments": {
                **common, "issue_number": 2147483647, **extra}})
            assert result["isError"] and result["structuredContent"]["kind"] == "api_error"

        # Suspending an assigned account leaves its raw relation but hides it
        # from REST. [] must not become a successful clear/no-op in that state.
        hidden = writer.call("create_issue", {**common, "title": "Hidden assigned account"})
        target = {**common, "issue_number": hidden["number"]}
        writer.call("add_issue_assignees", {**target, "assignees": [disabled]})
        status, _ = admin.request("PUT", "/api/v3/users/" + disabled + "/suspended")
        assert status == 204
        assert names(reader.call("get_issue", target)) == set()
        result = writer.rpc("tools/call", {"name": "replace_issue_assignees", "arguments": {**target, "assignees": []}})
        assert result["isError"]
        status, _ = admin.request("DELETE", "/api/v3/users/" + disabled + "/suspended")
        assert status == 204
        assert names(reader.call("get_issue", target)) == {disabled}
        writer.call("replace_issue_assignees", {**target, "assignees": []})
