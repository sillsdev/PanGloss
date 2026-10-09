//! The `TraceManager` port: rule-by-rule parse tracing.
//!
//! Pure data types plus the `TraceSink` trait. No call sites live
//! here — `pg-rules`/`pg-parse`'s own functions gain `trace` parameters elsewhere. This module
//! is unit-testable in isolation: build a small tree by hand through the trait and assert the
//! resulting `TreeTraceSink`'s structure (see the tests below, which pin the two trickiest pieces
//! of C#'s `TraceManager.cs` cursor semantics: the "applying a rule reassigns the cursor so later
//! events nest UNDER it" behavior, and `SynthesizeWord`'s two-levels-deep
//! `curTrace.Children.Last.Children.Add` reach).
//!
//! ## Zero-cost-when-off
//! `NoopSink` is the always-present no-op `TraceSink` every existing call path uses by default
//! (`Morpher::parse_word` stays a thin wrapper over a traced variant called with `NoopSink`).
//! Every real call site must check `TraceSink::is_tracing` BEFORE doing any other
//! trace-related work (cloning a `Word`, computing a `FailureReason`) — the single branch that
//! must be free when tracing is off.
//!
//! ## Deliberate simplification vs. the design sketch
//! The original sketch takes `input`/`output` as `&Word` and leaves "the sink
//! itself decides whether/when to clone" up to the implementation. This port's `TreeTraceSink`
//! always snapshots via `Word::clone()` (a whole owned `Word`, not a hand-trimmed lighter struct) —
//! simpler than threading a separate `WordSnapshot` type through every call site, and costs nothing
//! on the no-op path (the clone only happens inside `TreeTraceSink`'s methods, never inside
//! `NoopSink`'s, and call sites must check `is_tracing()` before calling either).
//! The original port omitted C#'s free-form `failureObj`. Rich diagnostics now opt in to
//! `FailureContext`: rejection owners capture their actual operands after the gate fires.
//! Ordinary tracing retains its existing snapshots without formatting this extra evidence.
use std::cell::{Cell, RefCell};

use pg_grammar_model::model::{
    AllomorphId, CoOccurrenceAdjacency, LexEntryId, MRuleId, MorphemeId, MprSet, PRuleId,
    StratumId, TemplateId,
};

use crate::word::Word;

/// A stable handle into the trace tree a `TraceSink` is building — the Rust analog of C#'s
/// `Word.CurrentTrace`, carried as an explicit value (an arena index) rather than a mutated field.
/// `Word` itself carries `trace: Option<TraceHandle>` mirroring `CurrentTrace: object`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceHandle(u32);

impl TraceHandle {
    /// A placeholder handle for call sites that thread a `parent: TraceHandle` parameter
    /// unconditionally (so the traced and untraced code paths share one function body) but only
    /// ever dereference it behind an `is_tracing()` guard. Never produced by a real
    /// `TraceSink` and never valid to pass to one — `NoopSink`'s methods that would read it are
    /// all `unreachable!()`, so this value is never actually looked up.
    pub const DUMMY: TraceHandle = TraceHandle(u32::MAX);

    /// The document-local arena index, for linking published events within one trace.
    pub fn index(self) -> u32 {
        assert_ne!(self, Self::DUMMY, "a dummy handle has no document identity");
        self.0
    }
}

/// `TraceType` (C# `TraceType`, `Trace.cs`, 19 real values ported 1:1 by name; no `None` variant —
/// see `FailureReason`'s doc for why Rust drops C#'s sentinel defaults in this port).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceType {
    GenerateWords,
    WordAnalysis,
    StratumSynthesisInput,
    StratumSynthesisOutput,
    StratumAnalysisInput,
    StratumAnalysisOutput,
    LexicalLookup,
    Blocked,
    WordSynthesis,
    PhonologicalRuleAnalysis,
    PhonologicalRuleSynthesis,
    TemplateAnalysisInput,
    TemplateAnalysisOutput,
    TemplateSynthesisInput,
    TemplateSynthesisOutput,
    MorphologicalRuleAnalysis,
    MorphologicalRuleSynthesis,
    CompoundingRuleAnalysis,
    CompoundingRuleSynthesis,
    Successful,
    Failed,
}

/// The rule/stratum/template/language object that produced a `TraceNode` (C#'s `IHCRule Source`,
/// replacing C#'s OOP polymorphism with a closed enum — every concrete source kind is already known
/// to the grammar model, per §4.3). `None` for the leaf-most `Successful`/`Failed` nodes,
/// which are keyed off a `Word`, not a rule (matching C#'s `Source == null` for those).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceSource {
    /// The language/grammar itself — the root `WordAnalysis`/`GenerateWords` node's source.
    Language,
    Stratum(StratumId),
    Template(TemplateId),
    MorphRule(MRuleId),
    PhonRule(PRuleId),
    None,
}

/// `FailureReason` (C# `ITraceManager.cs`'s enum), ported 1:1 by name — a human diffing a Rust trace
/// against a C# trace should see identical reason names. Cross-referenced against the C# original:
/// 11 of these are already exact 1:1 gates in Rust today, 9 more exist as gates that fold two C#
/// reasons into one bool (need to report which fired), and `Pattern`/`HeadPattern`/`NonHeadPattern`
/// are the residual last-resort case. Two gaps are carried forward, NOT yet closed: the
/// `RequiredSyntacticFeatureStruct` apply-time timing mismatch, and the FST-granularity ceiling on
/// `Pattern`.
///
/// No `None` variant: C#'s sentinel exists only to seed `CurrentRuleResults`' dictionary slot before
/// a subrule is evaluated (`SynthesisRewriteSubruleSpec.cs:82`) — an implementation artifact of that
/// one side channel, not a real trace-worthy state. Rust represents "no failure to report" as the
/// absence of a `FailureReason` (`Option<FailureReason>`).
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
}

/// The lookup strategy that produced a lexical lookup's candidate roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupMode {
    Lexicon,
    Guesser,
}

/// A completed lookup result. `match_count` counts root candidates materialized and returned by
/// the lookup owner: grammar entries are expanded by allomorph, while guesser outputs are counted
/// after the guesser's per-pattern deduplication. It does not count distinct lexical entries or
/// candidates accepted by later synthesis and validity gates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LookupResult {
    pub mode: LookupMode,
    pub match_count: usize,
}

/// The evaluator's outcome for one slot on one retained template path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateSlotStatus {
    Applied,
    OptionalSkipped,
    RequiredUnfilled,
    NotReached,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateSlotOutcome {
    pub slot_index: usize,
    pub status: TemplateSlotStatus,
    pub selected_rule: Option<MRuleId>,
}

/// The completion gate that rejected this candidate; it does not identify a prior causal event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartialParseCause {
    RemainingAnalyzedRules,
    RemainingRulesInStratum,
    NonFinalTemplateAppliedLast,
    ApplicableTemplatesNotApplied,
}

/// Runtime identity captured by the rejecting owner, resolved only for display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceObject {
    Allomorph(AllomorphId),
    Morpheme(MorphemeId),
    MorphRule(MRuleId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentResult {
    pub index: usize,
    pub require: bool,
    pub accepted: bool,
    pub source_id: Option<String>,
    pub authored_text: Option<String>,
}

/// A typed payload whose operands come from one existing rejection computation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RejectionEvidence {
    SyntacticFeatures {
        object: TraceObject,
        required: pg_featstruct::FeatureStruct,
        actual: pg_featstruct::FeatureStruct,
    },
    MprFeatures {
        object: TraceObject,
        required: MprSet,
        excluded: MprSet,
        actual: MprSet,
    },
    CoOccurrence {
        object: TraceObject,
        constraint_owner: TraceObject,
        rule_index: usize,
        require: bool,
        adjacency: CoOccurrenceAdjacency,
        others: Vec<TraceObject>,
        actual: Vec<TraceObject>,
    },
    Environments {
        object: TraceObject,
        constraint_owner: TraceObject,
        start: u32,
        end: u32,
        alternatives: Vec<EnvironmentResult>,
    },
}

/// Evidence captured by the rejection owner, without re-evaluating a predicate.
/// Values are display representations of the actual gate inputs, not new parser decisions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FailureContext {
    pub required: Option<String>,
    pub actual: Option<String>,
    pub environment: Option<String>,
    pub evidence: Option<RejectionEvidence>,
}

/// One node in the trace tree (C# `Trace`, `Trace.cs`). `input`/`output` are owned snapshots (see
/// this module's doc for the simplification vs. the design sketch's `WordSnapshot`), not live
/// references — matching §1.2's clone-discipline finding.
#[derive(Clone, Debug)]
pub struct TraceNode {
    pub type_: TraceType,
    pub source: TraceSource,
    /// Which subrule/allomorph fired (`-1`/`None` when not applicable — stratum/template/word-level
    /// nodes never set this).
    pub subrule_index: Option<i32>,
    pub input: Option<Word>,
    pub output: Option<Word>,
    pub failure_reason: Option<FailureReason>,
    pub failure_context: Option<FailureContext>,
    /// Present only when rich trace capture recorded the lookup's completed output.
    pub lookup_result: Option<LookupResult>,
    pub template_slots: Option<Vec<TemplateSlotOutcome>>,
    pub partial_parse_cause: Option<PartialParseCause>,
    /// On a `Blocked` node, the family entry whose seed replaced the rule's output.
    pub blocked_by_entry: Option<LexEntryId>,
    pub children: Vec<TraceHandle>,
}

impl TraceNode {
    fn new(type_: TraceType, source: TraceSource) -> Self {
        TraceNode {
            type_,
            source,
            subrule_index: None,
            input: None,
            output: None,
            failure_reason: None,
            failure_context: None,
            lookup_result: None,
            template_slots: None,
            partial_parse_cause: None,
            blocked_by_entry: None,
            children: Vec::new(),
        }
    }
}

/// The zero-cost-when-off guard + event-emission surface (C# `ITraceManager`). Mirrors C#'s
/// `if (_morpher.TraceManager.IsTracing)` idiom: every call site must check `Self::is_tracing`
/// before doing ANY other trace-related work.
///
/// One instance per `parse_word` call (never shared/reused across words — mirrors C#'s per-`Morpher`
/// -but-effectively-per-call `IsTracing` check plus per-word `Trace` tree root).
///
/// **Interior mutability, not `&mut self`** (a correction made empirically during chunk 4/5
/// development, not part of the original chunk-0 landing): the rule-application cascades
/// (`pg_rules::cascade::Cascade::linear`/`combination`) take their per-rule closure as `A: Fn(usize,
/// &T) -> Vec<T>`, not `FnMut` -- a real `&mut dyn TraceSink` captured by such a closure cannot be
/// re-borrowed across the closure's many sequential (but not simultaneously live) invocations
/// without fighting the borrow checker. Every method here therefore takes `&self`;
/// `TreeTraceSink` wraps its arena in a `RefCell`/`Cell` to get the mutation back. This is the
/// standard Rust shape for a "logger" trait threaded through combinators that don't need unique
/// access, and it is what let chunks 4/5 pass `trace: &dyn TraceSink` through the cascade closures
/// as a plain shared reference instead of threading a raw pointer or restructuring the cascade API.
pub trait TraceSink {
    /// Mirrors C# `ITraceManager.IsTracing`.
    fn is_tracing(&self) -> bool;

    /// Rich diagnostic evidence is separately opt-in; ordinary tracing does not format it.
    fn captures_failure_context(&self) -> bool {
        false
    }

    /// Opt-in owner facts beyond rejection operands, including completed lookup and slot results.
    fn captures_details(&self) -> bool {
        false
    }

    fn end_unapply_template_with_slots(
        &self,
        _parent: TraceHandle,
        _template: TemplateId,
        _output: &Word,
        _unapplied: bool,
        _slots: &[TemplateSlotOutcome],
    ) -> TraceHandle {
        panic!("this trace sink cannot capture template slot outcomes");
    }

    fn end_apply_template_with_slots(
        &self,
        _parent: TraceHandle,
        _template: TemplateId,
        _output: &Word,
        _applied: bool,
        _slots: &[TemplateSlotOutcome],
    ) -> TraceHandle {
        panic!("this trace sink cannot capture template slot outcomes");
    }

    fn failed_partial_parse(
        &self,
        _parent: TraceHandle,
        _word: &Word,
        _cause: PartialParseCause,
    ) -> TraceHandle {
        panic!("this trace sink cannot capture partial parse causes");
    }

    /// Called only after the owner has emitted the exact failed event.
    fn set_failure_context(&self, _event: TraceHandle, _context: FailureContext) {
        panic!("this trace sink cannot capture failure context");
    }

    /// Attach the result from the lexical lookup owner after its candidate list is fully built.
    fn complete_lexical_lookup(&self, _event: TraceHandle, _result: LookupResult) {
        panic!("this trace sink cannot capture lexical lookup results");
    }

    /// Mint the root node for one `parse_word` call (`AnalyzeWord`). Returns the handle later events
    /// thread through as `parent`.
    fn analyze_word(&self, input: &Word) -> TraceHandle;

    /// Mint the root node for one `GenerateWords` call (a distinct root from `AnalyzeWord`'s).
    fn generate_words(&self) -> TraceHandle;

    fn begin_unapply_stratum(
        &self,
        parent: TraceHandle,
        stratum: StratumId,
        input: &Word,
    ) -> TraceHandle;
    fn end_unapply_stratum(
        &self,
        parent: TraceHandle,
        stratum: StratumId,
        output: &Word,
    ) -> TraceHandle;
    fn begin_apply_stratum(
        &self,
        parent: TraceHandle,
        stratum: StratumId,
        input: &Word,
    ) -> TraceHandle;
    fn end_apply_stratum(
        &self,
        parent: TraceHandle,
        stratum: StratumId,
        output: &Word,
    ) -> TraceHandle;

    fn begin_unapply_template(
        &self,
        parent: TraceHandle,
        template: TemplateId,
        input: &Word,
    ) -> TraceHandle;
    fn end_unapply_template(
        &self,
        parent: TraceHandle,
        template: TemplateId,
        output: &Word,
        unapplied: bool,
    ) -> TraceHandle;
    fn begin_apply_template(
        &self,
        parent: TraceHandle,
        template: TemplateId,
        input: &Word,
    ) -> TraceHandle;
    fn end_apply_template(
        &self,
        parent: TraceHandle,
        template: TemplateId,
        output: &Word,
        applied: bool,
    ) -> TraceHandle;

    /// `NonFinalTemplateAppliedLast` — always `FailureReason::PartialParse` (§1.1/§4.2's last note).
    /// P12 chunk 5 correction (verified against `ITraceManager.cs:72`/`TraceManager.cs:145-153`):
    /// C#'s actual signature is `(Stratum stratum, Word word)` -- the SOURCE is the enclosing
    /// stratum, not a template (chunk 0's original `TemplateId` param was never checked against the
    /// oracle at landing time; fixed here at chunk 5, its first real call site, before any caller
    /// existed to break).
    fn non_final_template_applied_last(
        &self,
        parent: TraceHandle,
        stratum: StratumId,
        output: &Word,
    ) -> TraceHandle;
    /// `ApplicableTemplatesNotApplied` — always `FailureReason::PartialParse`, but a DISTINCT trace
    /// event from `Self::non_final_template_applied_last` (§3.2's last row / §4.2's third bullet).
    /// P12 chunk 5 correction: C#'s actual signature (`ITraceManager.cs:73`) is also `(Stratum
    /// stratum, Word word)` -- chunk 0 omitted the stratum source entirely; added here.
    fn applicable_templates_not_applied(
        &self,
        parent: TraceHandle,
        stratum: StratumId,
        input: &Word,
    ) -> TraceHandle;

    fn phonological_rule_unapplied(
        &self,
        parent: TraceHandle,
        rule: PRuleId,
        subrule: i32,
        output: &Word,
    ) -> TraceHandle;
    fn phonological_rule_not_unapplied(
        &self,
        parent: TraceHandle,
        rule: PRuleId,
        subrule: i32,
        input: &Word,
    ) -> TraceHandle;
    fn phonological_rule_applied(
        &self,
        parent: TraceHandle,
        rule: PRuleId,
        subrule: i32,
        input: &Word,
        output: &Word,
    ) -> TraceHandle;
    fn phonological_rule_not_applied(
        &self,
        parent: TraceHandle,
        rule: PRuleId,
        subrule: i32,
        input: &Word,
        reason: FailureReason,
    ) -> TraceHandle;

    fn morphological_rule_unapplied(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        subrule: i32,
        output: &Word,
    ) -> TraceHandle;
    /// Dead-end-attribution census addition (`deadend_census.rs`): unlike the
    /// synthesis-side `Self::morphological_rule_not_applied` (which has carried a `FailureReason`
    /// since P12 chunk 4), this method originally carried none — there was no call site at all
    /// (`pg_rules::stratum::StratumAnalyzer`'s analysis cascade has never been traced; confirmed by
    /// grep, zero production or test callers before this census). Adding the `reason` parameter here
    /// is therefore a pure signature change on dead code, not a behavior change to anything that
    /// exists today — see `crate::morph::analyze_cached_traced`'s doc for the first real caller.
    fn morphological_rule_not_unapplied(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        subrule: i32,
        input: &Word,
        reason: FailureReason,
    ) -> TraceHandle;
    fn morphological_rule_applied(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        subrule: i32,
        output: &Word,
    ) -> TraceHandle;
    fn morphological_rule_not_applied(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        subrule: i32,
        input: &Word,
        reason: FailureReason,
    ) -> TraceHandle;

    fn compounding_rule_not_unapplied(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        subrule: i32,
        input: &Word,
        reason: FailureReason,
    ) -> TraceHandle;
    fn compounding_rule_unapplied(
        &self,
        _parent: TraceHandle,
        _rule: MRuleId,
        _subrule: i32,
        _output: &Word,
    ) -> TraceHandle {
        panic!("this trace sink cannot record compound analysis results");
    }
    fn compounding_rule_not_applied(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        input: &Word,
        reason: FailureReason,
    ) -> TraceHandle;

    /// `LexicalLookup(stratum, input)` — one per stratum's root-allomorph search (both the real
    /// lexicon path and P11's guesser pattern-match path reuse this exact hook).
    fn lexical_lookup(&self, parent: TraceHandle, stratum: StratumId, input: &Word) -> TraceHandle;

    /// An applied rule's output was replaced by `by_entry`, an entry in the root's lexical family
    /// whose syntactic features the output subsumes (C# `Word.CheckBlocking`). `output` is the
    /// replacement word.
    fn blocked(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        output: &Word,
        by_entry: LexEntryId,
    ) -> TraceHandle;

    fn successful(&self, parent: TraceHandle, word: &Word) -> TraceHandle;
    fn failed(&self, parent: TraceHandle, word: &Word, reason: FailureReason) -> TraceHandle;

    /// C#'s subtlest cursor move (§1.2): appends **two levels deep** —
    /// `parent`'s LAST child's children, not `parent`'s own children directly — because by the time
    /// `SynthesizeWord` fires the cursor logically needs to descend into the just-appended
    /// `LexicalLookup` node's children. `parent` here is the cursor BEFORE this call (unchanged by
    /// `LexicalLookup`, which does not reassign it) — see this module's tests.
    fn synthesize_word(&self, parent: TraceHandle, input: &Word) -> TraceHandle;
}

/// The always-present no-op `TraceSink` (see the `impl` below for why every method but
/// [`is_tracing`](TraceSink::is_tracing) panics). Kept as a concrete (not `dyn`) type at call sites
/// where possible so `is_tracing()` can const-fold away; some call sites accept `&dyn TraceSink`
/// at the boundary instead.
pub struct NoopSink;

impl TraceSink for NoopSink {
    #[inline(always)]
    fn is_tracing(&self) -> bool {
        false
    }
    fn analyze_word(&self, _input: &Word) -> TraceHandle {
        unreachable!("NoopSink methods are never called; every call site checks is_tracing() first")
    }
    fn generate_words(&self) -> TraceHandle {
        unreachable!()
    }
    fn begin_unapply_stratum(&self, _p: TraceHandle, _s: StratumId, _i: &Word) -> TraceHandle {
        unreachable!()
    }
    fn end_unapply_stratum(&self, _p: TraceHandle, _s: StratumId, _o: &Word) -> TraceHandle {
        unreachable!()
    }
    fn begin_apply_stratum(&self, _p: TraceHandle, _s: StratumId, _i: &Word) -> TraceHandle {
        unreachable!()
    }
    fn end_apply_stratum(&self, _p: TraceHandle, _s: StratumId, _o: &Word) -> TraceHandle {
        unreachable!()
    }
    fn begin_unapply_template(&self, _p: TraceHandle, _t: TemplateId, _i: &Word) -> TraceHandle {
        unreachable!()
    }
    fn end_unapply_template(
        &self,
        _p: TraceHandle,
        _t: TemplateId,
        _o: &Word,
        _u: bool,
    ) -> TraceHandle {
        unreachable!()
    }
    fn begin_apply_template(&self, _p: TraceHandle, _t: TemplateId, _i: &Word) -> TraceHandle {
        unreachable!()
    }
    fn end_apply_template(
        &self,
        _p: TraceHandle,
        _t: TemplateId,
        _o: &Word,
        _a: bool,
    ) -> TraceHandle {
        unreachable!()
    }
    fn non_final_template_applied_last(
        &self,
        _p: TraceHandle,
        _s: StratumId,
        _o: &Word,
    ) -> TraceHandle {
        unreachable!()
    }
    fn applicable_templates_not_applied(
        &self,
        _p: TraceHandle,
        _s: StratumId,
        _i: &Word,
    ) -> TraceHandle {
        unreachable!()
    }
    fn phonological_rule_unapplied(
        &self,
        _p: TraceHandle,
        _r: PRuleId,
        _s: i32,
        _o: &Word,
    ) -> TraceHandle {
        unreachable!()
    }
    fn phonological_rule_not_unapplied(
        &self,
        _p: TraceHandle,
        _r: PRuleId,
        _s: i32,
        _i: &Word,
    ) -> TraceHandle {
        unreachable!()
    }
    fn phonological_rule_applied(
        &self,
        _p: TraceHandle,
        _r: PRuleId,
        _s: i32,
        _i: &Word,
        _o: &Word,
    ) -> TraceHandle {
        unreachable!()
    }
    fn phonological_rule_not_applied(
        &self,
        _p: TraceHandle,
        _r: PRuleId,
        _s: i32,
        _i: &Word,
        _reason: FailureReason,
    ) -> TraceHandle {
        unreachable!()
    }
    fn morphological_rule_unapplied(
        &self,
        _p: TraceHandle,
        _r: MRuleId,
        _s: i32,
        _o: &Word,
    ) -> TraceHandle {
        unreachable!()
    }
    fn morphological_rule_not_unapplied(
        &self,
        _p: TraceHandle,
        _r: MRuleId,
        _s: i32,
        _i: &Word,
        _reason: FailureReason,
    ) -> TraceHandle {
        unreachable!()
    }
    fn morphological_rule_applied(
        &self,
        _p: TraceHandle,
        _r: MRuleId,
        _s: i32,
        _o: &Word,
    ) -> TraceHandle {
        unreachable!()
    }
    fn morphological_rule_not_applied(
        &self,
        _p: TraceHandle,
        _r: MRuleId,
        _s: i32,
        _i: &Word,
        _reason: FailureReason,
    ) -> TraceHandle {
        unreachable!()
    }
    fn compounding_rule_not_unapplied(
        &self,
        _p: TraceHandle,
        _r: MRuleId,
        _s: i32,
        _i: &Word,
        _reason: FailureReason,
    ) -> TraceHandle {
        unreachable!()
    }
    fn compounding_rule_not_applied(
        &self,
        _p: TraceHandle,
        _r: MRuleId,
        _i: &Word,
        _reason: FailureReason,
    ) -> TraceHandle {
        unreachable!()
    }
    fn lexical_lookup(&self, _p: TraceHandle, _s: StratumId, _i: &Word) -> TraceHandle {
        unreachable!()
    }
    fn blocked(&self, _p: TraceHandle, _r: MRuleId, _o: &Word, _e: LexEntryId) -> TraceHandle {
        unreachable!()
    }
    fn successful(&self, _p: TraceHandle, _w: &Word) -> TraceHandle {
        unreachable!()
    }
    fn failed(&self, _p: TraceHandle, _w: &Word, _reason: FailureReason) -> TraceHandle {
        unreachable!()
    }
    fn synthesize_word(&self, _p: TraceHandle, _i: &Word) -> TraceHandle {
        unreachable!()
    }
}

/// The concrete tree-builder (C# `TraceManager`, `TraceManager.cs`): an arena of `TraceNode`s
/// (`TraceHandle` = index) built by appending exactly as `TraceManager.cs` does, including the two
/// cursor subtleties documented on `TraceSink::synthesize_word` and
/// `TraceSink::morphological_rule_applied`.
pub struct TreeTraceSink {
    nodes: RefCell<Vec<TraceNode>>,
    root: Cell<Option<TraceHandle>>,
    capture_failure_context: bool,
}

impl Default for TreeTraceSink {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeTraceSink {
    /// Retain owner-provided rejection evidence for an explicit rich diagnostic request.
    pub fn with_failure_context() -> Self {
        Self {
            capture_failure_context: true,
            ..Self::new()
        }
    }
    pub fn new() -> Self {
        TreeTraceSink {
            nodes: RefCell::new(Vec::new()),
            root: Cell::new(None),
            capture_failure_context: false,
        }
    }

    /// The tree's root node handle, if one has been minted yet (`analyze_word`/`generate_words`).
    pub fn root(&self) -> Option<TraceHandle> {
        self.root.get()
    }

    /// Read one node by handle (for rendering/tests). Clones out of the `RefCell` so the borrow
    /// does not outlive this call — callers needing the whole tree walk by handle repeatedly
    /// (see this module's tests and `pg-cli`'s renderer, chunk 7).
    pub fn node(&self, h: TraceHandle) -> TraceNode {
        self.nodes.borrow()[h.0 as usize].clone()
    }

    /// The number of nodes minted so far (for iteration without re-walking from the root).
    pub fn len(&self) -> usize {
        self.nodes.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn append(&self, parent: TraceHandle, node: TraceNode) -> TraceHandle {
        let mut nodes = self.nodes.borrow_mut();
        let h = TraceHandle(nodes.len() as u32);
        nodes.push(node);
        nodes[parent.0 as usize].children.push(h);
        h
    }

    fn append_root(&self, node: TraceNode) -> TraceHandle {
        let mut nodes = self.nodes.borrow_mut();
        let h = TraceHandle(nodes.len() as u32);
        nodes.push(node);
        drop(nodes);
        self.root.set(Some(h));
        h
    }

    /// The last child of `parent` — used by `TraceSink::synthesize_word`'s two-levels-deep reach.
    fn last_child(&self, parent: TraceHandle) -> TraceHandle {
        *self.nodes.borrow()[parent.0 as usize]
            .children
            .last()
            .expect(
                "SynthesizeWord requires the cursor's last child (LexicalLookup) to exist first",
            )
    }
}

impl TraceSink for TreeTraceSink {
    fn captures_failure_context(&self) -> bool {
        self.capture_failure_context
    }

    fn captures_details(&self) -> bool {
        self.capture_failure_context
    }

    fn end_unapply_template_with_slots(
        &self,
        parent: TraceHandle,
        template: TemplateId,
        output: &Word,
        unapplied: bool,
        slots: &[TemplateSlotOutcome],
    ) -> TraceHandle {
        assert!(
            self.captures_details(),
            "template slot capture was not enabled"
        );
        let event = self.end_unapply_template(parent, template, output, unapplied);
        self.nodes.borrow_mut()[event.0 as usize].template_slots = Some(slots.to_vec());
        event
    }

    fn end_apply_template_with_slots(
        &self,
        parent: TraceHandle,
        template: TemplateId,
        output: &Word,
        applied: bool,
        slots: &[TemplateSlotOutcome],
    ) -> TraceHandle {
        assert!(
            self.captures_details(),
            "template slot capture was not enabled"
        );
        let event = self.end_apply_template(parent, template, output, applied);
        self.nodes.borrow_mut()[event.0 as usize].template_slots = Some(slots.to_vec());
        event
    }

    fn failed_partial_parse(
        &self,
        parent: TraceHandle,
        word: &Word,
        cause: PartialParseCause,
    ) -> TraceHandle {
        assert!(
            self.captures_details(),
            "partial parse capture was not enabled"
        );
        let event = self.failed(parent, word, FailureReason::PartialParse);
        self.nodes.borrow_mut()[event.0 as usize].partial_parse_cause = Some(cause);
        event
    }

    fn set_failure_context(&self, event: TraceHandle, context: FailureContext) {
        assert!(
            self.capture_failure_context,
            "failure context capture was not enabled"
        );
        let mut nodes = self.nodes.borrow_mut();
        let node = &mut nodes[event.0 as usize];
        assert!(
            node.failure_reason.is_some(),
            "failure context requires a failed event"
        );
        node.failure_context = Some(context);
    }
    fn complete_lexical_lookup(&self, event: TraceHandle, result: LookupResult) {
        assert!(
            self.capture_failure_context,
            "lexical lookup result capture was not enabled"
        );
        let mut nodes = self.nodes.borrow_mut();
        let node = &mut nodes[event.0 as usize];
        assert_eq!(node.type_, TraceType::LexicalLookup);
        assert!(
            node.lookup_result.is_none(),
            "lexical lookup completion already recorded"
        );
        node.lookup_result = Some(result);
    }
    #[inline(always)]
    fn is_tracing(&self) -> bool {
        true
    }

    fn analyze_word(&self, input: &Word) -> TraceHandle {
        let mut n = TraceNode::new(TraceType::WordAnalysis, TraceSource::Language);
        n.input = Some(input.clone());
        self.append_root(n)
    }

    fn generate_words(&self) -> TraceHandle {
        self.append_root(TraceNode::new(
            TraceType::GenerateWords,
            TraceSource::Language,
        ))
    }

    fn begin_unapply_stratum(
        &self,
        parent: TraceHandle,
        stratum: StratumId,
        input: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::StratumAnalysisInput,
            TraceSource::Stratum(stratum),
        );
        n.input = Some(input.clone());
        self.append(parent, n)
    }
    fn end_unapply_stratum(
        &self,
        parent: TraceHandle,
        stratum: StratumId,
        output: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::StratumAnalysisOutput,
            TraceSource::Stratum(stratum),
        );
        n.output = Some(output.clone());
        self.append(parent, n)
    }
    fn begin_apply_stratum(
        &self,
        parent: TraceHandle,
        stratum: StratumId,
        input: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::StratumSynthesisInput,
            TraceSource::Stratum(stratum),
        );
        n.input = Some(input.clone());
        self.append(parent, n)
    }
    fn end_apply_stratum(
        &self,
        parent: TraceHandle,
        stratum: StratumId,
        output: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::StratumSynthesisOutput,
            TraceSource::Stratum(stratum),
        );
        n.output = Some(output.clone());
        self.append(parent, n)
    }

    fn begin_unapply_template(
        &self,
        parent: TraceHandle,
        template: TemplateId,
        input: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::TemplateAnalysisInput,
            TraceSource::Template(template),
        );
        n.input = Some(input.clone());
        self.append(parent, n)
    }
    fn end_unapply_template(
        &self,
        parent: TraceHandle,
        template: TemplateId,
        output: &Word,
        unapplied: bool,
    ) -> TraceHandle {
        // `TraceManager.cs:61-66` sets `Output = unapplied ? output : null`, touching no `FailureReason` field.
        let mut n = TraceNode::new(
            TraceType::TemplateAnalysisOutput,
            TraceSource::Template(template),
        );
        if unapplied {
            n.output = Some(output.clone());
        }
        self.append(parent, n)
    }
    fn begin_apply_template(
        &self,
        parent: TraceHandle,
        template: TemplateId,
        input: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::TemplateSynthesisInput,
            TraceSource::Template(template),
        );
        n.input = Some(input.clone());
        self.append(parent, n)
    }
    fn end_apply_template(
        &self,
        parent: TraceHandle,
        template: TemplateId,
        output: &Word,
        applied: bool,
    ) -> TraceHandle {
        // Same fix as `end_unapply_template` above -- `TraceManager.cs:211-216`.
        let mut n = TraceNode::new(
            TraceType::TemplateSynthesisOutput,
            TraceSource::Template(template),
        );
        if applied {
            n.output = Some(output.clone());
        }
        self.append(parent, n)
    }

    fn non_final_template_applied_last(
        &self,
        parent: TraceHandle,
        stratum: StratumId,
        output: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::StratumSynthesisOutput,
            TraceSource::Stratum(stratum),
        );
        n.output = Some(output.clone());
        n.failure_reason = Some(FailureReason::PartialParse);
        if self.captures_details() {
            n.partial_parse_cause = Some(PartialParseCause::NonFinalTemplateAppliedLast);
        }
        self.append(parent, n)
    }
    fn applicable_templates_not_applied(
        &self,
        parent: TraceHandle,
        stratum: StratumId,
        input: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::StratumSynthesisOutput,
            TraceSource::Stratum(stratum),
        );
        n.input = Some(input.clone());
        n.failure_reason = Some(FailureReason::PartialParse);
        if self.captures_details() {
            n.partial_parse_cause = Some(PartialParseCause::ApplicableTemplatesNotApplied);
        }
        self.append(parent, n)
    }

    fn phonological_rule_unapplied(
        &self,
        parent: TraceHandle,
        rule: PRuleId,
        subrule: i32,
        output: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::PhonologicalRuleAnalysis,
            TraceSource::PhonRule(rule),
        );
        n.subrule_index = Some(subrule);
        n.output = Some(output.clone());
        self.append(parent, n)
    }
    fn phonological_rule_not_unapplied(
        &self,
        parent: TraceHandle,
        rule: PRuleId,
        subrule: i32,
        input: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::PhonologicalRuleAnalysis,
            TraceSource::PhonRule(rule),
        );
        n.subrule_index = Some(subrule);
        n.input = Some(input.clone());
        self.append(parent, n)
    }
    fn phonological_rule_applied(
        &self,
        parent: TraceHandle,
        rule: PRuleId,
        subrule: i32,
        input: &Word,
        output: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::PhonologicalRuleSynthesis,
            TraceSource::PhonRule(rule),
        );
        n.subrule_index = Some(subrule);
        n.input = Some(input.clone());
        n.output = Some(output.clone());
        self.append(parent, n)
    }
    fn phonological_rule_not_applied(
        &self,
        parent: TraceHandle,
        rule: PRuleId,
        subrule: i32,
        input: &Word,
        reason: FailureReason,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::PhonologicalRuleSynthesis,
            TraceSource::PhonRule(rule),
        );
        n.subrule_index = Some(subrule);
        n.input = Some(input.clone());
        n.failure_reason = Some(reason);
        self.append(parent, n)
    }

    fn morphological_rule_unapplied(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        subrule: i32,
        output: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::MorphologicalRuleAnalysis,
            TraceSource::MorphRule(rule),
        );
        n.subrule_index = Some(subrule);
        n.output = Some(output.clone());
        self.append(parent, n)
    }
    fn morphological_rule_not_unapplied(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        subrule: i32,
        input: &Word,
        reason: FailureReason,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::MorphologicalRuleAnalysis,
            TraceSource::MorphRule(rule),
        );
        n.subrule_index = Some(subrule);
        n.input = Some(input.clone());
        n.failure_reason = Some(reason);
        self.append(parent, n)
    }
    fn morphological_rule_applied(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        subrule: i32,
        output: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::MorphologicalRuleSynthesis,
            TraceSource::MorphRule(rule),
        );
        n.subrule_index = Some(subrule);
        n.output = Some(output.clone());
        self.append(parent, n)
    }
    fn morphological_rule_not_applied(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        subrule: i32,
        input: &Word,
        reason: FailureReason,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::MorphologicalRuleSynthesis,
            TraceSource::MorphRule(rule),
        );
        n.subrule_index = Some(subrule);
        n.input = Some(input.clone());
        n.failure_reason = Some(reason);
        self.append(parent, n)
    }

    fn compounding_rule_not_unapplied(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        subrule: i32,
        input: &Word,
        reason: FailureReason,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::CompoundingRuleAnalysis,
            TraceSource::MorphRule(rule),
        );
        n.input = Some(input.clone());
        n.subrule_index = (subrule >= 0).then_some(subrule);
        n.failure_reason = Some(reason);
        self.append(parent, n)
    }
    fn compounding_rule_unapplied(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        subrule: i32,
        output: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::CompoundingRuleAnalysis,
            TraceSource::MorphRule(rule),
        );
        n.output = Some(output.clone());
        n.subrule_index = (subrule >= 0).then_some(subrule);
        self.append(parent, n)
    }
    fn compounding_rule_not_applied(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        input: &Word,
        reason: FailureReason,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::CompoundingRuleSynthesis,
            TraceSource::MorphRule(rule),
        );
        n.input = Some(input.clone());
        n.failure_reason = Some(reason);
        self.append(parent, n)
    }

    fn lexical_lookup(&self, parent: TraceHandle, stratum: StratumId, input: &Word) -> TraceHandle {
        let mut n = TraceNode::new(TraceType::LexicalLookup, TraceSource::Stratum(stratum));
        n.input = Some(input.clone());
        self.append(parent, n)
    }

    fn blocked(
        &self,
        parent: TraceHandle,
        rule: MRuleId,
        output: &Word,
        by_entry: LexEntryId,
    ) -> TraceHandle {
        let mut n = TraceNode::new(TraceType::Blocked, TraceSource::MorphRule(rule));
        n.output = Some(output.clone());
        n.blocked_by_entry = Some(by_entry);
        self.append(parent, n)
    }

    fn successful(&self, parent: TraceHandle, word: &Word) -> TraceHandle {
        let mut n = TraceNode::new(TraceType::Successful, TraceSource::None);
        n.output = Some(word.clone());
        self.append(parent, n)
    }
    fn failed(&self, parent: TraceHandle, word: &Word, reason: FailureReason) -> TraceHandle {
        let mut n = TraceNode::new(TraceType::Failed, TraceSource::None);
        n.output = Some(word.clone());
        n.failure_reason = Some(reason);
        self.append(parent, n)
    }

    fn synthesize_word(&self, parent: TraceHandle, input: &Word) -> TraceHandle {
        let target = self.last_child(parent);
        let mut n = TraceNode::new(TraceType::WordSynthesis, TraceSource::None);
        n.input = Some(input.clone());
        self.append(target, n)
    }
}

#[cfg(test)]
mod tests;
