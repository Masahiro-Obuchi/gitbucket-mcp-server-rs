#!/usr/bin/env python3
"""Acceptance infrastructure failures must not expose private child output."""
import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import acceptance
import ci_plan


class SelectionTests(unittest.TestCase):
    def test_documentation_only_skips_live_but_shared_or_unknown_code_runs_all(self):
        self.assertEqual(ci_plan.select_domains(["README.md", "docs/list-continuation.md"]), [])
        for path in ["src/server.rs", "src/models/pull_request.rs", "src/api/new.rs", "Cargo.lock",
                     ".github/workflows/ci.yml", "scripts/e2e/test_acceptance.py", "docker/e2e/compose.yaml", ""]:
            self.assertEqual(ci_plan.select_domains(["README.md", path]), list(ci_plan.DOMAINS), path)

    def test_domains_union_in_fixed_order_and_shared_pr_surface_is_not_under_selected(self):
        self.assertEqual(ci_plan.select_domains(["src/api/review_post.rs", "src/api/pull_request_merge.rs"]), ["review", "merge"])
        self.assertEqual(ci_plan.select_domains(["src/config.rs", "src/api/review_html.rs", "tests/list_page_test.rs"]),
                         ["core", "paging", "review", "startup"])
        self.assertEqual(ci_plan.select_domains(["src/tools/pull_request.rs"]), ["core", "paging", "review", "merge"])
        self.assertEqual(ci_plan.select_domains(["src/api/milestone_write.rs"]), ["core", "paging"])
        self.assertEqual(ci_plan.select_domains(["src/tools/list_summary.rs"]), ["paging", "review"])

    def test_git_paths_preserve_newlines_spaces_and_deleted_rename_sources(self):
        with tempfile.TemporaryDirectory() as directory:
            def git(*args):
                return subprocess.check_output(["git", *args], cwd=directory, stderr=subprocess.DEVNULL).decode().strip()
            git("init")
            git("config", "user.name", "Test")
            git("config", "user.email", "test@example.test")
            old = Path(directory) / "source.rs"
            old.write_text("source")
            git("add", ".")
            git("commit", "-m", "First")
            base = git("rev-parse", "HEAD")
            old.rename(Path(directory) / "name with\nnewline.md")
            git("add", ".")
            git("commit", "-m", "Rename")
            head = git("rev-parse", "HEAD")
            previous = os.getcwd()
            try:
                os.chdir(directory)
                paths = ci_plan.changed_paths(base, head)
            finally:
                os.chdir(previous)
            self.assertEqual(set(paths), {"source.rs", "name with\nnewline.md"})
            self.assertEqual(ci_plan.select_domains(paths), list(ci_plan.DOMAINS))
        with self.assertRaises(ValueError):
            ci_plan.changed_paths("--help", "a" * 40)


class AcceptanceTests(unittest.TestCase):
    def test_private_child_error_and_timeout_do_not_escape(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "child.log"
            with self.assertRaises(acceptance.StageFailed) as error:
                acceptance.execute([sys.executable, "-c", "print('private-token'); raise ValueError('private-password')"], os.environ, log)
            self.assertIn("private-password", log.read_text())
            self.assertNotIn("private-", str(error.exception))
            with self.assertRaises(subprocess.TimeoutExpired):
                acceptance.execute([sys.executable, "-c", "import time; time.sleep(60)"], os.environ, log, timeout=0.05)

    def fake_run(self, failed=None, empty_suite=False):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        base = Path(temporary.name)
        calls = []
        captured_env = []

        def execute(args, env, log, timeout=900):
            stage = log.stem
            calls.append(stage)
            captured_env.append(dict(env))
            log.write_text("private-token private-password http://private.example.test\n")
            if stage in {"core", "paging", "core-and-paging"}:
                with log.open("a") as stream:
                    stream.write("\n".join(name + " ... ok" for name in acceptance.PAGING_TESTS))
                    stream.write(f"\ntest result: ok. {0 if empty_suite else 23} passed; 0 failed; 0 ignored;\n")
            if stage == "bootstrap":
                root = Path(env["GITBUCKET_E2E_ROOT"])
                root.mkdir(mode=0o700)
            if stage == "cleanup":
                root = Path(env["GITBUCKET_E2E_ROOT"])
                if root.exists():
                    root.rmdir()
            if stage == failed:
                raise RuntimeError("private-token private-password http://private.example.test")

        private_paths = []
        original = acceptance.tempfile.TemporaryDirectory

        def scratch(*args, **kwargs):
            temp = original(*args, **kwargs)
            private_paths.append(Path(temp.name))
            return temp

        buffer = io.StringIO()
        runtime = {"GITBUCKET_E2E_TOKEN": "private-token", "GITBUCKET_E2E_WEB_PASSWORD": "private-password"}
        with patch.object(acceptance, "execute", side_effect=execute), \
                patch.object(acceptance, "read_runtime", return_value=runtime), \
                patch.object(acceptance, "image_identity", return_value={"id": "sha256:" + "a" * 64, "digests": []}), \
                patch.object(acceptance, "diagnostics"), \
                patch.object(acceptance.tempfile, "TemporaryDirectory", side_effect=scratch), \
                patch.dict(os.environ, {"GITBUCKET_URL": "http://production.invalid", "GITBUCKET_E2E_IMAGE": "custom-image", "GITBUCKET_TOKEN": "deployment-token"}), \
                contextlib.redirect_stdout(buffer):
            passed = acceptance.run("4.48.0", list(ci_plan.DOMAINS), base / "output", base / "runtime", base / "target", 18880, "acceptance-test")
        summary = (base / "output/summary.json").read_text()
        for secret in ("private-token", "private-password", "http://private.example.test", "production.invalid", "deployment-token"):
            self.assertNotIn(secret, summary + buffer.getvalue())
        self.assertEqual(calls[-1], "cleanup")
        self.assertFalse((base / "runtime").exists())
        self.assertTrue(all(not p.exists() for p in private_paths))
        self.assertTrue(all(e["GITBUCKET_E2E_IMAGE"] == "ghcr.io/gitbucket/gitbucket:4.48.0" for e in captured_env))
        self.assertTrue(all("GITBUCKET_TOKEN" not in e and "GITBUCKET_URL" not in e for e in captured_env))
        return passed, json.loads(summary), calls

    def test_full_success_and_failures_always_cleanup_and_publish_only_allowlisted_summary(self):
        passed, report, calls = self.fake_run()
        self.assertTrue(passed)
        self.assertEqual(calls, ["build", "bootstrap", "core-and-paging", "review", "merge", "cleanup"])
        self.assertTrue(all(s["passed"] for s in report["stages"]))
        for failed in ["build", "bootstrap", "core-and-paging", "review", "merge"]:
            with self.subTest(failed=failed):
                passed, report, calls = self.fake_run(failed)
                self.assertFalse(passed)
                self.assertEqual(report["failed_stage"], failed)
                self.assertNotIn("cleanup_failed", report)
                self.assertEqual(set(report), {"schema", "version", "domains", "passed", "stages", "failed_stage", "source",
                                              "transport", "context_path", "plugins"}
                                 | ({"image"} if failed not in {"build", "bootstrap"} else set()))

    def test_zero_executed_tests_cannot_pass_acceptance(self):
        passed, report, calls = self.fake_run(empty_suite=True)
        self.assertFalse(passed)
        self.assertEqual(report["failed_stage"], "core-and-paging")
        self.assertNotIn("review", calls)
        self.assertFalse(next(s for s in report["stages"] if s["name"] == "core-and-paging")["passed"])

    def test_image_metadata_is_validated_before_publication(self):
        good = {"Id": "sha256:" + "a" * 64, "RepoDigests": ["ghcr.io/gitbucket/gitbucket@sha256:" + "b" * 64,
                                                             "http://private.example.test/private-token"]}
        env = {"GITBUCKET_E2E_IMAGE": "ghcr.io/gitbucket/gitbucket:4.48.0"}
        with patch.object(acceptance.subprocess, "check_output", return_value=json.dumps([good]).encode()):
            identity = acceptance.image_identity(env)
            self.assertNotIn("private-token", json.dumps(identity))
        for invalid in [{**good, "Id": "private-token"}, {**good, "RepoDigests": ["private-token"]}]:
            with patch.object(acceptance.subprocess, "check_output", return_value=json.dumps([invalid]).encode()):
                with self.assertRaises(acceptance.StageFailed):
                    acceptance.image_identity(env)

    def test_cleanup_failure_fails_acceptance(self):
        passed, report, _ = self.fake_run("cleanup")
        self.assertFalse(passed)
        self.assertTrue(report["cleanup_failed"])

    def test_cli_refuses_non_disposable_paths_bad_domains_and_missing_opt_in_before_commands(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            base = ["acceptance", "--version", "4.48.0", "--output", str(root / "out"), "--runtime-root", str(root / "runtime")]
            cases = [base, base + ["--allow-disposable-writes", "--domains-json", '["unknown"]'],
                     base + ["--allow-disposable-writes", "--runtime-root", str(root)],
                     base + ["--allow-disposable-writes", "--output", str(root / "runtime/out")]]
            for args in cases:
                with patch.object(sys, "argv", args), patch.object(acceptance, "run") as run, contextlib.redirect_stderr(io.StringIO()):
                    with self.assertRaises(SystemExit) as error:
                        acceptance.main()
                    self.assertEqual(error.exception.code, 2)
                    run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
