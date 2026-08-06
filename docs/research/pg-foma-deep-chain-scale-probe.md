# `deep_chain_scale_probe` — reproducing the apply_up chain explosion synthetically

`rust/crates/pg-foma/examples/deep_chain_scale_probe.rs` checks whether a synthetic deep
standalone-affix chain (`pg_grammar_gen::build::chain`) reproduces the real-language `apply_up`
non-termination/OOM this shape is known to trigger, without needing a real-language corpus at all.

## Root cause

`pg_foma::emit::build_deriv_chain`'s legacy `TextMode::SurfaceProbed` strategy lets every level
offer every rule: depth = `rules.len()` for a grammar's standalone (stratum-attached, non-template)
prefix/suffix rules. That strategy is what the mainline `emit()` path (used by every reference
grammar via `FomaProposer::new`) uses unconditionally; a chain-restriction fix
(dedicated-level-per-rule) exists but applies only under a different text mode reached by a
templated construction path, never by this one.

A grammar with `N` independent standalone suffix rules therefore builds an `N`-level chain where
each of the `N` levels independently offers all `N` rules — the same rule can be "chosen" at any of
`N` levels, so a single target surface string using `k` of the `N` rules (in order) is reachable via
`C(N, k)` distinct raw `apply_up` paths, all decoding to the identical candidate.
`pg_grammar_gen::build::chain` reproduces exactly that shape, sized only by `N` (capped at 25 by the
26-ASCII-letter ceiling for a single table) — no corpus needed.

## What is measured, and how

Two axes, measured separately, never assumed:

1. **Compile-time resource envelope** (states/arcs/lexc-lines/compile-wall-time) via
   `FomaProposer::new_with_profile`, the mainline production path.
2. **Apply-time behavior** on a deliberately maximally-ambiguous query word (root shape plus every
   other rule's own suffix character, in order — using `k = N/2` rules out of `N` maximizes
   `C(N, k)`, since using all `N` collapses to exactly one placement with no freedom left):
   - the unbounded `FomaProposer::propose` call, wall-clock timed on a background thread with a
     hard cutoff — the cutoff itself is the "did it explode" signal, not a correctness bound;
   - the same word through `FomaProposer::propose_budgeted` with a small `ApplyBudget`, to check
     whether the shipped apply-path containment guard catches this specific vector fast.

The unbounded call is deliberately allowed to time out; timing out at some `N` is a valid,
expected measurement outcome, not a probe failure.
