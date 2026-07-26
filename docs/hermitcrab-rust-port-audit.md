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
| Guesser API (`guessRoot`/`LexicalGuess`) | **Ported** (2026-07-25) | engine logic, plus `--guess` on `batch`/`parse` (default off) and additive `hc_parse_word_opts`/`hc_parse_batch_opts` carrying an explicit `guessed` byte at word and analysis level, plus a conformance fixture. See §3a. The old `hc_parse_word`/`hc_parse_batch` deliberately return NO guesses — their wire format cannot mark one |
| Rule-by-rule tracing (`TraceManager`/`ITraceManager`) | **Ported** (2026-07-25) | synthesis side was already complete; the five unwired analysis-side events (`begin`/`end_unapply_stratum`, `begin`/`end_unapply_template`, `lexical_lookup`) are now wired against their C# call sites. See §3a — the earlier "rule bookends untraced" wording overstated the gap, since the rule level was already wired |
| `XmlLanguageWriter` round-trip (grammar → XML export) | **WON'T DO** (2026-07-26) | not a capability gap. The 2026-07-16 data-format decisions sunset HC XML in PanGloss, and the grammar→external round-trip is served by `lcm-grammar` v1 (`GrammarJsonServices.ExportGrammar`, sillsdev/liblcm#392) byte-gated against `pg-fwdata`. Full reasoning in §3a |

## 3a. Update (2026-07-25) — resolutions and newly-found gaps

The §3 list below is the **squash-copy-era** inventory. This section supersedes it where they differ.
Items are numbered to match §3.

**§3 item 3 (`XmlLanguageWriter` round-trip) — CLOSED 2026-07-26 as WON'T-DO, superseding "explicitly
deferred, not a permanent non-goal".** §2's classification predates the 2026-07-16 data-format
decisions, which retire the format this port would target:

- **LibLCM is and remains the authoritative store**; PanGloss is a pure function over grammars
  (verification + field deployment), never a writer back into the authority's format.
- **HC XML importing in PanGloss is being sunset** — fwdata plus snapshot JSON only. Deletion is
  staged (the XML loader is still load-bearing for ~92 gate-test fixtures) but the direction is
  settled, and the standing instruction is explicit: *do not add features to the HC-XML loader; route
  new work through fwdata/snapshot.*
- **The replacement round-trip already exists and is better.** The grammar → external-format
  round-trip is served by `lcm-grammar` v1 (`GrammarJsonServices.ExportGrammar`, shipped in
  sillsdev/liblcm#392 with a published JSON Schema), gated byte-identical against `pg-fwdata` on real
  projects. That gate is what replaces the HC-XML oracle.

So porting C#'s 1,321-line `XmlLanguageWriter` would build an exporter for a format we are removing,
duplicating a round-trip that already has an authority-owned implementation and a stronger
two-implementation byte gate. Reclassified as a **permanent non-goal for the HC-XML format
specifically** — not a capability gap. If a grammar→XML export is ever genuinely needed (e.g. to feed
a legacy tool), it should be re-opened as its own change with that consumer named, not carried as an
unclosed port gap.

**§3 item 5 (no benchmark matrix) — CLOSED 2026-07-26**, `docs/benchmark-matrix.md`. The measurement
turned up something more important than the latency numbers: **all three reference grammars are
REFUSED by the `--engine=foma` optimized path under default capability enforcement** — Indonesian on
`quantifier.bounded-expansion` (the unbounded quantifier resolved PROVABLE the previous day), Amharic
on `mpr-group.overwrite-output` (a permanent carve-out), Sena on both `compounding.non-recursive` and
that same MPR carve-out. So the coverage cross-check reporting 20/20 Covered and the optimized path
not running on any reference grammar are both true at once; they are different claims. And because
`MprGroupOverwrite` is a *permanent* carve-out present in two of the three, "the FST path runs all
three reference grammars with enforcement on" is not reachable by design — only via the ADR 0005
override, stamped `trust=unproven`. Oracle-path tail latency is the other finding: Amharic p99
105 s, worst word 7.6 min; Sena worst 10.3 min (Sena's run is partial and labelled as such).
Force-compiled Indonesian on the foma path is ~11× faster end-to-end with byte-identical signatures
for all 121 words.

**Resolved 2026-07-25 (later in the same day — these supersede the entries further down this section):**

- **§3 item 1 (guesser surface) — CLOSED.** `--guess` on `pangloss batch`/`parse` (default off, output
  byte-identical without it); `hc_parse_word_opts`/`hc_parse_batch_opts` as additive FFI symbols with
  their own magic carrying an explicit `guessed` byte at word *and* analysis level; ABI 2 → 3; fixture
  `conformance-staging/edge-cases/guesser-pattern-root-fallback` with engine-derived ground truth. The
  adjacent overclaim found while building it (see "OVERCLAIM in shipping code" below) is also closed:
  the `pg_lexicon` retry is now explicit opt-in defaulting off, the two old symbols pass `false`, and
  the old format's encoder additionally *filters* guessed analyses so a future caller cannot
  reintroduce it. `pg-wasm` opts back in explicitly at both call sites — the default flip would
  otherwise have silently stopped the demo guessing, which its own doc says is deliberate.
- **§3 item 2 (analysis-side tracing) — CLOSED.** All five previously-unwired events now fire:
  `begin_unapply_stratum` (`stratum.rs::analyze`), `end_unapply_stratum` at both C# exits,
  `begin_unapply_template`, `end_unapply_template`, and `lexical_lookup` at both the real-lexicon and
  guesser sites. The four C# `end_unapply_template` line-pairs collapse onto two Rust call sites
  because C# has `#if SINGLE_THREADED` and `Parallel.ForEach` implementations of `ApplySlots` each
  firing the same two logical events — a structural fact, not under-wiring.
- **§3 item 7 (`FailureReason` order) — CLOSED, aligned to C# rather than accepted.** The syn-FS gate
  moved to C#'s position (last) in both `synth_affix` and `synth_affix_cached`. Outcome invariance
  verified by reading: the three intervening gates read only `rule`/`word.flags`/`word.mpr`/the root
  allomorph's stem name, never `word.syn_fs`, and `synth_syn_fs` is pure. The analysis-side twin was
  checked and correctly left alone — it has only the one gate, so there is no order to fix.
- **`max_stem_count` — CLOSED.** Not wrong as a default (`Morpher.cs:56` sets `2` too), but C# exposes
  `MaxStemCount` as a settable per-instance property (`Morpher.cs:72`, read at
  `AnalysisCompoundingRule.cs:45`) and raises it to 3 in its own tests
  (`CompoundingRuleTests.cs:87,105`). The gap was the hardcoding; a `with_max_stem_count` builder now
  exposes it, default unchanged.

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

**§3 item 2 (analysis-side tracing) — scope narrowed, and the doc's wording is partly stale.** "stratum/
template/rule bookends on the unapplication side aren't traced" over-states it: the **rule** level IS
wired. `morphological_rule_unapplied`/`_not_unapplied` fire from `pg-rules/src/morph.rs:371-494`,
`phonological_rule_unapplied` from `rewrite.rs:1815`/`:1940` and `metathesis.rs:770`. What is genuinely
unwired is exactly **five** `TraceSink` methods with **zero** pipeline call sites (the trait declares
them; only the no-op impl uses them), each with a precise C# counterpart:

| Unwired `TraceSink` method | C# call site(s) |
|---|---|
| `begin_unapply_stratum` | `AnalysisStratumRule.cs:105` |
| `end_unapply_stratum` | `AnalysisStratumRule.cs:125`, `:143` (two exits) |
| `begin_unapply_template` | `AnalysisAffixTemplateRule.cs:44` |
| `end_unapply_template` | `AnalysisAffixTemplateRule.cs:72`, `:78`, `:108`, `:117` (four sites; the `false`/`true` arg is the unapplied flag) |
| `lexical_lookup` | `Morpher.cs:352`, `:379` — **not previously recorded as a gap at all**; it is an analysis-side event and is likewise never fired |

Verified by counting pipeline call sites per event: all four apply-side bookends have 1-2 call sites
each, all four unapply-side bookends and `lexical_lookup` have 0.

**§3 item 7 (`FailureReason` order) — mechanism now pinned exactly; it is a fixable divergence, not an
inherent one.** The two gates are the syn-FS unify gate and the non-final-template prohibition, in
`synth_affix`/`synth_affix_cached` (`pg-rules/src/morph.rs`, the gate at `:1669` vs the one at `:1681`).
C#'s `SynthesisAffixProcessRule.Apply` checks, in order: `MaxApplicationCount` →
`NonPartialRuleProhibitedAfterFinalTemplate` → `NonPartialRuleRequiredAfterNonFinalTemplate` →
`RequiredStemName` → `RequiredSyntacticFeatureStruct` (`SynthesisAffixProcessRule.cs:44-131`, syn-FS
**last**). Rust checks syn-FS **first**, so the port is inverted on exactly that pair. Every one of these
gates returns empty on failure, so the surviving-word set is order-independent — only the reported first
reason differs, which is why this is trace-only. **Resolution: align to C# order** (move the syn-FS gate
after the stem-name gate) rather than documenting the divergence as accepted; the C# order is cited and
the change cannot alter parse outcomes.

**Newly found (not in §3):**

- **OVERCLAIM in shipping code — `hc_parse_word`/`hc_parse_batch` can return a guessed analysis
  unmarked.** `pg_lexicon::analysis` retries via the guesser (`ParseOptions::default()
  .with_guess_only(true)`, `analysis.rs:127-129`) **unconditionally** whenever a word produces zero
  analyses and the shape was valid — there is no on/off switch. `UnifiedAnalysis` and each
  `WordAnalysis` both carry the `guessed`/`provenance` fact correctly (`analysis.rs:51`, `:136`), so
  the information survives all the way to the FFI boundary — and is then **discarded** there, because
  `hc_parse_word`/`hc_parse_batch`'s wire format has no guessed field. A caller of those two symbols
  therefore receives guessed analyses byte-indistinguishable from confirmed ones. This is the ONE
  invariant (never overclaim) being violated in shipping code rather than in a plan. Found while
  building G3's guesser surface; correctly left alone by that task (different crate, additive-only
  scope) and escalated here. **Decision (2026-07-25):** the fix is NOT to add a bit to the existing
  format — that would break every existing native decoder. `hc_parse_word`/`hc_parse_batch` must
  instead behave as guess-OFF (return no guessed analyses at all), since they cannot mark what they
  return; callers who want guessing use the `_opts` symbols G3 added, whose format carries an explicit
  `guessed` byte at both word and analysis level. Existing callers lose results they were silently
  being handed, which is the correct direction: a dropped guess is a recoverable disappointment, an
  unmarked guess is a false claim.

- **Structural recall-loss risk: `Dir::RightToLeft` + a bounded `Quantifier` builds a WRONG mirror.**
  `reversed_slots` (`pg-foma/src/replace.rs:394-396`) is a **shallow** reverse —
  `slots.iter().rev().cloned()` — and does not recurse into a `Slot::Repeat`'s own `children`. The
  RTL construction (`compile_rtl_branch_net`, `:502-507`) applies it to the LHS, RHS, and both
  environments to build the mirror rule. For a slot list containing a `Repeat` whose children are not
  palindromic, the mirror is not the reverse of the original: reversing
  `[Fixed(y), Repeat{[a,b]}, Fixed(x)]` must yield `[Fixed(x), Repeat{[b,a]}, Fixed(y)]`, but the
  shallow reverse leaves the group's interior in document order.

  The combination is **reachable**: `is_fully_supported_shape` (`:965-970`) gates only on
  `RewriteMode`, returning `true` unconditionally for `Iterative`, and `pattern_slots` has ACCEPTED
  bounded quantifiers since `compile-bounded-fst-quantifiers` (it builds `Slot::Repeat`). So the
  capability side's `RightToLeftRewriteDetail::reversal_construction_attempted` — which re-runs
  `pattern_slots` — now reports `true` for these rules and the predicate admits them at
  `ConfirmOnly`. `ConfirmOnly` protects against over-generation, not omission, so a missed
  RTL-preferred outcome would be silent and unrecoverable. Note the two changes were each correct in
  isolation; the hole is in their interaction, and `capability.rs:391-395`'s doc claiming the check
  "must avoid `Quantifier`" is now **stale** as a description of what `pattern_slots` does.

  **Not yet reproduced — deliberately stated as structural, not measured.** The existing fixture
  `conformance-staging/edge-cases/right-to-left-bounded-quantifier-rewrite` does not detect it: its
  quantifier wraps a SINGLE `<SimpleContext>` (`grammar.xml:78`), so its children list is trivially
  palindromic and the shallow reverse is accidentally correct. The first task of any fix is to author
  a multi-child-quantifier RTL fixture and confirm the miss before changing `reversed_slots`.

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

> **This list is HISTORICAL and now fully resolved — §3a supersedes it.** Status as of 2026-07-26:
> items 1, 2, 4, 5, 6 and 7 are **closed**; item 3 is **won't-do**. Item 4's remaining depth (seeded
> random subtree mutation, second-topology generators beyond `Gate`, failure minimisation to a named
> recipe) closed the same day — `Union` got a sound generator, and `Compose`/`Replace` were declined
> with `#[should_panic]` proof that `build_controllable` mechanically rejects the unsound reorderings.
> Read §3a; this section is kept verbatim for provenance, not as a to-do list.
>
> **Open work that remains is NOT in this list** — it is the construct/configuration backlog in
> `openspec/changes/plan-construct-coverage-completion/tasks.md` §4 (circumfix C1/C3/C2, `MultiTable`
> aliasing, recursive compounding, the quantifier and RTL-metathesis builds), plus two things that will
> never close and should not be counted as gaps: `MprGroupOverwrite` is a permanent carve-out present
> in two of the three reference grammars, and `SimultaneousRewrite`'s overlapping-subrule
> configuration stays oracle-blocked until a real `hc.dll` harness exists (ADR 0001 names it).

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
