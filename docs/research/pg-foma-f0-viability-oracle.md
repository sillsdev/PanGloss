# `f0_viability`'s C-foma fidelity oracle

`rust/crates/pg-foma/tests/f0_viability.rs`'s `print_toy_lexc_for_oracle` and
`print_rule_lexc_for_oracle` are `#[ignore]`d tests that dump the exact lexc source strings this
file compiles internally (byte-for-byte, via the same `toy_lexc()`/`rule_lexc()` builders the real
tests use), for feeding to the official C foma CLI (`mhulden/foma`) as a side-by-side comparison
against `foma-rs`'s `apply_up` output. They are not part of the normal test run: no network
dependency, no binary checked into the repo. Run manually with `cargo test -p pg-foma --test
f0_viability -- --ignored --nocapture <name>` and redirect the dumped source into a `.lexc` file
next to the C foma binaries.

Doing this once found: C foma's `flookup` (default direction = surface->analysis, matching this
crate's `apply_up`) reproduced the exact same analysis sets as `foma-rs` for every word in
`toy_lexc()`. The composed rule network (`rule_lexc() .o. "N -> m || _ [p|b]"`) initially appeared
to mismatch, but reproduced the same result once the CLI's `compose net` stack-operand order was
accounted for — a CLI stack-ordering detail, not a semantic divergence between the two
implementations.
