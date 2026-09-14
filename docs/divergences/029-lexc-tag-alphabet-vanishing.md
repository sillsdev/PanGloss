# 029 — Declared morpheme tags can vanish from the compiled lexc alphabet at depth

## Kind
Unported (a Rust-only compiler defect with no C# equivalent path, since C# has no lexc-compilation
stage at all).

## Status
Open, under investigation; severity explicitly not yet settled by the port's own tracking.

## C# site
Not applicable — this concerns only PanGloss's own optimized FST-compilation path (`pg-foma`), which
has no counterpart in `hc.dll` at all. C# always confirms via its own rule-cascade engine; it never
compiles a grammar to a lexc/foma network.

## Rust site
`pg_foma::emit` (`verify_tags_reachable`, reporting via `EmitReport::uncovered` with
`kind: "unreachable-after-lexc-compile"`).

## What differs
A morpheme tag that is genuinely declared in the grammar can disappear from the **compiled** lexc
alphabet at sufficient stratum depth, silently, under two different lexc-generation strategies (so
the cause is not specific to one code path). The root cause is downstream of lexc generation itself —
under investigation in the `pg-foma` crate's lexc state-deduplication.

## Can it change a parse?
Unsettled. Per the port's own tracking: "if the network's language is unchanged and only `sigma`
bookkeeping differs, this is bookkeeping, not recall loss" — i.e., it is possible this only affects
what the compiled network's alphabet *reports* rather than what it *accepts*, in which case it would
be `representational`/cosmetic rather than `behavioural`. That has not been confirmed either way as
of the source this catalogue reads. `EmitReport::uncovered` also affects `mainline emit()` at depth,
where it is currently undetected (i.e., this specific detector only catches the case at one call
site so far).

## Evidence
`pg_foma::emit::verify_tags_reachable` reports the finding structurally
(`kind: "unreachable-after-lexc-compile"`) but no dedicated test asserting the parse-vs-bookkeeping
question was found during this research pass. **Unverified**: whether this has ever been observed to
change an actual confirmed analysis on any grammar, versus only ever affecting alphabet reporting.

## Upstream
Not applicable.

## Notes
Classified `unported` rather than `behavioural` because there is no C# construct being ported here —
this is a Rust-only compilation artifact. If future investigation confirms it changes what the
network accepts (not just what it reports), this entry should be reclassified `behavioural` and its
status changed from "under investigation" accordingly; do not treat the current "not yet settled"
framing as evidence of safety.
