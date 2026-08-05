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
//! `NoopSink`'s, and call sites must check `is_tracing()` before calling either). The sketch's
//! `FailureObj` (C#'s `object failureObj` parameter — a free-form extra failure detail, e.g. which
//! specific co-occurrence rule rejected) is dropped entirely: no call site in this landing needs it
//! to answer "why", and adding a type for it now would be speculative. Both simplifications are
//! flagged here rather than silently decided.

use std::cell::{Cell, RefCell};

use pg_grammar::model::{MRuleId, PRuleId, StratumId, TemplateId};

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
/// to the grammar model, per §4.3). `None` for the leaf-most `Successful`/`Failed`/`Blocked` nodes,
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
        input: &Word,
    ) -> TraceHandle;
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

    /// A rule that would have applied again but was blocked by the "don't re-apply to your own
    /// output" guard.
    fn blocked(&self, parent: TraceHandle, rule: MRuleId, output: &Word) -> TraceHandle;

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
        _i: &Word,
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
    fn blocked(&self, _p: TraceHandle, _r: MRuleId, _o: &Word) -> TraceHandle {
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
}

impl Default for TreeTraceSink {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeTraceSink {
    pub fn new() -> Self {
        TreeTraceSink {
            nodes: RefCell::new(Vec::new()),
            root: Cell::new(None),
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
        // P12 chunk 5 correction: `TraceManager.cs:61-66` sets `Output = unapplied ? output : null`
        // and touches NO `FailureReason` field at all -- chunk 0's original body fabricated a
        // `FailureReason::PartialParse` here that C# never records (that reason is real only for
        // `NonFinalTemplateAppliedLast`/`ApplicableTemplatesNotApplied`, a DIFFERENT pair of call
        // sites entirely -- see those methods' docs). Fixed here, at this method's first real call
        // site, before any caller could come to depend on the fabricated reason.
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
        // P12 chunk 5 correction: same fix as `end_unapply_template` above -- `TraceManager.cs:
        // 211-216` sets `Output = applied ? output : null`, no `FailureReason` touched.
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
        output: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::PhonologicalRuleSynthesis,
            TraceSource::PhonRule(rule),
        );
        n.subrule_index = Some(subrule);
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
        input: &Word,
    ) -> TraceHandle {
        let mut n = TraceNode::new(
            TraceType::CompoundingRuleAnalysis,
            TraceSource::MorphRule(rule),
        );
        n.input = Some(input.clone());
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

    fn blocked(&self, parent: TraceHandle, rule: MRuleId, output: &Word) -> TraceHandle {
        let mut n = TraceNode::new(TraceType::Blocked, TraceSource::MorphRule(rule));
        n.output = Some(output.clone());
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
mod tests {
    use super::*;
    use pg_shape::ShapeBuilder;

    fn w() -> Word {
        Word::new(ShapeBuilder::new().finish(), StratumId(0))
    }

    #[test]
    fn noop_sink_is_not_tracing() {
        let sink = NoopSink;
        assert!(!sink.is_tracing());
    }

    #[test]
    fn tree_sink_is_tracing() {
        let sink = TreeTraceSink::new();
        assert!(sink.is_tracing());
    }

    /// The first cursor subtlety (§1.2): applying a rule reassigns the cursor so a LATER event nests
    /// UNDER the rule's own node, rather than as a sibling of it. The call site is responsible for
    /// passing the handle returned by the previous call as the next call's `parent` — this test
    /// exercises exactly that discipline and asserts the resulting tree shape.
    #[test]
    fn rule_applied_return_nests_next_event_under_it() {
        let sink = TreeTraceSink::new();
        let word = w();
        let root = sink.analyze_word(&word);

        // First rule application: appended as root's child.
        let h1 = sink.morphological_rule_applied(root, MRuleId(0), 0, &word);
        assert_eq!(sink.node(root).children, vec![h1]);

        // Caller passes h1 (not root) as parent for the next event -- it nests UNDER the rule.
        let h2 = sink.morphological_rule_applied(h1, MRuleId(1), 0, &word);
        assert_eq!(sink.node(h1).children, vec![h2]);
        assert!(
            sink.node(root).children == vec![h1],
            "h2 must not become a sibling of h1 under root"
        );
    }

    /// The second, trickier cursor subtlety (§1.2): `SynthesizeWord` reaches TWO levels deep --
    /// `parent`'s LAST child's children -- because by the time it fires the cursor logically needs
    /// to descend into the just-appended `LexicalLookup` node's children, even though `LexicalLookup`
    /// itself never reassigned the cursor (the call site still passes the unchanged `root` as
    /// `parent`).
    #[test]
    fn synthesize_word_nests_two_levels_deep_under_lexical_lookup() {
        let sink = TreeTraceSink::new();
        let word = w();
        let root = sink.analyze_word(&word);

        let lex = sink.lexical_lookup(root, StratumId(0), &word);
        assert_eq!(
            sink.node(root).children,
            vec![lex],
            "LexicalLookup is root's child, cursor unchanged"
        );

        // Call site passes `root` (the cursor, unchanged by LexicalLookup) -- not `lex`.
        let syn = sink.synthesize_word(root, &word);

        assert_eq!(
            sink.node(root).children,
            vec![lex],
            "no new direct child of root"
        );
        assert_eq!(
            sink.node(lex).children,
            vec![syn],
            "SynthesizeWord landed under lex's children, not root's"
        );
    }

    #[test]
    fn failed_and_successful_carry_reason_and_word() {
        let sink = TreeTraceSink::new();
        let word = w();
        let root = sink.analyze_word(&word);

        let f = sink.failed(root, &word, FailureReason::PartialParse);
        assert_eq!(sink.node(f).type_, TraceType::Failed);
        assert_eq!(
            sink.node(f).failure_reason,
            Some(FailureReason::PartialParse)
        );
        assert!(sink.node(f).output.is_some());

        let s = sink.successful(root, &word);
        assert_eq!(sink.node(s).type_, TraceType::Successful);
        assert_eq!(sink.node(s).failure_reason, None);
    }

    #[test]
    fn stratum_bookends_carry_stratum_source() {
        let sink = TreeTraceSink::new();
        let word = w();
        let root = sink.analyze_word(&word);
        let begin = sink.begin_unapply_stratum(root, StratumId(2), &word);
        assert_eq!(sink.node(begin).source, TraceSource::Stratum(StratumId(2)));
        assert_eq!(sink.node(begin).type_, TraceType::StratumAnalysisInput);
        let end = sink.end_unapply_stratum(begin, StratumId(2), &word);
        assert_eq!(sink.node(end).type_, TraceType::StratumAnalysisOutput);
        assert_eq!(sink.node(begin).children, vec![end]);
    }
}
