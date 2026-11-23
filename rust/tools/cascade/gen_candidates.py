#!/usr/bin/env python3
"""Reference candidate generator for the speller error model (plan S.2, WS0).  Stdlib only; run with `python3 -I`.

Normative model (plan S.2)
  * every string is a canonical-equivalence class held as its NFD code points;
  * unit = grapheme = a non-mark code point plus the marks that follow it (word-initial marks: one unit with an
    empty base).  No reference to the grammar's segments;
  * Fold(unit) = full Unicode case folding of the base (str.casefold = CaseFolding.txt statuses C+F), marks kept;
    one unit stays one unit;
  * E1 on folded units: sub 1.0 / kb 0.8 (different base), dia 0.5 (same base, whole stack changed; also insert or
    delete of an empty-base unit), del 1.0, ins 1.0, trans 0.9 (adjacent unequal units);
  * dist(t, s) = least number of E1 steps (unrestricted Damerau-Levenshtein over folded units); E = E1 o E1.
  * alphabet = the units cut from concatenations of character-definition representations (segment AND boundary),
    at most --mark-depth marks; plus the typed word's own units that the grammar can write at any depth.  No letter
    filter, no word list.

Importable: units, fold, fold_units, dist, case_distance, nearest_casing, inventory, Grammar, d1, ball, test32.

Output TSV, one line per candidate:
  source  candidate  distance  error_type  kb  source_units  candidate_folded  case_distance
(source, candidate, candidate_folded are the NFC names of the NFD classes.)  error_type at d1: ins|del|sub|trans|dia;
at d2 the two d1 types joined with '+'.  kb=1 when a substitution is between keyboard neighbours.
Modes: --count | --full (FB_k, one exact casing B0 per folded member) | --casings (every exact casing the
representations can spell, including the other casings of the word itself at distance 0 with type `case`: the full B, for the fuzz gate and KC-S1) | --sample N --seed S (N/2 from d1, N/2 from d2).
--mark-depth K is required (no default).  --test32 runs TEST 32 (case folding never ends in a mark).
"""
import argparse, html, itertools, os, random, re, sys, unicodedata as ud, json

W_SUB, W_KB, W_DIA, W_DEL, W_INS, W_TRANS = 1000, 800, 500, 1000, 1000, 900   # thousandths (S.2 placeholders)

# ---------------------------------------------------------------- units and fold
def nfd(s): return ud.normalize("NFD", s)
def nfc(s): return ud.normalize("NFC", s)
def is_mark(c): return ud.category(c).startswith("M")

def units(s):
    """NFD string -> list of (base, marks).  Leading marks form one unit with base ''."""
    out = []
    for c in nfd(s):
        if is_mark(c):
            if out: out[-1][1] += c
            else: out.append(["", c])
        else:
            out.append([c, ""])
    return [(b, m) for b, m in out]

def ustr(us): return "".join(b + m for b, m in us)

def fold_unit(u):
    return (nfd(u[0].casefold()), u[1])

def fold_units(us): return [fold_unit(u) for u in us]
def fold(s): return ustr(fold_units(units(s)))     # NFD string; fold of a string is unit by unit

# ---------------------------------------------------------------- keyboard
QWERTY_ROWS = ["qwertyuiop", "asdfghjkl", "zxcvbnm"]
def kb_neighbours():
    offs = [0.0, 0.25, 0.75]
    xy = {ch: (r, c + offs[r]) for r, row in enumerate(QWERTY_ROWS) for c, ch in enumerate(row)}
    return {a: {b for b, (rb, xb) in xy.items() if b != a and abs(ra - rb) <= 1 and abs(xa - xb) <= 1.0}
            for a, (ra, xa) in xy.items()}
KB = kb_neighbours()
def kb_adjacent(b1, b2): return b2 in KB.get(b1, ())

# ---------------------------------------------------------------- distance
def dist_units(a, b):
    """Unrestricted Damerau-Levenshtein (Lowrance-Wagner), unit cost, over lists of (folded) units.
    Inserting or deleting any unit, substituting, and swapping two adjacent unequal units are one step each
    (dia is a substitution here: it counts one step like every other operation)."""
    n, m = len(a), len(b)
    if n == 0: return m
    if m == 0: return n
    INF = n + m
    H = [[0] * (m + 2) for _ in range(n + 2)]
    H[0][0] = INF
    for i in range(n + 1):
        H[i + 1][0] = INF; H[i + 1][1] = i
    for j in range(m + 1):
        H[0][j + 1] = INF; H[1][j + 1] = j
    da = {}
    for i in range(1, n + 1):
        db = 0
        for j in range(1, m + 1):
            i1 = da.get(b[j - 1], 0); j1 = db
            if a[i - 1] == b[j - 1]:
                cost = 0; db = j
            else:
                cost = 1
            H[i + 1][j + 1] = min(H[i][j] + cost, H[i + 1][j] + 1, H[i][j + 1] + 1,
                                  H[i1][j1] + (i - i1 - 1) + 1 + (j - j1 - 1))
        da[a[i - 1]] = i
    return H[n + 1][m + 1]

def dist(t, s):
    """D(t, s) = dist(Fold(t), Fold(s)); t and s are strings in any normalisation or casing."""
    return dist_units(fold_units(units(t)), fold_units(units(s)))

# ---------------------------------------------------------------- weighted, case-aware alignment
def _pair_w(a, b):
    if a[0] == b[0]: return W_DIA
    if len(a[0]) == 1 and len(b[0]) == 1 and kb_adjacent(a[0], b[0]): return W_KB
    return W_SUB
def _gap_w(u): return W_DIA if u[0] == "" else W_INS      # insert/delete of an empty-base unit is a dia edit
def _add(x, y): return (x[0] + y[0], x[1] + y[1], x[2] + y[2])

def _align(te, tf, cf, ce, inv_exact):
    """Cheapest alignment of folded typed units tf with folded candidate units cf, lexicographic cost
    (weight, edits, case).  te = exact typed units.  If ce (exact candidate units) is None, the best exact
    casing of each candidate unit is chosen (nearest casing) and returned.  Returns (cost, exact list or None)."""
    n, m = len(tf), len(cf)
    def choice(f):
        opts = inv_exact.get(f, ())
        if f in opts: return 0, f
        if opts: return 1, opts[0]
        return 0, f
    def newcase(j):
        if ce is None: return choice(cf[j])[0]
        return 0 if ce[j] == cf[j] else 1
    D = [[None] * (m + 1) for _ in range(n + 1)]; B = [[None] * (m + 1) for _ in range(n + 1)]
    D[0][0] = (0, 0, 0)
    for i in range(n + 1):
        for j in range(m + 1):
            if i == 0 and j == 0: continue
            best = None; bop = None
            if i > 0 and j > 0:
                if tf[i - 1] == cf[j - 1]:
                    c = (0, 0, 0 if ce is None else int(ce[j - 1] != te[i - 1])); op = "m"
                else:
                    c = (_pair_w(tf[i - 1], cf[j - 1]), 1, newcase(j - 1)); op = "s"
                best = _add(D[i - 1][j - 1], c); bop = op
            if i > 1 and j > 1 and tf[i - 1] == cf[j - 2] and tf[i - 2] == cf[j - 1] and tf[i - 1] != tf[i - 2]:
                cs = 0 if ce is None else int(ce[j - 2] != te[i - 1]) + int(ce[j - 1] != te[i - 2])
                c = _add(D[i - 2][j - 2], (W_TRANS, 1, cs))
                if best is None or c < best: best, bop = c, "t"
            if i > 0:
                c = _add(D[i - 1][j], (_gap_w(tf[i - 1]), 1, 0))
                if best is None or c < best: best, bop = c, "d"
            if j > 0:
                c = _add(D[i][j - 1], (_gap_w(cf[j - 1]), 1, newcase(j - 1)))
                if best is None or c < best: best, bop = c, "i"
            D[i][j] = best; B[i][j] = bop
    exact = None
    if ce is None:
        exact = [None] * m
        i, j = n, m
        while i > 0 or j > 0:
            op = B[i][j]
            if op == "m": exact[j - 1] = te[i - 1]; i -= 1; j -= 1
            elif op == "s": exact[j - 1] = choice(cf[j - 1])[1]; i -= 1; j -= 1
            elif op == "t": exact[j - 2] = te[i - 1]; exact[j - 1] = te[i - 2]; i -= 2; j -= 2
            elif op == "d": i -= 1
            else: exact[j - 1] = choice(cf[j - 1])[1]; j -= 1
    return D[n][m], exact

def case_distance(t, s, inv_exact=None):
    """Over the alignments of Fold(t) with Fold(s) of least weight, then fewest edits: the least count of
    (matched pairs whose exact units differ) + (candidate units not matched to an equal-fold typed unit that
    differ from their own fold).  Interpretation for sub/dia pairs: the candidate unit's own case only."""
    te, se = units(t), units(s)
    return _align(te, fold_units(te), fold_units(se), se, inv_exact or {})[0][2]

def nearest_casing(t, f_units, inv_exact):
    """B0: the exact string (list of units) for the folded member f_units, nearest to typed t in casing."""
    te = units(t)
    cost, exact = _align(te, fold_units(te), list(f_units), None, inv_exact)
    return cost, exact

# ---------------------------------------------------------------- the grammar's unit inventory
# HCLoader always appends two synthetic boundaries to every character table (HCLoader.cs:2710-2712; PanGloss
# pg-grammar/src/compile/chardef.rs:151-167): the null boundary with these four representations and the
# space-replacement boundary `.`.  HC never accepts them as written text in an input word (probed on Sena, WS0
# follow-up: 0 of 32 insertions each accepted, while the project-authored boundary `+` is accepted), and
# segmentation of phonemes ignores them (segment.rs `segment_phonemes_only`).  So they are not units a typed
# word can carry and not alphabet.
SYNTHETIC_BOUNDARIES = (frozenset(["^0", "*0", "&0", "\u2205"]), frozenset(["."]))

def reps_from_xml(path, keep_synthetic=False):
    """Representations that HC can write: every SegmentDefinition representation and every BoundaryDefinition
    representation except the two synthetic boundaries HCLoader appends (above).  A file with no definition
    blocks (bare Representation elements) is read as segments only."""
    txt = open(path, encoding="utf-8").read()
    rx = re.compile(r"<Representation>(.*?)</Representation>", re.S)
    blocks = list(re.finditer(r"<(SegmentDefinition|BoundaryDefinition)\b.*?</\1>", txt, re.S))
    if not blocks:
        return [html.unescape(m.group(1)) for m in rx.finditer(txt)]
    out = []
    for b in blocks:
        reps = [html.unescape(m.group(1)) for m in rx.finditer(b.group(0))]
        if b.group(1) == "BoundaryDefinition" and not keep_synthetic and frozenset(reps) in SYNTHETIC_BOUNDARIES:
            continue
        out += reps
    return out

class Grammar:
    """Units cut from concatenations of representations (segment and boundary), at most k marks."""
    def __init__(self, reps):
        self.reps = sorted({nfd(r) for r in reps if r})
        self.chunks = set()       # strings of marks that can follow a unit
        self.rep_units = []       # (unit, extendable)
        for r in self.reps:
            us = units(r)
            if us[0][0] == "":
                self.chunks.add(us[0][1])
                self.rep_units.append((("", us[0][1]), True))
                us = us[1:]
            for u in us[:-1]: self.rep_units.append((u, False))
            if us: self.rep_units.append((us[-1], True))
        self._inv = {}
    @staticmethod
    def _canon(b, s): return nfd(b + s) == b + s
    def exact_inventory(self, k):
        if k in self._inv: return self._inv[k]
        seen, queue, plain = set(), [], set()
        for u, ext in self.rep_units:
            if len(u[1]) <= k:
                if ext: queue.append(u)
                else: plain.add(u)      # an inner digraph unit must not shadow the same unit written as a segment
        while queue:
            u = queue.pop()
            if u in seen: continue
            seen.add(u)
            for c in self.chunks:
                nu = (u[0], u[1] + c)
                if len(nu[1]) <= k and nu not in seen and self._canon(*nu): queue.append(nu)
        seen |= plain
        self._inv[k] = seen
        return seen
    def _decomposes(self, rest, memo=None):
        memo = {} if memo is None else memo
        if rest == "": return True
        if rest in memo: return memo[rest]
        ok = any(rest.startswith(c) and self._decomposes(rest[len(c):], memo) for c in self.chunks)
        memo[rest] = ok
        return ok
    def writable_folded(self, fu):
        """Can the cascade write a unit whose fold is fu, at any mark depth (typed deeper stacks)?"""
        for (b, s0), ext in self.rep_units:
            if nfd(b.casefold()) != fu[0]: continue
            if s0 == fu[1]: return True
            if ext and fu[1].startswith(s0) and self._canon(b, fu[1]) and self._decomposes(fu[1][len(s0):]):
                return True
        return False
    def alphabet(self, k, typed_folded):
        """sorted folded alphabet for a typed word, and exact units by fold."""
        by_fold = {}
        for u in self.exact_inventory(k):
            by_fold.setdefault(fold_unit(u), set()).add(u)
        alpha = set(by_fold)
        for fu in set(typed_folded):
            if fu not in alpha and self.writable_folded(fu): alpha.add(fu)
        return sorted(alpha), {f: sorted(v) for f, v in by_fold.items()}

# ---------------------------------------------------------------- the ball
def d1(word, alpha):
    """word: tuple of folded units.  dict candidate(tuple) -> (type, kb).  First type in priority order wins."""
    out = {}
    def put(c, t, kb=0):
        if c != word and c not in out: out[c] = (t, kb)
    n = len(word)
    for i in range(n - 1):
        if word[i] != word[i + 1]:
            put(word[:i] + (word[i + 1], word[i]) + word[i + 2:], "trans")
    for i, u in enumerate(word):
        for a in alpha:
            if a[0] == u[0] and a != u: put(word[:i] + (a,) + word[i + 1:], "dia")
    for i, u in enumerate(word):
        for a in alpha:
            if a[0] != u[0]:
                kb = 1 if (len(a[0]) == 1 and len(u[0]) == 1 and kb_adjacent(u[0], a[0])) else 0
                put(word[:i] + (a,) + word[i + 1:], "sub", kb)
    for i, u in enumerate(word):
        put(word[:i] + word[i + 1:], "dia" if u[0] == "" else "del")
    for i in range(n + 1):
        for a in alpha:
            put(word[:i] + (a,) + word[i:], "dia" if a[0] == "" else "ins")
    return out

def valid(c):
    """A unit sequence is a string only if an empty-base unit is first: a mark-only unit anywhere else is
    just more marks on the unit before it (S.2: headless marks form a unit only at the very start)."""
    return all(u[0] != "" for u in c[1:])

def ball(word, alpha):
    """(s1, s2): dicts of folded tuples at dist 1 and dist 2 (strings only; labels from dist_units).
    dist is defined on unit sequences, so intermediate sequences of the two steps are not filtered; only the
    members are (an invalid sequence is not a string and is never output)."""
    u1 = d1(word, alpha)
    u2 = {}
    for c1, (t1, k1) in u1.items():
        for c2, (t2, k2) in d1(c1, alpha).items():
            if c2 != word and c2 not in u1 and c2 not in u2:
                u2[c2] = (t1 + "+" + t2, 1 if (k1 or k2) else 0)
    return ({c: v for c, v in u1.items() if valid(c)}, {c: v for c, v in u2.items() if valid(c)})

def casing_options(typed_exact, inv_exact, f):
    opts = set(inv_exact.get(f, ()))
    opts |= {e for e in typed_exact if fold_unit(e) == f}
    return sorted(opts) or [f]

# ---------------------------------------------------------------- TEST 32
def test32():
    """No base character that survives NFD has a full case folding ending in a combining mark.
    Enumerates every code point (str.casefold is CaseFolding.txt C+F).  Returns (checked, failures)."""
    checked, bad = 0, []
    for cp in range(0x110000):
        if 0xD800 <= cp < 0xE000: continue
        c = chr(cp)
        if nfd(c) != c or is_mark(c): continue
        f = nfd(c.casefold()); checked += 1
        if f and is_mark(f[-1]): bad.append(cp)
        elif any(is_mark(x) for x in f): bad.append(cp)     # a mark inside a folded base would split the unit
    return checked, bad

def load_research():
    p = os.environ.get("GEN_RESEARCH")
    return json.load(open(p)) if p and os.path.exists(p) else None

# ---------------------------------------------------------------- main
def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--grammar")
    ap.add_argument("--mark-depth", type=int, help="k: marks per generated unit (required, no default)")
    ap.add_argument("--words-file")
    ap.add_argument("--count", action="store_true")
    ap.add_argument("--full", action="store_true")
    ap.add_argument("--casings", action="store_true")
    ap.add_argument("--sample", type=int)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--test32", action="store_true")
    ap.add_argument("words", nargs="*")
    a = ap.parse_args()
    if a.test32:
        n, bad = test32()
        print(f"TEST 32: {n} NFD-surviving base characters checked, {len(bad)} failures"
              + ("" if not bad else ": " + " ".join(f"U+{c:04X}" for c in bad[:20])))
        sys.exit(1 if bad else 0)
    if not a.grammar: ap.error("--grammar is required")
    if a.mark_depth is None: ap.error("--mark-depth is required (the caller states the test depth)")
    if a.mark_depth < 0: ap.error("--mark-depth must be >= 0")
    if not (a.count or a.full or a.casings or a.sample): ap.error("one of --count, --full, --casings, --sample is required")
    words = list(a.words)
    if a.words_file:
        words += [l.strip() for l in open(a.words_file, encoding="utf-8") if l.strip()]
    G = Grammar(reps_from_xml(a.grammar))
    load_research()
    rng = random.Random(a.seed)
    w = sys.stdout.write
    for word in words:
        te = units(word); tf = tuple(fold_units(te)); srcname = nfc(word); nu = len(te)
        alpha, inv_exact = G.alphabet(a.mark_depth, tf)
        s1, s2 = ball(tf, alpha)
        if a.count:
            if a.casings:
                def nc(d): return sum(_prod(len(casing_options(te, inv_exact, f)) for f in c) for c in d)
                n0 = _prod(len(casing_options(te, inv_exact, f)) for f in tf) - 1   # case variants of the word itself
                w(f"{srcname}\t{nc(s1)}\t{nc(s2)}\t{n0}\n")
            else:
                w(f"{srcname}\t{len(s1)}\t{len(s2)}\n")
            continue
        def emit(items):
            for c, (t, k) in items:
                d = dist_units(list(tf), list(c))
                cost, exact = nearest_casing(word, c, inv_exact)
                rows = [(exact, cost[2])]
                if a.casings:
                    rows = []
                    for combo in itertools.product(*[casing_options(te, inv_exact, f) for f in c]):
                        rows.append((list(combo), case_distance(word, ustr(combo), inv_exact)))
                for ex, cd in rows:
                    w(f"{srcname}\t{nfc(ustr(ex))}\t{d}\t{t}\t{k}\t{nu}\t{nfc(ustr(c))}\t{cd}\n")
        k1, k2 = sorted(s1.items()), sorted(s2.items())
        if a.full or a.casings:
            if a.casings:      # D = 0: the other exact casings of the word's own fold are in B too
                for combo in itertools.product(*[casing_options(te, inv_exact, f) for f in tf]):
                    if list(combo) != te:
                        w(f"{srcname}\t{nfc(ustr(combo))}\t0\tcase\t0\t{nu}\t{nfc(ustr(tf))}\t{case_distance(word, ustr(combo), inv_exact)}\n")
            emit(k1); emit(k2)
        else:
            h = a.sample // 2
            emit(rng.sample(k1, min(h, len(k1)))); emit(rng.sample(k2, min(a.sample - h, len(k2))))

def _prod(it):
    p = 1
    for x in it: p *= x
    return p

if __name__ == "__main__":
    main()
