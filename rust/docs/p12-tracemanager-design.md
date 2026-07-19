# P12 — TraceManager (rule-by-rule parse tracing) port: design

Status: **design only** (plan `rust-optimizations-phase2.md` §P12, [FABLE-PLAN then SONNET]).
Decided in scope 2026-07-10 (Open scope decisions #2: "PORT IT"). No engine code changes accompany
this doc — every quoted Rust snippet below is read-only evidence, not a diff. Target implementer: a
Sonnet-tier agent working mechanically from §5.

Motivating quote (John, verbatim, plan doc "Open scope decisions #2"): *"I want to see (and I want
you to see) how you parse or don't parse like the C# code."* This is explicitly **not** a
FieldWorks-integration nicety — it is a debugging/verification tool for this port itself. Every
prior root-cause hunt in this project (P1, P2, P6, P10, etc.) had to manually re-derive what the C#
engine tried and rejected, word by word, by reading both engines' source side by side and reasoning
backward from a diffed output. A real, faithful trace would have shortened several of those hunts
from multi-day dissections to a side-by-side diff at the exact rule/subrule/reason level. §6 below
walks through P10 concretely to sanity-check that this design actually delivers that value.

Oracle: `.worktrees/parse-opt/src/SIL.Machine.Morphology.HermitCrab/` — `ITraceManager.cs`,
`TraceManager.cs`, `Trace.cs` read in full, plus every `_traceManager`/`TraceManager` call site
across `Morpher.cs`, `Allomorph.cs`, `RootAllomorph.cs`, `AnalysisStratumRule.cs`,
`SynthesisStratumRule.cs`, `AnalysisAffixTemplateRule.cs`, `SynthesisAffixTemplateRule.cs`,
`SynthesisAffixTemplatesRule.cs`, `MorphologicalRules/*.cs`, `PhonologicalRules/*.cs`.

---

## 1. What the C# does, read end-to-end

### 1.1 `ITraceManager` — the interface (`ITraceManager.cs`, full)

One property, 24 methods, all called unconditionally *by the interface contract*, but every real
call site in the engine guards with `if (_morpher.TraceManager.IsTracing)` first (the zero-cost-off
requirement). Grouped by lifecycle stage:

- **Word-level bookends**: `AnalyzeWord(lang, input)` (called once, at `ParseWord` entry — mints the
  root `Trace` and hangs it on `input.CurrentTrace`), `Successful(lang, word)`,
  `Failed(lang, word, reason, allomorph, failureObj)`, `GenerateWords(lang)` (a different root, for
  the synthesis-only `GenerateWords` API), `SynthesizeWord(lang, input)` (marks the transition from
  an analysis candidate into the synthesis half).
- **Stratum bookends**: `BeginUnapplyStratum`/`EndUnapplyStratum` (analysis),
  `BeginApplyStratum`/`EndApplyStratum` (synthesis), plus two synthesis-only partial-parse signals
  `NonFinalTemplateAppliedLast` and `ApplicableTemplatesNotApplied` (both distinct TraceType entries
  but both always carry `FailureReason.PartialParse` — see `TraceManager.cs`).
- **Template bookends**: `BeginUnapplyTemplate`/`EndUnapplyTemplate(..., bool unapplied)` (analysis),
  `BeginApplyTemplate`/`EndApplyTemplate(..., bool applied)` (synthesis).
- **Rule attempts** (the bulk of the interface, 8 methods, symmetric analysis/synthesis ×
  phonological/morphological): `PhonologicalRuleUnapplied`/`PhonologicalRuleNotUnapplied`,
  `PhonologicalRuleApplied`/`PhonologicalRuleNotApplied(..., reason, failureObj)`,
  `MorphologicalRuleUnapplied`/`MorphologicalRuleNotUnapplied`,
  `MorphologicalRuleApplied`/`MorphologicalRuleNotApplied(..., reason, failureObj)`. Each "not"
  variant on the *synthesis* side carries a `FailureReason`; the *analysis* "not unapplied" variants
  carry no reason at all (C# just never distinguishes why an unapplication didn't fire — worth
  noting as a place where even C# itself doesn't reify a reason).
- **Compounding-specific**: `CompoundingRuleNotUnapplied`/`CompoundingRuleNotApplied(..., reason,
  failureObj)` — compounding gets its own two methods (not routed through the generic
  Morphological* pair) because its trace type (`CompoundingRuleAnalysis`/`CompoundingRuleSynthesis`)
  is distinct.
- **Lexical lookup**: `LexicalLookup(stratum, input)` — fired once per stratum's root-allomorph trie
  search (both the real lexicon path and, per P11's design, the guesser's pattern-match path reuse
  this exact call).
- **Blocking**: `Blocked(rule, output)` — a rule that *would* have applied again but was blocked by
  the "don't re-apply the same rule to its own output" guard.

### 1.2 `TraceManager` — the concrete tree-builder (`TraceManager.cs`, full)

Every method is a one-liner: build a `new Trace(TraceType.X, source) { ...fields }` and append it to
`((Trace)someWord.CurrentTrace).Children`. Two subtleties that matter for the Rust design:

1. **Word cloning discipline.** Several methods clone the word before stashing it
   (`output.Clone()` in `PhonologicalRuleUnapplied`/`PhonologicalRuleApplied`, `input.Clone()` in
   `PhonologicalRuleNotUnapplied`, `input.Clone()` in `LexicalLookup`/`SynthesizeWord`) — because the
   live `Word` object continues mutating after the trace event fires (further rule applications
   append to its shape/morphs in place), so the trace tree needs a **frozen snapshot at the moment
   of the call**, not a live reference that would silently reflect later state. Others store the
   live reference directly (`Input = input` in most Begin* methods, since nothing mutates it before
   the matching End* call reads it back). This snapshot-vs-reference split is intentional and must
   be preserved faithfully — the Rust equivalent has the same hazard: cloning a `Word` is not free
   (`Shape` + `FeatureStruct` + `Vec<MorphRecord>`), so the design must clone *only* at the same
   points C# does, not everywhere defensively.
2. **`CurrentTrace` reassignment as a stack-like cursor.** `MorphologicalRuleUnapplied`/
   `MorphologicalRuleApplied` don't just append a child — they *reassign* `output.CurrentTrace =
   trace` (or `input.CurrentTrace = trace` for the unapplied case with different semantics), so the
   next event nests **under** this rule's trace node rather than as a sibling. `SynthesizeWord` does
   something subtler still: `curTrace.Children.Last.Children.Add(trace)` — it reaches *two* levels
   deep (the last child of the current trace) before appending, because by the time
   `SynthesizeWord` fires the cursor logically needs to descend into the just-appended
   `LexicalLookup` node's children. This "cursor that moves as the parse progresses" is the single
   trickiest piece of state to port — it is exactly what makes the C# design a live callback
   interface rather than a simple event log (see §4.1 for how the Rust design handles it without
   replicating a mutable per-`Word` field).

### 1.3 `Trace`/`TraceType` — the tree node (`Trace.cs`, full)

`Trace : OrderedBidirTreeNode<Trace>` (a generic ordered n-ary tree from
`SIL.Machine.DataStructures`), fields: `Type` (`TraceType`, 20 values — `None` plus 19 real ones,
listed in the C# source order in §1.1/§4.3 below), `Source` (`IHCRule` — the rule/stratum/template/
language object that produced this node; `null` for the leaf-most `Successful`/`Failed`/`Blocked`
nodes since those are keyed off `Word`, not a rule), `SubruleIndex` (`-1` default; set for
phonological/morphological rule nodes to say *which* subrule/allomorph fired), `Input`/`Output`
(`Word`, possibly a clone per §1.2), `FailureReason`.

### 1.4 `FailureReason` — the 24 real values (`ITraceManager.cs`, full enum; `None` is the 25th,
never a real failure)

Read every call site (not just the enum declaration) to know what each value means operationally:

| Value | Fires from (C# file:line) | Operational meaning |
|---|---|---|
| `ObligatorySyntacticFeatures` | `Morpher.cs:723` | Final word validity: an obligatory feature (`Feature.ObligatorySyntacticFeatures`, e.g. a required inflectional slot) is missing from the accumulated syntactic FS. |
| `AllomorphCoOccurrenceRules` | `Allomorph.cs:171` (via `CheckAllomorphConstraints`) | An `AllomorphCoOccurrenceRule` attached to the specific allomorph used rejected the word. |
| `Environments` | `Allomorph.cs:119` | The allomorph's declared `Environments` (Required or Excluded) don't hold at this morph's actual span in the final word. |
| `MorphemeCoOccurrenceRules` | `Allomorph.cs:193` | A `MorphemeCoOccurrenceRule` on the morpheme (not the specific allomorph) rejected the word. |
| `DisjunctiveAllomorph` | `Allomorph.cs:145` | The W3.2-equivalent disjunctive re-check: a passed-over, non-free-fluctuating sibling allomorph would *also* have matched here, so this word is rejected in favor of whichever word used that sibling instead. |
| `SurfaceFormMismatch` | `Morpher.cs:740` | `IsMatch`: the synthesized word's rendered surface doesn't literally match the input string (only reachable via `IsMatch`'s final-gate call, never mid-parse). |
| `Pattern` | many (`SynthesisAffixProcessRule.cs:246`, `SynthesisRealizationalAffixProcessRule.cs:177`, `SynthesisCompoundingRule.cs:240` (as `HeadPattern` there instead — see next row), `SynthesisMetathesisRule.cs:61`, `SynthesisRewriteRule.cs:82`) | The generic "no allomorph/subrule's LHS pattern matched this input at all" fallback — the catch-all reported when the loop over allomorphs/subrules exhausts with zero hits and no more specific reason was recorded. |
| `HeadPattern` | `SynthesisCompoundingRule.cs:240` | Compounding-specific `Pattern`: the *head* side's pattern didn't match. |
| `NonHeadPattern` | `SynthesisCompoundingRule.cs:233` | Compounding-specific: the *non-head* side's pattern didn't match. |
| `RequiredSyntacticFeatureStruct` | `AffixProcessAllomorph.cs:96`, `SynthesisAffixProcessRule.cs:131`, `SynthesisRealizationalAffixProcessRule.cs:71`, `SynthesisRewriteSubruleSpec.cs:38` | An allomorph/subrule's `RequiredSyntacticFeatureStruct` doesn't subsume the word's current syntactic FS. |
| `HeadRequiredSyntacticFeatureStruct` / `NonHeadRequiredSyntacticFeatureStruct` | `SynthesisCompoundingRule.cs:110` / `:94` | Compounding-specific splits of the same gate, one per side. |
| `HeadProdRestrictMprFeatures` / `NonHeadProdRestrictMprFeatures` | `SynthesisCompoundingRule.cs:125` / `AnalysisCompoundingRule.cs:92` | The MPR-feature productivity-restriction gate (`ProdRestrict`), split per compounding side. |
| `RequiredMprFeatures` / `ExcludedMprFeatures` | `SynthesisAffixProcessRule.cs:155/172`, `SynthesisRealizationalAffixProcessRule.cs:95/112`, `SynthesisCompoundingRule.cs:147/164`, `SynthesisRewriteSubruleSpec.cs:54/69` | The allomorph/subrule's `RequiredMPRFeatures`/`ExcludedMPRFeatures` gate against the word's MPR set. |
| `RequiredStemName` | `RootAllomorph.cs:68`, `SynthesisAffixProcessRule.cs:115` | The root's `StemName.IsRequiredMatch` gate, or (rule-level) `requiredStemName` reference-equality gate. |
| `ExcludedStemName` | `RootAllomorph.cs:83` | A sibling allomorph's stem name's `IsExcludedMatch` rejects this allomorph. |
| `PartialParse` | `Morpher.cs:709`, `SynthesisStratumRule.cs:76`, `TraceManager.cs:151/162` (the two `Stratum*Output` trace-methods, both hardcoded to this reason) | The word left a stratum (or the whole parse) with an unconfirmed/unapplied rule still pending — an incomplete confirmation chain. |
| `BoundRoot` | `RootAllomorph.cs:61` | A root flagged `isBound` is the word's *only* distinct allomorph (bound roots cannot stand alone). |
| `NonPartialRuleProhibitedAfterFinalTemplate` | `SynthesisAffixProcessRule.cs:77`, `SynthesisCompoundingRule.cs:74` | After a *final* template applied, a non-partial rule tried to apply again — prohibited. |
| `NonPartialRuleRequiredAfterNonFinalTemplate` | `SynthesisAffixProcessRule.cs:100` | After a *non-final* template applied, only a partial rule may follow — a non-partial one is rejected. |
| `MaxApplicationCount` | `SynthesisAffixProcessRule.cs:55`, `SynthesisCompoundingRule.cs:59` | The rule has already hit its declared `MaxApplicationCount` on this word's trail. |

`None` is the sentinel default (`SynthesisRewriteSubruleSpec.cs:82`, `CurrentRuleResults[i] =
Tuple(None, null)` seeded before a subrule attempt, overwritten to a real reason or left as `None`
meaning "matched, nothing to report").

### 1.5 Full call-site inventory (every `_traceManager`/`TraceManager.` reference)

Collected by grepping `TraceManager\.|_traceManager` across the whole
`SIL.Machine.Morphology.HermitCrab` tree (176 matches). Organized by file, since this is exactly the
per-file work breakdown §5's implementation chunks follow:

- **`Morpher.cs`** (the top-level orchestrator): `IsTracing` guard at `ParseWord` entry (gates
  whether `AnalysisScope`'s lexical-gating optimization even runs — tracing changes control flow
  here, not just observability, per the existing Rust doc-comment already flagging this: see §2),
  `AnalyzeWord` (once per `ParseWord`), `GenerateWords`-as-root-trace + `SynthesizeWord` +
  `Successful` (inside `GenerateWords`'s parallel loop), `LexicalLookup` + `SynthesizeWord` (twice:
  once in `LexicalLookup` proper, once in `LexicalGuess` — P11's guesser path, confirming P11 and
  P12 share this exact hook), `Failed(PartialParse)` / `Failed(ObligatorySyntacticFeatures)` /
  `Successful` / `Failed(SurfaceFormMismatch)` inside `IsWordValid`/`IsMatch`.
- **`Allomorph.cs`** (`IsWordValid` + `CheckAllomorphConstraints`, the base class both `RootAllomorph`
  and `AffixProcessAllomorph` extend): `Failed(Environments)`, `Failed(DisjunctiveAllomorph)`,
  `Failed(AllomorphCoOccurrenceRules)`, `Failed(MorphemeCoOccurrenceRules)` — all four fire from the
  **final per-word validity check**, not from rule application.
- **`RootAllomorph.cs`**: `Failed(BoundRoot)`, `Failed(RequiredStemName)`, `Failed(ExcludedStemName)`
  — same final-validity-check family, root-specific.
- **`MorphologicalRules/AffixProcessAllomorph.cs`**: `Failed(RequiredSyntacticFeatureStruct)` — same
  family, affix-specific.
- **`AnalysisStratumRule.cs`**: `BeginUnapplyStratum`/`EndUnapplyStratum` (the stratum bookends,
  called even when `mergeEquivalentAnalyses` — note the C# comment "Don't merge if tracing because
  it messes up the tracing," `cs:151-152` — tracing **disables** the shape-merge optimization
  outright, a second control-flow-changing interaction beyond the `Morpher.cs` one above).
- **`AnalysisAffixTemplateRule.cs`**: `BeginUnapplyTemplate`/`EndUnapplyTemplate(unapplied: bool)`
  (multiple call sites — the template's own top-level attempt plus each slot-batch outcome).
- **`AnalysisCompoundingRule.cs`**: `CompoundingRuleNotUnapplied`, `MorphologicalRuleUnapplied`/
  `MorphologicalRuleNotUnapplied`.
- **`MorphologicalRules/AnalysisAffixProcessRule.cs`**,
  **`MorphologicalRules/AnalysisRealizationalAffixProcessRule.cs`**: `MorphologicalRuleUnapplied`/
  `MorphologicalRuleNotUnapplied`, symmetric to the compounding pair above.
- **`PhonologicalRules/AnalysisMetathesisRule.cs`**, **`PhonologicalRules/AnalysisRewriteRule.cs`**:
  `PhonologicalRuleUnapplied`/`PhonologicalRuleNotUnapplied`.
- **`SynthesisStratumRule.cs`**: `BeginApplyStratum`, `NonFinalTemplateAppliedLast`
  (`FailureReason.PartialParse`), `Failed(PartialParse)` (a *third*, Morpher-level-shaped call for
  the same underlying condition — worth noting three distinct trace events can all carry
  `PartialParse` at three different tree depths), `EndApplyStratum` (twice: normal completion, and
  the `output.Count == 0` fallback branch).
- **`SynthesisAffixTemplateRule.cs`**: `BeginApplyTemplate`, `EndApplyTemplate(applied: bool)`.
- **`SynthesisAffixTemplatesRule.cs`**: `ApplicableTemplatesNotApplied` (`FailureReason.PartialParse`
  again), `Blocked`.
- **`MorphologicalRules/SynthesisAffixProcessRule.cs`** (the single busiest file, ~14 distinct call
  sites): every one of `RequiredSyntacticFeatureStruct`/`RequiredMprFeatures`/`ExcludedMprFeatures`/
  `RequiredStemName`/`NonPartialRuleProhibitedAfterFinalTemplate`/
  `NonPartialRuleRequiredAfterNonFinalTemplate`/`MaxApplicationCount` as `MorphologicalRuleNotApplied`
  reasons, plus `Blocked`, `MorphologicalRuleApplied`, and the `Pattern` fallback.
- **`MorphologicalRules/SynthesisRealizationalAffixProcessRule.cs`**: the realizational-rule mirror
  of the above (subset of reasons — no `NonPartialRule*`/`MaxApplicationCount`, since realizational
  rules don't carry those gates), plus `Blocked`.
- **`MorphologicalRules/SynthesisCompoundingRule.cs`** (~13 sites): the compounding-specific reason
  set (`HeadPattern`/`NonHeadPattern`, `Head/NonHeadRequiredSyntacticFeatureStruct`,
  `Head/NonHeadProdRestrictMprFeatures`, `RequiredMprFeatures`/`ExcludedMprFeatures`,
  `MaxApplicationCount`, `NonPartialRuleProhibitedAfterFinalTemplate`), `CompoundingRuleNotApplied`,
  `Blocked`, `MorphologicalRuleApplied`.
- **`PhonologicalRules/SynthesisMetathesisRule.cs`**: `PhonologicalRuleApplied`/
  `PhonologicalRuleNotApplied(Pattern)`.
- **`PhonologicalRules/SynthesisRewriteRule.cs`**: `PhonologicalRuleApplied`/
  `PhonologicalRuleNotApplied(reason from CurrentRuleResults, or Pattern fallback)` — this is the one
  site where the reason comes from a **per-subrule side-channel dictionary**
  (`Word.CurrentRuleResults: Dictionary<int, Tuple<FailureReason, object>>`) populated by
  `SynthesisRewriteSubruleSpec.cs` during the underlying pattern-matcher's own evaluation, then read
  back out by the wrapping rule after the whole `_patternRule.Apply(input)` call returns — i.e. the
  *matcher internals* (not the trace call site itself) are what decide which of
  `RequiredSyntacticFeatureStruct`/`RequiredMprFeatures`/`ExcludedMprFeatures`/`None` gets recorded
  per subrule index, and the trace call site is purely a reader of that side-channel. This is the
  most structurally different site from a plain "gate that returns a bool" — see §3's mismatch list.

---

## 2. Current Rust state

Confirmed absent by direct grep: no `Trace`, `TraceManager`, `ITraceManager`, or `FailureReason` type
anywhere in `rust/crates/`. `pg-parse/src/morpher.rs`'s own doc comment already flags this
explicitly: *"the trace-less overload; this port has no `ITraceManager`, plan rust-conversion.md
§7"* (on `Morpher::generate_words`'s doc). Every C#-side control-flow interaction tracing has
(disabling `mergeEquivalentAnalyses`, disabling the Phase-5 lexical gate, bypassing the
`MaxAnalysisLength` Gate-B early return) currently has **no Rust equivalent to disable**, because
there is nothing to gate on. §4.1 and §5 chunk 2 address what the Rust design must do about these
three specific interactions once tracing exists (short version: mirror them — tracing must produce
the *unmemoized, unshortcut* code path exactly as C# does, or trace output would diverge from actual
parse behavior in exactly the cases where a human most wants to trust it).

The Rust engine's architecture is a chain of free functions returning `Vec<Word>`
(`pg_rules::morph::{synthesize, analyze}`, `pg_rules::rewrite::{synthesize, analyze}`,
`pg_rules::metathesis::{synthesize, analyze}`, `pg_rules::stratum::{StratumAnalyzer,
synthesize_stratum}`), not C#'s per-rule object graph where every rule is an `IRule<Word,int>`
instance holding a `Morpher` reference it can call back into at will. This has one deep structural
consequence for tracing: **C# fires a "not applied" trace event from inside the exact rule instance
that tried and failed; Rust's functions return an empty `Vec<Word>` (or omit a candidate) with no
side channel at all for *why*.** Concretely:

- `pg_rules::morph::synth_affix`/`synth_affix_cached` (`morph.rs:1138-1304`) already has **explicit,
  individually early-returning gates** with doc comments that cite the exact C# line numbers each one
  ports (`RequiredSyntacticFeatureStruct`-adjacent `synth_syn_fs` gate, the
  `NonPartialRuleProhibitedAfterFinalTemplate`/`NonPartialRuleRequiredAfterNonFinalTemplate` pair at
  `morph.rs:1150-1166`, the `RequiredStemName` gate at `morph.rs:1172-1174`) — every one of these
  currently just does `return Vec::new();` on failure, discarding exactly the information a trace
  event needs. This is the best-positioned code in the whole engine for tracing: the gates already
  exist, are already individually identified, and already cite their C# `FailureReason` by name in
  comments — they just don't *emit* anything today.
- `pg_rules::stratum::apply_one_mrule` (`stratum.rs:546-603`) has the `MaxStemCount`/
  `MaxApplicationCount` analysis-side gates as explicit early returns, same shape.
- `pg_rules::validity::allomorphs_valid_impl` (`validity.rs:390-518`) is the **direct** Rust
  counterpart of C#'s `Allomorph.IsWordValid` — same function, same gates, same order
  (bound-root → stem-name → allomorph-co-occurrence → morpheme-co-occurrence → environments →
  disjunctive-recheck), confirmed by the module's own doc comment citing `Allomorph.cs:105-156` line
  for line. This is the single most direct 1:1 mapping in the entire codebase between a C# trace
  call-site cluster (`Allomorph.cs`'s four `Failed(...)` sites plus `RootAllomorph.cs`'s three) and
  one Rust function's control flow.
- `pg_rules::stratum::synth_apply_templates` (`stratum.rs:1325-1401`) already has an explicit doc
  comment ("Tier-2 #13, gate 1") distinguishing the exact C# branch that decides between
  `NonFinalTemplateAppliedLast` and `ApplicableTemplatesNotApplied` — both currently collapse to the
  same Rust `if out.is_empty() && ...` passthrough with no distinguishing signal emitted.
- The generic rule-application functions in `morph.rs`/`rewrite.rs`/`metathesis.rs` that return
  `Vec::new()` on total mismatch (the `Pattern`/`HeadPattern`/`NonHeadPattern` fallback reason) have
  **no distinguishing signal at all today** between "gate N failed" and "no allomorph/subrule's
  pattern matched" — an empty return is just an empty return.

`pg-cli`'s existing diagnostics (`HC_STEP_STATS`, `HC_FST_PROFILE`) are unconditional numeric
accumulators bumped inside `pg_fst::traverse` and `pg_rules::morph::push_remove_duplicates` — see §4
for why these are a genuinely different concern from tracing and must stay separate mechanisms.

---

## 3. Cross-reference: every C# trace call site vs. Rust today

This is the heart of "flag rather than force a 1:1." Three buckets:

### 3.1 Direct 1:1 — Rust already has the exact gate, just silent

| C# event | Rust location | Note |
|---|---|---|
| `Failed(BoundRoot)` | `validity.rs:423-425` (`if def.is_bound && distinct_count == 1`) | Exact match. |
| `Failed(RequiredStemName)` / `Failed(ExcludedStemName)` | `validity.rs:214-226` (`stem_name_gates_ok`, itself calling `stem_name_required_match`/`stem_name_excluded_match` — already split into two functions internally) | Rust already distinguishes required-vs-excluded internally; just needs to report which one failed instead of folding both into one bool. |
| `Failed(Environments)` (final validity) | `validity.rs:442,493` (`check.envs_ok(...)`) | Exact match. |
| `Failed(DisjunctiveAllomorph)` | `validity.rs:457-469,502-513` (the W3.2 disjunctive-recheck loops) | Exact match — this is the single cleanest mapping in the whole codebase; Rust's own doc comment already cross-references `Allomorph.cs:127-152` line for line. |
| `Failed(AllomorphCoOccurrenceRules)` / `Failed(MorphemeCoOccurrenceRules)` | `validity.rs:436-441,487-492` (`allomorph_co_occurrence_ok`/`morpheme_co_occurrence_ok`) | Exact match. |
| `Failed(ObligatorySyntacticFeatures)` | `morpher.rs:713-724`/`is_word_valid`'s `for &f in &w.obligatory` loop | Exact match — Rust already isolates the specific failing `FeatId` in the loop variable; just isn't surfaced. |
| `Failed(PartialParse)` (Morpher-level) | `morpher.rs:337-339`/`is_word_valid`'s `w.mrule_app_index != -1` check | Exact match. |
| `Failed(SurfaceFormMismatch)` | `morpher.rs:350-355`/`is_match` | Exact match. |
| `MaxApplicationCount` (analysis side) | `stratum.rs:568-570`/`apply_one_mrule` | Exact match. |
| `NonPartialRuleProhibitedAfterFinalTemplate` / `NonPartialRuleRequiredAfterNonFinalTemplate` | `morph.rs:1150-1166`/`synth_affix`(`_cached`) | Exact match, already cross-referenced by C# line number in the Rust doc comment. |
| `RequiredStemName` (rule-level) | `morph.rs:1172-1174` | Exact match. |

### 3.2 Gate exists, but Rust currently folds distinct C# reasons into one bool

| C# reasons | Rust location | What's missing |
|---|---|---|
| `RequiredMprFeatures` / `ExcludedMprFeatures` | `g.mpr_group_ok(allo.required_mpr, allo.excluded_mpr, word.mpr)` calls throughout `morph.rs` (e.g. `morph.rs:1183`) | One bool return covers both C# reasons; needs to report which of the two (required-missing vs. excluded-present) failed. |
| `RequiredSyntacticFeatureStruct` (allomorph-level, at *apply time*) | **Not gated at apply time at all** in `synth_affix`/`ana_affix` — `validity.rs`'s own module doc explicitly states: *"this port's `synth_affix`/`ana_affix` in `morph.rs` never gate on this per-allomorph FS at apply time; only the rule-level `required_syn_fs` is enforced there."* The check only happens later, in `validity.rs`'s final-word-validity pass (`validity.rs:481`, `:508` for the disjunctive recheck). | This is a genuine **timing** mismatch, not just a missing signal: C# fires `MorphologicalRuleNotApplied(..., RequiredSyntacticFeatureStruct, ...)` immediately, mid-synthesis, at the exact rule-application attempt; Rust defers the equivalent rejection to the final `allomorphs_valid_cached` pass, several call frames and possibly several rule-applications later. End-to-end correctness is unaffected (a word that C# would reject at apply time, Rust still rejects — just later), but a **trace tree built naively from Rust's current control flow would show this rejection at the wrong tree position** relative to a C# trace: as a leaf under the final word-validity check rather than nested under the specific rule/subrule attempt. §4.2 flags this as an explicit scoping note for the ported `FailureReason` enum and the implementation plan. |
| `Pattern` / `HeadPattern` / `NonHeadPattern` (generic "nothing matched") | Implicit in every `Vec::new()`/empty-iterator return from `morph::synth_affix`, `morph::synth_compound_subrule`, `rewrite::synthesize(_with_mpr)`, `metathesis::synthesize` | No distinguishing signal today between "every allomorph's own specific gate rejected it" (which — once 3.1/3.2's other gates are wired for tracing — would already be reported under a more specific reason) and "the LHS pattern itself never matched the input shape at all." The `Pattern` fallback is genuinely the *last resort* reason in C# too (only reached once every subrule's `CurrentRuleResults` entry is either absent or itself `None`), so this is less a gap than an ordering requirement: the specific-reason gates must be threaded through and checked first, with `Pattern` as the residual "nothing else fired and no output was produced" case, exactly mirroring C#'s own precedence. |
| `HeadProdRestrictMprFeatures` / `NonHeadProdRestrictMprFeatures` | Compounding-side `mpr_group_ok` calls in `synth_compound`/`synth_compound_subrule` (`morph.rs`) | Same shape as the `RequiredMprFeatures`/`ExcludedMprFeatures` row above, compounding-specific. |
| `HeadRequiredSyntacticFeatureStruct` / `NonHeadRequiredSyntacticFeatureStruct` | Compounding-side syntactic-FS subsumption checks in `synth_compound_subrule` | Same shape. |
| `NonFinalTemplateAppliedLast` vs. `ApplicableTemplatesNotApplied` | `stratum.rs:1388-1399`'s single `if out.is_empty() && (input.flags.is_partial || !applicable)` passthrough | Both C# events carry the same `FailureReason.PartialParse`, so for *reason-reporting* purposes this is actually a non-issue — but the two are genuinely distinct *trace events* (different `TraceType`, different English meaning: "a template applied but this word wasn't the last/final one" vs. "some template was applicable but none actually applied"). A faithful port needs the Rust condition split into its two component branches (which the existing "Tier-2 #13, gate 1" doc comment already names) so each can fire its own distinct trace event, not folded into one. |

### 3.3 Structurally different — needs its own mechanism, not a simple bool-to-reason upgrade

| C# site | Why it's different |
|---|---|
| `SynthesisRewriteRule.cs`'s `PhonologicalRuleApplied`/`NotApplied` (§1.5's last bullet) | The reason comes from a **per-subrule-index side channel** (`Word.CurrentRuleResults: Dictionary<int, Tuple<FailureReason,object>>`) populated *during* the underlying `IPatternRule.Apply` matcher's own internal evaluation (in `SynthesisRewriteSubruleSpec.cs`), then read back by the wrapping rule after the whole call returns. Rust's `rewrite::synthesize_with_mpr(_cached)` (`rewrite.rs:881-989`) has no equivalent side channel today — subrule-level rejection reasons (`RequiredSyntacticFeatureStruct`/`RequiredMprFeatures`/`ExcludedMprFeatures` gates inside `subrule_applicable`, `rewrite.rs:820-840`) are checked and discarded inline, per subrule, during the FST-based matching walk, not accumulated into a per-index map for a caller to inspect afterward. Wiring this for tracing means adding an analogous per-subrule accumulator to `rewrite.rs`'s synthesis path — the single largest structural change in this whole design, not a reason-upgrade of an existing bool. |
| Compiled-FST-based matching generally (`rewrite.rs`, `morph.rs`'s pattern-compile paths, `pg-fst`) | These operate over compiled nondeterministic/deterministic automata (`Fst`, `traverse.rs`) rather than a step-by-step interpreted pattern walk the way C#'s `Matcher<Word,int>`/`PatternRule` machinery does. C# can trace "which specific pattern element failed to match at which position" because its matcher is a direct AST walk; Rust's FST-compiled matchers report only match/no-match for the whole compiled automaton, with no intermediate position-level failure signal surfacing today. This means the Rust `Pattern` reason (§3.2) is realistically the *finest* granularity available for rewrite/metathesis rule tracing without deeper FST-internals surgery — flagged as a known, permanent granularity gap versus C#, not a temporary omission to "fix" in a later chunk. |
| Analysis-side "not unapplied" events carry no reason in C# either | `PhonologicalRuleNotUnapplied`/`MorphologicalRuleNotUnapplied` take no `FailureReason` parameter at all (§1.1) — so there is nothing to port here beyond the bare event; Rust's analysis-side functions already return `Vec::new()`/omit candidates exactly as opaquely as C# itself treats this case. No mismatch — both engines are equally silent on *why* an unapplication didn't fire. |

**Summary for §4.2**: of the 24 real `FailureReason` values, 11 are already exact 1:1 gates in Rust
today (§3.1), 9 more exist as gates that need to report which of 1-2 specific reasons fired instead
of a bare bool (§3.2), and the `Pattern`/`HeadPattern`/`NonHeadPattern` trio is a residual
last-resort case whose Rust ordering must mirror C#'s own precedence once the others are wired. Only
one call-site family (`SynthesisRewriteRule`'s per-subrule side channel, §3.3) needs new
architecture beyond "add a reason parameter to an existing early return."

---

## 4. Design decisions

### 4.1 (a) The Rust trait/callback shape

**Decision: a `TraceSink` trait, threaded as `Option<&dyn TraceSink>` through the same call chain
that already threads `&RuleCache`/`&StepBudget`, guarded at every call site by a `sink.is_some()` (or
an inlined `is_tracing()`) check exactly mirroring C#'s `if (_morpher.TraceManager.IsTracing)`
idiom.**

Rejected alternatives and why:

- **An internal event log built unconditionally, filtered at read time.** This is the cheapest to
  implement (append to a `Vec<TraceEvent>` inside a `RefCell`, no threading needed beyond one shared
  handle) but fails the *zero-cost-when-off* requirement outright: the accumulation cost (allocating
  and cloning a `Word` snapshot into every event) would be paid on every single rule-application
  attempt of every parse, always, whether or not anyone ever reads the log. Given `pg-cli batch`
  parses tens of thousands of words per run and the analysis cascade alone can attempt hundreds of
  thousands of rule applications per pathological word (`docs/budget-model.md`), this is not a
  tolerable default-on cost, and gating the *accumulation* itself behind a runtime flag just
  reinvents the guard-check design below with extra indirection.
- **A channel-based approach** (send `TraceEvent`s to an `mpsc`/crossbeam channel, consumed by a
  separate thread or at the end of the call). This adds a genuine ordering hazard the C# design
  does not have: the whole point of the trace tree is that a rule's "not applied" event must nest
  under the correct stratum/word/template ancestor **at the moment it fires**, using the live
  cursor position (§1.2's `CurrentTrace` reassignment). A channel decouples emission from ordering
  unless the receiver reconstructs the same cursor logic downstream from a flat event stream tagged
  with enough parent-linking metadata to do so — which is strictly more machinery than building the
  tree inline, for no offsetting benefit (this port is single-threaded per parse; `--threads N`
  batch mode parses *different words* in parallel, never one word across threads, so there is no
  concurrency reason to prefer a channel over direct synchronous calls).
- **A direct tree-building callback (mirroring C# almost verbatim: the sink itself owns and mutates
  a growing `Trace` tree, with a mutable "current node" cursor field on the sink, updated by
  `begin_*`/rule-applied calls exactly as C# reassigns `Word.CurrentTrace`).** This is *almost* the
  recommendation, and captures the C# design most literally, but pins the cursor to the **sink**
  rather than to each `Word` the way C# does — C#'s `CurrentTrace` is a per-`Word` field precisely
  because multiple words are in flight simultaneously within one stratum's candidate set (a `Word`
  producing several candidate children, each of which needs its OWN cursor position to keep
  recording into once it diverges from its siblings). A single sink-level cursor would conflate
  these. The design below keeps the callback shape but threads the cursor identity through an
  explicit **trace handle** value carried alongside each `Word` (see below) rather than mutating
  global sink state — this is the one place the Rust design deviates from a literal C# mirror, and
  it is a **strengthening**, not a shortcut: it makes the multi-candidate-cursor hazard impossible
  to get wrong by construction (a stale/wrong cursor is a compile-time-visible "which handle did I
  pass" question, not a runtime "did I remember to reassign `CurrentTrace` on the right object"
  bug — exactly the class of bug the C# object-mutation design is prone to and that a faithful
  literal port would reproduce).

**Concrete shape:**

```rust
// pg-rules/src/trace.rs (new module; pg-parse re-exports for CLI/FFI convenience)

/// A stable handle into the trace tree the sink is building — the Rust analog of C#'s
/// `Word.CurrentTrace`, but carried as an explicit value (a small `Copy` index into the sink's
/// arena) alongside a `Word` rather than mutating a field on it. `Word` itself gains one new
/// field, `trace: Option<TraceHandle>`, mirroring `CurrentTrace: object` — `None` when tracing is
/// off (the common case; adds one `Option<u32>`-sized field, no allocation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceHandle(u32);

/// The zero-cost-when-off guard + event-emission surface. One instance per `parse_word` call
/// (never shared/reused across words — mirrors C#'s per-`Morpher`-but-effectively-per-call
/// `IsTracing` check plus per-word `Trace` tree root).
pub trait TraceSink {
    /// Mirrors C# `ITraceManager.IsTracing`. Checked at every call site before doing ANY other
    /// trace-related work (cloning a `Word`, computing a `FailureReason`) -- the single branch
    /// that must be free when tracing is off.
    fn is_tracing(&self) -> bool;

    /// Mint the root node for one `parse_word` call (`AnalyzeWord`). Returns the handle later
    /// events thread through.
    fn analyze_word(&mut self, input: &Word) -> TraceHandle;

    fn begin_unapply_stratum(&mut self, parent: TraceHandle, stratum: StratumId, input: &Word) -> TraceHandle;
    fn end_unapply_stratum(&mut self, parent: TraceHandle, stratum: StratumId, output: &Word) -> TraceHandle;
    // ... begin/end_apply_stratum, begin/end_{un}apply_template, lexical_lookup, synthesize_word,
    // blocked, successful, failed -- one method per ITraceManager method, same argument shape
    // (Word arguments taken by &Word; the sink itself decides whether/when to clone, matching each
    // C# call site's own clone-or-reference choice from ss1.2).

    fn phonological_rule_applied(&mut self, parent: TraceHandle, rule: PRuleRef, subrule: i32, input: &Word, output: &Word) -> TraceHandle;
    fn phonological_rule_not_applied(&mut self, parent: TraceHandle, rule: PRuleRef, subrule: i32, input: &Word, reason: FailureReason, obj: FailureObj) -> TraceHandle;
    // ... symmetric morphological_rule_{applied,not_applied,unapplied,not_unapplied}, plus
    // compounding_rule_{not_applied,not_unapplied}.
}

/// The always-present no-op implementation -- monomorphized call sites against a concrete
/// `NoopSink` (rather than always going through `dyn TraceSink`) let the compiler inline
/// `is_tracing()` to a compile-time `false` and dead-code-eliminate every argument-computation at
/// that call site entirely when tracing is statically known off. See "performance" below.
pub struct NoopSink;
impl TraceSink for NoopSink {
    #[inline(always)]
    fn is_tracing(&self) -> bool { false }
    // every other method: `unreachable!()` -- never called, since every call site checks
    // `is_tracing()` first.
}
```

**Threading mechanics**: every function currently taking `cache: &RuleCache` / `budget: &StepBudget`
gains one more parameter, `trace: &mut impl TraceSink` (generic, not `&mut dyn TraceSink` — see
performance note), plus (where the function currently returns `Vec<Word>` with no handle) each
output `Word` also needs its own `trace: Option<TraceHandle>` field threaded alongside it, exactly
mirroring how C# threads `Word.CurrentTrace` through clones (`Word::clone`, `clone_without_alternatives`
already exist as the clone points to update).

**Performance**: the brief requires this be free when tracing is off, mirroring C#'s
`if (_traceManager.IsTracing)` guard, since it wraps the hot per-word parse path. Two complementary
techniques, both already precedented elsewhere in this codebase:

1. **Generic monomorphization over `TraceSink`, not `dyn TraceSink`.** `pg-parse::Morpher::parse_word`
   becomes generic over `T: TraceSink` (or gains a sibling `parse_word_traced<T: TraceSink>`,
   keeping the existing `parse_word` as a thin `parse_word_traced::<NoopSink>` wrapper — the latter
   is simpler and keeps every existing call site, including `pg-ffi` and every test, byte-for-byte
   unchanged). With `T = NoopSink`, `is_tracing()` inlines to a compile-time `false`, and every
   guarded block (`if trace.is_tracing() { ... }`) becomes dead code the compiler removes entirely
   — including the argument *computation* inside the block (e.g. cloning a `Word`, building a
   `FailureObj`), not just the call itself. This is strictly better than C#'s own guard (which still
   pays a runtime branch even when consistently false) and costs nothing beyond one extra type
   parameter threaded through the same functions that already thread `&RuleCache`/`&StepBudget`
   generically-by-reference.
2. Where full monomorphization through every `pg-rules` function is judged too invasive for a first
   landing (a real risk given the fan-out — dozens of functions across 6 files, §5's chunking
   reflects this), a `&dyn TraceSink` trait object is an acceptable fallback with a small, bounded
   cost (one vtable-dispatched `is_tracing()` bool check per call site, then early-return) — still
   far cheaper than any clone/allocation, and still zero *allocation* cost when off. This is the
   pragmatic middle ground if chunk-by-chunk landing (§5) needs an escape hatch partway through: land
   with `&dyn TraceSink` first, convert to full generics later only if profiling shows the vtable
   check actually matters (it should not — it is one branch plus one indirect call per rule attempt,
   dwarfed by the FST traversal work at the same call site).

Recommendation: **start with `&dyn TraceSink` for the initial landing (§5), leave the generic
monomorphization upgrade as an explicit, separately-measured follow-up** — this keeps the initial
diff bounded (no signature explosion propagating a type parameter through every generic-over-cache
function) while still meeting "zero-cost when off" in the sense that matters most (no allocation, no
`Word` cloning, no `FailureReason` computation — only a cheap bool check survives).

### 4.2 (b) The `FailureReason`-equivalent enum for Rust

Ported as a flat enum mirroring C#'s 24 values exactly by name (no renaming — a human diffing a
Rust trace against a C# trace should see identical reason names). Per §3's cross-reference:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureReason {
    ObligatorySyntacticFeatures,
    AllomorphCoOccurrenceRules,
    Environments,
    MorphemeCoOccurrenceRules,
    DisjunctiveAllomorph,
    SurfaceFormMismatch,
    Pattern,
    HeadPattern,
    NonHeadPattern,
    RequiredSyntacticFeatureStruct,
    HeadRequiredSyntacticFeatureStruct,
    NonHeadRequiredSyntacticFeatureStruct,
    HeadProdRestrictMprFeatures,
    NonHeadProdRestrictMprFeatures,
    RequiredMprFeatures,
    ExcludedMprFeatures,
    RequiredStemName,
    ExcludedStemName,
    PartialParse,
    BoundRoot,
    NonPartialRuleProhibitedAfterFinalTemplate,
    NonPartialRuleRequiredAfterNonFinalTemplate,
    MaxApplicationCount,
    // NOTE: no `None` variant -- Rust represents "no failure to report" as the absence of a
    // `FailureReason` (an `Option<FailureReason>` at call sites, or simply not calling the
    // not-applied/failed method at all), rather than porting C#'s sentinel value. C#'s `None`
    // exists only because `CurrentRuleResults`'s dictionary slot needs SOME default before a
    // subrule is evaluated (SynthesisRewriteSubruleSpec.cs:82) -- an implementation artifact of
    // that one side channel (S:3.3), not a real trace-worthy state. `rewrite.rs`'s ported side
    // channel (chunk 5, S5) should use `Option<FailureReason>` for the same slot instead.
}
```

**Flagged, not forced, per the brief's instruction**:

- `RequiredSyntacticFeatureStruct` at **apply time** (§3.2's timing mismatch): the ported reason
  value is the same, but early implementation chunks (chunk 4, before `synth_affix`/`ana_affix`
  themselves are changed to gate at apply time) will only be able to attach it at the final
  `allomorphs_valid_cached` position, one or more tree levels shallower than where a byte-identical
  C# trace would show it. This is called out explicitly rather than silently accepted: **chunk 4's
  own acceptance criterion (§5) requires moving the check into `synth_affix`/`ana_affix` itself**,
  not just wiring the existing final-validity-pass site, specifically so this timing gap closes
  rather than becoming permanent.
- `Pattern`/`HeadPattern`/`NonHeadPattern` fidelity against `rewrite.rs`/`metathesis.rs`'s
  FST-compiled matchers is capped at "the whole compiled automaton didn't match" — no
  finer-grained "which pattern element at which position" signal is available without deeper FST
  internals work (§3.3). This is documented as a **permanent, acceptable granularity gap**: the
  reason name is still correct (C# would also, ultimately, report `Pattern` for a total mismatch —
  the finer C# granularity only exists for *why a specific subrule with satisfiable syntactic-FS/MPR
  gates still failed to match*, which the ported per-subrule side channel (chunk 5) DOES capture
  correctly; only the position-level detail inside one subrule's own pattern is unavailable).
- Two distinct C# `TraceType` values (`StratumSynthesisOutput` fired from
  `NonFinalTemplateAppliedLast` vs. from `ApplicableTemplatesNotApplied`) share one `FailureReason`
  (`PartialParse`) but must remain **two distinct trace *events*** at the Rust `TraceSink` call-site
  level (§3.2's last row) — the enum itself needs no new value for this, but chunk 3 (§5) must split
  the one Rust `if` branch that currently conflates them.

No C# value is dropped, renamed, or fabricated. No Rust-only value is added (Rust's engine does not
currently distinguish anything C# doesn't — every place Rust's control flow is coarser than C#'s is
listed above as a flagged gap, not silently smoothed over).

### 4.3 (c) The trace tree/output data structure and surface

**Tree structure**: a `TraceType` enum (ported 1:1 from C#'s 19 real values —
`GenerateWords`/`WordAnalysis`/`StratumSynthesisInput`/`StratumSynthesisOutput`/
`StratumAnalysisInput`/`StratumAnalysisOutput`/`LexicalLookup`/`Blocked`/`WordSynthesis`/
`PhonologicalRuleAnalysis`/`PhonologicalRuleSynthesis`/`TemplateAnalysisInput`/
`TemplateAnalysisOutput`/`TemplateSynthesisInput`/`TemplateSynthesisOutput`/
`MorphologicalRuleAnalysis`/`MorphologicalRuleSynthesis`/`CompoundingRuleAnalysis`/
`CompoundingRuleSynthesis`/`Successful`/`Failed`) plus a `TraceNode` struct (`type_: TraceType`,
`source: TraceSource` — an enum over "which kind of rule/stratum/template/language object produced
this," replacing C#'s `IHCRule` OOP polymorphism with a closed Rust enum, since every concrete
source type is already known to the grammar model — `subrule_index: Option<i32>`, `input`/`output:
Option<WordSnapshot>` — a lightweight owned snapshot, not a live `&Word`, matching §1.2's
clone-discipline finding — `failure_reason: Option<FailureReason>`, `children: Vec<TraceNode>`).
The concrete `TraceSink` implementation (`TreeTraceSink`, the direct analog of C#'s `TraceManager`)
owns an arena (`Vec<TraceNode>` or a `Vec`-backed tree with parent-child indices, matching
`TraceHandle`'s design in §4.1) and appends/reassigns exactly as `TraceManager.cs` does.

**Surface**: **both** a CLI flag and (stubbed, deferred) an FFI surface, per the brief's framing
("CLI flag ... FFI, or both") — starting with CLI, since that is where the motivating use case (a
human, or a Sonnet agent, diagnosing a specific word) actually lives, and FFI has no real consumer
today (FieldWorks can't be compiled in this environment per prior memory) so building it out fully
now would be speculative.

- **New `pangloss parse <grammar.xml> <word> [--trace[=<file>]] [--trace-format=text|json]` subcommand**
  (today's CLI only has `batch`/`generate` — neither is the right shape for "trace exactly one
  word": `batch` is optimized for throughput over a word list with no per-word rich output, and
  adding `--trace` to it would mean either tracing every word in the batch — usually not what's
  wanted and expensive — or bolting on a second positional-argument-shaped "which word" filter that
  doesn't fit `batch`'s existing contract). `--trace` with no value writes the tree to stdout;
  `--trace=<file>` writes it there instead (leaving stdout for just the parse result, useful for
  scripting). Default format: an indented plain-text tree, one line per node, deliberately shaped to
  be visually diffable against a hand-transcribed or tooling-extracted C# trace — e.g.:
  ```
  WordAnalysis "atawirambo"
    StratumAnalysisInput surface
      MorphologicalRuleAnalysis "ku-Prefix" subrule=0
        StratumAnalysisOutput surface  shape=tawirambo
      MorphologicalRuleAnalysis "ku-Prefix" subrule=0  [not-unapplied]
    ...
    Successful  shape=atawirambo
  ```
  `--trace-format=json` emits the same tree as structured JSON (one object per node, `children` as a
  nested array) for tooling — specifically the side-by-side diff script §5 chunk 9 builds, and any
  future automated "trace-diff these two engines" harness. JSON is the right secondary format
  because a human-diffable text tree and a machine-diffable structured format are different design
  points (text optimizes for `diff -u` against a transcribed C# trace read by eye; JSON optimizes
  for a script asserting "these two trees have the same sequence of `(TraceType, rule_name,
  subrule_index, failure_reason)` tuples" without caring about exact indentation/whitespace).
- **FFI**: stub only in this design — `hc_parse_word`'s existing signature (`pg-ffi/src/parse.rs`)
  gains no new parameter yet; a future `hc_parse_word_traced` variant (or a flag bit on the existing
  call) would return a second buffer encoding the same JSON tree, gated the same way the CLI's
  `--trace` gate is (compute nothing extra unless requested). Left for a later, separately-scoped
  chunk once/if a real FFI consumer exists — not blocking §5's landing.

### 4.4 (d) Coexistence with `HC_STEP_STATS`/`HC_FST_PROFILE`

Read both mechanisms' actual implementation (not just their env-var names) to confirm the "different
concern" framing:

- **`HC_STEP_STATS`** (`pg-cli/src/main.rs:240-242`): reads `outcome.steps`, a single `usize` counter
  already accumulated on `pg_rules::stratum::StepBudget` (`stratum.rs`'s `tick()`/`steps()`) — the
  *count* of (un)application attempts across the whole `parse_word` call, with zero information
  about which rule or why. Printed once per word as one `STEPS\t{i}\t{word}\t{steps}` stderr line.
- **`HC_FST_PROFILE`** (`pg-cli/src/main.rs:247-281`, backed by `pg_fst::traverse`'s and
  `pg_rules::morph`'s module-level atomic counters, per those modules' own doc comments: *"a
  permanent diagnostic in the `HC_STEP_STATS` style (near-zero cost when unread: a few...)"*):
  accumulates call counts, total/max nanoseconds, and traversal-size sums for FST `run`/nondeterministic
  traversal/`distinct()`/dedup operations, globally, across the *entire process*, snapshotted and
  printed per word as `FSTPROF`/`DEDUPPROF` stderr lines. This is pure "where did the wall-clock
  time go" — it cannot even in principle answer "why didn't rule X apply to word Y," because it
  never records rule identity at all, only aggregate timing/size statistics across whatever FST
  operations happened to run.

Both are **unconditionally-accumulating, always-on counters gated only at the print site** (the
accumulation itself has no `if enabled` guard — it is genuinely "near-zero cost when unread," per
the doc comments, meaning a few atomic increments, not the clone-and-tree-build cost tracing would
require). Tracing is architecturally the opposite: the accumulation itself (cloning `Word`s, walking
the `TraceSink`) is the expensive part, which is exactly why §4.1 requires a real is-tracing guard at
every call site rather than "always accumulate, decide whether to print later." **Confirmed: these
must stay separate mechanisms, not merge.** Recommended coexistence contract: `--trace` and
`HC_STEP_STATS=1`/`HC_FST_PROFILE=1` are fully independent and may be set simultaneously with no
interaction — a traced word still bumps the FST/step counters exactly as before (those call sites
are untouched by this design), and the trace tree carries no timing information at all (matching
C#'s own `Trace`/`TraceType` design, which likewise has no timestamp field — trace answers "what/why
did the engine do," profiling answers "how long/how much did it cost").

---

## 5. Ordered implementation plan (landable chunks)

Each chunk should land as its own PR/commit, independently testable, mirroring the granularity of
`docs/p11-guesser-api-design.md`'s own §5. Earlier chunks deliberately touch the fewest call sites so
the plumbing (handle threading, sink trait, `NoopSink` no-op path) is validated end-to-end before
fanning out into `pg-rules`' dozens of sites.

0. **`pg-rules/src/trace.rs`: pure data types, no call sites.** `TraceType`, `FailureReason`,
   `TraceNode`/`TraceSource`, `TraceHandle`, the `TraceSink` trait (§4.1/§4.2/§4.3), `NoopSink`, and
   `TreeTraceSink` (the concrete tree-builder). Unit-testable in isolation: build a small tree by
   hand through the trait, assert the resulting `TreeTraceSink`'s structure. No `Word`/`Morpher`
   changes yet.
1. **`Word` gains `trace: Option<TraceHandle>` (`pg_rules::word::Word`).** Threaded through every
   existing clone point (`Word::clone`, `clone_without_alternatives`) exactly as C#'s `CurrentTrace`
   is threaded through `Word.Clone()` (`Word.cs:110`). `None` by default — every existing test's
   `Word` construction is unaffected (the field defaults via `Default`/explicit `None` at each
   construction site touched).
2. **Top-level plumbing: `pg-parse::Morpher::parse_word_traced<T: TraceSink>` (or `&dyn TraceSink`
   per §4.1's fallback), wired only at the outermost gates already in `morpher.rs` itself**:
   `analyze_word` (once, at entry — mints the root handle), `is_word_valid`'s three `Failed(...)`
   sites (`PartialParse`/`ObligatorySyntacticFeatures`, §3.1 direct matches), `is_match`'s
   `Successful`/`Failed(SurfaceFormMismatch)`. Deliberately the smallest possible end-to-end slice:
   proves the handle threads correctly from `parse_word`'s entry through to its exit without
   touching `pg-rules` at all. Existing `parse_word` becomes a thin wrapper calling this with
   `NoopSink`; every existing test and caller (`pg-ffi`, `pg-cli batch`) is unaffected.
   **Acceptance**: a new `pg-parse/tests/trace_gate.rs` fixture parses one trivially-valid word and
   one word that fails each of the three wired reasons, asserting the resulting tree's root + one
   child match the expected `(TraceType, FailureReason)` shape.
3. **`pg_rules::validity::allomorphs_valid_impl`** — the direct 1:1 cluster (§3.1): bound-root,
   stem-name (required/excluded), allomorph/morpheme co-occurrence, environments, disjunctive-recheck.
   This function's control flow already matches C#'s exactly line-for-line (per its own module doc),
   so this chunk is pure "add a `trace` parameter and call the right `TraceSink` method at each
   existing early return" — no logic changes. **Acceptance**: extend
   `pg-parse/tests/disjunctive_recheck_gate.rs`/`discontinuous_env_gate.rs`'s existing oracle-diffed
   fixtures to also assert the trace's `FailureReason` at the rejection point matches what a
   hand-inspection of the C# oracle's equivalent run would show (these fixtures already know the
   *outcome*; this chunk adds a same-fixture assertion on *why*).
4. **`pg_rules::morph::synth_affix`/`synth_affix_cached`/`ana_affix`/`ana_affix_cached`,
   `synth_realizational`/`ana_realizational`, `synth_compound`/`ana_compound` families** — the
   busiest chunk (§1.5's `SynthesisAffixProcessRule.cs`/`SynthesisCompoundingRule.cs`, ~27 combined
   call sites). Each already-identified early-return gate (§3.1/§3.2's tables) gains its
   `FailureReason` and a `TraceSink` call. **This chunk also closes §4.2's flagged timing gap**:
   `RequiredSyntacticFeatureStruct`/`RequiredMprFeatures`/`ExcludedMprFeatures` move from
   final-validity-only (`validity.rs`) to being checked (and, on failure, traced) at apply time
   inside `synth_affix` itself, matching C#'s exact call-site position — `validity.rs`'s own copy of
   these checks stays as the final-word-validity backstop it always was (a word that somehow reaches
   final validity with a violation should still be caught there too, exactly as C# re-checks via the
   disjunctive loop), it just stops being the *only* place the reason can be observed. **Acceptance**:
   a same-shape trace-assertion extension to the existing `morph_gate.rs`/`redup_and_free_fluctuation_gate.rs`/
   `nonhead_resolution_gate.rs` oracle-diffed fixtures.
5. **`pg_rules::stratum.rs`** — stratum/template bookends (`BeginUnapplyStratum`/`EndUnapplyStratum`/
   `BeginApplyStratum`/`EndApplyStratum`, template begin/end, `Blocked`,
   `NonFinalTemplateAppliedLast`/`ApplicableTemplatesNotApplied` split per §3.2's last row), plus
   `apply_one_mrule`'s `MaxApplicationCount`/`MaxStemCount` gates. **Also**: mirror C#'s two
   tracing-changes-control-flow interactions flagged in §2 — tracing must force
   `merge_equivalent = false` (matching C#'s `!_morpher.TraceManager.IsTracing` guard on
   `mergeEquivalentAnalyses`, `AnalysisStratumRule.cs:152`) and must bypass the M6 memo/Gate-B
   shortcuts the same way C#'s own "ground rule 1" (already documented in `morpher.rs`'s comments at
   the ANALYSIS-SCOPE construction site) bypasses `AnalysisScope`'s lexical gating while tracing —
   **a trace must reflect the unmemoized, unshortcut engine exactly**, or it will show rule attempts
   that the fast path silently skipped, producing a trace that doesn't match what actually happened
   when tracing is off. **Acceptance**: new fixture asserting `--trace` output stays identical with
   `--memo=on`/`--memo=off` (the memo must never change trace content, only whether the *live* code
   path used a replay) — a strong regression guard for exactly this hazard.
6. **`pg_rules::rewrite.rs`/`metathesis.rs`** — phonological rule tracing, including the per-subrule
   side channel (§3.3) `rewrite.rs`'s synthesis path needs for `SynthesisRewriteRule`-equivalent
   reason reporting. The largest net-new mechanism in this plan (not just adding parameters to
   existing early returns). **Acceptance**: extend `rewrite_gate.rs`'s oracle-diffed fixtures with
   subrule-level reason assertions; explicitly document (in-code, matching §3.3) the FST-granularity
   ceiling — a fixture asserting the `Pattern` fallback fires correctly for a total-mismatch case is
   sufficient; no fixture should assert position-level detail Rust cannot produce.
7. **CLI surface**: `pangloss parse` subcommand (§4.3), text + JSON renderers. **Acceptance**: golden
   test comparing the fixed text-tree output for a small hand-built grammar/word against a checked-in
   expected string (a straightforward snapshot test, the same style `pg-cli`'s existing TSV tests
   use).
8. **(Deferred, not blocking) FFI stub** — `hc_parse_word_traced` or a flag bit, per §4.3's stub note.
9. **Conformance: a side-by-side trace-diff harness.** A script (Python or PowerShell, matching this
   repo's existing `parse_compare.py`/`tools/*.ps1` conventions) that runs a word through both
   `pangloss parse --trace=json` and the live C# oracle's own trace output (C# already has a working
   `TraceManager` — this harness's job is extracting its tree into the same JSON shape, likely via a
   small oracle-side driver reusing the existing `hc.dll` harness this project already has for
   conformance fixtures), then diffs the two trees at the `(TraceType, rule/subrule identity,
   FailureReason)` level, ignoring cosmetic differences (exact `Word` shape rendering, ordering
   within a sibling list where both engines' set-based dedup makes order non-canonical). This is the
   chunk that actually delivers the motivating use case end-to-end — §6 walks through what running it
   against a P10-shaped repro would have shown.

Max ~2 implementation agents at once per this project's existing convention (chunks 3-6 all touch
`pg-rules` and would collide on shared files; land sequentially or split by file ownership within a
wave, same as the plan doc's own "Sequencing" section for P1-P13).

---

## 6. Acceptance test: would this have caught P10?

P10 (`rust-optimizations-phase2.md` §P10, DONE `63b0a89f`) is a good test case because its root
cause — a missing `StrRep` (character identity) dimension in Rust's `SegmentNaturalClass`/literal-
segment matching — manifested as **exactly** the kind of "which rule fired vs. which the C# oracle
fired" divergence this design exists to localize. Quoting the plan doc's own framing: *"the real
gap: the port dropped C#'s `StrRep` identity dimension... every `SegmentNaturalClass`/literal-
segment constraint/environment degenerated to 'matches any segment'"* — symptom 1 was *"null slot
never chosen in synthesis (spurious over-match let the disjunctive break fire before the
zero-allomorph subrule was tried)."*

This is a `SynthesisAffixProcessRule`-shaped bug (a class-prefix rule with a disjunctive slot of
allomorphs, one of which is the "null"/zero allomorph `[(^0)(*0)(&0)∅]?`) — exactly chunk 4's
territory (§5). Walking through what a side-by-side trace (chunk 9's harness) would have shown, for
a Sena word this rule applies to:

**C# trace** (reconstructed from the rule's actual gate order, `SynthesisAffixProcessRule.cs:182-246`):
```
MorphologicalRuleSynthesis "ClassPrefixRule" subrule=0  [not-applied, reason=Pattern]
    <- subrule 0's LHS pattern requires the following segment to belong to a specific StrRep-
       identified class; C#'s real StrRep-aware matching correctly rejects this word's actual
       segment identity here.
MorphologicalRuleSynthesis "ClassPrefixRule" subrule=<null-allomorph-index>  [applied]
    Output: ...
```

**Rust trace, pre-P10-fix** (reconstructed from the then-current `synth_affix` control flow, which
this design's chunk 4 would have instrumented): because Rust's `SegmentNaturalClass`/environment
matching had no `StrRep` identity lane at all, subrule 0's LHS pattern check (the FST-compiled
match, §3.3's granularity ceiling notwithstanding — a *total* match/no-match is exactly what's at
stake here, not a position-level detail) spuriously **succeeds** where C#'s correctly fails:
```
MorphologicalRuleSynthesis "ClassPrefixRule" subrule=0  [applied]
    Output: ...
```
— and, per `SynthesisAffixProcessRule.cs:235-242`'s disjunctive-break logic (already ported in
Rust's `synth_affix`, `morph.rs:1209-1223`, `next_free_fluctuates`/environments/required-syn-fs
check before `break`), once subrule 0 succeeds and doesn't free-fluctuate with subrule 1, the loop
**stops** — subrule for the null allomorph is never even attempted, so it never even reaches a
"not applied" trace event; it simply never appears in the Rust trace's children list at all.

**What the diff would show**: at the exact tree position `WordAnalysis > ... >
MorphologicalRuleSynthesis["ClassPrefixRule"]`, the C# trace has **two** children (subrule 0
not-applied/`Pattern`, subrule-null applied); the Rust trace has **one** (subrule 0 applied) and is
missing the null-allomorph child entirely. A diff harness (chunk 9) comparing children counts and
`(subrule_index, applied?)` tuples at each `MorphologicalRuleSynthesis` node would flag this exact
node as the **first point of divergence** between the two engines' traces — which is precisely where
P10's actual root cause lived. Critically, this localizes the search to "why did subrule 0's pattern
match here when it shouldn't have" — i.e., straight to the LHS-pattern compile/match machinery for
that one allomorph — rather than the multi-symptom, multi-day dissection the plan doc describes
actually happening (oracle-diffing three separate downstream symptoms — wrong null-slot choice, a
93% step-cap truncation rate, and W3.2 false-rejections — and reasoning backward from all three to
the one shared missing dimension). A trace diff would have shown symptom 1 directly, at the correct
rule and subrule, on the very first word exercising this rule, rather than requiring inference from
aggregate corpus statistics (V2's 91.7% step-cap-truncation measurement) and a second unrelated-
looking validity-gate bug (W3.2 false rejection) to triangulate toward the same cause.

**Caveat, honestly stated**: this walkthrough is a retrospective reconstruction (P10 was actually
diagnosed the hard way, before this design existed) — it demonstrates the design's mechanism would
have surfaced the divergence at the right place, not a claim that a trace diff alone would have
handed over the *fix* (the `StrRep` identity-lane design itself, §P10's `bridge.rs` addition, still
required understanding *why* the pattern spuriously matched — the trace narrows "where," a human or
Fable agent still reasons about "why" from there). That narrowing — from "some words in a 96%-vs-
50% corpus-level regression" down to "this exact rule/subrule, on this exact word, diverges from C#
right here" — is the concrete debugging value John asked for, and chunk 9's harness is what would
deliver it for the *next* P10-shaped bug this port produces.
