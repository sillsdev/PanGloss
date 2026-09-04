//! The capability envelope's verdict and the compiler's outcome must be the same answer.

use std::panic::{self, AssertUnwindSafe};

use pg_conformance_fixtures::discover;
use pg_foma::analyzer::{FomaError, FomaProposer};
use pg_foma::backend_selection::{select_backends, BackendReport};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::production_admission::assess_completed_fst;
use pg_foma::strategy_coverage::ALL_STRATEGIES;
use pg_foma::witnessed_coverage::compile_with_backend_for_measurement;
use pg_grammar::model::Grammar;

/// How a fixture's envelope verdict lines up with what its compiler actually did.
#[derive(Debug, PartialEq, Eq)]
enum Agreement {
    Agree,
    /// The envelope admitted, the compiler refused: safe, but decided in the wrong place.
    TooLax(String),
    /// The envelope refused a backend that compiles; gating on it would lose a working capability.
    TooStrict,
}

/// One fixture x strategy observation; `agreement` asks about capability versus the compiler and `production_blocks` asks, independently, about publishing.
#[derive(Debug)]
struct Row {
    label: String,
    agreement: Agreement,
    compiled: bool,
    production_blocks: bool,
    has_partials: bool,
}

/// The four differential columns, printed together so closing one cannot hide opening another.
#[derive(Default)]
struct DifferentialCounts {
    envelope_refuses_compiler_succeeds: usize,
    envelope_admits_compiler_fails: usize,
    compiler_succeeds_production_rejects: usize,
    compiler_succeeds_production_admits: usize,
}

fn observe(name: &str, strategy: EmissionStrategy) -> Option<Row> {
    let fixture = discover().into_iter().find(|f| f.label() == name)?;
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).ok()?;
    if grammar.char_tables.is_empty() {
        return None;
    }
    let semantics = GrammarSemantics::derive(&grammar);
    let admitted = select_backends(&semantics)
        .report_for(strategy)
        .is_some_and(BackendReport::can_represent);

    // The MEASUREMENT entry point, deliberately: it asks production admission nothing, so a readiness policy can never arrive here disguised as a compiler failure.
    let compiled = match panic::catch_unwind(AssertUnwindSafe(|| {
        compile_with_backend_for_measurement(&grammar, strategy)
    })) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(reason)) => Err(reason),
        Err(_) => Err("panicked".to_owned()),
    };
    let compiled_ok = compiled.is_ok();

    let facts = grammar
        .partial_morpheme_facts()
        .unwrap_or_else(|error| panic!("{name}: partial inventory must be valid: {error}"));
    let admission = assess_completed_fst(&grammar, strategy)
        .unwrap_or_else(|error| panic!("{name}: production admission must decide: {error}"));

    let agreement = match (admitted, compiled) {
        (true, Ok(())) | (false, Err(_)) => Agreement::Agree,
        (true, Err(reason)) => Agreement::TooLax(reason),
        (false, Ok(())) => Agreement::TooStrict,
    };
    Some(Row {
        label: format!("{name} x {}", strategy.label()),
        agreement,
        compiled: compiled_ok,
        production_blocks: admission.blocks_publication(),
        has_partials: facts.has_partials(),
    })
}

fn sweep() -> Vec<Row> {
    let mut rows = Vec::new();
    for fixture in discover() {
        for &strategy in ALL_STRATEGIES {
            if let Some(row) = observe(&fixture.label(), strategy) {
                rows.push(row);
            }
        }
    }
    rows
}

fn counts(rows: &[Row]) -> DifferentialCounts {
    let mut counts = DifferentialCounts::default();
    for row in rows {
        match row.agreement {
            Agreement::TooStrict => counts.envelope_refuses_compiler_succeeds += 1,
            Agreement::TooLax(_) => counts.envelope_admits_compiler_fails += 1,
            Agreement::Agree => {}
        }
        if row.compiled {
            if row.production_blocks {
                counts.compiler_succeeds_production_rejects += 1;
            } else {
                counts.compiler_succeeds_production_admits += 1;
            }
        }
    }
    counts
}

/// Only the surface probe gates on the envelope, so only there may a refusal never cost a compile.
#[test]
fn the_envelope_never_refuses_a_surface_probe_that_compiles() {
    let rows = sweep();
    assert!(
        rows.len() > 100,
        "the sweep must actually observe the fixture set; got {} rows",
        rows.len()
    );

    let too_strict: Vec<&str> = rows
        .iter()
        .filter(|row| {
            row.agreement == Agreement::TooStrict
                && row.label.ends_with(EmissionStrategy::TunedSurfaceProbed.label())
        })
        .map(|row| row.label.as_str())
        .collect();
    assert!(
        too_strict.is_empty(),
        "the envelope refuses {} surface-probe backend(s) that compile, so the gate in \
         `FomaProposer::new` would lose a capability the tree has: {too_strict:#?}",
        too_strict.len()
    );
}

/// Names the constructs behind each surface-probe divergence; a tier alone does not.
#[test]
fn report_uncovered_constructs_behind_surface_probe_divergence() {
    let mut named = 0usize;
    let mut unnamed: Vec<String> = Vec::new();
    for fixture in discover() {
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        if grammar.char_tables.is_empty() {
            continue;
        }
        // A capability refusal names its constructs via `CapabilityDiagnostic`, not `uncovered`.
        let report = match FomaProposer::new(&grammar) {
            Err(FomaError::Incomplete(report)) | Err(FomaError::Unsupported(report)) => report,
            Err(FomaError::CapabilityRefused(diagnostics)) => {
                for diagnostic in &diagnostics {
                    named += 1;
                    eprintln!(
                        "{}: [capability-refused] {} -- {}",
                        fixture.label(),
                        diagnostic.construct,
                        diagnostic.witness
                    );
                }
                continue;
            }
            _ => continue,
        };
        if report.uncovered.is_empty() {
            unnamed.push(format!("{} -- tier {:?}", fixture.label(), report.tier));
            continue;
        }
        for item in &report.uncovered {
            named += 1;
            eprintln!(
                "{}: [{}] {} -- {}",
                fixture.label(),
                item.kind,
                item.id,
                item.reason
            );
        }
        // A rule structural synthesis already claims is an emitter over-report, not a gap.
        let routed = pg_foma::emit::structurally_routed_rule_ordinals(&grammar);
        for item in &report.uncovered {
            let Some(ordinal) = item
                .id
                .strip_prefix("mrule")
                .and_then(|rest| rest.parse::<u32>().ok())
            else {
                continue;
            };
            eprintln!(
                "{}:   {} mrule {ordinal}: structurally_routed={}",
                fixture.label(),
                item.kind,
                routed.contains(&ordinal)
            );
        }
    }
    eprintln!("uncovered items named: {named}");
    eprintln!("refusals naming no construct: {}", unnamed.len());
    for row in &unnamed {
        eprintln!("  {row}");
    }
    assert!(
        named > 0,
        "no surface-probe refusal named a construct, so this report is measuring nothing"
    );
}

/// The published mixed-circumfix-zone fact must never claim a refusal the eager route does not make. `claimed == 0` is the currently-measured state (its one witness, `staging:edge-cases/circumfix-non-first-allomorph-selection`, now compiles), not a proof the condition can never recur -- a genuinely unowned zone mismatch on a future grammar can still trip it.
#[test]
fn the_published_mixed_circumfix_zone_fact_never_over_claims_a_refusal() {
    let mut claimed = 0usize;
    for fixture in discover() {
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        if grammar.char_tables.is_empty() {
            continue;
        }
        if !pg_foma::emit::eager_route_refuses_mixed_circumfix_zone(&grammar) {
            continue;
        }
        claimed += 1;
        assert!(
            FomaProposer::new(&grammar).is_err(),
            "{}: the mixed-circumfix-zone fact claims the eager route refuses, but it compiled",
            fixture.label()
        );
    }
    assert_eq!(
        claimed, 0,
        "mixed-circumfix-zone fact fired on {claimed} fixture(s) -- update this assertion \
         deliberately (its stale value was 1, this fixture's own commit made it 0) rather than \
         reverting to a bare non-vacuity check"
    );
}

/// Compiles `grammar` with `strategy`; asserts the attempt never panics, returns whether it compiled.
fn compiled_without_panicking(grammar: &Grammar, strategy: EmissionStrategy, label: &str) -> bool {
    match panic::catch_unwind(AssertUnwindSafe(|| compile_with_backend_for_measurement(grammar, strategy))) {
        Ok(result) => result.is_ok(),
        Err(_) => panic!("{label}: {strategy:?} panicked instead of returning a typed refusal"),
    }
}

/// The published untokenizable-root-shape fact must never claim a refusal `TemplatedUnderlyingTokens` does not make.
#[test]
fn the_published_untokenizable_root_shape_fact_never_over_claims_a_refusal() {
    let mut claimed = 0usize;
    for fixture in discover() {
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        if grammar.char_tables.is_empty() {
            continue;
        }
        if !pg_foma::replace::grammar_has_untokenizable_root_shape(&grammar) {
            continue;
        }
        claimed += 1;
        assert!(
            !compiled_without_panicking(
                &grammar,
                EmissionStrategy::TemplatedUnderlyingTokens,
                &fixture.label()
            ),
            "{}: the untokenizable-root-shape fact claims TemplatedUnderlyingTokens cannot \
             compile, but it did",
            fixture.label()
        );
    }
    assert!(
        claimed > 0,
        "no fixture exercised the untokenizable-root-shape fact, so this gate proves nothing"
    );
}

/// The published no-tokenizable-root fact must never claim a refusal `PlanComposed` does not make.
#[test]
fn the_published_no_tokenizable_root_fact_never_over_claims_a_refusal() {
    let mut claimed = 0usize;
    for fixture in discover() {
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        if grammar.char_tables.is_empty() || grammar.entries.is_empty() {
            continue;
        }
        if !pg_foma::replace::grammar_has_no_tokenizable_root(&grammar) {
            continue;
        }
        claimed += 1;
        assert!(
            !compiled_without_panicking(&grammar, EmissionStrategy::PlanComposed, &fixture.label()),
            "{}: the no-tokenizable-root fact claims PlanComposed builds no network, but it did",
            fixture.label()
        );
    }
    assert!(
        claimed > 0,
        "no fixture exercised the no-tokenizable-root fact, so this gate proves nothing"
    );
}

/// The root-spelling fact must never claim a drop the surface route does not make.
#[test]
fn the_published_root_spelling_fact_never_over_claims_a_drop() {
    let mut claimed = 0usize;
    for fixture in discover() {
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        if grammar.char_tables.is_empty() {
            continue;
        }
        if !pg_foma::emit::eager_route_drops_root_spellings(&grammar) {
            continue;
        }
        claimed += 1;
        assert!(
            FomaProposer::new(&grammar).is_err(),
            "{}: the root-spelling fact claims dropped spellings, but the route compiled",
            fixture.label()
        );
    }
    assert!(
        claimed > 0,
        "no fixture exercised the root-spelling fact, so this gate proves nothing"
    );
}

/// Both divergence inventories -- too-strict and too-lax -- reported and ratcheted by name.
#[test]
fn report_envelope_compiler_divergence() {
    let rows = sweep();
    let lax: Vec<(&String, &String)> = rows
        .iter()
        .filter_map(|row| match &row.agreement {
            Agreement::TooLax(reason) => Some((&row.label, reason)),
            _ => None,
        })
        .collect();

    let strict: Vec<&String> = rows
        .iter()
        .filter(|row| row.agreement == Agreement::TooStrict)
        .map(|row| &row.label)
        .collect();
    let counts = counts(&rows);
    eprintln!("envelope-vs-compiler: {} observation(s)", rows.len());
    eprintln!(
        "agree: {}",
        rows.iter()
            .filter(|row| row.agreement == Agreement::Agree)
            .count()
    );
    eprintln!(
        "envelope_refuses_compiler_succeeds: {}",
        counts.envelope_refuses_compiler_succeeds
    );
    eprintln!(
        "envelope_admits_compiler_fails: {}",
        counts.envelope_admits_compiler_fails
    );
    eprintln!(
        "compiler_succeeds_production_rejects: {}",
        counts.compiler_succeeds_production_rejects
    );
    eprintln!(
        "compiler_succeeds_production_admits: {}",
        counts.compiler_succeeds_production_admits
    );
    eprintln!(
        "envelope refused, build nonetheless succeeded: {}",
        strict.len()
    );
    for label in &strict {
        eprintln!("  {label}");
    }
    eprintln!(
        "too lax (envelope admitted, compiler refused): {}",
        lax.len()
    );
    for (label, reason) in &lax {
        eprintln!("  {label}: {reason}");
    }

    // Staged: names every too-strict row so a NEW one fails here rather than joining an unnamed backlog.
    const EXPECTED_TOO_STRICT: &[&str] = &[
        // strategy_coverage.rs's ProcessMorphology row is a static, grammar-independent claim; crate::build::unbuildable_marker_material admits this grammar's structural union, so the envelope is stricter than the compiler here until that row becomes grammar-aware.
        "machine:edge-cases/process-morphology-in-place-mutation x plan-composed",
    ];
    let mut strict_sorted: Vec<&str> = strict.iter().map(|label| label.as_str()).collect();
    strict_sorted.sort_unstable();
    let mut expected_sorted = EXPECTED_TOO_STRICT.to_vec();
    expected_sorted.sort_unstable();
    assert_eq!(
        strict_sorted, expected_sorted,
        "the too-strict inventory moved without this ratchet being updated to name the new set"
    );
    // A too-lax row is an envelope admitting what the compiler then refuses; there is no backlog of those, so any one fails here by name.
    const EXPECTED_TOO_LAX: &[&str] = &[];
    let mut lax_sorted: Vec<&str> = lax.iter().map(|(label, _)| label.as_str()).collect();
    lax_sorted.sort_unstable();
    let mut expected_lax_sorted = EXPECTED_TOO_LAX.to_vec();
    expected_lax_sorted.sort_unstable();
    assert_eq!(
        lax_sorted, expected_lax_sorted,
        "the too-lax inventory moved without this ratchet being updated to name the new set"
    );
}

/// The two production columns must partition the compile successes and correspond one-for-one with the grammar's own partial inventory in BOTH directions.
#[test]
fn production_admission_partitions_compile_successes_by_partial_inventory() {
    let rows = sweep();
    let counts = counts(&rows);
    let compiled = rows.iter().filter(|row| row.compiled).count();
    eprintln!(
        "compile successes: {compiled} (production rejects {}, admits {})",
        counts.compiler_succeeds_production_rejects, counts.compiler_succeeds_production_admits
    );
    assert_eq!(
        counts.compiler_succeeds_production_rejects + counts.compiler_succeeds_production_admits,
        compiled,
        "every compile success must land in exactly one production column"
    );

    let over_refused: Vec<&str> = rows
        .iter()
        .filter(|row| row.production_blocks && !row.has_partials)
        .map(|row| row.label.as_str())
        .collect();
    assert!(
        over_refused.is_empty(),
        "production admission refused {} partial-free observation(s), so it is refusing more than \
         the policy: {over_refused:#?}",
        over_refused.len()
    );

    let under_refused: Vec<&str> = rows
        .iter()
        .filter(|row| row.has_partials && !row.production_blocks)
        .map(|row| row.label.as_str())
        .collect();
    assert!(
        under_refused.is_empty(),
        "production admission ADMITTED {} partial-bearing observation(s), so the policy is not \
         being applied: {under_refused:#?}",
        under_refused.len()
    );

    // Non-vacuity, scope-honest: assert a rejection was really observed only when the claimed scope actually contains a partial-bearing fixture, and say so either way.
    let partial_bearing = rows.iter().filter(|row| row.has_partials).count();
    eprintln!("partial-bearing observations in this scope: {partial_bearing}");
    if partial_bearing == 0 {
        eprintln!(
            "NOTE: no discovered fixture declares a partial morpheme, so this run witnesses only \
             the ADMIT direction; the refuse direction is witnessed by \
             partial_fst_production_admission_gate's synthetic fixtures"
        );
    } else {
        assert!(
            counts.compiler_succeeds_production_rejects > 0
                || rows.iter().all(|row| !(row.has_partials && row.compiled)),
            "a partial-bearing fixture compiled but produced no production rejection"
        );
    }
    assert!(
        counts.compiler_succeeds_production_admits > 0,
        "no observation was production-admitted, so this census cannot see an over-refusal"
    );
}
