## Why

The optional C# founding-oracle comparison is useful but has a narrower input and stricter
signature contract than the Rust diagnostic. HermitCrab.Tool loads HC XML only, its CLI uses
`-i <grammar> -s <script>`, and conformance compares duplicate-sensitive multisets paired with
surface shape—not gloss sets.

## What Changes

- Add a non-colliding C# `gloss-batch` command producing the canonical five-column timed TSV with
  a gloss-keyed, surface-shape-paired multiset signature.
- Invoke it through the existing `-i/-s` script wrapper shape.
- Add diagnostic `--full`/PowerShell `-Full` comparison for `.xml` only; reject `.json` and
  `.fwdata` clearly.
- Compare Rust and C# gloss signatures using `PROTOCOL.md` §3–4 multiset semantics.
- Support an explicit two-pass delta comparison that reruns only participating grammar/engine sides
  with bounded tracing and emits machine-readable FieldWorks investigation handoffs without
  launching or depending on FieldWorks.

## Impact

This is a harness/evidence change only. It depends on the Rust gloss-signature artifact from
`add-grammar-diagnostics` and does not change deployment parsing. The C# utility remains source-only
developer/conformance tooling in this repository and is not distributed with PanGloss Runtime or
the PanGloss SDK.
