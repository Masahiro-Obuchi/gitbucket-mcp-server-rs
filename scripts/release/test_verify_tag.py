import os
from pathlib import Path
import subprocess
import tempfile
import unittest


VERIFY_TAG = Path(__file__).with_name("verify_tag.sh").resolve()
PUBLISH_DRAFT = VERIFY_TAG.with_name("publish_draft.sh")
TAG = "refs/tags/v0.3.0"


class VerifyReleaseTag(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.remote = self.root / "origin.git"
        self.source = self.root / "source"
        self.checkout = self.root / "checkout"
        self.env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        self.env.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")

        self.git("init", "-q", "--bare", "-b", "main", str(self.remote), cwd=self.root)
        self.git("init", "-q", "-b", "main", str(self.source), cwd=self.root)
        self.git("config", "user.name", "Release Fixture")
        self.git("config", "user.email", "release@example.test")
        self.git("remote", "add", "origin", self.remote.as_uri())
        self.git("commit", "-q", "--allow-empty", "-m", "Release")
        self.release_commit = self.git("rev-parse", "HEAD")
        self.git("tag", "-a", "v0.3.0", "-m", "Release v0.3.0")
        self.git("push", "-q", "origin", "main", TAG)
        self.git("clone", "-q", self.remote.as_uri(), str(self.checkout), cwd=self.root)
        self.git("checkout", "-q", "--detach", self.release_commit, cwd=self.checkout)

    def git(self, *args, cwd=None):
        return subprocess.run(
            ("git", *args), cwd=cwd or self.source, env=self.env,
            check=True, text=True, capture_output=True,
        ).stdout.strip()

    def verify(self, ref=TAG, version="0.3.0"):
        return subprocess.run(
            ("bash", str(VERIFY_TAG), ref, version), cwd=self.checkout,
            env=self.env, text=True, capture_output=True,
        )

    def publish(self, draft="v0.3.0\ttrue"):
        bin_dir = self.root / "bin"
        bin_dir.mkdir(exist_ok=True)
        gh = bin_dir / "gh"
        gh.write_text(
            '#!/usr/bin/env bash\n'
            'set -eu\n'
            'printf "%s\\n" "$*" >> "$RELEASE_FIXTURE_CALLS"\n'
            'if [[ "$2" != --method ]]; then\n'
            '  printf "%s\\n" "$RELEASE_FIXTURE_DRAFT"\n'
            'fi\n'
        )
        gh.chmod(0o700)
        calls = self.root / "api-calls"
        calls.write_text("")
        env = dict(
            self.env,
            PATH=str(bin_dir) + os.pathsep + self.env.get("PATH", os.defpath),
            GITHUB_REPOSITORY="fixture/project",
            RELEASE_FIXTURE_CALLS=str(calls),
            RELEASE_FIXTURE_DRAFT=draft,
        )
        result = subprocess.run(
            ("bash", str(PUBLISH_DRAFT), TAG, "0.3.0", "123"), cwd=self.checkout,
            env=env, text=True, capture_output=True,
        )
        return result, calls.read_text().splitlines()

    def assert_rejected(self, result, message):
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(message, result.stderr)

    def test_accepts_an_annotated_tag_after_remote_main_advances(self):
        self.git("commit", "-q", "--allow-empty", "-m", "Advance main")
        self.git("push", "-q", "origin", "main")

        result = self.verify()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.git("rev-parse", "refs/remotes/origin/main", cwd=self.checkout),
            self.git("rev-parse", "HEAD"),
        )

    def test_rejects_a_tag_on_an_unmerged_branch(self):
        self.git("switch", "-q", "-c", "topic")
        self.git("commit", "-q", "--allow-empty", "-m", "Work in progress")
        self.git("tag", "-f", "v0.3.0", "HEAD")
        self.git("push", "-q", "origin", "topic", "+" + TAG)
        self.git("fetch", "-q", "origin", "refs/heads/topic", cwd=self.checkout)
        self.git("checkout", "-q", "--detach", "FETCH_HEAD", cwd=self.checkout)

        self.assert_rejected(self.verify(), "outside origin/main history")

    def test_rejects_a_tag_moved_to_another_main_commit(self):
        self.git("commit", "-q", "--allow-empty", "-m", "Different main commit")
        self.git("tag", "-f", "v0.3.0", "HEAD")
        self.git("push", "-q", "origin", "main", "+" + TAG)

        self.assertEqual(self.git("rev-parse", "HEAD", cwd=self.checkout), self.release_commit)
        self.assert_rejected(self.verify(), "does not point to the checked-out release commit")

    def test_rejects_a_deleted_remote_tag_even_with_the_tag_present_locally(self):
        self.git("push", "-q", "origin", ":" + TAG)

        self.assertEqual(
            self.git("rev-parse", TAG + "^{commit}", cwd=self.checkout), self.release_commit,
        )
        self.assert_rejected(self.verify(), "Could not fetch the release tag and main")

    def test_recheck_rejects_a_release_commit_removed_from_main_history(self):
        first = self.verify()
        self.assertEqual(first.returncode, 0, first.stderr)
        self.git("switch", "-q", "--orphan", "replacement")
        self.git("commit", "-q", "--allow-empty", "-m", "Replace main history")
        self.git("push", "-q", "origin", "+HEAD:refs/heads/main")

        self.assert_rejected(self.verify(), "outside origin/main history")

    def test_rejects_a_branch_or_a_version_mismatch(self):
        for ref, version in (("refs/heads/main", "0.3.0"), (TAG, "0.3.1")):
            with self.subTest(ref=ref, version=version):
                self.assert_rejected(self.verify(ref, version), "tag matching the package version")

    def test_publishes_the_verified_draft_by_id_with_the_checked_out_target(self):
        self.git("commit", "-q", "--allow-empty", "-m", "Advance main")
        self.git("push", "-q", "origin", "main")

        result, calls = self.publish()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, [
            "api repos/fixture/project/releases/123 --jq [.tag_name, .draft] | @tsv",
            "api --method PATCH repos/fixture/project/releases/123 -F draft=false "
            f"-f tag_name=v0.3.0 -f target_commitish={self.release_commit} --jq .html_url",
        ])

    def test_refuses_publication_if_the_tag_moves_or_disappears_after_staging(self):
        staged = self.verify()
        self.assertEqual(staged.returncode, 0, staged.stderr)
        self.git("commit", "-q", "--allow-empty", "-m", "Different main commit")
        self.git("tag", "-f", "v0.3.0", "HEAD")
        self.git("push", "-q", "origin", "main", "+" + TAG)

        result, calls = self.publish()
        self.assert_rejected(result, "does not point to the checked-out release commit")
        self.assertEqual(len(calls), 1, calls)

        self.git("push", "-q", "origin", ":" + TAG)
        result, calls = self.publish()
        self.assert_rejected(result, "Could not fetch the release tag and main")
        self.assertEqual(len(calls), 1, calls)

    def test_refuses_to_publish_the_wrong_draft_or_an_already_published_release(self):
        for draft in ("v0.3.1\ttrue", "v0.3.0\tfalse"):
            with self.subTest(draft=draft):
                result, calls = self.publish(draft)
                self.assert_rejected(result, "not a draft for the expected tag")
                self.assertEqual(len(calls), 1, calls)


if __name__ == "__main__":
    unittest.main()
