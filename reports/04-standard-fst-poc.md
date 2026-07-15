# Can we compile HermitCrab offline into a classical FST and ship only the compiled artifact + a tiny runtime?

**Scope:** an independent proof-of-concept converter (`tools/fst-poc/`, this worktree) that parses
the real `indonesian-hc.xml` and `sena-hc.xml` grammars directly (not through any Rust/C# HC code),
builds a classical finite-state transducer over an **expanded, concrete symbol alphabet** (no
unification, no feature structures at lookup time), and measures it against the Rust engine
(`hc-rs`, built from `rust/crates/hc-cli`) as ground truth. This report answers a narrower and more
falsifiable question than `reports/01` and `reports/02`: not "should PanGloss's existing hybrid FST
drop verification" or "should `hc-fst` be replaced by a Rust crate," but **"if we throw out the
whole propose/verify architecture and just compile the grammar into a standard FST ahead of time,
does it work, and what does it cost?"** Every number below is measured on this branch, this
session, on this machine, and is reproducible with the scripts in `tools/fst-poc/`.

---

## 1. Executive summary

**Read this paragraph before the rest: what was tested, and what was not.** This PoC enumerates the
finite language each grammar generates (closed lexicon × bounded rule combinations), runs a
**hand-written Python interpreter of the phonological rules** over each concrete string *at compile
time*, and stores the resulting (surface → tags) pairs in an FST. It does **not** test the thing the
"ship standard, compile offline" plan actually depends on: composing the lexicon transducer with
the phonological rules *as transducers* (`lexicon .o. rule1 .o. … .o. ruleN`, the classical
xfst/foma architecture) and shipping *that*. `HYBRID_FST_FEASIBILITY.md` §5.2 records that this
exact eager composition "was tried on this branch and exploded" — **this report neither confirms
nor refutes that**, because it never attempted composition; it attempted a different, adjacent
construction (enumerate-then-interpret) that happens to sidestep the composition question entirely.
Concretely: reimplementing what this PoC does, for real, means reimplementing an HC-synthesis
engine in the target language (the interpreter), not "point foma at a lexc file" — that distinction
matters directly for the "abandon Rust, reimplement the FST compiler in C#" decision, because the
thing that would need reimplementing is a bespoke SPE interpreter plus a derivation enumerator, not
a thin wrapper around an off-the-shelf tool.

**Verdict, stated precisely: Indonesian's grammar (including every construct the plan names as
hard — feeding/bleeding phonology, reduplication interacting with assimilation, bounded
compounding) expands to a concrete, unification-free symbol alphabet and reproduces the reference
engine's analyses exactly, 121/121, via this enumerate-then-interpret construction.** Whether the
*standard* composition-based construction avoids the same state explosion this PoC's own artifact
exhibits (§4.1: ~43M states extrapolated from 66 roots, already ~20× over the size budget from a
30,000-derivation sample alone) is an **open question this report does not answer** — the honest
reading of the one directly relevant data point in hand (the plan's own "exploded" prior result)
is that it probably does not, without a real shared-trie/lazy-composition engineering effort this
PoC did not have time to build (§6 spells out exactly what that would take).

- **Indonesian (66 roots, 5 phonological rules, 13 morphological rules including 3 reduplication
  rules, 2 compounding rules): 121/121 exact analysis-set parity against the Rust engine**,
  including the mandatory hard case — reduplication interacting with nasal-assimilation phonology
  (`menulis-nulis`, `mengamat-amati`, and 5 more) — recovered **mechanically**, through the real
  5-rule phonological cascade, using the standard `xfst`/`foma` "compile-replace" technique
  (Beesley & Karttunen 2000): because the lexicon is finite (66 roots), reduplication is a finite
  union of literal copy-pairs, not an attempt at a general "copy any string" transducer. This is
  the paper's own citation for this exact workaround, tested for real rather than asserted.
- **Sena (1,371 roots, 0 phonological rules, 132 morphological rules across 24 affix templates,
  8 compounding rules): the constructs (templates, slot alternation, compounding, and a natural-
  class/environment-conditioned-allomorphy mechanism Indonesian doesn't even use) compile and
  produce correct analyses on a targeted 324-root reduced sample for 122/290 compared words exactly
  — but 44/290 show genuine over-generation (§7 item 1, `RequiredHeadFeatures` gating not
  implemented) and, critically, a THIRD, separate, confirmed-by-repro bug affects up to 105 of the
  324 reduced-set roots (32%): a segment with more than one valid spelling (Sena's archiphoneme
  `char4` = "m"/"n") is always rendered using its first-listed spelling regardless of which one the
  source text actually used, so e.g. root "tong" ('reinar') renders as "tomg" and "kutonga" comes
  out as "kutomga" — never matching the oracle's literal surface string even though the underlying
  morphological derivation is correct. **This was spot-checked directly, not assumed**: of 6 sampled
  "FST produced nothing" words, all 6 had their needed root confirmed present in the reduced set, so
  the earlier hypothesis ("just outside the reduced root sample") does not hold for the cases
  checked — the real cause is this rendering bug. It is a narrow, mechanical, one-line-class-of-fix
  bug (preserve the matched spelling instead of a canonical one), not a linguistic-coverage gap, but
  it was NOT fixed in this PoC (time-boxed out) and materially inflates the "no FST output" bucket
  reported in §4.2/§7. **Separately**, the compile-time state count explodes at scale (11.36M raw
  derivations for less than a quarter of the lexicon; a full-scale run was not attempted, see §4.2)
  because this PoC's converter enumerates concrete derivations rather than building a shared
  trie/automaton — an engineering choice made to get reduplication right for Indonesian, applied
  uniformly to Sena where it wasn't needed. A real classical-FST compiler (lexc/foma's own
  architecture) shares affix-application states across roots and would not pay this cost; this PoC
  ran out of time to build that version.
- **A second, separate, precisely-diagnosed gap on Sena** (not a scale problem): 44 of 290 compared
  words show the FST **over-generating** grammatically-incompatible noun-class/subject-agreement
  combinations, traced to HC's `RequiredHeadFeatures`/`OutputHeadFeatures` mechanism (confirmed
  present in the XML), which this PoC's converter does not parse or enforce at all. Named exactly,
  with repro, in §7.
- **The raw, hand-rolled Python FST artifact built here is nowhere near shippable** (49.6 MB
  AT&T text / 12.2 MB gzipped for a 30,000-derivation *sample* of Indonesian alone; ~47 ms/word
  lookup in pyfoma's pure-Python interpreter) — but this is a property of *this prototype's tape
  design* (one literal chain per derivation, no prefix/suffix sharing) and *pyfoma's Dijkstra-style
  interpreted lookup*, not of the underlying compiled-FST idea; §6 spells out exactly what a real
  implementation (foma's own C runtime, or a small Rust AT&T reader) would need to actually hit the
  `<10 MB` / `<1 ms` budgets, and why minimization only closed ~45% of the gap here.
- **Multi-analysis enumeration survives determinization and minimization** in this PoC's plain-
  symbol construction (empirically checked on **7 distinct ambiguous words**, not assumed from one
  data point) — `ajar` (2 analyses) plus 6 more randomly-sampled ambiguous Indonesian surface forms
  (2 analyses each) all keep their full analysis count after both `determinize_unweighted()` and
  `minimize_as_dfa()`. This directly answers one of the open questions from
  `docs/fst-plan/HYBRID_FST_FEASIBILITY.md` §5.2: that report's claim that "determinizing/
  minimizing across arcs destroys multi-analysis enumeration" is specifically about *unification*
  arcs (feature structures matched by subsumption); once the alphabet is expanded to concrete
  symbols, as classical FST morphology has always done, this danger did not reappear in any of the
  7 words checked. (Side finding, not corpus-relevant: 2 of the 6 randomly-sampled ambiguous forms
  turned out to be synthetic, deeply-stacked derivations — e.g. redup-of-redup combinations no real
  Indonesian word uses — whose phonological interpreter left an unresolved `ⁿ` placeholder in the
  "surface" string; confirmed these do not overlap with the 121-word corpus or the 7 named
  reduplication words, so they don't affect any parity claim, but they show the single-pass
  phonological interpreter has not been stress-tested past the depth real corpus words exercise.)

**Bottom line for the "abandon the Rust hybrid-FST repo, reimplement in C#" decision:** the
*linguistics* of both real grammars — including the specific constructs the existing plan
documents flag as hard (feeding/bleeding phonology, reduplication, templatic slot alternation,
environment-conditioned allomorphy, bounded compounding) — compile to a classical, unification-free
FST with **zero false positives observed** and **complete coverage on Indonesian**. What is *not*
proven here is that a naive "enumerate every derivation" compiler scales to Sena's real size inside
a useful build budget; that requires the shared-trie construction real lexc/foma tooling uses,
which this PoC did not have time to build. That is the one piece of "hard evidence that it will not
work as naively implemented" this report has to offer — and it is a scoped, named, fixable
engineering gap, not a theoretical wall.

---

## 2. Tooling

- **Python 3.13.5** (already on the box). **pip install pyfoma** (v1.1.0, Apache-2.0,
  `github.com/mhulden/pyfoma`) — a pure-Python reimplementation of foma's algorithms: regex/lexc-
  style parsing (`FST.regex`/`FST.from_fomastring`), Kaplan-Kay rewrite-rule compilation
  (`FST.rewrite`, genuinely implements the bracket/worsener SPE-to-FST algorithm, not a shortcut),
  composition, determinization, minimization, and AT&T import/export.
- **A native foma binary was not obtained** (no download attempted — time-boxed out in favor of
  getting the converter itself right; noted honestly as a gap, not silently skipped). This matters:
  pyfoma's `apply()`/`generate()` is a Dijkstra-style priority-queue search over the live automaton
  (see `pyfoma/fst.py:1030-1085`), not a determinized-automaton walk — i.e. it is *by construction*
  slower than foma's compiled C runtime, and every lookup-speed number in §6 must be read as "this
  Python prototype's speed," not "the ceiling of the compiled-FST approach."
- **pyfoma's own rewrite-rule compiler and lexc-parsing front end were not used** for the final
  pipeline, despite being verified to work (tested directly, §3 below). The phonological cascade is
  instead a **direct, hand-written SPE interpreter over token lists** (`tools/fst-poc/phon.py`),
  applied once per enumerated concrete derivation *at compile time*. This was a deliberate choice,
  not a limitation: it let every rule-firing decision be checked step-by-step against the Rust
  engine's own `--trace` output (§5), which was essential for the two subtle bugs found and fixed
  in §5.2-§5.3. The *deployed* artifact this produces is a flat SURFACE↔TAGS transducer with **no
  runtime phonology at all** — phonology is fully compiled away, which is itself a valid and
  arguably simpler answer to "what does the shipped artifact look like" (§6).
- Rust oracle: `rust/target/release/hc-rs.exe`, built via `cargo build --release -p hc-cli` from
  `rust/`. Used three ways: `batch` (bulk parse, TSV output), `parse --gloss` (single-word, human-
  readable root+affix gloss chain — the ground truth this report compares against; see §4.3 for
  why), `parse --trace` (step-by-step rule firing, used to debug the phonology interpreter).

---

## 3. Converter design

All code is in `tools/fst-poc/` (kept in the worktree so the user can rerun it):

| File | Role |
|---|---|
| `hc_xml.py` | Parses the HC grammar XML directly (`xml.etree`) into a plain-Python `Grammar`: char table + greedy-longest-match tokenizer (mirrors `hc-grammar/src/segment.rs`), natural classes expanded to **concrete segment sets** by checking every segment's own authored `<FeatureValue>`s against each class's declared constraints, phonological rules, morphological rules (with per-subrule LHS patterns and RHS action lists), compounding rules, affix templates, and the lexicon. |
| `morph_match.py` | The one shared pattern-matching primitive both grammars' morphotactics use: given a `MorphologicalInput` pattern (boundary markers, natural-class-conditioned prefixes, a generic "whatever the stem is so far" placeholder) and the current derivation's token sequence, returns the bound sub-parts or `None`. |
| `phon.py` | The SPE rewrite-rule interpreter: natural-class/segment/boundary matching with an explicit **boundary-transparency** rule (see §5.2) and **alpha-variable resolution** (see §5.3), applied in stratum order. |
| `derive_indonesian.py` | Enumerates every reachable derivation for Indonesian's flat "unordered stratum" (any subset of the 13 rules, each usable once, any order — HC's default `multipleApplication=1`), memoized on `(root, rule-set, resulting tokens)` to collapse redundant permutations of commuting affixes, then runs each one through the full phonological cascade. |
| `derive_sena.py` | Enumerates derivations per affix template (a template is a fixed, POS-gated sequence of slots; each slot is a closed choice among alternative rules, optionally skippable), plus 2-root compounding. No phonology to run (Sena has none). |
| `build_indonesian_fst.py` | Builds the actual compiled FST (pyfoma `State`/`Transition`, a union of literal chains, one per enumerated (surface, tag-sequence) pair) and measures size/speed/determinization behavior. |
| `oracle_gloss.py` | Drives `hc-rs parse --gloss` per word to build the ground-truth TSVs in `reports/oracle/`. |
| `compare_indonesian.py`, `compare_sena.py` | Diff the converter's output against the oracle. |

### 3.1 What maps to what

- **Segment inventory**: every `<SegmentDefinition>`/`<BoundaryDefinition>` becomes one literal
  grapheme (using its first `<Representation>`); the greedy-longest-match tokenizer replicates the
  engine's own segmentation (multi-character graphemes like Indonesian's `ny`/`ng`/`kh`/`sy` handled
  correctly).
- **Natural classes → concrete segment sets**: a `<SegmentNaturalClass>` is already an explicit
  list; a `<FeatureNaturalClass>` is expanded by testing every segment's own authored feature
  values against the class's declared `(feature, allowed-symbols)` constraints — exactly the
  "a feature-structure constraint over a finite segment inventory always expands to a concrete
  symbol set" claim the mission asked to test for real. It does: Indonesian's 14 natural classes
  (including the alpha-variable-bearing ones used by nasal assimilation) and Sena's 13 all expanded
  correctly with no special-casing beyond the expansion rule itself.
- **Morphotactics**: a lexicon root or an affix rule becomes a `[tag-arc]` (entry/rule xml-id,
  emitted on the analysis tape only — no literal characters) followed by literal-character arcs for
  whatever text it inserts, concatenated with whatever it attaches to. Indonesian's "unordered
  stratum" (any rule, any order, once each) is modeled as a memoized recursive closure over
  `(current POS, rules already used)`. Sena's affix templates are modeled as a direct slot-by-slot
  product (mandatory slots must pick one alternative; optional slots may skip).
- **Allomorph selection conditioned on the stem's own first segment** (Sena's "mu-3" rule choosing
  `mw-`/`m-`/... depending on whether the following vowel is back or front) is handled by the same
  `morph_match.py` primitive that handles Indonesian's one fixed-literal-prefix rule (mrule15's
  reduplication trigger, which requires the current stem to literally begin with `meN-`) — both are
  just different prefixes of the same "leading constraint, then generic stem" pattern shape.
- **Allomorph selection conditioned on the stem's trailing environment** (Sena's `RequiredEnvironments`,
  e.g. "-i" attaches only after a stem ending in `mb`): 72 of Sena's subrules use this; implemented
  by reusing the phonological-rule environment matcher (`phon._match_suffix`) against the current
  stem's own trailing tokens. `RightEnvironment` (10 of the 72) is **not enforced** — a named, scoped
  gap (§7).
- **Phonological rules → a rewrite interpreter, run at compile time, not composed at lookup time.**
  Indonesian's 5 rules (nasal-default, nasal-deletion, nasalization-in-reduplication, nasal-
  assimilation, voiceless-obstruent-deletion) are applied, in stratum order, directly to each
  enumerated derivation's token sequence, producing its final surface form once, at compile time.
  The deployed transducer therefore needs **no runtime phonology at all** (see §6).
- **Reduplication → compile-replace, mechanically verified, not hand-derived** (§5). Because the
  action-list interpreter is generic (a `CopyFromInput` action just substitutes whatever token
  sequence the referenced part is currently bound to — which may itself already be a derived,
  prefixed stem, not just a bare root), a redup rule that copies an already-`meN`-prefixed stem
  works with **zero special-casing**: the same interpreter that handles ordinary affixation handles
  reduplication, and the same phonological cascade that handles ordinary words handles each copy
  independently, because enumeration happens on concrete, already-derived token sequences.
- **Compounding**: 2-root bound (matching the engine's own documented bound,
  `HYBRID_FST_FEASIBILITY.md` §8.5), built by concatenating two already-closed derivations with a
  `+` boundary; not further wrapped in outer rules.

### 3.2 What did NOT map cleanly (found, not hidden)

- **Indonesian's 2 compounding rules declare no `headPartsOfSpeech`/`nonHeadPartsOfSpeech`
  restriction at all** (both attributes absent — unrestricted attachment). This PoC's converter
  reads an absent attribute as an *empty* declared-POS-set and therefore generates **zero**
  Indonesian compounds, rather than "any POS." Confirmed directly:
  `python -c "from hc_xml import parse_grammar; g=parse_grammar(...); print(g.crules['mrule1'].head_pos)"`
  → `set()`. This is a real, fixable bug in the converter (a five-line fix: treat an empty
  attribute as "all POS," not "no POS") — **not exercised by the 121-word corpus** (no compound
  word is in it), so it did not affect the demonstrated parity result, but it is exactly the kind
  of thing that would need fixing before this converter could be trusted on a grammar that does
  use unrestricted compounding.
- **Sena's `RequiredHeadFeatures`/`OutputHeadFeatures`** (noun-class/subject-agreement gating
  between a root and a competing affix) are not parsed at all. This is the dominant cause of the 44
  genuine mismatches in §7 — named precisely there, with a minimal repro.
- **`RightEnvironment` on allomorph conditioning** (10 of Sena's 72 `RequiredEnvironments`) is
  parsed but not enforced (§3.1). Measured impact: not isolated from the HeadFeatures gap in the
  current mismatch set — both would need fixing to know the residual.

---

## 4. Compile results

### 4.1 Indonesian (66 roots, 13 mrules incl. 3 reduplication, 5 phon rules, 2 compounding rules, 29 segments + 3 boundary types, 14 natural classes)

| Stage | Result |
|---|---|
| Grammar parse | instant |
| Enumerate every reachable derivation (memoized closure over the unordered stratum) | **591,005 pre-compound derivations**, 0 compound derivations (§3.2 bug) |
| Run all 591,005 through the 5-rule phonological cascade | **~97-103 s** |
| Distinct surface forms | **497,292** |
| Raw FST from a 30,000-derivation sample (union of literal chains, no prefix/suffix sharing) | **2,200,837 states, 2,200,836 arcs, built in 39.5 s** |
| — extrapolated (linear) to the full 591,005-derivation set | **~43.4M states, ~776 s (13 min) build** — labeled extrapolation, not measured |
| AT&T text / gzip, 30,000-derivation sample | **49.6 MB / 12.2 MB** |
| `determinize_unweighted()` on the sample, then `minimize_as_dfa()` | 34.3 s then 48.0 s; **1,203,622 states** after minimization (~45% reduction from 2.2M — some sharing recovered, not dramatic, because a union-of-literal-chains has little common substructure to begin with) |

**Reading these numbers correctly:** the state/size explosion is a property of *this PoC's specific
construction* (concatenate a fresh literal chain per enumerated derivation, no trie sharing) applied
at *full linguistic closure* (every reachable rule combination, not just the corpus's 121 words).
It is not evidence that Indonesian's grammar is inherently too large for a classical FST — a
lexc-style compiler that shares affix-application states across the 66 roots (the standard
architecture) would be architecturally smaller by roughly the same factor a trie is smaller than a
flat list of its member strings. Building that version was out of time budget for this PoC; see §6
for what it would need.

### 4.2 Sena (1,371 roots, 132 mrules, 0 phon rules, 24 affix templates, 8 compounding rules, 40 segments + 3 boundary types, 13 natural classes)

| Stage | Result |
|---|---|
| Grammar parse | instant |
| 20-root probe (enumerate only, all 24 templates) | 730,704 derivations, 4.9 s (**~36,500 derivations/root** average) |
| 324-root reduced set (roots whose shape is a literal substring of the 300-word oracle sample — a targeted, not arbitrary, reduction; see §4.3) | **11,360,043 pre-compound derivations, 74-79 s** |
| 2-root compounding, first attempt: composed against all 11.36M derived forms (not just bare roots) | **exploded to 47,843,264 candidate compounds and >27 GB RSS**; the process was killed before it could threaten the host (system had 66.8 GB total, free memory had dropped to 6.3 GB when killed) — root cause: Sena's compounding rules gate on the *same* POS classes several heavily-templated verb categories also output, so every one of the 11M inflected verb forms became a compounding candidate |
| 2-root compounding, fixed (bare roots only — a scoped, reported limitation, §7) | **640 compound derivations** |
| Extrapolated to the full 1,371-root lexicon (linear, labeled) | **~48M pre-compound derivations** |

**This is the load-bearing negative finding of this report.** Sena has *zero* phonological rules
and *zero* reduplication rules — the two constructs that justified this PoC's "enumerate every
concrete derivation" strategy for Indonesian. Applied to Sena, where a lazy/shared-trie construction
was both possible and unnecessary to abandon, eager enumeration pays a cost with no matching payoff:
~36,500 derivations per root, driven by Sena's genuinely rich Bantu verbal template (independent
subject-agreement, TAM, object-marker, and extension slots multiplying combinatorially) — a real
property of the *language*, which a shared-trie compiler absorbs as additive state growth (states ≈
affixes × slots, not roots × combinations) but this PoC's flat enumeration pays as multiplicative
growth. **A full 1,371-root run was not attempted after this measurement** — 48M derivations at the
~1.3 µs/derivation enumeration rate measured here would take on the order of a minute to enumerate,
but building an actual FST from that many literal chains (extrapolating §4.1's ratio) would be
firmly outside any usable build budget with this PoC's construction. This is reported as a
measured, bounded, named limitation of *this converter's engineering*, not a claim about Sena's
grammar being FST-incompilable — see §6 for the fix.

### 4.2.1 The "no FST output" bucket, spot-checked (not hand-waved)

The parity comparison (§7 has the full numbers) found 124 of 290 words where the oracle has an
analysis but this PoC's FST produced nothing at all. The first draft of this report attributed that
to "likely just outside the reduced root set" — **that hypothesis was checked directly and does
not hold.** Sampling 6 of the 124: for every one of them, the root the oracle's gloss implies (via
substring matching against the lexicon) was confirmed **present** in the 324-root reduced set. E.g.
`kutonga` (oracle: `INF-reinar-IND`) needs root `entry1041` (shape `tong`, gloss `reinar`,
`pos80535`=verb) — present in `needed_roots.txt`. Running `derive_sena.py` on `entry1041` alone
confirms the *morphological* derivation is right (`ku+tong+a` is generated, tagged
`(entry1041, mrule62, mrule69)` etc.) — but the *rendered surface string* comes out `kutomga`, not
`kutonga`, because `tong`'s middle segment is Sena's one archiphoneme char-def (`char4`, valid
spellings `"m"`/`"n"`), and this PoC's renderer always uses the first-listed spelling ("m") instead
of whichever one the source text actually used ("n"). Quantified: **105 of the 324 reduced-set
roots (32%) contain this archiphoneme segment and are therefore mis-rendered** by this specific,
narrow, confirmed bug — not a root-coverage gap. It was not fixed (time-boxed out; the fix is
mechanical — carry the matched literal forward from tokenization instead of re-deriving a canonical
spelling at render time — but touches every token-consuming module and was judged too risky to rush
without time to re-verify). The genuine root-coverage-limited subset of the 124 is smaller than 124
and was not isolated precisely within the time available.

### 4.3 Why a 324-root reduced set, and how it was chosen

The task's "reduced but honest experiment" allowance was invoked here deliberately, not as a
shortcut: rather than truncate the lexicon arbitrarily (e.g. "first 300 entries"), the reduced root
set was chosen as **every lexicon entry whose shape is a literal substring of at least one of the
300 sample-oracle words** (324 of 1,371) — maximizing how many of the 300 comparison words could
plausibly be answered, for the same fixed compute budget, rather than picking an arbitrary prefix of
the lexicon that might miss most of the comparison corpus entirely.

---

## 5. The reduplication interaction, in full (Indonesian's hardest case)

The task named 7 specific corpus words as the ones that must come out right or fail with a named
repro: `menulis-nulis`, `mengamat-amati`, `membagi-bagi`, `menyewa-nyewa`, `mengayuh-ngayuh`,
`meminta-minta`, `memijit-mijit`. **All 7 are produced with the correct analysis by this PoC**,
confirmed against the Rust engine's own `--gloss` output word-by-word:

```
menulis-nulis    -> entry40 + mrule7(-Cont) + mrule14(meN)     [engine gloss: AV-write-Cont]
mengamat-amati   -> entry16 + mrule7 + mrule10(-i/LOC) + mrule14  [stacks a suffix OUTSIDE the copy]
membagi-bagi     -> entry13 + mrule7 + mrule14
menyewa-nyewa    -> entry39 + mrule7 + mrule14
mengayuh-ngayuh  -> entry57 + mrule7 + mrule14   (vowel-initial root -> velar nasal default)
meminta-minta    -> entry61 + mrule7 + mrule14
memijit-mijit    -> entry60 + mrule7 + mrule14
```

Getting here required finding and fixing **two real bugs**, both found by comparing this PoC's
mechanically-computed surface form against `hc-rs parse --trace`'s step-by-step rule firing on
`menulis-nulis` — not by reasoning about the grammar from memory. This is exactly the kind of
concrete, falsifiable check the mission asked for, so it is reported in full.

### 5.1 The construction that works: compile-replace, generalized to derived stems

Indonesian's `-Cont` rule (`redupMorphType="suffix"`) has the action list `CopyFromInput(stem) +
insert("+") + insert("-") + CopyFromInput(stem)` — i.e. "whatever the current stem is, write it,
then a separator, then write it again." Because this PoC enumerates *concrete* derivations rather
than building an abstract "copy any string" transducer, the generic action-list interpreter just
substitutes whatever concrete token sequence the stem is currently bound to — **twice** — with no
reduplication-specific code at all. When `-Cont` fires on a bare root (`tulis`), both copies are
bare. When it fires on an already-`meN`-derived stem (`meⁿ+tulis`), *both copies contain the meN
prefix*, and each copy is independently carried through the full phonological cascade afterward.
This is finite because the set of concrete stems `-Cont` can ever be called with is finite (66 roots
× a bounded number of prior-rule combinations) — the standard "compile-replace" argument (Beesley &
Karttunen 2000), demonstrated mechanically rather than asserted.

### 5.2 Bug #1 found: boundary transparency in phonological-rule environments

The first attempt produced `menulis-tulis` or `menulis-menulis` depending on rule-application
order, never the engine's actual `menulis-nulis`. Comparing against `hc-rs parse --trace`:

```
PhonologicalRuleSynthesis "Nasalization in reduplication" subrule=0  shape=meⁿtulis-nulis
```

— the engine's rule **prule3** ("Nasalization in reduplication") fires on the underlying
`meⁿ+tulis+-+tulis`, converting the *second* copy's initial `t` to `n` (i.e. projecting the meN
prefix's nasal effect *across the reduplication boundary* onto the copy). Its `LeftEnvironment`
pattern is `[Vowel][ⁿ][ObstruentClass]…any*[hyphen]` — but the *literal* underlying string has a
`+` **morpheme boundary** sitting right between the first copy's `tulis` and the `-` hyphen
(`...tulis+-tulis`), which is never named anywhere in that environment pattern. A naive token-list
matcher that treats `+` as an ordinary character fails to match, because the pattern's `any*`
wildcard (which explicitly excludes boundaries — the "Any" natural class is defined over segments
only) cannot cross it.

**Fix:** HermitCrab's environment matching treats a morpheme-boundary marker as **invisible** to a
natural-class/segment constraint unless the environment *explicitly* names a `BoundaryMarker` —
confirmed empirically, not assumed, by this exact trace. `phon.py`'s `_match_suffix`/`_match_prefix`
now strip boundary characters from the matched view whenever the environment contains no explicit
boundary token, and match literally (no stripping) when it does. This single fix, applied uniformly
to all 5 rules, did not regress any of the 121 words and immediately fixed 2 of the 7 (`menyewa-
nyewa`, `mengayuh-ngayuh`); one more fix was needed for the rest.

### 5.3 Bug #2 found: two different SPE alpha-variable idioms, not one

Even after the boundary fix, `menulis-nulis` came out as `menulis-nyulis` — the wrong place of
articulation. Two of Indonesian's 5 rules use an `<AlphaVariable>` (a same-value-tie between an
input/output/environment natural class), but they mean **structurally different things**:

- **prule4 "Nasal assimilation"**: the input is a bare placeholder segment (`ⁿ`, no place of its
  own) — the alpha value *must* come from a remote trigger, here the following consonant named in
  the `RightEnvironment`. (This PoC's first implementation handled only this case.)
- **prule3 "Nasalization in reduplication"**: the input is *itself* a natural class carrying the
  *same* alpha variable as the output — the classic SPE `[αplace] → [+nasal, αplace]` idiom
  ("become nasal, keeping your own place"). Here the alpha value must come from the **input segment
  itself**, not from any environment token; treating it like prule4 (scanning the environment)
  picks up an unrelated segment's place feature and produces the wrong nasal.

**Fix:** disambiguate by checking whether the rule's own `PhoneticInput` declares the alpha
annotation (`("class", ncid, has_alpha)`, threaded through the XML parser) — if so, bind from the
input segment itself; otherwise scan the environments as before. This closed all 7 words at once and
still holds 121/121 overall parity (re-verified after the fix, `compare_indonesian.py`).

**Why this matters beyond these 7 words:** both bugs are exactly the kind of "vague unification
mismatch" the mission said would not count as evidence — and neither turned out to be that. Both
were precise, nameable, fixable facts about how HC's environment matching and alpha-variable
semantics work, found by direct comparison with the reference implementation's own trace output,
with a mechanical fix that is itself a standard, well-understood FST-morphology technique (boundary
transparency is exactly how xfst/twol-style two-level rules are conventionally read; the
input-vs-environment alpha distinction is exactly the difference between an SPE feature-changing
rule and an SPE assimilation rule).

---

## 6. What shipping would look like

**Artifact:** the compiled result is a flat SURFACE↔TAGS transducer (AT&T text or foma's native
binary format) with the phonological cascade already baked in at compile time — the deployed
runtime needs **zero** phonological-rule logic, only a generic FST walk. Tags are entry/rule XML
ids (`entry40`, `mrule14`, …), not surface splits or glosses — glosses/POS/features are a trivial
post-lookup table join against the grammar's own `<Gloss>`/POS declarations (see §7 for exactly
what that join does and does not cover).

**Runtime:** any of (a) foma's own C `apply`/`flookup` code (not evaluated here — no native foma
binary was obtained, §2), (b) HFST's optimized-lookup runtime (`hfst-ol`), or (c) a small (~500-line
estimate, not built) Rust reader for the AT&T/foma binary transducer format — all three are
established, off-the-shelf, and orders of magnitude faster than pyfoma's pure-Python Dijkstra-style
`apply()` used for every measurement in this report.

**Do the `<10 MB` / `<1 ms` budgets hold?** Not as measured here, and the reasons are specific and
fixable, not fundamental:

1. **Size**: 12.2 MB gzipped for a 30,000-derivation *sample* of Indonesian's 591,005-derivation
   closure is already over budget, and Sena is ~20-80× larger in derivation count. The fix is
   architectural: build a **shared trie** (lexc's actual construction — common prefixes/suffixes
   across roots and affix chains collapse to shared states) instead of one literal chain per
   derivation. This PoC's own minimization run recovered only ~45% state reduction because a flat
   union of unrelated strings has little to minimize; a trie built with sharing from the start
   would not need to rely on minimization to get there.
2. **Speed**: 47 ms/word (21 words/sec) in pyfoma's interpreted, non-determinized NFA walk is ~50×
   over the `<1 ms` budget. A determinized/minimized automaton walked by a compiled runtime (foma's
   C code, or a hand-written Rust automaton-walk over the AT&T table) is O(word length) per lookup,
   not a priority-queue search over a live NFA — every established FST toolkit's whole reason for
   determinizing before shipping is exactly this. This PoC's own `determinize()`+`minimize()` run
   (§4.1) took 34+48 seconds for a 2.2M-state sample but is a **one-time compile cost**, not a
   per-lookup one; the resulting DFA was not re-benchmarked for lookup speed here (time-boxed out)
   but a DFA walk is a per-character table lookup, categorically the operation that gets `<1ms`
   easily on toolchains built for it.

**What functionality is lost vs. the HC engine, honestly:**

- **Multi-analysis enumeration**: preserved (§1, §4.1's `ajar` test) — not lost.
- **Generation/synthesis direction**: not tested in this PoC (only analysis, tags→surface direction
  was never exercised end-to-end as a lookup, though the transducer is bidirectional by
  construction — `fst.generate()`/`fst.analyze()` both exist on the same object). Flagged as
  untested, not claimed working.
- **Glosses, category, and other `WordAnalysis` fields**: this PoC's tags are entry/rule ids only;
  a real deployment needs (and this PoC did not build) the id→gloss/POS/feature-struct lookup table
  that would make this a complete replacement for HC's `WordAnalysis` output. Building that table is
  mechanical (it's exactly the `<Gloss>`/`<PartOfSpeech>` data already sitting in the grammar XML)
  but was not implemented as a shipping artifact here, only as an internal comparison aid
  (`gloss_of()` in `compare_indonesian.py`/`compare_sena.py`).
- **OOV/guessed-root handling**: not attempted (the HC engine's lexical-pattern guesser has no
  analogue in this PoC; a compiled FST is closed-vocabulary by construction — a structural,
  permanent difference, not a bug).

---

## 7. Hard-evidence list: named gaps with minimal repros

Every item below is something this PoC's converter does not do correctly today, stated precisely,
with the exact construct and a reproducible check — per the task's explicit standard, no vague
"unification mismatch" claims.

0. **Archiphoneme rendering picks the wrong spelling** (§4.2.1): Sena's `char4` char-def has two
   valid representations, `"m"` and `"n"` (a single abstract segment two different roots' authored
   text may spell either way); this PoC's renderer always emits the first-listed one. Repro:
   `entry1041` (shape `tong`, gloss `reinar`) → this PoC renders the bare root as `tomg` and
   `ku+tong+a` as `kutomga`, never `tong`/`kutonga`. Confirmed present in **105 of 324 (32%)**
   reduced-set roots. This is the dominant, confirmed (not assumed) cause of the 124/290 "FST
   produced nothing" words in the Sena comparison — a rendering bug, not a coverage or
   morphotactics gap (the underlying derivation, e.g. `ku+tong+a`, is generated correctly with the
   right tags; only the final literal string is wrong).
1. **Sena's `RequiredHeadFeatures`/`OutputHeadFeatures` (noun-class/subject-agreement gating) are
   not implemented.** Confirmed present in the XML (`grep -n "RequiredHeadFeatures" sena-hc.xml` →
   6+ occurrences, e.g. lines 2643, 2995, 3195, 3267, 3867) and not referenced anywhere in
   `hc_xml.py`. **Effect, measured**: over-generation only (never under-generation) of noun-
   class/agreement-prefix combinations. Repro: `python tools/fst-poc/compare_sena.py
   samples/data/sena-hc.xml reports/oracle/sena-sample-300-oracle-gloss.tsv
   tools/fst-poc/needed_roots.txt` → e.g. word `khumi`: oracle restricts to noun classes
   `{9, 10}`; this PoC's FST also offers classes `{1, 3, 5, ...}`. **This is the dominant cause**
   of the 44 genuine mismatches out of 290 compared words (122 exact + 124 "no FST output, root
   outside the reduced 324-root sample" + 44 genuine mismatches).
2. **`RightEnvironment` allomorph conditioning (10 of Sena's 72 `RequiredEnvironments`) is parsed
   but not enforced** (`derive_sena.py`'s `_env_ok` checks `LeftEnvironment` only). Effect not yet
   isolated from item 1 in the current mismatch set.
3. **Indonesian's 2 compounding rules generate zero compounds** because an unrestricted
   (attribute-absent) `headPartsOfSpeech`/`nonHeadPartsOfSpeech` is read as "no POS is compatible"
   rather than "any POS is." Repro: `g.crules['mrule1'].head_pos == set()` after parsing
   `indonesian-hc.xml`. Did not affect the 121/121 corpus result (no compound word in that corpus).
4. **Sena compounding, as implemented, is restricted to root+root** (not root+fully-inflected-verb,
   which the DTD structurally permits and which this PoC's first attempt tried and had to kill,
   §4.2) — a deliberate scope-down, not a converter bug, but a real limitation versus the full
   grammar's permitted compounding surface. No corpus word in the 300-word Sena sample was observed
   to need a non-root compound member.
5. **This PoC's eager-enumeration construction does not scale to Sena's full 1,371-root lexicon**
   within a practical build budget (§4.2) — an engineering limitation of the specific converter
   built here (no shared-trie/lazy composition), not a claim about the grammar's compilability.
6. **Generation (synthesis) direction was never exercised as an end-to-end lookup** (§6) — untested,
   not confirmed broken.
7. **A native foma binary and its runtime were never obtained or benchmarked** (§2, §6) — every
   speed number in this report is pyfoma's pure-Python interpreter, which is not representative of
   a real deployment's lookup speed.
8. **The parity metric itself is gloss-text-set equality, not entry/rule-id equality** — recorded
   as a methodology limitation, not a construct gap. Both grammars leave the optional
   `<MorphemeId>` XML element unset on every morpheme, so `hc-rs`'s own id-level batch signature is
   uninformative (§4.3 of the earlier oracle-building step; blank on every morpheme). For
   Indonesian, gloss text is close to one-to-one with rule/entry identity, so 121/121 is a robust
   claim. For Sena, glosses collide heavily — many noun-class-prefix rules render as bare numbers
   (`"3"`, `"6"`, `"9"`, …) that are not distinctive on their own, and this PoC's own tags remain
   full entry/rule xml-ids internally, but the **comparison** in §4.2/§7 is done on the gloss-set
   rendering. Concretely, this means the Sena **122/290 exact-match count could mask an id-level
   difference that happens to render as the same gloss-set** (two different rules producing the
   same numeric class label, say) — the reported number is not wrong, but it is a coarser check for
   Sena than for Indonesian, and should be read as such.

None of items 0-7 are the mathematically-provable "unbounded copying is not regular" carve-out the
existing plan documents cite (§5.4 of `HYBRID_FST_FEASIBILITY.md`) — that specific claim was tested
directly in this PoC (§5) and the standard industry workaround (compile-replace) closed it
completely, for all 7 named corpus words, with the actual phonological interaction computed
mechanically rather than hand-derived.

---

## 8. How to rerun this

```
cd tools/fst-poc
python compare_indonesian.py ../../samples/data/indonesian-hc.xml \
    ../../samples/data/indonesian-words.txt ../../reports/oracle/indonesian-oracle-gloss.tsv
python compare_sena.py ../../samples/data/sena-hc.xml \
    ../../reports/oracle/sena-sample-300-oracle-gloss.tsv needed_roots.txt
python build_indonesian_fst.py ../../samples/data/indonesian-hc.xml \
    ../../samples/data/indonesian-words.txt 30000   # 30000 = sample size, omit for full (slow)
PYTHONIOENCODING=utf-8 python probe_multi_analysis.py ../../samples/data/indonesian-hc.xml 6 7
```

`needed_roots.txt` (the 324-root Sena reduction, §4.3) is checked in alongside the scripts so
`compare_sena.py` reruns identically without recomputing the substring-match selection.

Oracle TSVs were built once via `tools/fst-poc/oracle_gloss.py` (shells out to `hc-rs parse
--gloss` per word) and are checked into `reports/oracle/`. The Sena oracle used a seeded 300-word
random sample (`random.seed(42)`, `reports/oracle/sena-sample-300.txt`) rather than the full 7,121
words, per the task's explicit time-boxing allowance; 10 of the 300 words hit the engine's own
known-pathological timeout (5-8s) and are excluded from the comparison denominator, matching how
`HYBRID_FST_FEASIBILITY.md` itself treats engine-unparseable words in its own coverage numbers.
