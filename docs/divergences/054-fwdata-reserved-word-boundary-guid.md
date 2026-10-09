# 054 — `.fwdata` word-boundary identity was inferred from a missing marker record

## Kind
Behavioural.

## Status
Fixed-in-rust.

## C# site
FieldWorks `HCLoader.cs:2351/2489-2498` identifies the special anchor from
`LangProjectTags.kguidPhRuleWordBdry`. `LoadCharacterDefinitionTable` excludes that GUID from
ordinary boundary markers (`HCLoader.cs:2698`).

## Rust site
`pg-fwdata/src/extract/phonology.rs::resolve_phon_context` now maps that GUID to
`PhonContext::WordBoundary` directly, regardless of whether a referenced record resolves.
`extract_phoneme_set` omits the reserved GUID from `boundary_markers`. Other marker GUIDs still
resolve as `PhonContext::Boundary` and remain in the ordinary marker table.

## What differed
The importer treated any resolved `PhBdryMarker` record as a literal boundary. Some real
FieldWorks exports store the reserved word-boundary GUID as an owned `PhBdryMarker`, so a rule
context referencing that record became a literal marker instead of the word edge. The emitted
snapshot could therefore fail to match word-initial or word-final rules. The same record also
leaked into the ordinary `boundary_markers` table.

## Correct behavior
Boundary kind follows the referenced GUID. `LangProjectTags.kguidPhRuleWordBdry` is always the word
edge, including when a record exists. A user-created marker whose representation is `#` remains a
literal boundary because its GUID differs from the reserved GUID.

## Pinning fixture and conformance coverage
`pg-cli/tests/reserved_word_boundary.rs::reserved_word_boundary_record_is_a_word_anchor_and_literal_hash_remains_literal`
constructs a stored, phoneme-set-owned reserved marker and a rewrite context `r → t / #_V` from
the synthetic FWData fixture. It checks the snapshot context and marker table, compiles the project,
and checks that prefix `r` plus root `a` parses as `ta`. A literal-marker twin points the context at
the ordinary `#` marker and checks that `ta` does not parse. The test failed before the fix because
the reserved record appeared in the ordinary marker table.

The existing staged oracle witnesses are
`conformance-staging/edge-cases/word-boundary-affix-rewrite/` and
`conformance-staging/edge-cases/literal-hash-affix-context/`. They exercise word-edge rewriting and
literal-hash behavior through the HC-XML oracle path. Their presence is fixture evidence; the new
FWData integration test is the importer-path regression. The five copied sample projects provide
additional before/after context-reference counts and parse rows; see the implementation report for
the measured results.

## Evidence and limits
The pinned Machine source is commit `18cf242f4b114b0eb9bac304b4b171ca2f499a39`. Its HC-XML
word-edge rewrite and literal-hash fixtures both pass the C# harness. A scratch environment grammar
and a scratch Morphology-to-Clitics grammar also produce matching C# and PanGloss outputs. HCLoader
recognizes the reserved GUID and excludes it from the marker table, leaving a separate user `#`
marker as the only literal `#` row.

The integrator's exact Motif `.fwdata` was not available to reproduce. The installed PanGloss 0.7.0
binary also did not parse `ta` from this lane's minimal generated FWData, while the changed importer
does. That establishes the importer behavior on the regression fixture but does not prove the exact
reported Motif file changed behavior. The change is confined to the Rust `.fwdata` importer; no
`pg-parse` or `pg-rules` code changed. No Machine issue or upstream change is indicated.

## Upstream
No Machine issue or upstream change is needed. This corrects PanGloss import behavior to match the
existing FieldWorks loader contract.
