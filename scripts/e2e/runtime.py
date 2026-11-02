#!/usr/bin/env python3
"""Private runtime files; shell syntax and GitHub environment syntax are distinct."""
import json
import os
from pathlib import Path
import shlex
import stat
import sys
import tempfile
import uuid

KEYS = (
    "GITBUCKET_E2E_URL", "GITBUCKET_E2E_TOKEN", "GITBUCKET_E2E_OWNER",
    "GITBUCKET_E2E_REPO", "GITBUCKET_E2E_GIT_USERNAME", "GITBUCKET_E2E_GIT_PASSWORD",
    "GITBUCKET_E2E_WEB_USERNAME", "GITBUCKET_E2E_WEB_PASSWORD",
)


def runtime_path():
    root = Path(os.environ.get("GITBUCKET_E2E_ROOT", Path(__file__).resolve().parents[2] / ".tmp/e2e"))
    return Path(os.environ.get("GITBUCKET_E2E_ENV_FILE", root / "runtime.env"))


def atomic_private(path, content):
    if path.is_symlink() or (path.exists() and not path.is_file()):
        raise ValueError("Runtime destination must be a regular file")
    fd, name = tempfile.mkstemp(prefix=".runtime-", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            os.fchmod(stream.fileno(), 0o600)
            stream.write(content)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, path)
    finally:
        Path(name).unlink(missing_ok=True)


def read_runtime(path):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, encoding="utf-8") as stream:
        meta = os.fstat(stream.fileno())
        if not stat.S_ISREG(meta.st_mode) or meta.st_uid != os.getuid() or meta.st_mode & 0o077:
            raise ValueError("Runtime JSON must be owned by the current user with mode 0600")
        data = json.load(stream)
    if set(data) != set(KEYS) or any(not isinstance(v, str) or "\0" in v for v in data.values()):
        raise ValueError("Invalid runtime fields")
    return data


def write_runtime(token):
    env = os.environ
    data = dict(zip(KEYS, (
        env["GITBUCKET_E2E_BASE_URL"], token, env["GITBUCKET_E2E_REPO_OWNER"],
        env["GITBUCKET_E2E_REPO_NAME"], env["GITBUCKET_E2E_GIT_USERNAME"],
        env["GITBUCKET_E2E_GIT_PASSWORD"], env["GITBUCKET_E2E_GIT_USERNAME"],
        env["GITBUCKET_E2E_GIT_PASSWORD"],
    )))
    if any("\0" in v for v in data.values()):
        raise ValueError("NUL is not supported in environment variables")
    path = runtime_path()
    # Both representations use atomic private writes. JSON is used for reuse/CI;
    # bootstrap never executes an existing runtime shell file.
    atomic_private(path, "".join(f"export {key}={shlex.quote(value)}\n" for key, value in data.items()))
    atomic_private(Path(str(path) + ".json"), json.dumps(data))


def main():
    command = sys.argv[1]
    if command == "write":
        write_runtime(sys.stdin.read())
    else:
        data = read_runtime(Path(str(runtime_path()) + ".json"))
        if command == "token":
            sys.stdout.write(data["GITBUCKET_E2E_TOKEN"])
        elif command == "github":
            with open(sys.argv[2], "a", encoding="utf-8") as stream:
                for key, value in data.items():
                    delimiter = "GB_MCP_" + uuid.uuid4().hex
                    while delimiter in value.splitlines():
                        delimiter = "GB_MCP_" + uuid.uuid4().hex
                    stream.write(f"{key}<<{delimiter}\n{value}\n{delimiter}\n")
        else:
            raise ValueError("Unknown command")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, IndexError):
        # Exception strings may contain a secret-bearing input line or path.
        sys.exit("Failed to process private E2E runtime files; check paths, permissions and fields")
