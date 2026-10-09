# 081 — Inflectional process allomorphs require their classes

## Kind

Behavioural (Rust snapshot compiler).

## Status

Fixed-in-rust; source-backed FieldWorks loader correction.

## C# site

FieldWorks `HCLoader.cs:1095–1096`, in `LoadAffixProcessAllomorph`.
An inflectional MSA unions the
allomorph's inflection classes into the required MPR features. A derivational MSA does not.

## Rust site

`pg-grammar/src/compile/affixes.rs::build_affix_allomorphs_for` resolves classes before
dispatching to either the literal-form or process builder. `build_process_allomorph`
unions those classes with the rule's required MPR features.

## What differed

The process branch returned before reading inflection classes. A class-B process suffix
therefore admitted a class-A stem, although the equivalent ordinary suffix refused it.
The correction retains the selected-writing-system form resolution introduced on main.

## Pinning fixture and conformance coverage

`pg-grammar/tests/process_allomorph_inflection_classes.rs` constructs synthetic Snapshot
twins: ordinary and process suffixes, class-A and class-B stems, and an inflectional versus
derivational MSA. `process_allomorph_respects_inflection_class` requires one `pitu` parse
and zero `katu` parses. The ordinary suffix and derivational process are negative controls.
This is a Snapshot-to-parser integration test, not an exported HC-XML fixture or a new
execution of FieldWorks' loader. Managed gate and owner-revert evidence are recorded in
the integration handoff report.

## Upstream

No Machine correction is proposed: its FieldWorks loader already applies these classes.
The source review establishes the loader contract; a new executable FieldWorks comparison
remains unmeasured in this lane.
