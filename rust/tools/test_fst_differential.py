"""Tests for occurrence, completeness and multiset semantics in the stage-0 reporter."""
import json
from pathlib import Path
import tempfile
import unittest
from fst_differential import summarize


class DifferentialTests(unittest.TestCase):
    def test_missing_occurrence_is_incomplete_even_when_other_rows_are_equal(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.jsonl"
            Path(str(path) + ".manifest.json").write_text(json.dumps({"words": ["a", "a"]}))
            path.write_text(json.dumps({"word_id": 0, "word": "a", "raw_candidates": 0,
                "unique_candidates": 0, "incomplete": False, "oracle": [],
                "confirmed": [], "proposal_misses": [], "spurious_confirmed": [],
                "confirmed_multiset_equal": True}) + "\n")
            result = summarize(path)
            self.assertFalse(result["all_rows_complete"])
            self.assertEqual(result["summary"]["missing_rows"], 1)
            self.assertEqual(result["summary"]["multiset_equal_rows"], 1)
            self.assertEqual(result["rows"][1]["word_id"], 1)

    def test_interrupted_last_write_keeps_the_occurrence_incomplete(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.jsonl"
            Path(str(path) + ".manifest.json").write_text(json.dumps({"words": ["a"]}))
            path.write_text('{"word_id": 0, "word": "a"')
            result = summarize(path)
            self.assertEqual(result["summary"]["interrupted_jsonl_tail"], 1)
            self.assertEqual(result["summary"]["missing_rows"], 1)
            self.assertFalse(result["all_rows_complete"])

    def test_malformed_completed_row_is_an_error(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.jsonl"
            Path(str(path) + ".manifest.json").write_text(json.dumps({"words": ["a"]}))
            path.write_text('{"word_id": 0\n')
            with self.assertRaises(json.JSONDecodeError):
                summarize(path)

    def test_interrupted_csharp_write_is_missing_evidence_not_an_empty_parse(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.jsonl"
            oracle = Path(directory) / "oracle.jsonl"
            meta = {"source_sha256": "a", "words_sha256": "b", "machine_pin": "c"}
            Path(str(path) + ".manifest.json").write_text(json.dumps({**meta, "words": ["a"]}))
            path.write_text(json.dumps({"word_id": 0, "word": "a", "hc_status": "complete", "raw_candidates": None,
                "unique_candidates": None, "incomplete": True, "oracle": []}) + "\n")
            oracle.write_text(json.dumps(meta) + "\n" + '{"word_id": 0, "status": "complete"')
            result = summarize(path, oracle)
            self.assertEqual(result["summary"]["csharp_interrupted_jsonl_tail"], 1)
            self.assertEqual(result["summary"]["csharp_missing_rows"], 1)
            self.assertNotIn("csharp_hc_compared_rows", result["summary"])

    def test_csharp_hash_mismatch_refuses_comparison(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.jsonl"
            oracle = Path(directory) / "oracle.jsonl"
            Path(str(path) + ".manifest.json").write_text(json.dumps({"words": [], "source_sha256": "a"}))
            oracle.write_text(json.dumps({"source_sha256": "b"}) + "\n")
            with self.assertRaisesRegex(ValueError, "source_sha256"):
                summarize(path, oracle)

    def test_csharp_comparison_exposes_misses_and_spurious_confirmations(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.jsonl"
            oracle = Path(directory) / "oracle.jsonl"
            meta = {"source_sha256": "a", "words_sha256": "b", "machine_pin": "c"}
            Path(str(path) + ".manifest.json").write_text(json.dumps({**meta, "words": ["a"]}))
            expected = {"morphemes": ["expected"], "rootIndex": 0, "category": None}
            actual = {"morphemes": ["other"], "rootIndex": 0, "category": None}
            oracle.write_text(json.dumps(meta) + "\n" + json.dumps({"word_id": 0, "word": "a", "status": "complete", "analyses": [expected]}) + "\n")
            path.write_text(json.dumps({"word_id": 0, "word": "a", "hc_status": "complete", "raw_candidates": 1,
                "unique_candidates": 1, "incomplete": False, "oracle": [{"identity": actual, "count": 1}],
                "confirmed": [{"identity": actual, "count": 1}], "proposal_misses": [], "spurious_confirmed": [],
                "confirmed_multiset_equal": True, "candidates": [{"morphemes": ["other"], "root_index": 0}]}) + "\n")
            result = summarize(path, oracle)
            self.assertEqual(result["summary"]["multiset_equal_rows"], 1)
            self.assertEqual(result["summary"]["csharp_proposal_misses"], 1)
            self.assertEqual(result["summary"]["csharp_spurious_confirmed"], 1)
            self.assertEqual(result["summary"]["csharp_fst_multiset_different_rows"], 1)

    def test_csharp_multiplicity_difference_survives_set_equality(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.jsonl"
            oracle = Path(directory) / "oracle.jsonl"
            meta = {"source_sha256": "a", "words_sha256": "b", "machine_pin": "c"}
            Path(str(path) + ".manifest.json").write_text(json.dumps({**meta, "words": ["a"]}))
            analysis = {"morphemes": ["root"], "rootIndex": 0, "category": None}
            oracle.write_text(json.dumps(meta) + "\n" + json.dumps({"word_id": 0, "word": "a", "status": "complete", "analyses": [analysis, analysis]}) + "\n")
            path.write_text(json.dumps({"word_id": 0, "word": "a", "hc_status": "complete", "raw_candidates": None,
                "unique_candidates": None, "incomplete": True, "oracle": [{"identity": analysis, "count": 1}]}) + "\n")
            result = summarize(path, oracle)
            self.assertEqual(result["summary"]["csharp_hc_multiset_different_rows"], 1)
            self.assertFalse(result["recall_measured"])


if __name__ == "__main__":
    unittest.main()
