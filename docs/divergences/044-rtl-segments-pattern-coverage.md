# 044 — RTL rewrite-rule FST construction refused `Segments`-shaped patterns entirely

## Kind
Unported (a shape the oracle handles natively that Rust's FST proposer did not support at all).

## Status
Fixed-in-rust.

## C# site
None — `XmlLanguageLoader`'s pattern loader (`LoadPatternNode`, `HCLoader`-equivalent path) builds an
inline pre-segmented `<Segments>` node into an ordinary `PhoneticShape`/pattern-node sequence like any
other context; HermitCrab's interpreter has no separate "can this shape be FST-compiled" gate at all,
so this construct is unconditionally supported.

## Rust site
`pg_foma::replace::pattern_slots`/`compile_rtl_branch_net` (`rust/crates/pg-foma/src/replace.rs`),
`pg_foma::lower::UnsupportedPatternNode::Segments` (`lower.rs`), and the capability verdict
`RightToLeftRewriteFaithfulReversalPredicate` (`right-to-left-rewrite.faithful-reversal-construction`,
`pg_foma::capability`).

## What differs
Before this work, ANY `PatternNode::Segments` node — an inline pre-segmented literal
(`<Segments><PhoneticShape>...</PhoneticShape></Segments>`) rather than an ordinary
`<SimpleContext>`/`<Segment>` reference — caused `pattern_slots` to refuse the whole rewrite-rule
compile unconditionally, regardless of direction (`UnsupportedPatternNode::Segments`'s own doc).
`compile_rtl_branch_net`'s reversal-plus-safety-net-union construction was extended to accept this
shape in a rewrite rule's ENVIRONMENT (not its LHS/RHS focus — a `Segments`-shaped LHS/RHS hits a
separate, pre-existing `pg_rules::rewrite::width_matches` node-count-vs-physical-span limitation this
work did not touch, since environment matching tests first-match existence only, never a positional
per-node width array), in two steps: same-table first (`RightToLeftRewriteFaithfulReversalPredicate`
flips `Refuse` -> `ConfirmOnly` for a `Segments` node whose `characterDefinitionTable` matches the
rule's own table), then cross-table (an explicit `Segments@characterDefinitionTable` override binding
the pattern atom to a DIFFERENT, non-owning table, which the FST construction must render as a
recall-safe union of feature-unifiable tokens across both tables rather than reinterpreting one
table's raw id as the other's — the same aliasing mechanism entry 040 built for ordinary rewrite
rules, reused here for the environment-atom case).

## Can it change a parse?
Yes, in the specific sense "unported" entries track: before this work, a grammar with a
`Segments`-shaped RTL rewrite-rule environment could not be compiled through the FST-optimized path
at all (`Refuse`); after, it compiles and is confirmed against the oracle
(`ConfirmOnly`). No grammar shape went from correct to incorrect — coverage was ADDED, not changed —
but a caller relying on the FST-optimized engine for such a grammar previously got no delivered
parses through that path at all, and now gets oracle-matching ones.

## Evidence
`conformance-staging/edge-cases/right-to-left-segments-environment/` (same-table case, commit
`6418d9fa`) and `conformance-staging/edge-cases/right-to-left-cross-table-segments-environment/`
(cross-table case, commit `82ca3c0f`). Each fixture pins its own structural characterization
(`rtl_reversal_diagnosis` now reports `reversal_construction_attempted: true`), its own capability
verdict via a dedicated `pg-foma/src/capability.rs` unit test
(`right_to_left_predicate_confirm_only_for_same_table_segments_shaped_rule`, and the residual-refusal
counterpart `right_to_left_predicate_refuses_cross_table_segments_shaped_rule` for a table reference
the construction does not yet cover), and the oracle's own correct behavior
(`pg_parse::Morpher`, cross-checked by `pg-parse/tests/conformance_fixtures_gate.rs`'s
`all_discovered_fixtures_match_oracle`). Both fixtures are re-verified against the C# founding oracle
(hc.dll via `hc-conformance.exe`), signatures matching exactly, `rules:` fields transcribed from its
own trace. `right-to-left-segments-environment`'s own STAGING.md additionally records that hc.dll
originally crashed loading the grammar (a `NullReferenceException` in
`XmlLanguageLoader.LoadRewriteSubrule`, which omits `defaultTable` for an environment's
`PhoneticTemplate`, so an environment-embedded `Segments` node with no explicit
`characterDefinitionTable` has no default at all in C# — a structural XML fix, not a linguistic
content change).

## Upstream
None needed — this closes a Rust-only FST-construction coverage gap; hc.dll's own behavior needed no
change and was used unmodified as the correctness reference throughout.

## Notes
`pg_foma::replace::reversed_slots`/`compile_rtl_branch_net` is also the site of entry 017 (bounded
quantifier shallow-reverse mirror risk) — a distinct, still-open structural risk in the same
construction, unrelated to `Segments` coverage. The `Segments`-as-LHS/RHS-focus case (as opposed to
the environment case this entry covers) remains refused; that is `width_matches`'s own pre-existing
limitation (entry 020's neighbor, not duplicated here), confirmed direction-independent and outside
this work's boundary.
