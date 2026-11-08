import contextlib
import io
import sys
import unittest
from unittest.mock import patch

from probe_merges import main, options_form


class MergeProbeTests(unittest.TestCase):
    def test_options_form_preserves_content_checked_flags_and_selected_values(self):
        fields = options_form('''<form action="/other"><input name="secret" value="ignore"></form>
        <form action="/owner/repo/settings/options" method="post">
          <textarea name="description">日本語  \n    code</textarea>
          <input type="checkbox" name="isPrivate" value="true" checked>
          <input type="checkbox" name="allowFork" value="true">
          <input name="nonce" value="opaque">
          <input type="checkbox" name="mergeOptions" value="merge-commit" checked>
          <input type="checkbox" name="mergeOptions" value="squash" checked>
          <select name="defaultMergeOption"><option value="merge-commit">merge</option><option value="squash" selected>squash</option></select>
        </form>''', "/owner/repo/settings/options")
        self.assertIn(("description", "日本語  \n    code"), fields)
        self.assertIn(("isPrivate", "true"), fields)
        self.assertNotIn(("allowFork", "true"), fields)
        self.assertIn(("defaultMergeOption", "squash"), fields)
        self.assertIn(("nonce", "opaque"), fields)
        self.assertEqual([v for k, v in fields if k == "mergeOptions"], ["merge-commit", "squash"])
        self.assertNotIn("secret", dict(fields))

    def test_unknown_ambiguous_or_incomplete_settings_forms_fail_before_writing(self):
        form = '<form action="/owner/repo/settings/options"><input name="mergeOptions" value="squash" checked type="checkbox"><input name="defaultMergeOption" value="squash"></form>'
        for page in ["", form + form, '<form action="/owner/repo/settings/options"></form>', form.replace("/owner/repo", "/other/repo")]:
            with self.subTest(page=page), self.assertRaises(ValueError):
                options_form(page, "/owner/repo/settings/options")

    def test_explicit_disposable_write_flag_is_required(self):
        with patch.object(sys, "argv", ["probe_merges.py", "--mcp-binary", "/missing", "--output", "/tmp/unused", "--version-label", "test"]):
            with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as caught:
                main()
        self.assertEqual(caught.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
