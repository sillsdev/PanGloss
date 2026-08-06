# Orthogonal-basis Group A: methodology (`pg-foma/tests/orthogonal_basis_group_a.rs`)

Group A owns five of the eleven orthogonal-basis mechanisms, each exercised at least twice. Group B
(a disjoint file, `orthogonal_basis_group_b.rs`) owns the other six (bounded copy, unbounded peeled
copy, bounded metathesis, interdigitation, feature/POS/MPR gates, compounding).

| Mechanism | Exercise 1 | Exercise 2 | Exercise 3 |
|---|---|---|---|
| template order / co-occurrence | `machine:languages/suffixing-extension-slot-ordering` (slot ORDER, obligatory OUTERMOST, reverse order accepted) | `machine:languages/prefixal-discontinuous-slot-dependency` (DISJUNCTIVE slot, obligatory INNERMOST, reverse order refused) | `machine:languages/suffixing-evidential-adjacency-chain` (CO-OCCURRENCE) |
| cascade / strata | `machine:languages/suffixing-vowel-harmony` (rule CASCADE) | `machine:languages/polysynthetic-stratal-derivation-chain` (cross-STRATUM feed) | `machine:edge-cases/bistratal-overlapping-segment-representation` (per-stratum TABLE) |
| lexical class | `machine:languages/fusional-realizational-morphology` (allomorph-level stem-name regions) | `machine:languages/suffixing-extension-slot-ordering` (rule-level `requiredStemName`) | -- |
| allomorph priority | `machine:edge-cases/disjunctive-recheck` (earlier index BLOCKS) | `staging:edge-cases/circumfix-non-first-allomorph-selection` (later index is REACHABLE) | -- |
| zero morphology | `staging:edge-cases/optional-template-composite` (silent MANDATORY template slot) | `machine:edge-cases/subrule-morphosyntactic-gating` (zero DERIVATION, no template) | -- |

**On the source list's slashed mechanism names.** "template order/co-occurrence", "cascade/strata",
and "feature/POS/MPR gates" each name one mechanism with alternative labels. Group A is written to
the stricter reading (each half counted separately) wherever a committed fixture allowed it:
template order has two independent exercises of its own on top of the co-occurrence one, and
cascade/strata have one and two respectively. Co-occurrence has only one exercise — the only other
fixture in the inventory declaring co-occurrence rules is `machine:languages/templatic-root-modification`,
which is group B's interdigitation fixture, so claiming it here would cross the split.

## What each exercise is, structurally

Every exercise is an already-committed conformance fixture with two ends, matching
`morphotactics_boundary_cleanup_slice.rs`'s convention:

- The TOP end is a claim about the fixture's own grammar — that the mechanism is really declared in
  it. Without this end an exercise could pass while pointing at a grammar unrelated to the
  mechanism it claims to exercise.
- The BOTTOM end is what the engine actually produces for that fixture's own pinned words,
  projected through `pg_foma::parity::OccurrenceIdentities`.

Nothing here hand-derives an expected number: every count an assertion compares against is read out
of the committed `words.yaml` `parses:` rows, or computed from the grammar the fixture ships.

## Which relation each assertion uses

Conflating two of these once made an entire certification scope invisible (see `pg_foma::parity`'s
own module doc), so each is named at its use site:

- **MULTISET cardinality** — `OccurrenceIdentities::raw_analyses` against the committed `parses:`
  row count. `words.yaml` is sorted but not deduplicated, so a repeated signature is a measured
  multiplicity. Used by `assert_word_parity` for every word of every exercise.
- **SET** — `OccurrenceIdentities::len`, bounded below by the number of distinct morpheme-join
  strings the fixture records and above by the row count; pinned exactly where those bounds
  coincide.
- **The program's parity relation** — deduplicated set equality
  (`OccurrenceIdentities::same_identities`), blind to multiplicity by design. Used in exactly two
  places: `gray`/`grey` (free-fluctuating allomorphs of one morpheme must project to one identity
  set) and `mano`/`mino` (two lexical-class allomorphs of one morpheme, likewise).
- **Never** full `pg_parse::WordAnalysis` equality, which would make engine internals (`syn_fs`,
  `mpr`, dense ordinals) observable as disagreement.

Two further relations appear, named as not being parity relations: ordered morpheme SEQUENCE vs
unordered morpheme SET (the template-order exercise), and sequence-difference of size one (the
zero-morphology exercises).

## Two hard constraints this file observes

No assertion is a proposal-set ceiling or a truncation: every count is either read from a committed
record or is a lower bound stated as such. No assertion reads a clock.

## No sweep, therefore no exclusion list

This file names its eleven fixtures explicitly and discovers nothing generically, so the three
known-aborting/panicking fixtures (`machine:edge-cases/deep-optional-affix-nesting`,
`staging:edge-cases/recipe-template-generic`, `machine:edge-cases/loader-pattern-shapes`) are never
reached. `parity_divergence_census.rs`'s announced-exclusion + `catch_unwind` pattern is for
sweeping tests; a named-fixture test needs neither.

## Helper duplication is deliberate

`committed_words`/`assert_word_parity` mirror the equivalent helpers in
`morphotactics_boundary_cleanup_slice.rs`. Each integration test file is its own crate, so sharing
them would mean editing a third file; the duplication is the cheaper cost.

## Exercise inventory (`Exercise`, `EX_*` constants)

Each `Exercise` records `independent_falsifier`: prose naming the defect that ONLY this exercise
(not its sibling of the same mechanism) can detect. This is checked by a human, not the compiler —
recorded so a future edit that points two exercises at one falsifier is visible in one place.

- **`EX_TEMPLATE_SLOT_ORDER`** falsifier: an engine that stopped enforcing an obligatory template
  slot admits `andik`; one that collapsed the two extension orders into one identity loses the
  distinction between `andikishila` and `andikilisha`. No co-occurrence rule is involved.
- **`EX_TEMPLATE_DISJUNCTIVE_SLOT_AND_ENFORCED_ORDER`** falsifier: the exact converse of the
  slot-ordering fixture. There, the obligatory slot is outermost, so the stratum's fixpoint retry
  rescues an out-of-order optional pair and `andikilisha` is accepted. Here, three obligatory slots
  lie between the swappable pair and the root, so the single-pass walk dies on the first obligatory
  slot out of turn and `gahobishiyidkal` is refused. Also carries the disjunctive-slot construct the
  other declares none of. (Its `root: Root::Staging` — this fixture lives under
  `conformance-staging/edge-cases/circumfix-non-first-allomorph-selection` — was caught only by the
  deliberate panic-rather-than-skip on a missing fixture, on this file's first run.)
- **`EX_CO_OCCURRENCE_ADJACENCY`** falsifier: an engine that collapsed `adjacentToLeft` into
  `somewhereToLeft` admits `walaknichikwas`; the reverse collapse rejects `walaknichiktan`; per-morpheme
  (rather than per-allomorph) evaluation admits `kantancha`.
- **`EX_CASCADE_ORDERED_RULES`** falsifier: one stratum, so no stratum-ordering defect can reach it.
  Its falsifier is intra-stratum rule order: `kutagida` needs three phonological rules in declared
  sequence, and `unitide` needs the harmony rule's bounded transparency span to stop it while
  epenthesis still fires twice in one pass.
- **`EX_STRATA_CROSS_STRATUM_FEED`** falsifier: the derivational and inflectional rules live on
  different strata, so identity `nunaliqvuq` spans both. Failing to feed stratum N's output into
  N+1 loses it; ignoring stratum boundaries admits `nunavuq`, which must have no analysis. Every
  stratum shares one character table here, unlike the third cascade/strata exercise.
- **`EX_STRATA_PER_STRATUM_TABLE`** falsifier: the two strata declare different character-definition
  tables sharing one representation string. Merging them makes the inner-stratum roots tokenizable,
  so `basi`/`abis` stop being invalid-shape — invisible to the other two cascade/strata exercises.
- **`EX_LEXICAL_CLASS_ALLOMORPH_REGIONS`** falsifier: the class lives on the allomorph and is checked
  by unifying the stem name's regions against the word's feature structure, with two overlapping
  classes and an unrestricted default. A plain identity/inequality check flips at least one of
  twelve committed cells; forgetting the classes jointly exhaust the person space wrongly admits the
  default allomorph with a person suffix.
- **`EX_LEXICAL_CLASS_RULE_LEVEL`** falsifier: the class is read by the rule
  (`AffixProcessRule.RequiredStemName`), a different check from the allomorph-level one. Exactly one
  class with exactly one region, so the overlapping-region algebra the other exercise turns on is
  not expressible here — only presence/absence of the class label can decide anything.
- **`EX_ALLOMORPH_PRIORITY_EARLIER_BLOCKS`** falsifier: priority runs in the BLOCKING direction — a
  later-indexed allomorph is rejected when an earlier, non-free-fluctuating alternative also
  matched. Ignoring index order over-accepts (`wakta`, `pakda`); applying rejection without the
  free-fluctuation escape under-accepts (`grey`).
- **`EX_ALLOMORPH_PRIORITY_LATER_REACHABLE`** falsifier: priority runs in the REACHABILITY direction
  — the rule's second-declared allomorph is structurally a circumfix while the first is an ordinary
  suffix, so classifying/indexing by the first allomorph alone loses `kemitan` entirely (a recall
  gap, the opposite failure direction from the blocking exercise).
- **`EX_ZERO_MORPH_SILENT_TEMPLATE_SLOT`** falsifier: the zero morpheme sits in a mandatory template
  slot and is freely available, creating ambiguity (one surface, two identities differing by exactly
  its key). Template-composite pruning that treats a silent-output rule as prunable loses the second
  reading. The other zero-morphology exercise's zero rule is in no template at all.
- **`EX_ZERO_MORPH_ZERO_DERIVATION`** falsifier: this zero morpheme is disambiguating, not
  ambiguity-creating — it changes only the category, and its sole trace is a downstream
  category-gated rewrite firing. `bat` must have exactly one analysis with it, `pat` exactly one
  without: an over-generation falsifier the sibling exercise structurally cannot have, since there
  the zero morpheme genuinely is insertable everywhere.

## `template_order_exercise_slot_sequence_and_obligatory_slot` (Exercise 1: AffixTemplate slot order)

TOP end: a template with at least three slots, at least one non-optional (order is only enforceable
against an obligatory slot) and at least two optional (needed for the reverse-order finding). The
obligatory slot must be outermost — the condition under which the stratum's fixpoint retry can
rescue an out-of-order optional pair — and no slot may hold more than one rule (no disjunctive slot;
that construct belongs to Exercise 2).

BOTTOM end, four claims over the committed rows: (1) the obligatory slot is enforced — `andik` (bare
root) has no analysis; (2) the two optional slots compose independently — one filled, the other
skipped, each exactly one identity; (3) all three slots filled is exactly one identity; and (4)
order is observable in the identity — the committed record pins both `AND+CAUS+APPL+FV` and
`AND+APPL+CAUS+FV` as accepted (the oracle accepts the reverse order, since a template's slot
sequence is a hard synthesis constraint but the stratum retries its rule set to a fixpoint, so two
optional generically-shaped slots can be peeled in either order). The honest claim is not "the
reverse order is refused" — it is that the two orders are distinct identities with the same
morpheme set and different ordered sequences. The program's parity relation
(`OccurrenceIdentities::same_identities`) is asserted false for the pair, for the same reason.

## `template_order_exercise_disjunctive_slot_and_enforced_order` (Exercise 2: disjunctive slot, order enforced)

The exact converse of Exercise 1, and the pair is what makes either claim honest: Exercise 1's
obligatory slot is outermost (fixpoint retry rescues the reverse order, which is accepted); this
grammar puts three obligatory slots between the swappable pair and the root, so the single-pass
walk hits an obligatory slot out of turn and the reverse order is refused. An engine cannot satisfy
both by treating template order as unconditionally enforced or unconditionally rescuable.

TOP end: one template with >=6 slots, >=3 obligatory, >=3 optional; its last two slots optional with
an earlier obligatory slot inward (the obligatory-innermost mirror of Exercise 1's shape); and at
least one slot holding two or more rules (the disjunctive construct absent from Exercise 1).

BOTTOM end, five claims: (1) obligatory slots enforced from the inner edge — bare root and
one-slot-short word both refused; (2) minimal well-formed word and each added optional slot are one
identity each, up to the fully-loaded word; (3) the reverse order of the two outermost optional
slots is refused (the claim Exercise 1 cannot make); (4) a discontinuous dependency is a real gate —
an outer optional slot whose requirement is set two slots inward is refused when the intervening
choice went the other way, while that choice is independently well-formed alone; (5) the disjunctive
slot admits exactly one of its members per analysis, and both members are reachable (checked over
every identity in the fixture, not just one word — without the reachability half, the at-most-one
half would hold vacuously for a slot whose members were both unreachable).

## `co_occurrence_exercise_adjacency_and_granularity` (Exercise 3: morpheme/allomorph co-occurrence)

TOP end: co-occurrence constraints of both polarities (`require`/`exclude`) covering at least four
distinct adjacency kinds, plus at least one allomorph-level constraint (the second granularity).

BOTTOM end: six committed positive/negative pairs, one per constraint shape — each pair makes the
claim discriminating (without the negative half, "the constraint passed" is indistinguishable from
"never evaluated"; without the positive half, "blocked" is indistinguishable from "nothing parses").
The sharpest pair: a `somewhereToLeft` requirement is satisfied across an intervening morpheme
(`walaknichiktan`) while an `adjacentToLeft` requirement is not (`walaknichikwas`) — collapsing the
two kinds in either direction flips exactly one of those two words. The granularity pair
(`takincha`/`kantancha`) distinguishes evaluating co-occurrence per morpheme from per allomorph.

## `cascade_exercise_ordered_phonological_rule_chain` (cascade/strata Exercise 1)

TOP end: exactly one stratum (so no stratum-ordering defect can reach this exercise) with >=3
ordered phonological rules, and committed records showing a single parse driving at least three of
them (`committed_cascade_depth`, computed from `words.yaml`'s `rules:` lists, never chosen here).

BOTTOM end: three positive words each requiring a different cascade subset, and three negative
controls. The deep-cascade word needs three rules in declared sequence (the harmony rule must see
the pre-epenthesis cluster, since its transparency span elides consonants, not vowels); one word
where harmony correctly does not fire (wrong vowel class); one where it correctly does not fire
(transparency span exhausted) while epenthesis still fires twice in one pass. Without the two
non-firing words, "the cascade fired" would be indistinguishable from "always fires" — their
identity sets must be pairwise disjoint (set intersection, explicitly not the parity relation).

## `strata_exercise_cross_stratum_derivation_feed` (cascade/strata Exercise 2)

TOP end: at least two strata, and the derivational and inflectional morphemes owned by DIFFERENT
strata (`MorphemeInfo::stratum`) — the claim that makes this a strata exercise rather than a second
cascade one. Every stratum shares one character table (independence from Exercise 3).

BOTTOM end: one identity spans the two strata. `nunaliqvuq`'s sequence must contain both keys; the
intermediate word's must contain the deep one and not the shallow one; the word attempting the
shallow rule directly on the bare root must have no analysis (an engine ignoring stratum membership
admits it) — the negative control that distinguishes "strata are ordered" from "strata exist".

## `strata_exercise_per_stratum_character_table` (cascade/strata Exercise 3)

TOP end, computed independently of any fixture comment: strata reference at least two different
tables that share at least one segment representation (the overlap making `Grammar::char_tables`
non-pairwise-disjoint), and each declares at least one representation the other lacks.

BOTTOM end: surface tokenization is scoped to the last stratum's table, so a root declared only on a
non-final stratum is not tokenizable at all. The fixture pins this as invalid-shape; those words are
`expect_skip` (excluded from `committed_words`/`assert_word_parity`), so this exercise asserts them
directly through `Morpher` — the only thing keeping them from being silently dropped. Merging the
two tables would make them tokenize, the falsifier neither other cascade/strata exercise can see.

## Lexical class: what the mechanism means here

7.8's list is terse and its neighbouring entry is "feature/POS/MPR gates". The reading used: a
lexical class is a class declared on the lexical entry (or an allomorph) that partitions the lexicon
into groups taking different morphology — the mechanism under test is the partition and its
admissibility table, not a feature gate's match semantics. Both exercises use the grammar model's
`StemName` machinery, the only device whose class label is stored on the entry/allomorph itself.
They are independent because the two consumers of that label are different checks: an
allomorph-level region unification, and a rule-level presence requirement.

## `lexical_class_exercise_allomorph_level_stem_name_regions` (lexical class Exercise 1)

TOP end: at least two stem names, each with at least two regions; one entry carrying at least two
allomorphs restricted to different stem names plus at least one unrestricted (default-fallback)
allomorph — classes plus a default is what makes a priority question out of a partition question.

BOTTOM end: a committed twelve-cell admissibility table (three allomorphs × four contexts) read
straight out of `words.yaml`. The two classes overlap on one feature, so both class allomorphs are
admitted there; the default is admitted only bare, since the two classes jointly exhaust the feature
space. A plain identity/inequality class check flips at least one cell. The overlap cell carries the
sharper claim, and is the one place where the program's parity relation
(`OccurrenceIdentities::same_identities`) is the right question: the two class allomorphs are
allomorphs of ONE morpheme, so their analyses of the overlap feature must be the same identity set
even though the surface words differ — otherwise the identity relation has become
allomorph-sensitive, and "lexical class" would be indistinguishable from "two lexemes".

## `lexical_class_exercise_rule_level_required_stem_name` (lexical class Exercise 2)

TOP end: at least one affix-process rule declaring `requiredStemName`, and the lexicon partitioned
by it (at least one root allomorph carrying exactly that class, at least one carrying none — a
requirement nothing can fail is not a partition). Independence from Exercise 1 is asserted directly:
this grammar declares exactly one class with exactly one region, so the overlapping-region algebra
Exercise 1 turns on is not merely unused here but not expressible — a single region cannot overlap
another, so only presence/absence of the class label can decide the two cells below.

BOTTOM end: the two committed cells — the classed root admits the rule (one identity); the unclassed
root does not (no analysis).

## `allomorph_priority_exercise_earlier_index_blocks` (allomorph priority Exercise 1)

TOP end, three shapes the grammar must declare: a lexical entry with an environment-constrained
allomorph at an earlier index than an unconstrained "elsewhere" one (root disjunctivity); an affix
rule with the same shape across subrules (affix disjunctivity); and a lexical entry with two
allomorphs whose constraint sets are identical and empty (the free-fluctuation escape — without it
the rejection could be unconditional and untested).

BOTTOM end, five committed pairs/controls: root disjunctivity — the later-indexed allomorph is
rejected where the earlier one would also have matched, and accepted where the earlier one's own
environment fails; affix disjunctivity, the same one level up; free fluctuation — the later-indexed
allomorph is NOT rejected, since the two allomorphs' constraint sets are identical. The
free-fluctuation pair (`gray`/`grey`) also carries a parity-relation claim: two free variants of one
morpheme must project to equal identity sets, since the relation is specified to be allomorph-blind.

## `allomorph_priority_exercise_later_index_is_reachable` (allomorph priority Exercise 2)

The opposite failure direction from Exercise 1: there the danger is over-acceptance (index priority
ignored); here it is under-generation (only the first index ever consulted).

TOP end: one affix rule with at least two allomorphs whose output shapes differ in kind — the first
inserting material on one side of the copied stem only, a later one inserting on both sides
(computed from the rule's own output actions, since a rule classified by its first allomorph alone
never gets routed through the machinery the second one needs).

BOTTOM end: three committed words, all with exactly one identity — bare root, first-allomorph
derivation, and the later-allomorph derivation (`kemitan`, the load-bearing row; the fixture's own
record notes it was once unreachable from the proposer for exactly this reason). A second claim
makes it a priority claim rather than merely a reachability one: the two derivations are the same
morpheme in different positions — equal unordered morpheme sets, different ordered sequences,
different `root_index` values (one allomorph puts the affix after the root, the other wraps it). The
program's parity relation is asserted false for the pair: they are two analyses, not one.

## `zero_morphology_exercise_silent_mandatory_template_slot` (zero morphology Exercise 1)

TOP end: exactly one rule in the grammar is zero-exponence by `zero_exponence_rules`'s structural
definition (every subrule takes exactly one input part and its whole output is a single
`CopyFromInput` of that part — excluding both an inserting rule and a truncating one, since
truncation copies one of several input parts and drops the others; subtractive morphology is not
zero morphology), it is the one the committed signatures name, and it sits in a non-optional
template slot. Asserting uniqueness matters: a second zero rule would make the word-level counts
below stop meaning what this exercise says.

BOTTOM end: the zero morpheme creates ambiguity — one surface has two identities whose sequences
differ by exactly the zero morpheme's key, in one direction. Two further claims: every doubled
word's extra key is the SAME key (one zero morpheme, not an assortment); and no identity anywhere in
the fixture consists of the zero morpheme alone (it is a morpheme, not a root). Relation: sequence
set-difference of size exactly one — not the parity relation, which would only say the readings
differ.

## `zero_morphology_exercise_zero_derivation_changes_only_category` (zero morphology Exercise 2)

The opposite direction from Exercise 1: there the zero morpheme is freely available and creates
ambiguity; here it is forced and removes it, carrying an over-generation falsifier the other
structurally cannot (a spuriously insertable zero morpheme would give the un-derived word a second
analysis, invisible to Exercise 1's grammar since there the zero morpheme genuinely is insertable
everywhere).

TOP end: exactly one zero-exponence rule, named by the committed signatures; it is NOT a template
rule (`AffixProcessRuleDef::is_template_rule`), the structural fact that keeps it independent of the
template-composite-pruning defect that falsifies Exercise 1; its output feature structure must
differ from its requirement, since a category change is the rule's only effect.

BOTTOM end: two committed words of the same surface length. The un-derived word has one identity of
sequence length one, not containing the zero key (the over-generation falsifier); the derived word
has one identity of sequence length two, containing it (the under-generation falsifier) — its
surface differs only by a segment the category-gated rewrite produced, so the zero morpheme's
presence is inferable only from that downstream consequence. The equal surface length of the two
words is the cheapest direct statement that the zero morpheme contributed no segment.

## `group_a_basis_has_two_independent_exercises_per_mechanism` (the basis shape guard)

The per-exercise tests above each assert their own exercise; none of them would catch a future edit
that pointed two exercises of one mechanism at the same fixture, dropped a mechanism, or dropped an
exercise. So this asserts the shape itself: all five mechanisms appear; each has at least two
exercises; no two exercises of the same mechanism share a fixture; every exercise records a
non-empty independence rationale; and every named fixture really exists and really loads.

A fixture MAY appear under two different mechanisms — 7.8's own list says a language may compose any
number of mechanisms, and one of group A's does (a slot-ordering grammar that also declares a
rule-level lexical class). What is forbidden is the same fixture twice within one mechanism.
