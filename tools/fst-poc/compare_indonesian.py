#!/usr/bin/env python
"""Full coverage/parity comparison: derive_and_realize() vs. the Rust hc-rs engine oracle
(reports/oracle/indonesian-oracle-gloss.tsv), over the full 121-word corpus.

Ground truth caveat (recorded honestly, not hidden): both reference grammars leave the optional
`<MorphemeId>` XML element unset on every morpheme, so hc-rs's own raw batch signature
(morpheme-id join + surface) degenerates to blank ids -- there is no *populated* id-level oracle
to diff against mechanically. We therefore compare against `--gloss` text (populated distinctly
for every rule/entry in this grammar), which is a faithful proxy for entry/rule identity as long
as no two rules/entries that could compete for the same word share a gloss string -- checked
separately (see report). Our OWN FST tags remain full entry/rule xml-ids internally (per the
hard requirement); this script builds an id->gloss lookup once to render our tags into the same
comparable text.
"""
import sys
import time
from collections import defaultdict

from hc_xml import parse_grammar
from derive_indonesian import derive_and_realize


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
    words_path = sys.argv[2]
    oracle_tsv = sys.argv[3]

    g = parse_grammar(grammar_path)
    t0 = time.time()
    results = derive_and_realize(g, verbose=True)
    build_s = time.time() - t0

    surface_to_tagsets = defaultdict(set)
    for surf, tags in results:
        surface_to_tagsets[surf].add(tags)

    # Render our FST's tag-tuples into gloss-joined strings comparable to --gloss output. Root
    # gloss + each rule's gloss, in the ORDER the derivation applied them (root first) -- HC's
    # own --gloss rendering is root + affixes inside-out; since gloss text (not order) is what we
    # compare, we compare as a SORTED SET of individual gloss tokens per analysis, sidestepping
    # any ordering-convention mismatch between our tag order and hc-rs's internal rendering order.
    def render(tags):
        return tuple(sorted(gloss_of(g, t) for t in tags))

    words = [w.strip() for w in open(words_path, encoding="utf-8") if w.strip()]
    oracle = {}
    for line in open(oracle_tsv, encoding="utf-8"):
        parts = line.rstrip("\n").split("\t")
        word, status, glosses = parts[0], parts[1], parts[2]
        oracle[word] = (status, glosses)

    exact = 0
    mismatches = []
    for w in words:
        status, glosses_str = oracle.get(w, ("MISSING", "-"))
        oracle_set = set()
        if status == "ok":
            for gl in glosses_str.split(";"):
                oracle_set.add(tuple(sorted(tok.strip() for tok in gl.split("-"))))
        fst_tagsets = surface_to_tagsets.get(w, set())
        fst_set = set(render(t) for t in fst_tagsets)
        if status == "ok":
            if fst_set == oracle_set:
                exact += 1
            else:
                mismatches.append((w, status, oracle_set, fst_set))
        elif status == "no-parse":
            if not fst_set:
                exact += 1
            else:
                mismatches.append((w, status, oracle_set, fst_set))
        else:
            mismatches.append((w, status, oracle_set, fst_set))

    print(f"build (enumerate+realize): {build_s:.2f}s, {len(results)} derivations, "
          f"{len(surface_to_tagsets)} distinct surface forms")
    print(f"exact analysis-SET parity: {exact}/{len(words)}")
    print(f"mismatches: {len(mismatches)}")
    for w, status, oset, fset in mismatches:
        print(f"  {w!r}: oracle({status})={sorted(oset)} fst={sorted(fset)}")


if __name__ == "__main__":
    main()
