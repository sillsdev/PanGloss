# Proposal — surface-compile-profile-and-templated-routing

## Why

Onboarding the 5th test language measured `FomaAnalyzer::new` at 419-906s wall (range across a
quiet vs contended machine) for a grammar of only 310 lex entries / 223 mrules / 34 prules —
while Sena's 255k-line lexc compiles in <30s. Investigation attributed the cost to the eager
Rust-side composite enumeration, with a specific aggravator:

1. **Two O(roots × rules) probing passes both run.** `preexpand::build_composites` runs because
   the grammar has phonology; `emit::build_structural_composites` ALSO runs because
   `probe_would_refuse` is true — and when true it broadens the structural sweep from the narrow
   LHS-dropping set to EVERY ordinary Prefix/Suffix/Infix rule (`emit.rs:1851-1863`).
2. **The broadening trigger is a single ordinary epenthesis rule.** Any `Rewrite` with an empty
   LHS trips `probe_would_refuse` (`emit.rs:1833-1838`). Epenthesis inserts into nothing by
   definition, so any grammar with ordinary epenthesis pays the grammar-wide broadened sweep.
3. **The already-built, already-registered `TemplatedUnderlyingEmit` backend skips the composite
   pipeline entirely** (its own doc: "No composite pipeline at all ... skipping it unconditionally
   is the scale fix this function exists for") and is structurally applicable to this grammar
   today (`Applicability::HasPhonologyOrTemplates`, `backend_registry.rs:811-819`). Whether it
   actually compiles this grammar correctly and fast has never been measured.
4. **The per-stage timing that would have answered "where did 419s go" in one run already exists
   and is discarded**: `FomaProposer::new_with_profile` populates a full `CompileProfile`
   (6 `CompileStage` variants, per-stage durations, lexc lines, state/arc counts), but
   `pangloss fst-health` folds it through `health_evaluator`'s ≥80%-of-budget threshold filter
   and throws the rest away. No shipped command prints the stage breakdown for a real grammar.

**Verdict from the investigation: no new backend is warranted.** The work is (a) measure the
existing templated backend on cascade-family grammars, (b) make the profile visible, (c) narrow
the broadening trigger with a recall proof — in that order of certainty.

## What Changes

- `pangloss fst-health` gains a mode that emits the raw `CompileProfile` (per-stage wall,
  lexc lines, entry counts by mechanism) as JSON — measurement infrastructure, no semantics.
- A measured comparison of `TunedSurfaceProbed` vs `TemplatedUnderlyingTokens` on the
  cascade-family shape (synthetic fixture at comparable scale + local real-grammar numbers
  recorded in the PR only): compile wall, recall vs oracle, proposer candidate volume.
- If (and only if) the measurement supports it: backend-selection/optimizer wiring so
  cascade-family grammars route to the templated backend, per the existing witnessed-strategy
  machinery. Gated on the existing recall/conformance gates — a faster backend that loses recall
  is not a result.
- Narrow `probe_would_refuse`'s broad-mode so one empty-LHS epenthesis rule does not force the
  grammar-wide structural sweep — REQUIRES a recall-preservation proof (fire-count + deterministic
  counter delta per the repo's optimization evidence rules), since the predicate is a documented
  conservative over-approximation. If no sound narrowing exists, document why and drop the task.

## Non-goals / Dependencies

- Merges AFTER `cover-circumfix-cross-product-and-infix-drop` (that change owns the
  `is_structural_rule` region; this one owns `probe_would_refuse` + `fst_health.rs`; adjacent
  emit.rs regions are serialized per STAGING).
- Does not change enumeration-budget defaults; the just-over-budget regression pin stays.
- The templated backend's own `CircumfixOutputAction` known-gap
  (`strategy_coverage.rs:318-325`) is out of scope here; routing decisions must respect it via
  the capability gate exactly as today (grammar-wide gate semantics unchanged).

## Impact

- Claims: observation (stage attribution, measured), support (profile visibility), certification
  only through existing gates. All real-grammar numbers stay local; committed evidence is
  synthetic-fixture-based.
- Files: `pg-cli/src/fst_health.rs`, `pg-foma/src/emit.rs` (`probe_would_refuse` region only),
  backend-selection/optimizer call sites, one new synthetic scale fixture.
