# 08 — Adversarial audit of reports 04/06a/06b/07, and the reframed no-verify architecture

Date: 2026-07-15. Author: Fable (main session), from three independent verification agents
(toolkit claims re-verified against upstream sources; PoC scripts/logs re-run; HC claims re-checked
against C# + Rust code and live engine runs). This report **amends 07** — read 07 first, then this.

Decision context (reframed by John, 2026-07-15): drop the HC verify step entirely; own only the
HC-XML → FST **compiler** (C#); compile to a standard COTS/OSS FST format; run on off-the-shelf
runtimes per platform; lazy compilation acceptable; reduplication via pre/post processing is
acceptable; keep morphemes + full multi-candidate output.

---

## 1. Verdict after audit

**The architectural verdict of 07 stands: no HC construct is inexpressible in a classical FST, and
no cost lacks a standard mitigation.** It rests on 06a (construct inventory — census verified,
two semantic claims corrected below), 06b (toolkit matrix — verified at source level, several
findings *improved* on re-check), and Kaplan–Kay closure arguments.

**What changes: the PoC (04) carried far less empirical weight than 07 presented.** The
"one open cell" (composition sizes) is actually a **four-to-five-item verification gate** (§5),
because the PoC validated none of the load-bearing mechanisms on a native toolkit. The expected
outcome is still favorable — but that expectation now rests on external precedent and source-level
toolkit verification, not on anything measured in this repo.

---

## 2. Audit of report 04 (PoC) — headline claims downgraded

Re-run and code-audited (`tools/fst-poc/*`, logs, TSVs). All numbers reproduce; their meaning was
overstated:

1. **"121/121 exact parity" is real but coarse.** Metric = *sorted gloss-token sets, deduplicated*
   (`compare_indonesian.py:53-54,74`): allomorph identity, analysis multiplicity, and morpheme
   order were never checked. Parity was computed against **Python-enumerated pairs**, not FST
   lookups — the FST built from the pairs was only timed (`build_indonesian_fst.py:152-158`),
   never diffed against the oracle. Oracle = **hc-rs (Rust)**, not C# HermitCrab (see §4.3).
2. **Reduplication never ran through compile-replace.** Copies were made by runtime Python token
   copying at enumeration time (`derive_indonesian.py:83-90`); a genuine finite network was built
   from the resulting pair set afterward. Demonstrated proposition: *HC-Indonesian's closure is
   finite, and finite ⇒ regular* — true but trivial. NOT demonstrated: a composed FST handling
   redup×phonology.
3. **Det/min multi-analysis survival**: real algorithms; tested on the real 2.2M-state sample
   artifact for exactly **one** word (`ajar`); the other 6 ran on a purpose-built toy containing
   only their own 12 pairs; 3 of the 6 contain unresolved `ⁿ` placeholders (malformed synthetic
   stacks). (For unweighted machines over concrete symbols, language preservation under det/min is
   a theorem anyway — the test mainly checked pyfoma.)
4. **Sena: no FST was ever built.** The "122/290 exact" headline decomposes to **20/182 (11%)
   positive parity**; 102 of the 122 are mutual-failure agreements; 38 mismatch words; **6
   false positives** (FST analyzes, engine rejects) — contradicting §1's "zero false positives"
   except as an Indonesian-only claim. Sena evidence = a Python enumerator with three known bugs
   over a reduced 324-root lexicon.
5. **"HeadFeatures → flags" is construction-untested.** Nothing flag-like was ever built in the
   PoC. RequiredHeadFeatures/OutputHeadFeatures is (a) the dominant Sena mismatch class (44/290),
   (b) the one HC mechanism carrying feature unification into morphotactics, and (c) covered only
   by 06b's source-level reading of toolkit runtimes. Highest-risk item in §5.
6. Confirmed converter-engineering gaps (all reproduced): archiphoneme rendering (111/324 roots
   render wrong), compounding attribute default (0 compounds generated), RightEnvironment
   deliberately unchecked.

Net: the PoC was a **semantics dry-run of the conversion logic** on corpus words. The HFST compile
is the real proof of concept.

## 3. Corrections to 06a/07 (HC-side)

1. **α-variables: "real grammars use 1–2" is wrong.** Amharic prule6/prule7 (CV-merger) each
   declare **20 distinct variables** (of the 24-name DTD cap, `HermitCrabInput.dtd:463` —
   cap itself verified). Mitigation, verified by membership count: the 20 variables jointly copy
   the feature bundles of one (C,V) segment pair, so expansion is bounded by matching segment
   *pairs* — nc15=59 × nc16=6 ⇒ **≤354 expansions** for that rule, nowhere near v^20.
   **Reframe the compiler cost model:** the α-expansion bound is *the count of segment tuples
   satisfying the joint constraint* (product over independently-varying positions), not v^k over
   variable names. A naive per-variable expander would explode on this rule; a tuple-indexed one
   does not. Lint on estimated tuple count, not variable count.
2. **The Category-purity gap is in the LIVE configuration, not a hypothetical one.** 06a claimed
   all three grammars use `Linear` + single-threaded. False: all three declare
   `morphologicalRuleOrder="unordered"` on their active strata (indonesian-hc.xml:1024,2556;
   sena-hc.xml:463,32982; amharic-hc.xml:12427,16404), and `SINGLE_THREADED` is defined in no
   csproj — the stock C# build runs `ParallelCombinationRuleCascade` with the FS-blind `Distinct`
   (`Morpher.cs:339`, `ParallelCombinationRuleCascade.cs:32-76`). Which Word survives dedup —
   hence potentially the exposed `Category` — can depend on thread scheduling. Implication for a
   no-verify FST: the FST is *more* deterministic than the reference engine here; parity testing
   must treat Category instability as an engine artifact, not an FST bug.
3. **Tag-key sufficiency is an inference, not a code fact.** Engine dedup is at Word level over
   (shape, rootAllomorph, mruleApps sequence, …) — richer than (morpheme IDs, allomorph IDs,
   root index). mbali proves morpheme IDs alone under-count; nothing yet proves two retained Words
   never share the same tag triple while differing (e.g. same morphemes via different application
   order). The verification compile's parity metric must compare **multisets keyed exactly the way
   the product will key analyses**, and this needs a decision: what IS analysis identity for the
   product? (Recommend: (morpheme ID, allomorph ID) sequence + root index, count multiplicity,
   and record any engine-side collisions found.)
4. **Verified intact:** censuses (rules/entries/segments/strata — independent XML re-parse matched
   everywhere checked); Sena 72/1,702 (environment-only — Sena has **zero** MPR constraints);
   Sena 0 prules / Indonesian 5 / Amharic 7 + 417 segments; lexicons 66–1,371; mbali 9×+6×=15
   **re-established by live C# run** (the cited parity-out/golden TSVs no longer exist on disk).
5. **NEW BUG (parity divergence):** the current-worktree Rust port returns **8** analyses for Sena
   `mbali` (5×+3×) vs the C# reference's **15** (9×+6×), memo on or off. The Rust free-fluctuation
   gate only pins `ana` (4), not mbali. Since the PoC's Indonesian oracle was hc-rs, any future
   parity work should re-baseline against **C#** (the engine the product story returns to anyway).

## 4. Audit of 06b (toolkits) — verified, with upgrades

Everything load-bearing checked out at source level: `.hfstol` runtime evaluates flag diacritics
per-path and prunes (always-on for hfstol, not optional); `hfst-optimized-lookup-java` is
Apache-2.0 under the hfst org (13 files; stale vs current format per HFST's own wiki);
compile-replace shipped in hfst-xfst (NEWS 3.8.2, bugfix 3.9.1); foma Apache-2.0 with `_eq()` only;
16-bit symbol ceiling confirmed in HFST's own header (`typedef unsigned short SymbolNumber`,
65,535 usable); omorfi 555,144 states / 29 MB; OpenFst `ComposeFst` delayed composition.

**Upgrades (06b was too pessimistic on laziness):**
- **foma `flookup` multi-net mode**: several nets in one file ⇒ "inputs will be passed through all
  of them (simulating composition)" — per-word rule-cascade without eager composition, in the same
  ~450-line Apache-2.0 runtime. 06b's "foma is eager-only" is true only of the compiler.
- **`hfst-lookup --cascade=composition`**: documented lookup-time cascade over multiple transducers
  (not available in `hfst-optimized-lookup`, single-hfstol).
- **Thrax/Pynini + OpenFst** is one Apache-2.0 family with both a rule calculus (offline) and true
  delayed composition (runtime) — 06b's "the toolkit with rules and the toolkit with laziness are
  different tools" overstates; the accurate form is "no *small shipped* runtime does both."
- Kleene (Apache-2.0, alternation-rule calculus on OpenFst) was missing from the matrix — but
  abandoned since 2018-10-24; noted for completeness only.
- divvunspell: library is Apache-2.0 OR MIT, CLI tools GPL-3.0; consumes ZHFST/THFST/BHFST spell
  archives, not bare .hfstol — cite as porting reference, not a reader.

## 5. The reframed architecture and its verification gate

**Architecture (John's four bullets, all supported by the audited analysis):**
1. **Own one component**: the HC-XML → FST compiler, in C#. It emits lexc + xfst-calculus rules +
   flag diacritics (+ compile-replace sections or a peel plan for redup). Everything downstream is
   COTS: HFST offline compile (GPL fine — nothing GPL ships), `.hfstol` artifact (data), existing
   readers (C++/Java/Python/Rust); the only build gap is a C# `.hfstol` reader (~1.5–2k lines, two
   permissive precedents, must target the *current* format).
2. **Laziness**: probably unnecessary (lexicons 66–1,371 entries vs 555k-lexeme precedents at
   29 MB), but now has TWO documented word-at-a-time fallbacks (flookup multi-net; hfst-lookup
   --cascade) plus OpenFst delayed composition as the heavyweight option.
3. **No verify step**: every verify function maps to a compiled mechanism —
   environments → continuation-arc conditions (linear); MPR/co-occurrence/stem-names/HeadFeatures →
   flags (linear, runtime-evaluated — verified in HFST source); α-rules → tuple-indexed expansion
   (≤354 on the worst real rule, Amharic CV-merger, k=20); opacity/feed/bleed → replace-rule
   calculus + stratum-ordered composition; free fluctuation → allomorph-ID tag paths; templates/
   clitics/compounding(≤2) → continuation classes. Redup without verify: prefer compile-replace;
   if using the runtime peel instead, make it self-checking by **FST round-trip** (analyze residual
   → generate from candidate tags → string-compare against input) — no HC engine anywhere.
4. **Morphemes + multi-candidates**: all-paths lookup is default in every surveyed runtime; tags at
   (morpheme, allomorph, root) granularity; multiplicity is the FST's natural output.

**The verification-compile gate (replaces 07's "one open cell") — all on native HFST:**
1. Real `lexicon ∘ rules` composition sizes: Indonesian (5 rules), Amharic (7 rules, 417 segments,
   the k=20 α-rules), Sena (0 rules — lexc-only).
2. **HeadFeatures→flags on Sena** — highest risk; construction-untested; dominant PoC mismatch class.
3. Real compile-replace (or `_eq()`/peel+round-trip) for Indonesian's 7 redup words, through the
   real meN- cascade, on an actual network.
4. **Allomorph-granularity multiset parity** vs the **C# engine** (not hc-rs), incl. mbali 15×;
   requires deciding the product's analysis-identity key (§3.3) and a stricter comparison metric
   than the PoC's gloss-set proxy.
5. Amharic α-rule expansion: confirm tuple-indexed encoding lands near the ≤354 estimate and
   compiles/compresses acceptably against the 417-symbol alphabet.

Budgets (<5 s build applies to the shipped C# build only; grammar compiles are offline; <10 MB and
<1 ms/word expectations from 06b precedent stand, now explicitly flagged as precedent-based, not
measured in-repo).

**If the gate passes:** per the standing decision criterion, abandon the Rust repo and build the
compiler in C# (sillsdev/machine), reusing the golden-TSV oracle discipline (re-baselined on C#)
as the compiler's acceptance suite. **If item 2 fails** (flags can't carry head-feature unification
on a real grammar): that is the specific, isolated failure mode that would justify keeping a
verifier — everything else in the gate has precedent-backed fallbacks.
