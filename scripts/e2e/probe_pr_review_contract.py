"""Observe renamed-fork and review behavior on the disposable core fixture."""
from contextlib import closing, ExitStack
import json
import re
import secrets
import time
from urllib.parse import quote

from mcp_review_client import McpReviewClient
from probe_inline_comments import Http, Parser, decode_json


def verify_pr_review_contract(binary, base, owner, password, token, repo, base_branch, git, *, version):
    nonce = secrets.token_hex(5)
    guest = "review-fork-" + nonce
    secret = secrets.token_urlsafe(24)
    fork = "renamed-" + nonce
    target = f"/{owner}/{repo}"
    rest = "/api/v3/repos" + target
    api = Http(base, token=token)
    admin = Http(base, basic=("root", "root"))
    admin.json("POST", "/api/v3/admin/users", {
        "login": guest, "password": secret, "email": guest + "@example.test",
        "fullName": "Fork contract author", "isAdmin": False,
    })
    api.json("PUT", rest + "/collaborators/" + guest, {"permission": "pull"})
    assert api.json("GET", rest + "/collaborators/" + guest + "/permission")["permission"] == "GUEST"
    guest_web = Http(base)
    guest_web.signin(guest, secret)
    status, _ = guest_web.request("POST", f"/{guest}/_personalToken", form={"note": "review-contract"})
    assert status in {302, 303}
    status, page = guest_web.request("GET", f"/{guest}/_application")
    match = re.search(r'data-clipboard-text="([0-9a-f]{40})"', page)
    assert status == 200 and match
    guest_api = Http(base, token=match.group(1))
    assert guest_api.json("GET", "/api/v3/user")["login"] == guest

    base_sha = api.json("GET", rest + "/branches/" + quote(base_branch, safe=""))["commit"]["sha"]
    head_sha = git("rev-parse", "HEAD")
    control, branch = "review-same-name-" + nonce, "review-renamed-" + nonce
    fork_branch, ready_branch = "fork/draft-" + nonce, "fork/ready-" + nonce
    git("push", "origin", *["HEAD:refs/heads/" + ref
                           for ref in (control, branch, fork_branch, ready_branch)])
    status, _ = guest_web.request("POST", target + "/fork", form={"account": guest})
    assert status in {302, 303}
    original_fork = f"/{guest}/{repo}"
    for _ in range(120):
        status, body = guest_api.request("GET", "/api/v3/repos" + original_fork + "/branches/" + branch)
        if status == 200:
            assert decode_json(body)["commit"]["sha"] == head_sha
            break
        time.sleep(0.1)
    else:
        raise RuntimeError("Disposable fork branch did not become readable")

    original_pr = guest_api.json("POST", rest + "/pulls", {
        "title": "Same-name fork control", "head": f"{guest}:{control}", "base": base_branch,
    })
    assert original_pr["head"]["repo"]["full_name"] == original_fork[1:]
    assert original_pr["head"]["ref"] == control and original_pr["head"]["sha"] == head_sha
    status, _ = guest_web.request("POST", original_fork + "/settings/rename", form={"repositoryName": fork})
    assert status in {302, 303}
    renamed = f"/{guest}/{fork}"
    assert guest_api.request("GET", "/api/v3/repos" + original_fork)[0] == 404
    assert guest_api.json("GET", "/api/v3/repos" + renamed + "/branches/" + branch)["commit"]["sha"] == head_sha

    def pull_numbers():
        return {pr["number"] for pr in api.json("GET", rest + "/pulls")}

    before = pull_numbers()
    assert original_pr["number"] in before
    requested = {"title": "Renamed fork contract", "head": f"{guest}:{branch}", "base": base_branch}
    expected_status = {"4.44.0": 200, "4.48.0": 404}[version]
    for fields in ({}, {"head_repo": fork}):
        status, response = guest_api.request("POST", rest + "/pulls", data={**requested, **fields})
        assert status == expected_status, ("renamed-fork REST", version, status)
        if expected_status == 200:
            # 4.44.0 serializes the failure as a wrapped 404.
            wrapped = json.loads(response)
            assert int(wrapped["status"]) == 404 and "body" in wrapped
        assert pull_numbers() == before

    # Fetch the compare form rendered for this fork and use its actual bindings.
    compare = renamed + "/compare/" + quote(f"{owner}:{base_branch}...{branch}", safe=":") + "?check=true"
    status, page = guest_web.request("GET", compare)
    assert status == 200
    action = target + "/pulls/new"
    form, = [node for node in Parser(page).root.nodes() if node.tag == "form"
             and node.attrs.get("action") == action and node.attrs.get("method", "").lower() == "post"]
    expected = {
        "targetUserName": owner, "targetBranch": base_branch, "requestUserName": guest,
        "requestRepositoryName": fork, "requestBranch": branch, "commitIdFrom": base_sha,
        "commitIdTo": head_sha, "isDraft": "false",
    }
    hidden = [node for node in form.nodes() if node.tag == "input" and node.attrs.get("type") == "hidden"
              and node.attrs.get("name") in expected]
    fields = {node.attrs["name"]: node.attrs["value"] for node in hidden}
    assert len(hidden) == len(expected) and fields == expected
    body = "  Native **Web** body 日本語  \n"
    status, _ = guest_web.request("POST", action, form={
        **fields, "title": requested["title"], "content": body, "isDraft": "true",
    })
    assert status in {302, 303}
    after = pull_numbers()
    assert before <= after
    number, = after - before
    pull = rest + f"/pulls/{number}"
    saved = api.json("GET", pull)
    assert saved["state"] == "open" and saved["draft"] is True and saved["merged"] is False
    assert saved["user"]["login"] == guest and saved["title"] == requested["title"]
    assert saved["body"] == body.strip() and saved["body"] != body
    for side, full_name, ref, sha in [("head", renamed[1:], branch, head_sha),
                                      ("base", target[1:], base_branch, base_sha)]:
        assert saved[side]["repo"]["full_name"] == full_name
        assert saved[side]["ref"] == ref and saved[side]["sha"] == sha

    args = {"owner": owner, "repo": repo, "pull_number": number}
    with closing(McpReviewClient(binary, base, token, owner, password, read_only=False)) as client:
        observed = client.call("get_pull_request", args)
        for key in ("number", "draft", "body"):
            assert observed[key] == saved[key]
        for side in ("head", "base"):
            assert observed[side]["repo"]["full_name"] == saved[side]["repo"]["full_name"]
            assert observed[side]["ref"] == saved[side]["ref"] and observed[side]["sha"] == saved[side]["sha"]

        # A draft could reject reviews even if ready PRs supported them.
        ready = client.call("mark_pull_request_ready", args)
        assert ready["state"] == "open" and ready["draft"] is False and ready["merged"] is False
        comments = rest + f"/issues/{number}/comments"
        api.json("POST", comments, {"body": "Ordinary conversation comment"})
        inline = client.call("add_pull_request_review_comment", {
            **args, "commit": head_sha, "path": "review.txt", "new_line": 2,
            "head_guard": "preflight", "body": "Inline discussion comment",
        })["comment"]
        before_review = client.call("list_pull_request_review_comments", args)
        assert any(comment["id"] == inline["id"] for comment in before_review["comments"])
        ordinary = api.json("GET", comments)
        assert any(comment["body"] == "Ordinary conversation comment" for comment in ordinary)
        for path in ("/reviews", "/requested_reviewers", "/comments"):
            assert api.request("GET", pull + path)[0] == 404, path
        for event in ("APPROVE", "REQUEST_CHANGES"):
            status, _ = api.request("POST", pull + "/reviews", data={
                "event": event, "body": "Native review contract", "commit_id": head_sha,
            })
            assert status == 404, (event, status)
        assert api.json("GET", comments) == ordinary
        assert client.call("list_pull_request_review_comments", args)["comments"] == before_review["comments"]

    # A successful named check can be written and read separately from comments.
    check = "p15/review-contract"
    api.json("POST", rest + "/statuses/" + head_sha, {"state": "success", "context": check})
    statuses = api.json("GET", rest + "/commits/" + head_sha + "/statuses")
    assert any(status["context"] == check and status["state"] == "success" for status in statuses)
    current = api.json("GET", pull)
    assert current["draft"] is False
    for key in ("state", "merged", "head", "base", "title", "body"):
        assert current[key] == saved[key]

    # Advance a distinct destination branch while the fork still shares the older
    # comparison base. This leaves the main branch used by other probes intact.
    fork_base = "fork/base-" + nonce
    git("checkout", "-b", fork_base, base_sha)
    git("commit", "--allow-empty", "-m", "Advance the destination for the fork creation probe")
    base_tip = git("rev-parse", "HEAD")
    git("push", "origin", fork_base)
    git("checkout", "-")
    assert base_tip != base_sha
    decoy = guest_api.json("POST", "/api/v3/user/repos", {"name": repo, "private": True, "auto_init": True})
    assert decoy["full_name"] == original_fork[1:]

    params = {"owner": owner, "repo": repo, "head_owner": guest, "head_repo": fork,
              "head": fork_branch, "base": fork_base, "title": "Fork PR 日本語"}
    tool = "create_fork_pull_request"
    with ExitStack() as clients:
        def connect(**options):
            return clients.enter_context(closing(McpReviewClient(
                binary, base, match.group(1), guest, secret, **options)))

        def rejected(client, arguments, kind):
            result = client.rpc("tools/call", {"name": tool, "arguments": arguments})
            assert result["isError"] and result["structuredContent"]["kind"] == kind

        before = pull_numbers()
        writer = connect(read_only=False)
        rejected(connect(read_only=False, profile="rest-only"), params, "unsupported_error")
        rejected(connect(read_only=False, web_auth=False), params, "authentication_error")
        rejected(connect(), params, "read_only_error")
        rejected(writer, {**params, "body": "  Unpreservable"}, "validation_error")
        rejected(writer, {**params, "head": "fork/missing"}, "api_error")
        rejected(writer, {**params, "head_repo": repo, "head": decoy["default_branch"]}, "api_error")
        assert pull_numbers() == before

        ids = set()
        raw = "日本語 **Markdown**  \n\n    indented\ttext\nend"
        for requested, expected_draft, expected_body in [
            ({**params, "draft": True, "body": raw}, True, raw),
            ({**params, "head": ready_branch, "title": "Ready fork PR"}, False, ""),
        ]:
            result = writer.call(tool, requested)
            number = result["number"]
            ids.add(number)
            read = api.json("GET", rest + f"/pulls/{number}")
            ref = quote(f"refs/pull/{number}/head", safe="")
            fetched, = api.json("GET", rest + f"/commits?sha={ref}&per_page=1")
            assert fetched["sha"] == head_sha
            for observed in (result, read):
                assert observed["title"] == requested["title"] and observed["body"] == expected_body
                assert observed["user"]["login"] == guest and observed["state"] == "open"
                assert observed["draft"] is expected_draft and observed["merged"] is False
                assert observed["head"]["repo"]["full_name"] == renamed[1:]
                assert observed["head"]["ref"] == requested["head"] and observed["head"]["sha"] == head_sha
                assert observed["base"]["repo"]["full_name"] == target[1:]
                assert observed["base"]["ref"] == fork_base and observed["base"]["sha"] == base_sha
        assert len(ids) == 2 and pull_numbers() == before | ids
        assert api.json("GET", rest + "/branches/" + fork_base)["commit"]["sha"] == base_tip
