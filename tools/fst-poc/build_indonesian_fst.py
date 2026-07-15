#!/usr/bin/env python
"""Build the actual compiled FST artifact for Indonesian from derive_and_realize()'s
(surface, tags) pairs, measure its size (states/arcs, AT&T text size, gzipped size), measure
per-word lookup speed, and -- critically -- empirically test whether determinize()/minimize()
preserve full multi-analysis enumeration (the thing HYBRID_FST_FEASIBILITY.md §5.2 claims breaks
under determinization, there specifically for *unification* arcs; this PoC's arcs are plain
expanded symbols, so we test the DIFFERENT and more fundamental question of whether determinizing
a non-functional relation -- a word with >1 analysis -- loses analyses at all).

Tape convention here (chosen for build simplicity): tape[0] = SURFACE, tape[-1] = TAGS (bracket-
wrapped entry/rule xml-ids, one arc per tag, no literal characters on this tape at all). Lookup:
`fst.generate(word)` (= apply(inverse=False): tokenize against tape[0], emit tape[-1]).
"""
import gzip
import random
import sys
import time

from pyfoma import FST, State

from hc_xml import parse_grammar
from derive_indonesian import derive_and_realize

TAG_OPEN = "["  # ASCII, not the fancy U+2045/2046 quill brackets -- AT&T/save_att opens its
TAG_CLOSE = "]"  # output files with the platform-default encoding (cp1252 on this Windows box),
# which cannot represent U+2045; ASCII avoids that entirely and is what a real deployed tag
# alphabet would use anyway.


def build_union_fst(results, ct):
    f = FST()
    top = f.initialstate
    finals = set()
    alphabet = set()
    n_states = 1
    for surf, tags in results:
        cur = top
        for ch in ct.tokenize(surf):
            lit = ct.primary(ch)
            nxt = State()
            cur.add_transition(nxt, (lit, ''), 0.0)
            alphabet.add(lit)
            cur = nxt
            n_states += 1
        for t in tags:
            tagstr = f"{TAG_OPEN}{t}{TAG_CLOSE}"
            nxt = State()
            cur.add_transition(nxt, ('', tagstr), 0.0)
            alphabet.add(tagstr)
            cur = nxt
            n_states += 1
        finals.add(cur)
    f.finalstates = finals
    f.alphabet = alphabet
    # collect states via f.states (pyfoma expects this populated for save/determinize)
    seen = set()
    stack = [top]
    while stack:
        s = stack.pop()
        if s in seen:
            continue
        seen.add(s)
        for lbl, t in s.all_transitions():
            stack.append(t.targetstate)
    f.states = seen
    return f, n_states


def main():
    grammar_path = sys.argv[1]
    g = parse_grammar(grammar_path)
    ct = g.chartable

    t0 = time.time()
    all_results = derive_and_realize(g, verbose=True)
    realize_s = time.time() - t0

    sample_n = int(sys.argv[3]) if len(sys.argv) > 3 else None
    # Corpus words must ALWAYS be included regardless of sampling, so the lookup-speed and
    # parity-relevant probes below are never artifacts of what got sampled out.
    corpus_words = set(w.strip() for w in open(sys.argv[2], encoding="utf-8") if w.strip())
    if sample_n is not None and sample_n < len(all_results):
        random.seed(42)
        forced = [r for r in all_results if r[0] in corpus_words]
        rest = [r for r in all_results if r[0] not in corpus_words]
        take = max(0, sample_n - len(forced))
        results = forced + random.sample(rest, min(take, len(rest)))
        print(f"SAMPLED build: {len(results)}/{len(all_results)} derivations "
              f"({len(forced)} forced-in corpus-word derivations + {len(results)-len(forced)} random)",
              file=sys.stderr)
    else:
        results = all_results

    t0 = time.time()
    fst, n_states_est = build_union_fst(results, ct)
    build_s = time.time() - t0
    n_states = len(fst.states)
    n_arcs = sum(len(list(s.all_transitions())) for s in fst.states)
    print(f"raw NFA: {n_states} states, {n_arcs} arcs, built in {build_s:.2f}s "
          f"(after {realize_s:.2f}s enumerate+realize)")
    if sample_n is not None and sample_n < len(all_results):
        ratio = len(all_results) / len(results)
        print(f"EXTRAPOLATED to the full {len(all_results)}-derivation set (linear scaling "
              f"assumption, labeled as extrapolation, not measured): "
              f"~{int(n_states*ratio)} states, ~{int(n_arcs*ratio)} arcs, "
              f"~{build_s*ratio:.0f}s build")

    att_path = "indonesian_raw.att"
    fst.save_att(att_path)
    import os
    raw_size = os.path.getsize(att_path)
    with open(att_path, "rb") as fh, gzip.open(att_path + ".gz", "wb") as gz:
        gz.writelines(fh)
    gz_size = os.path.getsize(att_path + ".gz")
    print(f"AT&T text size: {raw_size} bytes ({raw_size/1024:.1f} KiB), gzipped: {gz_size} bytes "
          f"({gz_size/1024:.1f} KiB)")

    # --- multi-analysis integrity test: pick a genuinely ambiguous word from the results ---
    from collections import defaultdict
    surf_tags = defaultdict(set)
    for surf, tags in results:
        surf_tags[surf].add(tags)
    ambiguous = [(s, ts) for s, ts in surf_tags.items() if len(ts) > 1]
    ambiguous.sort(key=lambda x: -len(x[1]))
    print(f"{len(ambiguous)} surface forms have >1 analysis in the enumerated set")
    if ambiguous:
        probe_word, probe_tagsets = ambiguous[0]
        print(f"probe word: {probe_word!r} ({len(probe_tagsets)} analyses in source data)")

        before = set(fst.generate(probe_word))
        print(f"  raw NFA apply(): {len(before)} distinct analysis strings returned")

        t0 = time.time()
        det = fst.determinize_unweighted()
        det_s = time.time() - t0
        after_det = set(det.generate(probe_word))
        print(f"  after determinize_unweighted() ({det_s:.2f}s): {len(after_det)} distinct analyses"
              f" -- {'PRESERVED' if after_det == before else 'LOST ANALYSES (' + str(len(before)-len(after_det)) + ' missing)'}")

        try:
            t0 = time.time()
            mini = det.minimize_as_dfa()
            min_s = time.time() - t0
            after_min = set(mini.generate(probe_word))
            print(f"  after + minimize_as_dfa() ({min_s:.2f}s): {len(after_min)} distinct analyses"
                  f" -- {'PRESERVED' if after_min == before else 'LOST ANALYSES'}")
            print(f"  minimized: {len(mini.states)} states")
        except Exception as e:
            print(f"  minimize_as_dfa() raised: {e!r}")

    # --- lookup speed over the corpus ---
    words = [w.strip() for w in open(sys.argv[2], encoding="utf-8") if w.strip()]
    t0 = time.time()
    for w in words:
        list(fst.generate(w))
    elapsed = time.time() - t0
    print(f"raw-NFA lookup: {len(words)} words in {elapsed*1000:.1f}ms "
          f"({elapsed/len(words)*1e6:.1f}us/word avg, {len(words)/elapsed:.0f} words/sec)")


if __name__ == "__main__":
    main()
