//! Checks over the conformance suite that the admitted restricted parse keeps every analysis of the plain parse.
use foma::options::FomaOptions;
use pg_conformance_fixtures::{FixtureRef, Root, WordEntry};
use pg_foma::admission::build_rule_admission;
use pg_parse::{Morpher, ParseOptions, ParseOutcome};
use pg_rules::stratum::RuleRef;
use std::fs;
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

const WORK_CAP: usize = 2_000_000;
const PATH_ALLOWANCE: usize = 4096;

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
    fallbacks: usize,
    mismatches: Vec<String>,
    plain_steps: u64,
    confirm_steps: u64,
}

fn run(
    fixture: &FixtureRef,
    settings: &pg_conformance_fixtures::MorpherSettings,
    words: &[WordEntry],
    label: &str,
    tally: &mut Tally,
) -> Result<Option<String>, String> {
    let mut g = pg_grammar::load(&fixture.load_grammar_xml()).map_err(|e| format!("load:{e}"))?;
    settings.apply_to(&mut g);
    let admission = match build_rule_admission(&FomaOptions::default(), &g) {
        Ok(admission) => admission,
        Err(refusal) => return Ok(Some(refusal.reason())),
    };
    let morpher = Morpher::new(&g, WORK_CAP);
    for entry in words {
        let word = entry.word.as_str();
        let plain = morpher.parse_word_selected(word, &ParseOptions::default(), None, None);
        tally.words += 1;
        tally.plain_steps += plain.work_steps as u64;
        let Some(admitted) = admission.admit(word, PATH_ALLOWANCE) else {
            tally.fallbacks += 1;
            continue;
        };
        let filter = |rule: RuleRef| match rule {
            RuleRef::Stratum(_) | RuleRef::Template(_) => true,
            RuleRef::MRule(id) => admitted[id.0 as usize],
        };
        let narrowed =
            morpher.parse_word_selected(word, &ParseOptions::default(), None, Some(&filter));
        tally.confirm_steps += narrowed.work_steps as u64;
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
    let [suite, output] = args.as_slice() else {
        return Err("usage: admission_conformance <suite-root> <out.jsonl>".into());
    };
    let mut out = fs::File::create(output).map_err(|e| e.to_string())?;
    std::panic::set_hook(Box::new(|_| {}));
    let (mut built, mut refused, mut skipped, mut bad, mut panicked) = (0, 0, 0, 0, 0);
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
            let base = run(&fixture, &spec.morpher, &spec.words, "", &mut tally)?;
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
                    &mut tally,
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
        let row = serde_json::json!({
            "fixture": fixture.label(), "refused": refusal, "error": error,
            "words": tally.words, "mismatches": tally.mismatches, "capped": tally.capped,
            "fallbacks": tally.fallbacks, "plain_steps": tally.plain_steps,
            "confirm_steps": tally.confirm_steps,
        });
        writeln!(out, "{row}").map_err(|e| e.to_string())?;
    }
    println!(
        "built {built} refused {refused} skipped-expect-crash {skipped} panicked {panicked} mismatches {bad}"
    );
    Ok(())
}
