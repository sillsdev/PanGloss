#!/usr/bin/env python3
"""Dependency-free semantic validation for an FST recipe registry JSON document."""

from __future__ import annotations

import json
import sys
from pathlib import Path


ALLOWED_KINDS = {"Leaf", "Compose", "Union", "Gate", "Replace"}
ELIGIBILITY = {"documented", "validated", "automated"}


def fail(message: str) -> None:
    raise ValueError(message)


def validate_recipe(recipe: object, index: int) -> None:
    if not isinstance(recipe, dict):
        fail(f"recipes[{index}] must be an object")
    recipe_id = recipe.get("id")
    if not isinstance(recipe_id, str) or not recipe_id:
        fail(f"recipes[{index}].id must be a non-empty string")
    eligibility = recipe.get("planner_eligibility")
    if eligibility not in ELIGIBILITY:
        fail(f"{recipe_id}: unknown planner_eligibility {eligibility!r}")
    template = recipe.get("plan_template")
    if not isinstance(template, dict):
        fail(f"{recipe_id}: plan_template must be an object")
    nodes = template.get("nodes")
    root = template.get("root")
    if not isinstance(nodes, list) or not nodes:
        fail(f"{recipe_id}: plan_template.nodes must be a non-empty list")
    node_ids: set[str] = set()
    for node in nodes:
        if not isinstance(node, dict):
            fail(f"{recipe_id}: each plan node must be an object")
        node_id, kind = node.get("id"), node.get("kind")
        if not isinstance(node_id, str) or not node_id:
            fail(f"{recipe_id}: every node needs a non-empty id")
        if node_id in node_ids:
            fail(f"{recipe_id}: duplicate node id {node_id!r}")
        node_ids.add(node_id)
        if kind not in ALLOWED_KINDS:
            fail(f"{recipe_id}: {node_id} uses unsupported primitive {kind!r}")
    if root not in node_ids:
        fail(f"{recipe_id}: root {root!r} is not a node")
    for node in nodes:
        inputs = node.get("inputs", [])
        if not isinstance(inputs, list) or not all(isinstance(x, str) for x in inputs):
            fail(f"{recipe_id}: {node['id']}.inputs must be a string list")
        unknown = set(inputs) - node_ids
        if unknown:
            fail(f"{recipe_id}: {node['id']} references unknown inputs {sorted(unknown)}")
        if node["kind"] == "Leaf" and inputs:
            fail(f"{recipe_id}: Leaf {node['id']} cannot have inputs")
    evidence = recipe.get("evidence")
    if not isinstance(evidence, dict):
        fail(f"{recipe_id}: evidence must be an object")
    trials = evidence.get("trials")
    if not isinstance(trials, list):
        fail(f"{recipe_id}: evidence.trials must be a list")
    if eligibility == "automated":
        if not evidence.get("feature_extractor") or not evidence.get("builder"):
            fail(f"{recipe_id}: automated recipes require feature_extractor and builder")
        measured_grammars = {
            trial.get("grammar") for trial in trials
            if isinstance(trial, dict) and trial.get("outcome") != "not-run"
        }
        if len(measured_grammars) < 2:
            fail(f"{recipe_id}: automated recipes require evidence from two grammars")


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(f"usage: {argv[0]} REGISTRY.json", file=sys.stderr)
        return 2
    path = Path(argv[1])
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(data, dict) or data.get("registry_version") != 1:
            fail("registry_version must be 1")
        recipes = data.get("recipes")
        if not isinstance(recipes, list):
            fail("recipes must be a list")
        ids: set[str] = set()
        for index, recipe in enumerate(recipes):
            validate_recipe(recipe, index)
            recipe_id = recipe["id"]
            if recipe_id in ids:
                fail(f"duplicate recipe id {recipe_id!r}")
            ids.add(recipe_id)
    except (OSError, json.JSONDecodeError, ValueError) as error:
        print(f"invalid recipe registry: {error}", file=sys.stderr)
        return 1
    print(f"valid recipe registry: {path} ({len(recipes)} recipes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
