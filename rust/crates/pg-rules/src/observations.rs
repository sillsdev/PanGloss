//! Diagnostic observations at decisions in the ordinary HC evaluator.

use pg_grammar_model::model::{
    AllomorphId, LexEntryId, MRuleId, MprSet, PRuleId, StemNameId, StratumId, TableId, TemplateId,
};

use crate::trace::{EnvironmentResult, TraceHandle, TraceMode, TraceSink};
use crate::word::Word;

pub trait Recorder {
    fn record(&self, decision: Decision<'_>);

    fn advance(&self, parent: Option<TraceHandle>) -> Option<TraceHandle> {
        parent
    }

    fn record_at(&self, _word: &Word, decision: Decision<'_>) {
        self.record(decision);
    }

    fn record_terminal(&self, word: &Word, decision: Decision<'_>) {
        self.record_at(word, decision);
    }

    fn merged(&self, _discarded: &Word, _retained: Option<TraceHandle>, _analysis: bool) {}

    fn accepted(&self, _word: &Word, _output: usize, _surface: &str) {}

    fn unobserved_terminal(&self, _word: &Word) {}
}

pub struct ObservationTrace<'a>(pub &'a dyn Recorder);

impl TraceSink for ObservationTrace<'_> {
    fn is_tracing(&self) -> bool {
        false
    }

    fn mode(&self) -> TraceMode<'_> {
        TraceMode::Observe(self.0)
    }
}

#[derive(Clone, Copy)]
pub enum MprGateOwner {
    Allomorph(AllomorphId),
    CompoundSubrule { rule: MRuleId, subrule: usize },
    RewriteSubrule { rule: PRuleId, subrule: usize },
}

/// Owners pass established facts. Surface rendering is deferred until capture can use it.
#[derive(Clone, Copy)]
pub enum Decision<'a> {
    InvalidShape {
        table: TableId,
        spelling: &'a str,
    },
    LookupCompleted {
        stratum: StratumId,
        materialized: usize,
        root: Option<AllomorphId>,
        no_matches: bool,
    },
    LookupFeatures {
        entry: LexEntryId,
        candidate: AllomorphId,
        required: &'a pg_featstruct::FeatureStruct,
        actual: &'a pg_featstruct::FeatureStruct,
    },
    LookupMpr {
        candidate: AllomorphId,
        allomorph: AllomorphId,
        tested: &'a MprSet,
        actual: &'a MprSet,
        excluded: bool,
        group: Option<usize>,
    },
    LookupStemName {
        candidate: AllomorphId,
        rule: MRuleId,
        required: StemNameId,
        actual: Option<StemNameId>,
    },
    RootNotViable {
        stratum: StratumId,
    },
    RequiredSlotUnfilled {
        template: TemplateId,
        slot: usize,
        analysis: bool,
    },
    ApplicableTemplatesNotApplied {
        stratum: StratumId,
        template: Option<TemplateId>,
    },
    NonfinalTemplateLast {
        stratum: StratumId,
    },
    RemainingRulesInStratum {
        stratum: StratumId,
    },
    RemainingAnalyzedRules {
        stratum: StratumId,
        remaining: usize,
    },
    TemplateIneligible {
        template: TemplateId,
        partial_root: bool,
    },
    OptionalSlotSkipped {
        template: TemplateId,
        slot: usize,
        analysis: bool,
    },
    FinalTemplatePolicy {
        rule: MRuleId,
        after_final: bool,
    },
    Environment {
        constraint_owner: AllomorphId,
        allomorph: AllomorphId,
        span: (u32, u32),
        alternatives: &'a [EnvironmentResult],
    },
    RewriteNoOutput {
        rule: PRuleId,
        analysis: bool,
    },
    MorphPatternNoMatch {
        allomorph: AllomorphId,
        table: TableId,
        analysis: bool,
    },
    MprGate {
        owner: MprGateOwner,
        tested: &'a MprSet,
        actual: &'a MprSet,
        excluded: bool,
        group: Option<usize>,
    },
    BoundRoot {
        allomorph: AllomorphId,
    },
    /// An earlier-indexed allomorph of the same morpheme was eligible at the used allomorph's span, so the used one may not stand.
    AllomorphBlocked {
        blocked: AllomorphId,
        blocker: AllomorphId,
    },
    SurfaceMismatch {
        table: TableId,
        requested: &'a str,
        morphs: &'a [crate::word::MorphRecord],
        built: &'a dyn Fn(usize) -> Option<String>,
    },
}
