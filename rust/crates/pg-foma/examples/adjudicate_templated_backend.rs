//! One-off measurement (`pg.ps1 -Mode run -Example adjudicate_templated_backend`): raw templated-network acceptance vs. post-confirm result, per fixture.

use pg_conformance_fixtures::{discover, FixtureRef};
use pg_foma::backend_optimizer::Certification;
use pg_foma::backend_runtime::{
    certify_word, word_proposal_containment, RunEvaluationCache, RuntimeBudget,
};
use pg_foma::enumerate::{enumerate_default, CandidateRole, EmissionStrategy, LoweredCandidate};
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::lowering_adapter::LoweringAdapter;
use pg_grammar::model::Grammar;

/// The nine disputed (fixture name, category) pairs under measurement.
const NINE: &[(&str, &str)] = &[
    ("diacritic-segments", "edge-cases"),
    ("disjunctive-recheck", "edge-cases"),
    ("loader-isactive-breadth", "edge-cases"),
    ("mpr-gated-exception", "edge-cases"),
    ("stem-name-restricted-root-allomorph", "edge-cases"),
    ("strrep-identity", "edge-cases"),
    ("truncate-morphotactic", "edge-cases"),
    ("suffixing-evidential-adjacency-chain", "languages"),
    ("backend-gated-generic", "edge-cases"),
];

/// The four witness forms the two earlier agents disagreed about.
const WITNESSES: &[&str] = &["vokadan", "gül", "zal", "gas"];

fn main() {
    // `discover` panics unless a run claims a scope; `all` reaches both fixture roots.
    std::env::set_var("PANGLOSS_CONFORMANCE_SCOPE", "all");

    println!("commit under measurement: see `git rev-parse HEAD` at run time");
    println!(
        "columns: A = raw templated-network proposals (pre-confirm); B = post-full-HC-confirm \
         result vs oracle\n"
    );

    let all_fixtures = discover();
    let mut witness_rows: Vec<(String, String, String, String, String)> = Vec::new();

    for &(name, category) in NINE {
        let Some(fixture) = all_fixtures
            .iter()
            .find(|f| f.name == name && f.category == category)
        else {
            println!("=== {category}/{name} === NOT FOUND in discovery (scope/root mismatch)\n");
            continue;
        };
        run_fixture(fixture, &mut witness_rows);
    }

    println!("\n================ WITNESS FORMS ================");
    println!(
        "{:<10} {:<45} {:<12} {:<40} {:<40}",
        "word", "fixture", "yaml says", "column A (raw)", "column B (confirmed)"
    );
    for (word, fixture, yaml, a, b) in &witness_rows {
        println!("{word:<10} {fixture:<45} {yaml:<12} {a:<40} {b:<40}");
    }
}

fn run_fixture(
    fixture: &FixtureRef,
    witness_rows: &mut Vec<(String, String, String, String, String)>,
) {
    let label = fixture.label();
    println!("=== {label} ===");

    let grammar_xml = fixture.load_grammar_xml();
    let grammar: Grammar = match pg_grammar::load(&grammar_xml) {
        Ok(g) => g,
        Err(e) => {
            println!("  COULD NOT MEASURE: grammar failed to load: {e}\n");
            return;
        }
    };
    if grammar.char_tables.is_empty() {
        println!("  COULD NOT MEASURE: grammar has no character table\n");
        return;
    }

    let words_yaml = fixture.load_words_yaml();
    if words_yaml.words.is_empty() {
        println!("  COULD NOT MEASURE: words.yaml has no words\n");
        return;
    }
    let words: Vec<String> = words_yaml.words.iter().map(|w| w.word.clone()).collect();

    let semantics = GrammarSemantics::derive(&grammar);
    let phonology = PhonologyProbe::new_with_semantics(&semantics);
    let baseline_plan =
        enumerate_default(&grammar, semantics.prules_in_order(), phonology.as_ref());
    let candidate = LoweredCandidate {
        label: "adjudicate-templated-backend",
        plan: baseline_plan,
        adapter: LoweringAdapter::for_strategy(EmissionStrategy::TemplatedUnderlyingTokens),
        // Not PlanComposed, so never CandidateRole::Baseline -- see LoweredCandidate::role's doc.
        role: CandidateRole::Alternative,
    };

    let mut cache = match RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default()) {
        Ok(cache) => cache,
        Err(fault) => {
            println!("  COULD NOT MEASURE: oracle preparation faulted: {fault}\n");
            return;
        }
    };

    let observed = pg_foma::backend_runtime::evaluate_plans_observed_with_cache(
        &grammar,
        &[candidate],
        &words,
        RuntimeBudget::default(),
        &mut cache,
    );
    let observation = &observed[0];
    println!(
        "  realized_strategy={:?} certification={:?}",
        observation.evaluation.realized_strategy, observation.evaluation.certification
    );

    let Some(evidence) = &observation.words else {
        println!(
            "  COULD NOT MEASURE per-word evidence: evaluation did not reach comparable words \
             (certification={:?})\n",
            observation.evaluation.certification
        );
        return;
    };

    let mut a_holds = 0usize;
    let mut a_fails: Vec<String> = Vec::new();
    let mut a_overgenerates: Vec<String> = Vec::new();
    let mut b_holds = 0usize;
    let mut b_fails: Vec<(String, String)> = Vec::new();
    let mut divergent_word: Option<String> = None;

    for word_evidence in evidence {
        let word = &word_evidence.word;
        let expects_nothing = word_evidence.expected.is_empty();

        // Column A: raw containment (does the raw proposal set contain every oracle identity?).
        let a_ok = match word_proposal_containment(word_evidence) {
            Ok(()) => {
                a_holds += 1;
                true
            }
            Err(gap) => {
                a_fails.push(format!("{word}: {gap}"));
                false
            }
        };
        if expects_nothing && !word_evidence.proposals.is_empty() {
            a_overgenerates.push(format!(
                "{word}: raw net proposed {} candidate(s) though the oracle asserts no valid \
                 derivation",
                word_evidence.proposals.len()
            ));
        }

        // Column B: full post-confirm result vs oracle, exact identity-set equality.
        let cert = certify_word(
            &grammar,
            word.clone(),
            &word_evidence.expected,
            &word_evidence.actual,
        );
        let b_ok = matches!(cert, Certification::FullHcConfirmed { .. });
        if b_ok {
            b_holds += 1;
        } else {
            b_fails.push((word.clone(), format!("{cert:?}")));
        }

        if divergent_word.is_none() && a_ok != b_ok {
            divergent_word = Some(word.clone());
        }
        // ADR-0001-legal divergence: raw over-generated something confirm then pruned away.
        if divergent_word.is_none()
            && expects_nothing
            && !word_evidence.proposals.is_empty()
            && b_ok
        {
            divergent_word = Some(word.clone());
        }

        if WITNESSES.contains(&word.as_str()) {
            let yaml_says = if word_evidence.expected.is_empty() {
                let entry = words_yaml.words.iter().find(|w| &w.word == word);
                match entry {
                    Some(e) if e.expect_skip => "expect_skip".to_string(),
                    Some(e) if e.expect_fail => "expect_fail".to_string(),
                    _ => "no-analyses(?)".to_string(),
                }
            } else {
                format!("real word ({} parse(s))", word_evidence.expected.len())
            };
            let a_desc = if word_evidence.proposals.is_empty() {
                "raw: 0 proposals".to_string()
            } else if a_ok {
                format!(
                    "raw: {} proposal(s), contains oracle",
                    word_evidence.proposals.len()
                )
            } else {
                format!(
                    "raw: {} proposal(s), MISSING oracle identity",
                    word_evidence.proposals.len()
                )
            };
            let b_desc = if b_ok {
                "confirmed: matches oracle exactly".to_string()
            } else {
                format!("confirmed: MISMATCH ({cert:?})")
            };
            witness_rows.push((word.clone(), label.clone(), yaml_says, a_desc, b_desc));
        }
    }

    println!(
        "  Column A (raw containment): {a_holds}/{} words HELD (raw proposals contained every \
         oracle identity)",
        evidence.len()
    );
    for f in &a_fails {
        println!("    A-FAIL: {f}");
    }
    for o in &a_overgenerates {
        println!("    A-OVERGENERATES (legal under ADR-0001 if B still matches): {o}");
    }
    println!(
        "  Column B (post-confirm vs oracle): {b_holds}/{} words FullHcConfirmed",
        evidence.len()
    );
    for (word, detail) in &b_fails {
        println!("    B-FAIL {word}: {detail}");
    }
    match &divergent_word {
        Some(w) => println!("  columns DIFFER on at least one word: {w:?}"),
        None => println!("  columns never differed for this fixture's corpus"),
    }

    let verdict = if b_holds == evidence.len() {
        "VERDICT: backend genuinely works for this fixture's corpus (column B == oracle) -- any \
         column-A over-generation is LEGAL under ADR-0001 (pruned by confirm); envelope refusal is \
         TOO STRICT here."
    } else {
        "VERDICT: at least one word's CONFIRMED result diverges from the oracle -- a real defect, \
         not merely legal over-generation; envelope refusal is JUSTIFIED here."
    };
    println!("  {verdict}\n");
}
