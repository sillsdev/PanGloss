# HermitCrab → PanGloss: Rust Port Feature & Change Audit

**Purpose.** This is a living ledger of what was ported from the C# `SIL.Machine.Morphology.HermitCrab`
engine (in [`sillsdev/machine`](https://github.com/sillsdev/machine)) into `rust/` here, what's known
to still differ, and exactly which C# commits/PRs/bugs this port has already accounted for. As
HermitCrab in `Machine` keeps evolving — bug fixes, new capabilities — this doc is how someone checks
"does PanGloss need to catch up?" without re-deriving the whole history by hand.

**This is not the correctness gate.** Per project decision (2026-07-10): PanGloss's actual parity
contract is passing the evolving conformance oracle that lives in `Machine` (see the FST plan docs
and, once pulled in, the conformance submodule) — not this document, not the corpora under
`samples/`, not unit tests. This doc exists purely as an **audit trail** — a way to reason about drift
between the two engines over time, and a map of "where do I even look" when the oracle flags a new
divergence. PanGloss's own algorithms are free to diverge from what's described here as the port
evolves into its broader FST-hybrid scope.

## 1. Provenance

- Source repo/branch: `sillsdev/machine`, branch `rust`, built on top of two unmerged C# PRs this
  port's optimizations depend on: **#446** (`hc-rustify` — allocation/CPU optimizations to the C#
  engine itself) and **#451** (`parse-optimization`, based on #446 — analysis-length pruning,
  nogood-memoization, corpus scheduling).
- Squash-copied into this repo 2026-07-10 from `machine`'s `rust` branch at commit `6781b9ac`
  (post-P5), as one commit, no preserved line history. The original `machine` repo retains full
  commit-by-commit history if it's ever needed.
- The conformance fixture set (`rust/conformance/` in the source repo) was **deliberately not**
  copied over yet — see §5.
- One live C#-oracle **bug found and fixed** during this port's work, filed and landed against the
  C# engine itself, not just worked around here: **LT-22613** (`GrammarAnalyzer
  .ComputeMaxAnalysisLength` under-budgeted analysis-length growth for any phonological rewrite
  subrule whose `Lhs`/`Rhs` segment counts differ — insertion or deletion alike — causing the
  default non-tracing parse path to silently over-prune valid analyses that the traced path found
  correctly). Fixed on `sillsdev/machine` PR #453 (branch `lt22613-nogood-cache-fix`, based on
  `parse-optimization`). Worth knowing: the oracle itself needed correcting once, not just this
  port — don't assume "C# says so" is automatically ground truth without checking for known open
  oracle bugs first.

## 2. What's ported: grammar-model construct checklist

| Construct | Status | Notes |
|---|---|---|
| Stratum (`Linear`/`Unordered` rule order) | Ported | |
| `AffixProcessRule`: prefix/suffix/circumfix/infix | Ported | |
| `AffixProcessRule`: reduplication (`ReduplicationHint`) | Ported | tested against the real Indonesian corpus (`indonesian_redup_gate.rs`) |
| `AffixProcessRule`: subtraction/truncation | Ported | |
| `RealizationalAffixProcessRule` | Ported | |
| `CompoundingRule` (+ MPR-feature productivity restrictions) | Ported | recursive non-head analysis fixed (P3) |
| `MorphologicalOutputAction` (`CopyFromInput`/`InsertSegments`/`InsertSimpleContext`/`ModifyFromInput`) | Ported | |
| Phonological `RewriteRule` — **Iterative** mode (epenthesis/deletion/feature-change/expansion/merge/coalescence) | Ported | deepest-tested area; several historical C# bugs mined and pinned |
| Phonological `RewriteRule` — **Simultaneous** mode | Ported (P13) | previously hard-linted as unsupported; now fully implemented with synthetic oracle fixtures, since no real reference grammar exercises it |
| `MetathesisRule` | Ported | |
| Affix templates/slots (obligatory/disjunctive, `Unordered`/`Linear`) | Ported | exercised via real corpora; not yet isolated as synthetic single-feature fixtures |
| Natural classes: `Segments` (literal char-def list) vs `FeatureNaturalClass`/`SegmentNaturalClass` | Ported | `NaturalClassKind::Segments` union-approximation formally proven inert on all 3 reference grammars (P7); cross-table/cross-char-def `FeatureStruct` unification for root lookup fixed (P5) |
| Boundary markers (`CharacterDefinitionTable`) | Ported | |
| `MorphemeCoOccurrenceRule` / `AllomorphCoOccurrenceRule` | Ported | |
| MPR features/groups | Ported | |
| Disjunctive allomorphs / free-fluctuation | Ported | the null/zero-allomorph arm of a disjunctive slot was a real gap, fixed (P10) — this was the dominant cause of a ~50%→98.4% jump in Sena corpus parity |
| Stem names | Ported | |
| Guesser API (`guessRoot`/`LexicalGuess`) | **Partially ported** | engine logic (fabricate an out-of-lexicon root, re-run synthesis) is implemented and verified against the C# unit test's literal expected values; the CLI flag, FFI wire-format bit, and oracle-verified conformance fixtures are not yet done |
| Rule-by-rule tracing (`TraceManager`/`ITraceManager`) | **Mostly ported** | synthesis-side tracing fully wired including phonological rules; analysis-side stratum/template/rule bookends remain untraced |
| `XmlLanguageWriter` round-trip (grammar → XML export) | **Not ported** | explicitly deferred, not a permanent non-goal |

## 3. Known open gaps (as of the squash-copy)

1. **Guesser CLI/FFI surface** — engine logic works, no external surface yet (see §2).
2. **Analysis-side tracing** — stratum/template/rule bookends on the unapplication side aren't
   traced; synthesis side is complete.
3. **`XmlLanguageWriter` round-trip** — not attempted.
4. **Differential fuzzing** — scoped in a design doc, never built.
5. **No aggregated p50/p95/build-time benchmark matrix** has been published across all three
   reference corpora — informal timing measurements exist per-item, not a single authoritative
   table.
6. **A confirmed, still-open Rust-side bug** (not a missing capability, an actual divergence):
   `syn_epenthesis`'s environment check spuriously matches an internal morpheme boundary in one
   specific case, inserting an epenthetic segment the real C# oracle does not. Root cause narrowed
   to two candidate mechanisms, not yet pinned to one. See the `#[ignore]`d note on
   `epenthesis_rules` in `rust/crates/pg-parse/tests/csharp_port_rewrite.rs`.
7. **A tracing-only divergence** (does not affect parse outcome): when two synthesis gates would
   both independently reject a candidate, Rust and C# can report a different *first* `FailureReason`
   because the two gates are checked in the opposite order in each engine.

Full narrative detail for all of the above (and everything already closed) lives in
`docs/history/rust-optimizations-phase2.md` (the live-at-copy-time working plan) and
`rust/docs/phase2-completed/` (the earlier, larger workstreams' archived rationale).

**Housekeeping note (2026-07-10, at copy time):** 14 test files (36 test functions) under
`rust/crates/pg-parse/tests/` and `rust/crates/pg-rules/tests/` read fixtures from
`rust/conformance/`, which was deliberately excluded from this copy (§5). Every affected test
function was marked `#[ignore]` with a reason citing this section, rather than left to fail — they
will start running again automatically once the conformance submodule is added, no manual
un-ignoring needed. This does not represent any loss of engine capability, only loss of that
specific regression coverage until the submodule lands.

## 4. Measured parity, at copy time (2026-07-10)

| Corpus | Result |
|---|---|
| Indonesian | 121/121 (100%) exact |
| Amharic | 660/673 (98.1%) parse-exact; remaining 4 are `TIMEOUT` on pathologically slow words, not wrong answers |
| Sena | 7009/7121 (98.4%) parse-exact, 0 genuine wrong-answer divergences; remaining 112 are cases where Rust succeeds where a frozen C# baseline had timed out |
| Branch coverage of live C# `HermitCrab` achieved by the combined test+corpora+fixture suite | 82.27% (2320/2820) — a C#-side metric, not a Rust-side one |

These numbers describe the state of the `machine` repo's `rust` branch at the moment of the squash
copy. **Do not treat them as still current** once any work happens here — re-derive from the
conformance oracle instead (§5).

## 5. The conformance oracle (not yet pulled in)

`Machine`'s `conformance/` directory (a separate, evolving, engine-agnostic fixture suite — grammar +
word list + oracle-generated expected output, covering single-feature, negative, cross-cutting, and
pathological/stress cases) is meant to become PanGloss's actual correctness gate, pulled in as a git
submodule once that work is further along in `Machine`. It is **deliberately not included** in this
initial copy. When it's added:
- Update this section with the submodule commit/tag pinned.
- The adapter CLI contract it expects (`<engine-binary> batch <grammar.xml> <words.txt>
  <output.tsv>`, producing the existing order-independent signature TSV format) is already what
  `pangloss batch` does — no engine-side changes should be needed to start consuming it.

## 6. Process for future audits

When `Machine`'s HermitCrab gets a bug fix or new capability:
1. Check whether it's covered by a fixture in the (eventually pulled-in) conformance submodule —
   if the oracle catches it, that's the authoritative signal, use it first.
2. If not yet fixture-covered, read the change directly against §2/§3 above: is it touching an
   already-ported construct (likely just needs picking up the fix) or a not-yet-ported one (needs
   scoping as new work)?
3. Update this doc's §2/§3 tables when the answer changes — this document is only useful if it's
   kept current, not left to drift the way `Machine`'s own `HISTORY-MATRIX.md` mining process
   showed ad hoc, whoever-hit-it-first coverage tends to under-track systematic gaps.
