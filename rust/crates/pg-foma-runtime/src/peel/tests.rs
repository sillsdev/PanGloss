use super::*;
use pg_grammar::model::{Grammar, MorphemeId};

fn sample_path(name: &str) -> Option<std::path::PathBuf> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn load_indonesian() -> Option<Grammar> {
    let path = sample_path("indonesian-hc.xml")?;
    let xml = std::fs::read_to_string(&path).expect("read grammar");
    Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
}

fn load_sena() -> Option<Grammar> {
    let path = sample_path("sena-hc.xml")?;
    let xml = std::fs::read_to_string(&path).expect("read grammar");
    Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
}

/// Sena has no reduplication rules at all -- the peeler must be a true no-op, never calling `propose`.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn sena_has_no_redup_rules() {
    let Some(g) = load_sena() else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let peeler = ReduplicationPeeler::new(&g);
    assert!(!peeler.has_redup_rules());
    let mut calls = 0usize;
    let mut propose = |_: &str| {
        calls += 1;
        Vec::new()
    };
    let budget = ComposeBudget::unbounded();
    let out = peeler
        .peel_candidates(&g, "mbali", &budget, &mut propose)
        .expect("a no-redup grammar's peel never consults the chain-depth budget at all");
    assert!(out.is_empty());
    assert_eq!(
        calls, 0,
        "no-redup grammar must never invoke the propose closure"
    );
}

/// Indonesian's redup rules recover "membagi-bagi" when residual "membagi" is handed a stub proposer returning one fixed base candidate.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn reduplication_recovers_known_corpus_word() {
    let Some(g) = load_indonesian() else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let peeler = ReduplicationPeeler::new(&g);
    assert!(
        peeler.has_redup_rules(),
        "Indonesian must have at least one redup rule"
    );

    let root = g.entries[0].morpheme;
    let mut seen_residuals: Vec<String> = Vec::new();
    let mut propose = |residual: &str| {
        seen_residuals.push(residual.to_string());
        if residual == "membagi" {
            vec![Candidate {
                morphemes: vec![root],
                root_index: 0,
            }]
        } else {
            Vec::new()
        }
    };
    let budget = ComposeBudget::unbounded();
    let out = peeler
        .peel_candidates(&g, "membagi-bagi", &budget, &mut propose)
        .expect("an unbounded chain-depth budget never refuses");
    assert!(
        !out.is_empty(),
        "expected at least one reduplication candidate for membagi-bagi"
    );
    assert!(seen_residuals.iter().any(|r| r == "membagi"));
    for c in &out {
        assert_eq!(c.root_index, 0);
        assert!(
            c.morphemes.len() >= 2,
            "expected root + at least the redup morpheme"
        );
    }
}

/// A word engineered to be maximally self-similar (every character identical): every scan position matches at every layer, so nested recursion is genuinely, repeatedly exercised, the adversarial shape the chain-depth budget exists for.
fn monochar_word(len: usize) -> String {
    "a".repeat(len)
}

/// A small chain-depth cap deterministically refuses a genuinely deep self-similar chain, never a hang or unbounded blow-up.
#[test]
fn deep_self_similar_chain_is_refused_deterministically_under_a_small_cap() {
    let g = minimal_redup_grammar_for_test();
    assert!(reduplication_rule_is_peelable(&g, MRuleId(0)));
    let peeler = ReduplicationPeeler::new(&g);
    assert!(peeler.has_redup_rules());
    let mut propose = |_: &str| Vec::new();
    let budget = ComposeBudget::unbounded().with_chain_depth_cap(3);
    let word = monochar_word(16);
    let err = peeler
        .peel_candidates(&g, &word, &budget, &mut propose)
        .expect_err(
            "a monochar word's self-similar structure genuinely needs more than 3 nested \
             reduplication layers; a cap of 3 must refuse it deterministically rather than \
             silently truncating or hanging",
        );
    match err {
        ComposeError::ChainDepthExceeded { depth, limit, site } => {
            assert_eq!(limit, 3);
            assert!(depth > limit, "the reported depth must exceed the cap");
            assert_eq!(site, CHAIN_DEPTH_SITE);
        }
    }
}

/// The same adversarial word, under a generous cap, succeeds and genuinely recurses, proven by a `propose`-call count strictly above the single-layer baseline.
#[test]
fn deep_self_similar_chain_succeeds_under_a_generous_cap_and_genuinely_recurses() {
    let g = minimal_redup_grammar_for_test();
    let peeler = ReduplicationPeeler::new(&g);
    let mut propose_calls = 0usize;
    let mut propose = |_: &str| {
        propose_calls += 1;
        Vec::new()
    };
    // Generous but still explicit and finite: never hand an unbounded budget to an adversarial input, even here.
    let budget = ComposeBudget::unbounded().with_chain_depth_cap(64);
    let word = monochar_word(10);
    let out = peeler
        .peel_candidates(&g, &word, &budget, &mut propose)
        .expect("a generous cap must admit this word in full");
    assert!(out.is_empty(), "the stub propose always returns no base candidates, so no wrapped candidate can exist either, regardless of how many layers were tried");
    assert!(
        propose_calls > 1,
        "a purely single-layer (non-recursive) peel would call propose a small, bounded \
         number of times for a 10-char word; genuine nested recursion must call it MORE, one \
         extra time per accepted nested layer -- got {propose_calls} calls"
    );
}

/// An ordinary single-layer reduplication must succeed even under the smallest meaningful cap (1): an attempt that finds nothing never counts against the budget.
#[test]
fn ordinary_single_layer_reduplication_never_trips_the_smallest_cap() {
    let g = minimal_redup_grammar_for_test();
    let peeler = ReduplicationPeeler::new(&g);
    let root = MorphemeId(1);
    let mut seen_residuals: Vec<String> = Vec::new();
    let mut propose = |residual: &str| {
        seen_residuals.push(residual.to_string());
        if residual == "kab" {
            vec![Candidate {
                morphemes: vec![root],
                root_index: 0,
            }]
        } else {
            Vec::new()
        }
    };
    let budget = ComposeBudget::unbounded().with_chain_depth_cap(1);
    let out = peeler
        .peel_candidates(&g, "kabkab", &budget, &mut propose)
        .expect(
            "an ordinary single-layer reduplication (whose residual has no further structure \
             of its own) must never trip even the smallest cap -- the nested-peel ATTEMPT on \
             \"kab\" finds nothing and so never consults the budget a second time",
        );
    assert!(
        !out.is_empty(),
        "the ordinary redup candidate must still be produced"
    );
    assert!(seen_residuals.iter().any(|r| r == "kab"));
}
