# Staging evidence

This synthetic fixture is a generic, language-neutral optimizer witness for: Ordered rules, metathesis, copying, and multiple strata.

It is derived from an existing checked-in conformance shape, with language-identifying names removed. The `words.yaml` oracle is replayed by the shared PanGloss HC conformance harness. Promotion requires oracle parity, intended grammar-fact assertions, and either a content-distinct buildable recipe or an explicit elimination report.

Verification:

- `cargo test -p pg-foma --test recipe_promoted_fixtures`
- `cargo test -p pg-parse --test conformance_fixtures_gate`
