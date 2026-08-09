# STAGING: stem-name-restricted-root-allomorph

## Why this fixture exists

Narrow, single-purpose probe for `pg_foma::capability::CharacteristicKind::StemName` — a root
allomorph restricted to a `<StemName>` region (`RootAllomorphDef::stem_name`), checked only at
confirm time by `pg_rules::validity`'s `stem_name_gate_reason`/`stem_name_required_match` (C#
`StemName.IsRequiredMatch`/`IsExcludedMatch`) against the word's accumulated syntactic feature
structure. `crate::emit` has no stem-name-aware admission filter anywhere; every stem-restricted
root allomorph is proposed unconditionally today.

One entry, two root allomorphs, one `<StemName>` region covering `featPers=1`:

- `aDefault` ("tam"), no `stemName` — an EXCLUDED match: valid only when the word's FS falls
  OUTSIDE every declared `StemName`'s region.
- `aRestricted` ("kap"), `stemName="snP1"` — a REQUIRED match: valid only when the word's FS
  falls INSIDE `snP1`'s region.

Two ordinary person-marking rules (`mrPers1` assigns `featPers=1`, `mrPers2` assigns `featPers=2`)
move the word's FS in and out of `snP1`'s region, giving four combinations that pin exactly which
allomorph the confirm-time gate accepts in each state.

## What it pins

- `kap` bare must **fail**: `snP1`'s region requires `featPers=1` to be assigned, and a bare word
  has no person feature at all. Zero parses.
- `kapom` (restricted allomorph + `mrPers1`) must **succeed**; `tamom` (default allomorph +
  `mrPers1`) must **fail** — the required-match/excluded-match pair for the SAME rule, differing
  only in which allomorph was used. An engine that proposes every allomorph uniformly regardless
  of stem-name restriction (`crate::emit`'s own documented gap) would accept both.
- `tamur` (default + `mrPers2`) must **succeed**; `kapur` (restricted + `mrPers2`) must **fail** —
  the mirror-image pair, outside `snP1`'s region.

Empirically confirmed (see "Verification"): all six words came back exactly as predicted from a
real `pg_parse::Morpher` run — the confirm-time StemName gate itself is correct and already
enforces this. `words.yaml`'s six rows are that behavior transcribed, not asserted blind.

## IMPORTANT: this fixture does NOT close its coverage-gate gap

`pg_foma::capability::characterize` — the function `pg_foma::witnessed_coverage`'s completeness
account reads to decide which `CharacteristicKind`s a grammar "exhibits" — contains **no code path
that ever emits a `StemName` observation**, for any grammar. Confirmed empirically (a temporary,
now-deleted probe test) three separate ways: (1) a synthetic grammar built specifically to contain
a stem-name-restricted allomorph produced zero `StemName` observations from `characterize`; (2)
this fixture itself, loaded and characterized directly, produces `{Affixation,
UnorderedMorphRuleApplication, NaturalClassDefinition}` and nothing else; (3)
`machine/conformance/edge-cases/disjunctive-recheck` and
`machine/conformance/languages/fusional-realizational-morphology` — two ALREADY-DISCOVERED
fixtures that structurally contain stem-name-restricted allomorphs (the latter has one by name) —
contribute zero `StemName` observations to the 46-fixture baseline sweep either.

This is a **compiler detection gap in `characterize`**, not a fixture-authoring gap: the
`CharacteristicKind::StemName` variant, its `Disposition::ConfirmOnly`, and
`strategy_coverage.rs`'s three `Represents` rows all exist and are internally consistent — what is
missing is the walk over `Grammar::entries`/`RootAllomorphDef::stem_name` inside `characterize`
that would ever push the observation in the first place. No fixture, however constructed, can
close `StemName x {plan-composed, tuned-surface-probed, templated-underlying-tokens}` in
`witnessed_strategy_coverage_gate` until that walk is added. This fixture is staged anyway because
it correctly pins real, load-bearing confirm-time behavior (see "What it pins" above) and is ready
to be the FIRST fixture that closes those three gaps the moment `characterize` gains the
detection code.

## Oracle discipline

**Oracle: `pangloss` (this repo's own Rust engine), NOT the C# founding oracle.** Authored fresh
for this task; `words.yaml` signatures captured by driving `pg_parse::Morpher::parse_word` directly
(a throwaway in-repo test — see "Verification" below).

## Verification

Signatures and the `characterize`/`select_backends_for_grammar` findings above were captured by a
throwaway test (`rust/crates/pg-foma/tests/temp_probe_new_fixtures.rs`, deleted after
transcription) that loaded this fixture's `grammar.xml` from disk, printed
`characterize(&g).observations()` and `select_backends_for_grammar(&g)`'s per-backend reports, and
ran `pg_parse::Morpher::parse_word` over every word above. All three backends
(`TunedSurfaceProbed`/`TemplatedUnderlyingTokens`/`PlanComposed`) are selected with decision
`ConfirmOnly` for this grammar (no `RealizationalMorphology`/`ProcessMorphology`, so nothing here
triggers a refusal) — the gate's compile step (not this probe) is what actually turns that
selection into a witness, and cannot, per the finding above.

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/stem-name-restricted-root-allomorph/`. On acceptance, delete this
staged copy in the same change (graduation guard enforces this mechanically).
