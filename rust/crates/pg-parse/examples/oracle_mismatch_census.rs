//! Collects every oracle mismatch instead of panicking on the first, unlike `conformance_fixtures_gate.rs`.

use pg_conformance_fixtures::discover;
use pg_parse::Morpher;

fn main() {
    let fixtures = discover();
    let mut mismatches = 0usize;
    let mut checked = 0usize;
    for f in &fixtures {
        let words_yaml = f.load_words_yaml();
        if let Some(reason) = words_yaml.skip_in_generic_replay() {
            eprintln!("skipping {}: {reason}", f.label());
            continue;
        }
        let xml = f.load_grammar_xml();
        let grammar = match pg_grammar::load(&xml) {
            Ok(g) => g,
            Err(e) => {
                println!("{}: grammar failed to load: {e}", f.label());
                continue;
            }
        };
        let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);
        for w in &words_yaml.words {
            if !w.adapter_visible() {
                continue;
            }
            let outcome = morpher.parse_word(&w.word);
            checked += 1;
            if w.expect_skip {
                if !outcome.invalid_shape {
                    mismatches += 1;
                    println!(
                        "{}: word {:?} expected SKIPPED but engine produced a result",
                        f.label(),
                        w.word
                    );
                }
                continue;
            }
            if outcome.invalid_shape {
                mismatches += 1;
                println!("{}: word {:?} unexpectedly SKIPPED", f.label(), w.word);
                continue;
            }
            let got = outcome.signature();
            let expected = w.expected_signature();
            if got != expected {
                mismatches += 1;
                println!(
                    "{}: word {:?} got {got:?} expected {expected:?}",
                    f.label(),
                    w.word
                );
            }
        }
    }
    println!(
        "--- {mismatches} mismatch(es) across {checked} checked words, {} fixtures ---",
        fixtures.len()
    );
}
