#!/usr/bin/env python3
"""Select disposable contract domains from NUL-delimited Git paths."""
import argparse
import json
from pathlib import Path
import re
import subprocess

DOMAINS = ("core", "paging", "review", "merge", "startup")
VERSIONS = ("4.44.0", "4.48.0")


def select_domains(paths):
    selected = set()
    for path in paths:
        if path.startswith((".github/", "scripts/", "docker/")) or path in {"Cargo.toml", "Cargo.lock"}:
            return list(DOMAINS)
        if path.endswith(".md") or path.startswith("skills/") or path == "LICENSE":
            continue
        if path in {"src/main.rs", "src/config.rs", "tests/cli_test.rs"} or path.startswith("tests/fixtures/tls/"):
            selected.update(("startup", "core"))
        elif "review" in path or path.startswith("tests/fixtures/inline_comments/"):
            selected.add("review")
        elif path in {"src/api/pull_request_merge.rs", "tests/merge_test.rs"}:
            selected.add("merge")
        elif path in {"src/tools/list_page.rs", "src/tools/list_summary.rs", "tests/list_page_test.rs"}:
            selected.update(("paging", "review"))
        elif path == "tests/list_api_test.rs":
            selected.add("paging")
        elif path == "tests/e2e_test.rs":
            selected.update(("core", "paging"))
        elif path in {"src/tools/pull_request.rs", "src/api/pull_request.rs"}:
            selected.update(("core", "review", "merge", "paging"))
        elif path.startswith(("src/api/issue", "src/api/label", "src/api/milestone", "src/api/repository", "src/api/user",
                              "src/tools/issue", "src/tools/label", "src/tools/milestone", "src/tools/repository", "src/tools/user")):
            selected.update(("core", "paging"))
        else:
            # Shared models, transport, validation, unknown/new code and deletions
            # must not silently evade live coverage.
            return list(DOMAINS)
    return [domain for domain in DOMAINS if domain in selected]


def changed_paths(base, head):
    if not all(re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", ref) for ref in (base, head)):
        raise ValueError("Expected commit IDs")
    result = subprocess.run(["git", "diff", "--no-renames", "--name-only", "-z", base, head, "--"],
                            check=True, capture_output=True)
    return result.stdout.decode("utf-8", errors="replace").rstrip("\0").split("\0")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True)
    parser.add_argument("--head", required=True)
    parser.add_argument("--github-output", type=Path, required=True)
    args = parser.parse_args()
    domains = select_domains(changed_paths(args.base, args.head))
    with args.github_output.open("a", encoding="utf-8") as out:
        out.write("run=" + str(bool(domains)).lower() + "\n")
        out.write("domains=" + json.dumps(domains) + "\n")


if __name__ == "__main__":
    main()
