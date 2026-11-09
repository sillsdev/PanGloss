use pg_analyzer::{analyze, ProfileInput, StyledRun};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct Case {
    name: String,
    text: String,
    ldml: String,
    additions: String,
    styles: Vec<StyledRun>,
}
#[derive(Deserialize)]
struct Reference {
    liblcm_revision: String,
    machine_revision: String,
    icu_version: String,
    unicode_version: String,
    cases: Vec<ReferenceCase>,
}
#[derive(Deserialize)]
struct ReferenceCase {
    name: String,
    profile_sha256: String,
    run_profile_hashes: Vec<String>,
    words: Vec<Slice>,
    machine: Vec<Slice>,
}
#[derive(Deserialize, PartialEq, Eq)]
struct Slice {
    text: String,
    start: usize,
    end: usize,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: parity cases.json oracle.json (missing oracle is an error)".into());
    }
    let cases: Vec<Case> = serde_json::from_str(&std::fs::read_to_string(&args[1])?)?;
    let reference: Reference = serde_json::from_str(&std::fs::read_to_string(&args[2])?)?;
    if cases.is_empty()
        || reference.liblcm_revision != "d564a719b1cce16c25ebea53a537393cb757f5d1"
        || reference.machine_revision != "4d9b454d42a0da35e058641a65596339a79d115b"
        || reference.icu_version != "70.1"
        || reference.unicode_version != "14.0"
    {
        return Err("missing or incompatible oracle provenance".into());
    }
    let reference_count = reference.cases.len();
    let mut refs: HashMap<_, _> = reference
        .cases
        .into_iter()
        .map(|r| (r.name.clone(), r))
        .collect();
    let total = cases.len();
    if refs.len() != reference_count || reference_count != total {
        return Err("duplicate, missing, or extra oracle cases".into());
    }
    let (mut matches, mut machine_matches) = (0, 0);
    for case in cases {
        let actual = analyze(
            &case.text,
            ProfileInput::Ldml {
                identity: &case.name,
                xml: &case.ldml,
            },
            &case.styles,
        )?;
        let r = refs.remove(&case.name).ok_or("missing oracle case")?;
        if r.profile_sha256 != actual.profile.source_sha256 {
            return Err("profile hash mismatch".into());
        }
        let run_hashes: Vec<_> = case
            .styles
            .iter()
            .filter_map(|s| s.profile.as_ref().map(|p| p.source_sha256.clone()))
            .collect();
        if run_hashes != r.run_profile_hashes {
            return Err("run profile hash mismatch".into());
        }
        let mut expected_chars: Vec<char> = case.additions.chars().collect();
        expected_chars.sort();
        expected_chars.dedup();
        if actual.profile.word_forming != expected_chars {
            return Err("oracle additions differ from LDML profile".into());
        }
        let slices: Vec<Slice> = actual
            .occurrences
            .into_iter()
            .map(|o| Slice {
                text: o.text,
                start: o.utf16[0],
                end: o.utf16[1],
            })
            .collect();
        if slices == r.words {
            matches += 1;
        } else {
            eprintln!("LibLCM mismatch: {}", case.name);
        }
        if slices == r.machine {
            machine_matches += 1;
        }
    }
    if !refs.is_empty() {
        return Err("extra oracle cases".into());
    }
    println!(
        "LibLCM {}: {matches}/{total} ({:.1}%); Machine: {machine_matches}/{total} ({:.1}%)",
        reference.liblcm_revision,
        matches as f64 * 100. / total as f64,
        machine_matches as f64 * 100. / total as f64
    );
    if matches != total {
        return Err("LibLCM parity failure".into());
    }
    Ok(())
}
