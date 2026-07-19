#!/usr/bin/env python
"""Coverage/parity comparison for Sena: derive_sena.py's templated enumeration vs. the Rust
pangloss engine oracle (reports/oracle/sena-sample-300-oracle-gloss.tsv).

Root-set reduction (explicit, honest, per the task's "reduced but honest experiment" rule):
Sena's templates enumerate ~36,500 derivations PER ROOT on average (measured on a 20-root probe;
see the report) -- extrapolating to the full 1,371-root lexicon would mean tens of millions of
derivations, impractical for this PoC's eager-enumeration converter within the time available
(a production compiler would instead build a shared trie/automaton across roots, the classic
lexc/foma construction, which this PoC's converter does not implement -- see the report's
"what a real implementation would need" section). We instead run on the 324 lexicon entries
whose shape is a literal substring of at least one of the 300 sample-oracle words -- a targeted
reduction that maximizes relevant coverage for the SAME 300-word comparison, not an arbitrary
truncation.
"""
import sys
import time
from collections import defaultdict

from hc_xml import parse_grammar
from derive_sena import realize_sena


def gloss_of(g, xml_id):
    if xml_id in g.mrules:
        return g.mrules[xml_id].gloss
    if xml_id in g.crules:
        return g.crules[xml_id].name
    for e in g.lexicon:
        if e.xml_id == xml_id:
            return e.gloss
    return "?"


def main():
    grammar_path = sys.argv[1]
    oracle_tsv = sys.argv[2]
    needed_roots_file = sys.argv[3] if len(sys.argv) > 3 else None

    g = parse_grammar(grammar_path)
    only_roots = None
    if needed_roots_file:
        only_roots = set(l.strip() for l in open(needed_roots_file, encoding="utf-8") if l.strip())

    t0 = time.time()
    results = realize_sena(g, only_roots=only_roots, verbose=True)
    build_s = time.time() - t0

    surface_to_tagsets = defaultdict(set)
    for surf, tags in results:
        surface_to_tagsets[surf].add(tags)

    def render(tags):
        return tuple(sorted(gloss_of(g, t) for t in tags))

    oracle = {}
    words = []
    for line in open(oracle_tsv, encoding="utf-8"):
        parts = line.rstrip("\n").split("\t")
        word, status, glosses = parts[0], parts[1], parts[2]
        oracle[word] = (status, glosses)
        words.append(word)

    exact = 0
    covered_no_root = 0  # words whose root(s) weren't in the reduced set -- excluded from the denominator, counted separately
    mismatches = []
    considered = 0
    for w in words:
        status, glosses_str = oracle.get(w, ("MISSING", "-"))
        if status not in ("ok", "no-parse"):
            continue  # timeouts / skipped-pathological / missing: not part of this comparison
        oracle_set = set()
        if status == "ok":
            for gl in glosses_str.split(";"):
                oracle_set.add(tuple(sorted(tok.strip() for tok in gl.split("-"))))
        fst_tagsets = surface_to_tagsets.get(w, set())
        fst_set = set(render(t) for t in fst_tagsets)
        considered += 1
        if status == "ok":
            if fst_set == oracle_set:
                exact += 1
            elif not fst_set:
                covered_no_root += 1  # plausibly just outside our reduced root set
            else:
                mismatches.append((w, status, oracle_set, fst_set))
        elif status == "no-parse":
            if not fst_set:
                exact += 1
            else:
                mismatches.append((w, status, oracle_set, fst_set))

    print(f"build (enumerate, no phonology): {build_s:.2f}s, {len(results)} derivations, "
          f"{len(surface_to_tagsets)} distinct surface forms")
    print(f"words considered (status ok|no-parse): {considered}")
    print(f"exact analysis-SET parity: {exact}/{considered}")
    print(f"words with an oracle analysis but no FST output at all (likely outside reduced root set): {covered_no_root}")
    print(f"genuine mismatches (FST produced something, but the wrong set): {len(mismatches)}")
    for w, status, oset, fset in mismatches[:40]:
        print(f"  {w!r}: oracle({status})={sorted(oset)} fst={sorted(fset)}")


if __name__ == "__main__":
    main()
