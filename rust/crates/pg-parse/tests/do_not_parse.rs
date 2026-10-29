//! FieldWorks entry exclusion through import, grammar compilation, and parsing.

use pg_parse::{Morpher, ParseOptions};

const PROJECT: &str = include_str!("data/merge-msas/project.fwdata");
const ENTRY: &str = "00000000-0000-0000-0000-000000000040";
const SENSE: &str = "00000000-0000-0000-0000-000000000043";

fn added_entry(id: &str, form: &str, component: Option<(&str, bool)>) -> String {
    let entry = format!("10000000-0000-0000-0000-{id:0>12}");
    let allo = format!("20000000-0000-0000-0000-{id:0>12}");
    let msa = format!("30000000-0000-0000-0000-{id:0>12}");
    let sense = format!("40000000-0000-0000-0000-{id:0>12}");
    let reference = format!("50000000-0000-0000-0000-{id:0>12}");
    let own_analysis = component.is_none_or(|(_, complex)| complex);
    let analysis = if own_analysis {
        format!("<MorphoSyntaxAnalyses><objsur guid=\"{msa}\" t=\"o\" /></MorphoSyntaxAnalyses><Senses><objsur guid=\"{sense}\" t=\"o\" /></Senses>")
    } else {
        String::new()
    };
    let refs = component.map_or_else(String::new, |(target, complex)| {
        let kind = if complex { "<ComplexEntryTypes><objsur guid=\"60000000-0000-0000-0000-000000000001\" t=\"r\" /></ComplexEntryTypes>" } else { "" };
        format!("<rt class=\"LexEntryRef\" guid=\"{reference}\" ownerguid=\"{entry}\"><ComponentLexemes><objsur guid=\"{target}\" t=\"r\" /></ComponentLexemes>{kind}</rt>")
    });
    let entry_refs = if component.is_some() {
        format!("<EntryRefs><objsur guid=\"{reference}\" t=\"o\" /></EntryRefs>")
    } else {
        String::new()
    };
    format!(
        r#"<rt class="LexEntry" guid="{entry}"><LexemeForm><objsur guid="{allo}" t="o" /></LexemeForm>{analysis}{entry_refs}</rt>
<rt class="MoStemAllomorph" guid="{allo}" ownerguid="{entry}"><Form><AUni ws="fx">{form}</AUni></Form><MorphType><objsur guid="d7f713e5-e8cf-11d3-9764-00c04f186933" t="r" /></MorphType></rt>
<rt class="MoStemMsa" guid="{msa}" ownerguid="{entry}"><PartOfSpeech><objsur guid="00000000-0000-0000-0000-00000000000c" t="r" /></PartOfSpeech></rt>
<rt class="LexSense" guid="{sense}" ownerguid="{entry}"><MorphoSyntaxAnalysis><objsur guid="{msa}" t="r" /></MorphoSyntaxAnalysis></rt>{refs}"#
    )
}

fn observe(flag: Option<bool>) -> (usize, usize, Vec<usize>) {
    let anchor = format!("<rt class=\"LexEntry\" guid=\"{ENTRY}\">");
    let mut source = PROJECT.replacen(&anchor, &format!("{anchor}<AlternateForms><objsur guid=\"70000000-0000-0000-0000-000000000001\" t=\"o\" /></AlternateForms>"), 1);
    let alternate = r#"<rt class="MoStemAllomorph" guid="70000000-0000-0000-0000-000000000001" ownerguid="00000000-0000-0000-0000-000000000040"><Form><AUni ws="fx">rann</AUni></Form><MorphType><objsur guid="d7f713e5-e8cf-11d3-9764-00c04f186933" t="r" /></MorphType></rt>"#;
    source = source.replace(
        "</languageproject>",
        &format!("{alternate}</languageproject>"),
    );
    if let Some(value) = flag {
        source = source.replacen(
            &anchor,
            &format!("{anchor}<DoNotUseForParsing val=\"{value}\" />"),
            1,
        );
    }
    let added = [
        added_entry("1", "ran", None),
        added_entry("2", "ar", Some((ENTRY, false))),
        added_entry("3", "an", Some((SENSE, false))),
        added_entry("4", "nana", Some((ENTRY, true))),
    ]
    .join("\n");
    source = source.replace("</languageproject>", &format!("{added}</languageproject>"));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.fwdata");
    std::fs::write(&path, source).unwrap();
    let (snapshot, _, delta) = pg_fwdata::import_file_measured(&path).unwrap();
    if flag == Some(true) {
        for id in ["2", "3", "4"] {
            let guid = format!("10000000-0000-0000-0000-{id:0>12}");
            let entry = snapshot
                .lexicon
                .entries
                .iter()
                .find(|entry| entry.guid == guid)
                .unwrap();
            let components = match &entry.entry_refs[0] {
                pg_snapshot::EntryRef::Variant {
                    component_lexemes, ..
                }
                | pg_snapshot::EntryRef::ComplexForm {
                    component_lexemes, ..
                } => component_lexemes,
            };
            assert!(
                components.is_empty(),
                "all components of {guid} must be pruned"
            );
            assert!(
                !snapshot
                    .validate()
                    .iter()
                    .any(|warning| warning.message.contains(&guid)),
                "empty reference on {guid} must validate"
            );
        }
    }
    assert!(delta.silently_omitted.is_empty());
    let excluded = snapshot
        .conversion_provenance
        .import_load_decisions
        .iter()
        .filter(|d| d.reason_code.as_str() == "doNotUseForParsing")
        .count();
    let (grammar, warnings) = pg_grammar::compile_project(&snapshot).unwrap();
    assert!(!format!("{warnings:?}").contains("does not resolve to an entry or sense"));
    let morpher = Morpher::new(&grammar, usize::MAX);
    let counts = ["ranna", "ran", "ar", "an", "nana", "rann"]
        .iter()
        .map(|word| {
            let out = morpher.parse_word_opts(word, &ParseOptions::default());
            assert!(
                !out.invalid_shape && !out.capped && !out.timed_out && !out.guessed,
                "{word}: incomplete, invalid, or guessed outcome"
            );
            out.structured.len()
        })
        .collect();
    (snapshot.lexicon.entries.len(), excluded, counts)
}

#[test]
fn do_not_parse_excludes_entry_and_variant_borrowing() {
    let absent = observe(None);
    let disabled = observe(Some(false));
    let enabled = observe(Some(true));
    assert_eq!(absent, disabled);
    assert_eq!((absent.0, absent.1), (5, 0));
    assert_eq!((enabled.0, enabled.1), (4, 1));
    assert!(absent.2.iter().all(|n| *n > 0), "baseline: {:?}", absent.2);
    assert_eq!(enabled.2[0], 0, "excluded main entry");
    assert_eq!(enabled.2[5], 0, "excluded alternate allomorph");
    assert_eq!(enabled.2[2], 0, "variant of excluded entry");
    assert_eq!(enabled.2[3], 0, "variant of excluded sense");
    assert_eq!(enabled.2[1], absent.2[1], "unflagged neighbor");
    assert_eq!(
        enabled.2[4], absent.2[4],
        "complex form with its own analysis"
    );
}
