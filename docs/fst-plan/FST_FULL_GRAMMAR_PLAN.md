# FST full-grammar coverage plan — 100% of Sena + Indonesian

> **LEGACY — superseded by [`foma-fst-plan.md`](foma-fst-plan.md).** This document is part of the
> record of the earlier custom-spun FST prototype (`hc-hybrid`), which PanGloss has sunset (plan
> P5, gate F5, 2026-07-16) in favor of a foma-based FST proposer with the full HermitCrab engine
> confirming/pruning. Kept for historical record only — not current design guidance.

> Written 2026-07-02, after the left-environment session (commit `308f269c`) and its finding that
> **0 of Indonesian's 5 phonological rules have ever compiled** (boundary-representative gap).
> Companion to `FST_FAST_PATH_PLAN.md` (which stays the architecture reference); this doc is the
> execution plan for closing the LAST gaps on both real grammars.
>
> **STATUS (2026-07-03): Phases A, C, D, H, G1, AND G2 are ALL DONE.** Indonesian is at **121/121
> fully covered, 0 unsound, 0 false positives** — every engine-parseable word in the corpus is
> closed. Sena's `ndikhali` (the one confirmed gap in its sampled corpus) is closed with **exact
> 8/8 set parity** — a guarded 60-word Sena slice is now 57/57 fully covered, 0 unsound. Sena's
> build-time regression is fixed (9.3 s → ~1.0–1.5 s). **Both real grammars this plan targeted are
> now fully covered on every word measured this session.** The compounding "data-model lift"
> premise (Phase E, and the matching `FST_FAST_PATH_PLAN.md` KNOWN_GAPS entry) is confirmed FALSE —
> closing it took a `FstReplay` fix, a trie compound loop, and (found only during implementation)
> extending `DerivableToCategory` to treat compounding as a category-transition edge — see Phase G2
> for the full account of what the original spec got right and what it missed. **Remaining: only
> Phase I** (the true-FST generalization). **Phase I now has a FULL execution spec (2026-07-03,
> same day): milestones I0–I7 + optional I8, ~6–9 days, each commit-gated with its own tests and
> verification battery** — the deliberate scope change from "cover these two grammars" to "correct
> by construction for arbitrary regular HC grammars." Start at I0 and work in order; the marquee
> new capabilities (word-internal rules, long-distance harmony, deep feeding chains) each get a
> toy-grammar test that must FAIL on today's composite before the chain makes it pass.
>
> **UPDATE (2026-07-03, later same day): I0 and I1 are DONE.** I0 (ε-output groundwork on
> `InversePhonology`) and I1 (`EnvNfaCompiler` + `RuleInverseCompiler` — the env-pattern NFA compiler
> and Exact-tier substitution compiler) both landed; full suite **126/126 green** (was 121). I1's
> marquee long-distance-harmony toy test (a quantified environment span) is confirmed Exact-tier, not
> Permissive, as the spec demanded. Measured on the real Indonesian grammar: **`Exact=1, Permissive=2,
> IdentitySkip=2`** — every in-scope (substitution-shaped) rule reaches ≥ Permissive, exactly matching
> this doc's own pre-written expectation; the 2 `IdentitySkip`s are both genuinely deletion rules,
> correctly deferred to I3. Getting there required catching a real bug an advisor review flagged (an
> initial pass showed only 1/5 compiling, with two real assimilation/default rules mislabeled
> `"no-effect"` — a false claim the toy-test suite alone couldn't see, since it never probed a
> boundary-crossing rule): fixed by building the probe window as a `Shape` directly from known
> `FeatureStruct`s instead of concatenating representation strings and re-segmenting (which could
> silently merge two adjacent pieces at a join into an unintended grapheme), plus a `before`/`after`
> filter mismatch this fix's own first draft introduced. Neither new compiler is wired into the real
> analyzer yet (I2's job) — zero regression risk on either grammar's measured coverage. See Phase I1
> section below for the full account, including all four real bugs this phase's tests and measurement
> caught. Next: I2 (the chain walker).
>
> **UPDATE (2026-07-03, later): I2 is DONE** — suite **129/129** (+3 `ChainWalkerTests`). ONE walker
> (`AnalyzeComposed` delegates to the general `AnalyzeChain`), state-vector configs, shared
> `CascadeSymbol` cascade, `ChainClosure` with all three ε-branches. Executing the toy tests' baseline
> assertions properly surfaced an **honesty correction: today's full composite empirically covers all
> three marquee toy shapes** (via `ComposedPhonologyProposer` runtime un-application AND the
> surface-precompile — two independent mechanisms each); only v1's `LockstepPhonologyProposer` misses
> them. The chain's honest value: same shapes in LOCKSTEP at FST-walk cost, retiring the covering
> mechanisms that don't scale (the Amharic findings above are the evidence). See the I2 section and the
> corrected "What it fixes" section. Next: I3 (deletion-inverse + epenthesis-inverse — Amharic's main
> phonology lever, 6 of its 7 rules).
>
> **UPDATE (2026-07-03, later still): I3 is DONE** — suite **132/132**. Deletion compiles via a
> STRUCTURAL restoration cap (automaton "floors", strictly ascending — cannot hang, default =
> `Morpher.DeletionReapplications + 1`, knob available); epenthesis via real ε-output arcs (I0's shape,
> first compiler to emit it). Walker unchanged. **Amharic: `Exact=2, Permissive=4, IdentitySkip=1`** (was
> 1/0/6 — 5 of 6 deletion rules now compile; the holdout is the α-variable boundary-conditioned CV
> merger, waiting on per-binding enumeration and/or I4). **Indonesian: `Exact=2, Permissive=3,
> IdentitySkip=0`** (was 1/2/2). Bonus: the honesty baselines caught a real v1 bug
> (`LockstepPhonologyProposer` start-state-only identity check silently disables left-env deletion
> rules) — retirement evidence for I7. See the I3 section. Next: I4 (the boundary tape).
>
> **UPDATE (2026-07-04): I4 is DONE** — suite **134/134**. Boundary nodes become real trie arcs; the
> bare walker crosses them as free ε-moves (byte-identical gate PASSED, Fable-verified via stash-built
> true baselines: Indonesian 533→547 states, Sena 16,347→18,871 states, zero drift across all 7,121
> Sena words); the chain walker inserts them as real symbols, hang-proof via `InsertionsUsed` in the
> config key. **Marquee cross-check PASSED: Indonesian with junction probing OFF + chain ON covers
> 46/46 non-redup meN- words** — the general mechanism subsumes Phase C's special case (coverage half of
> I7's junction-probing-retirement question answered YES; perf half open until I7 wires the chain in).
> Honest deviation: tier reports UNCHANGED (Indonesian 2/3/0, Amharic 2/4/1) — I4 touches the walker,
> not the tier classifier, so the plan's "Permissive→Exact" expectation for I4 was mis-chosen (that
> premise was already resolved in I1). Next: I5 (metathesis + application-semantics honesty).
>
> **FULL-PLAN REVIEW (2026-07-07, single-reviewer audit of I0–I4 committed + the uncommitted I5
> partial):** the committed core is CLEAN — every targeted risk probe came back verified-fine: the
> insert-boundary re-wrap loses nothing; I3 floors × boundary env fragments have no soundness hole;
> the "nothing wired into the composite" claim re-confirmed by grep; `enableJunctionProbing`
> default-true confirmed at both ctor sites with only the marquee test passing false. Two standing
> NOTEs recorded for later milestones: (1) an ε-cycle that accrues tokens would defeat `PConfigKey`
> dedup (growing token arrays hash distinctly) — structurally impossible today because the trie is a
> DAG by construction, but UNDEFENDED: lifting compounding beyond 2 roots via a real loop would
> silently reintroduce a hang vector (defend or assert when that day comes); (2) `PConfigKey`/
> `TokenArrayKey` recompute hashes per call and `CascadeSymbol` clones an `int[]` per candidate arc
> per rank — fine for correctness, but real allocation pressure to profile BEFORE setting I7's p50
> budget. **The uncommitted I5 partial (~+350 lines in `RuleInverseCompiler.cs`) got a REDO verdict
> and must not land as-is: it breaks 4 tests (130/134) and silently regresses Amharic's tier report
> (Exact 2→1)** — see the I5 section for the full account, including the finding that I5.2's own spec
> text was the bug's root cause. Tree state as of this review: the partial is left in the working
> tree (its metathesis skeleton + RTL flag are salvageable) with the suite RED until the redo.
>
> **UPDATE (2026-07-07, later): the I5 REDO is DONE and green** — suite **137/137**, both tier
> reports restored (Indonesian 2/3/0, Amharic 2/4/1). Self-feeding detection dropped to a documented
> residual (option c, with in-code reasoning); metathesis landed with a 256-combo compile-time cap
> and its first-ever executions (3 new tests); bonus contract bug fixed (IdentitySkip early-returns
> were reject-all, now identity-seeded). See the I5 section. Next: I6 (the beam cap).
>
> **UPDATE (2026-07-07, later): I6 is DONE** — suite **139/139**. One per-word `BeamBudget`
> (default 10,000, ctor knob) debited on BOTH the frontier axis and `CascadeSymbol`'s within-symbol
> enumeration axis (the review-mandated sharpening); latches on overflow → word falls to unparsed,
> counted (`BeamOverflowCount`/`ProbeReport.BeamOverflows`), never throws/hangs. Pathological
> 12-rank×8-branch chain: unparsed in ~22 ms. Indonesian: ZERO overflows, byte-identical to the I4
> baseline. Oldest KNOWN_GAPS item closed. Next: I7 (wiring, measurement, retirement by evidence) —
> the finale.
>
> **FINAL UPDATE (2026-07-07): PHASE I IS COMPLETE — I0 through I7 all DONE, suite 144/144.**
> I7's measured outcome: `ChainPhonologyProposer` is built, tested, and wired into
> `CompositeProposer`/`FstCoverageProbe` **OPT-IN** (chain-on holds coverage exactly — Indonesian
> 121/121, 0 unsound — but regresses verified-walk p50 ~37×, far past the ≤1.5× budget; the plan's
> own rule fired: ship opt-in, record, retire nothing). All retirement decisions were made BY
> MEASUREMENT and all landed on "keep": v1 stays as the default fast path (wins p50 where coverage
> ties), junction probing stays (its retirement's coverage half is proven — I4's 46/46 — but
> requires chain-as-default). I7a's battery also caught and fixed I6's mis-calibrated beam default
> (three-point calibration → 1,000,000). The chain is Phase I's deliverable exactly as re-scoped by
> the I2 honesty correction: not "covers what nothing else can" but the correct-by-construction,
> lockstep, lexicon-constrained instrument — wireable today for any grammar where v1's limits bite
> (word-internal rules, deep feeding, harmony, deletion/epenthesis/metathesis, boundary
> conditioning), the foundation future perf work promotes into the default slot, and the honest
> tier report that tells a grammar author exactly what compiles and why. **Remaining, both
> optional/deferred by their own specs:** I8 (Clitic + Process MorphOps — spec them when a grammar
> needs them); chain-walk performance (allocation pressure profiled and recorded — the path to
> flipping the default); per-grammar beam calibration (complexity-cap plan). Follow-up already
> queued in this doc: test the chain end-to-end against the real Amharic grammar (I2+I3 made it
> meaningful: 6 of its 7 rules compile) once the FieldWorks-export fixture pipeline settles.
>
> **UPDATE (2026-07-03, later still): a THIRD real grammar is now available for Phase I — Amharic**,
> exported from two real FieldWorks/LCM projects (`Amharic ... for Andy Black and John Lambert.fwbackup`,
> `Amharic ... for John and Beth.fwbackup` — same underlying project, two time-separated snapshots) via
> FieldWorks' own `Src/GenerateHCConfig` tool (`HCLoader.Load` + `XmlLanguageWriter.Save`). Getting a
> usable export required a one-line data fix in BOTH `.fwdata` files: both contained an identical stale
> `MoMorphAdhocProhib` (guid `aea110aa-595a-40a7-bb62-d9bf95280bb4`) — an ad-hoc morpheme co-occurrence
> prohibition still targeting an inflectional affix (`MoInflAffMsa` `8497c72c-...`) whose only home is a
> slot in the "perfective verb with object" `MoInflAffixTemplate`, which the linguist had already
> disabled. `HCLoader` registers the affix's `Morpheme` in its internal id map regardless of whether the
> owning template is disabled, but `XmlLanguageWriter` only assigns a written id to morphemes that
> actually land in a stratum/template — so the orphaned prohibition crashed the writer with
> `KeyNotFoundException` in `WriteMorphemeCoOccurrenceRule`. Fixed for testing by flipping that one
> `MoMorphAdhocProhib`'s `<Disabled>` to `True` in scratch copies of both `.fwdata` files (the original
> `.fwbackup`s in the repo were left untouched — this is a stale-data cleanup a linguist would normally
> do in FLEx itself, not a repo change). **A general-purpose fix for this class of bug (any
> `MorphemeCoOccurrenceRule`/`AllomorphCoOccurrenceRule` referencing a morpheme/allomorph the writer
> never assigned an id to) is in progress separately, in another `machine` worktree
> (`fix/xml-writer-dangling-cooccurrence-rules`), on `XmlLanguageWriter.WriteLanguage`.**
>
> With the export unblocked, measured Amharic against the Phase-I machinery (2026-07-03, investigated
> same day — `Census_NoSearchOracle` now prints the full `GrammarFstReport.Format()` dump and times the
> bare vs morpher-based builds separately):
>
> - **Census: Tier 2 candidate — hybrid; the 3 escapes are all INFIXATION affixes** (`-ä-1`, `-ä-2`,
>   `-ää-` — material inserted between two copies of the stem; Semitic templatic morphology showing up
>   exactly where expected). All 3 are opaque (phonology applies after them, so strip-and-reparse
>   probing needs the search backstop) but **regular — FST-reclaimable**; 0 genuinely non-regular.
>   Until the chain reclaims them they are the composite's infix-generator's job, same as Sena/Tagalog
>   shapes. Grammar census: 35 affix rules, 7 phonological rules, 1 compounding rule (`Balebet`,
>   bounded), 76 lexical entries.
> - **I1's compiler on Amharic's 7 phonological rules (`TierReport_OnRealGrammar`):
>   `Exact=1, Permissive=0, IdentitySkip=6`.** The Exact one is `remove consonant length from lexical
>   forms` (plain substitution). The 6 IdentitySkips are ALL deletion-shaped (LHS longer than RHS, per
>   the census Cost advisories) — squarely **I3's scope** — and 3 of them (`e-creation` ×2,
>   `o-creation`) are additionally MPR/syntactic-gated, so they need I3 + the gate handling. Amharic is
>   therefore the first real grammar where **I3 is the main phonology lever** (Indonesian's remaining
>   gap after I1 was only 2 deletion rules; here it is 6 of 7).
> - **FST build: 5641 states; the "113s build" is NOT the FST — and (measured, 2026-07-03, second pass)
>   NOT synthesis speed either.** Split measurement: pure FST construction (no-morpher constructor) =
>   **3,739 states in 40 ms**; morpher-based build = **~112 s**. A first-pass hypothesis blamed "HC
>   synthesis being pathologically slow"; the new `FstSenaBenchmark.BuildCost_Attribution` diagnostic
>   (times each probing component in isolation over exactly the builder's inputs) **refutes that**:
>   - (a) bare-root synthesis (`GenerateWords` per root): 77 roots → 37 surfaces in **91 ms**. HC
>     synthesis per se is FAST on Amharic.
>   - (b) `SurfacePhonology.Variants` (2×alphabet cascades per distinct affix string): 58 strings in
>     **~12 s** (~200 ms/string; worst `+ያችን` 462 ms).
>   - (c) `SurfacePhonology.DeletionJunctions` (alphabet→alphabet² cascades per distinct prefix-op
>     string): 16 strings in **~278 s isolated** (worst single string `aል+` = 22 s) — **and it found 0
>     junctions**. The entire cost discovers nothing.
>   **Root cause: the alphabet.** Amharic's character table has **417 segment definitions** (the Ge'ez
>   fidel enumerated as segments) vs Indonesian's 29 / Sena's 40. `DeletionJunctions` is explicitly
>   designed around "bounded by alphabet² (dozens², not lexicon-sized)" (its own doc comment) — 417
>   breaks that assumption: 417² ≈ 174k full synthesis-cascade runs per probed affix. Amharic has
>   deletion-shaped subrules (6), so the `_anyDeletionSubrule` fast-bail doesn't apply, and every probe
>   runs the 7-rule cascade. The 112 s build ≈ (b) over all 58 strings + (c) over the derivational-prefix
>   subset. **This is exactly the scenario the not-yet-landed complexity-cap plan (`complexity-cap.md`)
>   guards against** — do not run the search-oracle benchmark against Amharic again without that cap, a
>   hard per-word timeout, or a much smaller `HC_MAX_WORDS`.
>
> **Investigations queued from the build-cost finding** (do these before treating Amharic as a routine
> iteration grammar; none block I2):
> 1. **Feature-quotient the probe alphabet** (likely the real fix, benefits `Variants` AND
>    `DeletionJunctions`): the probes only need one representative per class of segments the
>    PHONOLOGICAL RULES can distinguish — quotient the 417 segments by the feature values the grammar's
>    rules actually reference (or by full FeatureStruct equality as a first cut) and probe one
>    representative per class. A syllabary's fidel mostly differ in features no rule mentions; expect
>    dozens of classes, restoring the design assumption. Verify: `Census_NoSearchOracle` build time on
>    Amharic drops to seconds with byte-identical FST coverage on Indonesian/Sena (their alphabets are
>    already small, so any state/coverage drift there = a bug in the quotient).
> 2. **Static pre-gate for `DeletionJunctions`**: before context-probing an affix at all, check whether
>    ANY deletion subrule's deleted-segment pattern can even unify with a neighbor of the affix's final
>    segment (cheap FeatureStruct unification, no cascade). Amharic's measured 0-junction result means
>    this gate alone would skip ~all 174k×16 probes. Sound: skipping a probe that cannot match loses
>    nothing (same argument as the existing `_anyDeletionSubrule` gate, just per-rule/per-affix).
> 3. **Measure the analysis-side oracle blowup separately** (it is NOT explained by the above — the
>    killed `Benchmark_FstVsSearch` run was analysis, which doesn't use `SurfacePhonology`): plausible
>    suspect is deletion UN-application guessing re-inserted segments from the same 417-wide alphabet
>    (×6 deletion rules, compounding per word), i.e. the alphabet hits analysis too, via a different
>    code path. Needs a per-word step counter / wall-clock cap first (complexity-cap's instrument) so
>    the measurement itself can't hang; a 5–10 word `HC_MAX_WORDS` run with the step profiler from the
>    15M-step Sena dissection would confirm or kill the hypothesis.
> 4. **Do NOT over-invest in optimizing `DeletionJunctions` itself**: I4 (the boundary tape) is the
>    principled replacement that retires junction probing entirely on the chain path. Items 1–2 are
>    worth it only as cheap unblockers for pre-I4 Amharic iteration; if they turn out non-trivial, park
>    them and let I4 retire the mechanism instead.
>
> **Follow-up (once Phase I / the general chain walker is complete): test it end-to-end against this
> real Amharic grammar**, not just Sena/Indonesian — it's a substantively different typological case
> (templatic-leaning verb morphology, Tier 2/hybrid today) and will exercise the general FST path in
> ways the other two grammars don't. Concretely: Amharic becomes a meaningful chain test at **I2+I3**
> (6 of its 7 phonological rules are deletion-shaped and only compile at I3; 3 also need the MPR-gate
> handling), and it is the strongest motivating grammar for the whole phase — today's analysis engine
> blows up on it (oracle killed at 20 min/60 words; investigation 3 above), the pre-chain build-time
> probing is quadratic in its 417-segment alphabet (investigations 1–2, retired outright by I4), while
> the pure FST construction builds in 40 ms. Local paths for reruns: grammar + a 673-word corpus extracted from
> the project's own `WfiWordform` records at `C:\Users\johnm\Documents\repos\machine\samples\data\
> amharic-hc.xml` / `amharic-words.txt` (gitignored, machine-local, same convention as
> `indonesian-hc.xml`/`sena-hc.xml`) — but regenerate from a `.fwbackup` with the `MoMorphAdhocProhib`
> fix applied, or from a project where the `machine`-side writer fix has landed.

## Goal (definition of done)

For BOTH grammars (`sena-hc.xml` + 7,121-word list, `indonesian-hc.xml` + 121-word list):

1. Every **engine-parseable** word is **fully covered** — set parity per word
   (`Benchmark_CompositeVsSearch`'s `SetEquals(oracle)` criterion), not just "some parse".
2. **0 unsound** (the propose-and-verify contract is untouched — verify still gates everything).
3. No construct actually used by these two grammars is silently unsupported: the probe's
   diagnostics account for every rule (compiled, handled-by-peel, or engine-fallback with reason).

The denominator is engine-parseable words: the raw lists contain loanwords, typos, and
deliberately ungrammatical meN- variants (`menaca`, `menlangit`, `memlangit`…) that the engine
itself rejects; those count as covered when the FST also (quickly) rejects them.

## Verdict first: yes, this is reachable — and WITHOUT the "multi-week" generic cascade composer

The two grammars' remaining gaps are narrower than the generic problem:

- **Sena has zero phonological rules.** Its whole remaining gap is *morphotactic proposer
  coverage* (copula/TAM, prefixal derivation, depth-3 derivation — `ndikhali`, and the archived
  plan's `nyari`/`cawo`/`miwiri` family). No FST theory needed; the trie builder just doesn't lay
  down those paths yet.
- **Indonesian's 5 phonological rules are ALL boundary-conditioned at affix junctions** (or, for
  `Nasalization in reduplication`, conditioned inside a redup copy). Nothing fires word-internally
  far from a morpheme join. That means the interacting `meN-` cluster (assimilation feeding
  obstruent deletion, MPR gating, α-place variables) can be handled by **bounded build-time
  junction probing through the REAL synthesis cascade** — baking junction surface-variants into
  the trie — instead of per-rule inverse transducers + generic multi-rule composition.

Key insight for the junction approach: the forbidden move in `PhonologyRuleCompiler`'s design notes
is probing the combined multi-rule effect **and attributing it to a single rule's branch** (that
misreads feeding/bleeding). Junction probing does NOT attribute anything per rule — it records the
junction's *total* surface↔underlying map, which is exactly the object analysis needs. HC itself
applies the cascade during the probe, so feeding/bleeding, α-variables (concrete segments — no
symbolic expansion), boundary markers (present in the probe string by construction), and MPR gating
(probe ungated → over-propose → verify rejects) all come for free. Everything stays
verify-backstopped, so a misread alignment costs a rejected candidate, never a wrong answer.

---

## Phase A — measurement: exact gap lists — ✅ DONE (2026-07-02)

1. **Indonesian**: ran `FstSenaBenchmark.Diagnose_Divergences` against the full 121-word corpus
   (`HC_MAX_WORDS=121`). Result: **28 divergent words, zero compounds** — every missed analysis has
   a single `RootMorphemeIndex`. Two clean buckets, exactly matching Phases C/D below:
   - **21 simple meN- forms** (Phase C target): `melangit`, `melempar`, `melihat`, `memakai`,
     `memasak`, `memukul`, `menanti`, `mengaca`, `mengaco`, `mengamat-amati`* , `menganga`,
     `mengarang`, `mengirim`, `menikah`, `menulis`, `menyanyi`, `menyatu`, `menyewa`, `merancang`,
     `merasa`, `mewakili`, `meyakini` (*`mengamat-amati` also has a `LOC` suffix stacked on).
   - **7 REDUP-meN forms** (Phase D target): `membagi-bagi`, `memijit-mijit`, `meminta-minta`,
     `mengayuh-ngayuh`, `menulis-nulis`, `menyewa-nyewa`, plus `mengamat-amati` above (dual-tagged:
     redup + phonology both needed).
   No compound ever appears in the oracle set for any of the 121 words — **Phase E is confirmed
   unnecessary for Indonesian.** The census also reconfirmed the known escapes: `-Cont`/`-Pl`/
   `REDUP-meN` (all reduplication, unbounded-copy escapes) and `Nasalization in reduplication`
   (unbounded left environment) — consistent with the plan's Phase D framing (that rule never needs
   to compile; the peel handles it on the surface side).
2. **Sena**: did NOT re-run a second 200-word oracle sample — the known pathology (some words need
   12–90+ s unbounded search, one OOM-crashed an in-process test host in the prior session) makes
   that expensive to redo safely, and the existing 99.2%-of-engine-parseable result already isolates
   exactly one gap class. Instead, ground-truthed the known gap directly: a bounded (30 s timeout)
   diagnostic ran `Morpher.AnalyzeWord("ndikhali")` and printed every analysis. Result: **8 analyses,
   all of the shape `{9,1,10,5}+é+ser+NZR`, with `RootMorphemeIndex` alternating between 1 (`é`) and
   2 (`ser`)** — i.e. `ndikhali` = `ndi` ("é", root, PoS `pos69519`) compounded with `khal` ("ser",
   root, PoS `pos87418`) via Sena's real `CompoundingRule` (`mrule7`/`mrule8`, confirmed in
   `sena-hc.xml`: `mrule8` has `headPartsOfSpeech="pos69519"`, `nonHeadPartsOfSpeech="...pos87418..."`),
   THEN the `-i` NZR suffix (`mrule9`) attaches to the compound's output PoS (`pos80535`). The
   leading `{9,1,10,5}` morpheme is a null-surface noun-class agreement marker (class 9/10 nouns
   in Sena take a zero prefix) — it contributes 0 phonetic content, which is why `ndi+khal+i` alone
   spells the 8-letter surface `ndikhali` exactly.
   **This corrects the archived plan's guess** ("prefixal derivation layer" would close it) — it is
   a genuine TWO-ROOT compound, not a single-root derivational prefix. Closing it for real needs the
   Phase E `WordAnalysis.RootMorphemeIndex` multi-root lift, not a trie-builder tweak. See Phase B
   below for the resulting scope call.

Exit: gap tables above. Everything after this phase is sized by real data — and Phase C/D (28
Indonesian words, ~23% of that corpus) is unambiguously the higher-value target vs. Sena's 1-word
gap (~0.014% of its corpus) that would require the biggest, most cross-cutting change in this plan.

## Phase B — Sena morphotactic closure — ⚠ INVESTIGATED, DEFERRED (not a small fix after all)

Phase A's ground-truthing found `ndikhali`'s gap is NOT a missing prefixal-derivation layer (the
archived plan's guess, made without ever running the engine on this word) — it is a genuine
two-root compound (`é` ⊕ `ser`, via Sena's real `CompoundingRule`), so closing it requires the same
`WordAnalysis.RootMorphemeIndex` single-scalar lift Phase E already scopes for Indonesian
compounding (extending `WordAnalysis`/`MorphToken` to carry multiple root positions, a compounding
candidate generator, `FstReplay.Confirm` pinning two roots — cross-cutting across `FstReplay`,
`FstVerification`, `CompositeProposer`, and every `Sig`-style function). That is Phase-E-sized work
to close ONE word in a 7,121-word corpus already at 99.2% of engine-parseable coverage (120/121 on
the previously-measured sample) — disproportionate next to Phase C/D's 28-word, ~23%-of-corpus win
on Indonesian. **Decision: defer, same as the original plan's Phase 4.3 compounding call** — do
Phase C and D first (they need zero data-model changes), then revisit Sena's `ndikhali` only if
Phase E ends up being built anyway for some other reason. If Phase E is never built, this stays a
documented, understood residual (unlike the archived plan's guess, its actual cause is now on
record) — update `KNOWN_GAPS` accordingly rather than leaving the stale "prefixal derivation" theory
in place.

## Phase C — Indonesian: junction-variant compilation (the core piece) — ✅ DONE (2026-07-02)

**What actually shipped is simpler than the original design below** (kept for the record — the
"full window + re-implement the cascade" plan was NOT needed): `FstTemplateAnalyzer` already had a
`SurfacePhonology`-precompiled surface-variant mechanism for every affix (`Variants(underlying)`,
probing one neighbor segment on each side and reading back the morpheme's own portion when the
result is length-preserving). Investigation found that mechanism ALREADY discovers the correct
`mem`/`men`/`meng`/`meny` assimilated-nasal prefix variants for free — it only needed a probe with a
non-deleting representative of each place class (e.g. voiced `b`/`d`/`g`) to "unlock" the variant,
and Indonesian's grammar always has one. Two real gaps remained, both fixed with much smaller,
targeted changes:

1. **`SurfacePhonology`'s deleted-node rendering bug** (`SurfaceOf`/`AddBoundaryVariant`): HC marks
   a deletion via `ShapeNode.IsDeleted()` rather than removing the node (confirmed via code read of
   `NarrowSynthesisRewriteSubruleSpec.cs` — the node stays in the `Shape`'s linked list, same
   position, same original `FeatureStruct`, just flagged), so the OLD rendering loop still printed
   the pre-deletion segment's own representation instead of nothing. Fix: a shared `RenderNodes`
   helper that skips `IsDeleted()` nodes when building the surface string. This alone closed the
   **nasal-deletion-before-sonorant** case (`Nasal deletion`, prule2) — `melangit`, `melempar`,
   `melihat`, `menanti`, `mengaco`, `menganga`, `menikah`, `menyanyi`, `merancang`, `merasa`,
   `mewakili`, `meyakini` (12 words) — with ZERO new mechanism, just the rendering fix.
2. **New `SurfacePhonology.DeletionJunctions(underlying)`**: for the remaining case — the cascade
   deleting the NEIGHBOR itself (assimilation feeding `Voiceless obstruent deletion`, prule4+prule5)
   — probes each alphabet representative as a right neighbor (falling back to a SECOND trailing
   neighbor when the first alone doesn't trigger deletion, since `Voiceless obstruent deletion`'s own
   `RightEnvironment` needs a vowel *beyond* the deleted segment — the exact shape that broke the
   first, single-neighbor-only version of this method during testing) and returns
   `(affixSurface, deletedNeighborFeatureStruct)` pairs. `FstTemplateAnalyzer` gained **root-chain
   checkpoints** (`_rootCheckpoints`, `RootChainAfterSkip`) — states reached after consuming 0, 1, 2…
   of a root's own leading segments — so a junction-deletion outcome can be wired to "skip the root's
   deleted onset" via a build-time gate (`WireDeletionSkips`: only for roots whose own leading
   segment unifies with the recorded class — never a blind skip). This closed `memukul`, `mengaca`,
   `mengarang`, `mengirim`, `menulis`, `menyatu`, `menyewa`, `memakai` (8 words).

No window-size computation, no re-implemented cascade, no Pinv/lockstep involvement, and no
`roots × affixes` cost: both mechanisms are bounded by `|junction affixes| × alphabet` (or ×
alphabet² for the two-neighbor fallback) — a few hundred probes total, independent of lexicon size.

**Measured result** (`Benchmark_CompositeVsSearch`, full 121-word Indonesian corpus): **114/121
fully covered** (up from 93/121 pre-Phase-C), **0 unsound**, **0 false positives**
(`Soundness_NegativeExamples`, 50/50 clean). The only 7 remaining gaps are ALL `REDUP-meN`
reduplicated forms (`membagi-bagi`, `memijit-mijit`, `meminta-minta`, `mengamat-amati`,
`mengayuh-ngayuh`, `menulis-nulis`, `menyewa-nyewa`) — exactly Phase D's target, nothing left over
for non-reduplicated words. Full 118-test HermitCrab suite green (was 116; +2 new toy-grammar
tests), CSharpier clean. Sena unaffected by construction (0 phonological rules ⇒
`DeletionJunctions` always returns empty there — not re-measured this session, see Phase A note on
the cost of a full Sena oracle re-run).

**Tests**: `SurfacePhonologyJunctionTests.cs` (new) — a toy grammar with a boundary-abutting prefix
(`m+`) and a `RewriteRule` requiring BOTH a left-boundary AND a right-context vowel beyond the
deleted segment (deliberately exercising the two-neighbor fallback):
`Junction_RecoversRootOnsetDeletion_RequiringTwoSegmentProbe` (positive: `FstTemplateAnalyzer`,
`VerifiedFstAnalyzer`, and the real engine all agree; a non-word yields nothing) and
`Junction_DoesNotSkip_WhenRootOnsetIsNotTheDeletedClass` (soundness: a root starting with a
different, non-deleting class must never get the skip arc — verified by checking the "wrong" skip
target is NOT recoverable, not just that the right one is).

**Original design (superseded by the simpler mechanism above — kept for context on what was
considered and why it wasn't necessary):** build, for each junction-bearing affix allomorph and each
candidate root onset in the alphabet plus one representative following segment, an explicit
underlying window (`affix-tail + boundary + onset + context`), run the full phonological cascade via
`CompileSynthesisRule` reused across the whole rule list, and emit junction arcs from the recorded
surface↔underlying alignment. The actual mechanism reuses the EXISTING per-affix `Variants`
precompile for the substitution-only outcomes (assimilation, default-nasal) and only adds new
machinery (`DeletionJunctions` + root-chain checkpoints) for the one case that mechanism structurally
cannot express (a NEIGHBOR disappearing) — smaller surface area, less new code, same soundness
guarantees.

## Phase D — reduplication × phonology (the `-X-X` forms) — ✅ DONE, 6/7 (2026-07-02)

Corpus words: `membagi-bagi`, `meminta-minta`, `memijit-mijit`, `mengamat-amati`,
`mengayuh-ngayuh`, `menulis-nulis`, `menyewa-nyewa`.

**The construct is `-Cont` (mrule13), not `REDUP-meN` (mrule15, glossed RECIP — unused by any of
these words)** — confirmed by tracing the real engine's analysis (`AV+write+Cont`, `AV+divide+Cont`,
…), a plan-writing-time misreading corrected during execution. **`-Cont` is also glossed `Cont`,
matching the divergence table** (`FstSenaBenchmark.Diagnose_Divergences` labels each missed
analysis by its morpheme glosses, which is what surfaced this).

**What actually shipped**, via a bounded-cost extension to the EXISTING `ReduplicationProposer`
(no new proposer class): confirmed via a custom `ITraceManager` logging every `MorphologicalRuleUnapplied`
step that `-Cont` produces `[meN-word] + "-" + [nasal+stem, WITHOUT the literal "me" text]` — e.g.
`menulis-nulis`, where `nulis` is exactly `menulis`'s own trailing 5 characters. This is NOT
"copy the whole prefixed word" (the `-` + full copy the plan originally guessed) — it is a genuine
**TAIL copy separated by a literal character**, one shape narrower than `ReduplicationProposer`
already handled (adjacent, no separator, either full-word or tail-vs-tail). Added a third scan to
`ReduplicationProposer.AnalyzeWord`: for every position `sepPos`, treat `word[sepPos]` as a literal
separator and check whether everything after it is a genuine surface tail of everything before it
(`before.EndsWith(copy)`); on a match, recurse the residual (`before`) through the existing FST
proposer and wrap with the redup morpheme, exactly like the other two scans. No new mechanism, no
window/separator-character enumeration needed — the scan is separator-CHARACTER-agnostic (it
doesn't need to know `-` is special; a wrong guess is pruned by verify like any other candidate
here), which is why it needed no new field or grammar introspection.

**`Nasalization in reduplication` (prule3 — unbounded `OptionalSegmentSequence` + α-vars, the one
rule that can never fit any bounded compiler) never needed to compile**, confirmed: it only fires
inside redup copies, which the surface-level tail-copy scan matches without any phonology-aware
machinery at all.

**Measured result**: 6 of 7 corpus words fixed — `membagi-bagi`, `memijit-mijit`, `meminta-minta`,
`mengayuh-ngayuh`, `menulis-nulis`, `menyewa-nyewa`. Indonesian composite coverage: **114/121 →
120/121**, still 0 unsound, 0 false positives (`Soundness_NegativeExamples` unchanged, 50/50 clean).

**Residual: `mengamat-amati` (1 word, NOT fixed).** Traced separately: `me(ng)+amat+-amat+i` — the
`-i` (LOC) suffix attaches to ONLY the reduplicated copy (`amat+i` = `amati`), not to the whole
word. `"amati"` is NOT a tail of `"mengamat"` (last 5 chars are `gamat`, not `amati`), so the
tail-copy scan correctly does not fire on it — this is a materially different shape (an affix
stacked onto just the copy) that the current scan does not attempt. Closing it would need either
(a) trying "strip a known suffix surface off the copy, then tail-match the remainder" — real new
mechanism, grammar-introspection-dependent, not just a scan-shape extension — or (b) a multi-group
`Lhs` pattern reconstruction of `-Cont`/`-i`'s real interaction, which is exactly the kind of
unvalidated-pattern-API territory Phase 4's own CV-reduplication work already declined to attempt
under time pressure (no test in this repo builds a multi-group `Pattern`). Documented as a known
residual (added to `KNOWN_GAPS`) rather than pursued further — one word out of 121, against a
120/121 result, did not justify the added mechanism's risk/complexity for this session.

**Tests**: `VerifiedFstAnalyzerTests.Composite_CoversSeparatorReduplication_WhereFstAloneMisses`
(toy grammar: a full copy with a literal separator, `sagzsag`; soundness check that a tail-copy
candidate — `sagzag`, which passes the surface-shape scan but isn't what this toy rule's FULL-copy
semantics actually produce — is correctly rejected by verify). A toy grammar exercising the REAL
partial-tail shape (requiring a multi-group `Lhs` pattern) was not built, same call as Phase 4's
CV-reduplication case; the full Indonesian corpus benchmark is the positive evidence for that shape.

**Gate**: 6/7 engine-parseable redup corpus words fully covered, 0 unsound. `mengamat-amati` is a
documented residual, not a silent gap. Committed.

## Phase E — ❌ CANCELLED (2026-07-03): the premise was falsified by a code re-read

This phase scoped a "cross-cutting `WordAnalysis.RootMorphemeIndex` data-model lift" for
compounding. A direct re-read of `MorphToken.cs` and `FstReplay.cs` on 2026-07-03 showed the
data model ALREADY supports compounds (`MorphOp.Compound` exists; the engine emits two-root
`WordAnalysis` objects today — the `ndikhali` diagnostic printed them) and the only real blocker
is ~6 lines in `FstReplay.Confirm`. **See Phase G2 below for the actual spec.** Kept here so the
original (wrong) reasoning stays on record.

## Phase F — hardening + final gates — folded into Phases H and I below

- The **frontier beam cap** moves into Phase I (it belongs with the walker generalization).
- Final-numbers reporting is now the standing "stats battery" requirement in the execution specs.
- `FST_FAST_PATH_PLAN.md` STATUS + KNOWN_GAPS updates: partially done 2026-07-02/03 (boundary-gap
  moot-for-Indonesian note, compounding-premise correction, `mengamat-amati` entry); keep
  maintaining as G/H/I land.

---

# EXECUTION SPECS FOR THE NEXT SESSION (written 2026-07-03, for Sonnet)

Everything below is speced from a direct code re-read on 2026-07-03 (file/member references
verified that day). Work each phase to green (full suite + the phase's own gates) and commit
before starting the next. **Always report the stats battery with every result** (this is a
standing requirement from John, not optional): FST `StateCount`, build wall-time (note JIT-cold
vs warm — run the build twice in-process and report the second), and verified-walk p50/p95 ms/word.

## Current measured baseline

**Pre-Phase-H (2026-07-03, before H1/H2, this machine, Debug build, warm where noted):**

| | Indonesian | Sena |
|---|---|---|
| FST states (bare, morpher ctor) | 532 | 20,737 |
| FST states (trie-only, no-morpher ctor) | — | 15,901 |
| Bare FST build | 682 ms (JIT-cold; mostly JIT) | 9,281 ms cold / 8,920 ms warm |
| Grammar load (XML) | — | 245 ms |
| GenerateWords loop (1,463 allomorph calls) | — | ~175 ms |
| Trie-only build (no probing) | — | **105 ms** |
| `Variants` × 25 distinct affixes (memoized) | — | 47 ms |
| `DeletionJunctions` × 25 distinct affixes, ONCE each | — | **746 ms** |
| Verified-composite walk p50 / p95 / p99 | 1.8 / 14.7 / 21.6 ms | 49.8 / 288 / 893 ms (first 150 words) |
| Coverage (set parity vs oracle) | **120/121, 0 unsound** | 58/60 slice; 99.2% of engine-parseable (200-sample) |

**Post-Phase-H (after H1+H2 landed — see Phase H status for the state-count note):**

| | Indonesian | Sena |
|---|---|---|
| FST states (bare, morpher ctor) | 532 (unchanged) | **16,322** (was 20,737 — see Phase H) |
| Bare FST build | 266 ms | **~1.0–1.1 s** (cold and warm alike; was 8.9–9.3 s) |
| Coverage (set parity vs oracle) | **120/121, 0 unsound** (unchanged) | 55/57 guarded slice (60 words, 5s/word cap, 3 excluded), 0 unsound |

**Post-Phase-G1+G2 (2026-07-03, final this session):**

| | Indonesian | Sena |
|---|---|---|
| FST states (bare, morpher ctor) | 533 (+1, compound-loop join state) | 16,347 (+25 vs. post-H) |
| Bare FST build | ~433 ms | ~1.3–1.5 s |
| Coverage (set parity vs oracle) | **121/121, 0 unsound, 0 false positives** | 57/57 guarded slice (60 words, 5s/word cap, 3 excluded), 0 unsound; **`ndikhali` 8/8 exact set parity** |

## Phase H — ✅ DONE (2026-07-03): Sena build time 9.3 s → ~1.0–1.1 s

**H1 (memoize `DeletionJunctions`) and H2 (capability-gate `Variants`/`DeletionJunctions` on
`_anyPhonologicalRules`/`_anyDeletionSubrule`) landed together** in `SurfacePhonology.cs` — same
pattern as speced below, both in one pass since they touch the same lines. **Measured: Sena build
9.3 s → 1.0–1.1 s (cold and warm alike), Indonesian unaffected (266 ms, has real deletion subrules
so its gates stay open).** This is short of the ~0.3–0.5 s originally estimated; the remaining
~1 s is trie construction (105 ms measured standalone) plus `GenerateWords` (175 ms) plus JIT/other
overhead not isolated further — good enough that Phase H's practical goal (fast edit-loop
iteration) is met, and further squeezing wasn't pursued.

**A real, unexplained side effect: Sena's `StateCount` dropped from 20,737 (Phase C/D's own
number, measured 2026-07-02) to 16,322 after H1+H2 — not identical, as this doc's gate below
originally demanded.** Investigated rather than dismissed: the gate's own reasoning predicts
IDENTICAL variant sets before/after (a 0-phonological-rule grammar's un-gated `ComputeVariants`
should already degenerate to `{underlying}` only, since an empty rule cascade changes nothing —
verified by hand-tracing `AddBoundaryVariant`'s behavior with a no-op cascade). The most likely
explanation not fully confirmed: some affix's underlying string, round-tripped through
`_table.Segment` + `GetMatchingStrReps` under the OLD (un-gated) path, produced a
string-identical-but-FeatureStruct-distinct "variant" that `BuildAffixArcs`' dedup-by-string-value
check (`if (variant == underlying) continue`) does NOT catch (it dedups by the RENDERED STRING,
not by the resulting FeatureStruct sequence), building a redundant-but-distinct arc chain. H2's
gate short-circuits before that round-trip ever happens, removing the redundant states. **This
was NOT chased to a certain root cause** (would need instrumenting `BuildAffixArcs`), because the
gates that actually matter — coverage and soundness — were reverified directly and are unaffected:
Indonesian `Benchmark_CompositeVsSearch` **120/121, 0 unsound, identical to before**; a per-word-
timeout-guarded Sena coverage check (first 60 words, 5 s/word cap, full random-corpus oracle
comparison is the known-hazardous one) showed **55/57 fully covered (3 timed out, excluded), 0
unsound** — consistent with the known single-gap pattern, no regression signature. Full 119-test
suite green throughout. Treat "StateCount decreased, unexplained but coverage/soundness verified
unaffected" as the honest status — a future session touching `BuildAffixArcs`'s dedup should
resolve this fully rather than re-litigate it.

**H3 (stop building the FST twice in the composite path) — turned out not to be a real bug;
struck.** The plan's evidence for H3 ("bare FST build 8.7 s + composite build 9.8 s back-to-back")
came from the DIAGNOSTIC SCRIPT that produced that measurement, which itself constructed
`new FstTemplateAnalyzer(language, morpher)` twice (once standalone, once inline as an argument to
`CompositeProposer.ForLanguage`) — an artifact of the measurement code, not the library. Checked
the actual call sites: `FstCoverageProbe.ForLanguage` builds ONE `FstTemplateAnalyzer` and passes
it to `CompositeProposer`'s instance constructor (not `.ForLanguage`), sharing it correctly.
`CompositeProposer.ForLanguage(language, fst, ...)` itself takes an already-built `fst` and never
constructs another. The only place two independent (real, morpher-based) FSTs get built is
`FstSenaBenchmark.Benchmark_CompositeVsSearch`'s OWN comparison code (`bare` vs `composite`
deliberately use separate instances to compare them) — and now that H1+H2 make a build ~1 s, that
duplication costs ~1 s of benchmark time, not worth touching. `LockstepPhonologyProposer` builds
a SEPARATE, but cheap (~105 ms, no-morpher/no-probing ctor), internal `FstTemplateAnalyzer` — a
minor, harmless redundancy, not the reported 8-9 s. No code change made for H3.

**Verification gates actually run:**
- `dotnet test --filter "TestCategory!=Explicit"` → 119/119 green; CSharpier clean.
- Indonesian `Benchmark_CompositeVsSearch` (`HC_MAX_WORDS=121`): **120/121 fully covered, 0
  unsound, 0 false positives** — identical to pre-H.
- Sena: per-word-timeout-guarded coverage check (60 sequential words, 5 s cap) — 55/57 fully
  covered (3 excluded on timeout, a known pre-existing hazard unrelated to this change), 0
  unsound. Full unbounded `Benchmark_CompositeVsSearch` on Sena still hangs on pathological words
  regardless of this session's changes (same as every prior session — not attempted further).
- `StateCount`: Indonesian identical (532); Sena dropped 20,737 → 16,322 (see above — investigated,
  not fully root-caused, coverage/soundness confirmed unaffected by two independent checks).

## Phase G1 — ✅ DONE (2026-07-03): `mengamat-amati` closed, Indonesian now 121/121

Implemented exactly as speced below (`ReduplicationProposer.cs`): collected suffix surface texts
in the constructor (boundary-stripped via `HCFeatureSystem.Segment`-only rendering, catching
Indonesian's `-i` being underlyingly `"+i"`), added the suffix-peel fallback to the separator
scan, threaded an optional `extraSuffix` parameter through `ProposeForResidual`. **Measured:
Indonesian `Benchmark_CompositeVsSearch` — 121/121 fully covered (was 120/121), 0 unsound, 0
false positives; `Diagnose_Divergences` — zero divergent words.** New toy test
(`Composite_CoversSuffixStackedOutsideReduplication_WhereSeparatorScanAloneMisses` in
`VerifiedFstAnalyzerTests.cs`) passed on the first run — the real engine happily stacked a plain
suffix rule on top of the toy reduplication rule with no PoS-gating adjustment needed (both rules'
`RequiredSyntacticFeatureStruct`/`OutSyntacticFeatureStruct` were `V`→`V`, and the stratum's
default `MorphologicalRuleOrder.Unordered` let HC try the stack). Full 120-test suite green
(was 119; +1). No regression on the toy-grammar suite or Indonesian's existing coverage.

Ground truth (traced 2026-07-02 with a logging `ITraceManager`): the engine's analysis is
`AV+observe+Cont+LOC`, i.e. `-i` (LOC) suffixes the WHOLE reduplicated word:
`meng+amat` → `-Cont` → `mengamat-amat` → `-i` → `mengamat-amati`. The current separator scan
splits at `-` into `before="mengamat"`, `copy="amati"`, and `"amati"` is not a tail of
`"mengamat"` — correctly no match. The fix is to peel known suffix surfaces off the END of the
copy before tail-matching (this closes the whole class "any suffix stacked outside the
reduplication", not just this word):

1. In `ReduplicationProposer`'s constructor, alongside `_redupRules`, collect suffix surface
   strings: for every stratum's `MorphemicMorphologicalRule` whose allomorph classifies as
   `MorphOp.Suffix` (`MorphTokenCodec.ClassifyOp(allomorph, false)`), take the allomorph's
   `InsertSegments.Segments.Representation`, segment it via the surface stratum's
   `CharacterDefinitionTable.Segment(...)`, keep only `HCFeatureSystem.Segment`-type nodes, and
   render their string reps (`GetMatchingStrReps(node).First()`). **This boundary-stripping step
   is required**: Indonesian's `-i` inserts `"+i"` (the `+` is boundary `char30`), and the raw
   representation would never match surface text. Store `(string SurfaceText, IMorpheme Rule)`
   pairs; skip empty results.
2. In the separator scan (third loop of `AnalyzeWord`), when the plain
   `before.EndsWith(copy)` check fails, additionally try each collected suffix pair: if
   `copy.EndsWith(s.SurfaceText)` and the remainder `copy[..^s.SurfaceText.Length]` is non-empty
   and IS a tail of `before`, then for each analysis from `ProposeForResidual(before)`, emit a
   variant with the suffix morpheme appended AFTER the redup morpheme (engine order:
   `…root…, Cont, LOC` — redup first, then the outer suffix). Easiest shape: give
   `ProposeForResidual` an optional `IMorpheme extraSuffix` parameter appended after the redup
   wrap; `RootMorphemeIndex` is unchanged (both additions are after the root).
3. Do NOT recurse suffix-peeling (one suffix layer is what the corpus needs; unbounded stacking
   here would be scan-cost without evidence). Note the single-layer bound in the class remarks.

**Tests + gates:**
- Extend the toy grammar in `Composite_CoversSeparatorReduplication_WhereFstAloneMisses` (or add a
  sibling test): add a plain suffix rule (e.g. Table1 `"s"`), assert the engine parses
  `sagzsags` (= CONT(`sag`) + suffix; confirm the toy engine really produces this before asserting
  — if HC's rule ordering rejects suffix-after-redup in the toy setup, adjust the toy PoS gating
  until the ENGINE parses it, then assert parity), assert the composite covers it, and assert a
  soundness negative (e.g. `sagzdats`) stays empty.
- Indonesian `Benchmark_CompositeVsSearch` (`HC_MAX_WORDS=121`): **121/121 fully covered, 0
  unsound** — this is the phase gate and the whole point.
- Full suite green, CSharpier, stats battery (walk p50/p95 must not measurably regress — the new
  scan branch only runs on words containing a separator character that already failed the plain
  tail match).

## Phase G2 — ✅ DONE (2026-07-03): `ndikhali` closed with EXACT set parity (8/8)

**Confirmed correct: the "data-model lift" premise WAS false.** `MorphOp.Compound` already existed,
`WordAnalysis` already represented compounds, and the only hard blocker really was `FstReplay.Confirm`
— implemented exactly as speced (step 1 below). **But the spec UNDER-ESTIMATED one thing**: for
`ndikhali` specifically, a THIRD piece was needed beyond `FstReplay` + the trie loop — see "What the
spec missed" below. Implemented in `FstTemplateAnalyzer.cs`, `FstReplay.cs`.

**What shipped, matching the spec:**
1. **`FstReplay.Confirm`**: non-head `LexEntry` morphemes go into a `HashSet<LexEntry> extraRoots`
   instead of triggering an early `return null`; `LexEntrySelector = e => e == root ||
   extraRoots.Contains(e)`; `RuleSelector` gains `|| (extraRoots.Count > 0 && r is CompoundingRule)`.
2. **Trie compound loop**: `BuildCompoundLoop(roots, continuation)` — one shared "join" state per
   attachment site (template-less path, and each template) with an ε-arc into every root's shared
   chain `Entry`; every qualifying root's chain `End` gets an ε-arc to the join (alternative to its
   normal continuation) AND every root's chain `End` gets an ε-arc from the join's downstream back
   to `continuation`. Bounded to one extra root (no arc back into the join).
3. **Headedness via token post-processing**: `ToWordAnalyses` (renamed from `ToWordAnalysis`,
   now `IEnumerable<WordAnalysis>`) scans a token array for `MorphOp.Root` positions; 0 or 1 →
   the old single-candidate behavior; 2+ → one `WordAnalysis` per root position as
   `RootMorphemeIndex`, same morpheme list. Both `AnalyzeShape` and `AnalyzeComposed` updated to
   `AddRange` instead of `Add`.
4. Gated on `hasCompoundingRules` (any `CompoundingRule` in any stratum) — zero cost for a grammar
   without one.

**What the spec missed (found during implementation, fixed):**
- **The compound loop must be reachable even without OTHER standalone derivational rules.** The
  spec's own step 2 said "add the loop" but didn't notice the loop lives inside the template-less
  path's `if (_derivPrefixRules.Count > 0 || _derivSuffixRules.Count > 0)` block — a grammar with
  compounding but no other standalone prefix/suffix rule (my own toy test hit exactly this) never
  built the block AT ALL, so the loop silently never existed. Fixed: the guard is now
  `|| hasCompoundingRules`. Both real grammars have standalone derivational rules too, so this
  never manifested on Sena/Indonesian — only on a minimal toy grammar — but it would have bitten
  the next grammar tried.
- **`ndikhali` needed a THIRD extension: `DerivableToCategory` must treat compounding as a
  category-transition edge, not just `_derivSuffixRules`/`_derivPrefixRules`.** Root cause (found
  via reflection-inspecting `_derivPrefixRules`' actual contents, then a rule-application trace):
  Sena's noun-class markers (glossed `"1"`/`"9"`/`"10"`/`"5"`, e.g. `mrule56`) are NOT standalone
  derivational rules — `_derivPrefixRules` came back with only 4 unrelated entries, none of them
  class markers. They are class-agreement PREFIX-TEMPLATE-SLOT rules requiring `pos100407` as
  their OWN input category — which is NZR's (`-i`, gloss `NZR`) OUTPUT category, which is in turn
  reachable only via `[é ⊕ khal compound] → NZR`. Since a template's root-attachment gate
  (`CategoryMatches || DerivableToCategory`) never considered COMPOUNDING as a way to change
  category, neither `é` nor `khal` ever qualified for the class-marker template at all — the
  compound loop's OWN pairing worked fine (confirmed: `é+ser+NZR` candidates without a class
  prefix appeared immediately), but the template carrying the class prefix was unreachable.
  Fixed by adding a `_compoundingRules` list (collected in the constructor) and extending
  `DerivableToCategory`'s frontier-expansion loop with a second edge type: for each category in
  the frontier, if it unifies with a compounding rule's `HeadRequiredSyntacticFeatureStruct` OR
  `NonHeadRequiredSyntacticFeatureStruct` (permissively — either role, no partner-root check, same
  philosophy as every other gate in this file), `OutSyntacticFeatureStruct` becomes a new frontier
  node. Since the BFS already runs `_derivDepth` iterations trying any available edge at each
  step, this one addition makes "compound, then derive further" chains fall out for free — no
  other structural change needed.

**Measured result**: Sena's `ndikhali` — **8/8 exact set parity, sound** (all four class markers ×
both head orderings, matching the engine's own 8 analyses exactly). Guarded 60-word Sena slice:
**57/57 fully covered** (up from 55/57 pre-G2), 0 unsound. Indonesian (`HC_MAX_WORDS=121`):
**unchanged at 121/121, 0 unsound, 0 divergent words** — its compounding rules (`mrule1`/`mrule2`)
now build the loop too, but the corpus needs no compound analyses (confirmed in Phase A), so
verify correctly prunes every proposed compound; `Soundness_NegativeExamples` 0 false positives on
both grammars. Full 121-test suite green (was 120; +1). Stats: Indonesian states 532→533 (+1, the
compound-loop join state — Indonesian's template-less path already existed for other reasons, and
the loop adds exactly one join state there); Sena states 16,322→16,347 (+25, one join state per
template + the template-less path); build time Indonesian ~266ms→~433ms, Sena ~1.0–1.1s→~1.3–1.5s
— both still far below the pre-Phase-H 9.3s baseline. Walk p50/p95 not separately re-measured
this session (no regression signal in the guarded coverage run's wall-clock).

**Tests**: `Fst_CoversCompound_ViaTheCompoundLoop` (`VerifiedFstAnalyzerTests.cs`) — a toy grammar
with an unrestricted `CompoundingRule` (no head/non-head PoS gating, matching
`CompoundingRuleTests.cs`'s existing pattern reused here) and two roots (`pat`, `tak`); asserts the
engine parses the compound, the BARE `FstTemplateAnalyzer` alone now proposes it directly (no
sibling generator needed — the mechanism lives in the trie itself, unlike reduplication/infix), and
soundness via `CompoundingRule`'s own default `MaxApplicationCount = 1`: a three-root chain
(`pattakpat`) is rejected by both the real engine and the verified FST, confirming the loop is
correctly bounded to exactly one extra root.

**Correction note for future readers**: the "Tests + gates" bullet below calling for a
"head/non-head PoS-gated" toy grammar was written before implementation; the toy test that shipped
uses an UNGATED compounding rule instead (simpler, and the PoS-gating behavior is already exercised
for real by Indonesian's `mrule1`/`mrule2` staying silent on its own non-compound corpus, and by
`ndikhali`'s real class-agreement gating on Sena). A dedicated PoS-gated toy test was judged
redundant given those two real-grammar checks.

## Phase I — the true-FST generalization (lazy per-rule chain) — FULL EXECUTION SPEC (2026-07-03)

> Speced for implementation in the same style as G1/G2/H (which executed cleanly from these specs).
> This is the largest remaining item — realistically **6–9 days** across seven commit-gated
> milestones (I0–I7, below), plus an optional I8. Unlike G/H it is not driven by a failing corpus
> word: its purpose is to make the fast path correct-by-construction for **arbitrary regular HC
> grammars**, not just the two measured ones. Everything below was written against a code re-read
> of `InversePhonology.cs`, `FstTemplateAnalyzer.AnalyzeComposed`/`ComposedClosure`,
> `RewriteRule` (`Direction`, `ApplicationMode`), and `SIL.Machine.Matching`'s node inventory
> (`Constraint`/`Quantifier`/`Group`/`Alternation` — the complete set an env compiler must handle).

### What it fixes that nothing else can

> **CORRECTED BY I2's EXECUTION (2026-07-03) — this section's claim was too strong.** The paragraph
> below is accurate about junction probing and v1's `LockstepPhonologyProposer`, but the FULL composite
> also contains `ComposedPhonologyProposer` (runtime un-application of the real cascade) and the
> surface-precompile mechanisms (`BareRootSurfaces` full-cascade synthesis, `Variants` probing), and
> those EMPIRICALLY cover all three I2 toy shapes at toy scale (live-asserted in `ChainWalkerTests`).
> What the chain uniquely provides is reaching these shapes in LOCKSTEP with the lexicon at FST-walk
> cost — the covering mechanisms are exactly the ones with measured scaling pathologies (Amharic:
> 112-s probing build, analysis-oracle blowup) that I7 retires by evidence. Boundary-conditioned
> word-internal rules (where un-application is blocked) remain genuinely chain-only. Full account in
> the I2 section below.

Junction probing (Phase C) and the peels are bounded LOCAL mechanisms — exact for grammars whose
phonology fires within ~2 segments of a morpheme boundary. They structurally cannot represent:
word-internal rules far from any boundary; long-distance harmony (a suffix vowel conditioned by a
trigger several syllables back); feeding/bleeding chains deeper than the probe window. The chain
handles all of these because each rule's inverse automaton carries its own state across the whole
word.

Theory anchor (so nobody re-litigates feasibility): SPE-style ordered rewrite rules are regular
(Kaplan & Kay 1994); lexc/xfst/HFST/foma have compiled full morphologies this way for decades. The
only provably non-regular construct is unbounded copying — which stays with the peel. The reason
eager composition exploded IN THIS CODEBASE is specific: arcs are FeatureStructs matched by
unification and cannot be determinized/minimized without destroying multi-analysis enumeration;
classical toolkits stay small because they minimize over a CONCRETE alphabet — and HC's surface
alphabet IS concrete and small (~30 chars/grammar). Lazy composition sidesteps the issue entirely:
the composed machine is **never materialized**, so state explosion is structurally impossible; the
risk moves to walk-time frontier width, which I6's beam cap bounds.

### Governing principle: SUPERSET, NEVER SILENT SKIP

Soundness comes from verify (`FstReplay`), so a rule's compiled inverse only needs to be a
**superset** of the true inverse relation — over-generation costs verify time, never correctness.
Every rule therefore compiles at one of three tiers, and the compiler must never claim "supported"
for something that under-generates:

- **Exact** — environments compiled precisely (including quantified/Kleene spans, see I1); minimal
  slop. The normal case.
- **Permissive** — some gating dropped (an env anchor it can't express, an MPR/syntactic-feature
  gate, a direction subtlety): still a superset, just more verify traffic. The automatic fallback.
- **Identity-skip** — the rule contributes only identity arcs (today's behavior for unsupported
  rules): words genuinely needing it fall to the engine. ONLY as an explicit per-rule escape hatch
  when Permissive measurably blows the beam (I6) — never a silent compiler default.

`ProbeReport` gains a per-rule tier report (rule name → Exact/Permissive/Identity-skip + reason),
replacing the bare `UnsupportedPhonologyRuleCount` integer. A grammar author must be able to see
exactly which rule is costing what.

### I0 — data-type groundwork (small) — ✅ DONE (2026-07-03)

1. Extend `InversePhonology.Arc` with **ε-output**: `UnderlyingOutput == null` = consume the
   surface/incoming symbol, emit nothing downstream (needed for epenthesis-inverse, I3). Add
   `IsEpsilonOutput`; audit the two existing consumers (`AnalyzeComposed`, `ComposedClosure`) to
   reject/ignore ε-output arcs until I2 lands (they can't appear yet — the v1 compiler never emits
   them — but make the assumption explicit, not accidental).
2. Each rule gets its OWN `InversePhonology` instance; the chain is
   `IReadOnlyList<InversePhonology>` in **reverse application order**. Do not trust this doc for
   the order — read `AnalysisLanguageRule`/`AnalysisStratumRule` and mirror exactly what the
   engine's own unapplication does (strata outermost-first, each stratum's phonological rules
   reversed).
3. Gates: build green, full suite green (pure additive change).

**What shipped**: point 1 only, in `InversePhonology.cs` and `FstTemplateAnalyzer.cs`. `Arc` gained
`IsEpsilonOutput` (`UnderlyingOutput == null`); `AnalyzeComposed`'s segment-consuming loop and
`ComposedClosure`'s ε-input branch each gained an explicit guard skipping (not silently
mishandling) an ε-output arc, with a comment noting no compiler emits this shape yet and I2 owns
the real emit path. **Point 2 (the `IReadOnlyList<InversePhonology>` chain type) was deliberately
NOT built in this commit** — confirmed via `AnalysisLanguageRule`/`AnalysisStratumRule`: the real
order is *strata in `language.Strata` reversed, and within each stratum `PhonologicalRules`
reversed* (both feed a `LinearRuleCascade`/sequential apply, confirming a true cascade, not
independent branches) — but nothing consumes a chain type until I2, so introducing one now would
be untested dead code, not "pure additive". The order finding is recorded here for I2 to use
directly rather than re-derive.

**Gates run**: `dotnet build` clean (0 warnings incl. the new XML doc `cref`s); baseline
established first (`dotnet test --filter "TestCategory!=Explicit"` on the pre-I0 tree: **121/121
green**, matching this doc's last recorded count) and reconfirmed after the change: **121/121
green, identical**. CSharpier clean on both touched files. **Stats battery**: no state/coverage
change is possible or expected from this commit (no proposer or compiler behavior changed — the
new `Arc` field is inert until a compiler sets it) — reporting that explicitly rather than omitting
the section, per the standing requirement.

### I1 — env-pattern→NFA compiler + Exact-tier substitution compiler — ✅ DONE (2026-07-03)

**What shipped**: `EnvNfaCompiler.cs` (new, `internal`) and `RuleInverseCompiler.cs` (new, `public`),
exactly as speced in points 1–2 below, with these scoping calls made during implementation:

- **Structural epsilon, a shape I0 didn't anticipate.** Building a real NFA fragment for
  `Quantifier`'s zero-occurrence bypass/unbounded loop-back and `Alternation`'s branch rejoin needs
  a transition that consumes and emits nothing but still moves the walk to a different state — I0's
  `Arc.IsEpsilonOutput` (consumes surface, emits nothing) doesn't cover this. Added
  `Arc.IsStructuralEpsilon` (both `SurfaceInput` and `UnderlyingOutput` null) and
  `InversePhonology.AddEpsilon`, and corrected I0's own comment in `FstTemplateAnalyzer.ComposedClosure`
  (it had called the both-null shape "a true no-op no compiler should ever construct" — wrong, once I1
  needed exactly this shape for NFA wiring; not a no-op, `Target` still moves state). Not fed through
  `AnalyzeComposed`/`ComposedClosure` this phase — I1's own tests walk the automaton with a standalone
  interpreter (per point 3 below); I2 owns wiring real closure handling for it into the shared walker.
- **Sequential composition needs no epsilon at all**: a fragment builder that takes `(node, startState)`
  and returns the reached end state lets mandatory quantifier copies and env fragments chain by literally
  starting the next piece AT the previous piece's returned state — only branching (quantifier
  bypass/loop-back, alternation rejoin) needs a real epsilon arc. Alternation's FAN-OUT needs no epsilon
  either (multiple arcs from one state is already valid NFA structure).
- **Anchors** (`Constraint.Type() == HCFeatureSystem.Anchor`) are dropped in `EnvNfaCompiler` (no arc,
  continue from the same state) and flagged `"anchor"` — correctly Permissive, since a real analysis-time
  walk could apply the branch anywhere, not just at the true word edge. Probing is UNAFFECTED by this
  (no separate flag needed there): the probe string is itself a standalone word, so its own edges already
  satisfy the anchor — confirmed by reasoning through `RewriteRuleTests`/`XmlLanguageLoader`'s anchor
  placement (always the outermost element of the pattern), not by trial and error.
- **α-variable agreement scoped down from the spec's letter.** The spec says "enumerate concrete alphabet
  bindings via unification... env↔target agreement falls out of enumerating consistent combos" — fully
  implementing that (per-binding branches, joint Lhs×env enumeration) was judged too large for this
  commit. Shipped instead: detect `FeatureStruct.HasVariables` anywhere in Lhs/Rhs/env and tag the rule
  Permissive (`"alpha-variable"`) rather than Exact; the ONE representative still probed (same technique
  as every other position) yields a real, verify-safe transformation, just not every agreeing binding.
  Confirmed via a real nasal-place-assimilation toy grammar (same construct as
  `RewriteRuleTests.AlphaVariableRules`) that this still recovers a genuine cross-candidate result (both
  `n` and `ŋ` assimilating to the first bilabial alphabet representative, `p`, both surface as `m`).
  Documented as an honest residual, not silently dropped — a future pass can add per-binding enumeration
  for the common case (a variable shared with the immediately adjacent environment segment) if
  measurement shows it matters.
- **Deletion/epenthesis (`Rhs.Count != Lhs.Count`) is explicitly NOT attempted** — confirmed as I3's
  scope, not a regression: this compiler adds no branch and no reason for that shape (distinct from
  Permissive — it isn't a precision loss within what THIS compiler claims to do).
- **The v1 boundary-representative bug (KNOWN_GAPS, `FST_FAST_PATH_PLAN.md`) is fixed as a side effect**:
  the probe-alphabet lookup now searches Segment ∪ Boundary character definitions (v1's `_alphabet` was
  Segment-only), so a `BoundaryMarker` constraint can find a representative instead of unconditionally
  failing before its shape is even evaluated.

**A real bug found via advisor review, not self-testing (worth recording how it surfaced):** the first
pass of this measurement showed only 1/5 Indonesian rules compiling (`Nasal deletion`/`Voiceless
obstruent deletion` correctly IdentitySkip as deletion, but `Unspecified nasal default` and
`Nasal assimilation` ALSO fell to IdentitySkip with reason `"no-effect"` — i.e., the probe claimed the
rule's own assimilation had no observable effect, which is false for a rule that demonstrably
reassigns place of articulation in real Indonesian). A targeted debug dump (temporary `RIC_DEBUG_RULE`
env-var instrumentation, removed after use) found the actual cause in TWO layers:
1. **String-concatenation re-segmentation ambiguity.** The original probe built one string
   (`leftProbe + candidateRep + rightProbe`) and re-segmented the WHOLE thing via
   `CharacterDefinitionTable.Segment` — for `Nasal assimilation`'s real environment (`_+p` — a boundary
   then a bilabial consonant, following the placeless-nasal target `ⁿ`), the joined string `"eⁿ+p"`
   re-segmented to only 3 nodes instead of the expected 4, most likely the table's maximal-munch
   matching merging two adjacent representative characters into an unintended multi-character grapheme
   at a join neither piece owns alone. **Fixed** by never building or re-segmenting a joined string at
   all: `BuildProbeRepresentative` now returns a `List<FeatureStruct>` directly (each env
   representative's own already-known `FeatureStruct`, not a string round-trip), and
   `TryCompileCandidate` assembles the probe `Shape` node-by-node
   (`Shape.Add(FeatureStruct)`) from three independently-known pieces (left-env representatives, the
   Lhs candidate's own segments, right-env representatives) — each piece's identity is never in
   question, so the ambiguity is structurally impossible, not just avoided by luck.
2. **A `before`/`after` filter mismatch this fix's OWN first draft introduced.** Once boundary
   characters could appear in `before` (the KNOWN_GAPS fix — env representatives now include
   Boundary-typed segments), `after` was still filtered to `Type() == Segment` only (v1's convention,
   safe there only because v1's probe alphabet never contained a boundary character at all) — silently
   dropping the boundary node from `after` but not from `before`, misaligning every position after a
   boundary and making the whole comparison look length-mismatched. **Fixed** by filtering `after` to
   Segment ∪ Boundary, matching `before`'s own composition exactly.

**Measured on the real grammars, after both fixes** (`TierReport_OnRealGrammar`, new `[Explicit]`
diagnostic, `HC_GRAMMAR` env var, machine-local paths under
`C:\Users\johnm\Documents\repos\machine\samples\data\`):
- **Sena** (0 phonological rules): 0 rules compiled — confirmed no-op, matching every prior session's
  finding.
- **Indonesian** (5 rules): `Exact=1, Permissive=2, IdentitySkip=2` —
  `Unspecified nasal default: Exact []`,
  `Nasal deletion: IdentitySkip []` (deletion — I3's scope, expected),
  `Nasalization in reduplication: Permissive [alpha-variable]` (a real compile — the plan's own
  Phase-D note said this rule "never needs to compile"; turns out it partially CAN, at Permissive),
  `Nasal assimilation: Permissive [alpha-variable]` (confirmed via the debug dump: the nasal's
  `OrthPlace` feature genuinely flips from unspecified to `labial` across the boundary — a real,
  verify-safe assimilation branch, not a false claim),
  `Voiceless obstruent deletion: IdentitySkip [mpr-or-syntactic-gate]` (deletion + gated — I3's scope).
  **This MEETS this doc's own pre-written expectation** ("3–4 Exact... boundary-env rules will be
  Permissive" — read as "the substitution-shaped rules reach ≥ Permissive," since deletion is
  out-of-scope by design until I3): all 3 in-scope substitution rules compile (1 Exact + 2 Permissive);
  the 2 IdentitySkips are both genuinely deletion rules.
- **Zero regression risk on either grammar's real coverage**: confirmed structurally (not just by
  inference) — `grep` shows no file outside `RuleInverseCompiler.cs`/`EnvNfaCompiler.cs` references
  either type; neither is wired into `LockstepPhonologyProposer`/`CompositeProposer`/`FstCoverageProbe`.
  The slow real-corpus `Benchmark_CompositeVsSearch` was therefore not re-run this session (nothing in
  its call graph changed).

**Tests**: `RuleInverseCompilerTests.cs` (new), 5 toy-grammar tests exactly matching point 3's list
(plain substitution, left+right env, quantified env span — the marquee long-distance-harmony case,
α-variable assimilation, 2-segment Lhs) plus the real-grammar `[Explicit]` diagnostic above. All 5 pass
via a standalone test-local interpreter (`RunPinv`/`Closure` in the test file). Four real bugs were
caught and fixed across this phase, two by the toy tests and two by real-grammar measurement (the
advisor flagged the second pair before they were mis-recorded as an understood, accepted limitation —
see above): (1) the substitution arc's surface/underlying arguments were built swapped
(`AddArc(state, before[i], after[i], ...)` instead of `AddArc(state, after[i], before[i], ...)` —
`InversePhonology` arcs run surface→underlying, so this had every arc consuming the WRONG side); (2)
the interpreter's own closure dedup key (`(state, count)`) was too coarse and silently merged two
genuinely different readings (e.g. an identity pass-through and a restored substitution) that land on
the same state with the same segment count — fixed by keying on `(state, rendered-underlying-string)`
instead; (3) and (4) the string-resegmentation ambiguity and the before/after filter mismatch detailed
above. Full suite: **126/126 green** (was 121; +5), CSharpier clean, build clean (0 warnings).

**Gate assessment**: point 3's toy-grammar gate is fully met (all 5 shapes pass, quantified spans
confirmed Exact-tier as required). Point 4's Indonesian tier gate is **met**: all 3 in-scope
(substitution-shaped) rules reach ≥ Permissive, exactly as the spec anticipated; the 2 IdentitySkips
are both deletion, correctly deferred to I3.

Original spec (for what was executed against):

1. New `EnvNfaCompiler` (or private to the new compiler class): recursively map a
   `Pattern<Word, int>` to an NFA fragment of identity pass-through arcs inside the rule's
   transducer. Node handling — this is the COMPLETE inventory, handle all four:
   `Constraint` → one identity arc labeled with its FeatureStruct; `Quantifier` (0/1, 0/∞, 1/∞,
   bounded n..m) → optional edges / self-loops / unrolled repeats; `Group` → sequence;
   `Alternation` → branch-and-rejoin. **Quantified env spans are what make long-distance harmony
   Exact-tier** (an "any consonants*" span is just a self-loop) — do not relegate quantifiers to
   Permissive; they are cheap here. Check how word-edge anchors appear in env patterns
   (`HCFeatureSystem` anchor annotations) and gate on word start/end if expressible; if awkward,
   drop anchor gating → Permissive with reason "anchor".
2. New compiler (new file, e.g. `RuleInverseCompiler.cs`; leave v1 `PhonologyRuleCompiler`
   untouched until I7 retirement): for each `RewriteRule` subrule, build the inverse transducer:
   identity self-loops at state 0 for every alphabet segment AND boundary character (boundaries
   matter from I4 on); one branch per concrete effect: enumerate alphabet segments unifying with
   the Lhs constraint(s), determine each one's output **by probing the rule's own compiled
   synthesis rule in isolation** (reuse v1's proven probe trick per concrete segment — do NOT
   reimplement HC feature arithmetic), and add `[left-env fragment] out:in [right-env fragment]`
   branches. Multi-segment Lhs = a chain of out_i:in_i arcs (probe the whole window). α-variables
   in target or env: enumerate concrete alphabet bindings via unification (bounded by alphabet) —
   the env↔target agreement (Indonesian-nasal-assimilation-style) falls out of enumerating
   consistent concrete combos. MPR/syntactic-feature-gated subrules: compile ungated → Permissive
   ("mpr-gate dropped").
3. Tests (new `RuleInverseCompilerTests.cs`), at the TRANSDUCER level before any walker exists:
   feed symbol sequences through the automaton by hand (a tiny test-local interpreter is fine),
   assert accepted surface→underlying mappings and rejected ones, for: plain substitution,
   left+right env, quantified env span, α-variable agreement, a 2-segment Lhs.
4. Gates: full suite green; tier report shows Indonesian's 5 rules ≥ Permissive (expected: 3–4
   Exact once boundaries land in I4; before I4 the boundary-env rules will be Permissive — note it,
   don't fight it yet).

### I2 — the chain walker — ✅ DONE (2026-07-03)

> **Executed 2026-07-03** (Sonnet implementation, Fable review). All four points landed; suite
> **129/129** (was 126; +3 `ChainWalkerTests`). `AnalyzeComposed` is a one-line delegate to the new
> `AnalyzeChain` (ONE walker — every pre-existing lockstep test guards the shared code);
> `PConfig.RuleStates`/`PConfigKey` generalized to a content-hashed state vector; `CascadeSymbol` is the
> single shared cascade (main per-segment step AND the closure's restoration branch); `ChainClosure`
> handles lexicon ε-arcs + per-rank structural-ε (pure state move) + per-rank ε-input restorations that
> cascade down — the I0 ε-shapes now have their real walk-time semantics, ahead of any compiler emitting
> them (I3). Frontier sizes on the toy tests collapse to single digits after the first consumed segment,
> confirming the "rules sit in identity state almost everywhere" premise. Chain indexing: array index 0 =
> surface-facing (inverse of the LAST-applied rule); the plan prose's "level 0" is array index
> `chain.Count-1` — documented on `AnalyzeChain` itself.
>
> **HONESTY CORRECTION to this phase's own premise, found by executing point 3's baseline assertions
> properly:** the spec (and the "What it fixes that nothing else can" section above) claimed the current
> composite cannot reach these three shapes. **Empirically false at toy scale: today's full composite
> covers ALL THREE toy words**, via two INDEPENDENT mechanisms each — (1) `ComposedPhonologyProposer`
> un-applies the real analysis cascade at runtime (feeding chains don't defeat it — the cascade composes
> in reverse; quantified-span harmony doesn't either — HC's analysis rule is span-agnostic; only
> boundary-conditioning would); and (2) the surface-precompiled FST itself: the two bare-root cases ride
> `BareRootSurfaces`' full-cascade synthesis (any phonology confined to a bare root is laid into the trie
> at build time, feeding included), and the harmony case rides `SurfacePhonology.Variants`' single-left-
> neighbor probe (representative `u` = a ZERO-consonant harmony span fires the rule; the variant is wired
> unconditionally wherever the affix occurs, so the 3-consonant span matches too, verify pruning wrong
> spans). Only v1's `LockstepPhonologyProposer` misses all three (2-segment Lhs, multi-rule composition,
> and quantified envs are each outside v1's compiler) — **that is the true delta I2 closes: the chain
> reaches these shapes in LOCKSTEP with the lexicon**, where the composite's existing coverage rides
> exactly the mechanisms with measured scaling pathologies (runtime un-application — the Amharic
> analysis-oracle blowup's neighborhood; bare-root full-cascade synthesis and variant probing — the
> Amharic 112-s build; permissive unconditional wiring — verify load). So I2 sharpens rather than
> invalidates the phase's value proposition, and it is now stated honestly: not "covers what nothing
> else can" but "covers it at FST-walk cost, lexicon-constrained, retiring the mechanisms that don't
> scale" — which is I7's retirement-by-evidence thesis. The three `ChainWalkerTests` carry LIVE
> assertions of current reality (composite covers: `Is.Not.Empty`; v1 lockstep misses: `Is.Empty`), so
> if either fact ever changes, a test fails rather than a doc rotting. Word-internal shapes that ARE
> boundary-conditioned (where `ComposedPhonologyProposer`'s inverse is blocked and bare-root synthesis
> doesn't apply) remain the chain's unique territory — Amharic's gated deletion rules (I3) are the first
> real-grammar instance in hand.

Original spec (for what was executed against):

1. Generalize `AnalyzeComposed` from one Pinv to a chain — and make the existing single-Pinv path
   DELEGATE to a length-1 chain, so there is ONE walker, not two drifting copies, and every
   existing lockstep test keeps guarding the new code. Config = `(int[] ruleStates, trieConfig)`;
   generalize `PConfigKey` to hash the vector.
2. Per surface segment: cascade the symbol down the chain — at level i, arcs consuming the
   incoming symbol (unification match); each emits one symbol to level i−1 (or nothing, ε-output,
   from I3 on); level 0's emission must unify a trie arc (advance trie, accrue tokens) exactly as
   today. Closure step (generalizing `ComposedClosure`): trie ε-arcs, plus PER-LEVEL ε-input arcs
   — a rule at level i may spontaneously emit a symbol downward (deletion restoration, I3;
   boundary insertion, I4) that cascades through levels i−1…0 to the trie.
3. Toy tests (each: engine parses it, CURRENT composite misses it — assert that baseline first —
   chain covers it, a non-word stays unparsed):
   - **Word-internal rule**: a rule firing inside the root, conditioned ≥3 segments away from any
     morpheme boundary (junction probing provably can't see it).
   - **Two-rule word-internal feeding chain**: rule A's output creates rule B's context,
     mid-root.
   - **The marquee general-case test — long-distance harmony**: a suffix vowel agreeing in some
     feature with the FIRST root vowel across an arbitrary consonant span (quantified env). This
     is the test that certifies "general", not "two languages".
4. Gates: full suite green; both real corpora unchanged (chain not yet wired into the composite —
   these tests construct the chain directly); stats battery on the toy grammars (frontier sizes
   printed, sanity-check the "rules sit in identity state almost everywhere" claim).

### I3 — deletion-inverse and epenthesis-inverse — ✅ DONE (2026-07-03)

> **Executed 2026-07-03** (Sonnet implementation, Fable review; suite **132/132**, was 129). Only
> `RuleInverseCompiler.cs` changed in src — the I2 walker, `InversePhonology`, `EnvNfaCompiler`, v1, and
> all proposers untouched (the walker needed zero changes, as hoped). The compiler is restructured into
> a probe phase (`TryBuildSubruleSpec`) + per-floor emission (`EmitSubrule`).
>
> **The restoration cap is STRUCTURAL, not counted at walk time**: a rule with deletion-shaped subrules
> gets `cap + 1` full copies ("floors") of its automaton; every restoration branch rejoins on the NEXT
> floor up, and the top floor has no deletion branches — closure ε-moves strictly ascend floors, so the
> walker cannot hang even on an unconditioned deletion (now compilable; the trie prunes in lockstep,
> floors bound absolutely). Substitution-only rules get one floor — byte-identical to I1's output.
> Default cap = the engine's own bound, correctly derived: `Morpher.DeletionReapplications + 1` (the
> engine always un-applies once unconditionally, THEN counts reapplications). New
> `Compile(language, morpher, restorationCap)` knob; `cap=0` drops deletion subrules with honest reason
> `"restoration-cap"`. **Honest semantic gap, pinned by test**: one ENGINE round restores several
> independent sites simultaneously; the chain cap counts EVENTS — so the chain's default is narrower
> than the engine on multi-site words (falls to unparsed, never hangs). Epenthesis (∅→ψ) = ε-output
> arcs consuming the epenthesized surface segment (I0's shape, finally emitted by a compiler);
> unconditioned/anchor-only epenthesis reports `"epenthesis-unprobeable"`. Reason-string changes:
> `"empty-lhs"` retired (an empty Lhs is now valid epenthesis); new `"empty-subrule"`,
> `"epenthesis-unprobeable"`, `"restoration-cap"`.
>
> **Real-grammar tier deltas (measured, Fable re-verified Amharic independently):**
> - **Amharic: `Exact=1, Permissive=0, IdentitySkip=6` → `Exact=2, Permissive=4, IdentitySkip=1`** — 5
>   of the 6 deletion-shaped rules now compile (`a deletion before a` at Exact; the 3 MPR-gated
>   e/o-creation rules at Permissive via I1's existing ungated-superset gate handling — the gates were
>   NOT deferred after all; `CV merger inside` Permissive `[alpha-variable]`). The one holdout,
>   `CV merger at morpheme boundaries`, is honest: `[alpha-variable,no-effect]` — its α-variable
>   representative probe observes no effect; likely needs per-binding enumeration (I1's documented
>   residual) and/or I4's boundary tape (it IS the "at morpheme boundaries" rule).
> - **Indonesian: `Exact=1, Permissive=2, IdentitySkip=2` → `Exact=2, Permissive=3, IdentitySkip=0`**
>   (`Nasal deletion` now Exact, `Voiceless obstruent deletion` Permissive). Sena: 0 rules, no-op.
> - 3 new `ChainDeletionEpenthesisTests` (word-internal env-gated deletion; word-internal env-gated
>   epenthesis; unconditioned-deletion cap semantics incl. default-derivation pinning), all with I2's
>   honest empirical baselines: composite covers all three (bare-root synthesis again), v1 lockstep
>   misses all three.
>
> **Real v1 bug found by the honesty assertions** (documented in-test, deliberately NOT fixed — v1 is
> I7's retirement candidate): `LockstepPhonologyProposer.HasNonIdentityArcs` inspects only arcs leaving
> the START state, so any rule whose branches all begin with a LEFT-environment identity arc is
> misjudged as all-identity and the proposer silently disables itself — v1's compiler supports left-env
> deletion, but its own gate never lets it run. (Fable verified at source:
> `LockstepPhonologyProposer.cs` `ArcsFrom(pinv.StartState)` only.) Evidence FOR retirement, not a fix
> target.

Original spec (for what was executed against):

1. Deletion (φ→∅): ε-input restoration arcs bracketed by env fragments, exactly v1's concept but
   through the new compiler; **cap restorations per rule per word** (reuse the engine's own
   deletion-reapplication bound as the default — find it on `Morpher`; make it a knob). An
   unconditioned deletion is now compilable (the trie prunes restorations in lockstep) but respect
   the cap strictly.
2. Epenthesis (∅→ψ): ε-OUTPUT arcs — consume the epenthesized surface segment, emit nothing
   (this is what I0's arc extension exists for). Trivially bounded.
3. Toy tests: word-internal deletion recovered; epenthesis recovered; both with env gating; a
   non-word rejected for each; cap respected (a word demanding more restorations than the cap
   falls to unparsed, not a hang).

### I4 — the boundary tape — ✅ DONE (2026-07-04)

> **Executed 2026-07-04** (Sonnet implementation across several interrupted/resumed runs, Fable review
> incl. the byte-identical gate run personally). Suite **134/134** (+2 toy tests over I3's 132). Only
> `FstTemplateAnalyzer.cs` changed in src.
>
> **What landed:** (1) the trie build stops dropping boundary nodes — `GetSegments` now includes
> Boundary-typed nodes (`IsSegmentOrBoundary`), so a `+` junction inside an affix like `per+`/`+kan`
> becomes a real trie arc. (2) The BARE walker (`EpsilonClosure`, used by `AnalyzeWord`) crosses a
> boundary arc as a FREE ε-move — a real surface word contains no literal junction marker, so this keeps
> every pre-I4 bare-walk analysis reachable byte-identically. (3) The CHAIN walker treats a boundary arc
> as a real symbol: a global "insert boundary" closure move (`ChainClosure` branch (d)) offers each
> boundary-alphabet symbol at rank 0, cascading it through the rules' boundary-identity self-loops down
> to a trie boundary arc — surviving only where the trie actually has one (lexicon-constrains-insertion,
> the I3 argument). Bounded by `PConfig.InsertionsUsed` baked into `PConfigKey` — **hang-proof by
> construction**, same discipline as I3's floors (default cap 8, `maxBoundaryInsertions` knob).
>
> **THE GATE — byte-identical bare-walk on BOTH corpora — PASSED (Fable verified personally via a
> stash-generated true pre-I4 baseline, not the agent's captures):**
> - Indonesian: StateCount 533 → 547 (+14 boundary arcs), analysis signatures byte-identical (121 words).
> - Sena: StateCount 16,347 → 18,871 (+2,524 boundary arcs), byte-identical across all 7,121 words.
> - (Process note worth keeping: the implementing agent's own Sena before-baseline was TRUNCATED at 6,138
>   of 7,121 words — it died mid-capture — which would have looked like drift if trusted. Regenerating a
>   complete true-before via `git stash` of the src change was what made the gate trustworthy. Lesson for
>   future risky-refactor gates: generate the before-baseline yourself by stashing, don't trust a
>   long-running agent's captured artifact.)
>
> **THE MARQUEE CROSS-CHECK — PASSED (Fable re-ran personally):** Indonesian with junction probing
> DISABLED (new `enableJunctionProbing` ctor knob, default true) and the rule-inverse chain ENABLED
> covers **46/46 non-redup meN- words** — every such word (identified from the engine's OWN analyses:
> has `AV`, lacks `Cont`) has its correct reading among the chain's candidates, with zero help from
> Phase C. This is the proof the general mechanism SUBSUMES the special case rather than coexisting
> untested beside it. 10/46 words also carry extra unverified candidates (expected over-generation from
> the 3 Permissive-tier rules; a real `FstReplay`/verify pass prunes them) — reported as a separate
> metric, not conflated with coverage.
>
> **DEVIATION from the spec's point-4 expectation, flagged honestly:** the plan predicted "Indonesian's
> boundary-env rules move Permissive → Exact." They did NOT, and structurally CANNOT from what I4
> shipped: I4 changed only the walker/trie (`FstTemplateAnalyzer.cs`), never the tier classifier
> (`RuleInverseCompiler.cs`). Tier reports are UNCHANGED from I3 (Indonesian `Exact=2, Permissive=3,
> IdentitySkip=0`; Amharic `Exact=2, Permissive=4, IdentitySkip=1` — the "CV merger at morpheme
> boundaries" holdout did NOT move, still `[alpha-variable,no-effect]`). The expectation was based on a
> premise I1 had already resolved (the boundary-representative probe lookup was fixed there), so no rule
> was ever Permissive *because of* boundaries; the remaining Permissive reasons (`alpha-variable`,
> `mpr-or-syntactic-gate`) are unrelated to I4's scope. I4's real contribution is at WALK time (the
> 46/46), invisible to the compile-time tier report — the plan's own metric for this milestone was
> mis-chosen. The Amharic boundary holdout likely needs per-binding α-variable enumeration (I1's
> documented residual), not the boundary tape.
>
> **Second honest correction to a prior claim:** the boundary-conditioned-substitution toy test found
> the FULL composite already covers that shape too (via `ComposedPhonologyProposer`) — contradicting
> I2's own speculative note that boundary-conditioning was "the one shape" that defeats the composite.
> Corrected with a live in-test assertion. The consistent pattern across I2/I3/I4: the composite's
> runtime-un-application + bare-root-synthesis mechanisms cover far more at toy scale than the plan
> assumed; the chain's value is doing it in lockstep at FST-walk cost while retiring those mechanisms'
> measured scaling pathologies (I7's thesis), NOT reaching shapes nothing else can.
>
> New tests: `BoundaryTapeBaselineTests` (the reusable before/after regression guard),
> `BoundaryTapeMarqueeCrossCheckTests`, `BoundaryTapeChainTests` (2 toy tests). Next: I5 (metathesis +
> application-semantics honesty).

Original spec (for what was executed against):

1. Trie build: stop dropping boundary nodes from root/affix chains — build boundary arcs. The
   BARE walk must treat boundary-labeled arcs as free (ε) moves so its behavior is byte-identical;
   the chain walk treats them as real symbols. Expect `StateCount` to GROW (each `+` in an affix
   like `meⁿ+` becomes an arc+state) — measure and record the delta; the H-era state-count lesson
   applies: any coverage/soundness drift is a bug, a state-count change alone is not.
2. Chain walk: a global "insert boundary" ε-move — emits a boundary symbol at the TOP of the
   chain, which passes through every rule's boundary-identity self-loops (from I1.2) down to a
   trie boundary arc. Only survives where the trie actually has a boundary — the same
   lexicon-constrains-restoration argument as deletions. Cap insertions per word (configurable;
   default generous, e.g. 8).
3. Now boundary-conditioned rules' env fragments (which reference `BoundaryMarker` FeatureStructs)
   gate correctly on intermediate tapes — the v1 `_alphabet`-excludes-boundaries bug is obsolete
   rather than fixed.
4. Gates: bare-walk analyses byte-identical on BOTH corpora (the risky refactor — this gate is the
   whole point); then the marquee cross-check: **Indonesian with junction probing DISABLED and the
   chain ENABLED must independently cover all non-redup meN- words** — proving the general
   mechanism subsumes the special case rather than coexisting untested beside it. Tier report:
   Indonesian's boundary-env rules move Permissive → Exact.

### I5 — metathesis + application-semantics honesty — ✅ DONE (2026-07-07, via redo)

> **REDO EXECUTED 2026-07-07** (Sonnet redo per the review verdict below, Fable re-verified all
> gates). Suite **137/137** (134 + 3 new); both tier reports restored and Fable-confirmed:
> Indonesian `Exact=2, Permissive=3, IdentitySkip=0`, Amharic `Exact=2, Permissive=4, IdentitySkip=1`
> — the partial's Amharic regression is gone, no spurious reasons anywhere.
>
> - **Self-feeding detection: DROPPED (option c), now a documented residual** on `Compile`. The
>   redo first researched HC's variable-sharing convention and found option (b) would be vacuous or
>   wrong (shared variables in this codebase always pair Rhs↔environment — assimilation — never
>   Lhs↔Rhs), and option (a) requires per-position reachability analysis the black-box prober has no
>   machinery for. Honest residual: a truly self-feeding `Iterative` rule may under-cover via the
>   chain with no reason string naming it — under-coverage only (falls to engine/unparsed), never
>   unsoundness (verify remains the backstop).
> - **Metathesis-inverse landed with a compile-time combo cap** (`MaxMetathesisCombos = 256`,
>   pool-product checked BEFORE any probing — no partial enumeration, which would silently prefer
>   whichever combos enumerate first). Exceeding it → IdentitySkip `"metathesis-too-many-combos"`
>   (IdentitySkip not Permissive: with zero probed candidates there is nothing partial to offer;
>   matches the `restoration-cap` convention). First-ever execution of the metathesis path: toy
>   four-part chain test (`s↔k` word-internal swap recovered at Exact tier; composite already covers
>   it via bare-root synthesis — the recurring I2/I3/I4 pattern, asserted live; v1 misses it
>   entirely), a broad-switch-class cap test (24×24=576 combos > cap, bounded, honest downgrade),
>   and an RTL `"direction"` flag test.
> - **Bonus real bug found by advisor review during the redo:** every early-exit `IdentitySkip`
>   return in `CompileMetathesisRule` (including five pre-existing ones from the partial, like
>   `metathesis-group-not-found`) built a bare reject-all automaton — violating IdentitySkip's own
>   documented "identity-only Pinv" contract (a rule that can't compile must be transparent, not a
>   wall). Fixed by seeding identity (accepting state + alphabet self-loops) at method entry before
>   any early return, with a test that actually walks a capped rule's Pinv to prove pass-through.
> - Disclosed residual: the framing-group (`MetathesisSlot.Fixed`) path reuses tested machinery but
>   has no dedicated toy test — flagged rather than silently assumed proven.
>
> Original review verdict (for the record of what the redo executed against):

⚠️ superseded-by-redo: PARTIAL, REDO VERDICT (2026-07-07)

> **STATUS (2026-07-07 review):** a Sonnet implementation pass (interrupted by session limits before
> writing any tests) left ~+350 uncommitted lines in `RuleInverseCompiler.cs`. A full single-reviewer
> audit issued a **REDO (not revert)** verdict:
>
> - **KEEP:** the metathesis-inverse skeleton (black-box probe + positional pairing — architecturally
>   sound on code reading), the RTL `Direction` → `"direction"` Permissive flag, and the
>   `CompiledRuleInverse.Rule` widening from `RewriteRule` to the rule base type.
> - **BLOCKER — `DetectsSelfFeeding` is worse than absent and is the reason the tree is RED
>   (130/134; 4 failures all citing its new `iterative-self-feeding` reason):** it tests whether the
>   rule's Rhs unifies with a segment its own ENVIRONMENT requires — but environment is context that
>   must PRE-EXIST; the rule doesn't produce it. Real self-feeding means the OUTPUT creates a NEW
>   trigger at an ADJACENT position, which the check never examines. Since assimilation's defining
>   shape is "output resembles a neighbor," this trips on essentially every environment-conditioned
>   substitution rule. Worse, the `ApplicationMode == Iterative` guard filters nothing:
>   `XmlLanguageLoader` defaults every loaded rule to `Iterative` (and HC's enum default matches), so
>   real-grammar rules and toy rules alike trip it. Measured damage: **Amharic's
>   `remove consonant length from lexical forms` silently downgraded Exact→Permissive**
>   (tier report `2/4/1` → `1/5/1`) and 2 Indonesian rules gained a spurious reason (masked only
>   because they were already Permissive). **Root cause is partly this spec's own point-2 wording**
>   ("output unifies the rule's own env/target") — implemented literally, and literally wrong. The
>   REDO must fix the CRITERION, not just the code: either reason about output-creates-new-adjacent-
>   trigger properly, or demote to "flag only when Lhs and Rhs share an actual variable binding," or
>   drop to a documented residual until a real self-feeding grammar motivates it (mirrors I1's
>   α-variable-enumeration deferral). A grammar author must be able to trust the tier report — a
>   diagnostic that lies is worse than no diagnostic.
> - **MAJOR — uncapped alphabet² in `CompileMetathesisRule`:** the probe enumerates the FULL
>   |pool1|×|pool2| Cartesian product of alphabet combinations when both switch groups are broad
>   natural classes, each combo a real cascade `Apply` — the EXACT anti-pattern this plan already
>   diagnosed as Amharic's `DeletionJunctions` 417²-build-time killer. No grammar in hand has a
>   `MetathesisRule` (grep-verified: none in Indonesian/Sena/Amharic XML), so the path has NEVER
>   EXECUTED — the redo needs a compile-time combo cap with an honest reason string (e.g.
>   `"metathesis-too-many-combos"`) AND a first-ever real execution via toy tests, including a
>   broad-class switch case (existing `MetathesisRuleTests` shapes are all single-concrete-segment,
>   pool size 1, which cannot exercise the defect).
> - Remaining for the redo: fix/drop `DetectsSelfFeeding`; combo cap; the spec's toy tests
>   (metathesis positive/negative, self-feeding miss demo or flag assertion, RTL flag); CSharpier
>   (one violation); re-run both tier reports and confirm Amharic back to `2/4/1`.

Original spec (for what is being executed against — NOTE point 2's detection criterion is
superseded by the review finding above):

1. `MetathesisRule` inverse: bounded window swap — a hold-one-symbol transducer (state remembers
   the held concrete segment; ~alphabet-sized state count, fine).
2. **Self-feeding iterative rules** (`ApplicationMode == Iterative` where the output can create a
   new context for the same rule): one transducer pass models one simultaneous sweep, which
   under-covers self-feeding. Detect the shape (output unifies the rule's own env/target); flag it
   in the tier report ("iterative-self-feeding: may under-cover"); OPTIONALLY chain the rule's
   inverse twice consecutively when detected — implement only if a toy test demonstrates a real
   miss, otherwise the flag + engine fallback is the honest v-next residual.
3. RTL `Direction`: the chain walks LTR regardless; for most rules this is absorbed by the
   superset principle; flag RTL rules Permissive ("direction").

### I6 — the beam cap — ✅ DONE (2026-07-07)

> **Executed 2026-07-07** (Sonnet implementation, Fable review with independent gate verification).
> Suite **139/139** (+2 `BeamCapTests`).
>
> **Design — ONE per-word budget closes BOTH axes:** a `BeamBudget` (single decrementing counter,
> fresh instance per `AnalyzeShape`/`AnalyzeChain` call, `maxBeamWork = 10_000` default, ctor knob on
> all four constructors) is debited at (a) every post-dedup frontier admission across the bare walk,
> `EpsilonClosure`, the chain walk, and all four `ChainClosure` branches — the original frontier
> axis — AND (b) once per matching arc inside `CascadeSymbol` BEFORE recursing — the review-mandated
> within-symbol enumeration axis that a frontier-only check cannot see. `Overflowed` latches
> permanently, so active recursion unwinds in O(chain depth) regardless of automaton shape — wall
> time is bounded by the budget, not the grammar. Overflow → the word returns EMPTY (unparsed) and is
> COUNTED (`BeamOverflowCount`, Interlocked for the concurrent pool usage; `LastBeamOverflowWord`
> breadcrumb; surfaced through `ProbeReport.BeamOverflows` as a per-probe delta). Never throws, never
> hangs, never silently drops.
>
> **Gates (Fable re-verified):** pathological test (hand-built 12-rank × 8-branch wildcard chain
> engineered so overflow can ONLY come from the enumeration axis — the deduped frontier stays
> trivial) falls to unparsed in ~22 ms with the counter incremented, while a healthy walk over the
> same lexicon still parses; knob test at `maxBeamWork: 5`; Indonesian spot-check **zero overflows**
> at the default cap and bare-walk output **byte-identical to the I4 baseline** (StateCount 547
> unchanged); full-composite probe also 0 overflows at the known 103/121 bare-composite coverage.
>
> Honest gaps flagged: `LockstepPhonologyProposer`'s internal analyzer's overflow count is invisible
> to `ProbeReport` (documented in-code; moot once I7 replaces it); the bare walker shares the
> mechanism but has no dedicated explosion test (non-recursive design; covered indirectly). This
> closes the oldest open KNOWN_GAPS item (frontier-beam cap, deferred since the original plan).
> Next: I7.
>
> **CORRECTION (2026-07-07, found by I7a's measurement battery): I6's default was MIS-CALIBRATED and
> its gate was INSUFFICIENT.** The 10,000 default budget silently truncates **58 of 60** Sena
> guarded-slice words to unparsed (slice coverage 13/60 vs the recorded 57/57-of-60) — I4's +2,524
> boundary ε-arcs made ordinary Sena walks exceed 10k work units, and I6's "zero behavior change"
> gate only spot-checked Indonesian (547 states), never Sena (18,871 states). The overflow counter
> worked exactly as designed (58 overflows recorded — this is what made the regression DIAGNOSABLE
> instead of silent), but the default clipped healthy walks, violating I6's own "must not clip
> healthy walks" requirement. **Fixed in the follow-up commit after I7a via a measured THREE-POINT
> calibration** (each point empirical, on the slice): 10,000 → clips 58/60 healthy words (too low);
> 10,000,000 → covers 60/60 but allocates ~1.9 GB/word on the pathological tail and CRASHED a test
> host during verification (effectively-unbounded is unsafe in the other direction — the first
> candidate fix was itself refuted by measurement); **1,000,000 → 58/60 covered, 0 unsound, exactly
> 2 overflows, and both are the pathological-tail words the cap EXISTS to stop** (the pre-I6
> recorded gate itself excluded 3 such words via a 5 s timeout, so 58-covered + 2-gracefully-stopped
> meets or beats the recorded 57/57-of-60). Default set to 1,000,000; Indonesian re-verified
> byte-identical with 0 overflows at the new default; per-grammar percentile calibration delegated
> to the complexity-cap plan. REVIEW LESSONS: a "zero behavior change" gate must run on the LARGEST
> grammar in hand, not the fastest one; and a fix to a measurement-discovered bug needs its own
> measurement (the obvious "raise it high" fix failed differently).

Original spec + review sharpening (for what was executed against):

> **SCOPE SHARPENED by the 2026-07-07 full-plan review:** a post-closure frontier-size check
> (`current.Count`) is NOT sufficient. `CascadeSymbol`'s recursive rank-by-rank fan-out can enumerate
> exponentially many candidate paths WITHIN a single input symbol, before any of them reach the dedup
> `HashSet` — I6 must budget total `CascadeSymbol` enumerations per symbol/word, not only frontier
> size after dedup. Separately, the metathesis compile-time Cartesian blowup (I5's MAJOR finding) is
> a COMPILE-time axis that I6's walk-time cap does not cover — it gets its own cap in the I5 redo,
> and both should eventually fold into the complexity-cap plan's budget framework rather than
> accumulating parallel one-off mechanisms.

1. Max live configurations per word across `AnalyzeShape`/`AnalyzeComposed`/the chain (one shared
   implementation — they're one walker after I2). Default generous (e.g. 10,000), ctor knob.
   Overflow → stop that word, count it, surface via `ProbeReport.BeamOverflows` and an analyzer
   property. Never throw, never hang.
2. Toy pathological test: a grammar+word engineered to explode the frontier (many Permissive-tier
   rules × ambiguous unification paths); assert graceful "unparsed", bounded wall-time.
3. This is also the Identity-skip escape hatch's trigger: if a real grammar's rule blows the beam,
   the tier report + per-rule skip knob is the response, recorded in diagnostics.

### I7 — wiring, measurement, retirement by evidence — ✅ DONE (2026-07-07; measured outcome: chain ships OPT-IN, nothing retired)

> **I7a EXECUTED 2026-07-07** (Sonnet implementation, Fable review). Suite **144/144** (+5). The
> plan's own fallback rule fired, and that IS the completion of this milestone — retirement was
> always "by evidence, either outcome fine, but measured":
>
> - **`ChainPhonologyProposer` built and wired** (new file): compiles the rule-inverse chain once per
>   language (flat `.Reverse()` of `RuleInverseCompiler.Compile`'s forward order = reverse
>   application order, proof in class remarks), drops IdentitySkip rules, walks via `AnalyzeChain`
>   over the underlying-only analyzer. Does NOT inherit v1's `HasNonIdentityArcs` start-state-only
>   bug (worth-walking = any rule at tier ≥ Permissive). Inert at zero overhead on no-phonology
>   grammars (Sena — verified by test).
> - **THE MEASURED DECISION: chain-on HOLDS coverage everywhere but blows the p50 budget ~37×, so
>   the chain ships OPT-IN** (`useChainPhonology`, default false, on `CompositeProposer.ForLanguage`
>   + `FstCoverageProbe.ForLanguage`; decision recorded in the knob's doc comment and PINNED by a
>   reflection test so a future flip must be deliberate). Battery (Indonesian, full 121 words):
>   chain-on **121/121 fully covered, 0 unsound** — identical to chain-off — but verified-walk p50
>   58.6 ms vs 1.6 ms (37.4×; the chain cascades every segment through 5 rule inverses where v1
>   walks one merged automaton; +43% allocations/word is real but secondary to walk+verify work).
>   Sena slice: chain inert, p50 ratio 0.95–1.16×, within budget. Per point 2's own rule: "if
>   exceeded, ship the chain OPT-IN and record the decision — do not silently eat a regression, do
>   not silently drop the chain." Done exactly.
> - **RETIREMENT DECISIONS (point 3), all measured, none executed:** `LockstepPhonologyProposer`
>   STAYS (it wins p50 37× where both hold coverage); `ComposedPhonologyProposer` /
>   `ForwardSynthesisProposer` / v1 compiler internals STAY (their replacement depends on the chain
>   being the default path, which the measurement rejected); junction probing STAYS (its coverage
>   half was answered YES by I4's 46/46, but retiring it requires chain-as-default, which lost on
>   p50). The chain remains the correctness/generality instrument: it exists, is tested (144-test
>   suite incl. its unique word-internal/feeding/harmony/deletion/epenthesis/metathesis lockstep
>   coverage), wired opt-in for any grammar where v1's limits bite, and is the foundation I8+/future
>   perf work optimizes INTO the default slot rather than re-derives.
> - **Preconditions discharged:** (2) allocation profiling done — Indonesian chain-on 6.44 MB/word
>   vs 4.51 (+43%); Sena full-walk ~1.9 GB/word BOTH configs (pre-existing product-walk+verify
>   pressure, confirming the review NOTE; a dedicated pass belongs before any future default flip).
>   (3) standing tier gate landed (`TierGate_OnRealGrammar_MatchesRecordedCounts`, asserts
>   Indonesian 2/3/0 + Amharic 2/4/1) — both PASS.
> - **Bonus discovery: I7a's battery caught I6's mis-calibrated default** (Sena slice clipped to
>   13/60 by the 10k budget — see the I6 CORRECTION block; fixed in the follow-up commit; the
>   battery gained an `HC_MAX_BEAM_WORK` knob during diagnosis).
> - Point 4 (docs sweep) executed with the closing commits: both plans' STATUS blocks updated;
>   FST_FAST_PATH_PLAN KNOWN_GAPS items (boundary gap, v1 scope, beam cap, §3b) closed with
>   pointers to their I-milestone resolutions.

> **PRECONDITIONS added by the 2026-07-07 full-plan review:** (1) the I5 redo's `DetectsSelfFeeding`
> fix must land BEFORE I7 measures the p50 budget — otherwise the numbers measure a known-bad
> heuristic's inflated Permissive/verify traffic, not the chain's true cost. (2) Profile
> `PConfigKey`/`TokenArrayKey` hash recomputation and `CascadeSymbol`'s per-arc `int[]` clones once on
> Sena-scale input before fixing the p50 gate (allocation pressure is real at 16k+ states × 7,121
> words). (3) Adopt a standing per-commit gate: diff `TierReport_OnRealGrammar` (Indonesian + Amharic)
> against the plan doc's last recorded numbers — the I5 partial's silent Amharic Exact 2→1 drift is
> exactly what this catches automatically. (4) Coverage half of the junction-probing-retirement
> question was already answered YES by I4's 46/46 marquee; I7 only owes the performance half.
> (5) If compounding is ever lifted beyond 2 roots via a genuine trie loop, the ε-cycle token-accrual
> hang vector (review NOTE) must be defended at the same time — the DAG guarantee is currently
> by-construction, not asserted.

1. `ChainPhonologyProposer` replaces `LockstepPhonologyProposer` in `CompositeProposer.ForLanguage`
   and `FstCoverageProbe.ForLanguage`; chain built once per language.
2. Full battery, stats-battery reported for EVERY row (states incl. boundary delta, build ms
   cold+warm, walk p50/p95 chain-on vs chain-off, coverage, unsound): both corpora must hold
   121/121 and the Sena guarded slice, 0 unsound. Walk p50 regression budget with chain on:
   ≤ ~1.5×; if exceeded, ship the chain OPT-IN (composite keeps junction probing as default fast
   path) and record the decision — do not silently eat a regression, do not silently drop the
   chain.
3. Retirement strictly by measurement, one commit each: `ComposedPhonologyProposer` and
   `ForwardSynthesisProposer` (+ its flag threading) go if the chain matches or beats them
   everywhere they fire; v1 `PhonologyRuleCompiler`'s probing internals go once nothing consumes
   them (keep the `InversePhonology` type — it's the chain's substrate); `LeverTwoSpikeTests`'
   hand-built transducers become tests OF the new compiler (assert the compiler now GENERATES what
   the spike hand-built) rather than being deleted. Junction probing (`DeletionJunctions` + skip
   arcs) is retired ONLY if chain-on/probing-off matches coverage without blowing the p50 budget;
   otherwise both stay (probing = precision fast path, chain = completeness backstop) — either
   outcome is fine, but it must be a measured decision.
4. Docs: sweep `FST_FAST_PATH_PLAN.md` KNOWN_GAPS (boundary gap, v1 scope, beam cap, §3b — all
   close), update both plans' STATUS blocks.

### I8 (optional backlog, small, independent) — the last two uncovered MorphOps

After I7 the fast path covers every REGULAR HC construct (all rewrite phonology incl. harmony and
feeding, metathesis, morphotactics, compounding ≤2 roots) plus peels for non-regular copying. The
only remaining `UncoveredOps` are `MorphOp.Clitic` (clitic strata compile like affix layers — a
trie-build extension, likely small) and `MorphOp.Process`/`ModifyFromInput` (simulfix = a
feature-change over stem segments — expressible as substitution-variant arcs over root chains, or
left as engine fallback). Neither is needed by any grammar in hand; spec them properly when one is.

### What "general" still does NOT mean (honest boundary)

Unbounded copying (full/partial reduplication) is provably non-regular and stays a peel — that is
not a limitation of this design but of finite-state mathematics; every FST toolkit ever shipped
has the same carve-out (xfst's compile-replace is a two-pass trick, not a counterexample).
Compounding stays bounded at 2 roots (the loop's bound; lift to `MaxStemCount` if a grammar needs
3). Self-feeding iterative rules may under-cover until I5's optional doubling is implemented —
flagged, never silent. And the whole edifice keeps the propose-and-verify contract: the chain
proposes, `FstReplay` confirms, so even a compiler bug costs coverage, never a wrong answer.

Original short design notes (superseded by the spec above, kept for continuity):
1. Compile each `RewriteRule` subrule to its own small INVERSE transducer over the concrete
   segment alphabet (states = position in the λ·φ·ρ window ⇒ ~5–10 states/rule; textbook
   construction; replaces `PhonologyRuleCompiler`'s probing v1).
2. Generalize `AnalyzeComposed` from ONE `InversePhonology` to a CHAIN — plan §3b of
   `FST_FAST_PATH_PLAN.md`, which this section supersedes in detail.
3. Keep boundary nodes as trie arcs (ε on surface, matchable by rule transducers on the
   intermediate tapes).
4. Add the frontier **beam cap** (the standing Phase-F/KNOWN_GAPS item) as part of this work —
   overflow ⇒ word counted unparsed, never wrong, never a hang.
5. Gates: all existing toy tests + both real corpora unchanged; new toy tests for a word-internal
   rule and a two-rule feeding chain that junction probing provably cannot cover (assert the bare
   composite misses them TODAY, then that the chain covers them).

## Risks / honesty

- **Set parity may surface analyses nobody expected** (compounds, doubled derivations) — Phase A
  exists to find that before any design commitment.
- **Junction windows deeper than probed** on some future grammar — the build-time window
  assertion turns that into a visible "unsupported", never a silent miss.
- G2's walk-cost note is real: the compound loop multiplies root-entry fan-in; the stats battery
  after G2 decides whether PoS-gating the re-entry is needed.
- This makes the two REAL grammars fully covered; it does NOT claim 100% for arbitrary HC grammars
  until Phase I exists (word-internal cascades remain the open frontier, unchanged).

## Rough effort

| Phase | Size |
|---|---|
| A (measure) | ✅ done |
| B (Sena morphotactics) | superseded by G2 |
| C (junction probing) | ✅ done |
| D (redup peel) | ✅ done (6/7; 7th → G1) |
| E (compounding data-model lift) | **cancelled — premise falsified, see G2** |
| F (hardening/gates) | beam cap folded into Phase I |
| H (build-time regression) | ✅ done (H1+H2; H3 struck — not a real bug) |
| G1 (suffix-peel in separator scan) | ✅ done (Indonesian now 121/121) |
| G2 (compound loop + FstReplay fix) | ✅ done (`ndikhali` 8/8 exact parity; also needed `DerivableToCategory` extension the spec missed) |
| I (lazy per-rule chain — the true FST) | I0 ✅ done, I1 ✅ done (2026-07-03); I2–I7 + optional I8 remain |
