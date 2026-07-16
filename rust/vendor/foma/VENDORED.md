# Vendored: foma 0.1.1 (patched for wasm32)

- **Upstream:** https://github.com/divvun/foma-rs — crates.io `foma = "0.1.1"`
  (a Rust port of Mans Hulden's C foma).
- **License:** Apache-2.0 (declared in the package's `Cargo.toml`; the published
  crate ships no separate LICENSE file). This vendored copy is redistributed
  under the same license.
- **Provenance:** byte-for-byte copy of the published crates.io package
  (`~/.cargo/registry/src/index.crates.io-*/foma-0.1.1/`), minus registry
  artifacts (`.cargo-ok`, `.cargo_vcs_info.json`, `Cargo.toml.orig`), plus the
  deliberate changes below. Wired into the workspace via `[patch.crates-io]`
  in `rust/Cargo.toml`, so `hc-foma`'s `foma = "=0.1.1"` pin still resolves —
  every consumer gets this copy.

## Why this exists

`apply_init` (`src/apply.rs`) eagerly called `std::time::SystemTime::now()` to
seed the LCG used only by the `apply_random_*` family. On
`wasm32-unknown-unknown` that call aborts ("time not implemented on this
platform"), crashing every `apply_up`/`apply_down` caller at init — i.e. the
PanGloss browser demo panicked on the first word analyzed (foma-fst-plan gate
F4). Gate F0 only `cargo check`ed the wasm32 target, so the runtime abort was
latent until F4's runtime verification.

## Deliberate changes from the published package

1. `src/apply.rs` `apply_init`: the `SystemTime` seed is now behind
   `#[cfg(not(target_arch = "wasm32"))]`; on wasm32 a fixed seed (`0`) is used.
   Consequence: `apply_random_*` is deterministic on wasm32 (PanGloss never
   calls it); all other platforms and functions are byte-identical in behavior.
2. `Cargo.toml`: an empty `[workspace]` table, so the vendored copy is not
   claimed by the enclosing workspace.

## Exit criteria (drop this vendored copy when either lands)

- Upstream releases a version with the fix (re-run gate F0 against it, then
  delete `rust/vendor/foma` and the `[patch.crates-io]` entry), or
- the project moves off the crate entirely.

An upstream PR to divvun/foma-rs is the intended follow-up (John's call — it is
the only outward-facing piece of this fix).
