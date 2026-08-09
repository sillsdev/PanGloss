# STAGING: free-fluctuating-allomorph-pair

## Why this fixture exists

Narrow, single-purpose probe for `pg_foma::capability::CharacteristicKind::FreeFluctuation` — the
disjunctive-allomorph re-check (`pg_rules::validity::allomorphs_valid_impl`,
`free_fluctuates`/`disjunctive_candidates`/`root_constraints_equal`; C# `Allomorph.cs:127-152`):
engaged whenever a `LexEntryDef` carries more than one `RootAllomorphDef`. Confirm enforces
"first-listed matching allomorph wins" for any two allomorphs whose own
`environments`/`is_bound` do NOT compare equal; when they DO compare equal, the allomorphs
"free-fluctuate" and either is accepted. `crate::emit` builds no ordering/preference machinery for
this at all — every allomorph of a multi-allomorph entry is proposed uniformly, in every position.

Two lexical entries:

- `eAlt` ("pol"/"pel") — THE construct: two allomorphs, both totally unconstrained, so their
  constraint sets compare EQUAL and they free-fluctuate.
- `eOrd` ("kit"/"kot") — the contrastive negative control: allomorph 0 is environment-restricted
  (requires a following "s"), allomorph 1 is not, so their constraint sets DIFFER and ordinary
  disjunctive gating (not free-fluctuation) applies.

This is a dedicated, minimal isolation of exactly the shape
`machine/conformance/edge-cases/disjunctive-recheck` already carries informally via its own
`eGray`/`eWalk` pair (that fixture's module doc calls it the "free-fluctuation escape" — one row
among several disjunctivity probes). This fixture makes FreeFluctuation the sole subject rather
than a footnote.

## What it pins

- `pol`/`pel` both parse (bare, unconstrained) — free-fluctuation acceptance: the later-indexed
  "pel" is not rejected even though "pol" "would also have matched" trivially.
- `kits` parses (allomorph 0's own environment satisfied); `kots` must **fail** — allomorph 0's
  environment is ALSO satisfied at that position, and allomorph 1 does not free-fluctuate with it
  (their constraints differ), so the disjunctive re-check rejects it. This is the load-bearing
  contrast: an engine that treats every multi-allomorph entry as free-fluctuating regardless of
  whether the constraints actually compare equal would wrongly accept `kots`.
- `kot`/`kit`: plain structural controls (bare forms, one valid, one not, for the reasons the
  `words.yaml` notes give).

Empirically confirmed (see "Verification"): all six words came back exactly as predicted from a
real `pg_parse::Morpher` run.

## IMPORTANT: this fixture does NOT close its coverage-gate gap

`pg_foma::capability::characterize` contains **no code path that ever emits a `FreeFluctuation`
observation**, for any grammar. Confirmed empirically (a temporary, now-deleted probe test) the
same three ways described in `stem-name-restricted-root-allomorph/STAGING.md`: a synthetic probe
grammar, this fixture itself (characterizes as `{Affixation, UnorderedMorphRuleApplication,
NaturalClassDefinition}`, nothing more), and — most tellingly — `machine/conformance/edge-cases/
disjunctive-recheck`, an ALREADY-DISCOVERED fixture in the 46-fixture baseline sweep that
structurally contains the exact free-fluctuating-pair shape (`eGray`) this new fixture isolates,
contributes zero `FreeFluctuation` observations either.

This is a **compiler detection gap in `characterize`**, not a fixture-authoring gap — see that
sibling `STAGING.md` for the full argument, which applies here unchanged (the only difference is
which `CharacteristicKind` variant lacks a detection walk: `FreeFluctuation`'s trigger condition,
per its own doc, would be "a `LexEntryDef` with more than one `RootAllomorphDef`", and no code
anywhere raises it). No fixture, however constructed, can close `FreeFluctuation x
{plan-composed, tuned-surface-probed, templated-underlying-tokens}` in
`witnessed_strategy_coverage_gate` until `characterize` gains that detection code. This fixture is
staged anyway, both because it correctly pins real, load-bearing confirm-time behavior, and
because — being a DEDICATED, minimal isolation rather than one row inside a broader disjunctivity
probe — it is the natural first fixture to close those three gaps once the detection code exists.

## Oracle discipline

**Oracle: `pangloss` (this repo's own Rust engine), NOT the C# founding oracle.** Authored fresh
for this task; `words.yaml` signatures captured by driving `pg_parse::Morpher::parse_word` directly
(a throwaway in-repo test — see "Verification" below).

## Verification

Signatures and the `characterize`/`select_backends_for_grammar` findings above were captured by a
throwaway test (`rust/crates/pg-foma/tests/temp_probe_new_fixtures.rs`, deleted after
transcription) that loaded this fixture's `grammar.xml` from disk, printed
`characterize(&g).observations()` and `select_backends_for_grammar(&g)`'s per-backend reports, and
ran `pg_parse::Morpher::parse_word` over every word above. All three backends are selected with
decision `ConfirmOnly` for this grammar.

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/free-fluctuating-allomorph-pair/`. On acceptance, delete this
staged copy in the same change (graduation guard enforces this mechanically).
