# 026 — Syntactic feature structures are provably variable-free; phonological variables live in a separate mechanism

## Kind
Representational (flagged for where it could become behavioural).

## Status
Open.

## C# site
`FeatureValue.UnifyImpl` (`FeatureValue.cs:64-118`), `SimpleFeatureValue.IsUnifiableImpl`/
`SubsumesImpl`/`DestructiveUnify` (`SimpleFeatureValue.cs:52-102,104-154,156-235`): one shared
implementation handles both syntactic and phonological feature structures, branching internally on
`IsVariable`, `useDefaults`, re-entrancy (`copies`/`Forward`/`Dereference`), and a negation flag
(`not`/`notOther`) on every symbolic value.

## Rust site
Two entirely separate implementations for the two domains:
- `pg_featstruct::{tree, ops}` (`rust/crates/pg-featstruct/src/tree.rs`,
  `rust/crates/pg-featstruct/src/ops.rs`) for the **syntactic** domain: a tree model (no
  re-entrancy), no variables, no negation flag on symbolic values, `useDefaults` always false.
- `pg_rules::rewrite::bind_or_check`/`resolve_bindings` (`rust/crates/pg-rules/src/rewrite.rs`) for
  **phonological** alpha-variable agreement, checked against node lanes after a candidate span is
  reported by the FST (which cannot itself bind variables).

## What differs
Not a behavioural difference today — `ops.rs`'s own module doc makes an explicit, argued case that
every omission is **exact, not approximate**, because HC's actual construct set never reaches the
omitted branches on the syntactic side:
- Re-entrancy: a tree has no node reached twice, so C#'s `copies`-populated branch is never taken in
  this domain either — recursing directly over the tree is exact.
- Variables: "alpha variables are phonological-only in HC," so the syntactic path always takes C#'s
  `!IsVariable && !otherSfv.IsVariable` arm.
- `useDefaults`: always false on this path in both engines.
- Negation flag: `crate::tree::FeatureValue::Symbolic` carries no negation flag at all (unlike C#'s
  `not`/`notOther`-parameterized API), so every call passes `(false, false)` — inspecting the
  `(false, false)` arm of each bitvec op shows the `mask` parameter is unused there, so this omission
  is likewise argued exact rather than approximate.

Phonological alpha-variable agreement, meanwhile, is handled entirely separately: since the FST
engine cannot itself bind variables, Rust resolves bindings post-hoc against node lanes, in C#'s own
`MatchSubrule` binding order (target, then left environment, then right environment).

## Can it change a parse?
Not today, per the argued exactness above. This entry exists specifically because a **future**
change could make it behavioural: if a syntactic feature system were ever authored with negated
symbolic values (`not`), or if a grammar construct that binds a variable syntactically (rather than
phonologically) were ever added, the `ops.rs` module would need to grow the branches it currently
omits — and until then, any code path attempting to construct such a value would need to fail loudly
(a loader lint), not silently drop the negation, per this repo's stated policy on incomplete
representations.

## Evidence
The exactness argument is made directly in `pg-featstruct/src/ops.rs`'s module doc
("## What was skipped, and why it's exact (not an approximation)"), citing the specific C# branch
conditions each omission corresponds to. **Unverified**: whether `pg-grammar`'s loader actually
refuses (rather than silently drops) an attempt to author a negated symbolic value or a syntactic
variable in a real grammar file — this was not traced during this research pass.

## Upstream
Not applicable — no divergence exists today to converge on.

## Notes
Flagging the "unverified" loader-refusal question above as the one open follow-up this entry should
prompt: if a grammar with a negated syntactic feature value loads silently and simply drops the
negation rather than refusing, that would be a `CannotRepresent`-class problem in this repo's own FST
vocabulary (per CLAUDE.md's FST-evidence classification section), not merely a documentation gap.
