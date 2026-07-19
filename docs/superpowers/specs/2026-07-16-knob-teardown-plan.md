# Plan: tear out the FST precision knob (keep the ConstraintCatalog)

Status: PLANNED 2026-07-16 (John's decision). Execute AFTER the round-2 perf commits land on
main (merge_states / arc-sort / parallel-confirm) — their identity checks reference the
AllFlags cells this plan deletes.

## Decision and rationale

The knob is settled: **Strip (minimal, maximally permissive FST + full HC prune) is the
permanent and only setting.** Evidence, all recorded in
`2026-07-15-fst-precision-knob-design.md` §9 and the 2026-07-16 knob exercise:

- `FullCompile`/`Auto` are emit-identical stubs to Strip (byte-identical lexc, all 3 grammars).
- `AllFlags` only differs on Sena (20/72 provable constraints): 8.4x lexc, 4.5x compile,
  ~0.5x propose, candidate precision 0.0504 → 0.0506 (nil).
- Both growth paths are dead by measurement: the Eliminate tuner has zero eligible candidates
  by construction (@P@ family is non-eliminable), Compose is an architectural mismatch
  (0/30 composable; emitter is boundary-stripped + phonology-pre-resolved).

Precision work moves OUT of the network into a deterministic post-propose candidate
pre-filter (see `2026-07-16-candidate-prefilter-plan.md`). The teardown deliberately KEEPS
the `ConstraintCatalog` machinery — enumerating/classifying environment-constraint instances
is exactly the input the pre-filter needs.

## Scope — what goes, what stays

Files touching the knob surface (grep `PrecisionConfig|PrecisionEmit|ConstraintCatalog|AllFlags|FullCompile`):

| File | Action |
|---|---|
| `rust/crates/pg-foma/src/precision.rs` | **Slim, don't delete.** Keep `ConstraintCatalog`, the `classify`/`EnvCoverage` analysis, and the module docs (adjacency finding, foma-rs flag traps — this is recorded institutional knowledge). Remove `PrecisionAction`, `PrecisionConfig`, `flag_id`, and all flag-symbol EMISSION machinery. |
| `rust/crates/pg-foma/src/emit.rs` | Remove `emit_with_precision` / the AllFlags hookup; plain `emit` (today's Strip behavior) becomes the only path. Emitted lexc must be byte-identical to current Strip output. |
| `rust/crates/pg-foma/src/lib.rs` | Drop removed re-exports. |
| `rust/crates/pg-foma/tests/pk1_precision_recall_invariance.rs` | **Delete** (its subject — AllFlags recall invariance — no longer exists). |
| `rust/crates/pg-foma/tests/pk2_eliminate_flag_oracle.rs` | **Keep.** It is an oracle over VENDORED FOMA flag semantics (U/R/D vs E/N/C/P behavior), independent of the knob; it guards the vendor port. If it imports removed knob types, rewrite those imports, not the assertions. |
| `rust/crates/pg-foma/examples/precision_bench.rs` | Remove the preset matrix axis; keep it as a single-configuration (Strip) bench. Do not rename. |
| `rust/crates/pg-foma/examples/knob_probe.rs` | UNTRACKED fixture — do NOT commit changes to it. Note in the report that its `min|mid|max` arg becomes meaningless; the main-loop reviewer updates the main-checkout copy after merge. |
| `docs/superpowers/specs/2026-07-15-fst-precision-knob-design.md` | Add a status header: knob TORN OUT 2026-07-16 per this plan; findings in §9 remain the record; ConstraintCatalog survives for the candidate pre-filter. |

Do NOT touch: the vendored foma flag machinery (`rust/vendor/foma`), pg-rules/pg-parse,
`pg_grammar::model` environments (the pre-filter needs them).

## Identity bar (all must pass before commit)

1. **Strip emit byte-identity:** lexc hash for sena/indonesian/amharic identical before vs
   after teardown (use `knob_probe <grammar> min`'s `lexc_hash`, or a minimal emit driver).
   This is the non-negotiable gate: teardown is dead-code removal, not behavior change.
2. `cargo test -p pg-foma --release` green (pk1 deleted, pk2 green).
3. `cargo build --workspace --release` green (no dangling imports elsewhere).
4. **wasm runtime smoke** — emit.rs is on the emit/proposer path: run
   `node rust/tools/f4-wasm-smoke.js` (cargo check --target wasm32 does NOT catch runtime
   aborts; this bit us twice).

## Process (repo conventions)

Worktree agent: `git merge --ff-only main` first; copy fixtures (`samples/data/*.xml`,
`*-words.txt`) and `rust/crates/pg-foma/examples/knob_probe.rs` from the main checkout
(never commit them); commit to the worktree branch with the
`Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` trailer; report BEFORE/AFTER
lexc hashes + test results. Main loop reviews the diff and cherry-picks.
