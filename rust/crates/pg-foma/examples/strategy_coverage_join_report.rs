//! Deliverable 1: `strategy_coverage` table x measurement JOIN report (`pg.ps1 -Mode run -Example strategy_coverage_join_report`); see `pg_foma::strategy_coverage_join` for the sound/unsound direction split this classifies by.

use std::collections::{HashMap, HashSet};

use pg_conformance_fixtures::{discover, FixtureRef};
use pg_foma::capability::CharacteristicKind;
use pg_foma::strategy_coverage::{representation_of, ALL_STRATEGIES};
use pg_foma::strategy_coverage_join::{
    classify_with_witnesses, kinds_exercised_by, measure_fixture_exact, FixtureExactness,
    JoinVerdict,
};

/// Same safety margin `examples/conf_matrix.rs` applies; no fixture on disk triggers it today.
const MAX_WORDS_PER_FIXTURE: usize = 200;

fn main() {
    // `discover` panics unless a run claims a scope; `all` reaches both fixture roots.
    std::env::set_var("PANGLOSS_CONFORMANCE_SCOPE", "all");
    let fixtures = discover();
    println!("discovered {} fixtures under scope=all\n", fixtures.len());

    let exercised = exercised_kinds_by_fixture(&fixtures);
    let exact = exactness_by_fixture_and_strategy(&fixtures);

    println!("\n================ JOIN: every table row vs. the measurement ================");
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut contradicted_rows = Vec::new();
    let mut unsupported_rows = Vec::new();

    for &strategy in ALL_STRATEGIES {
        for &kind in CharacteristicKind::ALL {
            let row = representation_of(strategy, kind);
            let exhibiting: Vec<FixtureExactness> = fixtures
                .iter()
                .map(FixtureRef::label)
                .filter(|label| exercised.get(label).is_some_and(|k| k.contains(&kind)))
                .map(|label| {
                    let exact = exact
                        .get(&(label.clone(), strategy.label()))
                        .copied()
                        .unwrap_or(false);
                    FixtureExactness { label, exact }
                })
                .collect();
            let (verdict, witnesses) = classify_with_witnesses(row.representation, &exhibiting);
            let tag = verdict_tag(verdict);
            *counts.entry(tag).or_default() += 1;
            println!(
                "  {strategy:?} x {kind:?}: table={:?} verdict={tag} witnesses={witnesses:?}",
                row.representation
            );
            match verdict {
                JoinVerdict::Contradicted => contradicted_rows.push(format!(
                    "{strategy:?} x {kind:?}: table claims CannotRepresent, contradicted by \
                     oracle-exact fixture(s) {witnesses:?}"
                )),
                JoinVerdict::Unsupported => unsupported_rows.push(format!(
                    "{strategy:?} x {kind:?}: table claims {:?}, no exhibiting fixture measured \
                     exact (exhibiting but not exact: {witnesses:?}) -- suggestive only, another \
                     construct the same fixture(s) exercise may be the real cause",
                    row.representation
                )),
                _ => {}
            }
        }
    }

    println!("\n================ HEADLINE ================");
    for tag in ["agreed", "contradicted", "unsupported", "no-evidence"] {
        println!("  {tag}: {}", counts.get(tag).copied().unwrap_or(0));
    }

    println!(
        "\ncontradicted rows -- SOUND findings, table says CannotRepresent but a fixture measured \
         exact anyway:"
    );
    if contradicted_rows.is_empty() {
        println!("  none");
    }
    for r in &contradicted_rows {
        println!("  {r}");
    }

    println!(
        "\nunsupported rows -- suggestive only, NEVER a refutation (see module doc for why):"
    );
    if unsupported_rows.is_empty() {
        println!("  none");
    }
    for r in &unsupported_rows {
        println!("  {r}");
    }
}

fn verdict_tag(v: JoinVerdict) -> &'static str {
    match v {
        JoinVerdict::Agreed => "agreed",
        JoinVerdict::Contradicted => "contradicted",
        JoinVerdict::Unsupported => "unsupported",
        JoinVerdict::NoEvidence => "no-evidence",
    }
}

/// Every fixture's authored `exercises:` tags, reduced to `CharacteristicKind`s, computed once per fixture.
fn exercised_kinds_by_fixture(
    fixtures: &[FixtureRef],
) -> HashMap<String, HashSet<CharacteristicKind>> {
    let mut out = HashMap::new();
    for f in fixtures {
        let words_yaml = f.load_words_yaml();
        let mut ids: HashSet<&str> = HashSet::new();
        for w in &words_yaml.words {
            ids.extend(w.exercises.iter().map(String::as_str));
            for p in &w.parses {
                ids.extend(p.exercises.iter().map(String::as_str));
            }
        }
        out.insert(f.label(), kinds_exercised_by(&ids));
    }
    out
}

/// Every fixture's oracle-exactness on every strategy; an unmeasurable fixture contributes no entries and reads back as not-exact via `unwrap_or(false)` at the call site.
fn exactness_by_fixture_and_strategy(
    fixtures: &[FixtureRef],
) -> HashMap<(String, &'static str), bool> {
    let mut out = HashMap::new();
    for f in fixtures {
        let label = f.label();
        println!("measuring {label}...");
        let Ok(grammar) = pg_grammar::load(&f.load_grammar_xml()) else {
            println!("  COULD NOT MEASURE: grammar failed to load");
            continue;
        };
        if grammar.char_tables.is_empty() {
            println!("  COULD NOT MEASURE: no character table");
            continue;
        }
        let all_words: Vec<String> = f
            .load_words_yaml()
            .words
            .iter()
            .map(|w| w.word.clone())
            .collect();
        if all_words.is_empty() {
            println!("  COULD NOT MEASURE: no words");
            continue;
        }
        let words = if all_words.len() > MAX_WORDS_PER_FIXTURE {
            all_words[..MAX_WORDS_PER_FIXTURE].to_vec()
        } else {
            all_words
        };
        for &strategy in ALL_STRATEGIES {
            let exact = measure_fixture_exact(&grammar, &words, strategy);
            println!("  [{strategy:?}] exact={exact}");
            out.insert((label.clone(), strategy.label()), exact);
        }
    }
    out
}
