# `hc-*` → `pg-*` rename

Date: 2026-07-17
Status: approved, in progress
Branch/worktree: `pg-rename` (`.claude/worktrees/pg-rename`)

## Why

PanGloss's Rust crates, binary, and FFI library still carry their original `hc-*`
(HermitCrab-port) names from when this workspace began as a straight port of
`SIL.Machine.Morphology.HermitCrab`. Ahead of planning CI/CD and a public 0.1 release, the
project is being renamed to consistently use `pg-*` (PanGloss), matching the two crates that
already got it right (`pg-snapshot`, `pg-fwdata`). This is the cheapest point to do it: before
any public package (crates.io/npm/PyPI) or release binary exists under the old names.

This rename is a prerequisite for the CI/CD and release design (which will name packages,
binaries, and artifacts) — it lands first, as its own branch/PR, verified green on its own.

## Scope

**Exact token mapping** (not a blind `hc` substring replace — `HermitCrab` as a proper noun,
referring to the upstream SIL C# engine this was ported from, is never touched):

| Old | New |
|---|---|
| crate dir `hc-grammar` | `pg-grammar` |
| crate dir `hc-featstruct` | `pg-featstruct` |
| crate dir `hc-shape` | `pg-shape` |
| crate dir `hc-foma` | `pg-foma` |
| crate dir `hc-fst` | `pg-fst` |
| crate dir `hc-rules` | `pg-rules` |
| crate dir `hc-memo` | `pg-memo` |
| crate dir `hc-parse` | `pg-parse` |
| crate dir `hc-ffi` | `pg-ffi` |
| crate dir `hc-cli` | `pg-cli` |
| crate dir `hc-realize` | `pg-realize` |
| crate dir `hc-lexicon` | `pg-lexicon` |
| crate dir `hc-wasm` | `pg-wasm` |
| Rust import path `hc_grammar::…` (and the 12 other matching `hc_*` module names) | `pg_grammar::…` etc. |
| CLI binary name `hc-rs` | `pangloss` |
| FFI cdylib name / Rust lib name / DllImport string `hermit_crab` | `pangloss` |
| `pg-snapshot`, `pg-fwdata` | unchanged (already correct) |

Not touched:
- `docs/hermitcrab-rust-port-audit.md` — filename and prose correctly refer to the *upstream*
  HermitCrab C# engine/algorithm, a distinct thing from our crate names.
- `docs/history/*`, `reports/*`, loose root `*.txt` session-transcript files — archival record
  of what was true when written; not rewritten.
- `machine/` — a git submodule (the upstream conformance oracle); out of scope entirely.
- Gitignored sample grammar data (`samples/data/*`).

## Mechanics

1. `git mv` each crate directory (preserves per-file git history) to its `pg-*` name.
2. `rust/Cargo.toml`: update `[workspace].members` paths and `[workspace.dependencies]`
   entries (both the path and the `name` key) for all 13 renamed crates.
3. Per-crate `Cargo.toml`: `[package] name`; `pg-cli`'s `[[bin]] name = "hc-rs"` →
   `"pangloss"`; `pg-ffi`'s `[lib] name = "hermit_crab"` → `"pangloss"`.
4. Token-exact find/replace across all live `.rs`/`.toml`/`.md` files (excluding the archival
   paths above): the 13 hyphenated crate names, the 13 underscored module names, `hc-rs`, and
   `hermit_crab` — each matched as a whole token.
5. `rust/dotnet-harness/HcFfiHarness/Program.cs` and `.csproj`: update the `LibraryName` const
   and any comments referencing `hermit_crab.dll`.
6. `README.md` / `rust/README.md`: update crate-map tables and CLI usage examples (`hc-rs
   parse …` → `pangloss parse …`).
7. Live (non-archival) docs under `docs/` and `rust/docs/` that describe current architecture:
   same token-exact replacement.

## Verification

- `cargo build --workspace --all-targets`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all -- --check`
- Manual smoke: rebuild `dotnet-harness/HcFfiHarness` against the renamed `pangloss.dll` and
  confirm it still runs against a sample grammar.
- `grep` sweep afterward for any surviving bare-word `hc-` / `hc_` / `hermit_crab` occurrences
  outside the excluded archival paths, to catch anything the mechanical pass missed.

Executed as one continuous, build-verified pass in the `pg-rename` worktree/branch (not split
across parallel agents — a workspace-wide rename must stay compilable at each step; concurrent
independent edits would risk colliding on the same shared `Cargo.toml`/import graph).

## Out of scope (tracked separately)

CI/CD workflow updates, release packaging (GitHub Releases, crates.io/npm/PyPI), and the
Python-bindings question are a separate design, to follow once this rename lands on `main`.
