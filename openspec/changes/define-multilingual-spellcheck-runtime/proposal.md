## Why

`docs/research/spellcheck/PLAN.md` has decided the single-language design for spell-checking and
word prediction: D1 fixes the load-bearing factors to what the parse deterministically produces
(`WordAnalysis`, `rust/crates/pg-parse/src/lib.rs:25-44`); D3 defers Constraint Grammar; D4 ships a
two-scale class n-gram (inter-word class trigram + intra-word morpheme n-gram) as the ranking layer;
D5 treats anything neural as a bounded later ablation. All four decisions, and every research report
behind them (`00-synthesis.md` through `10-rust-inference-and-ports.md`), are scoped to **one loaded
grammar**. Nothing in that decided design says what happens when a host (FieldWorks, Paratext,
Keyman, or a browser embedding) has more than one Language Pack loaded at once and needs to check or
predict text that may be in any of them — which is the normal case, not an edge case, for
multilingual field contexts and code-switched text.

This change is a **design-only** OpenSpec change: it answers the three questions the multilingual
case raises (language identification per word, n-gram compatibility across languages, and the
multi-language data model relative to `.pgpack`) without writing implementation code or running
spikes. It builds on top of `PLAN.md`'s decisions rather than revisiting them, and it explicitly
does not adopt Constraint Grammar, semantic-domain factors, or any generalization of D1/D3/D4/D5 —
those stay exactly as decided, once per loaded language.

## What Changes

- Define how a word is attributed to one of several simultaneously loaded languages before running
  the expensive FST propose→confirm pipeline against more than one grammar, using a cheap
  cascade (host-declared writing system → per-writing-system script/character-set gate → session/
  document language prior) ahead of full parsing, and define the outcome contract for a word that
  parses in more than one loaded language and for a word that parses in none.
- Define that D4's two-scale class n-gram stays **strictly per-language** (no shared vocabulary, no
  union model), and define how a candidate's score is computed when its left context crosses a
  language boundary (a code-switch), including that cross-language score comparability is an
  explicit, unresolved design bet rather than a solved problem.
- Define the multi-language data model: what additive, per-language data `.pgpack` carries for
  spell-checking beyond the existing FST/runtime-data payload (`CONTEXT.md:126-131`), what new
  session-level (not per-pack) state is needed to track the active language set and a seen-word
  cache, that personal overlays stay per-(user, language) by reusing the existing
  `SuppliedRootOverlay`/`LexiconSnapshot` mechanism (`rust/crates/pg-parse/src/overlay.rs`,
  `rust/crates/pg-lexicon/src/runtime.rs`), and load/unload policy for adding or removing a language
  at runtime.
- Record, as an explicit **Open Questions** section rather than an invented answer, every place this
  change could not settle a decision: whether per-writing-system script/character-set data is
  actually extracted into `pg_snapshot` today, whether the resource envelope is scoped per-pack or
  per-process across all resident packs, and how (or whether) to normalize scores across languages
  for the ambiguous-word tie-break.

## Capabilities

### New Capabilities

- `multilingual-spellcheck-runtime`: language identification, cross-language ranking behavior, and
  the multi-language data/session model for running spell-checking and word prediction with more
  than one Language Pack loaded at once.

## Impact

- Design-only. No `pg-*` crate, `.pgpack` format, or CLI is modified by this change. It defines the
  contract that a later implementation change (or changes) must satisfy, the same way
  `define-grammar-coverage-contract` and `define-fst-compilation-health` define contracts consumed
  by later implementation changes in the FST-coverage track (`openspec/changes/STAGING.md`).
- Not added to `openspec/changes/STAGING.md`: that file's own scope is "the active grammar-coverage
  changes" (`STAGING.md:3`) — the FST-compilation/coverage track. This change is a different,
  downstream capability (spell-checking on top of an already-buildable analysis artifact) and does
  not participate in that dependency graph. It assumes a compiled, loadable `.pgpack` exists per
  language, which is exactly what the coverage track is building toward, but does not insert itself
  into that track's stage/merge ordering.
- Depends on `PLAN.md`'s D1/D3/D4/D5 remaining as decided; this change does not reopen them.
