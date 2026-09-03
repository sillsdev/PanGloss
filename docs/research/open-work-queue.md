# Open work queue

What is genuinely owed, with what "done" means for each. Delete an entry when it lands; this file is
a queue, not a history.

Audited against the code at `cb42ab92`. A queue that misreports its own state is worse than no
queue, so each entry below carries the evidence its status rests on.

## 0. Full, accurate conformance coverage -- the standing goal

Every fixture oracle-verified against hc.dll, every fixture with at least one oracle-exact backend,
zero compile-but-miss cells, soundness zero. Where it stands at `cb42ab92`, per the gates that hold
these numbers (`backend_scoreboard_gate`, `faithfulness_coverage_gate`, `conformance_fixtures_gate`,
`rust/tools/oracle-conformance.ps1`):

**Oracle side.** 38 of 39 staged fixtures are `founding-oracle`; the one `rust-only` is `guesser-pattern-root-fallback`, whose guessed words hc.dll exposes no CLI surface for. Upstream 34/34 (the pin sits at `25ddf914`, which carries the two witnesses for this week's port divergences, `rewrite-analysis-feature-neutralization` and `synthesis-stratum-render-stale-table`), filter-passes 9/9 (mirrored into the harness by `oracle-conformance.ps1`). HC-Rust agrees with hc.dll on every replayed word, compared as a multiset.

**Per fixture, at `63a81cfc` (64 fixtures, `conf_matrix`; the pin moved to `25ddf914` and brought two
upstream witnesses for this week's port fixes):** 24 exact on all three backends, 26 on two, 12 on
one, **2 on none** -- `segment-natural-class-table-binding` and its upstream twin
`rewrite-analysis-feature-neutralization`, one phenomenon (grill-me G11). Those two are the whole
remaining "at least one backend" gap. At v0.2.0 the headline over 62 read 16 / 21 / 17 / 7. Soundness
is 0 on every cell; every miss listed below is the ADR-0001 direction and named in a ratchet.

**Backend side, TunedSurfaceProbed** (the shipping backend): 61 exact / 2 miss / 1 refused of 64.
- The misses (ADR-0001's forbidden direction): `segment-natural-class-table-binding` "g" and
  `rewrite-analysis-feature-neutralization` "d" -- the cross-table second analysis hc.dll's
  analysis-side feature erasure yields, which no forward composition produces (G11).
- The refusal is deliberate and witnessed: `pattern-root-required-environment`, an unbounded root
  with required environments, the one shape the regex route excludes.
- `mpr-gated-exception` "mentanukam" (two derivation orders, one proposal) is NOT a miss: confirm's
  multiplicity recovery returns both derivations from the one proposal (measured 1 proposed, 2
  confirmed, 2 in the oracle), so the faithfulness instrument now checks that each distinct oracle
  identity is proposed at least once. Faithfulness ratchet 5.
- The "g" cell above is a decision, not a patch -- see grill-me G11.
- TUT and PC have no misses left (figures below): the alpha-variable polarity flip, the document-order limitation under an unordered stratum (an order lattice for small unordered zones), the insert-then-truncate-leading rule (its deletion now anchors to the rule's marker, not the word edge), and PC's wholesale skip of realizational rules are all fixed.
- Fixed in this campaign: `process-morphology-in-place-mutation`, `circumfix-non-first-allomorph-
  selection`, `suffixing-extension-slot-ordering`, `metathesis-comparison-crash` (an instrument
  defect), `morphotactic-attribute-breadth` on TSP/TUT (a repeated application decoded as an extra
  morpheme), `two-table-shared-representation-recall` "y", and the three `[Any]*` pattern-root
  fixtures via the regex route.

**Backend side, the other two.** TemplatedUnderlyingTokens: 45 exact / 0 miss / 19 refused of 64. Its
refusals are typed `Partial { uncovered }` items, named one by one: infix, reduplication and
circumfix-prefix shapes (13 fixtures; no root-splitting or copying construction exists on the token
route, and two prior attempts to bypass the classification broke working fixtures), process
morphology, four bistratal roots whose segment has no representation in the final table, and two
rewrite-cascade refusals (one deliberate: simultaneous-subrule overlap, unsupported by definition).
PlanComposed: 30 exact / 0 miss / 31 refused / 3 unmeasurable of 64. It now builds a composite marker
subtree by calling the tuned route's own construction, but only when that material is provably
complete (no other rule or template can wrap the stem); every other marker plan keeps its typed
refusal, because a bare-word union that could under-generate must never be admitted. Widening that
admission (affix wrapping around composite stems) is the next PlanComposed lever.

**Tooling gap found while bumping the pin.** `Initialize-ConformanceSubmodule`'s fast path tests only
the sentinel file, so an existing checkout whose pin has MOVED keeps its old submodule tree and
`oracle-conformance.ps1` then picks the control exe for the old pin. A fresh worktree is fine (it
fetches the pin); an existing one silently lags. The fast path should compare the checked-out
submodule commit to `git ls-tree HEAD -- machine` and re-fetch on drift.

**Instrument note.** The pg-parse replay gate compares parse MULTISETS (PROTOCOL.md section 4 rule
3): a doubled derivation such as `mpr-gated-exception` "mentanukam" is compared by count, and HC-Rust
matches hc.dll there. The scoreboard, by contrast, compares deduplicated identity sets, so a
proposer offering one candidate for two derivations is visible only to the faithfulness gate.

**Done when:** the miss lists above are empty, every fixture has an exact backend, and the submodule
pin carries the two upstream fixtures.

## 2. Merge `fix/env-repvariant`

Five commits of Aweti/Mbugwe recall-census tooling. `git merge-base --is-ancestor` says not merged.
Its blocker -- `-Mode run` stdout capture -- is done, so it is mergeable now.

## 3. Three registered facts have no can-fire fixture

`default_grammar_wide_checks()` registers 10 checks. Seven have a one-way pin (six in
`tests/envelope_agrees_with_compiler_gate.rs`, one in `tests/backend_selection_contract.rs`). These
three have none:

- `TemplatedRouteUncoveredCheck` (`templated-route.emission-uncovered`)
- `RuleCascadeUncompilableCheck` (`templated-route.rule-cascade-uncompilable`)
- `TemplatedShapeFloorCheck` (`strategy-coverage.templated-unsupported-shape`)

A registered fact with no can-fire test is a refusal nobody has proven fires. That is the shape this
repo keeps finding: computed, registered, and never demonstrated to act.

**Done when:** each has a fixture proving it fires, following the pattern of the existing six.

Related, same family, cheaper: `cargo check` reports `BACKEND_REFUSED_GRAMMAR_XML`
(`pg-cli/src/test_support.rs:96`) as never used. A fixture named for a refusal that no test consumes
is a refusal nobody exercises. Either wire it to a test or delete it.

## 4. One advice shape key is wrong, and the right one does not exist

`capability_shape_key`'s `_ => "nonregular-process-morphology"` catch-all is reached by exactly three
diagnostics today, all `strategy-coverage.construct-not-representable`:

| pair | label it gets | correct? |
|---|---|---|
| PlanComposed x ProcessMorphology | nonregular-process-morphology | yes, coincidentally |
| TemplatedUnderlyingTokens x ProcessMorphology | nonregular-process-morphology | yes, coincidentally |
| **PlanComposed x RealizationalMorphology** | nonregular-process-morphology | **no** |

The third gets advice about process morphology for a realizational-morphology refusal.
`assets/backend-advice-v1.toml` defines nine shape keys and none covers realizational morphology, so
the fix is not a routing change -- it needs either a new catalog entry (advice content: a linguistic
judgement about what to recommend) or a decision that no advice is better than wrong advice.

**Not spec-required:** a prior review established ADR-0001's no-catch-all rule targets the
characterizer's enumerator, not this advice lookup. This is a correctness wart, not a contract
breach.

**Done when:** the realizational pair gets correct advice, or explicitly none.

## 5. ADR-0001's behavioral provenance tier has no representation

`EvidenceProvenance` has one variant. ADR-0001's "Two-tier, migrating" names two, and the missing one
-- `behavioral`, proven by oracle witnesses -- describes the **production mainline** (the black-box
foma compiler), not a future path.

The gap is now recorded on the type itself rather than silent, which was the review finding. Closing
it needs a predicate that actually constrains on an oracle witness; adding a variant nothing
constructs would be a control that cannot act.

**Done when:** a behavioral-evidence predicate exists, or the ADR is amended to say one tier is
enough today.

## 6. `stats_cmd` is nondeterministic, and it is probably two defects

Five distinct tests in `pg-cli`'s `stats_cmd::tests` have failed intermittently. Scratch-path
collision is ruled out (pid+counter keyed), and so is pure cross-process contention -- one failed
running its module alone, 29/30.

An audit found no single mechanism, and good reason to think there is none: the batch-report flake
traces to `run_batch`'s 8-thread word fan-out, while `never_fires_keeps_attempt_denominators_within_rule_kind`
and `named_allomorph_report_does_not_claim_rule_attempts` are single-threaded unit tests on synthetic
data with no batch involvement. Two shapes, not one.

Each costs a re-run to distinguish from a real regression, every time. Not re-verified this pass --
these are `#[ignore]`d/gated or require a live run to observe flake, and no test execution was done
for this audit; the entry stands on the prior finding, not a fresh re-check.

## Future, not queued

- **Circumfix cross-product loading** (a FieldWorks/LCM `MoAffixProcess`-shaped entry, prefix-typed x
  suffix-typed halves) is now IMPLEMENTED for unconditioned entries; an environment-bearing half is
  refused rather than silently dropped or mis-combined. See
  `docs/research/circumfix-cross-product-loading.md`. Not a queue item -- recorded here because it
  was open work the last time this file was written and is no longer.
- **Aweti/Mbugwe sizing**, parked by instruction. The `REP_VARIANT_CAP` constant this entry used to
  name no longer exists -- it was replaced by `REP_VARIANT_WARN_THRESHOLD` (advisory, drops nothing)
  and `REP_VARIANT_BYTE_BUDGET` (a 1 GiB containment stop), reported through a `VariantLimit` enum
  (`rust/crates/pg-foma/src/emit.rs`).
  **Backend-acceptance status contradicts what this task was told to assume**, and is recorded here
  as checked-in code rather than guessed: `rust/crates/pg-foma/tests/
  five_language_backend_reports_gate.rs` (`#[ignore]`d on a gitignored corpus, last touched
  2026-08-30, commit `6b46914b`) still pins `assert_no_backend_accepts` for BOTH Aweti and Mbugwe --
  `TunedSurfaceProbed` itself now refuses with shape `"repeated-application"`
  (`Severity::CannotRepresent`), not an accepted backend. This is a *different* mechanism than the
  old REP_VARIANT overflow story (that shape key comes from `compounding.non-recursive` /
  `quantifier.bounded-expansion`, per `backend_selection.rs`'s `capability_shape_key`), and it means
  neither grammar has an accepted backend as of this commit. Separately and NOT in conflict:
  `pangloss fst-health`'s *representability* axis (a different, whole-grammar report) can still read
  `WithinLimits` for both (per `docs/research/circumfix-cross-product-loading.md` and
  `docs/research/conformance-containment-inventory.md`) -- that axis answers "can this be
  structurally represented at all", not "does a specific `EmissionStrategy` accept it". This test is
  corpus-gated and was not run for this audit; if something has changed backend acceptance since
  `6b46914b`, re-run `five_language_backend_reports_gate.rs --include-ignored` to confirm before
  trusting either version of this claim.
