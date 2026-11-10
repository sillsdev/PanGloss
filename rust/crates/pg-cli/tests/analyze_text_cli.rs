use std::path::{Path, PathBuf};
use std::process::Command;

use pg_analyzer::{
    AnalyzeOptions, LanguagePack, ProfileInput, TextAnalysis, TextInput, TextRequest,
};
use pg_parse::Morpher;

const LDML: &str =
    "<ldml><characters><exemplarCharacters>[a-z]</exemplarCharacters></characters></ldml>";

fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("analyze-text-cli-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cli(args: &[&std::ffi::OsStr]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .arg("analyze-text")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn cli_json_equals_the_serialised_library_result() {
    let dir = scratch("roundtrip");
    let ldml = dir.join("qaa.ldml");
    let one = dir.join("one.txt");
    let two = dir.join("two.txt");
    std::fs::write(&ldml, LDML).unwrap();
    std::fs::write(&one, "kibes vem, zozozo kibes\n").unwrap();
    std::fs::write(&two, "kib qqqq").unwrap();
    let grammar = data("trace-family.xml");
    let pack = format!("xx={}", grammar.display());

    let output = cli(&[
        "--profile".as_ref(),
        ldml.as_os_str(),
        "--pack".as_ref(),
        pack.as_ref(),
        one.as_os_str(),
        two.as_os_str(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let from_cli: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let xml = std::fs::read_to_string(&grammar).unwrap();
    let loaded = pg_grammar::load(&xml).unwrap();
    let morpher = Morpher::new(&loaded, 5_000_000_000).with_guess_roots(false);
    let sha = {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(xml.as_bytes()))
    };
    let packs = [LanguagePack {
        language: "xx".into(),
        source_id: grammar.display().to_string(),
        source_sha256: sha,
        morpher: &morpher,
    }];
    let ldml_text = std::fs::read_to_string(&ldml).unwrap();
    let texts = [
        TextInput {
            id: one.display().to_string(),
            text: "kibes vem, zozozo kibes\n",
            styles: &[],
        },
        TextInput {
            id: two.display().to_string(),
            text: "kib qqqq",
            styles: &[],
        },
    ];
    let direct = pg_analyzer::analyze_text(&TextRequest {
        profile: ProfileInput::Ldml {
            identity: &ldml.display().to_string(),
            xml: &ldml_text,
        },
        packs: &packs,
        texts: &texts,
        options: AnalyzeOptions::default(),
    })
    .unwrap();

    assert_eq!(from_cli, serde_json::to_value(&direct).unwrap());
    let parsed: TextAnalysis = serde_json::from_value(from_cli).unwrap();
    assert_eq!(parsed, direct);
    assert_eq!(parsed.corpus.valid.occurrences, 4);
    assert_eq!(parsed.corpus.incomplete.occurrences, 1);
}

#[test]
fn cli_refuses_missing_profile_and_unknown_flags() {
    let missing = cli(&["x.txt".as_ref()]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("usage: analyze-text"));
    let unknown = cli(&["--nope".as_ref()]);
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("unknown option"));
}
