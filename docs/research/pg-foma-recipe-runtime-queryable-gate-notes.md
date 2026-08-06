# pg-foma recipe_runtime_net_is_queryable_gate.rs: design notes moved out of comments

Longer arguments pulled out of
`rust/crates/pg-foma/tests/recipe_runtime_net_is_queryable_gate.rs` so the source can carry a
one- or two-line pointer instead of the full argument.

## What this gate exists to catch

`recipe_runtime::evaluate_plans` builds each candidate through `build::build_controllable` (a
`Plan` interpreter), then queries it through the production propose→confirm pipeline. Two defects
together made every real-grammar measurement meaningless without anything in the suite noticing:

1. The mandatory finish step was missing. `gate::compile_gated_grammar_with_budget`'s own doc
   states that a caller further composing its result needs its own final minimize over a
   boundary-cleanup net; that step existed only as an open-coded copy inside
   `tests/p6_gate_parity.rs`, so the production caller omitted it and scored every candidate
   against a net still carrying `uflexc`'s inter-morph boundary tokens. Measured on the Indonesian
   corpus: 0 of 3 candidates confirmed → 3 of 3, proposals 51 → 131, once
   `build::finish_controllable_net` was applied.
2. Nothing cross-checked the two build paths: `build.rs` proves `build_controllable` equivalent to
   `gate.rs`'s direct compile for the controllable subtree, which is precisely the equivalence that
   cannot catch this, since both sides of it are pre-finish nets.

## Why this is not reproducible on any synthetic fixture yet, and what would fix that

Verified directly: with the finish step bypassed and then restored, every staged fixture declaring
a `Boundary` char-def (`guesser-pattern-root-fallback`, `recipe-ordered-generic`,
`recipe-strata-generic`) produced byte-identical proposal/confirmation/state counts in both states —
their corpora are too shallow for an inter-morph boundary token to ever block a query, so an
assertion over them would pass with the fix reverted. The real pin therefore lives in
`corpus_indonesian_confirms_after_the_finish_step`, gated on the private corpus.

The property that matters is not "declares a `BoundaryDefinition`" — every fixture above declares
one and none reproduces the defect. Emitting each grammar's `uflexc` lexc and counting lines
carrying a boundary token gives:

| grammar | boundary tokens | lexc lines with one | continuation class |
|---|---:|---:|---|
| `indonesian` (reproduces) | 3 | 7 | `PrefixOrRoot` |
| `recipe-ordered-generic` | 1 | 1 | `SuffixOrEnd` |
| `guesser-pattern-root-fallback` | 1 | 1 | `SuffixOrEnd` |
| `recipe-strata-generic` | 1 | 0 | never emitted |

The defect needs a morph whose emitted underlying text carries a boundary token in the *prefix*
chain, so a multi-morph path contains a boundary the surface form never does, and `apply_up` on a
plain surface query cannot traverse it until the cleanup compose removes it. A synthetic fixture
reproducing this would need: a `BoundaryDefinition`, a prefix affix whose allomorph text includes
that boundary's representation, roots it attaches to, and words whose surface omits the boundary.
Note `crate::emit`'s `with_boundary_insertions` can mask this on paths that go through it (it
expands the query with boundary-inserted variants, how `metathesis-phase-isolation`'s `mu+i`
works), and `crate::templated_compile` already applies its own cleanup; the gap was only in
`recipe_runtime`'s plan-driven path.
