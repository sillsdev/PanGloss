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

## 3a. Update (2026-07-25) — resolutions and newly-found gaps

The §3 list below is the **squash-copy-era** inventory. This section supersedes it where they differ.
Items are numbered to match §3.

**Resolved since the squash copy:**

- **§3 item 4 (differential fuzzing) — BUILT.** The doc's "scoped in a design doc, never built" is
  stale. `pg_foma::oracle` implements the differential-correctness oracle (build ≥2 capability-passing
  plans, assert identical results per word, report the shortest disagreeing word plus the symmetric
  difference), proven non-vacuous by a test that deliberately breaks a plan and confirms the oracle
  catches it. `pg_foma::plan_interaction_coverage` adds a subtree-fuzz slice that runs as a hard
  assertion over every corpus fixture with ≥2 gate groups. What remains is seeded random subtree
  mutation, second-topology generators for node kinds other than `Gate`, and failure minimisation to a
  named recipe.
- **§3 item 6 (`syn_epenthesis` spurious insertion) — FIXED.** Two real causes, both cited to C#: the
  site-enumeration loop treated a `Boundary` node's own slot as an epenthesis site (C#'s empty-LHS
  pattern is `Symbol(Segment, Anchor)` and can never match at a boundary,
  `SynthesisRewriteRuleSpec.cs:26-29`), and `bridge.rs::nat_class_lanes` never pinned the synthetic
  `Type=Segment` feature that C#'s `NaturalClass` ctor stamps unconditionally (`NaturalClass.cs:9-13`).
  The C#-parity test `epenthesis_rules` is now un-ignored and green. The originally-suspected
  "Optional-skip reaches one position too far" was investigated and is **not** a bug — it is a faithful
  port of `TraversalMethodBase.Initialize` (`cs:203-222`).

**Newly found (not in §3):**

- **`max_stem_count` is hardcoded to `2`** inside `Morpher::parse_word_opts`, so the confirm engine
  cannot confirm more than one compounding application — a genuine three-stem compound yields zero
  analyses regardless of what the proposer offers. Recursive compounding is therefore blocked at two
  independent layers. Open question: is `2` faithful to C#, a deliberate guard, or an arbitrary cap?
  Pinned by `conformance-staging/edge-cases/recursive-endocentric-compounding`.
- **`syn_epenthesis` is structurally Simultaneous-shaped regardless of a rule's declared `Iterative`
  mode** (`docs/p13-simultaneous-design.md` §2.3 / §7 item 2 — previously flagged there but undecided).
  Two cascading Iterative epenthesis rules over-fire relative to C#'s true cursor walk. Pinned by the
  `#[ignore]`d `epenthesis_rules_iterative_cascade_finding`, re-verified against the live C# oracle.
  Fixing it means a real cursor-walk rewrite.
- **Surface tokenization uses only the last stratum's character table**, so inner-stratum roots in a
  multi-table grammar cannot be tokenized at all. Architectural, consistent with
  `two_table_symbol_divergence.rs`'s documented convention — recorded so it is not re-diagnosed as a
  MultiTable bug. Pinned by `conformance-staging/edge-cases/bistratal-overlapping-segment-representation`.
- **Declared morpheme tags can vanish from the compiled lexc alphabet at stratum depth**, silently.
  `pg_foma::emit::verify_tags_reachable` now reports these via `EmitReport::uncovered`
  (`kind: "unreachable-after-lexc-compile"`). The cause is downstream of our lexc generation — the same
  tags disappear under two different generation strategies — and is under investigation in the `foma`
  crate's lexc state-deduplication. **Severity is not yet settled**: if the network's language is
  unchanged and only `sigma` bookkeeping differs, this is bookkeeping, not recall loss. Also affects
  mainline `emit()` at depth, where it is still undetected.
- **The tracked description of the one known conformance divergence is stale** — it says "no output"
  while the current baseline already shows "produced output", independent of any recent change.

**§4's parity numbers are superseded.** Re-measured 2026-07-25 against the same corpora (release):
large-lexicon 326/326 engine analyses; junction 97/97; interdigitation multiset parity 29/29 with 0
mismatches; `f3_parity` 0 mismatches across all three corpora; the gated composed net still exactly
82 states / 1,110,358 arcs. Treat those as the current anchor, and re-derive rather than trusting
either list.

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
   `epenthesis_rules` in `rust/crates/hc-parse/tests/csharp_port_rewrite.rs`.
7. **A tracing-only divergence** (does not affect parse outcome): when two synthesis gates would
   both independently reject a candidate, Rust and C# can report a different *first* `FailureReason`
   because the two gates are checked in the opposite order in each engine.

Full narrative detail for all of the above (and everything already closed) lives in
`docs/history/rust-optimizations-phase2.md` (the live-at-copy-time working plan) and
`rust/docs/phase2-completed/` (the earlier, larger workstreams' archived rationale).

**Housekeeping note (2026-07-10, at copy time):** 14 test files (36 test functions) under
`rust/crates/hc-parse/tests/` and `rust/crates/hc-rules/tests/` read fixtures from
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
  `hc-rs batch` does — no engine-side changes should be needed to start consuming it.

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
