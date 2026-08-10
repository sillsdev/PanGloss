# Staging evidence

This synthetic fixture is a generic, language-neutral optimizer witness for: Complete-template alternatives and deeply nested copying morphology.

It is derived from an existing checked-in conformance shape, with language-identifying names removed. The `words.yaml` oracle is replayed by the shared PanGloss HC conformance harness. Promotion requires oracle parity, intended grammar-fact assertions, and either a content-distinct buildable backend or an explicit elimination report.

## Bounded optimizer characterization

`rust/crates/pg-cli/tests/four_grammar_recipe_evidence.rs` selects the first checked-in word, `k`, as a deterministic bounded characterization observation. This measures the real production registry/materializer/capability/build/evaluation path without making the intentionally pathological `C(12,6) = 924` midpoint part of the pilot. The complete three-word fixture remains authoritative for full-HC identity and multiplicity in the conformance harness. Exact backend-space counts therefore do not imply that the bounded run certified the full fixture corpus.

Verification:

- `cargo test -p pg-foma --test backend_promoted_fixtures`
- `cargo test -p pg-cli --test four_grammar_recipe_evidence`
- `cargo test -p pg-parse --test conformance_fixtures_gate`
