# 025 — Two in-flight re-entrancy guards where C# has one

## Kind
Representational.

## Status
Moot — closed by [045](045-memoization-removed.md). Both guards lived in `pg-memo`, which is
deleted; HC-Rust has no re-entrancy guard because it has no memo to re-enter.

## C# site
`AnalysisScope.cs:56-60`: a single `InProgress` set, shared between the mrule-cascade memo and the
template-battery memo (both live over the same key space).

## Rust site
`pg_memo` (`rust/crates/pg-memo/src/lib.rs`): two separate guards, `in_flight` (mrule cascade) and
`template_in_progress` (template battery).

## What differs
C# uses one re-entrancy guard for both memo tables; Rust gives the template battery its own,
distinct guard, on the grounds that it is "a distinct computation over the same key space." A hit on
either guard falls back to plain, unmemoized expansion rather than reading a partial entry — that
fallback behavior is identical in both engines; only the granularity of *which* computations share a
guard differs.

## Can it change a parse?
No, by the source's own explicit argument: "A shared guard would be correctness-neutral (a false hit
only forgoes memoization for one call), but a separate one is cleaner." Both a shared guard (C#'s
choice) and separate guards (Rust's) can only ever cause an extra unmemoized recomputation on a
false-positive hit, never a wrong answer — re-entrant expansion without memoization is still a
correct expansion, just a slower one.

## Evidence
None dedicated — the argument is stated directly in `pg-memo/src/lib.rs`'s module doc as a design
rationale, not backed by a specific regression test distinguishing the two guard strategies. Marking
as **evidence: none — argued sound by construction, not measured**.

## Upstream
Not applicable — a pure implementation-strategy difference with no wrong/right target; C#'s single
guard is not a bug, and Rust's split is not a fix.

## Notes
Grouped with entries 023-024 as the third and smallest of the memo-representation family; included
mainly for completeness of the by-module lookup, since `pg-memo/src/lib.rs`'s "Deviations from the
C#" section groups all three under one heading.
