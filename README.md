# PanGloss

A Rust morphological/phonological parsing engine, starting from a native port of
[HermitCrab](https://github.com/sillsdev/machine)'s parser — *words in, morphemes out* — and
growing toward a broader FST-based toolkit (spell-checking, hybrid finite-state approaches, and
more) beyond HermitCrab's original scope.

## Origin

`rust/` here began as a from-scratch Rust reimplementation of `SIL.Machine.Morphology.HermitCrab`
(the C# morphological parser used by SIL's FieldWorks/FLEx), built to be callable directly or
switched in as a drop-in engine wherever HermitCrab is used today. It was extracted from
[`sillsdev/machine`](https://github.com/sillsdev/machine) as a single squashed commit (no line
history carried over — the original repo keeps the full commit-by-commit record if it's ever
needed). See [`docs/hermitcrab-rust-port-audit.md`](docs/hermitcrab-rust-port-audit.md) for exactly
what was ported, what's known to still differ, and the process for tracking HermitCrab's ongoing
evolution against this codebase.

**The parity contract is not this repo's own history or corpora.** As HermitCrab in `Machine`
evolves, the binding correctness gate is the conformance oracle that lives there (pulled in here as
a submodule once available) — PanGloss's own algorithms are free to diverge internally as its scope
grows, as long as they keep passing that oracle. `samples/` and `docs/history/` are historical
reference, not living gates.

## API parity goal

PanGloss aims to keep a public surface recognizably similar to HermitCrab's — so that, where the
two engines' scopes overlap, something already calling HermitCrab could call PanGloss instead with
minimal changes. PanGloss is meant to be called directly (not through `Machine`'s C# layer), so this
is about a familiar *shape*, not wire compatibility with any specific C# interop path.

## Layout

- `rust/` — the ported engine (crate map and build instructions in
  [`rust/README.md`](rust/README.md)).
- `samples/data/` — reference grammars (Amharic, Indonesian, Sena) used by `rust/`'s test suite.
- `docs/` — the port audit (above) plus historical planning documents under `docs/history/`.

## Building

```
cd rust
cargo build --release
cargo test
cargo clippy --workspace --all-targets
```

## License

MIT — see [`LICENSE`](LICENSE).
