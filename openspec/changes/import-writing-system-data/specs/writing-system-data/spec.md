## ADDED Requirements

### Requirement: Writing-system orthographic data is extracted from the project's own LDML files, never fabricated or fetched from SLDR
PanGloss SHALL extract per-writing-system orthographic data from a FieldWorks project's own
`.ldml` sidecar files (`<ProjectDir>/WritingSystemStore/<tag>.ldml`), keyed by writing-system tag.
PanGloss SHALL NOT fetch data from SLDR (`github.com/silnrsi/sldr`) or any other remote/network
source as part of this extraction, and SHALL NOT synthesize writing-system orthographic data that
is not present in the project's own `.ldml` file for that tag.

#### Scenario: A writing-system tag has a populated `.ldml` file
- **WHEN** a project's `WritingSystemStore/` folder contains an `.ldml` file for a writing-system
  tag referenced by `CurVernWss`/`CurAnalysisWss`
- **THEN** PanGloss extracts that file's orthographic data into the writing-system-data snapshot
  section keyed by that tag

#### Scenario: A writing-system tag has no `.ldml` file
- **WHEN** a project's `WritingSystemStore/` folder has no file for a writing-system tag the project
  references
- **THEN** PanGloss records that tag's writing-system data as not specified, emits a warning, does
  not fetch SLDR or any other external source to fill the gap, and does not fail the overall import

#### Scenario: The `WritingSystemStore/` folder itself is absent
- **WHEN** a project's `.fwdata` file has no sibling `WritingSystemStore/` folder at all
- **THEN** PanGloss records every referenced writing-system tag's data as not specified, emits a
  warning, and completes the rest of the import normally

### Requirement: Word-forming character classification is derived per writing system from its own LDML exemplar data
PanGloss SHALL derive, per writing-system tag, a classification of which characters are
word-forming versus non-word-forming from that writing system's own `<exemplarCharacters>` (main)
and `<exemplarCharacters type="punctuation">` data when present, rather than applying one
hardcoded or generic classification to every writing system.

#### Scenario: A project promotes a character to word-forming
- **WHEN** a writing system's `.ldml` places a character (e.g. a tone marker or apostrophe) in the
  main `<exemplarCharacters>` set rather than the punctuation set
- **THEN** PanGloss's extracted word-forming classification for that writing system reflects that
  character as word-forming, not the generic Unicode default

#### Scenario: A writing system's exemplar data is absent
- **WHEN** a writing system's `.ldml` file has no `<exemplarCharacters>` element, or no file exists
  for that tag
- **THEN** PanGloss reports the word-forming classification for that writing system as not
  specified rather than substituting a generic default inside the extraction step itself

### Requirement: Orthographic edit-unit data is captured from exemplar-set multi-character strings, collation tailoring, and SIL special elements
PanGloss SHALL extract, per writing-system tag, the orthographic edit-unit inventory from **all**
of the following sources in that writing system's own `.ldml`, treating them as complementary and
not assuming any one of them is sufficient:
1. UnicodeSet multi-character strings (`{…}`) declared inside `<exemplarCharacters>` (main and
   `type="punctuation"`) — the primary multigraph/cluster inventory;
2. `<collations>` tailoring rules;
3. the `<special xmlns:sil="urn://www.sil.org/ldml/0.1">` extension block (custom/PUA characters,
   combining-class overrides), when present.

PanGloss SHALL NOT source the edit-unit inventory from collation tailoring alone.

#### Scenario: A writing system declares multigraphs as exemplar brace-strings
- **WHEN** a writing system's `.ldml` main `<exemplarCharacters>` set contains UnicodeSet
  multi-character strings — e.g. a base letter plus a combining mark written `{...}`
- **THEN** PanGloss's extracted edit-unit inventory for that writing system includes every such
  string as a single orthographic unit, independently of whether the collation block declares it

#### Scenario: A writing system tailors collation for a multigraph
- **WHEN** a writing system's `.ldml` `<collations>` block defines tailoring that treats a
  multigraph (e.g. a digraph) as a single collation unit
- **THEN** PanGloss's extracted edit-unit data for that writing system includes that multigraph as
  a unit, merged with the units drawn from the exemplar sets

#### Scenario: A writing system's collation block is empty but its exemplar set declares units
- **WHEN** a writing system's `.ldml` `<collations><collation type="..."/></collations>` is present
  but self-closing (no tailoring rules), while its main `<exemplarCharacters>` set does contain
  multi-character strings
- **THEN** PanGloss reports the collation-derived edit-unit data as not specified and still extracts
  the exemplar-derived units — the not-specified state is per source, not one flag for the whole
  edit-unit inventory

#### Scenario: No source declares any multi-character unit
- **WHEN** a writing system's `.ldml` has an empty collation block, no SIL special orthographic
  elements, and no multi-character strings in either exemplar set
- **THEN** PanGloss reports that writing system's edit-unit inventory as not specified rather than
  inventing unit boundaries

### Requirement: Extraction never hard-fails the overall project import
Missing or partial writing-system LDML data, at the level of an individual field, an individual
tag's file, or the entire `WritingSystemStore/` folder, SHALL degrade to a warning and an explicit
not-specified state. PanGloss SHALL NOT abort or hard-fail the overall `.fwdata` project import
because writing-system LDML data is missing or incomplete.

#### Scenario: One writing system's LDML is malformed
- **WHEN** one writing-system tag's `.ldml` file exists but cannot be parsed
- **THEN** PanGloss records that tag's writing-system data as not specified, emits a warning
  identifying the file, and continues extracting the rest of the project normally, matching the
  existing warn-and-skip behavior for other malformed or dangling `.fwdata` references

### Requirement: Writing-system orthographic data is a separate snapshot section, independent of the grammar's phonological segment inventory
The extracted writing-system orthographic data SHALL be represented as its own `pg-snapshot`
section, keyed by writing-system tag, additive to the existing `project.vernacularWritingSystems`/
`analysisWritingSystems` tag lists. PanGloss SHALL NOT modify or merge this data into
`pg-grammar`'s existing `CharDefTable` (the grammar's phonological segment/boundary inventory) as
part of this extraction.

#### Scenario: A project has both grammar phonology and writing-system LDML data
- **WHEN** a project's `.fwdata` phonology records define a `CharDefTable` segment inventory and its
  `WritingSystemStore/` folder defines LDML orthographic data for the same writing system
- **THEN** PanGloss extracts both into their own separate representations without attempting to
  reconcile or unify them
