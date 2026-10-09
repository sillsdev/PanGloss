# 082 — Inflectional circumfixes require the prefix half's classes

## Kind

Behavioural (Rust snapshot compiler).

## Status

Fixed-in-rust; source-backed FieldWorks loader correction.

## C# site

FieldWorks `HCLoader.LoadCircumfixAffixProcessAllomorph`, lines 1055–1069 in the source
review recorded by `review/parsimony-fixes` commit `2eea9c55`. For an inflectional MSA
the prefix half supplies required inflection classes. The suffix half's classes are unread.

## Rust site

`pg-grammar/src/compile/affixes.rs::build_circumfix_allomorphs` calls
`read_inflection_classes` for the prefix and unions the resulting bits with required MPR
features. It records applied, unresolved and ignored gate outcomes at the deciding builder.

## What differed

Circumfix construction used only the MSA's required MPR features and ignored both halves'
classes. A class-B prefix consequently admitted a class-A stem. Applying suffix classes too
would overcorrect the defect by refusing forms FieldWorks admits.

## Pinning fixture and conformance coverage

`pg-grammar/tests/circumfix_inflection_classes.rs` constructs synthetic Snapshot cases
with restricted prefixes, restricted suffixes, and an unrestricted control. Its class-A and
class-B stems test both lost and unwanted parses. This is a Snapshot-to-parser integration
test, not a new exported HC-XML fixture or an execution of FieldWorks' loader. Managed
gate and owner-revert evidence are recorded in the integration handoff report.

## Upstream

No Machine correction is proposed: the FieldWorks loader already applies this rule.
A new executable FieldWorks comparison remains unmeasured in this lane.
