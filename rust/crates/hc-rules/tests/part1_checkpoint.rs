//! Part-1 checkpoint gate (plan §5.4/§5.5): compile **every** phonological-rule pattern from the
//! real Indonesian (5 rules) and Amharic (7 rules) grammars — LHS, each subrule's RHS, and each
//! subrule's left/right environment — through the [`hc_rules::bridge`] into a frozen `hc_fst::Fst`,
//! without error. This is a structural gate (like the M1 loader gate): it proves the bridge handles
//! the real authored constructs, not a hand-built subset.
//!
//! The sample grammars are untracked local corpus files (per `rust-conversion.md` §8); the test
//! self-skips when they are absent (fresh clone / CI).

use std::path::{Path, PathBuf};

use hc_grammar::model::{Grammar, Pattern, PhonRuleDef};
use hc_rules::bridge::PatternBridge;

fn sample_path(name: &str) -> Option<PathBuf> {
    // CARGO_MANIFEST_DIR = .../rust/crates/hc-rules ; samples live at repo_root/samples/data.
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

/// Compile one pattern both deterministically and nondeterministically (analysis uses the latter),
/// asserting a non-degenerate FST is produced in each case.
fn compile_ok(grammar: &Grammar, pattern: &Pattern, what: &str) -> usize {
    for det in [true, false] {
        let bridge = PatternBridge::new(grammar).deterministic(det);
        let compiled = bridge
            .compile_pattern(pattern)
            .unwrap_or_else(|e| panic!("{what}: bridge failed (det={det}): {e}"));
        let fst = compiled.compile();
        // Every compiled FST has at least a start state (empty patterns accept the empty string).
        assert!(
            fst.state_count() >= 1,
            "{what}: FST has no states (det={det})"
        );
    }
    1
}

/// Compile every phonological-rule pattern in `grammar`; returns (rule count, pattern count).
fn compile_all_prule_patterns(grammar: &Grammar, tag: &str) -> (usize, usize) {
    let mut pattern_count = 0usize;
    let mut alpha_var_patterns = 0usize;
    for (ri, prule) in grammar.prules.iter().enumerate() {
        let prule = match prule {
            PhonRuleDef::Rewrite(r) => r,
            // No reference grammar (Indonesian/Amharic/Sena) uses `<MetathesisRule>` (still zero
            // occurrences post-W4); its pattern has its own dedicated compile path
            // (`hc_rules::metathesis::compile_switch_pattern`, which additionally wraps the two
            // switch positions in named capture groups) exercised by
            // `crates/hc-parse/tests/csharp_port_metathesis.rs` instead of this LHS/RHS/environment
            // structural census, which is specific to `RewriteRuleDef`'s shape.
            PhonRuleDef::Metathesis(m) => {
                pattern_count += compile_ok(
                    grammar,
                    &m.pattern,
                    &format!("{tag} prule[{ri}] (metathesis) pattern"),
                );
                continue;
            }
        };
        pattern_count += compile_ok(grammar, &prule.lhs, &format!("{tag} prule[{ri}] LHS"));
        // Census: any alpha variables anywhere in this rule's patterns?
        if !prule.vars.vars.is_empty() {
            alpha_var_patterns += 1;
        }
        for (si, sr) in prule.subrules.iter().enumerate() {
            pattern_count += compile_ok(
                grammar,
                &sr.rhs,
                &format!("{tag} prule[{ri}] sub[{si}] RHS"),
            );
            if let Some(le) = &sr.left_env {
                pattern_count +=
                    compile_ok(grammar, le, &format!("{tag} prule[{ri}] sub[{si}] leftEnv"));
            }
            if let Some(re) = &sr.right_env {
                pattern_count += compile_ok(
                    grammar,
                    re,
                    &format!("{tag} prule[{ri}] sub[{si}] rightEnv"),
                );
            }
        }
    }
    eprintln!(
        "{tag}: {} phonological rules, {} patterns compiled, {} rules declaring alpha variables",
        grammar.prules.len(),
        pattern_count,
        alpha_var_patterns
    );
    (grammar.prules.len(), pattern_count)
}

fn load(name: &str) -> Option<Grammar> {
    let path = sample_path(name)?;
    let xml = std::fs::read_to_string(path).expect("read sample grammar");
    Some(hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {name}: {e}")))
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn compiles_all_indonesian_prule_patterns() {
    let Some(grammar) = load("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    // Independently confirmed count: 5 `<PhonologicalRule ` in indonesian-hc.xml.
    let (rules, patterns) = compile_all_prule_patterns(&grammar, "indonesian");
    assert_eq!(rules, 5, "indonesian phonological-rule count");
    assert!(
        patterns >= rules,
        "each rule contributes at least its LHS pattern"
    );
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn compiles_all_amharic_prule_patterns() {
    let Some(grammar) = load("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    // Independently confirmed count: 7 `<PhonologicalRule ` in amharic-hc.xml.
    let (rules, patterns) = compile_all_prule_patterns(&grammar, "amharic");
    assert_eq!(rules, 7, "amharic phonological-rule count");
    assert!(patterns >= rules);
}
