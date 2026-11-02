#!/usr/bin/env python3
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SCRIPT_DIR = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("runtime", SCRIPT_DIR / "runtime.py")
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.path = self.root / "runtime.env"
        self.password = "space '\" \\ $VALUE `false` $(false)\nsecond line"
        self.env = dict(os.environ, GITBUCKET_E2E_ROOT=str(self.root),
                        GITBUCKET_E2E_ENV_FILE=str(self.path),
                        GITBUCKET_E2E_BASE_URL="http://127.0.0.1:18080",
                        GITBUCKET_E2E_REPO_OWNER="test-user", GITBUCKET_E2E_REPO_NAME="test.repo",
                        GITBUCKET_E2E_GIT_USERNAME="test-user", GITBUCKET_E2E_GIT_PASSWORD=self.password)

    def write(self):
        with patch.dict(os.environ, self.env):
            runtime.write_runtime("fake-token-$`'\\\"\nsecond line")
        return runtime.read_runtime(Path(str(self.path) + ".json"))

    def test_private_shell_round_trip_and_existing_mode(self):
        self.path.write_text("old")
        self.path.chmod(0o644)
        data = self.write()
        result = subprocess.run(["bash", "-c", 'source "$1"; python3 -c \'import os,json; print(json.dumps({k:v for k,v in os.environ.items() if k.startswith("GITBUCKET_E2E_")}))\'', "test", str(self.path)],
                                env={}, check=True, capture_output=True, text=True)
        self.assertEqual(json.loads(result.stdout), data)
        for path in (self.path, Path(str(self.path) + ".json")):
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)

    def test_github_format_round_trip(self):
        data = self.write()
        output = self.root / "github-env"
        subprocess.run(["python3", str(SCRIPT_DIR / "runtime.py"), "github", str(output)], env=self.env, check=True)
        lines = iter(output.read_text().splitlines())
        parsed = {}
        for line in lines:
            key, delimiter = line.split("<<")
            value = []
            for line in lines:
                if line == delimiter:
                    break
                value.append(line)
            parsed[key] = "\n".join(value)
        self.assertEqual(parsed, data)

    def test_symlink_and_failure_preserve_target_and_clean_temp(self):
        target = self.root / "target"
        target.write_text("unchanged")
        self.path.symlink_to(target)
        with self.assertRaises(ValueError):
            self.write()
        self.assertEqual(target.read_text(), "unchanged")
        self.path.unlink()
        with patch.object(runtime.os, "replace", side_effect=OSError("failure")):
            with self.assertRaises(OSError):
                self.write()
        self.assertEqual(sorted(p.name for p in self.root.iterdir()), ["target"])

    def test_reader_refuses_public_file_and_symlink(self):
        self.write()
        path = Path(str(self.path) + ".json")
        path.chmod(0o644)
        with self.assertRaises(ValueError):
            runtime.read_runtime(path)
        link = self.root / "link"
        link.symlink_to(path)
        with self.assertRaises(OSError):
            runtime.read_runtime(link)

    def test_runtime_directory_is_private_and_symlink_refused(self):
        root = self.root / "runtime"
        root.mkdir(mode=0o755)
        env = dict(self.env, GITBUCKET_E2E_ROOT=str(root))
        command = ['bash', '-c', 'source "$1"; ensure_runtime_dir', 'test', str(SCRIPT_DIR / 'common.sh')]
        subprocess.run(command, env=env, check=True)
        self.assertEqual(stat.S_IMODE(root.stat().st_mode), 0o700)
        link = self.root / "link"
        link.symlink_to(root)
        result = subprocess.run(command, env=dict(env, GITBUCKET_E2E_ROOT=str(link)), capture_output=True)
        self.assertNotEqual(result.returncode, 0)

    def test_failed_bootstrap_cleans_private_scratch_files(self):
        bindir = self.root / "bin"
        bindir.mkdir()
        docker = bindir / "docker"
        docker.write_text("#!/bin/bash\nexit 0\n")
        docker.chmod(0o700)
        curl = bindir / "curl"
        curl.write_text("""#!/usr/bin/env python3
import pathlib, sys
args = sys.argv[1:]
if '-w' in args:
    pathlib.Path(args[args.index('-o')+1]).write_text('fake-secret-error-body')
    print('500', end='')
""")
        curl.chmod(0o700)
        root = self.root / "runtime"
        result = subprocess.run(["bash", str(SCRIPT_DIR / "bootstrap.sh")],
                                env=dict(self.env, PATH=str(bindir)+os.pathsep+os.environ["PATH"], GITBUCKET_E2E_ROOT=str(root)),
                                text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('fake-secret-error-body', result.stderr)
        self.assertEqual(list(root.iterdir()), [])
        self.assertEqual(stat.S_IMODE(root.stat().st_mode), 0o700)


if __name__ == "__main__":
    unittest.main()
