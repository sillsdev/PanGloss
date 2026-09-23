//! `pangloss grammar-health <grammar> [<out.json>] [--fw-project <project>] [--log-guids]`: run
//! the ported `hc-*` HermitCrab grammar-authoring checks (`pg_grammar::grammar_health`) and
//! print/serialize the findings.
//!
//! Deliberately a SEPARATE command from `fst-health`, not a section added to it: the two answer
//! different questions (grammar authoring correctness vs. FST compilation/production readiness),
//! and `fst-health`'s JSON is a versioned wire shape (`pg_health::HEALTH_SCHEMA_VERSION`) read by
//! `pg-pack`/`pg-wasm` -- folding a second vocabulary into it would need a version bump for a
//! question those readers never asked. This command emits one versioned structured report and a
//! lossless plain-text log.

use std::fs;

use pg_grammar::grammar_health::{
    check_grammar_health, render_json, render_log, GrammarHealthCheckFinding, GrammarHealthSeverity,
};

/// `pangloss grammar-health <grammar> [<out.json>] [--fw-project <project>] [--log-guids]`;
/// `<out.json>` omitted prints the versioned report to stdout instead of a file. Findings are
/// also logged one per line on stderr.
pub fn run_grammar_health(args: &[String]) -> Result<(), String> {
    let mut positionals = Vec::new();
    let mut fieldworks_project = None;
    let mut log_guids = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--log-guids" => {
                log_guids = true;
                index += 1;
            }
            "--fw-project" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--fw-project requires a project name".to_string())?;
                if value.starts_with("--") || value.trim().is_empty() {
                    return Err("--fw-project requires a nonempty project name".to_string());
                }
                fieldworks_project = Some(value.as_str());
                index += 2;
            }
            arg if arg.starts_with("--fw-project=") => {
                let value = arg.trim_start_matches("--fw-project=");
                if value.trim().is_empty() {
                    return Err("--fw-project requires a nonempty project name".to_string());
                }
                fieldworks_project = Some(value);
                index += 1;
            }
            arg if arg.starts_with("--") => {
                return Err(format!("unknown option: {arg}"));
            }
            arg => {
                positionals.push(arg);
                index += 1;
            }
        }
    }
    let (grammar_path, out_path) = match positionals.as_slice() {
        [grammar] => (*grammar, None),
        [grammar, output] => (*grammar, Some(*output)),
        _ => {
            return Err(
                "usage: grammar-health <grammar> [<out.json>] [--fw-project <project>] [--log-guids]"
                    .to_string(),
            );
        }
    };

    let (grammar, warnings) = crate::load_grammar(grammar_path)?;
    crate::print_grammar_warnings(&warnings);

    let report = check_grammar_health(&grammar, fieldworks_project)
        .map_err(|error| format!("run grammar health checks: {error}"))?;
    let json =
        render_json(&report).map_err(|e| format!("serialize grammar health findings: {e}"))?;

    match out_path {
        Some(path) => {
            fs::write(path, &json).map_err(|e| format!("write {path}: {e}"))?;
        }
        None => println!("{json}"),
    }

    let log = render_log(&report, log_guids);
    if !log.is_empty() {
        eprintln!("{log}");
    }
    eprintln!(
        "grammar-health complete: {} finding(s) ({})",
        report.len(),
        render_severity_counts(report.findings()),
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
    fn clean_grammar_serializes_to_an_empty_versioned_report() {
        let g = grammar(CLEAN_GRAMMAR_XML);
        let report = check_grammar_health(&g, None).expect("clean grammar checks");
        assert!(report.is_empty());
        let json = render_json(&report).expect("empty findings serialize");
        let report_value: serde_json::Value =
            serde_json::from_str(&json).expect("versioned report");
        assert_eq!(report_value["schema_version"], 1);
        assert!(report_value["findings"].is_array());
        assert_eq!(render_severity_counts(&report), "0 error(s), 0 warning(s)");
    }

    #[test]
    fn json_is_versioned_by_default() {
        let g = grammar(CLEAN_GRAMMAR_XML);
        let report = check_grammar_health(&g, None).expect("clean grammar checks");
        let json = render_json(&report).expect("structured findings serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("structured JSON");
        assert_eq!(value["schema_version"], 1);
        assert!(value["findings"].is_array());
    }

    #[test]
    fn command_emits_versioned_json_by_default() {
        let grammar_path = std::env::temp_dir().join(format!(
            "pangloss-grammar-health-{}-input.xml",
            std::process::id()
        ));
        let output_path = grammar_path.with_extension("json");
        fs::write(&grammar_path, PARTIAL_ENTRY_GRAMMAR_XML).expect("write grammar fixture");

        run_grammar_health(&[
            grammar_path.to_string_lossy().into_owned(),
            output_path.to_string_lossy().into_owned(),
        ])
        .expect("grammar-health command");
        let report: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&output_path).expect("read output"))
                .expect("structured JSON");
        assert_eq!(report["schema_version"], 1);
        assert!(report["findings"].is_array());
        assert_eq!(report["findings"].as_array().unwrap().len(), 1);

        let _ = fs::remove_file(grammar_path);
        let _ = fs::remove_file(output_path);
    }

    #[test]
    fn partial_entry_grammar_reports_one_warning_naming_its_code() {
        let g = grammar(PARTIAL_ENTRY_GRAMMAR_XML);
        let report = check_grammar_health(&g, None).expect("partial grammar checks");
        assert_eq!(report.len(), 1);
        let json = render_json(&report).expect("findings serialize");
        assert!(json.contains("hc-partial-morpheme"));
        assert_eq!(
            render_severity_counts(report.findings()),
            "0 error(s), 1 warning(s)"
        );
    }
}
