# Converting XAMPLE-authored FieldWorks projects to HC-Rust

Status: **LONG-LIVED RESEARCH AND DECISION RECORD**

Last substantive revision: 2026-09-04. Sections marked VERIFIED cite inspected FieldWorks,
Machine, AMPLE/XAMPLE, or PanGloss sources. Sections marked DECIDED state PanGloss policy.
Unresolved questions are kept explicitly rather than converted into implementation assumptions.

Related documents:

- Primary-source inventory: `docs/research/xample-primary-sources.md`
- Accepted contract: `docs/superpowers/specs/2026-09-03-xample-shape-grammars.md`
- Implementation plan: `docs/superpowers/plans/2026-09-04-xample-projects-on-hc.md`

## 1. Executive answer — VERIFIED and DECIDED

An “XAMPLE grammar” in FieldWorks is not a second authored grammar. FieldWorks generates XAMPLE's
control, dictionary, and PC-PATR files and HermitCrab's object model from the same LCM project
(`M3ToXAmpleTransformer.cs:199-216`; `HCLoader.cs:164-357`). The two engines receive different
projections of that project.

For the morphology XAMPLE normally consumes, HC already has the deeper representation: listed
allomorphs, string environments, segment-list classes, morpheme and allomorph co-occurrence,
templates/order restrictions, MSAs, syntactic features, clitics, and compounding. HC is legal with
zero phonological rules: its phonological cascade is then the identity, while the rest of analysis
and synthesis continues (`SynthesisStratumRule.cs:42-44`; `AnalysisStratumRule.cs:24-26`;
`Morpher.cs:162-220, 634-676`).

The principal conversion problem is not missing phonological rules. It is that HC requires a
**segmental substrate** able to segment every authored surface form and interpret each environment,
while a project used only with XAMPLE had no reason to maintain a complete FieldWorks phoneme set.
PanGloss currently builds that table only from authored phonemes and boundaries and skips forms it
cannot segment (`pg-grammar/src/compile/chardef.rs:23-80`;
`pg-grammar/src/compile/lexicon.rs:245-301`).

The accepted conversion policy is therefore:

> Complete only the missing character substrate, compile all authored constructs through the
> ordinary HC path, and refuse the whole grammar whenever conversion would otherwise guess,
> discard, weaken, or misrepresent authored semantics.

Exact reproduction of XAMPLE's bounded search and returned-analysis set is not the target. The real
XAMPLE engine remains valuable as a differential measurement tool.

## 2. “The same grammar” — VERIFIED

The shared object is the FieldWorks project, not an engine-specific grammar file:

```text
                         M3ToXAmpleTransformer
                        ┌──────────────────────→ adctl + lex + gram → XAMPLE
FieldWorks LCM project ─┤
                        └──────────────────────→ HC Language/Grammar → HermitCrab
                                  HCLoader
```

FLEx's XAMPLE path emits at least `adctl.txt`, `lex.txt`, and `gram.txt`; its managed wrapper also
loads the fixed dictionary code table (`XAmpleDLLWrapper.LoadFiles`). The HC path directly creates
feature systems, character definitions, natural classes, strata, entries, templates, and rules.

Consequently, “run the same grammar on both engines” means:

1. start from the same LCM objects;
2. preserve the meaning of every object each target claims to represent;
3. acknowledge that exporter defaults and engine search policies can still produce different
   result sets.

It does not mean feeding the XAMPLE text files to HC or copying XAMPLE's exporter workarounds into
HC's native model.

## 3. Point-by-point engine comparison — VERIFIED unless noted

| Concern | XAMPLE projection/runtime | HC projection/runtime | Conversion consequence |
|---|---|---|---|
| Character substrate | Uses surface strings and the AMPLE code table; FieldWorks phonological features are not its executable phonology. | Requires a `CharacterDefinitionTable` that segments every form. | Complete missing character definitions before compiling the lexicon. |
| Phonological rules | FieldWorks' XAMPLE transforms do not emit `PhRegularRule` or `PhMetathesisRule`. | Loads and executes active rules during analysis and synthesis. | Normal HC conversion keeps authored rules. Their previous absence from XAMPLE is a reported migration difference. |
| Phonological features | XAMPLE `\scl` classes are lists of grapheme strings; there is no rewrite-rule feature matrix. | Segments and classes may carry phonological features used by rules. | Inferred segments are featureless. Never invent features or feature classes. |
| Listed allomorphs | Emits each listed allomorph with its surface constraints. | Loads each listed allomorph as a structured alternative. | Directly representable when its form and references are valid. |
| String environments | Emits `/`, `~/`, class, optionality, and boundary syntax over surface strings. | Compiles required/excluded structured patterns and checks the final synthesized shape. | With no rewrite rules, both see the concatenation of selected allomorphs. With rules, HC intentionally sees the HC-derived surface. |
| Allomorph disjunction | The exporter manufactures negative SECs from earlier siblings to emulate ordered choice. | `Allomorph.IsWordValid` implements ordered/disjunctive choice natively. | Preserve owner-defined source order; do not emit NegSECs inside HC. |
| Co-occurrence | Emits `\mcc` and `\ancc` tests. | Loads and evaluates morpheme/allomorph co-occurrence rules directly. | Same intended constraint; encoding differs. |
| Templates and order | GAFAWS/order-class calculations and PC-PATR productions encode slot restrictions. | Executes explicit affix templates and slots. | Use HC's model. Differential fixtures detect transformer/importer mismatches. |
| Word grammar | PC-PATR validates candidates using generated productions and feature unification. | Native syntactic feature structures constrain entries, rules, and completed words. | Equivalent in intent, not presumed byte-for-byte equivalent. |
| Clitics and strata | The exporter duplicates/labels records and uses large order-class ranges and grammar productions. | HC has explicit Morphology, Clitics, and Surface strata. | Use HC's strata and report likely migration differences. |
| Affix processes | General FieldWorks affix-process rules are HC-only, although AMPLE has special full/partial reduplication syntax. | HC represents richer non-concatenative processes. | Keep authored HC processes; separately implement or refuse XAMPLE-supported reduplication that PanGloss cannot yet import. |
| Compounding | The XAMPLE exporter emits its own `\cr` policy. | HC may synthesize default left- and right-headed rules when none are authored. | Use normal HC policy. Pin the actual XAMPLE baseline with an oracle fixture before describing the difference more narrowly. |
| Search caps | AMPLE/XAMPLE limits nulls, affix kinds, roots, interfixes, and returned analyses; early stopping affects survivors. | HC has different containment controls and no equivalent per-kind validity predicates. | Preserve XAMPLE values as provenance. Do not turn them into HC linguistic validity. |

## 4. What is genuinely the same — VERIFIED

### 4.1 Phonology-free execution

An empty HC phonological-rule list is valid. Analysis still unapplies morphology, performs lexical
lookup, synthesizes candidates, and checks word/allomorph validity. Therefore no “identity rewrite
rule” or fabricated phonological grammar is needed.

This is already a real PanGloss shape: the Sena reference grammar has many required environments
and zero phonological rules. On the FST path, allomorphs are proposed and
`pg_rules::validity::environments_ok` confirms environments against the final candidate
(`pg-foma/src/precision.rs`; `pg-rules/src/validity.rs`).

### 4.2 Listed allomorphy and environments

XAMPLE tests an SEC against a surface string. HC's `AllomorphEnvironment.IsMatch` anchors the left
and right patterns at the conditioned morph and checks the final synthesized shape. Without rules,
that shape is the listed allomorph concatenation. Circumfix pieces remain separately conditioned
morph annotations.

Multiple positive XAMPLE SECs are alternatives, while negative SECs combine as restrictions. HC
models required/excluded environments directly. This is a representation change, not necessarily a
linguistic one.

### 4.3 Segment-list natural classes

An XAMPLE `\scl` emitted from a FieldWorks `PhNCSegments` is a literal set of grapheme strings.
HC's `SegmentNaturalClass` is its direct counterpart. A feature-based natural class is not the same
thing and must never be synthesized merely because a class name looks meaningful.

### 4.4 Co-occurrence and morphosyntax

Both paths consume the same FieldWorks morpheme/allomorph prohibitions, POS/MSA information,
inflection classes, exception features, stem names, and template structure. XAMPLE serializes them
as AMPLE tests and PC-PATR productions; HC retains structured objects and feature constraints.

## 5. What may change under ordinary HC — DECIDED

A successful, lossless conversion promises preservation of authored information. It does not
promise that HC will imitate every omission, default, or search decision of XAMPLE.

### 5.1 Authored rules start running

XAMPLE ignored FieldWorks phonological rules. HC runs them. A project that merely accumulated stale
rules while using XAMPLE may therefore behave differently after conversion. PanGloss reports that
fact but does not silently remove the rules. If a rule's references or feature requirements cannot
be represented, the grammar is refused.

### 5.2 HC-native defaults apply

HC's compounding, strata, clitic placement, rule ordering, and search controls are HC semantics.
Some differ from the code generated for XAMPLE. Those differences belong in migration reporting
and focused differential fixtures, not in a cross-cutting “XAMPLE profile.”

### 5.3 XAMPLE caps do not remove HC analyses

XAMPLE's `MaxNulls`, `MaxPrefixes`, `MaxInfixes`, `MaxSuffixes`, `MaxInterfixes`, `MaxRoots`, and
`MaxAnalysesToReturn` constrain its search and output. They are **resource containment**, with an
observable compatibility effect; they are not evidence that an otherwise well-formed HC analysis
is linguistically invalid.

PanGloss records these values so a comparison can explain a divergence. It does not attach them to
`Grammar`, reject finished HC words with them, or claim that a deterministic post-sort truncation
recreates XAMPLE's early-stop survivor set.

### 5.4 Native HC decisions replace exporter workarounds

Negative SEC generation, numeric order classes, duplicated clitic records, and PC-PATR scaffolding
exist because XAMPLE needs serialized encodings of decisions that HC can make directly. Conversion
must call the HC decision owner rather than re-derive the exporter's approximation.

## 6. The segmental substrate — VERIFIED design basis, DECIDED policy

### 6.1 Why XAMPLE projects expose the gap

`pg_grammar::compile::chardef::build` currently creates character definitions only from
`snapshot.phonology.phonemes` and boundary markers. A failed `segment_with_patterns` call causes an
allomorph to be skipped; if all alternatives disappear, the entry disappears. This is silent
semantic loss disguised as a non-fatal warning.

FieldWorks already contains the seed of the correct mechanism. With
`AcceptUnspecifiedGraphemes`, `HCLoader.Segment` takes the offending Unicode text element, adds it
as a featureless segment, and retries (`HCLoader.cs:2532-2560`). It can also seed word-forming and
non-word-forming characters from the vernacular writing system (`HCLoader.cs:2714-2730`).

The XAMPLE branch now imports `AcceptUnspecifiedGraphemes`, `ActiveParser`, XAMPLE cap metadata,
and `.fwbackup` LDML exemplar characters. The control is not complete until the compiler actually
uses the substrate information.

### 6.2 End-to-end conversion seam

Losslessness starts before `Snapshot`. The `.fwdata` graph contains the authored objects; an
extractor that warns and omits an unresolved object has already changed the grammar before
`pg-grammar` can count it. Consequently, each `pg-fwdata` extractor owner must publish the source
identities it considered and the identities it represented or rejected. Fatal import issues and
that inventory travel with in-memory and serialized snapshots. A compiler-only census cannot prove
lossless FieldWorks-project conversion.

The same ownership rule continues in `pg-grammar`: entry, allomorph, affix-process, environment,
co-occurrence, template, class, and rule builders publish the identities they actually selected and
represented. A central inventory walker must not reproduce active/disabled, writing-system,
morph-type, or representation-selection predicates.

Within the compiler, substrate completion belongs at this seam:

Completion belongs inside `pg_grammar::compile`, after the authored feature system and initial
character definitions are available but before natural classes consume `phoneme_of` and before any
form is segmented:

```text
authored phonological features
             ↓
authored character definitions
             ↓
complete character substrate from actual usage
             ↓
natural classes → environments → lexicon/templates/rules → Grammar
```

Every downstream engine then consumes the same compiled `Grammar`. There is no HC-versus-XAMPLE
fork in `pg-rules`, `pg-parse`, or an FST backend.

### 6.3 What completion may infer

Completion may infer only:

- that an authored surface text element must exist in the character table;
- that it is a segment when authoritative writing-system data identifies it as word-forming;
- that it is a boundary when it is authored as one or an explicitly versioned safe-boundary table
  names that exact element. Initially that table is limited to Unicode space separators and ASCII
  tab, carriage return, and line feed;
- a featureless character definition carrying its observed representation.

It may not infer:

- phonological feature values;
- positive or negative membership in a feature-based class (an unspecified HC feature lane is not
  evidence of non-membership);
- a rewrite rule or allomorphic relationship;
- the intended meaning of an unresolved environment;
- a segment/boundary choice whose alternatives would change accepted analyses.

Punctuation is not intrinsically a boundary. Apostrophes, hyphens, joiners, and other punctuation
can be word-forming in real orthographies and therefore require writing-system or authored
evidence. When the available `.fwdata` lacks that evidence, refusal is more honest than a Unicode
category guess.

The environment parser owns environment syntax and must expose literal elements to substrate
completion. A new module must not approximate that parser with its own regex or tokenizer.

## 7. Refusal doctrine — DECIDED

Lossless conversion has two successful operations: represent faithfully, or infer the narrowly
defined substrate above. Everything else that would change semantics is a refusal.

### 7.1 Refuse wrong data

Examples include dangling references, conflicting phoneme representations, invalid feature/value
references, structurally malformed environments, and a rule referring to a missing class or
boundary. The refusal identifies the owning object and reference.

### 7.2 Refuse ambiguity

Examples include a material segment-versus-boundary choice without writing-system evidence and an
unresolved source-order question that changes disjunctive allomorph selection. PanGloss does not
choose whichever interpretation compiles. An inferred segment with unspecified features is not by
itself ambiguous computationally: ordinary HC unification defines its behavior, and the migration
report calls out the combination when feature-based rules/classes are active.

### 7.3 Refuse unrepresentable constructs

An active construct the current compiler cannot express refuses the grammar. Current known audit
targets include metathesis, bracket-pattern/full/partial reduplication, circumfix cross-products,
some clitic-as-affix/stem placements, and custom strata reorganization. The exact inventory must be
generated from live compiler skip/fallback sites before implementation.

This distinction matters: AMPLE/XAMPLE's special full and partial reduplication is limited compared
with general HC affix processes, but it is still a supported XAMPLE construct. PanGloss currently
warns and drops bracket-pattern affix forms (`pg-grammar/src/compile/affixes.rs:431-455`). A grammar
using one must be implemented faithfully or refused, never accepted after deleting it.

### 7.4 Refuse weakened restrictions

FieldWorks HCLoader can treat an invalid environment as unrestricted. That preserves recall but
loses precision. Under the lossless conversion contract, a required/excluded restriction that
cannot be represented refuses the grammar. “Loaded unrestricted” is not successful conversion.

### 7.5 Refusal is not containment

These refusals are correctness/representability decisions. XAMPLE caps, time limits, HC
`MaxAlternatives`, and FST resource envelopes are separate containment mechanisms. A readiness or
containment limit must not be used as a substitute for proving the grammar representable.

## 8. Differential measurement — DECIDED

The real XAMPLE engine is still essential, but its jurisdiction changes.

### 8.1 Ownership and fixture materialization

The executable comparator is PanGloss work: it exercises PanGloss conversion and runtime behavior
and may use installed FieldWorks assemblies and `xample64.dll`. The synthetic source project is
cross-engine conformance data, so its canonical `project.fwdata` and
`phonology-mutations.yaml` live beside the corresponding fixture under `machine/conformance`.
This keeps the dependency one-way: PanGloss consumes the pinned Machine corpus; Machine neither
calls PanGloss nor acquires a FieldWorks/XAMPLE build dependency.

One canonical project is sufficient. A mutation case is a declarative request to remove named
phonemes, not another checked-in `.fwdata`. The runner clones the complete project directory,
applies the request through LibLCM, saves and reopens it, and passes the same temporary project to
both projection paths. It records the base digest, case id, resolved GUIDs and representations,
deletion count, reopened-project digest, projection digests, and diagnostics. A control that
matches nothing or cannot safely delete its target fails; it never becomes a skipped comparison.

Specific targets use GUID identity plus an asserted human-readable representation. A typed
remove-all operation is allowed only after the owner reference graph proves every target is
semantically unreferenced. Representation-only selectors and inferred "unused phoneme" queries are
excluded: they can silently retarget when aliases or fixture data change. Generated XAMPLE files,
generated HC XML, and mutated projects remain temporary.

The baseline project must reproduce the checked-in HC grammar semantically and reproduce its word
analyses. For an unreferenced-phoneme mutation, XAMPLE's generated text and both engines' normalized
analysis multisets must remain equal to baseline; HC's completion report must name exactly the
removed graphemes and assign no features. Raw HCLoader XML is expected to lose those character
definitions and is therefore not byte-compared to the baseline after mutation. A phoneme used by a
segment natural class, explicit segment context, feature structure, constraint, or rule is authored
semantics: the mutation preflight refuses its removal rather than presenting it as an invariance
experiment.

### 8.2 What the comparison answers

For a common supported fixture, the comparator has one source input: the same checked
`.fwdata`/`.fwbackup`. It invokes the real FieldWorks `M3ToXAmpleTransformer` (through a small
version-pinned helper process) to produce XAMPLE control/dictionary/grammar files, then runs the
real XAMPLE DLL. In parallel, PanGloss imports that same source and runs ordinary HC. A hand-written
`Snapshot`-to-XAMPLE approximation is not the primary differential because it could omit the same
LCM facts under test.

The helper returns a manifest containing transformer and DLL versions, source digest, generated
file digests, engine diagnostics, cap status, and normalized analysis signatures. `compared`
increments only when transformation and parsing complete with `engine_error == None` and at least
one word was exercised. Portable protocol tests retain only small captured helper/DLL responses
needed to test parsing and protocol compatibility. They do not retain per-fixture XAMPLE
projections or mutated projects, and replay is labelled captured evidence rather than a live
comparison.

For a successfully projected fixture, run both projections and record:

- words XAMPLE accepts and HC refuses;
- words HC accepts and XAMPLE refuses;
- analysis/morpheme-signature differences where identifiers can be normalized, preserving
  duplicate derivation multiplicity rather than collapsing results to a set;
- whether XAMPLE hit a configured cap or early-stop condition;
- which HC migration-difference categories the fixture exercises.

Both divergence directions are required. A gate that counts only new HC coverage cannot detect
lost XAMPLE behavior; a gate that compares nothing because every emitter attempt refused is not a
passing gate.

### 8.3 What the comparison does not decide

An XAMPLE-only result is evidence to investigate. It may reveal conversion loss, an HC bug, a
transform difference, or XAMPLE's own distinct semantics. It is not automatic authority to add an
XAMPLE branch to HC.

An HC-only result may be the intended effect of authored phonological rules, native HC defaults, or
the absence of XAMPLE caps. It is recorded and explained rather than automatically suppressed.

### 8.4 Oracle adapter refusal

The live adapter accepts FieldWorks project input and delegates projection decisions to the real
FieldWorks transformer. It does not reject an active HC phonological rule: the transformer's
omission of that rule is precisely the behavior the migration experiment measures. `OutsideSubset`
is reserved for infrastructure that the pinned transformer genuinely cannot load, or for portable
captured-fixture tooling that lacks a required artifact. It never emits an approximate XAMPLE
grammar and calls the result comparable.

## 9. Known questions that require measurement

### 9.1 Allomorph source order

Allomorph disjunction is order-sensitive. The earlier draft claimed `LexemeForm` followed by
`AlternateForms`; current PanGloss import explicitly loads `AlternateForms` first and `LexemeForm`
last (`pg-fwdata/src/extract/lexicon.rs:35-42`) to mirror `HCLoader.cs:263`. The XAMPLE NegSEC
generator's effective order must be established with a fixture whose result changes when the order
is reversed. Until then, do not “fix” the order from prose.

### 9.2 Default compounding

The earlier draft claimed XAMPLE emitted only explicit compound-category pairs and therefore HC
defaults must be disabled. A later source audit found an unconditional/baseline `\cr W W` in the
FieldWorks XAMPLE transform. The exact relationship to HC's two default headed rules remains an
oracle question. Normal HC behavior stays intact until a focused fixture establishes the difference.

### 9.3 Template and PC-PATR edge cases

The XAMPLE grammar transform contains special productions for partial analyses, required/optional
slots, and all-optional templates. HC represents these concepts directly. Agreement is expected in
intent, but only fixtures can establish the edge behavior.

### 9.4 Current importer coverage

The compiler module documents additional unsupported constructs. Each must be classified against
real project use and either implemented or converted from warning-and-drop into a typed lossless
refusal. This inventory is a prerequisite to claiming general XAMPLE-project support.

## 10. Implementation state as of 2026-09-04

Implemented on `research/xample-phonology`:

- snapshot `ActiveParser` and XAMPLE parameter metadata;
- `AcceptUnspecifiedGraphemes` preservation;
- project exemplar characters;
- `.fwdata` parser-parameter extraction;
- generic reader-based `.fwdata` parsing;
- `.fwbackup` ZIP import and LDML exemplar extraction;
- CLI `.fwbackup` routing.

Partially implemented but no longer part of the accepted architecture:

- `Grammar.analysis_caps` exists as an unwired `Option`, always `None`. It was introduced for the
  superseded exact-XAMPLE profile and should be removed before further implementation.

Not yet implemented:

- substrate completion;
- typed lossless-conversion refusals and reports;
- the end-to-end import/compiler skip/fallback inventory and differential ratchet;
- conversion of semantics-changing warning-and-drop paths into refusal;
- the XAMPLE differential adapter and fixtures;
- missing construct support such as bracket-pattern reduplication.

## 11. Decision history

### 2026-09-03 — superseded

The first accepted design promised the same analysis set as XAMPLE. It introduced an
`XAMPLE` parser profile that would drop phonological rules, disable HC default compounding, attach
XAMPLE caps to `Grammar`, and truncate HC results. That design mixed representability, engine
semantics, and resource containment. It also admitted that deterministic HC truncation could not
reproduce XAMPLE's early-stop survivor set.

### 2026-09-04 — current

The accepted target changed to lossless conversion followed by ordinary HC execution:

1. complete only the segmental substrate;
2. keep authored rules and HC-native decisions;
3. retain XAMPLE configuration as provenance and comparison metadata;
4. measure legacy differences in both directions;
5. refuse wrong, ambiguous, unsupported, or semantically lossy conversions.

Future changes to this contract must update this decision history, the accepted spec, and the
implementation plan together.
