# 023 — `AnalysisStateKey` representation: interned ids vs live references

## Kind
Representational.

## Status
Open (nothing to close — this is a permanent, deliberate representational choice, not a bug).

## C# site
`AnalysisStateKey.cs:26-34`, `:14-34`: the key holds **live references** — `Shape`, syntactic FS,
realizational FS objects — compared via C#'s deep `ValueEquals`/hand-rolled hashing, not interned
integer ids. `AnalysisScope` (`AnalysisScope.cs:12-15`) explicitly notes no interning pool exists
across different words' shapes/feature structures in C# either.

## Rust site
`pg_memo` (`rust/crates/pg-memo/src/lib.rs`), whose module doc explicitly discusses this: the memo
key is `{ shape: ShapeId, stratum: StratumId, syn_fs: FsId, real_fs: FsId, non_head_count: u8,
rule_counts_hash/BTreeMap }` — small interned integer ids, compared by integer equality, backed by a
per-parse interning pool for shapes and feature structures.

## What differs
Same logical identity ("what analysis-cascade state is this"), different underlying representation:
C# compares object graphs deeply (or via its own hand-rolled hash) every time; Rust compares
integers, with the deep-equality cost paid once per distinct value at interning time instead of once
per comparison. `rule_counts` is a `BTreeMap` with ordered iteration in Rust rather than a
`Dictionary` plus a commutative XOR hash in C# — the module doc records this explicitly as a
deliberate choice, kept because the crate's own doc says "a hand-rolled XOR hash" was considered and
not adopted (the specific reasoning for choosing `BTreeMap` over a matching XOR scheme is not
elaborated further in what was read for this catalogue — **unverified** beyond the doc's citation of
the difference).

## Can it change a parse?
No, by construction, as long as key equality remains behaviorally equivalent to C#'s notion of "same
analysis-cascade state" — which is exactly what per-parse interning is designed to guarantee (two
values intern to the same id iff they are, by the crate's own equality rule, the same value). This
representation could become behavioural only if the interning pool's own equality diverged from what
downstream code actually needs to distinguish (see entry 024 for exactly this kind of drift, in the
same key).

## Evidence
None dedicated — this is a structural fact read directly from both sources' module docs, not
test-pinned as a distinct assertion (the memo's overall correctness is pinned by the corpus-parity
suites this catalogue's other entries already cite, not by an equality-representation-specific test).

## Upstream
Not applicable — a pure implementation-strategy difference with no observable-behavior target to
converge on.

## Notes
See entry 024 (count saturation) and entry 025 (two in-flight guards) for two further,
more specific representational choices inside this same memo key/scope design, each independently
documented and each with its own risk profile.
