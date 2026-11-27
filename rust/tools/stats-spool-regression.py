#!/usr/bin/env python3
"""Compare complete frozen stats runs with Callgrind on a synthetic grammar and word list."""

import argparse
import re
import sqlite3
import subprocess
import tempfile
from pathlib import Path


def measure(binary, grammar, words, scratch):
    profile = scratch / "callgrind"
    cache = scratch / "stats.sqlite"
    args = [
        "valgrind", "--tool=callgrind", f"--callgrind-out-file={profile}",
        str(binary), "batch", str(grammar), str(words), str(scratch / "out.tsv"),
        "--threads", "1", "--no-flush", "--stats", "--cache", str(cache),
        "--stats-manifest", str(scratch / "manifest.json"),
    ]
    with (scratch / "batch.log").open("wb") as log:
        run = subprocess.run(args, stdout=log, stderr=log, check=False)
    if run.returncode:
        raise SystemExit(f"{binary.name} exited {run.returncode} under Callgrind")
    matches = re.findall(r"^totals:\s+(\d+)\s*$", profile.read_text(), re.MULTILINE)
    if len(matches) != 1:
        raise SystemExit(f"expected one instruction total for {binary.name}")
    with sqlite3.connect(cache.as_uri() + "?mode=ro", uri=True) as db:
        observed, complete = db.execute(
            "SELECT count(*), sum(gates_measured AND NOT (capped OR timed_out OR invalid_shape)) FROM word"
        ).fetchone()
        facts = db.execute("SELECT count(*) FROM fact").fetchone()[0]
        gates = db.execute("SELECT count(*) FROM gate_fact").fetchone()[0]
    if not observed or observed != complete or not facts or not gates:
        raise SystemExit("fixture must produce complete stats with ordinary facts and gate facts")
    return int(matches[0]), (observed, facts, gates)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("before", "after", "grammar", "words"):
        parser.add_argument(name, type=lambda value: Path(value).resolve(strict=True))
    parser.add_argument("--min-reduction", type=float, default=5, help="required percentage reduction")
    args = parser.parse_args()
    if not 0 < args.min_reduction < 100:
        parser.error("--min-reduction must be between 0 and 100")

    samples = {name: [] for name in ("before", "after")}
    coverage = set()
    for name in ("before", "after", "after", "before"):
        with tempfile.TemporaryDirectory(prefix="stats-spool-") as scratch:
            instructions, counts = measure(getattr(args, name), args.grammar, args.words, Path(scratch))
        samples[name].append(instructions)
        coverage.add(counts)
    if len(coverage) != 1:
        raise SystemExit("word, fact, or gate row counts changed")
    before = min(samples["before"])
    after = max(samples["after"])
    reduction = 100 * (before - after) / before
    print(f"Instructions {samples}. Minimum observed reduction {reduction:.2f}%.")
    print(f"Coverage (words, facts, gate facts) {coverage.pop()}.")
    if reduction < args.min_reduction:
        raise SystemExit(f"required at least {args.min_reduction:g}% fewer instructions")


if __name__ == "__main__":
    main()
