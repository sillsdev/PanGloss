//! Ad-hoc allomorph prohibitions naming an affix allomorph of an entry with two MSAs must hold on every morphological rule copy, as HCLoader.LoadAllomorphCoOccurrenceRules attaches them (data/adhoc-affix-copies/README.md).

use pg_parse::{Morpher, ParseOptions};

const PROJECT: &str = include_str!("data/adhoc-affix-copies/project.fwdata");
const ENV_PROJECT: &str = include_str!("data/adhoc-affix-environment-copies/project.fwdata");
const STEM_ALLO: &str = "00000000-0000-0000-0000-000000000101";
const PREFIX_ALLO: &str = "00000000-0000-0000-0000-000000000201";

fn prohibition(first: &str, rest: &str) -> String {
    format!(
        "<rt class=\"MoAlloAdhocProhib\" guid=\"00000000-0000-0000-0000-0000000009a1\"><Adjacency val=\"0\" /><FirstAllomorph><objsur guid=\"{first}\" t=\"r\" /></FirstAllomorph><RestOfAllos><objsur guid=\"{rest}\" t=\"r\" /></RestOfAllos></rt>"
    )
}

/// Analyses and work steps for `tada` (prefix `ta-` + stem `da`) and the bare stem `da`.
fn observe(rule: Option<String>) -> ((usize, usize), (usize, usize)) {
    observe_in(PROJECT, rule)
}

fn observe_in(project: &str, rule: Option<String>) -> ((usize, usize), (usize, usize)) {
    let source = match rule {
        Some(rule) => project.replace("</languageproject>", &format!("{rule}</languageproject>")),
        None => project.to_string(),
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.fwdata");
    std::fs::write(&path, source).unwrap();
    let (snapshot, _report) = pg_fwdata::import_file(&path).unwrap();
    let (grammar, _warnings) = pg_grammar::compile_project(&snapshot).unwrap();
    let morpher = Morpher::new(&grammar, usize::MAX);
    let run = |word: &str| {
        let out = morpher.parse_word_opts(word, &ParseOptions::default());
        assert!(!out.invalid_shape && !out.capped && !out.timed_out && !out.guessed);
        (out.structured.len(), out.work_steps)
    };
    (run("tada"), run("da"))
}

#[test]
fn adhoc_allomorph_prohibition_covers_both_msa_copies_of_a_multi_msa_prefix_in_both_directions() {
    let ((base, _), (bare, _)) = observe(None);
    assert!(
        base >= 2,
        "control: tada parses once per prefix MSA copy, got {base}"
    );
    assert!(bare > 0, "control: bare stem parses");
    for (name, first, rest) in [
        ("stem first", STEM_ALLO, PREFIX_ALLO),
        ("prefix first", PREFIX_ALLO, STEM_ALLO),
    ] {
        let ((prefixed, _), (bare_after, _)) = observe(Some(prohibition(first, rest)));
        assert_eq!(
            prefixed, 0,
            "{name}: prohibition must hold on every prefix MSA copy"
        );
        assert_eq!(bare_after, bare, "{name}: the bare stem is unaffected");
    }
}

#[test]
fn adhoc_allomorph_prohibition_covers_every_environment_pass_of_an_affix_allomorph_in_both_directions(
) {
    let ((base, _), (bare, _)) = observe_in(ENV_PROJECT, None);
    assert!(
        base >= 2,
        "control: tada parses once per prefix MSA copy through the second environment, got {base}"
    );
    for (name, first, rest) in [
        ("stem first", STEM_ALLO, PREFIX_ALLO),
        ("prefix first", PREFIX_ALLO, STEM_ALLO),
    ] {
        let ((prefixed, _), (bare_after, _)) =
            observe_in(ENV_PROJECT, Some(prohibition(first, rest)));
        assert_eq!(
            prefixed, 0,
            "{name}: prohibition must hold on every environment pass"
        );
        assert_eq!(bare_after, bare, "{name}: the bare stem is unaffected");
    }
}
