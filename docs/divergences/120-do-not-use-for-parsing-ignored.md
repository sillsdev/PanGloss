# 120: `DoNotUseForParsing` is ignored, as in FieldWorks

Kind: decision record (representational: PanGloss reads the flag nowhere, so there is no parse difference).
Status: permanent decision, director ruling of 2026-11-03 (issue #154). PanGloss 0.11.0 to 0.14.0 dropped flagged entries at import; that was a divergence from FieldWorks and is removed.

## Decision

A FieldWorks lexical entry flagged `DoNotUseForParsing` is imported and parsed exactly like an unflagged one. The importer neither drops the entry nor its forms, senses and MSAs, nor ad hoc prohibitions that name them, nor prunes variant or complex-form components. The flag is not read, and a malformed value does not refuse import.

## C# site

No parser reads the flag. A search of FieldWorks `Src` finds `DoNotUseForParsing` only in the LIFT exporter and merger (`LiftExporter.cs:271`, `LiftMerger.cs:2255-2259, 2429-2432`) and in test data; it is absent from ParserCore (`HCParser.cs`), HCLoader and XAmple. Machine never references it. liblcm only copies it when cloning an entry (`OverridesLing_Lex.cs:1662`). So after Apply FieldWorks parses flagged entries, and the parser input of every engine contains them.

## Rust site

None: `pg-fwdata/src/extract/lexicon.rs` no longer inspects the flag. The `LoadReasonCode::DoNotUseForParsing` variant (`doNotUseForParsing`) stays in `pg-snapshot` only so snapshots written by 0.11.0 to 0.14.0 still deserialize; nothing emits it.

## Verification

- Witness `conformance-staging/edge-cases/do-not-use-for-parsing-flag-ignored/`: `grammar.xml` and `words.yaml` are the oracle-recorded unflagged grammar; the FieldWorks project flags two of its entries; `pg-parse/tests/adhoc_fieldworks_witnesses.rs` requires the same analysis counts.
- `pg-fwdata/tests/fixture_tests.rs::do_not_use_for_parsing_is_ignored_like_fieldworks` and `pg-parse/tests/do_not_parse.rs` compare flagged against unflagged imports.
