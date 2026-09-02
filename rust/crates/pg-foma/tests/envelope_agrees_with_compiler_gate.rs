//! The capability envelope's verdict and the compiler's outcome must be the same answer.

use std::panic::{self, AssertUnwindSafe};

use pg_conformance_fixtures::discover;
use pg_foma::analyzer::{FomaError, FomaProposer};
use pg_foma::backend_selection::{select_backends, BackendReport};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::strategy_coverage::ALL_STRATEGIES;
use pg_foma::witnessed_coverage::compile_with_backend;
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

    // Compiled regardless of the verdict; honouring it would hide the too-strict direction.
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

/// The published closure fact must never claim a refusal the eager route does not make. `claimed == 0` is now the permanent expectation (see `TunedSurfaceClosureCheck`'s own doc), not an unexercised gate: any nonzero count needs a real fixture, never an inflated assertion here.
#[test]
fn the_published_closure_fact_never_over_claims_a_refusal() {
    let mut claimed = 0usize;
    for fixture in discover() {
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        if grammar.char_tables.is_empty() {
            continue;
        }
        if !pg_foma::emit::eager_route_refuses_unbounded_closure(&grammar) {
            continue;
        }
        claimed += 1;
        assert!(
            FomaProposer::new(&grammar).is_err(),
            "{}: the closure fact claims the eager route refuses, but it compiled",
            fixture.label()
        );
    }
    assert_eq!(
        claimed, 0,
        "closure fact fired on {claimed} fixture(s) -- it was proven permanently false \
         (`realizational_rule_is_semantically_unbounded` always returns `false`); if this is no \
         longer 0, that fact regressed or a new grammar shape resurrected the condition"
    );
}

/// The published unclaimed-standalone-rule fact must never claim a refusal the eager route does not make.
#[test]
fn the_published_unclaimed_standalone_rule_fact_never_over_claims_a_refusal() {
    let mut claimed = 0usize;
    for fixture in discover() {
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        if grammar.char_tables.is_empty() {
            continue;
        }
        if !pg_foma::emit::eager_route_refuses_unclaimed_standalone_rule(&grammar) {
            continue;
        }
        claimed += 1;
        assert!(
            FomaProposer::new(&grammar).is_err(),
            "{}: the unclaimed-standalone-rule fact claims the eager route refuses, but it compiled",
            fixture.label()
        );
    }
    // Role::Process always implies is_structural_rule now, so this fact is provably unwitnessable.
    assert_eq!(
        claimed, 0,
        "the unclaimed-standalone-rule fact fired for {claimed} fixture(s), but \
         `standalone_rule_unclaimed_role` defers every `Role::Process` rule to \
         `is_structural_rule`, and `rule_role(g, mid) == Role::Process` requires allomorph 0 to \
         carry an `OutputAction::Modify` -- which alone makes `is_structural_rule`'s \
         `has_unemittable_action` check true, so this fact can never fire for any grammar unless \
         that structural implication itself changed"
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
    match panic::catch_unwind(AssertUnwindSafe(|| compile_with_backend(grammar, strategy))) {
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
    eprintln!(
        "too lax (envelope admitted, compiler refused): {}",
        lax.len()
    );
    for (label, reason) in &lax {
        eprintln!("  {label}: {reason}");
    }

    // Staged: names every too-strict row so a NEW one fails here rather than joining an unnamed backlog.
    const EXPECTED_TOO_STRICT: &[&str] = &[
        "machine:edge-cases/loader-isactive-breadth x templated-underlying-tokens",
        "machine:edge-cases/strrep-identity x templated-underlying-tokens",
        "machine:edge-cases/truncate-morphotactic x templated-underlying-tokens",
    ];
    let mut strict_sorted: Vec<&str> = strict.iter().map(|label| label.as_str()).collect();
    strict_sorted.sort_unstable();
    let mut expected_sorted = EXPECTED_TOO_STRICT.to_vec();
    expected_sorted.sort_unstable();
    assert_eq!(
        strict_sorted, expected_sorted,
        "the too-strict inventory moved without this ratchet being updated to name the new set"
    );
    assert!(
        lax.is_empty(),
        "the too-lax inventory must be empty; got: {lax:#?}"
    );
}
