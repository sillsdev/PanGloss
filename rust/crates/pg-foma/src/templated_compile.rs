//! Shared compiler for the exact P6 templated-morphotactics prototype pipeline.

use std::fmt;
use std::time::{Duration, Instant};

use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;
use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::{Grammar, PhonRuleDef};

use crate::analyzer::FomaProposer;
use crate::emit::emit_underlying_templated;
use crate::replace::{compile_and_compose_rules, SegAlphabet, TupleReport};

/// Timings and sizes from each exact stage of the P6 pipeline.
pub struct TemplatedCompileProfile {
    pub templated_emit_elapsed: Duration,
    pub lexc_compile_elapsed: Duration,
    pub rule_compile_compose_elapsed: Duration,
    pub cleanup_compile_elapsed: Duration,
    pub final_compose_minimize_elapsed: Duration,
    pub lexc_state_count: i32,
    pub lexc_arc_count: i32,
    pub phonological_rule_count: usize,
    pub final_state_count: i32,
    pub final_arc_count: i32,
    pub skipped_rules: Vec<String>,
    pub tuple_reports: Vec<(String, Vec<TupleReport>)>,
}

/// The exact composed network and an owned proposer initialized from that network.
pub struct TemplatedCompileOutput {
    pub network: Fsm,
    pub proposer: FomaProposer,
    pub profile: TemplatedCompileProfile,
}

#[derive(Debug)]
pub enum TemplatedCompileError {
    MissingCharacterTable,
    LexcCompileFailed,
    RuleCompileFailed(String),
    NoCompiledRules,
    CleanupCompileFailed(String),
}

impl fmt::Display for TemplatedCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCharacterTable => write!(f, "grammar has no character table"),
            Self::LexcCompileFailed => write!(f, "templated lexc failed to compile"),
            Self::RuleCompileFailed(error) => write!(f, "rule compile/compose failed: {error}"),
            Self::NoCompiledRules => write!(f, "no phonological rule compiled"),
            Self::CleanupCompileFailed(regex) => {
                write!(f, "boundary cleanup regex failed to compile: {regex:?}")
            }
        }
    }
}

impl std::error::Error for TemplatedCompileError {}

/// Compile `g` through the exact manual pipeline formerly repeated by the P6 gate:
/// templated underlying lexc, stratum-ordered rules, boundary cleanup, compose, minimize.
pub fn compile_templated_morphotactics(
    g: &Grammar,
) -> Result<TemplatedCompileOutput, TemplatedCompileError> {
    let table = g
        .char_tables
        .first()
        .ok_or(TemplatedCompileError::MissingCharacterTable)?;
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();

    let started = Instant::now();
    let emitted = emit_underlying_templated(g, &alphabet, None);
    let templated_emit_elapsed = started.elapsed();

    let started = Instant::now();
    let lexc_net = fsm_lexc_parse_string(&opts, None, &emitted.lexc_source)
        .ok_or(TemplatedCompileError::LexcCompileFailed)?;
    let lexc_compile_elapsed = started.elapsed();
    let lexc_state_count = lexc_net.statecount;
    let lexc_arc_count = lexc_net.arccount;

    let rules_in_order: Vec<&PhonRuleDef> = g
        .strata
        .iter()
        .flat_map(|stratum| stratum.prules.iter().map(|id| &g.prules[id.0 as usize]))
        .collect();
    let phonological_rule_count = rules_in_order.len();
    let mut skipped_rules = Vec::new();
    let mut tuple_reports = Vec::new();
    let started = Instant::now();
    let rule_net = compile_and_compose_rules(
        &opts,
        g,
        &alphabet,
        &rules_in_order,
        &mut skipped_rules,
        &mut tuple_reports,
    )
    .map_err(|error| TemplatedCompileError::RuleCompileFailed(error.to_string()))?
    .ok_or(TemplatedCompileError::NoCompiledRules)?;
    let rule_compile_compose_elapsed = started.elapsed();

    let boundary_tokens: Vec<char> = table
        .iter()
        .filter(|(_, definition)| definition.kind() == CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect();
    let cleanup_regex = boundary_tokens
        .iter()
        .map(|token| format!("{token} -> 0"))
        .collect::<Vec<_>>()
        .join(", ");
    let started = Instant::now();
    let cleanup_net = fsm_parse_regex(&opts, &cleanup_regex, None, None)
        .ok_or_else(|| TemplatedCompileError::CleanupCompileFailed(cleanup_regex.clone()))?;
    let cleanup_compile_elapsed = started.elapsed();

    let started = Instant::now();
    let network = fsm_compose(&opts, lexc_net, rule_net);
    let network = fsm_compose(&opts, network, cleanup_net);
    let network = fsm_minimize(&opts, network);
    let final_compose_minimize_elapsed = started.elapsed();
    let final_state_count = network.statecount;
    let final_arc_count = network.arccount;

    // apply_init clones `network`; the returned proposer is wholly owned before this function
    // returns and carries the token encoder needed for raw orthographic queries.
    let proposer = FomaProposer::from_precompiled_network(&network, emitted.report)
        .with_segment_query_encoder(table);

    Ok(TemplatedCompileOutput {
        network,
        proposer,
        profile: TemplatedCompileProfile {
            templated_emit_elapsed,
            lexc_compile_elapsed,
            rule_compile_compose_elapsed,
            cleanup_compile_elapsed,
            final_compose_minimize_elapsed,
            lexc_state_count,
            lexc_arc_count,
            phonological_rule_count,
            final_state_count,
            final_arc_count,
            skipped_rules,
            tuple_reports,
        },
    })
}
