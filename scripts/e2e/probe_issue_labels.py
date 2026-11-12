"""Issue/PR label assignments in the changes probe's disposable private repository."""
from contextlib import ExitStack
import re
import secrets
from urllib.parse import quote

from mcp_review_client import McpReviewClient
from probe_inline_comments import Http


def verify_issue_labels(binary, base, owner, token, repo, pull_number):
    api = Http(base, token=token)
    rest = "/api/v3/repos/" + quote(owner, safe="") + "/" + quote(repo, safe="")
    issue = api.json("POST", rest + "/issues", {"title": "Issue label assignment contract"})
    guest, secret = "label-reader-" + secrets.token_hex(5), secrets.token_urlsafe(24)
    Http(base, basic=("root", "root")).json("POST", "/api/v3/admin/users", {
        "login": guest, "password": secret, "email": guest + "@example.test", "fullName": "Label reader", "isAdmin": False})
    api.json("PUT", rest + "/collaborators/" + guest, {"permission": "pull"})
    assert api.json("GET", rest + "/collaborators/" + guest + "/permission")["permission"] == "GUEST"
    web = Http(base)
    web.signin(guest, secret)
    status, _ = web.request("POST", "/" + guest + "/_personalToken", form={"note": "disposable-label-probe"})
    assert status in {302, 303}
    status, page = web.request("GET", "/" + guest + "/_application")
    match = re.search(r'data-clipboard-text="([0-9a-f]{40})"', page)
    assert status == 200 and match
    common = {"owner": owner, "repo": repo}
    exact = " edge 日本語 &+#? "

    with ExitStack() as resources:
        def connect(pat, readonly=False):
            client = McpReviewClient(binary, base, pat, "", "", web_auth=False, read_only=readonly, profile="rest-only")
            resources.callback(client.close)
            return client
        writer, reader, guest_client = connect(token), connect(token, True), connect(match.group(1))

        def collect(args, first=None):
            data = first or reader.call("list_issue_labels", {**args, "view": "full"})
            observation = data["range"]["observation_id"]
            items = []
            while True:
                assert data["range"]["observation_id"] == observation and data["range"]["source_complete"]
                items.extend(data["labels"])
                if not data["has_more"]:
                    assert data["next_cursor"] is None
                    return items
                data = reader.call("list_issue_labels", {**args, "view": "full", "cursor": data["next_cursor"]})

        def names(items):
            result = {item["name"] for item in items}
            assert len(result) == len(items)
            return result

        def change(tool, number, expected, **params):
            result = writer.call(tool, {**common, "issue_number": number, **params})
            assert result["issue_number"] == number and names(result["labels"]) == expected
            assert names(collect({**common, "issue_number": number, "limit": 7})) == expected
            assert names(api.json("GET", rest + f"/issues/{number}")["labels"]) == expected
            return result

        writer.call("create_label", {**common, "name": "keep", "color": "12ab34"})
        for number in (issue["number"], pull_number):
            target = {**common, "issue_number": number}
            assert collect({**target, "limit": 7}) == []
            change("add_issue_labels", number, {"keep", "remove"}, labels=["keep", "remove"])
            change("add_issue_labels", number, {"keep", "remove", exact}, labels=["keep", exact])
            assert writer.call("get_label", {**common, "name": "keep"})["color"] == "12ab34"
            created = api.json("GET", rest + "/labels/" + quote(exact, safe=""))
            assert created["name"] == exact and re.fullmatch(r"[0-9a-f]{6}", created["color"])
            change("remove_issue_label", number, {"keep", "remove"}, name=exact)
            assert api.json("GET", rest + "/labels/" + quote(exact, safe=""))["name"] == exact
            change("replace_issue_labels", number, {"keep", "new"}, labels=["keep", "new"])
            change("remove_issue_label", number, {"keep"}, name="new")
            change("remove_issue_label", number, {"keep"}, name="not-assigned")
            for tool, args in [("add_issue_labels", {"labels": ["denied"]}),
                               ("replace_issue_labels", {"labels": []}), ("remove_issue_label", {"name": "keep"})]:
                rejected = guest_client.rpc("tools/call", {"name": tool, "arguments": {**target, **args}})
                assert rejected["isError"] and rejected["structuredContent"]["kind"] == "operation_not_applied"
                assert rejected["structuredContent"]["status"] in {401, 403}
                blocked = reader.rpc("tools/call", {"name": tool, "arguments": {**target, **args}})
                assert blocked["isError"] and blocked["structuredContent"]["kind"] == "read_only_error"
            assert names(guest_client.call("add_issue_labels", {**target, "labels": ["keep"]})["labels"]) == {"keep"}
            assert names(guest_client.call("remove_issue_label", {**target, "name": "not-assigned"})["labels"]) == {"keep"}
            change("replace_issue_labels", number, set(), labels=[])
            assert writer.call("get_label", {**common, "name": "keep"})["color"] == "12ab34"
        number = issue["number"]
        batch = ["batch-" + str(i) for i in range(55)]
        change("add_issue_labels", number, set(batch), labels=batch)
        args = {**common, "issue_number": number, "limit": 7, "view": "full"}
        first = reader.call("list_issue_labels", args)
        assert first["has_more"]
        change("remove_issue_label", number, set(batch) - {batch[0]}, name=batch[0])
        assert names(collect(args, first)) == set(batch)
        assert names(collect(args)) == set(batch) - {batch[0]}
        change("replace_issue_labels", number, set(), labels=[])
        for invalid in [[], [""], ["keep", "keep"], ["line\nbreak"]]:
            result = writer.rpc("tools/call", {"name": "add_issue_labels", "arguments": {**common, "issue_number": number, "labels": invalid}})
            assert result["isError"] and result["structuredContent"]["kind"] == "validation_error"
        never_created = "not-created-" + secrets.token_hex(5)
        for tool, params in [("list_issue_labels", {}), ("add_issue_labels", {"labels": [never_created]}),
                             ("replace_issue_labels", {"labels": [never_created]}), ("remove_issue_label", {"name": "keep"})]:
            result = writer.rpc("tools/call", {"name": tool, "arguments": {**common, "issue_number": 2147483647, **params}})
            assert result["isError"] and result["structuredContent"]["status"] == 404
        assert api.request("GET", rest + "/labels/" + never_created)[0] == 404
