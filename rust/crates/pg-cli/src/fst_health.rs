//! `pangloss fst-health <grammar> [<out.json>]` runs the grammar-only characterization pass and
//! writes one canonical `HealthReport`. It never compiles a backend or evaluates a corpus;
//! proposal, confirmation, and duplicate-analysis measurements belong to a separate post-build
//! corpus operation over an explicitly completed artifact.

use std::fs;

use pg_foma::characterization::characterization_findings;
use pg_foma::health::HealthReport;
use pg_grammar::model::Grammar;


/// Builds a report from grammar characterization only; no backend compiler or corpus is run.
fn build_health_report(grammar: &Grammar) -> HealthReport {
    HealthReport::new(characterization_findings(grammar))
}

/// The `admission (per-axis breakdown)` fragment of `run_fst_health`'s completion message.
fn render_admission_summary(report: &HealthReport) -> String {
    format!(
        "{:?} ({})",
        report.admission(),
        report.admission_by_class().render()
    )
}

/// `pangloss fst-health <grammar> [<out.json>]`; `<out.json>` omitted writes the canonical JSON to stdout instead of a file, matching this crate's stdout/stderr split.
pub fn run_fst_health(args: &[String]) -> Result<(), String> {
    let (grammar_path, out_path): (&str, Option<&str>) = match args {
        [g] => (g.as_str(), None),
        [g, o] => (g.as_str(), Some(o.as_str())),
        _ => {
            return Err("usage: fst-health <grammar> [<out.json>]".to_string());
        }
    };

    let (grammar, warnings) = crate::load_grammar(grammar_path)?;
    crate::print_grammar_warnings(&warnings);

    let report = build_health_report(&grammar);
    let json = report
        .to_json()
        .map_err(|e| format!("serialize health report: {e}"))?;

    match out_path {
        Some(path) => {
            fs::write(path, &json).map_err(|e| format!("write {path}: {e}"))?;
        }
        None => println!("{json}"),
    }

    eprintln!(
        "fst-health complete: {} finding(s), admission={} (characterization only)",
        report.findings.len(),
        render_admission_summary(&report),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same clean, `Admit`-verdict shape `pack.rs`'s own `CLEAN_GRAMMAR_XML` uses.
    const CLEAN_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>FstHealthCleanFixture</Name>
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

    fn grammar(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture grammar failed to load: {e}"))
    }

    /// The printed fragment must name all four axes, not just the collapsed severity band.
    #[test]
    fn admission_summary_names_all_four_axes() {
        let g = grammar(CLEAN_GRAMMAR_XML);
        let report = build_health_report(&g);
        let summary = render_admission_summary(&report);
        assert!(summary.contains("representability="), "{summary}");
        assert!(summary.contains("readiness="), "{summary}");
        assert!(summary.contains("containment="), "{summary}");
        assert!(summary.contains("process="), "{summary}");
    }

}
