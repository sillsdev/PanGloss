//! Shared compiler for the templated-morphotactics pipeline.

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

use crate::analyzer::{prepare_network_for_apply, FomaProposer};
use crate::emit::{emit_underlying_templated, surface_table, EmitReport};
use crate::replace::{compile_and_compose_rules_recall_safe, SegAlphabet, TupleReport};

/// Timings and sizes from each exact stage of the pipeline.
pub struct TemplatedCompileProfile {
    pub templated_emit_elapsed: Duration,
    pub lexc_compile_elapsed: Duration,
    pub rule_compile_compose_elapsed: Duration,
    pub cleanup_compile_elapsed: Duration,
    pub final_compose_minimize_elapsed: Duration,
    pub apply_prepare_elapsed: Duration,
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
    Unsupported(EmitReport),
    LexcCompileFailed,
    RuleCompileFailed(String),
    NoCompiledRules,
    CleanupCompileFailed(String),
}

impl fmt::Display for TemplatedCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCharacterTable => write!(f, "grammar has no character table"),
            Self::Unsupported(report) => write!(
                f,
                "templated emission unsupported: {:?} ({} uncovered)",
                report.tier,
                report.uncovered.len()
            ),
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

/// Compiles `g` through the manual pipeline: templated underlying lexc, stratum-ordered rules, boundary cleanup, compose, minimize.
pub fn compile_templated_morphotactics(
    g: &Grammar,
) -> Result<TemplatedCompileOutput, TemplatedCompileError> {
    // The LAST stratum's table, never `g.char_tables[0]` -- same convention every other caller of `surface_table` already follows.
    let table = surface_table(g);
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();

    let started = Instant::now();
    let emitted = emit_underlying_templated(g, &alphabet, None);
    let templated_emit_elapsed = started.elapsed();

    match &emitted.report.tier {
        crate::emit::FomaTier::Full => {}
        crate::emit::FomaTier::Unsupported { .. }
        | crate::emit::FomaTier::Partial { .. } => {
            return Err(TemplatedCompileError::Unsupported(emitted.report));
        }
    }

    let started = Instant::now();
    let lexc_net = fsm_lexc_parse_string(&opts, None, &emitted.lexc_source)
        .ok_or(TemplatedCompileError::LexcCompileFailed)?;
    let lexc_compile_elapsed = started.elapsed();
    let lexc_state_count = lexc_net.statecount;
    let lexc_arc_count = lexc_net.arccount;
    let lexc_net = match crate::structural_allomorph::compile_layer(&opts, g, &alphabet) {
        Some(structural) => fsm_compose(&opts, lexc_net, structural),
        None => lexc_net,
    };

    let rules_in_order: Vec<&PhonRuleDef> = g
        .strata
        .iter()
        .flat_map(|stratum| stratum.prules.iter().map(|id| &g.prules[id.0 as usize]))
        .collect();
    let phonological_rule_count = rules_in_order.len();
    let mut skipped_rules = Vec::new();
    let mut tuple_reports = Vec::new();
    let started = Instant::now();
    // A grammar declaring no phonological rules composes nothing here (the identity), keeping NoCompiledRules for its real failure mode: rules declared but every one skipped or failed to compile.
    let rule_net = if rules_in_order.is_empty() {
        None
    } else {
        Some(
            compile_and_compose_rules_recall_safe(
                &opts,
                g,
                &alphabet,
                &rules_in_order,
                &mut skipped_rules,
                &mut tuple_reports,
            )
            .map_err(|error| TemplatedCompileError::RuleCompileFailed(error.to_string()))?
            .ok_or(TemplatedCompileError::NoCompiledRules)?,
        )
    };
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
    // An empty `cleanup_regex` is not accepted by `fsm_parse_regex`, so skip the pass: deleting nothing from a tape with no boundary tokens is the identity anyway.
    let cleanup_net = if boundary_tokens.is_empty() {
        None
    } else {
        Some(
            fsm_parse_regex(&opts, &cleanup_regex, None, None).ok_or_else(|| {
                TemplatedCompileError::CleanupCompileFailed(cleanup_regex.clone())
            })?,
        )
    };
    let cleanup_compile_elapsed = started.elapsed();

    let started = Instant::now();
    let network = match rule_net {
        Some(rule_net) => fsm_compose(&opts, lexc_net, rule_net),
        None => lexc_net,
    };
    let network = match crate::structural_allomorph::compile_authored_deletion_fallback(
        &opts, g, &alphabet,
    ) {
        Some(fallback) => fsm_compose(&opts, network, fallback),
        None => network,
    };
    let network = match cleanup_net {
        Some(cleanup) => fsm_compose(&opts, network, cleanup),
        None => network,
    };
    let mut network = fsm_minimize(&opts, network);
    let final_compose_minimize_elapsed = started.elapsed();
    let final_state_count = network.statecount;
    let final_arc_count = network.arccount;

    let started = Instant::now();
    prepare_network_for_apply(&mut network);
    let apply_prepare_elapsed = started.elapsed();

    // apply_init clones `network`, so the returned proposer is wholly owned here.
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
            apply_prepare_elapsed,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const STACK_BYTES: usize = 512 * 1024 * 1024;

    fn load_aweti() -> Option<Grammar> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../samples/data/aweti.json");
        let json = std::fs::read_to_string(&path).ok()?;
        let snapshot = pg_snapshot::Snapshot::from_json(&json)
            .unwrap_or_else(|error| panic!("parse snapshot {}: {error}", path.display()));
        let (grammar, _warnings) = pg_grammar::compile_project(&snapshot)
            .unwrap_or_else(|error| panic!("compile_project {}: {error}", path.display()));
        Some(grammar)
    }

    /// This compiler must actually build for a template-bearing grammar that declares no phonological rules, not just for the phonology-bearing shape its existing callers exercised.
    #[test]
    fn phonology_free_templated_grammar_compiles_through_this_path() {
        let fixtures = pg_conformance_fixtures::discover();
        let fixture = fixtures
            .iter()
            .find(|f| {
                f.root == pg_conformance_fixtures::Root::Staging
                    && f.name == "backend-template-generic"
            })
            .expect("missing staged fixture backend-template-generic");
        let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load");
        assert!(
            grammar.prules.is_empty(),
            "fixture is used here BECAUSE it declares no phonological rules"
        );
        assert!(
            !grammar.templates.is_empty(),
            "fixture is used here BECAUSE it declares affix templates"
        );

        let compiled = compile_templated_morphotactics(&grammar).expect(
            "a phonology-free templated grammar must compile, not fail with NoCompiledRules",
        );
        assert_eq!(
            compiled.profile.phonological_rule_count, 0,
            "this fixture declares no phonological rules; the profile should say so honestly"
        );
        let (states, arcs) = compiled.proposer.network_counts();
        assert!(
            states > 0 && arcs > 0,
            "must yield a real network, not an empty one that would analyze nothing: {states} \
             states / {arcs} arcs"
        );

        // The zero-slot boundary word: a plain propose against the compiled network must find at least one candidate.
        let mut proposer = compiled.proposer;
        let candidates = proposer.propose("k");
        assert!(
            !candidates.is_empty(),
            "the zero-slot word must propose at least one candidate on the compiled network"
        );
    }

    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/aweti.json); run with --include-ignored"]
    fn large_templated_network_is_prepared_for_binary_search_apply() {
        let handle = std::thread::Builder::new()
            .stack_size(STACK_BYTES)
            .spawn(|| {
                let Some(grammar) = load_aweti() else {
                    eprintln!("skipping: aweti.json not present on disk");
                    return;
                };
                let compiled = compile_templated_morphotactics(&grammar)
                    .expect("Aweti templated compile pipeline must succeed");
                assert!(
                    compiled.network.arcs_sorted_out,
                    "large templated network must use foma's binary-search apply path"
                );
            })
            .expect("spawn large-stack Aweti compile worker");
        handle
            .join()
            .expect("large-stack Aweti compile worker panicked");
    }
}
