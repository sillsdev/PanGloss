//! Ad-hoc allomorph prohibitions over a stem with two stem MSAs must hold on every copy, as HCLoader.LoadAllomorphCoOccurrenceRules attaches them (data/adhoc-allomorph-copies/README.md).

use pg_parse::{Morpher, ParseOptions};

const PROJECT: &str = include_str!("data/adhoc-allomorph-copies/project.fwdata");
const STEM_ALLO: &str = "00000000-0000-0000-0000-000000000101";
const PREFIX_ALLO: &str = "00000000-0000-0000-0000-000000000201";

fn prohibition(first: &str, rest: &str) -> String {
    format!(
        "<rt class=\"MoAlloAdhocProhib\" guid=\"00000000-0000-0000-0000-0000000009a1\"><Adjacency val=\"0\" /><FirstAllomorph><objsur guid=\"{first}\" t=\"r\" /></FirstAllomorph><RestOfAllos><objsur guid=\"{rest}\" t=\"r\" /></RestOfAllos></rt>"
    )
}

/// Analyses and work steps for `tada` (prefix `ta-` + stem `da`) and the bare stem `da`.
fn observe(rule: Option<String>) -> ((usize, usize), (usize, usize)) {
    let source = match rule {
        Some(rule) => PROJECT.replace("</languageproject>", &format!("{rule}</languageproject>")),
        None => PROJECT.to_string(),
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
fn adhoc_allomorph_prohibition_blocks_the_verb_prefix_on_the_verb_msa_copy_in_both_directions() {
    let ((base, _), (bare, _)) = observe(None);
    assert!(
        base > 0,
        "control: Verb-only prefix + Verb-first stem parses"
    );
    assert!(bare > 0, "control: bare stem parses");
    for (name, first, rest) in [
        ("stem first", STEM_ALLO, PREFIX_ALLO),
        ("prefix first", PREFIX_ALLO, STEM_ALLO),
    ] {
        let ((prefixed, _), (bare_after, _)) = observe(Some(prohibition(first, rest)));
        assert_eq!(
            prefixed, 0,
            "{name}: prohibition must hold on every MSA copy"
        );
        assert_eq!(bare_after, bare, "{name}: the bare stem is unaffected");
    }
}
