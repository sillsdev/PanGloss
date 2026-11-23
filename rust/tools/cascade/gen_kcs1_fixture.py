#!/usr/bin/env python3
"""Write the KC-S1 fixture of the cascade backend (plan S.10, WS1): the reference generator's full `B`.

For each typed word (and its three re-castings) and each mark depth k, the generator logic of
gen_candidates.py `--casings` (WS0 reference, copied here unchanged) is run in-process and the result is
stored as a DIGEST (row count, XOR and sum of 64-bit FNV-1a hashes of the rows) plus, for a few small sets, the
rows themselves. Everything is synthetic.

Row key: "<NFD code points in hex, space separated>|<dist>|<weight>" where dist is the true unit distance and
weight (thousandths) is given for dist <= 1 only (the reference alignment allows no transposition with a gap, so
a dist-2 weight is an upper bound there and is not compared); "-" otherwise.

Rows with a unit that the table cannot write at any depth are DROPPED (counted in `dropped`): the generator offers
the typed casing of a unit even when no representation spells it, which a cascade built from the representations
cannot produce (reported in reports/WS1.md).

Regenerate (about 10 minutes with KCS1_JOBS=8):
  python3 -I rust/tools/cascade/gen_kcs1_fixture.py > rust/crates/pg-foma-runtime/src/cascade/fixtures/kcs1.txt
"""
import itertools, os, random, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_candidates as g
from multiprocessing import Pool

# Every S.9 character kind in one table: a base with a precomposed marked form (e, é); a base with no precomposed
# marked form plus a combining mark (t̃); three mark-only segments of one combining class (and so two on one base);
# a digraph with an apostrophe (t') and the apostrophe as a segment of its own (the generator offers every unit of a
# digraph standalone, a cascade writes it only after the digraph's first unit); a hyphen; a segment with both cases (t, T); a segment with only a capital
# (Q); a base whose full folding is two characters (ß); a boundary with a written form (+).
REPS_SEG = ["e", "é", "t", "T", "t̃", "t'", "'", "Q", "ß", "-", "́", "̀", "̂"]
REPS_BND = ["+"]
M1, M2, M3 = "́", "̀", "̂"

ONE = ["e", "é", "t", "T", "Q", "ß", "-", "+", "'", "t̃", "e" + M2, "e" + M3, "e" + M1 + M2, "e" + M1 + M2 + M3,
       "t" + M3, "T" + M1, "Q" + M1, "ß" + M2, M1, M1 + M2, "z", "e" + (M1 + M2 + M3) * 3, "te"[:1] + M1 + M1,
       "'" + M1, "-" + M1 + M2]
TWO = ["et", "te", "Te", "Qe", "ße", "-t", "+e", "t'", "e" + M1 + "t", "t" + M2 + M1 + "e", M1 + "e", "ez",
       "zt", "ee", "tT", "ß-", "Q+"]

def recase(word):
    us = g.units(word)
    out = {word: "typed"}
    up = "".join(b.upper() + m if b.upper() != b else b + m for b, m in us)
    out.setdefault(g.nfd(up), "allcaps")
    first = "".join((b.upper() if i == 0 else b) + m for i, (b, m) in enumerate(us))
    out.setdefault(g.nfd(first), "firstcap")
    rng = random.Random(sum(map(ord, word)))
    i = rng.randrange(len(us)) if us else 0
    one = "".join((b.swapcase() if j == i else b) + m for j, (b, m) in enumerate(us))
    out.setdefault(g.nfd(one), "onecase")
    return list(out)

G = g.Grammar(g.nfd(r) for r in REPS_SEG + REPS_BND)

def writable_exact(u):
    b, s = u
    for (rb, s0), ext in G.rep_units:
        if rb != b:
            continue
        if s0 == s:
            return True
        if ext and s.startswith(s0) and G._canon(b, s) and G._decomposes(s[len(s0):]):
            return True
    return False

def casings(te, inv, f):
    """Every exact casing the table can spell for folded unit f, at any depth, plus the typed one."""
    opts = set(g.casing_options(te, inv, f))
    for (b, _), _ext in G.rep_units:
        if b and g.fold_unit((b, "")) [0] == f[0] and writable_exact((b, f[1])):
            opts.add((b, f[1]))
    return sorted(opts)

def hex_(s): return " ".join(f"{ord(c):04X}" for c in s)

def fnv(s):
    h = 0xcbf29ce484222325
    for b in s.encode():
        h = ((h ^ b) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return h

def run(job):
    word, k, want_rows = job
    te = g.units(word); tf = tuple(g.fold_units(te))
    alpha, inv = G.alphabet(k, tf)
    s1, s2 = g.ball(tf, alpha)
    n = x = sm = dropped = 0
    rows = []
    def add(units_, d, w):
        nonlocal n, x, sm, dropped
        if not all(writable_exact(u) for u in units_):
            dropped += 1; return
        key = f"{hex_(g.ustr(units_))}|{d}|{w}"
        h = fnv(key); n += 1; x ^= h; sm = (sm + h) & 0xFFFFFFFFFFFFFFFF
        if want_rows: rows.append(key)
    for combo in itertools.product(*[casings(te, inv, f) for f in tf]):
        if list(combo) != te: add(list(combo), 0, 0)
    for c in list(s1) + list(s2):
        d = g.dist_units(list(tf), list(c))
        w = g._align(te, list(tf), list(c), None, inv)[0][0] if d <= 1 else "-"
        for combo in itertools.product(*[casings(te, inv, f) for f in c]):
            add(list(combo), d, w)
    return word, k, n, x, sm, dropped, sorted(rows)

def main():
    jobs, ids = [], []
    for tier, words, ks in (("diag", ["e", "T", "ß", "te"], (1,)), ("one", ONE, (2, 3)), ("two", TWO, (2,))):
        for w in words:
            for t in ([g.nfd(w)] if tier == "diag" else recase(g.nfd(w))):
                for k in ks:
                    jobs.append((t, k, tier == "diag"))
                    ids.append(tier)
    if os.environ.get("KCS1_MINI"):
        jobs, ids = jobs[::9], ids[::9]
    out = sys.stdout
    out.write("# KC-S1 fixture (synthetic). Generated by rust/tools/cascade/gen_kcs1_fixture.py; do not edit.\n")
    out.write(f"# python {sys.version.split()[0]} unidata {g.ud.unidata_version}\n")
    for r in REPS_SEG: out.write(f"seg {hex_(g.nfd(r))}\n")
    for r in REPS_BND: out.write(f"bnd {hex_(g.nfd(r))}\n")
    with Pool(int(os.environ.get("KCS1_JOBS", "8"))) as p:
        res = p.map(run, jobs, chunksize=1)
    for (word, k, n, x, sm, dropped, rows), tier in zip(res, ids):
        out.write(f"set {tier} k={k} n={n} xor={x:016x} sum={sm:016x} dropped={dropped} typed={hex_(word)}\n")
        for r in rows: out.write(f"row {r}\n")

if __name__ == "__main__":
    main()
