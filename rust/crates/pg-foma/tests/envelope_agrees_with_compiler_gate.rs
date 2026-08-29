//! The capability envelope's verdict and the compiler's outcome must be the same answer.

use std::panic::{self, AssertUnwindSafe};

use pg_conformance_fixtures::discover;
use pg_foma::analyzer::{FomaError, FomaProposer};
use pg_foma::backend_selection::{select_backends, BackendReport};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::strategy_coverage::ALL_STRATEGIES;
use pg_foma::witnessed_coverage::compile_with_backend;

/// How a fixture's envelope verdict lines up with what its compiler actually did.
#[derive(Debug, PartialEq, Eq)]
enum Agreement {
    Agree,
    /// The envelope admitted the backend and the compiler then refused. Safe -- the compiler caught
    /// it -- but decided in the wrong place and in the wrong vocabulary.
    TooLax(String),
    /// The envelope refused a backend that compiles. Gating on the envelope would lose a capability
    /// the tree actually has, so this direction must stay empty.
    TooStrict,
}

fn observe(name: &str, strategy: EmissionStrategy) -> Option<(String, Agreement)> {
    let fixture = discover().into_iter().find(|f| f.label() == name)?;
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).ok()?;
    if grammar.char_tables.is_empty() {
        return None;
    }
    let semantics = GrammarSemantics::derive(&grammar);
    let admitted = select_backends(&semantics)
        .report_for(strategy)
        .is_some_and(BackendReport::can_represent);

    // Compiled REGARDLESS of the verdict: the disagreement is the measurement, so honouring the
    // verdict here would make the too-strict direction unobservable by construction.
    let compiled = match panic::catch_unwind(AssertUnwindSafe(|| {
        compile_with_backend(&grammar, strategy)
    })) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(reason)) => Err(reason),
        Err(_) => Err("panicked".to_owned()),
    };

    let agreement = match (admitted, compiled) {
        (true, Ok(())) | (false, Err(_)) => Agreement::Agree,
        (true, Err(reason)) => Agreement::TooLax(reason),
        (false, Ok(())) => Agreement::TooStrict,
    };
    Some((format!("{name} x {}", strategy.label()), agreement))
}

fn sweep() -> Vec<(String, Agreement)> {
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

/// The surface probe gates on the envelope, so for it a refusal must never cost a working compile.
///
/// Asserted for this backend alone. The other two also record `TooStrict` rows, but a build that
/// succeeds is not yet evidence there that the envelope is wrong: `crate::build`'s own doc records a
/// marker-bearing plan building a network that proposed nothing for 19 of 20 corpus words, so those
/// builds succeed while under-generating. Widening this assertion needs the builders to refuse what
/// they cannot faithfully build, not a looser envelope.
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
        .filter(|(label, a)| {
            *a == Agreement::TooStrict
                && label.ends_with(EmissionStrategy::TunedSurfaceProbed.label())
        })
        .map(|(label, _)| label.as_str())
        .collect();
    assert!(
        too_strict.is_empty(),
        "the envelope refuses {} surface-probe backend(s) that compile, so the gate in \
         `FomaProposer::new` would lose a capability the tree has: {too_strict:#?}",
        too_strict.len()
    );
}

/// Names the constructs behind each surface-probe divergence -- the input to writing a predicate.
///
/// A tier alone ("Partial { uncovered: 1 }") does not say what could not be done, which is what
/// ADR-0001 asks a refusal to carry. `EmitReport::uncovered` has carried kind/id/reason all along;
/// nothing on the production path reads it.
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
        let report = match FomaProposer::new(&grammar) {
            Err(FomaError::Incomplete(report)) | Err(FomaError::Unsupported(report)) => report,
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
        // Which route the profile thinks covers each reduplication rule. A rule the peel DOES claim
        // while the eager route still reports it uncovered is an emitter over-report, not a
        // capability gap, and the two want opposite fixes.
        let semantics = GrammarSemantics::derive(&grammar);
        for detail in semantics.characteristics().reduplication_details() {
            eprintln!(
                "{}:   redup mrule {} allo #{}: peel_attempted={} structural_composite_attempted={}",
                fixture.label(),
                detail.rule.0,
                detail.allomorph_index,
                detail.peel_attempted,
                detail.structural_composite_attempted
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

/// The too-lax inventory: the envelope admitted, the compiler refused. Reported, not yet gated.
#[test]
fn report_envelope_compiler_divergence() {
    let rows = sweep();
    let lax: Vec<(&String, &String)> = rows
        .iter()
        .filter_map(|(label, a)| match a {
            Agreement::TooLax(reason) => Some((label, reason)),
            _ => None,
        })
        .collect();

    let strict: Vec<&String> = rows
        .iter()
        .filter(|(_, a)| *a == Agreement::TooStrict)
        .map(|(label, _)| label)
        .collect();
    eprintln!("envelope-vs-compiler: {} observation(s)", rows.len());
    eprintln!(
        "agree: {}",
        rows.iter().filter(|(_, a)| *a == Agreement::Agree).count()
    );
    eprintln!(
        "envelope refused, build nonetheless succeeded: {}",
        strict.len()
    );
    for label in &strict {
        eprintln!("  {label}");
    }
    eprintln!("too lax (envelope admitted, compiler refused): {}", lax.len());
    for (label, reason) in &lax {
        eprintln!("  {label}: {reason}");
    }

    // Non-vacuity only, mirroring `faithfulness_coverage_gate`: this becomes an equality against 0
    // once each construct above is answered by a capability predicate instead of by the emitter.
    assert!(
        rows.iter().any(|(_, a)| *a == Agreement::Agree),
        "no fixture agreed, so this sweep is measuring nothing"
    );
}
