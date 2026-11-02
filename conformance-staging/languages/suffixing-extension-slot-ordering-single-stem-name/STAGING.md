# suffixing-extension-slot-ordering-single-stem-name

The oracle pin's `languages/suffixing-extension-slot-ordering` (`aab4c598`) under a distinct name: the suite rebuilt the stem names of `languages/suffixing-extension-slot-ordering`, so it no longer has exactly one. See
`conformance/docs/engine-only-fixture-retirement.md`.

`rust/crates/pg-foma-backend/tests/orthogonal_basis_group_a.rs` still pins the old shape (the `require`
polarity, or the single one-region stem name). The grammar and `words.yaml` are the oracle pin's bytes,
unchanged. It is a `local_regression`: PanGloss keeps the engine behavior, it is not proposed upstream and
must not graduate. The graduation guard forbids one name in both roots, which is why this copy has its own name.
