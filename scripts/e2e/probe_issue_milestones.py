"""Issue/PR milestone assignment through actual stdio in a disposable fixture."""
from contextlib import ExitStack
import re
import secrets
from urllib.parse import quote

from mcp_review_client import McpReviewClient
from probe_inline_comments import Http


def verify_issue_milestones(binary, base, owner, password, token, repo, pull_number):
    api = Http(base, token=token)
    rest = "/api/v3/repos/" + quote(owner, safe="") + "/" + quote(repo, safe="")
    common = {"owner": owner, "repo": repo}
    guest, secret = "milestone-reader-" + secrets.token_hex(5), secrets.token_urlsafe(24)
    Http(base, basic=("root", "root")).json("POST", "/api/v3/admin/users", {
        "login": guest, "password": secret, "email": guest + "@example.test", "fullName": "Milestone reader", "isAdmin": False})
    api.json("PUT", rest + "/collaborators/" + guest, {"permission": "pull"})
    guest_web = Http(base)
    guest_web.signin(guest, secret)
    status, _ = guest_web.request("POST", "/" + guest + "/_personalToken", form={"note": "disposable-milestone-probe"})
    assert status in {302, 303}
    status, page = guest_web.request("GET", "/" + guest + "/_application")
    match = re.search(r'data-clipboard-text="([0-9a-f]{40})"', page)
    assert status == 200 and match
    raw = "  日本語 **Markdown**  \n\n    code\nlast  \n"
    exact_label = " milestone 日本語 & label "

    with ExitStack() as resources:
        def connect(pat=token, username=owner, pwd=password, **options):
            client = McpReviewClient(binary, base, pat, username, pwd, read_only=options.pop("read_only", False), **options)
            resources.callback(client.close)
            return client

        writer = connect()
        reader = connect(read_only=True, web_auth=False)
        guest_client = connect(match.group(1), guest, secret)
        rest_only = connect(profile="rest-only")
        pat_only = connect(web_auth=False)
        schema = {tool["name"]: tool["inputSchema"] for tool in writer.rpc("tools/list", {})["tools"]}
        assert set(schema["set_issue_milestone"]["required"]) == {"owner", "repo", "issue_number", "milestone_number"}
        assert set(schema["clear_issue_milestone"]["required"]) == {"owner", "repo", "issue_number"}
        milestones = []
        for closed in (False, True):
            milestone = writer.call("create_milestone", {**common, "title": "assignment-" + ("closed" if closed else "open")})
            if closed:
                milestone = writer.call("update_milestone", {**common, "milestone_number": milestone["number"], "state": "closed"})
            milestones.append(milestone["number"])
        seed = writer.call("create_issue", {**common, "title": "Milestone assignment seed", "body": raw})
        writer.call("add_issue_labels", {**common, "issue_number": seed["number"], "labels": [exact_label]})
        issue = writer.call("create_issue", {**common, "title": "Closed milestone fixture", "body": raw,
                            "labels": [exact_label], "assignees": [owner, guest]})
        writer.call("update_issue", {**common, "issue_number": issue["number"], "state": "closed"})
        preserved = ("title", "body", "state", "labels", "assignees")
        pr_before = reader.call("get_pull_request", {**common, "pull_number": pull_number})
        for number in (issue["number"], pull_number):
            target = {**common, "issue_number": number}
            before = reader.call("get_issue", target)
            for milestone in milestones:
                for _ in range(2):
                    observed = writer.call("set_issue_milestone", {**target, "milestone_number": milestone})
                    assert observed["milestone"]["number"] == milestone
                    assert {key: observed[key] for key in preserved} == {key: before[key] for key in preserved}
                    assert reader.call("get_issue", target)["milestone"]["number"] == milestone
            # Guest can observe an already satisfied set, but a real change is refused.
            guest_client.call("set_issue_milestone", {**target, "milestone_number": milestones[-1]})
            denied = guest_client.rpc("tools/call", {"name": "set_issue_milestone", "arguments": {**target, "milestone_number": milestones[0]}})
            assert denied["isError"]
            assert reader.call("get_issue", target)["milestone"]["number"] == milestones[-1]
            denied = guest_client.rpc("tools/call", {"name": "clear_issue_milestone", "arguments": target})
            assert denied["isError"]
            assert reader.call("get_issue", target)["milestone"]["number"] == milestones[-1]
            for client, kind in [(rest_only, "unsupported_error"), (pat_only, "authentication_error"), (reader, "read_only_error")]:
                for name, arguments in [("set_issue_milestone", {**target, "milestone_number": milestones[0]}), ("clear_issue_milestone", target)]:
                    result = client.rpc("tools/call", {"name": name, "arguments": arguments})
                    assert result["isError"] and result["structuredContent"]["kind"] == kind
            for _ in range(2):
                observed = writer.call("clear_issue_milestone", target)
                assert observed["milestone"] is None
                assert {key: observed[key] for key in preserved} == {key: before[key] for key in preserved}
                assert reader.call("get_issue", target)["milestone"] is None
            # Native omitted/null REST metadata alone is not used as clear proof.
            guest_client.call("clear_issue_milestone", target)
        assert reader.call("get_issue", {**common, "issue_number": issue["number"]})["body"] == raw
        pr_after = reader.call("get_pull_request", {**common, "pull_number": pull_number})
        for field in ("title", "body", "state", "draft", "head", "base", "labels", "assignees"):
            assert pr_after[field] == pr_before[field]
        foreign_repo = api.json("POST", "/api/v3/user/repos", {"name": "milestone-scope-" + secrets.token_hex(5), "private": True, "auto_init": True})
        foreign = api.json("POST", "/api/v3/repos/" + quote(owner, safe="") + "/" + quote(foreign_repo["name"], safe="") + "/milestones", {"title": "foreign"})
        target = {**common, "issue_number": issue["number"]}
        for extra, kind in [({"milestone_number": foreign["number"]}, "api_error"), ({"milestone_number": 2147483647}, "api_error"),
                            ({"milestone_number": 0}, "validation_error"), ({"milestone_number": 2147483648}, "validation_error")]:
            result = writer.rpc("tools/call", {"name": "set_issue_milestone", "arguments": {**target, **extra}})
            assert result["isError"] and result["structuredContent"]["kind"] == kind
        assert reader.call("get_issue", target)["milestone"] is None
        for name, arguments in [("set_issue_milestone", {**common, "issue_number": 2147483647, "milestone_number": milestones[0]}),
                                ("clear_issue_milestone", {**common, "issue_number": 2147483647})]:
            result = writer.rpc("tools/call", {"name": name, "arguments": arguments})
            assert result["isError"] and result["structuredContent"]["kind"] == "api_error"
        for milestone in milestones:
            assert reader.call("get_milestone", {**common, "milestone_number": milestone})["number"] == milestone
