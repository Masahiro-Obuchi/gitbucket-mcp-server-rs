#!/usr/bin/env python3
"""P08 contract experiment, restricted to an explicitly disposable loopback server.

This is a research harness, not an MCP adapter. It creates a private repository,
a guest account, Git commits and comments. Destroy the dedicated Docker volume
afterwards. Only selected, credential-free response fragments are recorded.
"""

import argparse
import base64
from contextlib import ExitStack
import html
from html.parser import HTMLParser
import http.cookiejar
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import tempfile
from urllib.error import HTTPError
from urllib.parse import quote, urlencode, urlsplit
from urllib.request import (
    build_opener, HTTPCookieProcessor, HTTPRedirectHandler, ProxyHandler, Request,
)


class Node:
    def __init__(self, tag="root", attrs=()):
        self.tag = tag
        self.attrs = dict(attrs)
        self.children = []

    def nodes(self):
        yield self
        for child in self.children:
            if isinstance(child, Node):
                yield from child.nodes()

    def text(self):
        return "".join(c.text() if isinstance(c, Node) else c for c in self.children)

    def render(self):
        if self.tag == "script":
            return ""
        attrs = "".join(f' {k}="{html.escape(v or "", quote=True)}"' for k, v in self.attrs.items())
        content = "".join(c.render() if isinstance(c, Node) else html.escape(c) for c in self.children)
        if self.tag in Parser.VOID:
            return f"<{self.tag}{attrs}>"
        return f"<{self.tag}{attrs}>{content}</{self.tag}>"


class Parser(HTMLParser):
    VOID = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"}

    def __init__(self, value):
        super().__init__(convert_charrefs=True)
        self.root = Node()
        self.stack = [self.root]
        self.feed(value)
        self.close()

    def handle_starttag(self, tag, attrs):
        node = Node(tag, attrs)
        self.stack[-1].children.append(node)
        if tag not in self.VOID:
            self.stack.append(node)

    def handle_endtag(self, tag):
        for index in range(len(self.stack) - 1, 0, -1):
            if self.stack[index].tag == tag:
                del self.stack[index:]
                break

    def handle_data(self, value):
        self.stack[-1].children.append(value)


def comment_number(node):
    identifier = node.attrs.get("id", "")
    if re.fullmatch(r"discussion_r\d+", identifier):
        return int(identifier.removeprefix("discussion_r"))
    for name in node.attrs.get("class", "").split():
        if re.fullmatch(r"commit-comment-\d+", name):
            return int(name.removeprefix("commit-comment-"))
    return None


def comment_nodes(page):
    # The whole-commit card uses a class ID, while file/line cards use an ID
    # attribute. Looking only for discussion_r silently loses the former.
    return [n for n in Parser(page).root.nodes()
            if "commit-comment-box" in n.attrs.get("class", "").split()
            and comment_number(n) is not None]


def describe(page):
    root = Parser(page).root
    comments = []
    for node in comment_nodes(page):
        number = comment_number(node)
        body = next(n for n in node.nodes()
                    if f"commit-commentContent-{number}" in n.attrs.get("class", "").split())
        comments.append({
            "id": number, "path": node.attrs.get("filename"),
            "kind": "commit" if "filename" not in node.attrs else
                    "line" if "oldline" in node.attrs or "newline" in node.attrs else "file",
            "old_line": node.attrs.get("oldline"), "new_line": node.attrs.get("newline"),
            "inline_class": "inline-comment" in node.attrs.get("class", "").split(),
            "author": next(n.text() for n in node.nodes() if "username" in n.attrs.get("class", "").split()),
            "body_html": body.render(),
        })
    return {
        "comments": comments,
        "folded_groups": [n.attrs["class"] for n in root.nodes()
                          if n.attrs.get("class", "").startswith("fold-comments-")
                          and "display: none" in n.attrs.get("style", "")],
    }


def loopback_url(value):
    parsed = urlsplit(value)
    if (parsed.scheme != "http" or parsed.hostname not in {"127.0.0.1", "::1", "localhost"}
            or parsed.username is not None or parsed.password is not None
            or parsed.query or parsed.fragment or parsed.path not in {"", "/"}):
        raise ValueError("probe requires an HTTP loopback URL without credentials or a context path")
    return value.rstrip("/")


def decode_json(body):
    value = json.loads(body) if body else None
    for _ in range(3):
        if isinstance(value, str):
            value = json.loads(value)
        elif isinstance(value, dict) and "status" in value and "body" in value:
            status = int(value["status"])
            if not 200 <= status < 300:
                raise RuntimeError(f"upstream wrapped HTTP {status}")
            value = value["body"]
        else:
            return value
    raise RuntimeError("unsupported nested JSON response")


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class Http:
    def __init__(self, base, token=None, basic=None):
        self.base = base
        self.token = token
        self.basic = basic
        self.opener = build_opener(ProxyHandler({}), NoRedirect(), HTTPCookieProcessor(http.cookiejar.CookieJar()))

    def request(self, method, path, *, data=None, form=None, ajax=False):
        headers = {}
        if self.token:
            headers["Authorization"] = "token " + self.token
        if self.basic:
            headers["Authorization"] = "Basic " + base64.b64encode(":".join(self.basic).encode()).decode()
        if ajax:
            headers["X-Requested-With"] = "XMLHttpRequest"
        body = None
        if data is not None:
            headers["Content-Type"] = "application/json"
            body = json.dumps(data).encode()
        if form is not None:
            headers["Content-Type"] = "application/x-www-form-urlencoded"
            body = urlencode(form).encode()
        request = Request(self.base + path, data=body, headers=headers, method=method)
        try:
            with self.opener.open(request, timeout=30) as response:
                return response.status, response.read(8 * 1024 * 1024).decode()
        except HTTPError as error:
            return error.code, error.read(8 * 1024 * 1024).decode()

    def json(self, method, path, data=None):
        status, body = self.request(method, path, data=data)
        if status not in {200, 201, 204}:
            raise RuntimeError(f"{method} failed with HTTP {status}")
        return decode_json(body)

    def signin(self, user, password):
        status, _ = self.request("POST", "/signin", form={"userName": user, "password": password, "hash": ""})
        if status not in {302, 303}:
            raise RuntimeError(f"signin returned HTTP {status}")


def run(output, version, mcp_binary=None, verify_posts=False):
    base = loopback_url(os.environ["GITBUCKET_E2E_URL"])
    if verify_posts and not mcp_binary:
        raise ValueError("--verify-posts requires --mcp-binary")
    user = os.environ["GITBUCKET_E2E_GIT_USERNAME"]
    password = os.environ["GITBUCKET_E2E_GIT_PASSWORD"]
    token = os.environ["GITBUCKET_E2E_TOKEN"]
    api = Http(base, token=token)
    assert api.json("GET", "/api/v3/user")["login"] == user
    nonce = secrets.token_hex(5)
    repo = "inline-probe-" + nonce
    reader = "inline-reader-" + nonce
    reader_password = secrets.token_urlsafe(24)
    admin = Http(base, basic=("root", "root"))
    admin.json("POST", "/api/v3/admin/users", {
        "login": reader, "password": reader_password, "email": reader + "@example.test",
        "fullName": "Inline reader", "isAdmin": False,
    })
    repository = api.json("POST", "/api/v3/user/repos", {"name": repo, "private": True, "auto_init": True})
    owner_repo = quote(user, safe="") + "/" + quote(repo, safe="")
    rest = "/api/v3/repos/" + owner_repo
    web = "/" + owner_repo
    api.json("PUT", rest + "/collaborators/" + reader, {"permission": "pull"})
    permission = api.json("GET", rest + "/collaborators/" + reader + "/permission")
    owner_web, guest_web = Http(base), Http(base)
    owner_web.signin(user, password)
    guest_web.signin(reader, reader_password)
    output.mkdir(parents=True, exist_ok=False)
    result = {"version": version, "repository_private": True, "reader_permission": permission,
              "requests": [], "snapshots": {}, "cases": {}}
    aliases = {base: "http://gitbucket.test", user: "probe-owner", reader: "probe-reader", repo: "probe-repo"}

    def request(label, http, method, path, **kwargs):
        status, body = http.request(method, path, **kwargs)
        result["requests"].append({"label": label, "method": method, "path": path, "status": status,
                                   "form": kwargs.get("form")})
        return status, body

    mcp_clients = {}
    mcp_writers = {}

    def snapshot(label, path, http=guest_web):
        status, body = request(label, http, "GET", path)
        assert status == 200, (label, status)
        result["snapshots"][label] = describe(body)
        result["snapshots"][label]["normal_comment_text_present"] = "normal issue comment" in body
        result["snapshots"][label]["whole_commit_text_present"] = "commit_whole" in body
        fragment = "\n".join(n.render() for n in comment_nodes(body))
        (output / (label + ".html")).write_text(fragment, encoding="utf-8")
        if mcp_clients and label.startswith("conversation_"):
            expected = {c["id"]: c for c in describe(body)["comments"]}
            for identity, mcp in mcp_clients.items():
                args = {"owner": user, "repo": repo, "pull_number": number}
                observed = mcp.call("list_pull_request_review_comments", args)
                assert {c["storage_id"] for c in observed["comments"]} == set(expected), (label, identity, "ID set")
                coverage = observed["coverage"]
                assert coverage["complete"] is False and coverage["historical_complete"] is False
                assert coverage["body_raw_complete"] is False and coverage["association_verified"] is False
                assert coverage["observed_head"] == api.json("GET", rest + f"/pulls/{number}")["head"]["sha"]
                for comment in observed["comments"]:
                    source = expected[comment["storage_id"]]
                    assert comment["id"] == "commit:" + str(source["id"])
                    assert comment["kind"] == source["kind"] and comment["author"] == source["author"]
                    assert comment["path"] == source["path"]
                    assert comment["body_format"] == "html" and comment["body_raw"] is None
                    assert Parser(comment["body_html"]).root.text() == Parser(source["body_html"]).root.text()
                    assert comment["position_verified"] is False
                    for field in ["original_commit", "original_old_line", "original_new_line", "created_at", "updated_at"]:
                        assert comment[field] is None
                    for side in ["old", "new"]:
                        line = source[side + "_line"]
                        assert comment["observed_" + side + "_line"] == (int(line) if line else None)
                for cid in [3, 99999]:
                    lookup = mcp.call("get_pull_request_review_comment", {**args, "comment_id": f"commit:{cid}"})
                    assert lookup["status"] == ("found" if cid in expected else "not_observed")
                    assert (lookup["comment"] is not None) == (cid in expected)
                    assert lookup["coverage"]["complete"] is False
                result.setdefault("mcp_checks", []).append({"snapshot": label, "identity": identity,
                                                            "comments": len(expected), "passed": True})
        return body

    with ExitStack() as resources:
        directory = resources.enter_context(tempfile.TemporaryDirectory(prefix="gitbucket-inline-"))
        work = Path(directory)
        askpass = work / "askpass.sh"
        askpass.write_text('#!/bin/sh\ncase "$1" in *Username*) printf "%s\\n" "$INLINE_GIT_USER";; *) printf "%s\\n" "$INLINE_GIT_PASSWORD";; esac\n')
        askpass.chmod(0o700)
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        env.update({"GIT_ASKPASS": str(askpass), "GIT_TERMINAL_PROMPT": "0",
                    "INLINE_GIT_USER": user, "INLINE_GIT_PASSWORD": password})
        checkout = work / "repo"

        def git(*args):
            completed = subprocess.run(["git", "-c", "credential.helper=", *args], cwd=checkout if checkout.exists() else work,
                                       env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=90)
            if completed.returncode:
                raise RuntimeError("Git command failed: " + args[0])
            return completed.stdout.strip()

        git("clone", base + "/git/" + owner_repo + ".git", str(checkout))
        git("config", "user.name", "Inline probe")
        git("config", "user.email", "inline@example.test")
        default = repository["default_branch"]
        (checkout / "review.txt").write_text("alpha\nold value\nomega\n")
        (checkout / "deleted.txt").write_text("deleted line\n")
        (checkout / "rename before.txt").write_text("".join(f"line {i}\n" for i in range(1, 21)))
        git("add", ".")
        git("commit", "-m", "Seed base fixture")
        git("push", "origin", default)
        base_sha = git("rev-parse", "HEAD")
        aliases[base_sha] = "BASE"
        branch = "inline-probe"
        git("checkout", "-b", branch)
        (checkout / "review.txt").write_text("alpha\nnew value\nomega\n")
        (checkout / "deleted.txt").unlink()
        git("mv", "rename before.txt", "renamed_日本語.txt")
        (checkout / "renamed_日本語.txt").write_text("".join(("changed seven\n" if i == 7 else f"line {i}\n") for i in range(1, 21)))
        (checkout / 'quoted & name.txt').write_text("new file\n")
        if verify_posts:
            (checkout / '新規 & "quoted".txt').write_text("added Unicode file\n")
        git("add", ".")
        git("commit", "-m", "Change old and new sides, rename and delete")
        first_sha = git("rev-parse", "HEAD")
        aliases[first_sha] = "FIRST"
        (checkout / "second.txt").write_text("second commit\n")
        git("add", ".")
        git("commit", "-m", "Second PR commit")
        git("push", "origin", branch)
        head = git("rev-parse", "HEAD")
        aliases[head] = "HEAD1"
        pr = api.json("POST", rest + "/pulls", {"title": "Inline contract probe", "head": branch, "base": default})
        number = pr["number"]
        if mcp_binary:
            from mcp_review_client import McpReviewClient
            status, token_page = guest_web.request("POST", "/" + reader + "/_personalToken", form={"note": "p09-disposable-probe"})
            assert status in {302, 303}, "guest PAT creation did not redirect to applications"
            status, token_page = guest_web.request("GET", "/" + reader + "/_application")
            match = re.search(r'data-clipboard-text="([0-9a-f]{40})"', token_page)
            assert status == 200 and match, "guest PAT creation failed"
            for identity, pat, login, secret in [("owner", token, user, password), ("guest", match.group(1), reader, reader_password)]:
                mcp = McpReviewClient(mcp_binary, base, pat, login, secret)
                resources.callback(mcp.close)
                mcp_clients[identity] = mcp
                if verify_posts:
                    writer = McpReviewClient(mcp_binary, base, pat, login, secret, read_only=False)
                    resources.callback(writer.close)
                    mcp_writers[identity] = writer
        pull = web + f"/pull/{number}"
        normal = api.json("POST", rest + f"/issues/{number}/comments", {"body": "normal issue comment"})
        result["normal_comment_id"] = normal["id"]
        for label, suffix in [("review_rest", f"/pulls/{number}/comments"), ("commit_rest", f"/commits/{head}/comments")]:
            request(label, api, "GET", rest + suffix)

        def post(label, *, commit=head, file=None, old=None, new=None, issue=True, body=None, client=owner_web):
            form = {"content": body or label}
            for name, value in [("fileName", file), ("oldLineNumber", old), ("newLineNumber", new), ("issueId", number if issue else None)]:
                if value is not None:
                    form[name] = value
            # A minimal valid UI diff snapshot. It is display data supplied by the
            # client, not a server validation of the selected position.
            if file is not None:
                form["diff"] = json.dumps([{"oldLine": str(old or ""), "newLine": str(new or ""),
                                           "type": "delete" if old else "insert", "text": "probe line"}])
            status, response = request(label, client, "POST", web + f"/commit/{commit}/comment/_data/new", form=form, ajax=True)
            assert status == 200, (label, status)
            nodes = comment_nodes(response)
            assert len(nodes) == 1, label
            cid = comment_number(nodes[0])
            result["cases"][label] = {"id": cid, "commit": commit, **form}
            (output / (label + ".html")).write_text(nodes[0].render(), encoding="utf-8")
            return cid

        whole = post("commit_whole", commit=first_sha, issue=False)
        post("file_whole", file="review.txt")
        raw = "    indented code\n\n日本語 & <tag> **bold**  \n"
        line = post("new_line", file="review.txt", new=2, body=raw)
        post("same_line_second", file="review.txt", new=2)
        post("old_line", file="review.txt", old=2)
        post("deleted_line", file="deleted.txt", old=1)
        post("renamed_line", file="renamed_日本語.txt", new=7)
        post("special_path", file='quoted & name.txt', new=1)
        post("past_commit_line", commit=first_sha, file="review.txt", new=2)
        post("guest_comment", file="review.txt", new=3, client=guest_web)
        # These are deliberately invalid requests in the disposable repository.
        # Observe the upstream validation gap so future adapters cannot trust it.
        post("invalid_position", file="not-in-diff.txt", new=99999)
        for identity, client in [("owner", owner_web), ("guest", guest_web)]:
            status, body = request(identity + "_raw", client, "GET", web + f"/commit_comments/_data/{line}?dataType=html", ajax=True)
            fields = [n.text() for n in Parser(body).root.nodes() if n.tag == "textarea" and n.attrs.get("name") == "content"]
            result[identity + "_raw"] = {"status": status, "textarea_values": fields}
        form_status, form_body = request("new_line_form", guest_web, "GET", web + f"/commit/{head}/comment/_form?" + urlencode({"fileName": "review.txt", "newLineNumber": 2, "issueId": number}), ajax=True)
        result["new_line_form"] = {"status": form_status, "fields": [n.attrs for n in Parser(form_body).root.nodes() if n.tag == "input" and n.attrs.get("name") in {"fileName", "issueId", "oldLineNumber", "newLineNumber"}]}
        if verify_posts:
            ordinary_before = [c["id"] for c in api.json("GET", rest + f"/issues/{number}/comments")]
            args = {"owner":user,"repo":repo,"pull_number":number,"commit":head,"path":"review.txt",
                    "body":"P10 body\n\n    indented code\nhard break  \nend","head_guard":"preflight"}
            created = []
            for identity, position in [("owner",{"new_line":2}), ("guest",{"old_line":2}),
                                       ("owner",{"new_line":1,"path":'新規 & "quoted".txt'})]:
                submitted = {**args, **position}
                actual = mcp_writers[identity].call("add_pull_request_review_comment", submitted)
                comment = actual["comment"]
                assert comment["body_raw"] == args["body"] and comment["body_format"] == "markdown"
                assert comment["path"] == submitted["path"] and comment["current_commit"] == head
                assert comment["observed_old_line"] == position.get("old_line")
                assert comment["observed_new_line"] == position.get("new_line")
                assert comment["position_verified"] is True
                verification = actual["verification"]
                assert verification["raw_body_verified"] and verification["commit_page_verified"] and verification["pr_page_membership_verified"]
                assert not verification["atomic_head_guard"] and not verification["stored_pr_association_verified"]
                created.append(comment["storage_id"])
            status, visible = guest_web.request("GET", pull)
            assert status == 200
            ids_before = {c["id"] for c in describe(visible)["comments"]}
            for invalid in [
                {**args,"new_line":2,"head_guard":"atomic"},
                {**args,"new_line":2,"body":" leading"},
                {**args,"new_line":2,"path":" review.txt"},
                {**args,"old_line":2,"new_line":2},
                {**args,"new_line":0},
                {**args,"new_line":2,"commit":base_sha},
                {**args,"new_line":1},
                {**args,"new_line":2,"path":"missing.txt"},
            ]:
                reply = mcp_writers["owner"].rpc("tools/call", {"name":"add_pull_request_review_comment","arguments":invalid})
                assert reply["isError"] is True
            blocked = mcp_clients["owner"].rpc("tools/call", {"name":"add_pull_request_review_comment","arguments":{**args,"new_line":2}})
            assert blocked["structuredContent"]["kind"] == "read_only_error"
            status, visible = guest_web.request("GET", pull)
            assert status == 200 and {c["id"] for c in describe(visible)["comments"]} == ids_before
            assert [c["id"] for c in api.json("GET", rest + f"/issues/{number}/comments")] == ordinary_before
            result["mcp_post_checks"] = {"created_ids":created,"positive_cases":3,"negative_cases":8,"readonly_blocked":True,"ordinary_comment_ids_unchanged":True}
        before = snapshot("conversation_before", pull)
        snapshot("files_before", pull + "/files")
        snapshot("commit_first_before", web + "/commit/" + first_sha)
        result["pr_commits"] = [c["sha"] for c in api.json("GET", rest + f"/pulls/{number}/commits")]
        result["issue_rest_ids"] = [c["id"] for c in api.json("GET", rest + f"/issues/{number}/comments")]
        assert line in [c["id"] for c in describe(before)["comments"]]
        result["whole_commit_visible_before"] = whole in [c["id"] for c in describe(before)["comments"]]

        (checkout / "review.txt").write_text("inserted\nalpha\nnew value\nomega\n")
        git("add", ".")
        git("commit", "-m", "Move reviewed lines")
        git("push", "origin", branch)
        next_head = git("rev-parse", "HEAD")
        aliases[next_head] = "HEAD2"
        snapshot("conversation_after_push", pull)
        snapshot("files_after_push", pull + "/files")
        snapshot("historical_commit", web + "/commit/" + first_sha)

        # Replace the PR history without deleting fixture refs or another user's
        # data; the lease pins the branch value written by this harness.
        git("checkout", "-b", "replacement", base_sha)
        (checkout / "replacement.txt").write_text("replacement history\n")
        git("add", ".")
        git("commit", "-m", "Replace PR history")
        replacement = git("rev-parse", "HEAD")
        aliases[replacement] = "HEAD3"
        git("push", f"--force-with-lease=refs/heads/{branch}:{next_head}", "origin", f"HEAD:refs/heads/{branch}")
        snapshot("conversation_after_rewrite", pull)
        snapshot("files_after_rewrite", pull + "/files")
        snapshot("historical_after_rewrite", web + "/commit/" + first_sha)
        # Merge the replacement branch to check post-merge visibility as well.
        merged = api.json("PUT", rest + f"/pulls/{number}/merge", {"commit_message": "Merge inline probe"})
        result["merged"] = merged["merged"]
        assert result["merged"] is True
        snapshot("conversation_after_merge", pull)
        snapshot("files_after_merge", pull + "/files")

    # Never persist headers/cookies/passwords/full pages. Normalize only this
    # fixture's identities and object IDs, retaining comment IDs and exact text.
    def normalize(value):
        for original, replacement in sorted(aliases.items(), key=lambda pair: -len(pair[0])):
            value = value.replace(original, replacement)
        return value

    for path in output.glob("*.html"):
        path.write_text(normalize(path.read_text(encoding="utf-8")), encoding="utf-8")
    (output / "observations.json").write_text(normalize(json.dumps(result, ensure_ascii=False, indent=2)) + "\n", encoding="utf-8")
    print(f"GitBucket {version}: inline probe completed; {len(result['cases'])} comment cases")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow-disposable-writes", action="store_true", required=True)
    parser.add_argument("--version-label", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--mcp-binary", type=Path, help="Also verify owner/GUEST review reads through this built server's stdio MCP tools")
    parser.add_argument("--verify-posts", action="store_true", help="Also create verified MCP comments and test rejection paths in the disposable repository")
    args = parser.parse_args()
    os.umask(0o077)
    run(args.output, args.version_label, args.mcp_binary, args.verify_posts)


if __name__ == "__main__":
    main()
