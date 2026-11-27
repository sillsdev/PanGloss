//! Adjacent unmodified copies of nullable parts are one capture group: splitting a stretch between them changes no output, so the matcher must not enumerate the splits.

#[path = "csharp_port_common/mod.rs"]
mod csharp_port_common;
use csharp_port_common::build_grammar_cooccurrence;
use pg_grammar_model::model::Grammar;
use pg_parse::morpher::Morpher;

fn part(id: usize, min: u32, max: &str) -> String {
    format!(
        r#"<PhoneticSequence id="{id}"><OptionalSegmentSequence min="{min}" max="{max}"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>"#
    )
}

/// A suffix that copies `parts` adjacent parts (the first non-empty) and appends `k`.
fn suffix_grammar(parts: usize, later_max: &str) -> Grammar {
    let input: String = (1..=parts)
        .map(|i| part(i, u32::from(i == 1), if i == 1 { "-1" } else { later_max }))
        .collect();
    let copies: String = (1..=parts)
        .map(|i| format!(r#"<CopyFromInput index="{i}" />"#))
        .collect();
    let rule = format!(
        r#"<MorphologicalRule id="mrS" requiredPartsOfSpeech="posV" outputPartOfSpeech="posN"><Name>suffix</Name><MorphemeId>SUF</MorphemeId>
          <MorphologicalSubrules><MorphologicalSubrule id="sub1">
            <MorphologicalInput>{input}</MorphologicalInput>
            <MorphologicalOutput>{copies}<InsertSegments><PhoneticShape>k</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>"#
    );
    let root = r#"<LexicalEntry id="eT" partOfSpeech="posV"><MorphemeId>T8</MorphemeId>
        <Allomorphs><Allomorph id="aT"><PhoneticShape>tttttttt</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>"#;
    build_grammar_cooccurrence(&rule, "mrS", root, "")
}

/// The surviving analyses of `word` and the work they took.
fn parse(g: &Grammar, word: &str) -> (Vec<(String, String)>, usize) {
    let out = Morpher::new(g, usize::MAX / 2).parse_word(word);
    assert!(!out.capped && !out.capture_capped);
    (out.analyses, out.work_steps)
}

#[test]
fn many_adjacent_nullable_copies_cost_polynomial_work_and_keep_the_analysis() {
    let few = suffix_grammar(4, "-1");
    let many = suffix_grammar(12, "-1");
    let (few_out, few_work) = parse(&few, "ttttttttk");
    let (many_out, many_work) = parse(&many, "ttttttttk");
    assert_eq!(few_out, many_out, "the stem is the same whatever the split");
    assert_eq!(many_out.len(), 1, "one stem, once: {many_out:?}");
    // Unfolded, twelve groups over eight segments are C(19,11) = 75,582 splits.
    assert!(
        many_work < 50 * few_work.max(1_000),
        "{few_work} -> {many_work}"
    );
    assert!(many_work < 200_000, "work {many_work}");
}

#[test]
fn adjacent_copies_of_non_empty_parts_still_find_the_one_stem() {
    // Parts that cannot be empty fold too; the unique split of "tttttttt" into 8 one-or-more parts is the only analysis.
    let input: String = (1..=8).map(|i| part(i, 1, "-1")).collect();
    let copies: String = (1..=8)
        .map(|i| format!(r#"<CopyFromInput index="{i}" />"#))
        .collect();
    let rule = format!(
        r#"<MorphologicalRule id="mrS" requiredPartsOfSpeech="posV" outputPartOfSpeech="posN"><Name>suffix</Name><MorphemeId>SUF</MorphemeId>
          <MorphologicalSubrules><MorphologicalSubrule id="sub1">
            <MorphologicalInput>{input}</MorphologicalInput>
            <MorphologicalOutput>{copies}<InsertSegments><PhoneticShape>k</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>"#
    );
    let root = r#"<LexicalEntry id="eT" partOfSpeech="posV"><MorphemeId>T8</MorphemeId>
        <Allomorphs><Allomorph id="aT"><PhoneticShape>tttttttt</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>"#;
    let g = build_grammar_cooccurrence(&rule, "mrS", root, "");
    let (out, work) = parse(&g, "ttttttttk");
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(work < 200_000, "work {work}");
}
