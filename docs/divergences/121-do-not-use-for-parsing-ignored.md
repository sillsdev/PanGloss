# 121: `DoNotUseForParsing` is ignored, as in FieldWorks

Kind: decision record (representational: PanGloss reads the flag nowhere, so there is no parse difference).
Status: permanent decision, director ruling of 2026-11-03 (issue #154). PanGloss 0.11.0 to 0.14.0 dropped flagged entries at import; that was a divergence from FieldWorks and is removed.

## Decision

A FieldWorks lexical entry flagged `DoNotUseForParsing` is imported and parsed exactly like an unflagged one. The importer neither drops the entry nor its forms, senses and MSAs, nor ad hoc prohibitions that name them, nor prunes variant or complex-form components. The flag is not read, and a malformed value does not refuse import.

## C# site

No parser reads the flag. A search of FieldWorks `Src` finds `DoNotUseForParsing` only in the LIFT exporter and merger (`LiftExporter.cs:271`, `LiftMerger.cs:2255-2259, 2429-2432`) and in test data; it is absent from ParserCore (`HCParser.cs`), HCLoader and XAmple. Machine never references it. liblcm only copies it when cloning an entry (`OverridesLing_Lex.cs:1662`). So after Apply FieldWorks parses flagged entries, and the parser input of every engine contains them.

## FieldWorks runs

`evidence/121/` holds real FieldWorks 9.3.10.1452 runs (HermitCrab 3.8.2.0, XAMPLE 3.12.23.21) on two one-entry projects that differ only in `<DoNotUseForParsing val="False" />` against `"True"`. Both HermitCrab (`HCLoader.Load`, `Morpher.ParseWord`) and XAMPLE (`xample.dll`) return 1 analysis of `pat` for the flagged and the unflagged project, and the generated HC XML and XAMPLE `adctl`, `gram` and `lex` files are byte-identical (`evidence/121/fieldworks/projection-sha256.txt`). The `pangloss/` column of that evidence is PanGloss at v0.14.0, before this fix (0 analyses for the flagged project). Method and limits: `evidence/121/README.md`. Run on 2026-11-02 in the closed docs PR #135.

## Rust site

None: `pg-fwdata/src/extract/lexicon.rs` no longer inspects the flag. The `LoadReasonCode::DoNotUseForParsing` variant (`doNotUseForParsing`) stays in `pg-snapshot` only so snapshots written by 0.11.0 to 0.14.0 still deserialize; nothing emits it.

## Verification

- Witness `conformance-staging/edge-cases/do-not-use-for-parsing-flag-ignored/`: `grammar.xml` and `words.yaml` are the oracle-recorded unflagged grammar; the FieldWorks project flags two of its entries; `pg-parse/tests/adhoc_fieldworks_witnesses.rs` requires the same analysis counts.
- `pg-fwdata/tests/fixture_tests.rs::do_not_use_for_parsing_is_ignored_like_fieldworks` and `pg-parse/tests/do_not_parse.rs` compare flagged against unflagged imports.
