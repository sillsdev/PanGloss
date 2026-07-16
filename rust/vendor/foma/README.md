# foma (Rust port)

An idiomatic Rust re-implementation of [**foma**](https://fomafst.github.io/),
Mans Hulden's finite-state compiler and C library for building and applying
finite-state automata and transducers. It compiles regular expressions and
`lexc`/`xfst`-style sources into finite-state networks and applies them to
strings — the toolkit behind a great deal of computational morphology and
phonology work.

This repository contains:

- `crates/foma/` — the Rust port (library `foma` + three binaries).
- `docs/spec/port/` — the behavioral specification (per-function `def`/`sem`
  rules) that pins the port to the C behavior of
  [upstream foma](https://github.com/mhulden/foma), which served as the
  porting reference.
- `docs/port/rust-conventions.md` — the conventions the port was built under.

## Status

The port is complete. It was built in four waves:

1. **Literal port** — every C function translated 1:1 (bug-for-bug) into a
   matching Rust module, one module per C file.
2. **Spec** — a per-function behavioral spec (`def` + `sem` rules) extracted
   from the C, under `docs/spec/port/`.
3. **Tests** — **545 tests** pinning that spec, including the documented C
   quirks, so later refactors diff against a known oracle.
4. **Idiomatization** — the literal port reshaped into idiomatic Rust
   (`Result`-based errors, iterator front-ends, owned handles instead of
   global mutable state) while keeping every spec rule covered and all 545
   tests green.

The flex/bison regex and `lexc` grammars are replaced by the sibling parser
crates `nfst-xre` and `nfst-lexc`; the port walks their typed ASTs and calls
the same construction routines the C grammar actions would.

## Using

The crate is on [crates.io](https://crates.io/crates/foma):

```sh
cargo add foma         # library dependency
cargo install foma     # the foma / flookup / cgflookup binaries
```

To build from a checkout of [divvun/foma-rs](https://github.com/divvun/foma-rs):

```sh
cargo build            # builds the library and all three binaries
cargo test -p foma     # runs the full test suite (545 tests)
```

The regex and lexc parsers come from the
[`nfst-xre`](https://crates.io/crates/nfst-xre) and
[`nfst-lexc`](https://crates.io/crates/nfst-lexc) crates
([`necessary-nu/nfst`](https://github.com/necessary-nu/nfst)).

## Binaries

| Binary      | Purpose |
|-------------|---------|
| `foma`      | Interactive REPL / script interpreter: compile regexes, define networks, apply strings, load/save binaries. |
| `flookup`   | Batch application of a saved network to input lines (down/up lookup). |
| `cgflookup` | `flookup` variant emitting Constraint Grammar-style output. |

### Example session

```sh
$ cargo build
$ ./target/debug/foma
foma[0]: regex [c a t]:[d o g];
407 bytes. 4 states, 3 arcs, 1 path.
foma[1]: print words
c:da:ot:g
foma[1]: apply down cat
dog
foma[1]: apply up dog
cat
foma[1]: quit
```

`regex …;` compiles a transducer mapping `cat` → `dog`; `apply down` runs a
string through the input side, `apply up` through the output side.

## Library API

The crate exposes the C surface names with idiomatic Rust signatures. A sketch:

```rust
use foma::regex::fsm_parse_regex;
use foma::apply::apply_init;
use foma::io::fsm_write_binary_file;

// Compile a regular expression into a network.
let net = fsm_parse_regex("[c a t]:[d o g]", None, None).unwrap();

// Apply strings through it.
let mut h = apply_init(&net);
for output in h.down("cat") {
    println!("{output}"); // "dog"
}

// Persist it in foma's binary format.
fsm_write_binary_file(&net, "cat2dog.fst");
```

Module map (mirrors the C files): `regex` (compiler front-end), `apply`
(string application + iterator front-ends), `constructions` (automata/transducer
algebra), `determinize`/`minimize` (canonicalization), `io` (binary and text
network I/O), `lexcread` (lexc reader), `define` (named-network registry),
`structures`/`sigma`/`dynarray` (core representation). See the crate docs
(`cargo doc -p foma --open`) for the full surface.

## Specification

The port's behavior is specified rule-by-rule under `docs/spec/port/`. Every
ported function carries `// [spec:foma:def:…]` / `// [spec:foma:sem:…]`
annotations tying the code to its rule, and every rule is verified by a test
carrying the matching `…/test` facet. The conventions that governed the port
are in `docs/port/rust-conventions.md`.

## License

Apache-2.0, matching upstream foma. See [`LICENSE-APACHE`](LICENSE-APACHE).

This is a port; all credit for the original design, algorithms, and C
implementation goes to **Mans Hulden** and the foma project
(<https://fomafst.github.io/>).
