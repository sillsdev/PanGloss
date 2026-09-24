# Backend scoreboard extraction: reconciliation against the pre-extraction baseline

Records how `rust/crates/pg-foma/tests/backend_scoreboard_gate.rs`'s ratchet constants were
reconciled against the counts measured by hand from `examples/conf_matrix.rs` at commit
`57515281` (before its per-(fixture, backend) evaluation was extracted into
`pg_foma::scoreboard` and given an `expect_crash` skip). One measurement run, recorded so the
next person touching this gate does not have to re-derive it.

## Baseline (57515281, no `expect_crash` skip, 61 fixtures per backend)

| backend | oracle-exact | refused | compile-but-miss | compiled, no verdict |
|---|---|---|---|---|
| TunedSurfaceProbed | 51 | 6 | 2 | 2 |
| TemplatedUnderlyingTokens | 33 | 21 | 5 | 2 |
| PlanComposed | 20 | 36 | 2 | 3 |

## After extraction + the `expect_crash` skip (60 fixtures per backend, 1 excluded)

| backend | oracle-exact | refused | compile-but-miss | compiled, no verdict |
|---|---|---|---|---|
| TunedSurfaceProbed | 51 | 6 | 3 | 0 |
| TemplatedUnderlyingTokens | 33 | 21 | 6 | 0 |
| PlanComposed | 20 | 35 | 2 | 3 |

`oracle-exact` is unchanged for every backend (104 total either way), and `PlanComposed`'s
`refused` count drops by exactly 1 (36 -> 35), fully explained by the one fixture the
`expect_crash` skip removes (`machine:edge-cases/simultaneous-epenthesis-cascade`, which
`PlanComposed` refused outright with a plan-topology marker before the skip existed).

## The `compile-but-miss` / `compiled, no verdict` split for TunedSurfaceProbed and
## TemplatedUnderlyingTokens does not fully explain by the one exclusion alone

Removing one fixture can move at most one cell per backend between buckets. The measured shift
for these two backends is larger: `compile-but-miss` gained 1 (2->3, 5->6) while `compiled, no
verdict` lost 2 (2->0, 2->0) -- a net of -1 (matching the fixture-count drop), but distributed
across more than one cell's worth of bucket movement per backend.

Traced cause: `machine:edge-cases/metathesis-comparison-crash` (which no longer carries
`expect_crash: true` -- fixed upstream by `sillsdev/machine#471`, per its own `words.yaml`
comment -- so it is NOT the excluded fixture and remains normally scored) measures
`Certification::Truncated { stage: "no-analyzable-words", .. }` on both `TunedSurfaceProbed` and
`TemplatedUnderlyingTokens`. That certification variant carries per-word evidence (`obs.words` is
`Some`), so `scoreboard::measure_fixture` counts it as `CellOutcome::CompilesButMisses` --
and would have under the pre-extraction inline code too, since that code's own branch on
`obs.words` is unchanged by the extraction. `no-analyzable-words` fires because the fixture's one
word is `expect_fail: true` (the oracle has zero valid analyses for it), which
`backend_runtime.rs`'s own comment explains: agreeing about nothing is not agreement, so an
all-empty corpus cannot certify `FullHcConfirmed` even when the candidate also proposes nothing.

This is a deterministic, data-driven outcome (a fixed word count against a fixed oracle answer),
not a wall-clock race -- confirmed by reading `RuntimeBudget`'s fields: the dimension that
produced this fixture's `ResourceBreach` cells elsewhere in the same run is a decoded-path COUNT
(`value: 1000001, limit: 1000000`), not a `Duration`, and `no-analyzable-words` itself depends on
nothing but the oracle's own analysis count. Re-running the measurement should reproduce the same
split.

What this note does NOT establish is which second fixture (beyond
`metathesis-comparison-crash`) accounts for the remainder of the shift against the hand-transcribed
57515281 table -- that table was not itself re-derived from a saved log, so its exact
`compile-but-miss` / `compiled, no verdict` split cannot be re-checked line by line. The gate in
`backend_scoreboard_gate.rs` therefore ratchets against the FRESH, reproducible split recorded
above (measured post-extraction, post-`expect_crash`-skip), not against the older transcription --
this file is what makes that choice inspectable rather than a silent divergence from the
implementation brief's own baseline table.

## Soundness

0 `candidate_only_identities` (surviving over-generations) across every measured cell that
produced comparable evidence in this run (115 such cells: 104 `oracle_exact` + 11
`compiles_but_misses`). `backend_scoreboard_gate.rs` asserts this as a hard invariant, never a
ratchet.

## `circumfix-non-first-allomorph-selection`

Was measured `Refused` on all three backends (0/3); now 2/3, `TunedSurfaceProbed` AND
`TemplatedUnderlyingTokens` both `OracleExact`:

- `TunedSurfaceProbed`/`TemplatedUnderlyingTokens`: both were refused by the same root cause,
  because both routes' zone construction calls the SAME shared `emit.rs::emit_rule_allomorphs` /
  `allomorph_zone_outcome`. Zone membership was assigned PER RULE (a circumfix allomorph widens
  the whole rule into both the prefix and suffix zone), then the rule's plain suffix allomorph was
  reported "uncovered" in the prefix zone it never owned -- for TSP that surfaced as the
  `surface-probe.circumfix-zone-exclusive-allomorph` capability refusal, for TUT as `templated
  emission unsupported: Partial { uncovered: 1 }`. Fixed by moving zone ownership to per-ALLOMORPH
  (`AllomorphZoneOutcome::OwnZoneElsewhere`): a plain Prefix/Suffix allomorph absent from the zone
  a circumfix sibling forced the rule into is routed by its OWN zone instead, not reported
  uncovered. Both now `OracleExact` for `mits` and `kemitan`, zero soundness violations.
- `PlanComposed`: still refused -- the plan's `StructuralCompositeMarker` subtree cannot be
  honoured by `build_controllable`; the fixture needs a whole-grammar backend, unrelated to the
  zone-ownership bug above.

This does not contradict `tests/circumfix_candidate_selection.rs::non_first_allomorph_circumfix_recall_parity`,
which proves proposer-to-confirm containment for the same grammar's `kemitan` word -- that test
builds its plan directly via `crate::emit`, a route none of `ALL_STRATEGIES`'s three backends
takes. The census C1 fix this fixture pins made the compiler correctly DETECT that the rule needs
structural-composite handling; `TunedSurfaceProbed` and `TemplatedUnderlyingTokens` now also
correctly EMIT it (see above), while `PlanComposed` still fails closed with a named reason rather
than under-proposing.

## `suffixing-extension-slot-ordering` (upstream `machine:languages/...`)

Was measured `Refused` on `TunedSurfaceProbed` (`surface-probe.finite-closure-bound`, over its
`RealizationalRule rrRRealTest`, which has no `RealizationalFeatures` element); now `OracleExact`.
Root cause: `preexpand::realizational_rule_is_semantically_unbounded` treated an empty
`RealizationalFeatures` as proof of unbounded reapplication. Reading hc.dll's
`SynthesisRealizationalAffixProcessRule.Apply` (`SynthesisRealizationalAffixProcessRule.cs:46-49`)
shows the real bound: it checks `word.GetApplicationCount(rule) >= 1` before it ever looks at the
rule's realizational feature structure, so EVERY `RealizationalRule` -- content aside -- applies at
most once per word (the DTD's own `RealizationalRule` attribute list has no `multipleApplication`
attribute, unlike `MorphologicalRule`/`CompoundingRule`). The predicate could therefore never fire
again, so it was deleted outright rather than left registered as a permanently-vacuous check:
`realizational_rule_is_semantically_unbounded`, `preexpand::unbounded_candidate_rules`,
`emit::unbounded_closure_rule_ordinals`, `emit::eager_route_refuses_unbounded_closure`, and
`capability::TunedSurfaceClosureCheck` (`surface-probe.finite-closure-bound`) are all gone, along
with the tests that existed only to exercise them.
`TemplatedUnderlyingTokens`/`PlanComposed` still refuse this fixture for unrelated reasons.

## `final-template-partial-discriminators` (staged `edge-cases/`, added 2026-09-04)

One new scored fixture (65 -> 66 per backend), so every backend's total moves by exactly one:

| backend | cell | why |
|---|---|---|
| TunedSurfaceProbed | `compiles_but_misses` (0 -> 1) | misses `daknagafa` (`DAK+NONFINTAGA+GATE1+FINTAGA`: a non-final template, then a loose rule, then a final template) |
| TemplatedUnderlyingTokens | `compiles_but_misses` (0 -> 1) | the same word, for the same reason |
| PlanComposed | `oracle_exact` (31 -> 32) | whole-fixture containment held |

Root cause of the two misses, diagnosed and deliberately not patched on the branch that added the
fixture: both the surface skeleton (`emit()`) and the templated skeleton
(`emit_underlying_templated()`) admit at most one affix-template application per word, so a
template -> loose-rule -> template derivation has no path. It is a topology limit shared by the two
lexc routes, not a partiality effect -- the fixture's two partial-dependent words (`pilfbvb`,
`nibgi`) are contained on all three backends, which is what lets the partial production policy
stay `Readiness` rather than `CannotRepresent`. The full diagnosis, the containment measurement,
and the `faithfulness_coverage_gate` ratchet raised 5 -> 6 for the same word are in
`docs/superpowers/plans/2026-09-04-reject-partial-fst-builds-results.md`. Soundness stayed at 0
`candidate_only_identities` in the run that produced these figures (2026-09-05, full `-Mode test`).

## Machine 34215889: reduplication followed by deletion (`hasaasa`)

### Baseline measurement before staging-clone synchronization (2026-09-24)

The all-scope conformance run measured TunedSurfaceProbed at 70 `oracle_exact` and 2
`compiles_but_misses` fixture cells, against the prior 71/1 ratchet. The new miss is
`machine:languages/metathesis-phase-isolation`, word `hasaasa`; the faithfulness report names the
required oracle identity as `morphemes=[8, 16]`, `root_index=1`, with multiplicity 1 and zero
proposals for it. This is a real recall/correctness gap, not a readiness or resource-containment
refusal.

The faithfulness gate measured 20 failed `(construct kind, backend)` pairs against its ceiling of
14. Six newly failing pairs all identify the same TunedSurfaceProbed miss:

| construct kind | fixture / word | required identity |
|---|---|---|
| IterativeRewrite | `machine:languages/metathesis-phase-isolation` / `hasaasa` | `[8, 16]`, root index 1 |
| LeftToRightRewrite | same | `[8, 16]`, root index 1 |
| Metathesis | same | `[8, 16]`, root index 1 |
| SubruleGating | same | `[8, 16]`, root index 1 |
| CircumfixOutputAction | same | `[8, 16]`, root index 1 |
| Reduplication | same | `[8, 16]`, root index 1 |

Source inspection points to the interaction between the committed derivation and the query-time
peeler. The word file derives `hasaasa` from a leading full copy of `HASA`, then `prHDel` deletes
the second copy's initial `h`, leaving `hasa` + `asa`. The peeler in
`rust/crates/pg-foma-runtime/src/peel.rs` only matches equal character slices and infers morpheme
order from the matched prefix or suffix position. It can see the local repeated `asa` suffix, but
that yields a suffix-copy candidate; the oracle requires the leading reduplication morpheme and
root index 1. This is a source-based explanation of the measured proposal miss, not a new backend
measurement.

The static TunedSurfaceProbed card does not promise rule-aware inverse alignment for altered copy
material. Recovering this identity requires a broader relation between copied spans and subsequent
phonology, so this pass did not attempt an emitter change.

The same work synchronized `staging:edge-cases/backend-ordered-generic` to the Machine grammar and
ordered word inputs, changing only the language name. The clone is another scored fixture cell with
the same TSP outcome. Measured after the sync: TunedSurfaceProbed 69 exact / 3 misses / 1 refusal,
and 20 failed (kind, backend) faithfulness pairs (the clone repeats the same six pairs), with
soundness still 0. Both ratchets were set to those values.
