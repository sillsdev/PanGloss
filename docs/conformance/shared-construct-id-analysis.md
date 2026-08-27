# Shared `constructs.txt` ids — why 20/20 Covered is true, and why it was not yet flippable

Written 2026-07-25, immediately before deciding whether to flip the conformance-coverage cross-check
from advisory to build-breaking (`openspec/changes/plan-construct-coverage-completion` §D7 step 7,
tasks.md 6.2 — "This is the finish line, not a follow-on cleanup step").

The cross-check reports **20 rows / 20 Covered / 0 Uncovered / 0 Unmappable**. This document is the
audit trail for that number, because part of it rests on a hand check rather than on a mechanism.

## The structural weakness

`conformance_coverage::construct_ids_for` maps each `CharacteristicKind` to `constructs.txt` row
ids, and `passing_covered_constructs` credits a construct by byte-for-byte set match against
passing fixtures' `exercises:` tags. The two vocabularies are at **different granularities** —
`constructs.txt` is the coarser, upstream one — so four row ids are each mapped by *two*
characteristics:

| Shared row id | Characteristics sharing it |
|---|---|
| `Stratum (Linear/Unordered rule order)` | `OrderedMorphRuleApplication`, `UnorderedMorphRuleApplication` |
| `RewriteRule Iterative (epenthesis/deletion/feature/expansion/merge)` | `IterativeRewrite`, `Epenthesis` |
| `AffixProcessRule: prefix/suffix/circumfix/infix` | `Affixation`, `CircumfixOutputAction` |
| `MPR features/groups` | `MprGroupAppend`, `MprGroupOverwrite` |

Set matching cannot distinguish "this fixture exercises the finer construct" from "this fixture
exercises the coarser sibling and happens to tag the same row." So the finer characteristic can
report `Covered` on **inherited** evidence.

This is the same shared-id inheritance defect that an earlier G8 implementation first exposed with
`MprGroupOverwrite`: its row could be credited solely because `MprGroupAppend` tagged the shared
`"MPR features/groups"` id. The old refusal-only terminology is retired. The current implementation
keeps the general structural-witness check instead.

## Hand verification — the claim is true today

I checked each of the four by reading the covering fixtures' grammars directly, rather than trusting
the aggregate count:

1. **`UnorderedMorphRuleApplication`** — genuine.
   `machine/conformance/languages/polysynthetic-stratal-derivation-chain/grammar.xml` declares
   `morphologicalRuleOrder="unordered"` at lines 240, 290, and 408, and `coverage.csv:138,140` tags
   that fixture's words with `Stratum (Linear/Unordered rule order)`. So an actually-unordered
   stratum is exercised by a passing fixture that tags the id.
2. **`Epenthesis`** — genuine.
   `machine/conformance/languages/suffixing-vowel-harmony/grammar.xml` contains a rewrite rule with
   an empty `PhoneticInput` (the DTD's insertion-only convention), and `coverage.csv:246,251,256`
   tags its words with the Iterative id. `unitide` vs the `expect_fail` `untda` is an insertion case
   on its face.
3. **`CircumfixOutputAction`** — genuine.
   `coverage.csv:90-93` tags `FusionalRealizationalMorphology`'s `gelobt`/`gelobth` with the
   affixation id, and those are genuine circumfixes (a morpheme wrapping the root on both sides).
   Note the same id is also tagged by plainly-suffixal words elsewhere (`coverage.csv:4-20`), which
   is exactly the coarseness that makes this check necessary.
4. **`MprGroupAppend` / `MprGroupOverwrite`** — the shared id remains, so it is covered by a
   structural witness rather than inherited from the sibling's tag. The current
   `registered_structural_witnesses` entry checks that the grammar actually declares an overwrite
   MPR group. No refusal-based evidence is involved.

**So 20/20 Covered is a true statement about the current tree.** It is not an inflated number.

## Why that was not sufficient to flip

Flipping makes CI assert "zero coverage gaps" on every build. The assertion is true today. The
problem is that for three of the four rows the assertion is **not mechanically checkable**, so it can
become false *silently*:

- delete the unordered strata from `polysynthetic-stratal-derivation-chain`, and
  `UnorderedMorphRuleApplication` keeps reporting `Covered` off `OrderedMorphRuleApplication`'s
  evidence;
- let `suffixing-vowel-harmony`'s epenthesis word start failing, and `Epenthesis` keeps reporting
  `Covered` off `IterativeRewrite`'s;
- and the circumfix case is the one where this is most likely to bite, because
  `docs/conformance/circumfix-structural-composite-census.md` already documents three real
  candidate-selection gaps in circumfix handling (C1/C3/C2). A green build-breaking gate sitting
  next to a documented census of circumfix gaps would be actively misleading.

A gate whose green light can decay into a false claim without anything failing is worse than an
advisory report, because the green light is what people will cite. That is the same reasoning
`tests/exercises_tag_liveness.rs` and `tests/coverage_citation_liveness.rs` were written from: a
string (or a count) that silently resolves to nothing is worse than a failing assertion.

## The prerequisite, and the decision

**Decision: mechanize the three still-live witnesses first, then flip.**

The gate cross-checks *grammar shape* against *tag*: for each shared id, assert that some **passing**
fixture whose grammar structurally exhibits the finer construct is among those tagging it —
unordered rule order for `UnorderedMorphRuleApplication`, an empty-LHS rewrite rule for
`Epenthesis`, a `Role::CircumfixPrefix`-classified allomorph RHS for `CircumfixOutputAction`. The
circumfix predicate must use the compiler's own `emit::classify_affix` so the gate and the compiler
cannot drift, and must scan every allomorph (the census records that `rule_role` reads only allomorph
0, and that the `Reduplication` and `Infix` tests preempt the circumfix test).

The shared-id list must be **computed** from `construct_ids_for`, not hardcoded, and a newly-shared
id with no structural witness must fail — otherwise a future mapping quietly reintroduces
inheritance, which is the whole defect.

Once that is green, the flip is honest: every shared row is evidenced by a passing fixture whose
grammar is checked to exhibit the finer construct.

## What this does not claim

- That every fixture tags the *right* constructs. A tag asserting a construct the fixture does not
  exercise is a human-authoring error that no string or shape check can catch in general; the
  structural witnesses close it only for the four shared ids, where the risk is concentrated and
  mechanizable.
- That `Covered` means `Admit`. Per §D1, `ConfirmOnly → Admit` promotion stays an explicitly
  separate, optional track. Ten of the twenty rows are `ConfigPredicate` and three are `ConfirmOnly`;
  `Covered` means "evidenced by a passing fixture at its own disposition," never "proven admissible."
- That the constructs in `docs/conformance/circumfix-structural-composite-census.md` (C1/C3),
  `needs-decision-resolutions.md` (unbounded quantifier, RTL metathesis), or
  `multitable-shared-representation-design.md` (4.4b) are closed. Those are open *configuration*
  splits inside rows that are `Covered` at the row level. Row-level coverage and
  configuration-level completeness are different questions, and §D7's definition of done requires
  both.
