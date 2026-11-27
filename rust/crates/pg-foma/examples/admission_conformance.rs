//! Checks over the conformance suite that the admitted restricted parse keeps every analysis of the plain parse.
use foma::options::FomaOptions;
use pg_conformance_fixtures::{FixtureRef, Root, WordEntry};
use pg_foma::admission::{build_rule_admission, build_tiled, RuleAdmission};
use pg_parse::{Morpher, ParseOptions, ParseOutcome};
use pg_rules::stratum::RuleRef;
use std::fs;
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

const WORK_CAP: usize = 2_000_000;

fn signature(outcome: &ParseOutcome) -> Vec<(Vec<u32>, i32)> {
    let mut keys: Vec<_> = outcome
        .structured
        .iter()
        .map(|analysis| (analysis.morpheme_ids.clone(), analysis.root_morpheme_index))
        .collect();
    keys.sort();
    keys
}

#[derive(Default)]
struct Tally {
    words: usize,
    capped: usize,
    mismatches: Vec<String>,
    plain_steps: u64,
    confirm_steps: u64,
    /// Texts compared between the tables and foma's apply, and those where they differed.
    compared: usize,
    table_differences: Vec<String>,
    tiled: TiledTally,
}

/// The tiled acceptor over the words of one grammar: where it applied, what it rejected, and any rejected word that has an analysis.
#[derive(Default)]
struct TiledTally {
    active: bool,
    off: Option<String>,
    applied: usize,
    rejected: usize,
    violations: Vec<String>,
}

/// The word and edits of it, each checked against foma's apply on the whole net.
fn compare_with_foma(admission: &RuleAdmission, word: &str, tally: &mut Tally) {
    let chars: Vec<char> = pg_grammar::nfd::nfd(word).chars().collect();
    let mut texts = vec![word.to_owned()];
    for at in 0..chars.len() {
        for edit in ["", "z", "\u{301}"] {
            let mut text: String = chars[..at].iter().collect();
            text.push_str(edit);
            text.extend(&chars[at + 1..]);
            texts.push(text);
        }
        texts.push(chars[at..].iter().collect());
        texts.push(chars[..=at].iter().collect());
    }
    for text in texts {
        tally.compared += 1;
        if admission.admit_with_foma(&text, usize::MAX) != Some(admission.admit(&text)) {
            tally.table_differences.push(text);
        }
    }
}

fn run(
    fixture: &FixtureRef,
    settings: &pg_conformance_fixtures::MorpherSettings,
    words: &[WordEntry],
    label: &str,
    (tally, with_tiled): (&mut Tally, bool),
) -> Result<Option<String>, String> {
    let mut g = pg_grammar::load(&fixture.load_grammar_xml()).map_err(|e| format!("load:{e}"))?;
    settings.apply_to(&mut g);
    let admission = match build_rule_admission(&FomaOptions::default(), &g) {
        Ok(admission) => admission,
        Err(refusal) => return Ok(Some(refusal.reason())),
    };
    let tiled = with_tiled.then(|| build_tiled(&FomaOptions::default(), &g, &admission));
    if let Some(tiled) = &tiled {
        tally.tiled.active |= tiled.active();
        tally.tiled.off = tiled.inactive_reason().map(str::to_owned);
    }
    let morpher = Morpher::new(&g, WORK_CAP);
    let options = ParseOptions::default();
    for entry in words {
        let word = entry.word.as_str();
        let plain = morpher.parse_word_selected(word, &options, None, None);
        tally.words += 1;
        tally.plain_steps += plain.work_steps as u64;
        let admitted = admission.admit(word);
        compare_with_foma(&admission, word, tally);
        let filter = |rule: RuleRef| match rule {
            RuleRef::Stratum(_) | RuleRef::Template(_) => true,
            RuleRef::MRule(id) => admitted[id.0 as usize],
        };
        let narrowed = morpher.parse_word_selected(word, &options, None, Some(&filter));
        tally.confirm_steps += narrowed.work_steps as u64;
        if let Some(tiled) = &tiled {
            let applies = tiled.applies(&admitted, &options);
            let rejected = tiled.rejects(word, &admitted, &options);
            tally.tiled.applied += usize::from(applies);
            tally.tiled.rejected += usize::from(rejected);
            if rejected && !plain.structured.is_empty() {
                tally.tiled.violations.push(format!("{label}{word}"));
            }
        }
        if plain.capped || narrowed.capped {
            tally.capped += 1;
            continue;
        }
        if signature(&plain) != signature(&narrowed) {
            tally.mismatches.push(format!("{label}{word}"));
        }
    }
    Ok(None)
}

fn fixtures(suite: &Path) -> Vec<FixtureRef> {
    let mut out = Vec::new();
    for category in ["edge-cases", "languages"] {
        let Ok(read) = fs::read_dir(suite.join(category)) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = read.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        dirs.sort();
        for dir in dirs {
            if !dir.join("grammar.xml").is_file() || !dir.join("words.yaml").is_file() {
                continue;
            }
            out.push(FixtureRef {
                root: Root::Machine,
                category: category.to_owned(),
                name: dir.file_name().unwrap().to_string_lossy().into_owned(),
                dir,
            });
        }
    }
    out
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let with_tiled = args.iter().any(|arg| arg == "--tiled");
    let args: Vec<String> = args.into_iter().filter(|arg| arg != "--tiled").collect();
    let [suite, output] = args.as_slice() else {
        return Err("usage: admission_conformance <suite-root> <out.jsonl> [--tiled]".into());
    };
    let mut out = fs::File::create(output).map_err(|e| e.to_string())?;
    std::panic::set_hook(Box::new(|_| {}));
    let (mut built, mut refused, mut skipped, mut bad, mut panicked, mut differing) =
        (0, 0, 0, 0, 0, 0);
    let (mut tiled_active, mut tiled_applied, mut tiled_rejected, mut tiled_bad) = (0, 0, 0, 0);
    for fixture in fixtures(Path::new(suite)) {
        let spec = catch_unwind(AssertUnwindSafe(|| fixture.load_words_yaml()));
        let Ok(spec) = spec else {
            panicked += 1;
            continue;
        };
        if spec.expect_crash {
            skipped += 1;
            continue;
        }
        let mut tally = Tally::default();
        let result = catch_unwind(AssertUnwindSafe(|| {
            let base = run(
                &fixture,
                &spec.morpher,
                &spec.words,
                "",
                (&mut tally, with_tiled),
            )?;
            if base.is_some() {
                return Ok(base);
            }
            for variant in &spec.variants {
                let label = format!("{}:", variant.name);
                run(
                    &fixture,
                    &variant.morpher,
                    &variant.words,
                    &label,
                    (&mut tally, with_tiled),
                )?;
            }
            Ok::<_, String>(None)
        }));
        let (refusal, error) = match result {
            Ok(Ok(refusal)) => (refusal, None),
            Ok(Err(error)) => (None, Some(error)),
            Err(_) => {
                panicked += 1;
                (None, Some("panic".to_owned()))
            }
        };
        match (&refusal, &error) {
            (Some(_), _) => refused += 1,
            (None, None) => built += 1,
            _ => {}
        }
        bad += tally.mismatches.len();
        differing += tally.table_differences.len();
        tiled_active += usize::from(tally.tiled.active);
        tiled_applied += tally.tiled.applied;
        tiled_rejected += tally.tiled.rejected;
        tiled_bad += tally.tiled.violations.len();
        let mut row = serde_json::json!({
            "fixture": fixture.label(), "refused": refusal, "error": error,
            "words": tally.words, "mismatches": tally.mismatches, "capped": tally.capped,
            "plain_steps": tally.plain_steps,
            "confirm_steps": tally.confirm_steps, "compared": tally.compared,
            "table_differences": tally.table_differences,
        });
        if with_tiled {
            row["tiled"] = serde_json::json!({
                "active": tally.tiled.active, "inactive_reason": tally.tiled.off,
                "applied": tally.tiled.applied, "rejected": tally.tiled.rejected,
                "rejected_with_analysis": tally.tiled.violations,
            });
        }
        writeln!(out, "{row}").map_err(|e| e.to_string())?;
    }
    println!(
        "built {built} refused {refused} skipped-expect-crash {skipped} panicked {panicked} mismatches {bad} table-differences {differing}"
    );
    if with_tiled {
        println!(
            "tiled: active on {tiled_active} applied to {tiled_applied} words rejected {tiled_rejected} rejected-with-analysis {tiled_bad}"
        );
    }
    Ok(())
}
