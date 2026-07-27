"""Regression checks for the portable recipe-registry validator."""

from __future__ import annotations

import copy
import json
import tempfile
import unittest
from pathlib import Path

import validate_registry


HERE = Path(__file__).parent


class RegistryValidatorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.registry = json.loads((HERE / "registry.example.json").read_text(encoding="utf-8"))

    def test_example_registry_is_valid(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "registry.json"
            path.write_text(json.dumps(self.registry), encoding="utf-8")
            self.assertEqual(validate_registry.main(["validate_registry.py", str(path)]), 0)

    def test_rejects_a_non_primitive_node(self) -> None:
        invalid = copy.deepcopy(self.registry)
        invalid["recipes"][0]["plan_template"]["nodes"][0]["kind"] = "MagicOptimize"
        with self.assertRaisesRegex(ValueError, "unsupported primitive"):
            validate_registry.validate_recipe(invalid["recipes"][0], 0)

    def test_automated_recipe_needs_builder_and_two_real_trials(self) -> None:
        invalid = copy.deepcopy(self.registry)
        recipe = invalid["recipes"][0]
        recipe["planner_eligibility"] = "automated"
        with self.assertRaisesRegex(ValueError, "feature_extractor and builder"):
            validate_registry.validate_recipe(recipe, 0)


if __name__ == "__main__":
    unittest.main()
