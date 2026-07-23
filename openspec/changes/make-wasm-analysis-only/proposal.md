## Why

The current WASM API constructs foma networks from grammar XML and recompiles them after user-lexicon
changes. PanGloss now assigns compilation exclusively to supervised native tooling. Leaving dormant
compiler code in WASM would preserve an unsafe, unsupported second compilation environment.

## What Changes

- Define one self-contained, versioned analysis file around the existing foma binary-memory
  representation.
- Produce the artifact through native supervised tooling and load it in WASM.
- Package both the precompiled FST proposer and the matched runtime grammar data needed by the Rust
  HermitCrab port, bound by one package fingerprint.
- Define the `.pgpack` as a data-only PanGloss Language Pack/runtime plugin; executable extension
  code remains exclusively in PanGloss Runtime.
- Remove WASM grammar/FST construction, recompilation exports, and compiler dependencies.
- Preserve bounded analysis and permit separately supplied stem data without engine mutation.
- Provide optional package license declarations and publisher signatures as non-enforcing WASM
  deployment metadata; unsigned packages remain usable and no license server is contacted.

## Impact

WASM remains an analysis runtime, not a grammar-authoring or FST-compilation runtime. The single
file runs the FST proposal stage and the Rust HermitCrab confirmation/full-analysis stage. Grammar
and engine changes require a new native artifact. Browser callers receive explicit compatibility
errors rather than triggering FST compilation.
