"""Observe renamed-fork and review behavior on the disposable core fixture."""
from contextlib import closing
import json
import re
import secrets
import time
from urllib.parse import quote

from mcp_review_client import McpReviewClient
from probe_inline_comments import Http, Parser, decode_json


def verify_pr_review_contract(binary, base, owner, password, token, repo, base_branch, git):
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
    git("push", "origin", "HEAD:refs/heads/" + control, "HEAD:refs/heads/" + branch)
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
    statuses = []
    for fields in ({}, {"head_repo": fork}):
        status, response = guest_api.request("POST", rest + "/pulls", data={**requested, **fields})
        if status == 200:
            # 4.44.0 serializes the failure as a wrapped 404.
            wrapped = json.loads(response)
            assert int(wrapped["status"]) == 404 and "body" in wrapped
        else:
            assert status == 404, ("renamed-fork REST", status)
        statuses.append(status)
        assert pull_numbers() == before
    assert statuses[0] == statuses[1]

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

    comments = rest + f"/issues/{number}/comments"
    api.json("POST", comments, {"body": "Ordinary conversation comment"})
    args = {"owner": owner, "repo": repo, "pull_number": number}
    with closing(McpReviewClient(binary, base, token, owner, password, read_only=False)) as client:
        observed = client.call("get_pull_request", args)
        for key in ("number", "draft", "body"):
            assert observed[key] == saved[key]
        for side in ("head", "base"):
            assert observed[side]["repo"]["full_name"] == saved[side]["repo"]["full_name"]
            assert observed[side]["ref"] == saved[side]["ref"] and observed[side]["sha"] == saved[side]["sha"]
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
    for key in ("state", "draft", "merged", "head", "base"):
        assert current[key] == saved[key]
