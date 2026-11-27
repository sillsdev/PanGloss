#!/usr/bin/env python3
"""Summarize the stage-0 JSONL, preserving missing occurrences and optional C# evidence."""
import argparse
import collections
import json
from pathlib import Path


def identity(value):
    return (tuple(value["morphemes"]), value["rootIndex"], value["category"])


def multiset(entries):
    return collections.Counter({identity(entry["identity"]): entry["count"] for entry in entries})


def read_jsonl(path):
    data = Path(path).read_text()
    lines = data.splitlines()
    rows = []
    for index, line in enumerate(lines):
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError:
            if index != len(lines) - 1 or data.endswith("\n"):
                raise
            return rows, True
    return rows, False


def summarize(path, csharp_path=None):
    manifest = json.loads(Path(str(path) + ".manifest.json").read_text())
    observed = {}
    rows, interrupted_tail = read_jsonl(path) if Path(path).exists() else ([], False)
    for row in rows:
        key = row["word_id"]
        if key in observed:
            raise ValueError(f"duplicate word ID {key}")
        observed[key] = row
    expected = manifest["words"]
    if any(key < 0 or key >= len(expected) for key in observed):
        raise ValueError("unexpected word IDs")
    csharp = {}
    csharp_interrupted_tail = False
    if csharp_path:
        lines, csharp_interrupted_tail = read_jsonl(csharp_path)
        if not lines:
            raise ValueError("C# header is missing or incomplete")
        meta = lines[0]
        for field in ("source_sha256", "words_sha256", "machine_pin"):
            if meta[field] != manifest[field]:
                raise ValueError(f"C# {field} does not match manifest")
        for row in lines[1:]:
            if row["status"] != "started":
                if row["word_id"] in csharp:
                    raise ValueError("duplicate C# completed word ID")
                csharp[row["word_id"]] = row
        if any(key < 0 or key >= len(expected) for key in csharp):
            raise ValueError("unexpected C# word ID")
    summary = collections.Counter()
    if interrupted_tail:
        summary["interrupted_jsonl_tail"] = 1
    if csharp_interrupted_tail:
        summary["csharp_interrupted_jsonl_tail"] = 1
    differences = []
    for word_id, word in enumerate(expected):
        summary["words"] += 1
        row = observed.get(word_id)
        if row is None:
            summary["missing_rows"] += 1
            summary["incomplete_rows"] += 1
            differences.append({"word_id": word_id, "word": word, "status": "missing"})
            continue
        if row["word"] != word:
            raise ValueError(f"word ID {word_id} does not name {word!r}")
        summary["raw_candidates"] += row["raw_candidates"] or 0
        summary["unique_candidates"] += row["unique_candidates"] or 0
        if row["incomplete"]:
            summary["incomplete_rows"] += 1
        else:
            summary["complete_rows"] += 1
            summary["oracle_identities"] += len(row["oracle"])
            summary["proposal_misses"] += len(row["proposal_misses"])
            summary["spurious_confirmed"] += len(row["spurious_confirmed"])
            summary["multiset_equal_rows" if row["confirmed_multiset_equal"] else "multiset_different_rows"] += 1
        detail = {"word_id": word_id, "word": word, "fst_incomplete": row["incomplete"]}
        if csharp_path:
            oracle = csharp.get(word_id)
            if oracle is None:
                summary["csharp_missing_rows"] += 1
                detail["csharp_status"] = "missing"
            elif oracle["word"] != word:
                raise ValueError(f"C# word ID {word_id} mismatch")
            elif oracle["status"] != "complete" or row["hc_status"] != "complete":
                summary["csharp_hc_incomplete_rows"] += 1
                detail["csharp_status"] = oracle["status"]
            else:
                expected_counts = collections.Counter(identity(entry) for entry in oracle["analyses"])
                hc_counts = multiset(row["oracle"])
                summary["csharp_hc_compared_rows"] += 1
                equal = expected_counts == hc_counts
                summary["csharp_hc_multiset_equal_rows" if equal else "csharp_hc_multiset_different_rows"] += 1
                detail["csharp_hc_multiset_equal"] = equal
                detail["csharp_analyses"] = oracle["analyses"]
                if not row["incomplete"]:
                    proposed = {(tuple(candidate["morphemes"]), candidate["root_index"]) for candidate in row["candidates"]}
                    misses = [key for key in expected_counts if (key[0], key[1]) not in proposed]
                    actual_counts = multiset(row["confirmed"])
                    summary["csharp_fst_compared_rows"] += 1
                    summary["csharp_proposal_misses"] += len(misses)
                    summary["csharp_spurious_confirmed"] += len(actual_counts.keys() - expected_counts.keys())
                    fst_equal = expected_counts == actual_counts
                    summary["csharp_fst_multiset_equal_rows" if fst_equal else "csharp_fst_multiset_different_rows"] += 1
                    detail["csharp_fst_multiset_equal"] = fst_equal
                    detail["csharp_proposal_misses"] = misses
        differences.append(detail)
    return {"manifest": manifest, "summary": dict(summary), "rows": differences,
            "recall_measured": summary["complete_rows"] > 0,
            "all_rows_complete": summary["incomplete_rows"] == 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("--csharp", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    result = summarize(args.input, args.csharp)
    args.out.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(result["summary"], sort_keys=True))


if __name__ == "__main__":
    main()
