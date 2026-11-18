"""Verify organization creation and repository visibility through disposable stdio."""
from contextlib import ExitStack
import re
import secrets
from urllib.parse import quote

from mcp_review_client import McpReviewClient
from probe_inline_comments import Http


def verify_repository_discovery(binary, base, owner, token, fixture_repo):
    api = Http(base, token=token)
    admin = Http(base, basic=("root", "root"))
    suffix = secrets.token_hex(5)
    organization = "discovery-org-" + suffix
    guest = "discovery-guest-" + suffix
    secret = secrets.token_urlsafe(24)
    admin.json("POST", "/api/v3/admin/organizations", {"login": organization, "admin": owner})
    admin.json("POST", "/api/v3/admin/users", {
        "login": guest, "password": secret, "email": guest + "@example.test",
        "fullName": "Discovery collaborator", "isAdmin": False,
    })
    web = Http(base)
    web.signin(guest, secret)
    status, _ = web.request("POST", f"/{guest}/_personalToken", form={"note": "disposable-discovery-probe"})
    assert status in {302, 303}
    status, page = web.request("GET", f"/{guest}/_application")
    match = re.search(r'data-clipboard-text="([0-9a-f]{40})"', page)
    assert status == 200 and match

    def error(client, name, args, kind):
        result = client.rpc("tools/call", {"name": name, "arguments": args})
        assert result["isError"] and result["structuredContent"]["kind"] == kind
        return result["structuredContent"]

    def discover(client):
        args = {"limit": 2, "view": "full"}
        names, repositories, observation = [], {}, None
        while True:
            page = client.call("list_visible_repositories", args)
            assert page["range"]["source_complete"] and not page["range"]["atomic"]
            if observation is None:
                observation = page["range"]["observation_id"]
            assert observation == page["range"]["observation_id"]
            for repository in page["repositories"]:
                names.append(repository["full_name"])
                repositories[names[-1]] = repository
            if not page["has_more"]:
                break
            args["cursor"] = page["next_cursor"]
        assert names == sorted(set(names)) and len(names) > 2
        return repositories

    with ExitStack() as resources:
        def connect(pat=token, profile="rest-only", readonly=False):
            client = McpReviewClient(binary, base, pat, "", "", profile=profile,
                                     web_auth=False, read_only=readonly)
            resources.callback(client.close)
            return client

        writer = connect()
        standard = connect(profile="standard")
        guest_writer = connect(match.group(1))
        reader = connect(match.group(1), readonly=True)
        create = "create_organization_repository"
        args = {"organization": organization, "name": "shared"}
        raw = "  Japanese description 日本語  "
        shared = writer.call(create, {**args, "organization": " " + organization + " ", "name": " shared ",
                                      "description": raw, "private": True, "auto_init": True})
        assert shared["full_name"] == f"{organization}/shared"
        assert shared["owner"]["login"] == organization and shared["owner"]["type"] == "Organization"
        assert shared["private"] is True and shared["description"] == raw
        path = f"/api/v3/repos/{organization}/shared"
        branches = api.json("GET", path + "/branches")
        assert len(branches) == 1 and branches[0]["name"] == shared["default_branch"]
        assert re.fullmatch(r"[0-9a-f]{40}", branches[0]["commit"]["sha"])
        assert api.request("GET", path + "/raw/" + quote(shared["default_branch"], safe="") + "/README.md")[0] == 200
        hidden = standard.call(create, {**args, "name": "hidden", "private": True})
        public = writer.call(create, {**args, "name": "public"})
        assert api.json("GET", f"/api/v3/repos/{organization}/hidden/branches") == []
        assert api.json("GET", f"/api/v3/repos/{organization}/public/branches") == []
        assert public["private"] is False and public["description"] == ""
        guest_repo = guest_writer.call("create_repository", {"name": "personal", "private": True})
        denied = error(guest_writer, create, {**args, "name": "denied"}, "operation_not_applied")
        assert denied["cause"]["status"] == 403
        error(writer, create, {**args, "organization": owner, "name": "wrong-owner"}, "validation_error")
        error(writer, create, {**args, "organization": "missing-" + suffix}, "api_error")
        error(reader, create, {**args, "name": "readonly"}, "read_only_error")
        for target in [f"{organization}/denied", f"{organization}/readonly", f"{owner}/wrong-owner", f"{owner}/shared"]:
            assert api.request("GET", "/api/v3/repos/" + target)[0] == 404

        own_path = f"/api/v3/repos/{owner}/{fixture_repo}"
        api.json("PUT", path + f"/collaborators/{guest}", {"permission": "pull"})
        api.json("PUT", own_path + f"/collaborators/{guest}", {"permission": "pull"})
        visible = discover(reader)
        assert shared["full_name"] in visible and visible[shared["full_name"]]["private"] is True
        assert public["full_name"] in visible and guest_repo["full_name"] in visible
        assert f"{owner}/{fixture_repo}" in visible and hidden["full_name"] not in visible
        assert hidden["full_name"] in discover(standard)
        assert {r["full_name"] for r in Http(base, token=match.group(1)).json("GET", f"/api/v3/users/{guest}/repos")} == {
            guest_repo["full_name"],
        }
