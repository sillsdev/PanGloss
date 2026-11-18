//! Local observations at decisions in the ordinary HC evaluator.

use pg_grammar_model::model::{
    AllomorphId, LexEntryId, MRuleId, MprSet, StemNameId, StratumId, TableId, TemplateId,
};

use crate::trace::{TraceMode, TraceSink};

pub trait Recorder {
    fn record(&self, decision: Decision<'_>);
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

/// Owners pass established facts. Surface rendering is deferred until capture can use it.
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
    },
    LookupMpr {
        candidate: AllomorphId,
        allomorph: AllomorphId,
        tested: &'a MprSet,
        actual: &'a MprSet,
        excluded: bool,
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
    BoundRoot {
        allomorph: AllomorphId,
    },
    SurfaceMismatch {
        table: TableId,
        requested: &'a str,
        morphs: &'a [crate::word::MorphRecord],
        built: &'a dyn Fn(usize) -> Option<String>,
    },
}
