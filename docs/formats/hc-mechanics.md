# HermitCrab mechanics

HermitCrab is the rule-based engine that actually parses a word against the grammar in
`grammar.json` — it is what decides whether `unhappily` breaks down into `un-` + `happy` + `-ly`
or fails to parse at all. This page explains the order it does its work in and the ways a
grammar author's intent can go wrong without an error message, so you can reason about a
particular parse result instead of treating the engine as a black box.

## Processing order

A word is analysed in three stages, each of which matters for a different reason:

1. **Strata, in order.** A grammar is organised into ordered strata — every real FieldWorks
   project runs on exactly three, hardcoded by the loader: `Morphology`, `Clitics`, `Surface`.
   FieldWorks' own grammar model has a general `MoStratum`/`Strata` mechanism that would allow
   more, but no project actually configures it, so this is a fact about what grammars *are*, not
   a simplification. Rules within a later stratum apply to the output of an earlier one.
2. **Rules within a stratum, in declared order.** Order is semantic, not cosmetic: a rule can
   feed a later one (create the environment it needs) or bleed it (destroy the environment it
   needed), and reordering two rules can change which words parse at all, not just which
   analysis wins. This is why `grammar.json`'s rule list is never reordered by anything that
   reads it — see `grammar-format.md`.
3. **Templates and slots within a rule's own morphotactics.** An affix template's slots apply in
   a fixed position order — closest-to-stem first — and each optional slot's "apply or skip"
   choice is explored **independently of every other slot in the same template**. Two affixes
   meant to always co-occur can each fire without the other unless the grammar explicitly gates
   one on the other's feature, or the two live in separate templates.

Phonological rules within a stratum divide into two kinds that see different things: an
*iterative* rule rescans its own mutated output, so it can feed itself and needs a bound to
terminate; a *simultaneous* rule applies once to every match found in the original input and
cannot see the effect of its own application. Iterative epenthesis carries a hard internal cap
(256 nodes) precisely because a self-feeding rule can otherwise never stop; iterative metathesis
has no such backstop at all, so a self-feeding metathesis rule is the more dangerous of the two
to author carelessly.

## Two different class mechanisms, and why the choice matters early

A grammar author modelling agreement or lexical conditioning has two genuinely different tools,
and picking the wrong one produces a grammar that *loads* but behaves unexpectedly:

- **MPR features** (`MprFeature`/`MprFeatureGroup`) — arbitrary lexical conditioning with no
  semantic content and no visibility to agreement: an inflection class, a conjugation class, a
  declension. A feature group's `matchType` defaults to **Any (OR)**, not All — omitting it does
  not mean "every listed feature must match," it means "any one suffices." A group's `outputType`
  defaults to **Overwrite**, not Append — a later rule's tag silently replaces an earlier rule's
  tag from the same group rather than accumulating with it, and because this is a property of the
  *group*, every rule that references it inherits whichever default was left in place.
- **Syntactic (agreement) features**, in the syntactic feature system — semantically real,
  syntactically visible categories like gender, person, number, and case that another word's
  rule can test for.

Because both defaults are properties of the group rather than of any one rule, retrofitting a
group's semantics after several rules already depend on it means re-auditing every rule that
touches the group, not just the one being added. Deciding the typological frame (what marking the
language actually has) and the class inventory before authoring rules avoids discovering this
partway through a grammar.

## An untagged stem is not "matches nothing in particular" — it is "matches no class at all"

A stem with no inflection class assigned, and no default configured anywhere up its part-of-
speech hierarchy, receives no inflection-class MPR feature whatsoever. Every rule gated on *any*
inflection class then fails for it — not "picks the wrong class," fails outright, silently,
for a reason that is easy to misdiagnose as a missing rule when it is actually a missing default.

## Natural classes and environments are string-matched, not structural

An environment (the context a rule or allomorph is restricted to) is parsed from a plain string
(`/left_right`, `#` for a word boundary, `[NC]` for a natural class, parenthesised optionality),
never from a structured context graph, even though the underlying model also stores one. An
invalid environment string does not fail to load — it silently becomes "applies everywhere,"
which is a strictly more permissive rule than the author intended, never less. A natural class
built too broadly (a "kitchen sink" class meant to save authoring effort) widens every
environment and rule that references it, not just its own definition; this tends to show up as
parsing that is slower and less selective than expected, well after the class was defined.

## Known correctness pitfalls

- A **null (zero-realization) affix** modelling a default value is still a real rule application
  that must pass every one of that rule's required-feature, MPR, and environment gates — it does
  not fall back to a default silently just because it is realized as nothing. If those gates are
  not genuinely unconditioned, the derivation dead-ends instead of producing the intended default.
- A **discontinuous morpheme (circumfix) modelled as two independent affixes** in two template
  slots gives up the engine's atomic guarantee that both halves apply together or not at all,
  and can license a form realizing only one half.
- An **unclassified affix, or an inflectional affix left with no assigned slot**, bypasses normal
  template-ordering discipline and is specifically permitted to attach in positions a normally
  classified rule would be refused.
- A **co-occurrence exclusion naming several morphemes** blocks the key morpheme only when *all*
  of the named morphemes are present together — a much weaker constraint than one pairwise
  exclusion per named morpheme, and easy to mistake for the latter.
- A **stem-name-restricted allomorph** requires the relevant feature to be *explicitly* present
  on the word, not merely compatible with (not conflicting with) the stem-name region — an
  unmarked, otherwise-compatible form still fails to match.
- **Suppletion via a lexical family's blocking mechanism** can substitute a whole family's
  irregular form for a regularly-derived one, and fires only during generation (synthesis), not
  during analysis — a trace reader looking only at the analysis side will not see it happen.
- A rule's **required stem name** is likewise enforced only in the synthesis-direction pass, even
  when the overall derivation started from analysing a word — the same asymmetry as above,
  worth remembering whenever a trace looks like it skipped a check that should have applied.

## Known performance pitfalls

- **Independently-optional template slots multiply.** Each optional slot doubles the number of
  candidate derivations explored for a word, independent of every other slot — `O(2^n)` in the
  number of optional slots on one template, before any of them are ruled out by an actual
  feature mismatch.
- **Unordered strata are worse than linear ones in both directions.** A linear (ordered) stratum
  sequence is still combinatorial for generation and can be exponential for parsing in the worst
  case; an unordered stratum sequence is factorial either way, because every stratum ordering is
  a candidate.
- **Environment-conditioned allomorphs are not short-circuited early** — the engine builds a
  candidate surface form for each environment-restricted allomorph before checking whether the
  environment actually matches, so the cost is multiplicative in the number of such allomorphs,
  not merely additive.
- **MPR and co-occurrence checks filter late.** Each individual check is cheap, but it runs only
  after a full candidate derivation already exists — these rules prune the search space after
  the expensive part of building a candidate, not before it.
- **Compounding enumerates every split point** in a word and re-derives only the head side of
  each split, so cost scales with word length times the number of candidate stems, not with the
  number of actual compound rules.
- **Root allomorphs given as a literal pattern, rather than a plain form, bypass the lexicon's
  trie lookup** and fall back to a linear scan of every pattern-shaped allomorph in the project —
  fine for a handful of such allomorphs, a measurable cost if many entries use them.

## A build-order worth following when authoring, not just reading

For anyone asking "how should I model this," rather than "why did this parse this way," the
order that avoids most of the pitfalls above is: decide the language's typological frame first
(synthesis level, affixing direction, what agreement exists at all) — this is cheap to determine
and tells you which of the mechanisms above you need at all; then design the class inventory
(declension/conjugation classes, and how they propagate through agreement) as a schema, before
any rule references it; only then author rules and their exceptions, most-specific exception
first. Modelling opportunistically — adding a class the moment one rule seems to need it, or a
natural class scoped to exactly one rule's segments — tends to produce near-duplicate MPR
features that should have been one group, and natural classes that silently diverge from each
other by one segment.
