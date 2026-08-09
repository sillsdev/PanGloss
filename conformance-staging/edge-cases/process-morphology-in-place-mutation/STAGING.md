# STAGING: process-morphology-in-place-mutation

## Why this fixture exists

Narrow, single-purpose probe for `pg_foma::capability::CharacteristicKind::ProcessMorphology` — a
`Modify`-only allomorph (the input is mutated in place rather than affixed to: ablaut, mutation,
simulfix). Closes the `ProcessMorphology x tuned-surface-probed` gap in
`witnessed_strategy_coverage_gate` (the other two backends already declare this
`CannotRepresent`, per `strategy_coverage.rs`).

One `MorphologicalRule` (`mrAblaut`), Modify-only, no affix at all: past tense is realized purely
by mutating the root segment "i" to "a". Three single-segment lexical entries: `eI` ("i", the
mutable root), `eA` ("a", an unrelated root that happens to already be spelled with mrAblaut's own
output segment), `eU` ("u", a plain structural control).

### Why the root is a single segment, not a realistic multi-consonant shape

The first draft of this fixture mirrored `languages/fusional-realizational-morphology`'s own
`mrAblaut` section exactly: three `MorphologicalInput` parts (`pre`/`target`/`post`), with
`CopyFromInput` on the outer two and `ModifyFromInput` on the middle one — the natural way to
represent "an interior segment changes, everything around it doesn't". Verified via the probe
(see "Verification") that this shape characterizes as `CircumfixOutputAction`, **not**
`ProcessMorphology`: `crate::emit::classify_affix` returns `Role::Infix` for any RHS with a
non-`Copy` action strictly between two `Copy` actions (`emit.rs`'s own algorithm), and
`allomorph_drops_lhs_material` separately flags "never Copies at least one LHS part" (the
`Modify`-covered middle part) as `CircumfixOutputAction` regardless of `Role`. Neither reaches
`Role::Process`. Only a *Modify-only* RHS — zero `CopyFromInput` actions at all — reaches it
(the early-return branch in `classify_affix`), which is exactly `capability.rs`'s own
`ABLAUT_PROCESS_XML` unit fixture's shape: one input part, one `Modify`. This fixture's root is a
single segment for the same reason: it is the only shape that stays Modify-only while still
letting the mutation apply to something a self-check reversal can verify. The CONSTRUCT this pins
— a category realized purely by featural mutation, no affix — is a realistic morphological
pattern even though this particular root is minimal; the multi-segment `pre`/`target`/`post` shape
remains legitimate HermitCrab morphology, it simply classifies as a different characteristic in
THIS compiler.

## What it pins

- `i` bare parses as itself only.
- `a` — THE construct-exhibiting row — must have **exactly two** analyses: `eA` bare, and `eI` +
  `mrAblaut` (past, via mutation). If the `ProcessMorphology` compile path
  (`crate::emit::is_structural_rule` admitting `Role::Process`, replaying
  `pg_rules::morph::synthesize` for a faithful mutated surface) regressed, the second analysis
  would silently disappear and `a` would wrongly drop to one parse — that disappearance is the
  red-on-revert.
- `u` — a plain structural control — parses as itself only; `mrAblaut`'s output class contains
  only "a", so it can never leak onto an unrelated segment.

Empirically confirmed (see "Verification"): `a` came back as `AROOT|a;ROOT+ABLAUT|a` from a real
`pg_parse::Morpher` run — both analyses present, exactly as designed.

## Oracle discipline

**Oracle: `pangloss` (this repo's own Rust engine), NOT the C# founding oracle.** Authored fresh
for this task; `words.yaml` signatures captured by driving `pg_parse::Morpher::parse_word` directly
(a throwaway in-repo test — see "Verification" below).

## Verification

Signatures were captured by a throwaway test (`rust/crates/pg-foma/tests/temp_probe_new_fixtures.rs`,
deleted after transcription) that loaded this fixture's `grammar.xml` from disk, printed
`characterize(&g).observations()` and `select_backends_for_grammar(&g)`'s per-backend reports, and
ran `pg_parse::Morpher::parse_word` over every word above.

`characterize` reports `{Affixation, UnorderedMorphRuleApplication, NaturalClassDefinition,
ProcessMorphology}` for this grammar — confirming the construct is actually exhibited, not
assumed. Backend selection: `TunedSurfaceProbed` is selected with decision `ConfirmOnly` (this is
the backend whose witness closes the gap); `TemplatedUnderlyingTokens` and `PlanComposed` are both
**refused**, citing `strategy-coverage.construct-not-representable` for `ProcessMorphology` — the
exact two `CannotRepresent` rows `strategy_coverage.rs` already declares, so this fixture cannot
and does not manufacture a false witness for either of them; only the one open gap
(`ProcessMorphology x tuned-surface-probed`) is closed.

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/process-morphology-in-place-mutation/`. On acceptance, delete this
staged copy in the same change (graduation guard enforces this mechanically).
