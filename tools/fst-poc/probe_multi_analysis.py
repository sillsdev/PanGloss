#!/usr/bin/env python
"""Empirically test whether determinize()/minimize() preserve multi-analysis enumeration on a
handful of genuinely ambiguous Indonesian surface forms (not just one -- see
reports/04-standard-fst-poc.md §1/§7 for why this needed more than a single data point). Builds a
small FST from just the sampled ambiguous (surface, tags) pairs (fast: no need to build the full
591K-derivation closure into an FST just to check this).

Usage: PYTHONIOENCODING=utf-8 python probe_multi_analysis.py <grammar.xml> [n_samples] [seed]
"""
import random
import sys

from hc_xml import parse_grammar
from derive_indonesian import derive_and_realize
from build_indonesian_fst import build_union_fst
from collections import defaultdict


def main():
    grammar_path = sys.argv[1]
    n_samples = int(sys.argv[2]) if len(sys.argv) > 2 else 6
    seed = int(sys.argv[3]) if len(sys.argv) > 3 else 7

    g = parse_grammar(grammar_path)
    ct = g.chartable
    results = derive_and_realize(g, verbose=True)
    surf_tags = defaultdict(set)
    for surf, tags in results:
        surf_tags[surf].add(tags)
    ambiguous = [(s, ts) for s, ts in surf_tags.items() if len(ts) > 1]
    print(f"{len(ambiguous)} surface forms have >1 analysis in the enumerated set", file=sys.stderr)

    random.seed(seed)
    sample = random.sample(ambiguous, min(n_samples, len(ambiguous)))
    subset = [(s, t) for s, ts in sample for t in ts]

    fst, _ = build_union_fst(subset, ct)
    print(f"small FST from the sample: {len(fst.states)} states")
    det = fst.determinize_unweighted()
    mini = det.minimize_as_dfa()
    print(f"after determinize+minimize: {len(mini.states)} states")

    all_preserved = True
    for s, ts in sample:
        before = set(fst.generate(s))
        after = set(mini.generate(s))
        preserved = before == after
        all_preserved &= preserved
        status = "PRESERVED" if preserved else f"LOST ({len(before) - len(after)} missing)"
        print(f"  {s!r}: {len(ts)} source analyses, raw={len(before)}, after-det+min={len(after)} -- {status}")

    print(f"\nALL PRESERVED: {all_preserved}")


if __name__ == "__main__":
    main()
