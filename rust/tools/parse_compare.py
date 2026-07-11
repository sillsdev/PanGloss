#!/usr/bin/env python3
"""Compare two HermitCrab batch outputs by *analysis set*, not byte-for-byte.

We already gate on byte-identical signatures ("bit-perfect"). This tool answers a
softer, often more useful question: does each word get the *same set of parses*,
even if they come out in a different order (or with different duplicate counts)?

Batch TSV format (one word may appear on two lines: a STARTED sentinel then the
result):
    idx  word  time_ms  status  signature
where `signature` is a ';'-separated list of per-analysis parse strings, or '-'
for zero analyses.

For every word present in both files we bucket the pair into:
    IDENTICAL       signatures are byte-identical
    MULTISET_EQUAL  same analyses with same duplicate counts, only order differs
    SET_EQUAL       same *set* of analyses, but duplicate counts differ
    STATUS_DIFF     status differs (e.g. ok vs TIMEOUT/SKIPPED)
    DIFFERENT       the analysis sets genuinely differ

A word is "parse-exact" if it is IDENTICAL, MULTISET_EQUAL, or SET_EQUAL and the
status matches -- i.e. we have the correct analysis and only ordering/dups differ.

Usage:
    python parse_compare.py <rust.tsv> <reference.tsv> [--show N] [--multiset]

    --show N    list up to N words from each non-parse-exact bucket (default 20)
    --multiset  treat differing duplicate counts as a real difference (SET_EQUAL
                pairs are then reported as DIFFERENT)
"""
import sys
from collections import Counter


def load(path):
    """word -> (status, signature) using the last (result) line for each word."""
    out = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            parts = line.rstrip("\n").split("\t")
            if len(parts) < 4:
                continue
            word, third = parts[1], parts[2]
            if third == "STARTED":
                continue
            status = parts[3]
            sig = parts[4] if len(parts) >= 5 else "-"
            out[word] = (status, sig)
    return out


def analyses(sig):
    """Multiset (Counter) of analysis strings; '-'/'' means zero analyses."""
    if sig in ("-", ""):
        return Counter()
    return Counter(sig.split(";"))


def classify(a, b, strict_multiset):
    (sa, sga), (sb, sgb) = a, b
    if sga == sgb and sa == sb:
        return "IDENTICAL"
    if sa != sb:
        return "STATUS_DIFF"
    ca, cb = analyses(sga), analyses(sgb)
    if ca == cb:
        return "MULTISET_EQUAL"  # only order differs
    if set(ca) == set(cb):
        return "DIFFERENT" if strict_multiset else "SET_EQUAL"
    return "DIFFERENT"


PARSE_EXACT = {"IDENTICAL", "MULTISET_EQUAL", "SET_EQUAL"}


def main():
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:
        pass
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    show = 20
    strict_multiset = "--multiset" in sys.argv
    if "--show" in sys.argv:
        show = int(sys.argv[sys.argv.index("--show") + 1])
        args = [a for a in args if a != str(show)]
    if len(args) != 2:
        print(__doc__)
        sys.exit(2)
    rust, ref = load(args[0]), load(args[1])

    common = rust.keys() & ref.keys()
    only_rust = rust.keys() - ref.keys()
    only_ref = ref.keys() - rust.keys()

    buckets = {k: [] for k in
               ("IDENTICAL", "MULTISET_EQUAL", "SET_EQUAL", "STATUS_DIFF", "DIFFERENT")}
    for w in common:
        buckets[classify(rust[w], ref[w], strict_multiset)].append(w)

    n = len(common)
    exact = sum(len(buckets[k]) for k in PARSE_EXACT)
    print(f"rust     : {args[0]}")
    print(f"reference: {args[1]}")
    print(f"words in both: {n}   only-in-rust: {len(only_rust)}   only-in-ref: {len(only_ref)}")
    print("-" * 60)
    for k in ("IDENTICAL", "MULTISET_EQUAL", "SET_EQUAL", "STATUS_DIFF", "DIFFERENT"):
        c = len(buckets[k])
        pct = 100.0 * c / n if n else 0.0
        print(f"  {k:<15} {c:>6}  ({pct:5.1f}%)")
    print("-" * 60)
    print(f"  byte-exact     {len(buckets['IDENTICAL']):>6}  ({100.0*len(buckets['IDENTICAL'])/n if n else 0:5.1f}%)")
    print(f"  PARSE-EXACT    {exact:>6}  ({100.0*exact/n if n else 0:5.1f}%)   "
          f"(identical + reorder-only + dup-count-only)")
    print(f"  NOT parse-exact{n-exact:>6}  ({100.0*(n-exact)/n if n else 0:5.1f}%)   "
          f"(status or analysis-set differs)")

    for k in ("STATUS_DIFF", "SET_EQUAL", "DIFFERENT"):
        ws = sorted(buckets[k])
        if not ws:
            continue
        print(f"\n=== {k} ({len(ws)}) ===")
        for w in ws[:show]:
            rs, rg = rust[w]
            xs, xg = ref[w]
            print(f"  {w}")
            print(f"    rust[{rs}]: {rg}")
            print(f"    ref [{xs}]: {xg}")
        if len(ws) > show:
            print(f"  ... and {len(ws) - show} more")


if __name__ == "__main__":
    main()
