# Running XAmple-shaped grammars on HC-Rust and the FST compilers

Tag: **MIXED** — §1–§4 are VERIFIED against FieldWorks, `sillsdev/machine`, and this repo (file:line
cited); §5–§6 are PROPOSED.

Question asked (2026-09-02): "I want to be able to use XAmple parser grammars, which really means
I need a synthetic phonology that transparently enables XAmple-like grammars to just work with
HC-Rust / the FST compilers."

## 0. The one-paragraph answer

An "XAmple grammar" in FieldWorks is not a separate artifact: FLEx generates XAmple's control and
dictionary files and HermitCrab's `Language` object **from the same LCM model**
(`M3ToXAmpleTransformer.cs:199-216` vs `HCLoader.cs:164-357`). What makes a project "XAmple-shaped"
is what XAmple *ignores*: every `PhRegularRule`, `PhMetathesisRule`, and the whole phonological
feature system are never read by any XAmple transform. XAmple is pure item-and-arrangement — listed
allomorphs, surface string-environment constraints (SECs) over literal graphemes and segment lists,
morpheme/allomorph co-occurrence constraints, orderclass, and a word grammar. HermitCrab already
has a faithful counterpart for every one of those constructs, and the C# engine is fully legal with
zero phonological rules (`SynthesisStratumRule.cs:42-44` builds an empty, identity cascade). So the
"synthetic phonology" is **not** a set of rules. It is the *segmental substrate* XAmple never needed
and HC cannot run without: a character-definition table that segments every listed form, and a
natural-class inventory that resolves every `[X]` an environment mentions. FLEx's own
`AcceptUnspecifiedGraphemes` (`HCLoader.cs:2532-2560`, `2714-2730`) is exactly this mechanism in
embryo. The proposal (§5) is to make that substrate a first-class, deterministic, *reported* step in
PanGloss, add an XAmple-shape conformance profile with the real `xample64.dll` as the oracle of
record, and close the small list of genuine semantic gaps (§4) instead of approximating them.

## 1. What XAmple actually is (FieldWorks-verified)

FLEx emits four text files per project via XSLT over an M3 dump
(`Src/LexText/ParserCore/M3ToXAmpleTransformer.cs:199-216`): `<db>adctl.txt`
(`FxtM3ParserToXAmpleADCtl.xsl`), `<db>lex.txt` (`FxtM3ParserToXAmpleLex.xsl`), `<db>gram.txt`
(`FxtM3ParserToToXAmpleGrammar.xsl`) and the word-grammar-debugger XSLT. Before that it runs GAFAWS
to compute `orderclass` ranges for inflectional slots (`M3ToXAmpleTransformer.cs:91-197`).

| XAmple construct | Emitted at | HC counterpart (C#) |
|---|---|---|
| `\ca` categories, `\cr` compound category pairs | `ADCtl.xsl:46-73` | POS + `CompoundingRule` |
| `\maxnull \maxp \maxi \maxs \maxr \maxn` | `ADCtl.xsl:117-134` | **none** — analysis-count caps (§4.3) |
| `\mp` / `\ap` morpheme & allomorph properties (`RootPOSn`, `InflClassn`, `StemNamen`, `MSEnvPOSn`, `Bound`) | `ADCtl.xsl:155-208` | MSA POS/inflection class, `RootAllomorph.IsBound`, `StemName`, `MprFeature` |
| `\mcc` morpheme co-occurrence, `\ancc` allomorph co-occurrence, with `...`/adjacency | `ADCtl.xsl:213-364` | `MorphemeCoOccurrenceRule` / `AllomorphCoOccurrenceRule` (`MorphCoOccurrenceRule.cs:11-37`) |
| `\scl` string classes — **`PhNCSegments` only**, listed as grapheme strings | `ADCtl.xsl:368-391` | `SegmentNaturalClass` |
| `\a form {id} / L _ R` SECs; `~/` negative SECs | `Lex.xsl:723-772, 1660-1744, 1933-1971` | `AllomorphEnvironment` Require / Exclude (`AllomorphEnvironment.cs:12-97`) |
| `\o` orderclass, `\pt/\it/\nt/\rt/\st/\ft` user tests | `ADCtl.xsl:395-522` | `AffixTemplate` slots + stratum `MorphologicalRuleOrder` |
| `\loc` infix location | `Lex.xsl` | `AffixProcessRule` infix |
| Word grammar (`gram.txt`, PC-PATR) | `Grammar.xsl:53-838` | feature-structure unification in `Word.SyntacticFeatureStruct` |

Verified absences: no XAmple XSLT references `PhRegularRule`, `PhMetathesisRule`, or
`PhFeatureSystem`; `\scl` never emits a feature-based class. FLEx's own training material states it
outright: "Phonological rules (HC only) / Affix process rules (HC only) … Phonological constraints
… apply to both XAmple and Hermit Crab"
(`Docs/ai-parser-help/workflow/sources/black-parser-workshop-2026-L02-fulltext.txt:323-346`).

A real example of the emitted lexicon shape (`ParserCoreTests/M3ToXAmpleTransformerTestsDataFiles/
CliticEnvsLexicon.txt`): `\a ئون {23355} / [27536][23357]_` followed by sibling allomorphs carrying
the *negated* environments `~/ [27536][23357]_` — the NegSEC trick FLEx uses to make XAmple's
non-disjunctive allomorphs behave disjunctively.

**The XAmple engine is on this machine.** `DistFiles/xample64.dll`, `xample32.dll`,
`libxample64.so`, driven by `XAmpleManagedWrapper/XAmpleDLLWrapper.cs` (`LoadFiles` at the
`public void LoadFiles` method: needs `cd.tab`, `<db>adctl.txt`, `<db>gram.txt`, `<db>lex.txt`;
`ParseString`/`TraceString`). `DistFiles/Language Explorer/Configuration/Grammar/XAmplecd.tab` is
the fixed code table. `ParserCoreTests/M3ToXAmpleTransformerTestsDataFiles/` holds ~30 ready-made
M3 dumps and their generated `adctl/lex/gram` triples (Clitic, CircumfixInfix, StemName3,
IrregularlyInflectedForms, FullRedup, Latin, Orizaba, Abaza, emi-flex, …).

## 2. What C# HermitCrab already guarantees for the phonology-free case

- Zero phonological rules is legal and behaviour-preserving: `SynthesisStratumRule.cs:42-44`
  builds a `LinearRuleCascade` over an empty list; `AnalysisStratumRule.cs:24-26` mirrors it. The
  parse still runs unapply → lexical lookup → synthesis → `IsWordValid`/`IsMatch`
  (`Morpher.cs:162-220, 634-676`), so environments and co-occurrence rules are still enforced.
- `AllomorphEnvironment.IsMatch` (`AllomorphEnvironment.cs:88-97`) anchors left/right patterns
  immediately at the morph's edges and matches the **final synthesized shape**. With no rules, that
  shape is the concatenation of the chosen allomorphs, i.e. exactly the string XAmple tests SECs
  against. Circumfix pieces are separate `Morph` annotations, each checked independently
  (`SynthesisAffixProcessAllomorphRuleSpec.cs:237-259`).
- Disjunction: `Allomorph.IsWordValid` (`Allomorph.cs:105-156`) rejects an allomorph if an
  earlier-indexed sibling would also have been valid. This is the semantic FLEx *emulates* in XAmple
  with NegSECs, so order of allomorphs is the shared contract (§4.1).
- Co-occurrence: `MorphCoOccurrenceRule.cs:43-196`, checked at the same confirm point.
- The conformance protocol already names this profile: `requires: []` fixtures are "pure
  morphotactics, runnable by any conforming engine", motivated explicitly by XAmple
  (`machine/conformance/PROTOCOL.md:218-231`); §6 anticipates a `grammar.xample/` per-fixture
  representation and a future XAmple adapter (`PROTOCOL.md:244-250`); `parity-check.py` enforces an
  "XAmple floor" (`parity-check.py:13,164,191-202`). Existing `requires: []` fixtures:
  `suffixing-evidential-adjacency-chain` (environments + both co-occurrence kinds, no rules) and
  `edge-cases/truncate-morphotactic`.

## 3. What FLEx's HCLoader does about the missing substrate today

HC needs a `CharacterDefinitionTable` that can segment every form, and every `[NC]` in an
environment must resolve. FLEx's answer is `AcceptUnspecifiedGraphemes` (`HCLoader.cs:96`):

- `Segment(string)` (`HCLoader.cs:2532-2560`): on a segmentation failure, take the offending text
  element (base char + combining marks), `m_table.AddSegment(symbolStr)`, and retry until the whole
  form segments. Featureless segments, created on demand.
- Table construction (`HCLoader.cs:2714-2730`): pre-seed every *word-forming* character of the
  default vernacular writing system as a segment and every *other* character as a boundary.
- Environments whose natural class cannot be resolved are treated as **invalid as a whole** and the
  allomorph is loaded **unrestricted** (`IsValidEnvironment`, `HCLoader.cs:1199-1271`;
  `GetValidEnvironments` blank fallback) — PanGloss already ports this verbatim
  (`docs/research/pg-grammar-environment-validation-granularity.md`).

This is the seed of a synthetic phonology, with three defects for our purpose: it is opt-in and
off by default; it is silent (nothing reports which segments were invented); and it cannot invent a
*natural class*, only a segment, so an XAmple `\scl` that maps to a feature-based `PhNCFeatures`
class (which XAmple cannot express) or to a class whose members failed to load degrades to
"unrestricted" — a recall-preserving but precision-losing fallback the user never sees.

## 4. Genuine semantic gaps between XAmple and HC (the parts a substrate cannot fix)

4.1 **Allomorph disjunction vs NegSECs.** XAmple admits every allomorph whose SEC matches; FLEx
restores disjunction by emitting `~/` environments copied from *earlier* siblings of a compatible
morph type (`Lex.xsl:1660-1744`). HC gets the same effect from allomorph order
(`Allomorph.cs:127-152`) *only if* the order the loader hands HC equals the order FLEx walks for
NegSECs (`LexemeForm` then `AlternateForms`, preceding-sibling). Must be pinned by a fixture, not
assumed.

4.2 **Class membership.** `\scl` lists grapheme strings; an XAmple environment `[V]` is a literal
alternation. HC's `SegmentNaturalClass` is the exact analogue; a feature-based class is *not* and
must never be synthesized for an XAmple-shape grammar.

4.3 **Analysis caps.** `\maxnull/\maxp/\maxs/...` and `MaxAnalysesToReturn` bound XAmple's search;
HC has no per-affix-kind caps. A project that relied on `\maxnull 0` to suppress zero-morphs gets
more analyses from HC. This is an *output-count* divergence, not a recall loss; report it, do not
emulate it in the grammar.

4.4 **Default compounding.** HC synthesizes two default compound rules when the project has none
(`HCLoader.cs:235-238`, `DefaultCompoundingRules`); XAmple emits only explicit `\cr` pairs. An
XAmple-shape profile must set `NoDefaultCompounding`.

4.5 **Word grammar.** XAmple's PC-PATR `gram.txt` unifies features FLEx derives from MSAs; HC does
the same via syntactic feature structures. Equivalent in intent; divergences here are FLEx-transform
bugs, and only an oracle diff finds them.

4.6 **Strata.** XAmple has one level. HC's three default strata (`Morphology`, `Clitics`,
`Surface`, `HCLoader.cs:227-233`) with `NotOnClitics` are irrelevant with zero rules but must stay
rule-free.

## 4b. Where PanGloss stands today (VERIFIED)

The good news first: the grammar model never required phonology. `StratumDef.prules` is a plain
`Vec` with no minimum (`pg-grammar/src/model.rs:1056-1068`); `EnvironmentDef`,
`AllomorphCoOccurrenceRuleDef` and `MorphemeCoOccurrenceRuleDef` live on allomorphs and morphemes
independently of any rule (`model.rs:364-373, 520-543`); Sena — a reference grammar with 144
`RequiredEnvironments` and **zero** phonological rules — is the shape the whole confirm path was
measured on (`pg-foma/src/precision.rs:69-70, 750`). The FST side never compiles environments at
all: every allomorph is emitted and `pg_rules::validity::environments_ok` prunes at confirm
(`precision.rs:1-30`, `validity.rs:91-107`), which is recall-safe by construction and is why
environments have no `CharacteristicKind`. The phonology-free routing gap was already closed
(`Applicability::HasPhonologyOrTemplates`, `docs/research/pg-foma-templated-phonology-free-routing-notes.md`).
Confirm-time environment checks run on the final shape per contiguous morph run
(`validity.rs:50-59, 431-503`), so with no rules they see exactly the string XAmple's SEC sees.

The gap is the substrate, and it is a silent one:

- `pg_grammar::compile::chardef::build` (`compile/chardef.rs:23-80`) builds the character table
  **only** from `snapshot.phonology.phonemes` and `boundary_markers`. There is no
  `AcceptUnspecifiedGraphemes` path and no writing-system seeding.
- A form the table cannot segment is an `Err("cannot segment …")` in `build_root_allomorph`
  (`compile/lexicon.rs:300-301`, likewise `affixes.rs:664`, `environment.rs:188`,
  `templates.rs:286`), and the caller does `warnings.push(… "; skipped")` (`lexicon.rs:265`). The
  allomorph vanishes; if every allomorph of an entry vanishes, the entry vanishes (`lexicon.rs:268`).
  Recall loss, reported as a string in a `Vec<String>`.
- `accept_unspecified_graphemes` is parsed (`pg-fwdata/src/parser_params.rs:25-27`) and stored
  (`pg-snapshot/src/morphology.rs:373`) and **read by nothing else in the workspace** — a control
  that cannot act, in exactly the shape `CLAUDE.md` names.
- `pg-fwdata` does not read `MorphologicalData.ActiveParser`, and `parser_params.rs:1` treats an
  absent `<HC>` block ("e.g. an XAmple-configured project") purely as "use HC defaults". Nothing
  distinguishes an XAmple-configured project from an HC one, so the `<XAmple>` block
  (`MaxNulls/MaxPrefixes/…/MaxAnalysesToReturn`, seen live in the Mbugwe backup on this machine)
  is dropped.

Why this bites XAmple projects specifically: a linguist who has only ever run XAmple had no reason
to maintain `PhPhonemeSet`. FLEx's XAmple transforms never read it, so it can be empty, partial, or
stale, and the project still parsed perfectly. Import that project today and every allomorph with
an un-inventoried grapheme is silently dropped.

## 5. Proposal

Five workstreams, in the order the repo's own rules impose (measurement before change, oracle
before fixture, seam before condition). Estimated sizes are relative, not dates.

### 5.1 Red gates first (small)

1. **Inert-control gate.** A snapshot with `accept_unspecified_graphemes = true` and one allomorph
   carrying a grapheme absent from `phonemes` must compile that allomorph. Fails today; it is the
   `capability::inert_predicates`-style test for this flag.
2. **Substrate-loss ratchet.** For every fwdata/snapshot fixture in the suite, count allomorphs
   skipped with "cannot segment". Record today's counts as `NoMoreThan` ratchets, with the target
   0 for any fixture tagged XAmple-shape. This is the differential measurement that will show
   whether 5.2 helped and whether it broke anything.
3. **Two-oracle agreement gate over `requires: []` fixtures** (both directions: words XAmple
   accepts and HC refuses, and vice versa), see 5.4. Starts as a report, becomes a ratchet.

### 5.2 Substrate synthesis in `pg_grammar::compile` (medium) — the actual "synthetic phonology"

A new step between `chardef::build` and lexicon/affix compilation, owned by `pg-grammar` so
HC-Rust and every FST backend share one computation:

- **Alphabet collection.** Walk every allomorph form (roots, affixes, circumfix pieces, null-affix
  markers, environment literals) and take the text elements FLEx's `Segment()` would take:
  base character plus following combining marks, NFD-normalised the way `chardef.rs` already
  normalises (`nfd`). Every element the table cannot segment becomes a featureless
  `CharDefKind::Segment` (word-forming) — mirroring `HCLoader.cs:2532-2560`. Characters FLEx would
  classify as "other" become boundaries (`HCLoader.cs:2727-2729`); without LDML in `.fwdata` the
  v1 rule is: punctuation category → boundary, everything else → segment, and the choice is
  reported per character.
- **Class resolution.** `[X]` in an environment resolves to a `NaturalClassKind::Segments` class
  when X is a `PhNCSegments`; a `PhNCFeatures` class whose members fail to load keeps HCLoader's
  whole-environment "unrestricted" fallback (already ported, `compile/environment.rs`) but is now
  **reported as a precision loss**, not merely warned. No feature-based class is ever synthesized:
  synthesized segments have no features, and XAmple's `\scl` is a segment list by construction.
- **A `SubstrateReport`** — invented segments, invented boundaries, unresolved classes, allomorphs
  that fell back to unrestricted, allomorphs still skipped — surfaced by `pangloss import` and by
  the health/`stats` path, and asserted on by the gates in 5.1. `warnings: Vec<String>` is not a
  report.
- **Activation.** Three triggers, any one sufficient: the snapshot's
  `accept_unspecified_graphemes` (making the existing dead flag live); an explicit
  `--phonology-substrate synthesize` on the CLI; or the XAmple-shape profile of 5.3. `strict` (today's
  behaviour) stays the default for HC-configured projects, because for them an un-inventoried
  grapheme is usually a real data error the linguist wants to hear about.

### 5.3 The XAmple-shape profile (medium)

Extend the snapshot with `MorphologicalData.ActiveParser` and the parsed `<XAmple>` block, and add
a grammar-level profile `xample-shape`, selected automatically when `ActiveParser == "XAmple"` and
overridable on the CLI:

- **Phonological rules are dropped**, with the count reported. This is the parity choice: the
  parser the linguist validated never applied them, so applying them changes accepted analyses.
  The opposite choice (`hc-shape`, apply everything) remains one flag away.
- **`NoDefaultCompounding = true`** (§4.4).
- **Allomorph order pinned** to `LexemeForm` then `AlternateForms` document order, the order FLEx's
  NegSEC generator walks (§4.1), asserted by a fixture whose disjunction outcome differs if the
  order flips.
- **Analysis caps reported, never emulated** (§4.3). The report names the XAmple cap values so a
  count divergence against XAmple can be attributed.
- Substrate synthesis (5.2) is on.

### 5.4 XAmple as an oracle of record for the phonology-free profile (medium; ADR-level)

`xample64.dll` and its C API are on this machine (§1), and the conformance protocol already reserves
`grammar.xample/` and a `--capabilities ""` engine (§2). Two steps:

1. **An XAmple runner** implementing the PROTOCOL §1 adapter contract. Cheapest honest version: a
   thin Rust binary calling `xample64.dll`'s cdecl exports (`AmpleLoadControlFiles`,
   `AmpleLoadDictionary`, `AmpleLoadGrammarFile`, `AmpleParseText`) the way
   `XAmpleDLLWrapper.cs` does, run through `pg.ps1 -Mode run` so it gets the same containment as
   every other binary. A C# console referencing `XAmpleManagedWrapper.dll` is the fallback if the
   marshalling fights back.
2. **A `grammar.xml → grammar.xample/` emitter for the `requires: []` subset only**: lexical
   entries and allomorphs with environments → `\lx … \a form {id} / L _ R`; `Excluded` → `~/`;
   co-occurrence rules → `\mcc`/`\ancc` with the adjacency `...`/`~_` spellings FLEx uses
   (`ADCtl.xsl:712-729`); segment natural classes → `\scl`; templates → `\o` orderclass plus the
   same `OrderPfx_ST`/`OrderFinal_FT` test bodies FLEx emits. Refuse (loudly, per fixture) on
   anything outside the subset. FLEx's XSLTs are the reference; its
   `M3ToXAmpleTransformerTestsDataFiles/` triples are free regression data for the emitter.

Oracle hierarchy stays as `CLAUDE.md` states it: `hc.dll` remains the founding oracle for HC
semantics. XAmple becomes the oracle of record for **XAmple parity** on XAmple-shape fixtures, and a
disagreement between the two on such a fixture is a *finding about the FLEx transforms or about
§4*, recorded in `words.yaml`, never silently resolved toward either. This needs a short ADR
because it adds a second oracle with a bounded jurisdiction.

### 5.5 Fixtures (small, ongoing)

Synthetic data only, per the standing rule. First fixture, `conformance-staging/xample-shape/
substrate-synthesis/`: one stem with a grapheme absent from the phoneme table; one affix with two
allomorphs whose selection is `[V]`-conditioned via a segment class; one entry whose disjunction
depends on allomorph order; one `\mcc`-style exclusion; zero rules; `requires: []`. Oracle-verify
against both `hc.dll` and the XAmple runner and record both in `words.yaml`. Then add
`xample-shape` fixtures to `envelope_agrees_with_compiler_gate` and the recall gates so the FST
backends are measured on this shape, not just Sena.

### What this proposal deliberately does not do

- It does not invent phonological *rules*. Nothing in XAmple corresponds to one, and a rule that
  XAmple never applied would change parity.
- It does not import legacy standalone AMPLE `.ad/.dic` grammars (Carla Studio era). The snapshot
  is the right target for that too, but it is a separate parser (`sillsdev/CarlaLegacy`
  `pc-parse/ample/*.c` documents the format) and should only be built if such grammars exist to be
  imported (§6 Q1).
- It does not touch FST refusal thresholds or capability verdicts. Environments are confirm-only
  and recall-safe; the only FST-side work is measurement on the new fixtures.

## 6a. Decisions (2026-09-03)

1. **Input is FieldWorks projects only.** No standalone AMPLE `.ad/.dic` importer.
2. **If a project is XAmple-configured, treat it exactly as XAmple would, and warn loudly.**
   Consequences for §5.3: phonological rules are dropped (loud warning with the count, not a line
   in a report nobody reads); analysis caps from the `<XAmple>` block are **enforced**, not merely
   reported (§4.3 is revised: a parser-side bound on nulls/prefixes/infixes/suffixes/roots and on
   analyses returned, because "as XAmple would" means the same analysis set, not a superset).
3. Defaults adopted for the remaining questions, pending objection: XAmple is oracle of record for
   XAmple-shape fixtures and `hc.dll` for everything else, disagreements recorded not resolved;
   `.fwbackup` input is pulled forward so the letter-vs-separator decision for invented segments is
   read from LDML, with a Unicode-category guess plus loud warning as the bare-`.fwdata` fallback.

## 6. Open questions for the user (original list, kept for the record)

1. **What is the input?** FLEx projects whose active parser is XAmple (`.fwdata`/`.fwbackup`), or
   also standalone AMPLE/XAmple control and dictionary files from outside FLEx? The former is fully
   covered by §5; the latter adds an importer and is not scoped.
2. **Rules in an XAmple project.** When such a project also contains `PhRegularRule`s, is XAmple
   parity (drop them, report the count) the right default? §5.3 assumes yes.
3. **Oracle jurisdiction.** Is a second oracle with a bounded jurisdiction (XAmple for XAmple-shape
   parity, `hc.dll` founding for everything) acceptable as an ADR, or should XAmple stay a
   reporting-only cross-check?
4. **Caps.** Is reporting `\maxnull`/`\maxp`/`MaxAnalysesToReturn` divergences (rather than
   emulating them) acceptable? Emulation would mean bounding HC-Rust's search per affix kind, which
   is a parser change, not a grammar one.
5. **Boundary vs segment for non-letters.** Without LDML valid-characters in `.fwdata`, v1 decides
   by Unicode category and reports each choice. Is `.fwbackup` input (which carries the LDML) worth
   pulling forward from the follow-up list to remove the guess?

## Sources

- FieldWorks: `Src/LexText/ParserCore/M3ToXAmpleTransformer.cs`, `HCLoader.cs`,
  `XAmpleManagedWrapper/XAmpleDLLWrapper.cs`, `Src/Transforms/Application/FxtM3ParserToXAmple*.xsl`,
  `ParserCoreTests/M3ToXAmpleTransformerTestsDataFiles/`, `DistFiles/xample64.dll`,
  `DistFiles/Language Explorer/Configuration/Grammar/XAmplecd.tab`,
  `Docs/ai-parser-help/workflow/sources/black-parser-workshop-2026-L02-fulltext.txt`.
- machine: `src/SIL.Machine.Morphology.HermitCrab/{AllomorphEnvironment,Allomorph,Morpher,Stratum,
  MorphCoOccurrenceRule}.cs`, `PhonologicalRules/SynthesisStratumRule.cs`,
  `conformance/PROTOCOL.md`, `conformance/parity-check.py`,
  `conformance/languages/suffixing-evidential-adjacency-chain/`.
- PanGloss: `docs/fwdata-import-plan.md` (§7 names an XAMPLE-style backend as a follow-up),
  `docs/research/pg-grammar-environment-validation-granularity.md`,
  `docs/research/pg-foma-templated-phonology-free-routing-notes.md`,
  `docs/research/xample-primary-sources.md` (companion, web sources).
