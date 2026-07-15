# Adversarial review of reports 01–03

Reviewer: Fable (main session), against the code and with independent measurements.
Report 04 (standard-FST proof of concept) was commissioned mid-review specifically to test the
weakest shared assumption of reports 01–03; its findings are reviewed in §4 when available.

Verdicts used: **HOLDS** (independently verified), **HOLDS-WITH-CAVEAT**, **CHALLENGED**
(reasoning gap or unexamined alternative), **WRONG** (contradicted by evidence).

---

## 0. What I independently verified (all three reports)

- Dependency graph: `hc-wasm` → {hc-grammar, hc-parse, hc-realize, hc-lexicon}; `hc-ffi` →
  {hc-parse, hc-grammar}; only `hc-cli` depends on `hc-hybrid`. **HOLDS** (read directly from the
  three Cargo.tomls this session). The entire propose/verify hybrid is not shipped.
- `hc-wasm` calls `Morpher::parse_word_opts` directly (`hc-wasm/src/lib.rs:242`) and reports
  per-word ms (`lib.rs:91`). **HOLDS.**
- Workspace license MIT (`rust/Cargo.toml:25`) → report 02's GPL blocker on HFST/SFST is real.
  **HOLDS.**
- `hc-hybrid` does not depend on the `hc-fst` crate at all (its Cargo.toml lists only hc-grammar,
  hc-featstruct, hc-shape, hc-rules, hc-parse) — it builds its own automata on `hc-featstruct`
  bitset primitives. Report 02's characterization survives this check.
- **New measurement (mine): the user's "~100 ms worst words" is the Indonesian production path.**
  Single-threaded release batch over the 121-word Indonesian corpus: p50 = 1 ms, p90 = 6 ms,
  p99 = 22 ms, max = 58 ms — and every word over 20 ms is a reduplicated `-X-X` form
  (`mengamat-amati` 58 ms, `membagi-bagi` 24 ms, `menulis-nulis` 22 ms, ...). With typical
  wasm32 slowdown (1.5–3×), 58 ms native ≈ the observed ~100 ms in the browser demo. None of the
  three reports measured this corpus on the production engine; report 03 profiled Sena (where the
  tail is seconds, a different and worse problem).

---

## 1. Report 01 (FST-only feasibility)

**HOLDS (and valuable):**
- Verification is the *only* implementation of several construct semantics today (metathesis stub,
  no clitic/process proposers, no build-time MPR/environment/stem-name gating, single-binding
  α-variable probe). Removing verify deletes functionality, it doesn't shave a safety margin.
  Spot-checked the metathesis stub (`compiler.rs:129-150`) and the grep-zero claims — consistent.
- The junction-probing O(alphabet²) measurement (Amharic 25,977 ms vs 1,013 ms, identical state
  counts) is corroborated by the C# doc's independent ~112 s figure. A real, quantified,
  fix-queued build-time bug.
- The MPR/environment census (72/1,702 allomorphs on Sena, linear cost) is the right kind of
  evidence and kills the vague "gating would explode" worry.
- The central empirical finding — bare walk ≈ half of hybrid time (52.6/47.4 split, corroborated
  independently by report 03 on the same sample) — is the single most decision-relevant number in
  the report: dropping verify from the *current* hybrid buys at most 2×.

**CHALLENGED — three places where the report accepts the incumbent design's premises:**

1. **"Determinization is foreclosed by multi-analysis semantics" (§4.4).** This is true only of
   the *current encoding* (unification arcs + capture-style analysis recovery). Classical FST
   morphology has enumerated multiple analyses for 40 years: analyses are distinct output-tape
   strings; determinizing (or just efficiently indexing) the machine over pair symbols/concrete
   segments preserves all of them. A 27 ms *median* bare-trie walk on an 18,871-state machine is
   evidence that this walk implementation is slow, not that finite-state lookup is slow —
   foma/HFST-style lookup on machines this size is microseconds. The report's own quoted plan line
   ("determinizing the plain-symbol lexicon trie layer is fine — an unexploited, real lever")
   concedes the point and then doesn't pursue it. Report 04's PoC exists to settle this.
2. **The infeasibility verdict leans on constructs absent from every grammar in hand.** Metathesis,
   clitics, process morphs: none of Indonesian/Sena/Amharic uses them (the report itself says so,
   §6). "FST-only is unsafe for arbitrary FieldWorks grammars" is defensible; "FST-only is
   infeasible today" overstates it for the actual product, where a per-grammar advisor gate
   ("this grammar is Tier 1 → FST-only; this one isn't → engine") is exactly the architecture the
   advisor already implements.
3. **§7's "an FST-based replacement would not obviously be faster on the words that hurt" is a
   non sequitur.** It compares the *current hybrid's walk* to the plain engine. Both are
   implementations of the same never-determinized unification-arc design. It says nothing about a
   classically compiled transducer, which is the alternative actually on the table.
4. **The reduplication "mathematical carve-out" (§2.2 last row, §4.1) is accepted uncritically
   from the plan docs and is wrong in the form stated.** "w→ww is provably not regular" holds
   only for *unbounded* w. A HermitCrab grammar's lexicon is finite, so reduplication over the
   actual root inventory is a finite union of literal copies — regular by construction, with a
   measurable (not exponential) state cost. Production toolkits ship exactly this: xfst/foma
   `compile-replace` (Beesley & Karttunen 2000) and foma's `_eq()` built-in have compiled
   full-stem reduplication into shipped morphologies (Malay, Tagalog) for two decades. The plan
   docs cite compile-replace and wave it off as "a two-pass preprocessing trick, not a
   counterexample" — but for the product question ("can the shipped artifact be a pure FST?") a
   compile-time trick that produces a plain FST *is* the answer. Notably, the reduplicated
   Indonesian words are exactly the user's observed worst ~100 ms production words, so this
   carve-out sits precisely on the pain point. Report 04 is tasked with testing it, including the
   redup×phonology interaction (`menulis-nulis`) and suffix-outside-copy (`mengamat-amati`).

**Verdict on report 01:** its facts are the best-verified of the three; its *verdict* ("the hybrid
is genuinely necessary") is conditional on the current encoding and generalizes further than the
evidence. What it actually proves: (a) dropping verify from the current hybrid is pointless, and
(b) the current walk is slow for reasons unrelated to verification. Both true. Neither implies the
propose/verify architecture is the right endpoint.

## 2. Report 02 (established FST libraries)

**HOLDS:**
- `hc-fst` is a unification/capture regex engine (a port of SIL.Machine.Matching), not a
  transducer; no surveyed library implements that primitive. For a **drop-in swap** the "no"
  verdict is solid, and the rustfst-on-wasm friction + GPL findings are verified and useful.
- The empirical baseline (hc-fst clean build 4.05 s; hc-wasm 1.7 MB wasm) is the right budget
  framing: the hand-rolled matcher is not what threatens any budget.

**CHALLENGED:**
1. **The question it answered is narrower than the question that matters.** "Can a library replace
   `hc-fst` as-is?" → no. But `hc-fst` is the *pattern matcher inside the search engine*; the
   user's actual question is whether the *system* could be an offline-compiled classical FST with
   a standard runtime. Over a concrete, finite segment inventory, feature constraints expand to
   symbol sets — the standard move since Koskenniemi. The report acknowledges this path only via
   the "re-author grammars in lexc/twolc" strawman; the real option is **our own compiler emits
   the FST from HermitCrab XML** (no linguist re-authoring), and only the *runtime format* is
   standard. That option is untested by this report and is what report 04 tests.
2. **§4.3 "the < 1 ms/word target is already met" is WRONG.** It cites the C# hybrid's Indonesian
   composite p50 of 1.4 ms (a Debug C# number, > 1 ms, from a non-shipped subsystem) while the
   shipped engine's worst Indonesian words measure 58 ms native (my measurement above) and Sena's
   tail is seconds (report 03). The sentence should not survive into any decision.

**Verdict on report 02:** correct answer to the drop-in question, wrong altitude for the
architectural question; one factual claim refuted.

## 3. Report 03 (parse latency profile)

**HOLDS (strong empirical work):**
- Hypothesis (a) — lazy compilation — convincingly refuted: `RuleCache` built once at
  `Morpher::new`; identical step counts across repeated calls. Cross-checked the code refs.
- Hypothesis (b) as originally posed refuted: `FeatureStruct` is a small sorted Vec.
  `Shape::clone` (5 boxed slices) honestly flagged as unprofiled inference.
- The real production tail is seconds-to-10 s on Sena (p99 ≈ 3 s, uncapped worst ≈ 10 s,
  1.12 M steps for one word), and the memo is load-bearing (memo-off never finishes). The
  many-cheap-steps (Sena) vs few-expensive-steps (Amharic) distinction is genuinely useful.
- The hybrid verify diagnosis (per-candidate re-segmentation + fresh memo, cost scales with
  candidate count not confirmations) is a real architectural finding, corroborated by report 01.

**HOLDS-WITH-CAVEAT:**
- Timing hygiene: the cold/warm probe overlapped the batch run (contended CPU). The report
  flags this honestly and leans on step counts; acceptable, but its absolute per-step costs are
  order-of-magnitude only.

**CHALLENGED:**
1. **It never measured the corpus the user actually experiences.** The "100 ms" framing was
   dismissed as "underselling Sena's true tail" — correct for Sena, but the user's number is real:
   it's Indonesian reduplicated words on the shipped wasm path (my measurement, §0). The
   worst-case production story is therefore two-tier: ~100 ms redup words on small grammars
   (annoying, plausibly fixable), and multi-second tails on Sena-class grammars (disqualifying).
2. **"No constant-factor fix reaches < 1 ms; only an algorithmic matcher fix or a timeout" —
   true inside the search paradigm, but the paradigm is the variable.** The report's own numbers
   are the strongest argument *for* offline compilation: if analysis is a pure function of
   (grammar, word) — which the report notes when endorsing word-caching — then a compiled
   surface→analyses transducer answers in lookup time, and the 1.12 M-step search never happens at
   runtime. The report stops one step short of saying this.

**Verdict on report 03:** best measurements of the three; both its refutations stand; its "path to
< 1 ms" is correct within the current architecture and silent about the architecture switch.

---

## 4. Cross-report synthesis (updated after report 04)

The three reports converge on facts and share one blind spot:

**Converged facts (all independently verified or cross-corroborated):**
1. Nothing shipped uses the hybrid; the product's latency problem lives in `hc-parse::Morpher`.
2. The user's ~100 ms = Indonesian redup words on wasm; the real ceiling is Sena's multi-second,
   million-step searches.
3. Slowness is genuine combinatorial search, not lazy compilation, not (primarily) allocation.
4. Inside the hybrid, verify ≈ propose ≈ half each; dropping verify alone is not a speed win and
   deletes the only implementation of several construct semantics.
5. No established library replaces the current unification-arc machinery as-is; GPL rules out
   HFST/SFST for shipped artifacts; rustfst is wasm-hostile and solves a different problem.

**The shared blind spot:** all three treat "FST" as meaning *the hybrid's own unification-arc,
never-determinized design*, and evaluate FST-only against that implementation. None evaluates the
classical alternative — expand features over the concrete segment alphabet offline, compile
lexicon ∘ rules with 40-year-old technology, ship a compiled transducer + tiny reader — even
though the repo's own feasibility doc cites the entire literature for it (Kaplan–Kay, two-level
morphology, foma/HFST) before declining it for reasons (`unification arcs`, `multi-analysis`)
that are properties of the incumbent encoding, not of the problem. Report 04 tests exactly this.

**Report 04 (standard-FST PoC) — reviewed after landing.** The blind spot did not survive contact
with the experiment: Indonesian reached 121/121 exact analysis-set parity on a concrete-alphabet
classical FST, including all 7 reduplication words through the real phonological cascade
(compile-replace), and multi-analysis enumeration survived determinize/minimize on all 7 ambiguous
words tested. The report's own alarming numbers (~43M extrapolated states, 12.2 MB sample, 47 ms
lookups) are artifacts of its enumerate-then-interpret construction (no trie sharing; interpreted
Python NFA walk) and must not be read as evidence about the composed/minimized architecture — the
report says so itself (§1, §4.1) and the review concurs. Its genuinely open item — real
`lexicon ∘ rules` composition sizes on our grammars with a native toolkit — is carried forward as
the single remaining verification step. Its named gaps (HeadFeatures→flags, RightEnvironment,
archiphoneme rendering, compounding-attribute default) are converter engineering, not theory.
Verdicts on reports 01–03 above stand, with report 01's "FST-only is not feasible today" now
explicitly narrowed to "the *incumbent hybrid's* FST cannot go verifier-less" — the classical
compile path was outside its evidence base.

**Final synthesis: see `07-final-analysis.md`** (answers the three architecture questions using
06a/06b + this review; verdict: no inexpressible construct, no unmitigated super-linear cost;
one empirical cell — composition sizes — deferred to a final verification compile).
