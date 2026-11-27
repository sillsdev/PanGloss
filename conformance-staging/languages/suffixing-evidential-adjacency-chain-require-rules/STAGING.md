# suffixing-evidential-adjacency-chain-require-rules

The oracle pin's `languages/suffixing-evidential-adjacency-chain` (`aab4c598`) under a distinct name: the suite replaced the four `require` co-occurrence rules of `languages/suffixing-evidential-adjacency-chain` with producible ones. See
`conformance/docs/engine-only-fixture-retirement.md`.

`rust/crates/pg-foma-backend/tests/orthogonal_basis_group_a.rs` still pins the old shape (the `require`
polarity, or the single one-region stem name). The grammar and `words.yaml` are the oracle pin's bytes,
unchanged. It is a `local_regression`: PanGloss keeps the engine behavior, it is not proposed upstream and
must not graduate. The graduation guard forbids one name in both roots, which is why this copy has its own name.
