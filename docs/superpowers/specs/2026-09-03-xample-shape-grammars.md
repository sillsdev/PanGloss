# XAmple-shape grammars: running XAmple-configured FieldWorks projects on HC-Rust and the FST backends

Status: ACCEPTED 2026-09-03 (decisions in `docs/research/xample-grammars-on-hc-rust.md` §6a).
Research and citations: that document and `docs/research/xample-primary-sources.md`.

## 1. Scope

Input is a FieldWorks project (`.fwdata`, and `.fwbackup` zips containing one) whose parser
configuration selects XAmple. Output is the same analysis set XAmple would produce for that project,
from HC-Rust directly and from every FST propose + HC-confirm backend, with every deviation from the
project's authored data announced loudly.

Out of scope: standalone AMPLE/XAmple `.ad`/`.dic` grammars; inventing phonological rules; any FST
refusal or capability change (environments are confirm-only and recall-safe already).

## 2. Definitions

- **XAmple-configured.** `MoMorphData.ParserParameters` is an XML string. Its
  `/ParserParameters/ActiveParser` element names the parser. **When the element is absent or the
  string is unparsable, FieldWorks reports `"XAmple"`** (liblcm `OverridesLing_MoClasses.cs`,
  `ActiveParser` getter). PanGloss follows that rule exactly: absent means XAmple.
- **Profile.** `ParserProfile::{Hc, XAmple}`, resolved from `active_parser` unless overridden by the
  caller (`ProfileChoice::{Auto, Hc, XAmple}`). Every entry point that compiles a snapshot takes a
  choice; `Auto` is the CLI default.
- **Substrate.** The character-definition table plus segment natural classes HC needs to segment
  forms and resolve `[X]` in environments. XAmple never needed one, so XAmple projects may carry an
  empty or partial phoneme inventory.

## 3. Behaviour under `ParserProfile::XAmple`

1. **Phonological rules are dropped**: every `PhRegularRule`/`PhMetathesisRule` is excluded from
   the compiled grammar. Loud notice with the count. Rationale: XAmple never read them
   (`FxtM3ParserToXAmple*.xsl` reference no rule class), so applying them changes the analysis set.
2. **No default compounding**: HCLoader's two synthesized default compound rules are not created
   (XAmple emits only explicit `\cr` pairs). Explicit compound rules stay.
3. **Substrate synthesis is on** (§4).
4. **Analysis caps are enforced** (§5), read from `/ParserParameters/XAmple` with FieldWorks'
   transform defaults when a value is absent (`FxtM3ParserCommon.xsl`):
   | Cap | XML element | Default when absent |
   |---|---|---|
   | nulls | `MaxNulls` | 1 |
   | prefixes | `MaxPrefixes` | 5 |
   | suffixes | `MaxSuffixes` | 5 |
   | infixes | `MaxInfixes` | 1 if the project has any infix allomorph, else 0 |
   | interfixes | `MaxInterfixes` | 0 |
   | roots | `MaxRoots` | 3 if a compound rule has a linker, 2 if any compound rule exists, else 1 |
   | analyses returned | `MaxAnalysesToReturn` | 20; a value < 1 means unlimited (`XAmpleParser.cs:126-133`) |
5. **Allomorph order** is `LexemeForm` then `AlternateForms` in document order (the order FLEx's
   NegSEC generator walks), which `pg-fwdata` already preserves; a fixture pins it.
6. Everything else (environments, co-occurrence rules, templates, MSAs, MPR features, clitics)
   compiles exactly as under `Hc`.

Under `ParserProfile::Hc` nothing changes from today except that substrate synthesis can be
switched on explicitly and the previously inert `accept_unspecified_graphemes` flag now acts.

## 4. Substrate synthesis

Owned by `pg_grammar::compile`, so HC-Rust and every FST backend share one table.

- **Trigger**: `SubstratePolicy::Synthesize`, chosen when the profile is `XAmple`, or when
  `parser_parameters.accept_unspecified_graphemes` is true, or when the caller asks for it. Otherwise
  `Strict` (today's behaviour: an unsegmentable form skips the allomorph with a warning).
- **Alphabet**: walk every allomorph form (roots, affixes, circumfix pieces, null-affix markers) and
  every environment literal. Take text elements the way FLEx's `HCLoader.Segment` does: a base
  character plus its following combining marks (`StringInfo.GetNextTextElement`), NFD-normalised
  like `chardef.rs`. Every element the table cannot already segment becomes a featureless
  `CharDefKind::Segment` unless its Unicode general category is punctuation or a symbol, in which
  case it becomes a `Boundary`. When LDML exemplar characters are available (`.fwbackup` input),
  membership in the main exemplar set decides segment vs boundary instead of the category guess.
- **Classes**: `[X]` in an environment resolving to a segment class compiles as today. A
  feature-based class whose members fail to load keeps HCLoader's whole-environment "unrestricted"
  fallback and is reported as a precision loss. No feature-based class is ever synthesized.
- **Report**: `SubstrateReport { invented_segments, invented_boundaries, unresolved_classes,
  unrestricted_allomorphs, skipped_allomorphs }`, returned to the caller and printed by the CLI.
  Nothing is only a string in `warnings`.

## 5. Analysis caps

Counting is positional over the finished word, source-agnostic, matching XAmple's own `\eType`
mapping (FLEx emits proclitic as `prefix`, enclitic as `suffix`, both interfix kinds as interfix,
infix as `infix`; roots/stems are roots):

- **root**: a `Real` morph whose allomorph owner is `AllomorphOwner::Root`.
- **prefix**: a `Real` non-root morph whose span lies entirely before the first root span.
- **suffix**: entirely after the last root span.
- **interfix**: entirely between two root spans.
- **infix**: any other `Real` non-root morph (inside a root span).
- **null**: a non-root morph whose `MorphStatus != Real` (it owns no output nodes).

A word exceeding any cap fails validity (one more clause in `Morpher::is_word_valid_traced`, the
seam shared by direct parse and every FST-confirm call, which is why the caps live on `Grammar`
and not on a `Morpher` builder). `MaxAnalysesToReturn` truncates the final analysis list after a
deterministic sort by signature; XAmple stops its search early at the cap
(`CarlaLegacy pc-parse/ample/anal.c`, `EXPERIMENTAL` block) so *which* analyses survive differs,
and an oracle comparison where XAmple returned exactly the cap compares as a subset.

## 6. Loudness

A `Notice { code, severity: Loud | Info, message }` type replaces free strings for profile and
substrate facts. `Loud` notices are printed to stderr under a banner
`==== XAMPLE PROFILE: <n> notice(s) ====` before any other warning, by every CLI command that
compiles a snapshot, and are never suppressed by `--quiet`-style flags. Codes:
`xample.active-parser-absent`, `xample.rules-dropped`, `xample.default-compounding-off`,
`xample.caps-enforced`, `substrate.segments-invented`, `substrate.boundaries-invented`,
`substrate.class-unresolved`, `substrate.allomorph-unrestricted`, `substrate.allomorph-skipped`.

## 7. Known consequence to record, not hide

Of the real projects on this machine, `sena.fwdata` has no `ActiveParser` element and therefore
is XAmple-configured by FieldWorks' rule, with caps `MaxNulls 1, MaxPrefixes 5, MaxInfixes 1,
MaxSuffixes 5, MaxInterfixes 0, MaxAnalysesToReturn 10`. Existing Sena gates compare HC-Rust against
a committed `hc.dll` oracle, i.e. they test **HC parity**, so they pass `ProfileChoice::Hc`
explicitly. The default CLI run over Sena will announce the XAmple profile loudly and apply caps;
that is the decided behaviour, and a census gate records the profile every real project resolves to.

## 8. Oracle

`hc.dll` remains the founding oracle for HC semantics. For XAmple-shape fixtures
(`requires: []`, profile XAmple) the XAmple engine shipped with FieldWorks (`DistFiles/xample64.dll`,
C API `AmpleCreateSetup/AmpleLoadControlFiles/AmpleLoadDictionary/AmpleLoadGrammarFile/
AmpleParseText/AmpleSetParameter`) is the oracle of record for XAmple parity, reached through a
`grammar.xml → grammar.xample/` emitter over the phonology-free subset. A disagreement between the
two oracles on such a fixture is recorded in `words.yaml`, never resolved silently.

## 9. Gates (before the change, both directions)

1. `accept_unspecified_graphemes` must act: a snapshot with the flag and an un-inventoried grapheme
   compiles the allomorph. Red today.
2. Substrate-loss ratchet: per fixture, count of allomorphs skipped for "cannot segment";
   `NoMoreThan` today's number, 0 for XAmple-shape fixtures.
3. Profile census over real projects (self-skipping when absent): records resolved profile and caps.
4. Caps gate: a synthetic grammar where a word has 2 prefixes parses under `MaxPrefixes 2` and not
   under `MaxPrefixes 1`; likewise nulls, suffixes, roots, analyses-returned.
5. Two-oracle agreement over `requires: []` fixtures (Plan 4), both directions.
