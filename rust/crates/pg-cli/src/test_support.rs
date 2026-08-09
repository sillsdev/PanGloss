use std::borrow::Cow;

pub(crate) fn normalize_newlines(input: &str) -> Cow<'_, str> {
    if !input.contains('\r') {
        return Cow::Borrowed(input);
    }

    let mut normalized = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            chars.next_if_eq(&'\n');
            normalized.push('\n');
        } else {
            normalized.push(ch);
        }
    }
    Cow::Owned(normalized)
}

fn first_mismatch(actual: &str, expected: &str) -> Option<usize> {
    let actual_chars: Vec<char> = actual.chars().collect();
    let expected_chars: Vec<char> = expected.chars().collect();
    let shared_len = actual_chars.len().min(expected_chars.len());
    for index in 0..shared_len {
        if actual_chars[index] != expected_chars[index] {
            return Some(index);
        }
    }
    (actual_chars.len() != expected_chars.len()).then_some(shared_len)
}

fn line_column(input: &str, char_index: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for ch in input.chars().take(char_index) {
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn escaped_context(input: &str, char_index: usize) -> String {
    let chars: Vec<char> = input.chars().collect();
    if char_index >= chars.len() {
        return "<EOF>".to_string();
    }
    let start = char_index.saturating_sub(20);
    let end = (char_index + 20).min(chars.len());
    chars[start..end]
        .iter()
        .collect::<String>()
        .escape_default()
        .collect()
}

fn trailing_newline_count(input: &str) -> usize {
    input.chars().rev().take_while(|&ch| ch == '\n').count()
}

fn text_mismatch_message(actual: &str, expected: &str) -> String {
    let index = first_mismatch(actual, expected).expect("a mismatch is required");
    let (line, column) = line_column(actual, index);
    let at_eof = index >= actual.chars().count() || index >= expected.chars().count();
    let eof = if at_eof { " (EOF)" } else { "" };
    let trailing = if trailing_newline_count(actual) != trailing_newline_count(expected) {
        "; trailing newline difference"
    } else {
        ""
    };
    format!(
        "rendered text mismatch at line {line}, column {column}{eof}{trailing}; \
         actual context: {:?}; expected context: {:?}",
        escaped_context(actual, index),
        escaped_context(expected, index),
    )
}

#[track_caller]
pub(crate) fn assert_rendered_text_eq(actual: &str, expected: &str) {
    let actual_normalized = normalize_newlines(actual);
    let expected_normalized = normalize_newlines(expected);
    if actual_normalized != expected_normalized {
        panic!(
            "{}",
            text_mismatch_message(actual_normalized.as_ref(), expected_normalized.as_ref())
        );
    }
}

/// Synthetic: true reduplication (one input part copied twice) on a `RealizationalRule`, which the peel cannot propose, so the gated backend declines it while the emitter still compiles a network -- a refusal an `--allow-unproven` run can still produce output under.
/// Pinned by `capability_gate_enforce_refuses_permanently_refused_without_override`.
pub(crate) const BACKEND_REFUSED_GRAMMAR_XML: &str = r#"<HermitCrabInput><Language><Name>BackendRefusedFixture</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRules="rrRedup">
      <Name>S</Name>
      <MorphologicalRuleDefinitions>
        <RealizationalRule id="rrRedup">
          <Name>redup</Name>
          <MorphologicalSubrules>
            <MorphologicalSubrule id="subRedup">
              <MorphologicalInput>
                <PhoneticSequence id="qA"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
              </MorphologicalInput>
              <MorphologicalOutput redupMorphType="suffix">
                <CopyFromInput index="qA" />
                <CopyFromInput index="qA" />
              </MorphologicalOutput>
            </MorphologicalSubrule>
          </MorphologicalSubrules>
          <MorphemeId>RED</MorphemeId>
        </RealizationalRule>
      </MorphologicalRuleDefinitions>
      <LexicalEntries>
        <LexicalEntry id="e1">
          <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          <MorphemeId>A</MorphemeId>
        </LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

/// Synthetic: an `Unordered` stratum of `rule_count` loose rules, generated so "over the calibrated ordering-multiplicity budget" is arithmetic rather than a comment; the shape a gate exists to catch before the compiler does.
/// Pinned by `a_grammar_only_the_gated_backend_refuses_is_still_blocked_at_the_gate`.
pub(crate) fn unordered_over_budget_grammar_xml(rule_count: u32) -> String {
    let mut rules = String::new();
    let mut segments = String::new();
    for i in 0..rule_count {
        segments.push_str(&format!(
            r#"<SegmentDefinition id="cx{i}"><Representations><Representation>x{i}</Representation></Representations></SegmentDefinition>"#
        ));
        rules.push_str(&format!(
            r#"<MorphologicalRule id="mr{i}" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
                 <Name>r{i}</Name>
                 <MorphologicalSubrules>
                   <MorphologicalSubrule id="sub{i}">
                     <MorphologicalInput><PhoneticSequence id="stem{i}"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                     <MorphologicalOutput><CopyFromInput index="stem{i}" /><InsertSegments><PhoneticShape>x{i}</PhoneticShape></InsertSegments></MorphologicalOutput>
                   </MorphologicalSubrule>
                 </MorphologicalSubrules>
                 <MorphemeId>R{i}</MorphemeId>
               </MorphologicalRule>"#
        ));
    }
    let rule_ids: Vec<String> = (0..rule_count).map(|i| format!("mr{i}")).collect();
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>UnorderedOverBudgetFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        {segments}
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="{rule_ids}">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>{rules}</MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#,
        rule_ids = rule_ids.join(" "),
    )
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::*;

    fn panic_message(result: std::thread::Result<()>) -> String {
        let payload: Box<dyn Any + Send> = result.expect_err("the assertion must panic");
        match payload.downcast::<String>() {
            Ok(message) => *message,
            Err(payload) => match payload.downcast::<&'static str>() {
                Ok(message) => (*message).to_string(),
                Err(_) => "non-string panic payload".to_string(),
            },
        }
    }

    #[test]
    fn rendered_text_helper_accepts_crlf_and_lone_cr() {
        assert_rendered_text_eq("first\r\nsecond\rthird", "first\nsecond\nthird");
    }

    #[test]
    fn rendered_text_helper_preserves_content_and_reports_diagnostics() {
        assert_rendered_text_eq(
            "\u{feff} node-π\tvalue\u{0085}\u{2028}\u{2029}\n",
            "\u{feff} node-π\tvalue\u{0085}\u{2028}\u{2029}\r\n",
        );

        let content = panic_message(std::panic::catch_unwind(|| {
            assert_rendered_text_eq("header\nvalue\0X", "header\nvalue\0Y");
        }));
        assert!(content.contains("line 2, column 7"), "{content}");
        assert!(content.contains("\\u{0}"), "{content}");
        assert!(content.contains("actual context"), "{content}");
        assert!(content.contains("expected context"), "{content}");

        let trailing = panic_message(std::panic::catch_unwind(|| {
            assert_rendered_text_eq("same\n", "same");
        }));
        assert!(trailing.contains("EOF"), "{trailing}");
        assert!(trailing.contains("trailing newline"), "{trailing}");

        let identifier = panic_message(std::panic::catch_unwind(|| {
            assert_rendered_text_eq("node-A", "node-B");
        }));
        assert!(identifier.contains("line 1, column 6"), "{identifier}");
    }

    #[test]
    fn rendered_text_helper_rejects_whitespace_and_unicode_drift() {
        let whitespace = panic_message(std::panic::catch_unwind(|| {
            assert_rendered_text_eq("value\tA", "value A");
        }));
        assert!(
            whitespace.contains("rendered text mismatch"),
            "{whitespace}"
        );

        let unicode = panic_message(std::panic::catch_unwind(|| {
            assert_rendered_text_eq("naïve", "naive");
        }));
        assert!(unicode.contains("rendered text mismatch"), "{unicode}");
    }
}
