"""Unit tests for the documentation checker (adapted from REAS)."""
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from scripts.check_docs import outside_fences, split_frontmatter, validate  # noqa: E402


class CheckDocsTests(unittest.TestCase):
    def test_valid_frontmatter(self):
        metadata, body, err = split_frontmatter("---\ntype: Concept\n---\nText")
        self.assertIsNone(err)
        self.assertEqual(metadata["type"], "Concept")
        self.assertEqual(body, "Text")

    def test_invalid_yaml(self):
        _, _, err = split_frontmatter("---\ntype: [unclosed\n---\nBody")
        self.assertIsNotNone(err)

    def test_code_fence_filter(self):
        self.assertNotIn("bad link", outside_fences("```md\nbad link\n```"))

    def test_real_repository(self):
        self.assertEqual(validate(ROOT, "docs"), [])

    def test_missing_type_is_an_error(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            (root / "docs").mkdir()
            (root / "docs" / "index.md").write_text("---\nokf_version: '0.2'\n---\n# Index\n")
            (root / "docs" / "c.md").write_text("---\ntitle: no type\n---\nBody\n")
            self.assertTrue(any("requires a non-empty type" in x for x in validate(root, "docs")))

    def test_broken_link(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            (root / "docs").mkdir()
            (root / "docs" / "index.md").write_text("---\nokf_version: '0.2'\n---\n# Index\n")
            (root / "docs" / "c.md").write_text("---\ntype: Concept\n---\n[missing](absent.md)\n")
            self.assertTrue(any("Broken local link" in x for x in validate(root, "docs")))


if __name__ == "__main__":
    unittest.main()
