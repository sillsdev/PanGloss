import json
import subprocess
import sys
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("release_id.py")


class ReleaseIdTests(unittest.TestCase):
    def run_script(self, mode, payload, *args):
        return subprocess.run(
            [sys.executable, str(SCRIPT), mode, *args],
            input=json.dumps(payload),
            capture_output=True,
            text=True,
            check=False,
        )

    def test_created_release_id_comes_from_post_response(self):
        result = self.run_script("created", {"id": 24680, "tag_name": "v0.6.0"})

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "24680")

    def test_existing_release_id_is_found_across_list_pages(self):
        pages = [
            [{"id": 12, "tag_name": "v0.5.0"}],
            [{"id": 34, "tag_name": "v0.6.0", "draft": True}],
        ]
        result = self.run_script("existing", pages, "v0.6.0")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "34")

    def test_missing_existing_release_returns_empty_output(self):
        result = self.run_script("existing", [[{"id": 12, "tag_name": "v0.5.0"}]], "v0.6.0")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")

    def test_duplicate_existing_releases_are_rejected(self):
        pages = [[{"id": 12, "tag_name": "v0.6.0"}], [{"id": 34, "tag_name": "v0.6.0"}]]
        result = self.run_script("existing", pages, "v0.6.0")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("multiple releases", result.stderr)


if __name__ == "__main__":
    unittest.main()
