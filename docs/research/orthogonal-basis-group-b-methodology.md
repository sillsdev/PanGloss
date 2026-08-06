# Orthogonal-basis Group B: methodology (`pg-foma/tests/orthogonal_basis_group_b.rs`)

Group B exercises six of the eleven orthogonal-basis mechanisms, at least twice each where an
honest second exercise exists. Group A (a disjoint file) owns template order/co-occurrence,
cascade/strata, lexical class, allomorph priority, and zero morphology.

| Mechanism | Exercise 1 | Exercise 2 | Second exercise honest? |
|---|---|---|---|
| Compounding | `staging:edge-cases/compounding-non-recursive` | `staging:edge-cases/recursive-endocentric-compounding` | yes |
| Interdigitation | `staging:edge-cases/infix-interdigitation` | `machine:languages/templatic-root-modification` | yes |
| Bounded metathesis | `staging:edge-cases/right-to-left-metathesis-reversal` | `staging:edge-cases/multi-table-metathesis-shared-representation` | yes |
| Feature/POS/MPR gates | `machine:edge-cases/mpr-gated-exception` | `machine:edge-cases/subrule-morphosyntactic-gating` | partly — see "The gate pair is only half-independent" |
| Unbounded peeled copy | `machine:languages/suffixing-extension-slot-ordering` | `machine:languages/metathesis-phase-isolation` | yes |
| Bounded copy | `machine:languages/metathesis-phase-isolation` (word level) | the same fixture at the model level | no second fixture exists — see "Bounded copy has exactly one fixture" |

Every exercise is an already-committed conformance fixture, and every expected number an
assertion compares against is read out of that fixture's own committed `words.yaml`. Nothing here
hand-derives a signature, an analysis count, or a multiplicity, so this gate cannot certify a claim
its own author invented — the same discipline `morphotactics_boundary_cleanup_slice.rs` follows.

## Which relation each assertion uses

Naming this matters because a relation chosen for convenience is how the v1 certification scope
was once made invisible (see `pg_foma::parity`'s own module doc and its fix). Three distinct
relations appear in this file:

- **Deduplicated `pg_foma::parity::OccurrenceIdentities` SET cardinality**
  (`OccurrenceIdentities::len`) — the program's own parity relation, counted; multiplicity is
  deliberately not part of it. Every per-word claim about "how many distinct analyses exist" uses
  this relation, bounded above and below by the committed record in `assert_word_parity` and
  pinned exactly in `assert_identities_and_multiplicity`.
- **MULTISET cardinality** (`OccurrenceIdentities::raw_analyses`) against the committed `parses:`
  row count. `words.yaml` is documented as sorted-but-not-deduped (`pg_parse::result_signature`,
  `WordEntry::expected_signature`), so a repeated signature there is a real, measured multiplicity,
  not a formatting artifact. Asserted for every committed word by `assert_word_parity`, and again
  per witness word by `assert_identities_and_multiplicity` — so "one identity found twice" can
  never pass as "two identities".
- **The committed SIGNATURE relation** (`pg_conformance_fixtures::assert_matches_oracle`) — status
  plus sorted-joined signature string, the shared ground truth every conformance fixture already
  goes through. Used as the anchor each exercise opens with, so the identity work below is a
  refinement of existing ground truth rather than a second, independently-drifting one.

Two relations are deliberately not used, and naming them is part of naming the ones that are.
`OccurrenceIdentities::same_identities` (set equality between two occurrence sets) has no use here:
every exercise compares one engine run against a committed record, never two engine runs against
each other. And full `pg_parse::WordAnalysis` equality would make engine internals (`syn_fs`,
`mpr`, dense ordinals) observable as disagreement — a too-strong relation is the mirror image of
the too-weak one that hid the v1 scope.

Where an exercise needs two witnesses to be genuinely two, it compares morpheme sequences
(`morpheme_sequences`, the `morphemes` field of each identity) and requires the two sets to be
disjoint. Identity counts alone would not do it: two surfaces could each carry one analysis and
still be the same analysis, making a pair of witnesses one witness with two names.

## The bounded/unbounded copy distinction is structural, not asserted

Rather than leave "bounded copy" vs. "unbounded peeled copy" as an unfalsifiable label, this file
pins it to a property of the loaded grammar that `copy_width_bound` computes:

- A **bounded copy** copies an LHS part whose pattern has a finite width bound — e.g.
  `metathesis-phase-isolation`'s `mrRedupCV`, whose copied part `rcCV` is exactly two
  `SimpleContext` nodes. A finite-width copy is a finite relation over the alphabet, so a composed
  FST can express it outright.
- An **unbounded copy** copies a part with no finite width bound — an
  `<OptionalSegmentSequence min="1" max="-1">`, i.e. a `PatternNode::Quantifier` whose `max` is
  `None`. `{ww : w in Sigma*}` is not a regular language, so no finite-state relation expresses it
  and the surface must be peeled at query time (`pg_foma::peel::ReduplicationPeeler`) instead —
  which is why the peel path exists at all, and why it (and only it) carries a chain-depth budget.

`the_bounded_unbounded_copy_line_is_a_property_of_the_grammar` asserts this distinction is a real,
computed fact: `mrRedupCV`'s copied part must have a finite bound and `mrRedupFull`'s must not.

## A budget refusal must never read as a recall failure

The peel is the one mechanism here with a chain-depth budget, and a refused peel yields a
truncated candidate set. Two rules follow, both observed:

1. Every recall claim in this file is made under a budget whose `chain_depth_cap` is `None`
   (unbounded), built by `unbounded_budget`. No assertion about which analyses exist can therefore
   be a disguised budget refusal.
2. The refusal is a typed `Err`, never `Ok(vec![])`. `peel_candidates` returns
   `Result<Vec<Candidate>, ComposeError>`, so "I was refused" and "I looked and found nothing" are
   different values, and this file never collapses them: `peel_residuals_offered` returns `Result`
   and every caller matches on it. `the_smallest_chain_depth_cap_never_refuses_a_single_layer_copy`
   pins the other direction — a single-layer unbounded copy, which the peel genuinely supports,
   must not be refused even by the smallest configurable cap. A failure there is "the budget
   refuses a supported construct", a finding about the budget, never reported as a recall failure.

No assertion in this file is a proposal-set ceiling, no proposal set is truncated, and no assertion
reads a clock: wall time is never an eligibility or certification input here.

## Bounded copy has exactly one fixture, and that is a finding

A census of every `grammar.xml` under both fixture roots for a doubled `CopyFromInput` of one part
(the shape `pg_foma::emit::classify_affix` reads as `Role::Reduplication`) finds five rules in four
distinct grammars. Four of the five copy an unbounded part (`metathesis-phase-isolation`'s
`mrRedupFull`, `suffixing-extension-slot-ordering`'s `mrRedup`,
`deletion-reduplication-exception-composite`'s `mrRedupFull`,
`circumfix-reduplication-precedence`'s `mrCircRedup`). Exactly one copies a fixed-width part:
`metathesis-phase-isolation`'s `mrRedupCV`. Its apparent second home,
`staging:edge-cases/recipe-ordered-generic`, is a byte-identical clone of that upstream grammar
differing only in `<Language><Name>` — pinned by
`clone_fixtures_are_pinned_as_clones_not_independent_exercises` precisely so nobody pairs the two
and reports one exercise as two.

So there is no second bounded-copy fixture in the corpus, and authoring one is out of scope here: a
new fixture's `words.yaml` must be transcribed from a real oracle run
(`.claude/skills/conformance-grammars/SKILL.md`, and every existing fixture's own header), and a
hand-derived expectation would pin this file's arithmetic instead of the grammar — exactly what the
discipline above exists to prevent. What is offered instead is two exercises of the one fixture at
two different layers, with genuinely independent falsifiers:
`bounded_copy_exercise_fixed_width_reduplicant_recalls_exactly_one_reading` (word level,
oracle-anchored: a synthesis/analysis defect in the fixed-width copy fails it while the model stays
intact) and `the_bounded_unbounded_copy_line_is_a_property_of_the_grammar` (model level: a loader
change that collapsed `OptionalSegmentSequence`'s `min`/`max` fails it while both words could still
parse). That is weaker than two fixtures and is reported as weaker.

## The gate pair is only half-independent

`mpr-gated-exception` and `subrule-morphosyntactic-gating` are independent in the MPR direction,
and that half is asserted, not argued: `subrule-morphosyntactic-gating` declares no MPR features at
all (`gate_exercise_pos_requirement_on_a_rewrite_subrule` checks `Grammar::mpr_names.is_empty()`),
so no regression in `excluded_mpr` handling can be detected there and `mpr-gated-exception` is the
only witness for it.

The POS direction is weaker, and saying so is the point. `mpr-gated-exception` also carries
subrule-level `requiredPartsOfSpeech="posNasal"` on three of its rewrite subrules, so the claim
"only exercise 2 can fail on subrule POS gating" is false as stated. What is genuinely unique to
exercise 2 is the contrastive witness: `pat` and `bat` present the identical phonological
environment ("p" before "a") and differ only in derivation state, so the gate's licensing is the
only thing that can explain the difference. `mpr-gated-exception` has no such contrastive pair —
its `posNasal` subrules simply have no site to fire at in the non-`posNasal` words — so a
regression that ignored `required_pos` would plausibly leave it green. "Plausibly" is as far as
this goes: it is an argument, not a measurement, and it is not asserted anywhere in the file.

## Fixtures deliberately not visited

This file names its fixtures and never sweeps, so it needs no exclusion list. For the record,
three fixtures are known-bad at this base and would have had to be excluded by an announced skip
(the pattern `parity_divergence_census.rs::ABORTING_FIXTURES` establishes) had it swept:
`machine:edge-cases/deep-optional-affix-nesting` and `staging:edge-cases/recipe-template-generic`
abort the whole test process (unbounded recursion in `evaluate_plans`/apply), and
`machine:edge-cases/loader-pattern-shapes` panics at `replace.rs:498` ("char table too large for
the PUA token scheme"). None of them carries any of this file's six mechanisms.

Every fixture here is a synthetic construct-shaped probe; no identifier in this file names a
language, and each fixture is referred to by what it composes.
