# 030 — Disjunctive-allomorph / free-fluctuation re-check (W3.2), formerly deferred

## Kind
Behavioural.

## Status
Fixed-in-rust (formerly an accepted, deferred gap; now closed).

## C# site
`Allomorph.IsWordValid`'s second loop (Allomorph.cs:127-152), `Allomorph.FreeFluctuatesWith`'s
adjacent-pair `ConstraintsEqual` walk over an index range (Allomorph.cs:80-98,100-108), and
`Word.GetDisjunctiveAllomorphApplications` (`Allomorph.cs:127`, falling back to
`Enumerable.Range(0, Index)`).

## Rust site
`pg_rules::validity` (`rust/crates/pg-rules/src/validity.rs`, `allomorphs_valid_impl`), and
`crate::word::MorphRecord::passed_over` (`pg-rules/src/word.rs`).

## What differs
Per morph occurrence, C# checks every "passed-over" disjunctive alternative — an earlier-indexed
allomorph of the same morpheme recorded by synthesis, or (for root morphs, where nothing was
recorded) ALL earlier-indexed allomorphs — and **rejects** the word if the used allomorph does not
free-fluctuate with the passed-over one (constraints must be pairwise equal), its environments are
absent or satisfied at that morph's span, and its other allomorph constraints hold. Before this was
ported, Rust had no equivalent recheck at all — it accepted a disjunctive allomorph choice without
verifying free-fluctuation against alternatives synthesis had passed over.

## Can it change a parse?
Yes: without this recheck, a disjunctive-allomorph choice that C# would reject (because a
passed-over alternative does not free-fluctuate with the one actually used) is silently accepted by
Rust — an over-generation bug. `disjunctive_recheck_gate.rs`'s own module doc states this directly:
"removing the disjunctive candidate loop from `allomorphs_valid_impl` (or `MorphRecord::passed_over`
population) makes `wakta`/`pakda` start parsing again" — i.e. those two words are known-bad analyses
this recheck exists specifically to reject.

## Evidence
`rust/conformance/allomorphy/disjunctive-recheck/` (oracle-diffed). `pg-parse/tests/
disjunctive_recheck_gate.rs` is explicitly a red-on-revert regression gate, phrased in its own doc
comment as testing exactly the failure mode described above.

## Upstream
None, not applicable — this was a genuine, previously-deferred Rust-side gap (per its own history
row, plan #5d, formerly deferred); C# was already correct.

## Notes
`pg-rules/src/validity.rs`'s module doc places this alongside W3.3 (entry 006) as one of the two
formerly-deferred gates closed in the same area of the file — both concern per-occurrence
correctness of the final allomorph-validity check, one for environment anchoring on discontinuous
morphs, one for disjunctive free-fluctuation.
