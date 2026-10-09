use serde::Serialize;

pub const APPLICATION_ID: i64 = 1_346_848_321;
pub const FACT_SCHEMA_VERSION: u32 = 8;
pub const FACT_FORMAT: &str = "pangloss-grammar-facts";
pub const FACTS_CONTEXT_FORMAT: &str = "pangloss-facts-context";
pub const FACTS_CONTEXT_VERSION: u32 = 1;
pub const STATS_MANIFEST_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionStatus {
    pub section: String,
    pub status: String,
    pub source_scope: String,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProducerIdentity<'a> {
    pub compiler_version: &'a str,
    pub source_revision: &'a str,
    pub build_identity: &'a str,
}

pub(crate) fn fixed_sections(
    compile_completed: bool,
    source_inventory_complete: bool,
    adhoc_groups_available: bool,
    load_accounting_complete: bool,
    stats_requested: bool,
) -> Vec<SectionStatus> {
    let unavailable = |section: &str| SectionStatus {
        section: section.into(),
        status: "unavailable".into(),
        source_scope: "not emitted by facts schema v8".into(),
        reason_code: Some("not_in_facts_v8".into()),
    };
    vec![
        SectionStatus {
            section: "project".into(),
            status: "complete".into(),
            source_scope: "project metadata in the supplied Snapshot".into(),
            reason_code: None,
        },
        SectionStatus {
            section: "source_census".into(),
            status: if source_inventory_complete { "complete" } else { "unavailable" }.into(),
            source_scope: if source_inventory_complete {
                "grammar source objects with their parse retention state, plus class counts for every source header occurrence, from current import provenance".into()
            } else {
                "the supplied Snapshot provenance cannot establish every source header occurrence".into()
            },
            reason_code: if source_inventory_complete {
                None
            } else {
                Some("source_inventory_unknown".into())
            },
        },
        SectionStatus {
            section: "conversion_inventory".into(),
            status: "complete".into(),
            source_scope: "Snapshot importer inventory and compiler final inventory".into(),
            reason_code: None,
        },
        SectionStatus {
            section: "categories".into(),
            status: "complete".into(),
            source_scope: "part-of-speech hierarchy in the supplied Snapshot".into(),
            reason_code: None,
        },
        SectionStatus {
            section: "entries".into(),
            status: "complete".into(),
            source_scope: "entry identities and senses in the supplied Snapshot".into(),
            reason_code: None,
        },
        SectionStatus {
            section: "msas".into(),
            status: "complete".into(),
            source_scope: "all MSA identities and category, slot, class, and exception references in the supplied Snapshot".into(),
            reason_code: None,
        },
        SectionStatus {
            section: "adhoc_prohibitions".into(),
            status: "complete".into(),
            source_scope: "flat allomorph and morpheme prohibitions in the supplied Snapshot".into(),
            reason_code: None,
        },
        SectionStatus {
            section: "adhoc_groups".into(),
            status: if adhoc_groups_available { "complete" } else { "unavailable" }.into(),
            source_scope: if adhoc_groups_available {
                "group identity, multilingual rationale, and unordered member references in the supplied Snapshot".into()
            } else {
                "the supplied Snapshot does not declare ad hoc group facts".into()
            },
            reason_code: if adhoc_groups_available {
                None
            } else {
                Some("not_in_snapshot".into())
            },
        },
        SectionStatus {
            section: "load_accounting".into(),
            status: if compile_completed && source_inventory_complete && load_accounting_complete {
                "complete"
            } else {
                "partial"
            }
            .into(),
            source_scope: if compile_completed && source_inventory_complete && load_accounting_complete {
                "current source census, importer decisions, compiler decisions, and compacted outcomes have no unknown load facts".into()
            } else {
                "typed importer and compiler decisions; unknown or unavailable subjects remain explicit".into()
            },
            reason_code: if compile_completed && source_inventory_complete && load_accounting_complete {
                None
            } else {
                Some("reason_coverage_partial".into())
            },
        },
        SectionStatus {
            section: "effective_grammar".into(),
            status: if compile_completed { "partial" } else { "unavailable" }.into(),
            source_scope: "final source mappings, allomorph order, environment outcomes, class extensions, and strata from production default compiler output".into(),
            reason_code: Some(if compile_completed {
                "compiled_grammar_partially_exported".into()
            } else {
                "compile_refused".into()
            }),
        },
        SectionStatus {
            section: "templates".into(),
            status: if compile_completed { "complete" } else { "partial" }.into(),
            source_scope: "authored affix slots and templates with final compiled slot order from compiler decisions".into(),
            reason_code: if compile_completed {
                None
            } else {
                Some("compile_refused".into())
            },
        },
        SectionStatus {
            section: "allomorphs".into(),
            status: "complete".into(),
            source_scope: "allomorph identity, entry order, source class, morph type, abstract state, every supplied form, and each authored morphological gate with the compiler's effect on it".into(),
            reason_code: None,
        },
        SectionStatus {
            section: "environments".into(),
            status: "complete".into(),
            source_scope: "all authored environment definitions and all allomorph phone/position edges, with compiler resolution status for attempted owners".into(),
            reason_code: None,
        },
        SectionStatus {
            section: "features".into(),
            status: "complete".into(),
            source_scope: "phonological and morphosyntactic feature definitions and phoneme/natural-class feature structures in the supplied Snapshot".into(),
            reason_code: None,
        },
        SectionStatus {
            section: "phonology".into(),
            status: if compile_completed { "complete" } else { "partial" }.into(),
            source_scope: "authored phoneme, boundary, natural-class and feature-constraint facts; effective class extensions and strata when compilation completes".into(),
            reason_code: if compile_completed { None } else { Some("compile_refused".into()) },
        },
        SectionStatus {
            section: "patterns".into(),
            status: if compile_completed { "complete" } else { "partial" }.into(),
            source_scope: "authored rewrite and metathesis patterns plus compiler-resolved patterns for attempted valid environments".into(),
            reason_code: if compile_completed { None } else { Some("compile_refused".into()) },
        },
        unavailable("compound_rules"),
        unavailable("affix_processes"),
        SectionStatus {
            section: "compiled_mappings".into(),
            status: if compile_completed { "complete" } else { "unavailable" }.into(),
            source_scope: if compile_completed {
                "compiler-published final source-to-output mappings and contextual allomorph order after compaction".into()
            } else {
                "the compiler refused before publishing a final grammar".into()
            },
            reason_code: if compile_completed { None } else { Some("compile_refused".into()) },
        },
        SectionStatus {
            section: "parser_config".into(),
            status: "partial".into(),
            source_scope: "normalized Snapshot parser parameters and final compiled strata; original presence for defaulted scalar fields is not retained by Snapshot".into(),
            reason_code: Some("snapshot_raw_presence_not_retained".into()),
        },
        SectionStatus {
            section: "stats".into(),
            status: if stats_requested { "complete" } else { "not_requested" }.into(),
            source_scope: if stats_requested {
                "one manifest-bound HermitCrab batch run and its exact input completion census".into()
            } else {
                "no frozen stats cache and manifest supplied".into()
            },
            reason_code: None,
        },
    ]
}
