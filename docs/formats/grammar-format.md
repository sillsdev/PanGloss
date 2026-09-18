# The grammar format — `grammar.json`

This is the grammar of the language project, in one file. It tells you every rule, part of
speech, phoneme, and lexicon entry the parser knows about, exactly as of the moment the Baseline
was captured — not necessarily whatever the linguist has typed into FieldWorks since.

## What produced it, and what it is a snapshot of

`grammar.json` is written by `pangloss import <baseline.fwdata> grammar.json`, run once against
the Baseline copy that `motif handoff` captured. It is PanGloss's own JSON snapshot format, in
PanGloss's own camelCase field names — Motif does not invent a competing grammar format, and does
not write HermitCrab XML into the Handoff. PanGloss's import path (`pg-fwdata` → `pg_snapshot` →
`pg_grammar::compile_project`) is documented, by PanGloss itself, as a Rust port of FieldWorks'
own `HCLoader.cs` — the class that turns a FieldWorks language project into the HermitCrab
grammar its interactive parser actually runs. So every field below traces back to a specific
LibLCM property, by way of the specific construct `HCLoader` builds from it; a field with no
such trace is not part of the parser's behaviour, whatever else it might record.

Accompanying every snapshot, alongside `grammar.json` itself, is its own identity: a source hash
over the exact `.fwdata` bytes it was built from, a separate model fingerprint for the compiled
grammar, the importer's version, and any import warnings PanGloss retained. Two Handoffs whose
grammars differ can be told apart by these without re-diffing the grammar text; two Handoffs
whose grammars are identical will share a model fingerprint even if their source hashes differ
by nothing but formatting.

## The write-surface: what is in the grammar, and where it came from

This table is **complete** for what `HCLoader` reads — nothing licensed by the parser is missing
from it, and nothing in it is decorative. Each row is one construct in `grammar.json`, the
LibLCM property it was read from, and what happens with it once HermitCrab has it.

| Grammar construct | LibLCM source | What HermitCrab does with it |
| --- | --- | --- |
| Parts of speech | `LangProject.PartsOfSpeechOA`, a flat ID-referenced list at the engine level | Every rule, template, and slot that restricts itself to a category references this list by ID. FieldWorks lets an author organise categories hierarchically and have a template defined on a parent category apply to its children — that inheritance is resolved before export, so a category ID repeated across many rules is normal, not a mistake |
| Inflection classes and feature system | `PartsOfSpeechOA.{DefaultInflectionClassRA, InflectionClassesOC}`; `MsFeatureSystemOA`/`PhFeatureSystemOA` | Two different mechanisms answer two different questions. Arbitrary lexical conditioning with no semantic content and no visibility to agreement is an MPR feature; a semantically real, syntactically visible category (gender, person, number, case) is a value in the syntactic feature system. Getting this choice right matters early — see `hc-mechanics.md` |
| Phonemes and natural classes | `PhonologicalDataOA.PhonemeSetsOS[0]` **only** — every other phoneme set in the project is invisible to the parser; `PhPhoneme.{FeaturesOA, CodesOS}`; `.NaturalClassesOS` | Segments and the classes environments and rules match against. Only the first phoneme set is ever read, project-wide |
| Phonological rules | `PhonologicalDataOA.PhonRulesOS`, in sequence | Applied in the order stored, via each rule's own position (`OrderNumber`, the *virtual* `IndexInOwner + 1` — order is never authored as a separate field). Rule order is semantic: it encodes feeding and bleeding |
| Environments | `PhEnvironment.StringRepresentation.Text` | Parsed as a **string grammar** (`/left_right`, `#`, `[NC]`, optionality), never from the structured left/right-context graph the model also carries. An invalid environment string does not fail to load — it becomes "applies everywhere," which is more permissive than the author intended, not less |
| Compounding rules | `MorphologicalDataOA.CompoundRulesOS` (`MoEndoCompound`/`MoExoCompound`) | Endocentric and exocentric compounding. Capped at one application per derivation by default; FieldWorks' own loader only ever raises that cap for endocentric rules, so a recursive (three-or-more-element) pattern modelled as exocentric can never be configured to recurse |
| Lexicon | `LexEntry.{AlternateFormsOS, LexemeFormOA, MorphoSyntaxAnalysesOC, SensesOS}`; `MoStemAllomorph`/`MoAffixAllomorph`; `MoStemName` | Every entry, its allomorphs, and the environments and stem-name regions that select between them |
| MSAs (morphosyntactic analyses) | `MoStemMsa`, `MoDerivAffMsa`, `MoInflAffMsa`, `MoUnclassifiedAffixMsa` | What category an entry or affix belongs to, and what it derives to or requires |
| Affix templates and slots | `MoInflAffixTemplate.{SuffixSlotsRS, PrefixSlotsRS}`; `MoInflAffixSlot.{Optional, Affixes}` | Slot order is `SuffixSlotsRS` followed by `PrefixSlotsRS` reversed — **closest-to-stem first in both** directions. Only these two of FieldWorks' five parallel slot sequences are ever read; `Slots`, `ProcliticSlots`, and `EncliticSlots` are FieldWorks UI surface the parser never sees |
| Co-occurrence restrictions | `MoAlloAdhocProhib`/`MoMorphAdhocProhib` | Ad hoc prohibitions between allomorphs or morphemes. A restriction naming several other morphemes blocks the key morpheme only when **all** of them co-occur together, not when any one does |

## What is never part of the grammar, however it looks in FieldWorks

These are read from LibLCM by other FieldWorks tooling, or exist in the model, but `HCLoader`
never reads them, so they never appear in `grammar.json` and cannot affect a parse:

- `MoStratum` and every reference to one (`StratumRA`, `PhSegmentRule.InitialStratum`/
  `FinalStratum`) — real projects have zero strata objects; every one of them actually runs on
  the three strata the parser hardcodes, `Morphology`, `Clitics`, `Surface`. See `hc-mechanics.md`
  for what that means for rule ordering.
- Every phoneme set beyond the first (`PhonemeSetsOS[0]`).
- The `<XAmple>` half of the project's parser-parameters text field.
- Every `Description` field on every grammar object.
- Derivation-trace objects (`MoStratumApp`, `MoDerivTrace`, `MoPhonolRuleApp`,
  `MoAffixTemplateApp`) — these record what a *previous parse* did, not what the grammar *is*.

## Reading a grammar that looks broken or incomplete

A grammar snapshot that seems to be missing an entry, a rule, or a whole class is not
necessarily evidence of an export bug. `HCLoader` fails several things **silently** rather than
with a diagnostic, and PanGloss's port reproduces the same behaviour because matching it exactly
is the whole point of a faithful port:

- An entry, morphological rule, or template with zero surviving allomorphs or slots vanishes
  from the grammar entirely, with nothing marking the gap.
- A natural class with even one phoneme that fails a grapheme check becomes entirely unusable —
  cached as unusable, and every later reference to it fails just as silently.
- A stem name with zero non-empty regions produces nothing.
- Natural-class abbreviations that collide resolve last-one-wins, silently, in the lookup
  environments are parsed against.

Two conditions instead **crash the load outright** rather than silently dropping anything, so if
a Handoff's grammar is missing altogether, one of these is worth asking about: a single rule
using more than 24 distinct alpha variables (a fixed-size internal array), or a dangling
reference from an inflection class, production restriction, or inflection-class-referring
grammatical-info type to an object that no longer exists.

Finally, one HermitCrab capability — realizational morphology, `RealizationalAffixProcessRule` —
exists in the engine but is never produced by `HCLoader` from a FieldWorks project. A grammar
built from FieldWorks data cannot express it, regardless of what the FieldWorks UI otherwise
allows an author to describe.
