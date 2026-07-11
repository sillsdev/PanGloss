# HermitCrab Rust engine

Native Rust port of the low-level HermitCrab morphological parser — *words in, morphemes
out* — callable from .NET Framework 4.8 (FieldWorks) and switchable at runtime with the
managed engine. See [`../docs/history/rust-conversion.md`](../docs/history/rust-conversion.md) for the full plan.

## Layout (crate map — plan §5.1)

| Crate | Role |
|---|---|
| `hc-grammar` | HC XML load + lint + compile → immutable `GrammarTables` |
| `hc-featstruct` | bit-vector feature structures, interner, DAG unifier, variable bindings |
| `hc-shape` | shapes (struct-of-arrays), annotation spans, builders |
| `hc-fst` | pattern compile, FSA traversal, registers (CSR arc storage) |
| `hc-rules` | phonological + morphological rules, templates, strata, cascades |
| `hc-memo` | `AnalysisStateKey`, nogood + template memo, trail replay |
| `hc-parse` | Morpher pipeline: segment → analyze → lookup → synthesize → dedup |
| `hc-ffi` | C ABI (`cdylib`) with `catch_unwind` boundary |
| `hc-cli` | `hc-rs` binary: batch, parity-diff, bench (mirrors C# `hc batch` TSV) |

## Build

```
cargo build --release      # optimized cdylib + CLI
cargo test                 # unit + parity fixture tests
cargo clippy -- -D warnings
```

Requires the MSVC toolchain (`x86_64-pc-windows-msvc`); the linker is auto-located via
Visual Studio's vswhere.

## Status

Under active construction per the milestone plan. See `../docs/history/rust-conversion.md` §10 and the
session task list for current milestone.
