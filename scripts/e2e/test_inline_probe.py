"""Offline checks for the evidence used by the P08 design."""

import json
from pathlib import Path
import unittest

from probe_inline_comments import comment_nodes, decode_json, describe, loopback_url


FIXTURES = Path(__file__).resolve().parents[2] / "tests/fixtures/inline_comments"
VERSIONS = ("4.44.0", "4.48.0")


class InlineProbeTests(unittest.TestCase):
    def observations(self, version):
        return json.loads((FIXTURES / version / "observations.json").read_text())

    def test_probe_target_cannot_be_remote_or_secret_bearing(self):
        for value in ["https://gitbucket.example", "http://127.0.0.1.evil.test",
                      "http://user:private@localhost", "http://localhost/?token=private",
                      "http://localhost/context", "http://localhost/#private"]:
            with self.subTest(value=value), self.assertRaises(ValueError) as error:
                loopback_url(value)
            self.assertNotIn("private", str(error.exception))
        self.assertEqual(loopback_url("http://127.0.0.1:18204/"), "http://127.0.0.1:18204")
        self.assertEqual(loopback_url("http://[::1]:18204"), "http://[::1]:18204")

    def test_upstream_json_encodings_and_wrapped_rejections(self):
        value = {"merged": True}
        for body in [json.dumps(value), json.dumps(json.dumps(value)),
                     json.dumps({"status": 200, "body": json.dumps(value)})]:
            self.assertEqual(decode_json(body), value)
        self.assertEqual(decode_json('{"body":"ordinary Issue body"}'), {"body": "ordinary Issue body"})
        with self.assertRaisesRegex(RuntimeError, "409"):
            decode_json('{"status":409,"body":"private error body"}')

    def test_captured_dom_keeps_whole_file_and_line_cards(self):
        for version in VERSIONS:
            with self.subTest(version=version):
                page = (FIXTURES / version / "conversation_before.html").read_text()
                actual = describe(page)["comments"]
                expected = self.observations(version)["snapshots"]["conversation_before"]["comments"]
                self.assertEqual(actual, expected)
                self.assertEqual({c["id"] for c in actual}, set(range(1, 12)))
                self.assertEqual({c["kind"] for c in actual}, {"commit", "file", "line"})
                whole = next(n for n in comment_nodes(page) if "commit-comment-1" in n.attrs.get("class", "").split())
                self.assertNotIn("id", whole.attrs)
                self.assertEqual(next(c for c in actual if c["id"] == 7)["path"], "renamed_日本語.txt")
                special = describe((FIXTURES / version / "special_path.html").read_text())["comments"]
                self.assertEqual(special[0]["path"], "quoted & name.txt")

    def test_captured_raw_content_and_guest_authorization_limits(self):
        for version in VERSIONS:
            with self.subTest(version=version):
                data = self.observations(version)
                self.assertEqual(data["reader_permission"]["permission"], "GUEST")
                self.assertEqual(data["owner_raw"]["status"], 200)
                self.assertEqual(data["guest_raw"], {"status": 401, "textarea_values": []})
                requested = data["cases"]["new_line"]["content"]
                saved = data["owner_raw"]["textarea_values"][0]
                self.assertNotEqual(requested, saved)
                self.assertEqual(requested.strip(), saved)
                rendered = describe((FIXTURES / version / "new_line.html").read_text())["comments"][0]["body_html"]
                self.assertIn("<strong>bold</strong>", rendered)
                self.assertNotIn("**bold**", rendered)
                self.assertEqual(data["issue_rest_ids"], [data["normal_comment_id"]])
                statuses = {r["label"]: r["status"] for r in data["requests"]}
                self.assertEqual(statuses["review_rest"], 404)
                self.assertEqual(statuses["commit_rest"], 404)
                self.assertEqual(statuses["guest_comment"], 200)
                self.assertEqual(statuses["invalid_position"], 200)

    def test_captured_head_changes_show_partial_history_and_folded_cards(self):
        for version in VERSIONS:
            with self.subTest(version=version):
                data = self.observations(version)
                snapshots = data["snapshots"]
                before = snapshots["conversation_before"]
                self.assertFalse(before["normal_comment_text_present"])
                self.assertTrue(before["whole_commit_text_present"])
                moved = snapshots["conversation_after_push"]
                self.assertIn("fold-comments-2", moved["folded_groups"])
                self.assertEqual(next(c for c in moved["comments"] if c["id"] == 3)["new_line"], "3")
                retained = {5, 6, 7, 8, 10, 11}
                for label in ["conversation_after_rewrite", "files_after_rewrite",
                              "conversation_after_merge", "files_after_merge"]:
                    self.assertEqual({c["id"] for c in snapshots[label]["comments"]}, retained)
                self.assertEqual({c["id"] for c in snapshots["historical_after_rewrite"]["comments"]}, {1, 9})
                self.assertTrue(data["merged"])

    def test_source_and_image_provenance_is_present_for_both_versions(self):
        sources = json.loads((FIXTURES / "sources.json").read_text())
        self.assertGreaterEqual(len(sources), 10)
        for source in sources:
            for version in VERSIONS:
                evidence = source["versions"][version]
                self.assertRegex(evidence["blob"], r"^[a-f0-9]{40}$")
                self.assertEqual(evidence["url"], f'https://github.com/gitbucket/gitbucket/blob/{version}/{source["path"]}')
        for version in VERSIONS:
            environment = json.loads((FIXTURES / version / "environment.json").read_text())
            self.assertEqual(environment["image"], "ghcr.io/gitbucket/gitbucket:" + version)
            self.assertTrue(environment["repo_digests"])


if __name__ == "__main__":
    unittest.main()
