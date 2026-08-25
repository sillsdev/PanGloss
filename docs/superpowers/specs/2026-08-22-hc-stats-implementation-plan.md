# HC `--stats` implementation plan

Date: 2026-08-22

Design of record: `docs/research/pangloss-stats-attribution-and-aggregation-spec.md`.
All builds and tests go through `rust/tools/pg.ps1` per the repo's managed-build rule.

## Shape

Five phases, ordered by risk rather than by dependency, because the risky part is the collector and
everything else is plumbing around it. Phases 1 and 2 can proceed in parallel once Phase 0 lands.

Per-text reporting is **not** in this plan — it needs text and occurrence extraction that
`pg-fwdata` does not have, and that work belongs to the transient project cache design.

## Phase 0 — Prerequisites

Small, and everything else assumes them.

**Grammar hash.** Expose a digest of the loaded grammar so `batch --stats` can decide whether to
wipe. Hash `Snapshot::to_json()`, not the `.fwdata` bytes, so edits that cannot affect parsing do not
invalidate. Note the cost: a `.fwdata` input currently imports in-memory straight to a compiled
grammar with no JSON written, so this adds one serialization pass per run.

- Touches: `pg-cli`'s `load_grammar` dispatch, `pg-snapshot`.
- Verify: same project hashes identically across two loads; a gloss-only edit does not change it; a
  rule edit does.

**Identity resolution.** One function mapping every runtime id to
`(key, kind, label, identity_quality)`:

| Kind | Source |
|---|---|
| `lex_entry` | `LexEntryDef.authored_id` |
| `phon_rule` | `RewriteRuleDef.xml_id` |
| `morph_rule` | `AffixProcessRuleDef.morpheme` → morpheme registry → MSA GUID |
| `stratum` | structural locator: index + name |
| `allomorph` | structural locator: owning object + index |
| `root_index`, `guesser`, `overlay` | synthetic stable ids |

- Verify: every rule, entry, stratum, and allomorph in every conformance fixture resolves to a
  non-empty key with a correct `identity_quality`; keys are stable across two loads of the same
  grammar.

## Phase 1 — Collector core

The highest-risk phase. Write the invariants **first**; they are the only defence against a
plausible-looking wrong report.

**Placement** follows the existing design freeze: the collector type lives in `pg-rules`, which knows
when rule work begins; `pg-parse` owns the per-word lifecycle and the merge. It rides alongside
`StepBudget` — same per-word lifetime, same non-`Sync` `Cell` pattern, no locks, no atomics.

**Storage shapes.** Dense `Vec<u64>` per counter for rules and strata; sparse map or
insertion-ordered `Vec<(LexEntryId, Counters)>` for lexical entries, because dense would zero 0.5-5MB
per word at 10^4-10^5 entries for ~99.99% zeros. Counter arrays must not live on `Word`.

**Gating.** `Option<&StatsCollector>` threaded to the instrumentation sites. Off means nothing
allocated and one perfectly-predicted branch per site.

**Sites, in order of confidence:**

1. `apply_one_mrule` (`pg-rules/src/stratum.rs:565-590`) — `attempts` and `work` at the tick,
   `outputs` from `outs.len()`, `not_applied` when `outs.is_empty()`. Clean single choke point.
2. `rewrite::analyze` family (`pg-rules/src/rewrite.rs:1128`, `1314`, `1401`) — same four, with
   `work` from the shape length. `ordered_spans` already materializes match positions if a finer
   measure is ever wanted.
3. Entry materialization in `lexical_lookup_filtered` (`pg-parse/src/morpher.rs:474-493`) — one
   `attempts` per (matched entry × allomorph).
4. The trie walk (`pg-parse/src/root_trie.rs`) — charged to the stratum's `root_index`.
5. `no_root` at the failed lookup — charged to `w.mrule_apps.last()`.
6. `surface_mismatch` at the synthesis gate — charged to the `lex_entry`.
7. Commit-on-pass buffer for `uses` — a scratch buffer cleared per candidate and reused, committed
   only when a candidate passes. Writes `uses` for entries, morphological rules, and phonological
   rules from one mechanism.
8. **The ~15 allomorph loops in `pg-rules/src/morph.rs`** (267, 313, 1242, 1334, 1433, 1508, 2056,
   2080, 2152, 2176, and the `subrules` loops at 2309, 2581). Each distinguishes gated-out by MPR
   group, tried-and-failed, succeeded, and never-reached via the disjunctive `break`.

**Invariants (write these first):**

- `SUM(attempts)` over `morph_rule` kinds == the engine's `steps` for that word
- allomorph rows sum to their rule's row, per word — this is what catches a missed loop in (8)
- counters identical across `--threads N` for any N

## Phase 2 — Storage

**New crate `pg-stats`**, so `pg-cli` stays a thin front end and the netstandard2.0/FFI surface can
reach it later without pulling in the CLI.

`rusqlite` with the `bundled` feature, so there is no system SQLite dependency to manage across
machines — worth calling out explicitly against the repo's build-hardening posture.

Responsibilities: schema creation; wipe-and-recreate on `grammar_hash` or `schema_version` mismatch
(it is a cache, so migration is never needed); accumulate — skip words already present; upsert word
rows, since two runs can compute the same word concurrently; batched transactions after words
complete, never inside the parse; WAL plus `busy_timeout`.

Cache location: user-data, directory named by a hash of the canonical `.fwdata` path, full path
recorded inside. `--cache <path>` overrides and hands lifetime to the caller.

- Verify: round-trip; wipe fires on a grammar edit and says so; a second run over an overlapping word
  set adds only the new words; concurrent writers do not corrupt.

## Phase 3 — CLI

- `batch --stats [--cache <path>]` — writes the cache, prints one line (words analyzed, elapsed).
  Accepts `--engine foma`; writes `coverage` rows from the engine so unmeasurable counters are marked
  `unsupported`.
- `pangloss stats [--group ...] [--kind] [--stratum] [--object] [--min-attempts] [--top] [--sort]
  [--exclude-capped] [--out FILE]` — reads the one cache at the derived path, no grammar load.
  Warns when a query spans mixed option sets or counter-semantics versions.

- Verify: golden CSVs excluding the `elapsed_ns` columns, which are the only non-deterministic values
  in the design.

## Phase 4 — Reports

Two defaults. Per word: form, **actual** elapsed, attempts, passes, capped/timed-out, sorted by
elapsed descending. Per object: kind, label, attempts, **measured self** time, outputs, amplification,
the three dead-end columns, uses, in per-kind sections, sorted by measured self time descending.

Three presentation rules that are part of the design, not polish:

- actual elapsed time and per-object self time are labeled separately
- the per-object report's header states its times are measured self time
- an unmeasurable column renders `—`, never `0`, driven by the `coverage` table

## Risks

**The ~15 allomorph loops.** Every one is a place to silently under-count, and the feature's most
valuable signal — which allomorph of an over-applying rule is the culprit — depends on all of them.
The allomorph-sums-to-rule invariant is the mitigation, which is why it is written first.

**Gated instrumentation rots**, because ordinary test runs never exercise it. It needs its own tests,
and the invariants need to run under `--stats` in CI rather than only by hand.

**The counter-semantics version is hand-maintained** and will occasionally be forgotten, mixing
incomparable rows in one cache. `run` rows record build info so it is diagnosable after the fact, and
the invariants usually catch a semantics change loudly at the next test run.

## Start here

Phase 0's identity resolution, then Phase 1's invariants, then `apply_one_mrule`. That order gets a
real number out of a real grammar with a test proving it is not lying, before touching the fifteen
loops where the silent failure lives.
