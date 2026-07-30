# Kaplan-Kay replace cascade vs. shipped enumeration: an implementation-and-measurement experiment

> **Where the code lives.** This report is on `main`; the test driver it cites
> (`rust/crates/pg-foma/tests/cascade_vs_enumeration_experiment.rs`) is **not**. That file stays on
> the `fst-builder-improvements` branch (commit `53c5a32`) because its
> `templatic_interdigitation_case` is a deliberately **failing** test: the recall regression *is* the
> experiment's result, and a red test does not belong on `main`. Check that branch out to reproduce.
> The bare-root change discussed in Step 0 below *did* land, separately, as `0ec6007`.

Branch: `fst-builder-improvements`. Baseline reference read first (per the task brief) and not
re-derived here: `docs/fst-plan/conformance-fst-measurement.md` on `conformance-fst-measure`
(commit `3566b77`) — its central finding (`pg-foma` ships two non-interoperating FST-construction
pipelines, and the capability-characterization layer grades the one that does **not** ship) is the
premise this experiment tests empirically rather than by further reading.

## Step 0: resolving the unverified WIP (`0356f72`)

Before any measurement, the branch's tip carried an unverified bare-root compile-time discharge
change (`emit.rs`/`capability.rs`/`conformance_coverage.rs`/`coverage_ledger.rs` + a new test). Real
verification was run (not merely claimed):

1. **Fails with the fix reverted.** `git checkout <parent> -- rust/crates/pg-foma/src/emit.rs`, then
   `pg.ps1 -Mode test -Package pg-foma -ExtraArgs --status-level,fail,--no-fail-fast,-E,binary_id(~bare)`:
   ```
   FAIL [   0.021s] (2/2) pg-foma::bare_root_compile_time_discharge bound_single_allomorph_root_has_no_bare_accept_arc
   ...
   bound single-allomorph root 'bnd' must NOT get a bare ("#"-continuation) accept arc -- ...
   found in Root lexicon:
   %<R%:1%>:bnd # ;
   %<R%:2%>:fre # ;
   TLPfx0 ;
   Summary [ 0.022s] 2 tests run: 1 passed, 1 failed, 0 skipped
   ```
2. **Passes with the fix restored** (`git checkout 0356f72 -- rust/crates/pg-foma/src/emit.rs`):
   `Summary [ 0.033s] 2 tests run: 2 passed, 0 skipped`.
3. **Full `pg-foma` suite**: `Summary [ 23.122s] 618 tests run: 616 passed, 2 failed, 60 skipped` —
   the 2 failures (`plan_diagram::tests::plan_diagram_golden_mermaid`,
   `readiness_verdict::tests::readiness_verdict_golden_json`) are pre-existing `\n`-vs-`\r\n`
   golden-text drift from this Windows checkout's `core.autocrlf=true` (confirmed directly via
   `git config --get core.autocrlf` → `true`), unrelated to `emit.rs`/`capability.rs`, and reproduce
   identically with `emit.rs` reverted too.
4. **Conformance fixture suite** (`pg-parse`'s `conformance_fixtures_gate.rs`, `machine/conformance`
   + `conformance-staging`): `Summary [ 0.946s] 146 tests run: 146 passed, 43 skipped`.

**Verdict: (a) it verifies.** Kept as-is (commit `a29887a`); this experiment runs on top of it.

## What it took to drive the cascade as an analysis engine

**Nothing.** `pg_foma::templated_compile::compile_templated_morphotactics(&Grammar) -> Result<TemplatedCompileOutput, _>`
already exists, is `pub`, and already returns an ordinary `pg_foma::analyzer::FomaProposer`
(`TemplatedCompileOutput::proposer`, built internally via `FomaProposer::from_precompiled_network` +
`.with_segment_query_encoder`) — the exact same type `FomaProposer::new` (the shipped enumeration
path) returns. Both proposer values plug into the same
`FomaAnalyzer::from_precompiled_proposer(g, proposer)` propose→peel→confirm pipeline
(`composite.rs`), already public. `rust/crates/pg-foma/tests/cascade_vs_enumeration_experiment.rs`
(committed on this branch, `53c5a32`) is a ~280-line driver over these two already-public entry
points — no new construction code, no change to either engine.

This function is already the real body of `recipe_runtime.rs`'s `EmissionStrategy::
TemplatedUnderlyingTokens` (reachable today only through `pangloss recipe-optimize`'s
`token-cascade-morphology` recipe, `recipe_registry.rs:712`), so this driver exercises the identical
code the recipe system already calls — not a new, parallel cascade path built for this experiment.

**Constraint discovered, not built around**: `compile_templated_morphotactics` requires at least one
phonological rule (`rules_in_order` empty ⇒ `TemplatedCompileError::NoCompiledRules`) — a grammar
with zero `<PhonologicalRule>`/`<MetathesisRule>` elements cannot be compiled through this path at
all today. All three grammars below declare at least one, so this did not block the experiment, but
it is a real gap for a purely-affixational grammar with no phonology whatsoever.

## Corpus availability (checked directly, not assumed)

`ls samples/data` in this worktree → `No such file or directory`; `.gitignore` confirms
`/samples/data/*.json` (where `aweti.json` would live) is gitignored. **The real Aweti grammar (855
roots × 123 rules, the actual motivating blowup case) is absent from this worktree and was not
measured.** No self-contained synthetic Aweti-shaped grammar generator exists in `pg-foma`'s own
test suite either — every Aweti-scale test found (`analyzer.rs`'s
`aweti_trips_enumeration_budget_fast_with_typed_error` etc.) loads the same gitignored file and
silently skips (`eprintln!("skipping: ...")`) when it's absent, which is exactly what happened when
these were run as part of the full suite above. **This experiment could not reproduce or contradict
the 2.83M-entry/691MB Aweti number at its own scale** — that is reported as a gap, not estimated.

Three grammars were chosen from the public `machine/conformance` corpus instead, per the task's own
"prefer a small number of discriminating grammars" instruction:

| Case | Fixture | Why |
|---|---|---|
| Templatic/interdigitating | `languages/templatic-root-modification` | The only public fixture with `InsertSimpleContext`/`ModifyFromInput` ("process" morphs) — the same construct family §4 of the baseline doc ties to the Aweti blowup mechanism (`build_structural_composites`, `O(roots × rules^depth)`). 9 lexical entries, 5 phonological rules, 25 words. |
| Heavy rewrite cascade | `edge-cases/right-to-left-anchor-environment` | `simultaneous-epenthesis-cascade` was tried first and rejected: its own `words.yaml` is `expect_crash` (pathology-pinning, zero comparable words). This fixture instead exercises `Dir::RightToLeft` with a bare word-final `Anchor` environment — `capability.rs`'s own `RightToLeftRewriteFaithfulReversalPredicate` is characterized against `replace.rs` specifically; the baseline doc left the mainline engine's actual RTL behavior an open question. 2 entries, 1 rule, 6 words including 4 negative controls. |
| Plain concatenative control | `edge-cases/mpr-gated-exception` | Ordinary affixation + an MPR-gated phonological subrule, the same fixture the baseline doc's own Q4 build used. 4 entries, 3 mrules, 3 prules, 9 words. |

## Results

Real, executed output (`pg.ps1 -Mode test -Package pg-foma -ExtraArgs --no-capture,--status-level,all,--no-fail-fast,-E,binary_id(~cascade_vs_enumeration)`):

### RTL anchor (heavy rewrite cascade)

```
grammar scale: entries=2 mrules=0 prules=1 templates=0 strata=1
[enumeration/default]  build: ok in 3 ms   states=Some(4) arcs=Some(5)
[cascade/Kaplan-Kay]   build: ok in 9 ms (lexc_compile=109.4µs rule_compile_compose=8.8807ms final_compose_minimize=57.9µs skipped_rules=[] phonological_rule_count=1)  states=Some(4) arcs=Some(5)
[enumeration/default]  words_checked=6 total_candidates=4 total_confirmed=2 candidates/word=0.67 recall_mismatches=0
[cascade/Kaplan-Kay]   words_checked=6 total_candidates=4 total_confirmed=2 candidates/word=0.67 recall_mismatches=0
test heavy_rewrite_cascade_case ... ok
```
Both engines: **identical recall** (2/6 words confirm, matching the oracle exactly, including all 4
`expect_fail` negative controls — the mis-anchored/mis-positioned variants genuinely have zero
analyses under both), **identical final network size** (4 states / 5 arcs), **identical candidate
count** (4 total, 0.67/word). The shipped default engine independently gets this RTL construct right
— confirming, at small scale, that the baseline doc's open question ("does the mainline path
independently handle RTL rewrite") resolves in the mainline's favor here, same as it already found
for unbounded quantifiers (its own §9 Q3). Build time: 3ms vs 9ms — the cascade's fixed per-rule
compile overhead (`fsm_parse_regex`+`fsm_compose`, ~9ms) dominates at this scale; not a meaningful
signal either way for a 1-rule, 2-entry grammar.

### Plain concatenative control (`mpr-gated-exception`)

```
grammar scale: entries=4 mrules=3 prules=3 templates=0 strata=1
[enumeration/default]  build: ok in 11 ms   states=Some(29) arcs=Some(60)
[cascade/Kaplan-Kay]   build: ok in 17 ms (... rule_compile_compose=15.9013ms ...)  states=Some(25) arcs=Some(32)
[enumeration/default]  words_checked=9 total_candidates=9 total_confirmed=8 candidates/word=1.00 recall_mismatches=0
[cascade/Kaplan-Kay]   words_checked=9 total_candidates=9 total_confirmed=8 candidates/word=1.00 recall_mismatches=0
test plain_concatenative_control_case ... ok
```
**Identical recall** (8/9, matching the oracle including the MPR-excluded negative control
`vokadan`-shaped word), **identical candidates/word** (1.00 — no over-proposing difference at this
scale), and the cascade's compiled network is **smaller**: 25 states / 32 arcs vs. baseline 29 / 60
(roughly half the arcs). Build time: 11ms vs 17ms, same caveat as above (rule-compile fixed cost
dominates a 3-rule grammar).

### Templatic/interdigitating (`templatic-root-modification`) — the decisive case

```
grammar scale: entries=9 mrules=7 prules=5 templates=0 strata=1
[enumeration/default]  build: ok in 40 ms   states=Some(93) arcs=Some(148)
[cascade/Kaplan-Kay]   build: ok in 22 ms (... skipped_rules=["prEpenthesis", "prSimulFeeding"] phonological_rule_count=5)  states=Some(35) arcs=Some(61)
[enumeration/default]  words_checked=25 total_candidates=29 total_confirmed=17 candidates/word=1.16 recall_mismatches=0
[cascade/Kaplan-Kay]   words_checked=25 total_candidates=19 total_confirmed=11 candidates/word=0.76 recall_mismatches=6
  MISMATCH: cascade word "katabit": confirmed=0 oracle=1
  MISMATCH: cascade word "spr": confirmed=0 oracle=1
  MISMATCH: cascade word "sapr": confirmed=0 oracle=1
  MISMATCH: cascade word "smm": confirmed=0 oracle=1
  MISMATCH: cascade word "qil": confirmed=0 oracle=1
  MISMATCH: cascade word "gigugi": confirmed=0 oracle=1
test templatic_interdigitation_case ... FAILED
```

**The cascade path loses recall: 6 of 25 words (24%) that both the oracle and the shipped
enumeration engine confirm, the cascade confirms zero.** The network is smaller (35/61 vs. 93/148
states/arcs) and builds faster (22ms vs. 40ms) — but per this project's own stated priority order,
**neither matters once recall drops**. Two independent causes, both read directly from source, not
inferred from the symptom alone:

1. **`prEpenthesis`/`prSimulFeeding` are silently skipped by the rule compiler** —
   `compile_and_compose_rules_recall_safe`'s own `skipped_rules` output names them explicitly. `
   katabit` (needs `prEpenthesis`'s C_C-seam "i" insertion) and `gigugi` (needs `prSimulFeeding`'s
   simultaneous HighVowel→BackRnd rewrite) fail for exactly this reason — the templated emitter has
   no pre-baked junction-alternative fallback the way the default engine's `junctions.rs` probe does,
   so a rule the cascade skips is a rule with zero effect anywhere in this pipeline, not a
   graceful degradation.
2. **`mrFormII`/`mrPassive` (`InsertSimpleContext`/`ModifyFromInput`, the actual "process"/ablaut
   morphs) are marked `allomorphs_skipped` by `emit.rs`'s own `has_unemittable_action` gate** —
   verbatim reason string: `"Modify/InsertContext action has no literal text to emit (v1)"`. This
   gate is shared code (`emit.rs`, used by both `TextMode::SurfaceProbed` and `TextMode::
   UnderlyingTokens`), but only the surface-probed default path has a *fallback*
   (`build_structural_composites`, real oracle resynthesis, §4 of the baseline doc) that catches
   this exact case and recovers it. `emit_underlying_templated`'s own doc, read directly, confirms
   there is no equivalent: *"nothing in THIS function's call graph increments \[the enumeration
   budget\] today (no composite builder ever runs here)"* — i.e. the templated/cascade path has
   **no resynthesis mechanism at all** for `InsertSimpleContext`/`ModifyFromInput`, which is exactly
   the Aweti-relevant construct family. `spr`/`sapr`/`smm`/`qil` (both the bare roots of the
   FormII/Hollow-marked entries and their derived forms) all fail for this reason.

This is not a scale-dependent finding contingent on Aweti's absence — it reproduces on a 9-entry,
5-rule grammar, deterministically, every run.

## Verdict against each success criterion

1. **Recall must stay at 100%.** **Fails, on the one grammar that actually exercises templatic
   interdigitation.** Per the task's own rule ("any recall loss ends the experiment... no size or
   speed gain trades against it"), this is decisive: the cascade path, as it exists in this repo
   today (`compile_templated_morphotactics`/`emit_underlying_templated`), is **not a safe drop-in
   replacement** for the shipped enumeration engine on interdigitating/ablaut grammars. Recall holds
   exactly (100%, byte-for-byte candidate/confirm parity) on the two non-interdigitating cases.
2. **Does templatic interdigitation stop blowing up?** **Not answered — and cannot be answered from
   this worktree.** The real Aweti corpus is absent (gitignored, no synthetic replacement exists in
   this repo). What WAS measured is a different, prior question: on the one public fixture that
   exercises the relevant construct family at all, the cascade path doesn't reach a blowup — it
   reaches an **under-generation failure** first, at a scale far too small for either architecture to
   blow up regardless. Whether the (currently-nonexistent) fixed version of
   `emit_underlying_templated`'s process-morph handling would avoid the enumeration blowup at Aweti
   scale is a real, still-open, and now better-scoped question — this experiment narrows it to "fix
   the recall gap first," rather than answering the scale question directly.
3. **Network size.** Where recall holds, the cascade's compiled network was smaller in both cases
   measured (25/32 vs 29/60 states/arcs on the control; identical 4/5 on the RTL case, never larger).
   Consistent with, but far too small a sample to generalize as, "cascade composition beats
   enumeration on size."
4. **Build time.** No meaningful signal at this scale — every build in this experiment completed in
   3–40ms, with the cascade's fixed per-rule `fsm_parse_regex`/`fsm_compose` overhead (~9–20ms
   regardless of grammar size) dominating over any real growth-rate difference. This criterion
   needs an Aweti-scale (or at least two-orders-of-magnitude-larger) grammar to say anything real;
   none was available.
5. **Candidates per word.** Identical on both non-templatic cases (1.00 and 0.67/word, exactly). On
   the templatic case the cascade proposes *fewer* candidates per word (0.76 vs. 1.16) — but this is
   not "tighter proposing," it is the same recall loss showing up as a candidates-side symptom (the
   6 missing words simply propose 0 candidates each rather than 1).

## What could not be measured, honestly

- **Aweti-scale interdigitation blowup**, the single most important number per the task's own
  priority order — blocked by corpus absence (`samples/data/` does not exist in this worktree; no
  synthetic replacement exists in `pg-foma`'s own test suite). Building a large synthetic
  Aweti-shaped grammar generator to substitute was considered and rejected as out of scope for this
  session: it would itself need calibration against real typological facts to be a meaningful
  proxy, and the templatic-root-modification finding above (an outright recall failure) makes such a
  measurement moot until that gap is closed regardless — measuring the SIZE of a construction that
  doesn't preserve recall would not answer a useful question.
- **A larger, more diverse sample of grammars** — three fixtures, per the task's explicit
  preference for a small discriminating set over a broad sweep. A fourth or fifth grammar (e.g. a
  compounding-heavy or multi-table case) was not attempted; nothing found here suggests it would
  change the headline verdict, but it was not checked.

## Verdict

**No viable gain today, and a real, reproducible recall regression on exactly the construct family
this path was hoped to fix.** The Kaplan-Kay cascade (`compile_templated_morphotactics`) is
trivially reachable as a drop-in analysis engine (zero new plumbing beyond a ~280-line test driver)
and, where it already works (plain concatenative morphology, RTL rewrite), it matches the shipped
engine's recall exactly at equal-or-smaller network size — a genuine, if narrow, positive result.
But on the one available public fixture for the actual motivating case (templatic/interdigitating
"process" morphs), it loses 6 of 25 words' recall outright, for two named, source-verified reasons
(`prEpenthesis`/`prSimulFeeding` silently skipped by the rule compiler; `InsertSimpleContext`/
`ModifyFromInput` never resynthesized at all, no fallback). Per this project's own contract, that
recall loss ends the experiment for this construct family before any scale/size/speed question could
even be asked. The single number that most supports this verdict: **6 of 25 words (24%) that both
the oracle and the shipped engine confirm, the cascade path confirms zero** —
`rust/crates/pg-foma/tests/cascade_vs_enumeration_experiment.rs::templatic_interdigitation_case`.

## Reproduction

```powershell
# Step 0 verification (already committed at a29887a; re-run to reproduce):
rust\tools\pg.ps1 -Mode test -Package pg-foma -ExtraArgs --status-level,fail,--no-fail-fast,-E,binary_id(~bare)
rust\tools\pg.ps1 -Mode test -Package pg-foma -ExtraArgs --status-level,fail,--no-fail-fast
rust\tools\pg.ps1 -Mode test -Package pg-parse -ExtraArgs --status-level,fail,--no-fail-fast

# The experiment itself (committed at 53c5a32):
rust\tools\pg.ps1 -Mode test -Package pg-foma -ExtraArgs --no-capture,--status-level,all,--no-fail-fast,-E,binary_id(~cascade_vs_enumeration)
```

Both invocations were run in the foreground on this branch's worktree, no background jobs or
polling used anywhere in this session.
