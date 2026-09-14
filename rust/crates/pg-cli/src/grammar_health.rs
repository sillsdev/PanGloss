//! `pangloss grammar-health <grammar> [<out.json>]`: run the ported `hc-*` HermitCrab
//! grammar-authoring checks (`pg_grammar::grammar_health`) and print/serialize the findings.
//!
//! Deliberately a SEPARATE command from `fst-health`, not a section added to it: the two answer
//! different questions (grammar authoring correctness vs. FST compilation/production readiness),
//! and `fst-health`'s JSON is a versioned wire shape (`pg_health::HEALTH_SCHEMA_VERSION`) read by
//! `pg-pack`/`pg-wasm` -- folding a second vocabulary into it would need a version bump for a
//! question those readers never asked. This command's own output is a bare JSON array of findings,
//! mirroring the C# checker's `IList<GrammarHealthCheckFinding>` return shape exactly.
//!
//! Diagnostic only: this command always exits 0, even when findings are reported. It is not a
//! build gate -- the caller decides what to do with the findings.

use std::fs;

use pg_grammar::grammar_health::{
    check_grammar_health, GrammarHealthCheckFinding, GrammarHealthSeverity,
};

/// `pangloss grammar-health <grammar> [<out.json>]`; `<out.json>` omitted prints the findings as a
/// JSON array to stdout instead of a file.
pub fn run_grammar_health(args: &[String]) -> Result<(), String> {
    let (grammar_path, out_path): (&str, Option<&str>) = match args {
        [g] => (g.as_str(), None),
        [g, o] => (g.as_str(), Some(o.as_str())),
        _ => {
            return Err("usage: grammar-health <grammar> [<out.json>]".to_string());
        }
    };

    let (grammar, warnings) = crate::load_grammar(grammar_path)?;
    crate::print_grammar_warnings(&warnings);

    let findings = check_grammar_health(&grammar);
    let json = serde_json::to_string_pretty(&findings)
        .map_err(|e| format!("serialize grammar health findings: {e}"))?;

    match out_path {
        Some(path) => {
            fs::write(path, &json).map_err(|e| format!("write {path}: {e}"))?;
        }
        None => println!("{json}"),
    }

    eprintln!(
        "grammar-health complete: {} finding(s) ({})",
        findings.len(),
        render_severity_counts(&findings),
    );
    Ok(())
}

/// The `N error(s), M warning(s)` fragment of `run_grammar_health`'s completion message.
fn render_severity_counts(findings: &[GrammarHealthCheckFinding]) -> String {
    let errors = findings
        .iter()
        .filter(|f| f.severity == GrammarHealthSeverity::Error)
        .count();
    let warnings = findings.len() - errors;
    format!("{errors} error(s), {warnings} warning(s)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_grammar::model::Grammar;

    /// Same clean, zero-findings shape `fst_health.rs`'s own fixture uses.
    const CLEAN_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>GrammarHealthCleanFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="e1">
            <Allomorphs><Allomorph id="e1-1"><PhoneticShape>kat</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kat</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    const PARTIAL_ENTRY_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>GrammarHealthPartialFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="entry1" partial="true">
            <Allomorphs><Allomorph id="e1-1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    fn grammar(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture grammar failed to load: {e}"))
    }

    #[test]
    fn clean_grammar_serializes_to_an_empty_json_array() {
        let g = grammar(CLEAN_GRAMMAR_XML);
        let findings = check_grammar_health(&g);
        assert!(findings.is_empty());
        let json = serde_json::to_string_pretty(&findings).expect("empty findings serialize");
        assert_eq!(json, "[]");
        assert_eq!(
            render_severity_counts(&findings),
            "0 error(s), 0 warning(s)"
        );
    }

    #[test]
    fn partial_entry_grammar_reports_one_warning_naming_its_code() {
        let g = grammar(PARTIAL_ENTRY_GRAMMAR_XML);
        let findings = check_grammar_health(&g);
        assert_eq!(findings.len(), 1);
        let json = serde_json::to_string(&findings).expect("findings serialize");
        assert!(json.contains("hc-partial-morpheme"));
        assert_eq!(
            render_severity_counts(&findings),
            "0 error(s), 1 warning(s)"
        );
    }
}
