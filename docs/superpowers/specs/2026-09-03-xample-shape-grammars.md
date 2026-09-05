# XAMPLE-authored FieldWorks projects on HC-Rust

Status: **ACCEPTED, REVISED 2026-09-04**. This revision supersedes the 2026-09-03
"XAMPLE profile" contract. The durable rationale and engine comparison live in
`docs/research/xample-grammars-on-hc-rust.md`.

## 1. Contract

PanGloss accepts a FieldWorks project (`.fwdata` or `.fwbackup`) whose imported
`ActiveParser` selects XAMPLE (including FieldWorks' XAMPLE default when that field is absent),
converts every representable authored construct without semantic loss, and runs the result
under ordinary HC semantics in HC-Rust and the FST-propose + HC-confirm backends.

The objective is **not** to make HC reproduce XAMPLE's bounded search, early stopping, exporter
workarounds, or exact returned-analysis set. XAMPLE remains a comparison engine used to measure and
explain migration differences.

Standalone AMPLE/XAMPLE `.ad`/`.dic` input is out of scope.

## 2. Definitions

- **Project**: the FieldWorks LCM model. This is the shared source of truth; XAMPLE and HC consume
  different projections of it.
- **XAMPLE-authored**: a project whose `MoMorphData.ParserParameters/ActiveParser` selects XAMPLE,
  or is absent and therefore resolves to XAMPLE by FieldWorks' model-layer default. Malformed or
  unknown values are invalid source data, not evidence of XAMPLE authorship. Historical parser
  choices that are no longer present in the project cannot be inferred.
- **Substrate**: the character-definition table and explicit segment classes HC needs to segment
  forms and interpret environments. The substrate is not a set of phonological rules.
- **Substrate completion**: adding only the missing segment/boundary definitions needed to represent
  authored strings. Added segments are featureless. Completion never invents a phonological
  feature, feature value, feature class, environment, allomorph, or rule.
- **Lossless conversion**: every authored construct is either represented with the same meaning or
  the grammar is refused with a typed explanation. Warning-and-drop is not lossless conversion.
- **Legacy comparison**: running the real XAMPLE engine over an equivalent projection to measure
  acceptance and analysis differences. Comparison results do not select HC runtime semantics.

## 3. Semantic policy

1. **One HC compiler path.** There is no grammar-level `ParserProfile::XAmple` and no XAMPLE mode in
   `Grammar`, `Morpher`, or an FST backend. `ActiveParser` is provenance and may choose an import
   default; it does not create a second runtime language.
2. **Authored rules remain authored semantics.** PanGloss compiles active phonological rules,
   metathesis rules, affix-process rules, templates, and other HC constructs when it supports them.
   XAMPLE's historical choice to ignore a construct is reported as a migration difference, not
   reproduced by silently deleting that construct.
3. **Zero rules is ordinary HC.** A project with no phonological rules uses an identity phonological
   cascade. Morphology, environments, allomorph disjunction, co-occurrence constraints, templates,
   and feature unification continue to operate.
4. **Normal HC defaults apply.** Compounding, strata, clitic placement, and search controls follow
   HC semantics unless the project contains an authored HC setting that changes them. XAMPLE
   exporter policy is not reimplemented in the core grammar.
5. **XAMPLE caps are metadata, not grammatical validity.** `MaxNulls`, `MaxPrefixes`, `MaxInfixes`,
   `MaxSuffixes`, `MaxInterfixes`, `MaxRoots`, and `MaxAnalysesToReturn` remain available for reports
   and comparison tooling. They are not attached to `Grammar` and do not reject HC analyses.

The policy classifications are:

| Concern | Classification | Consequence |
|---|---|---|
| Missing character definitions that make authored forms disappear | correctness / representability | complete the substrate or refuse |
| Unsupported or unresolved authored constructs | correctness / representability | refuse the grammar; never drop them |
| Typed reports, provenance, and differential gates | production readiness | required before broad use |
| XAMPLE affix/root/analysis limits | resource containment | do not reinterpret as linguistic validity |
| XAMPLE/HC result-set differences after lossless conversion | migration compatibility | measure and explain; do not hide |

## 4. Substrate completion

Substrate completion is owned by `pg_grammar::compile`, at the seam after the authored phonological
feature system and initial character definitions are built but before natural classes,
environments, allomorphs, or rules are compiled. HC-Rust and every FST backend therefore consume
one completed table.

### 4.1 Activation

`SubstratePolicy` has two semantic choices:

- `Strict`: every used text element must already be represented by an authored phoneme or boundary.
- `CompleteFromUsage`: complete the table from authored forms and literal environment content.

`Auto` resolves to `CompleteFromUsage` for an XAMPLE-authored project or when
`AcceptUnspecifiedGraphemes` is true; it resolves to `Strict` for an explicitly HC-authored project
otherwise. A caller may override `Auto` in either direction. This is an import policy, not a parser
profile.

### 4.2 Completion algorithm

1. Begin with authored phoneme representations, boundary representations, and HC's required
   synthetic null, word, and morpheme boundaries.
2. Collect the actual surface strings from roots, affixes, circumfix pieces, clitics, variants,
   irregular forms, and every other supported allomorph form.
3. Ask the environment parser—the owner of environment syntax—for its literal text elements. Do
   not reparse environment strings in the substrate module.
4. Normalize exactly as the existing character-table segmenter does. On a segmentation failure,
   add the offending Unicode text element and retry. This mirrors FieldWorks
   `HCLoader.AcceptUnspecifiedGraphemes`.
5. Classify an inferred element as a segment only when the selected vernacular writing-system main
   exemplar set says it is word-forming. Classify it as a boundary only when an authored boundary
   record or an explicitly versioned PanGloss safe-boundary table identifies that exact text
   element. The initial table commits the scalar values from a pinned Unicode space-separator set
   plus ASCII tab/CR/LF; it does not classify from Unicode categories at runtime;
   apostrophes, hyphens, joiners, and punctuation are never inferred as boundaries. Without that
   evidence, a material segment/boundary choice is ambiguous and causes refusal.
6. Added segments have no authored phonological feature values. HC's ordinary semantics for
   unspecified feature lanes applies. When a project also has active feature-based rules/classes,
   the report calls out that combination; the compiler neither invents values nor pretends that
   “featureless” means “known not to be in any feature class.”
7. Build explicit segment-list natural classes from their authored members. Never synthesize a
   feature class or infer class membership from a name such as `Vowel`.

The compiler returns a `SubstrateReport` containing every inferred segment and boundary, its
evidence/provenance, and any unresolved or ambiguous use. Successful inference is visible but is
not itself a warning that the grammar changed: it supplies representation, not new linguistic
behavior.

## 5. Refusal policy

A conversion that cannot preserve the authored grammar must fail as a whole. In particular, it
must refuse when any of the following remains after substrate completion:

1. an allomorph, environment literal, template form, or rule form cannot be segmented;
2. two authored character definitions collide in a way that makes segmentation or feature identity
   ambiguous;
3. segment-versus-boundary classification is material and no authoritative classification exists;
4. a referenced phoneme, boundary, natural class, feature, feature value, MSA, slot, template,
   allomorph, or rule cannot be resolved and omitting it could change accepted analyses;
5. an environment cannot be represented without dropping or widening a restriction;
6. the snapshot contains an active construct PanGloss does not yet implement, including a supported
   XAMPLE construct such as bracket-pattern reduplication until that support lands;
7. an exporter/importer ordering ambiguity affects allomorph disjunction or another ordered
   decision and has not been resolved from its owner or an oracle fixture;
8. authored phonological rules contain a segment, class, feature, value, constraint, or boundary
   reference that cannot be resolved and substrate completion cannot legitimately invent.

Refusal is typed and actionable. Each issue records a stable code, the source construct GUID/path,
the representability category, and a human explanation. Collect independent issues where possible
so the user is not forced through one-error-at-a-time repair.

The following are forbidden success paths:

- warning that an allomorph or entry was skipped;
- treating an invalid environment as unrestricted;
- silently dropping a phonological or morphological rule;
- inventing feature values or natural-class semantics;
- reporting a control as active when it could not affect compilation.

## 6. Expected migration differences

After a lossless conversion, HC may still produce different results from XAMPLE:

- HC executes authored phonological and HC-only affix-process rules that XAMPLE ignored.
- HC uses native ordered allomorph disjunction; XAMPLE's FieldWorks export synthesizes negative
  environments to approximate that decision.
- HC executes templates and feature structures directly; XAMPLE encodes them as order classes and
  PC-PATR productions.
- HC uses its own strata, clitic, compounding, and search defaults.
- HC does not apply XAMPLE's legacy analysis caps or reproduce XAMPLE's early-stop survivor set.

These changes are not substrate synthesis and must not be hidden under that name. The conversion
report identifies which categories are present in a project so users know which differences could
be observable.

## 7. Diagnostics

The compile result contains structured notices and, on failure, structured refusal issues.
At minimum, stable codes cover:

- `substrate.segment-inferred`
- `substrate.boundary-inferred`
- `substrate.classification-ambiguous`
- `conversion.unsegmentable-form`
- `conversion.unresolved-reference`
- `conversion.environment-loss`
- `conversion.unsupported-construct`
- `migration.xample-cap-ignored`
- `migration.xample-ignored-authored-rule`
- `migration.engine-default-difference`

CLI output groups inferred substrate, possible migration differences, and fatal refusals separately.
It never describes normal HC execution as an "XAMPLE profile."

### 7.1 Conformance ownership and phoneme counterfactuals

Executable comparison machinery belongs in PanGloss. The canonical synthetic FieldWorks witness
for an eligible Machine conformance fixture belongs beside that fixture under
`machine/conformance/{group}/{fixture}/fieldworks/`: `project.fwdata` and
`phonology-mutations.yaml`. Machine therefore owns the cross-engine test data while acquiring no
dependency on PanGloss, LibLCM, FieldWorks, or `xample64.dll`; PanGloss depends one way on the
pinned Machine conformance corpus and supplies the adapters that execute it.

Each fixture stores one canonical `.fwdata`, never a full/partial/empty family of project copies.
`phonology-mutations.yaml` records named counterfactual operations. The PanGloss runner verifies
the canonical source digest, copies the whole FieldWorks project to a fresh temporary directory,
opens it through LibLCM, applies the requested deletion, saves and reopens it, and gives that exact
mutated project to the official HCLoader/XAMPLE projection path and to PanGloss. Generated XAMPLE
files, generated HC XML, and mutated `.fwdata` files are temporary evidence, not authored fixtures.

A specific phoneme is selected by stable LCM GUID and carries its expected default-vernacular
representations as a stale-target assertion. V1 also permits a typed `remove_all_phonemes`
operation. It does not permit representation-only selection, free-form queries, or fixture-local
scripts. Every operation is fail-closed: a source-digest mismatch, missing or duplicate target,
representation mismatch, retained semantic reference, zero deletions, save/reopen failure, or
projection failure makes the case fail and names the control that could not act.

The invariant cases remove only phonemes that are not referenced by a retained segment natural
class, simple phoneme context, feature structure, constraint, or phonological rule. They assert
that the official XAMPLE text projection and exact normalized analysis multiset are unchanged,
while HC substrate completion reports exactly the removed graphemes as featureless inferred
segments. Removing a semantically referenced phoneme is not an invariance case: the mutation
preflight refuses it as `mutation.referenced-phoneme`. An empty-inventory case is admitted only
when the same preflight proves that every removed phoneme is semantically unreferenced.

The baseline project, not a mutation, must reproduce the checked-in HC `grammar.xml` semantically
and reproduce its existing word analyses. Raw HC XML after a deletion is expected to differ in its
character-definition table; equality after mutation is asserted over completed grammar behavior,
not over those raw bytes.

## 8. Measurement and gates

Build the differential measurement before changing compilation behavior:

1. **End-to-end loss inventory, both directions.** Begin at the FieldWorks graph, before snapshot
   extraction can omit anything. Each extractor/compiler owner publishes the identities it selected,
   represented, synthesized, or rejected; no central census re-derives owner conditions. Carry the
   import report and inventory through serialized snapshots. Cover entries, allomorphs, affix
   processes, environments, classes, co-occurrence constraints, templates, and phonological and
   compound rules. Report both omitted authored constructs and synthesized compiled constructs.
   Stage each count as a `NoMoreThan` ratchet.
2. **Inert-control gate.** A project using `AcceptUnspecifiedGraphemes` and an otherwise
   unrepresented grapheme must either compile that grapheme or fail with a typed explanation that
   the control could not act.
3. **Lossless refusal gates.** Each former warning-and-drop path gets a test proving the entire
   conversion refuses and names the construct.
4. **Substrate equivalence.** Adding a featureless segment must not change results for words that
   do not contain it. Tests prove that no explicit feature value or class membership was invented;
   ordinary HC unspecified-feature behavior remains unchanged.
5. **Backend agreement.** Direct HC and every FST-propose + HC-confirm backend use the same completed
   grammar and agree on accepted analyses.
6. **XAMPLE differential.** Before substrate/refusal behavior changes, run the real FieldWorks
   XAMPLE transformer and engine and ordinary HC from the same `.fwdata`/`.fwbackup` fixture.
   Record both `XAMPLE_ONLY` and `HC_ONLY` results. Require a non-empty successfully completed
   comparison (`engine_error == None`); a missing projection, capped result, or comparison that
   exercised no grammar is not agreement.
7. **Open semantic fixtures.** Resolve allomorph source order and default-compounding exporter
   behavior with focused XAMPLE fixtures before changing those owner modules.

## 9. Non-goals

- Emulating XAMPLE analysis caps in HC validity.
- Dropping HC rules because `ActiveParser` once named XAMPLE.
- Recreating GAFAWS, PC-PATR, or negative-SEC exporter machinery inside HC.
- Returning the same arbitrary subset when XAMPLE stops early at `MaxAnalysesToReturn`.
- Guessing through malformed, ambiguous, unsupported, or semantically lossy input.
- Changing FST capability thresholds or refusal policy; this work prepares one grammar consumed by
  all backends.

## 10. Accepted decisions

1. FieldWorks project input only; no standalone AMPLE/XAMPLE file importer.
2. The target semantics are normal HC, not exact XAMPLE emulation.
3. The only inferred linguistic substrate is character identity and unambiguous
   segment-versus-boundary role; inferred segments are featureless.
4. Authored rules are compiled, not dropped. Missing information needed by a rule causes refusal.
5. XAMPLE caps remain provenance/comparison data and are not placed on `Grammar`.
6. Any semantic loss, unsupported active construct, material ambiguity, or unresolved restriction
   refuses the grammar.
7. XAMPLE is a differential measurement engine, not the oracle that defines HC behavior.
8. Machine owns canonical cross-engine fixture data; PanGloss owns the executable projector,
   mutator, XAMPLE adapter, and migration gates.
9. Phoneme variants are temporary materializations from one checked-in project and a declarative,
   fail-closed mutation manifest; no mutated project copy is checked in.
