# 011 — Missing word-initial epenthesis synthesis site

## Kind
Behavioural.

## Status
Fixed-in-rust.

## C# site
`SynthesisRewriteRuleSpec`'s pattern walk, which starts *before* the first segment annotation, so
position 0 (the word-initial gap) is an ordinary application site there like any other.

## Rust site
`pg_rules::rewrite::syn_epenthesis` (`rust/crates/pg-rules/src/rewrite.rs`).

## What differs
The site-enumeration loop (`for (site, &node) in node_of.iter().enumerate()` with
`left_end = right_start = site + 1`) only ever considered the gap *after* each existing segment —
the word-initial gap before segment 0 was never a candidate site, so an epenthesis rule whose
environment holds only at position 0 could never fire during synthesis.

## Can it change a parse?
Yes: any epenthesis rule gated on a word-initial environment silently never fires in synthesis,
producing no output where C# inserts the epenthetic segment.

## Evidence
Fixed by adding the site-0 gap (splice after `ms.nodes[0]`, the left anchor) — the synthesis twin of
`ana_narrow_deletion`'s already-landed equivalent fix. Unit gate:
`pg-rules/tests/rewrite_gate.rs::epenthesis_synthesis_word_initial_site`. This bug was confounded
with `boundary_rules_required_pos_on_subrule_finding`'s separate POS-gate case (a real, independently
correct gate) until this fix landed — the bare-root, no-morphological-rule epenthesis-only
phonological rule never re-applied on synthesis-confirm at all, independent of the POS gate's
condition.

## Upstream
None, not applicable — pure Rust-side bug; C#'s pattern walk already includes position 0.

## Notes
Once this fix and entry 012's landed together, `boundary_rules_required_pos_on_subrule_finding`'s
POS gate composes correctly: `taba` resolves to `pos2` only, `ba` to `pos1` only. The v1 oracle fixture for this did not survive the
v1 -> v2 migration; the live pin is `pg-rules/tests/rewrite_gate.rs`.
