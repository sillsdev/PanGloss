# STAGING: compounding-non-recursive

## Why this fixture exists

`openspec/changes/cover-compounding`'s conformance kit item (design.md's own scope: promote
`MorphRuleDef::Compounding`'s non-recursive case from `Disposition::FailClosed` to the
`ConfigPredicate` landing spot — `compounding.non-recursive` → `ConfirmOnly`, once a license-gated
propose shape and a recursion-reachability check both exist). This fixture pins:

1. **A single, non-recursive `CompoundingRuleDef`** compiles and runs correctly end to end (bare
   roots AND compounds), matching `crate::capability::compounding_recursive`'s own characterization
   (a lone rule, `multipleApplication` at its DTD default of 1, is never flagged recursive).
2. **The MPR group-(un)awareness contract** (design.md D4, the load-bearing trap): the rule's THREE
   `*ProdRestrictionsMprFeatures` fields (only `headProdRestrictionsMprFeatures` is exercised here)
   are tested with the group-**UNAWARE** `MprSet::compound_match` (a flat overlap test, mirroring
   C#'s `CompoundMprFeaturesMatch`), while each subrule's own `requiredMPRFeatures`/
   `excludedMPRFeatures` is tested with the group-**AWARE** `Grammar::mpr_group_ok`. Getting this
   backwards either direction is a real bug: using the group-aware test on a RULE-level field would
   silently REFUSE a stem `compound_match` admits (a genuine recall-loss bug); using the flat test
   on a SUBRULE field would silently ADMIT a stem the engine's own `synth_compound_subrule` gate
   rejects (a precision, not recall, bug — still safe for a proposer's over-approximation, but not
   what this fixture is pinning).
3. **A left-to-confirm syntactic-FS gate** (`nonHeadPartsOfSpeech`, design.md D3): a stem MPR-
   licensed as a non-head but disagreeing on part of speech has no valid derivation — proving this
   gate is honored (by whichever engine layer is responsible for it), never silently dropped or
   silently ignored.

## What it pins

- `fasu`/`bel`/`zon`/`numo`/`tiku`: five plain bare-root controls (one per lexical entry), proving
  ordinary lookup is unaffected by the compounding rule's presence.
- `fasubel` (headA `fasu` + nonHeadOk `bel`): **the positive witness** — headA carries only ONE of
  the two `{mpr1,mpr2}` all-type-group members `headProdRestrictionsMprFeatures` names (admitted by
  `compound_match`'s flat overlap) AND both of the `{mpr3,mpr4}` all-type-group members the
  subrule's own `requiredMPRFeatures` names (admitted by the group-aware `mpr_group_ok`, correctly
  requiring BOTH). This is the load-bearing pin: a naive "always use the group-aware helper"
  refactor of the rule-level field would make this word wrongly fail to derive.
- `fasuzon` (headA + nonHeadBadPos `zon`, `posOther`): **`expect_fail: true`** — MPR-licensed as a
  non-head, but `nonHeadPartsOfSpeech="posHead"` rejects `zon`'s own `posOther`. The load-bearing
  assertion: an engine that ignores this syntactic-FS gate (in EITHER direction: over-permissive
  propose that never gets pruned, or a confirm step that forgets to check it) would wrongly accept
  this.
- `tikubel` (headB `tiku` + nonHeadOk `bel`): **`expect_fail: true`** — headB carries only ONE of
  the subrule's own `{mpr3,mpr4}` all-type-group members. The group-aware `mpr_group_ok` correctly
  excludes it; an engine that used the flat `compound_match` test here instead would wrongly accept
  it (the complementary direction of the D4 trap — over-permissive, not recall-losing, but still a
  real correctness bug relative to the exact HermitCrab semantics this construct specifies).
- `numobel` (headC `numo`, no MPR features at all, + nonHeadOk `bel`): **`expect_fail: true`** — the
  rule-level gate's negative control, proving `headProdRestrictionsMprFeatures` genuinely restricts
  something rather than vacuously admitting every root.

## Oracle discipline

**Oracle: `pangloss` (this repo's own Rust engine), NOT the C# founding oracle.** Authored fresh for
this task; `words.yaml` signatures captured by driving `pg_parse::Morpher::parse_word` directly (a
throwaway in-repo test — see "Verification" below), matching every other fixture in this suite's own
documented oracle-discipline note (machine acceptance must re-verify against the C# founding oracle
before graduation).

## Verification

Signatures were captured via a throwaway test (`rust/crates/pg-foma/tests/zz_throwaway_sig_dump.rs`,
deleted after transcription) driving `pg_parse::Morpher::parse_word` directly over every word in
`words.yaml`, using the SAME grammar this directory's `grammar.xml` ships (byte-identical to
`rust/crates/pg-foma/tests/cover_compounding.rs`'s own already-oracle-verified containment fixture —
that file's five `#[test]`s independently prove the SAME five load-bearing scenarios end to end via
`pg_foma::composite::FomaAnalyzer` checked against `pg_parse::Morpher`, using
`FomaOutcome::candidates_generated`/`confirmed` to additionally prove the FST proposer over-generates
and confirm prunes to the oracle-exact set — a stronger, propose-vs-confirm-containment claim this
plain oracle-replay fixture does not itself make). Cross-checked in-repo by
`rust/crates/pg-parse/tests/conformance_fixtures_gate.rs`'s `all_discovered_fixtures_match_oracle`
test (dual-root discovery, default `cargo test --workspace` suite) — that test is what actually gates
CI; the throwaway dump test was deleted after transcription.

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/compounding-non-recursive/`. On acceptance, delete this staged copy
in the same change (graduation guard enforces this mechanically).
