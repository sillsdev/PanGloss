//! `pangloss plan-diagram <grammar> [--json] [--full] [--threshold=N] [<out>]` renders
//! `pg_foma::plan_diagram`'s versioned JSON projection of a grammar's reified compilation `Plan`,
//! and/or the mermaid diagram rendered from it, mirroring `coverage`/`fst-health`'s own
//! argument-parsing and stdout-vs-file convention (see `coverage.rs`/`fst_health.rs`).
//!
//! - No flags: prints the mermaid diagram (the default "how is my language handled" view).
//! - `--json`: prints `pg_foma::plan_diagram::PlanDocument`'s canonical JSON instead. The JSON is
//!   ALWAYS the complete, uncollapsed plan — it is the source artifact, so `--full`/`--threshold`
//!   are mermaid-only and have no effect when combined with `--json`.
//! - `--full`: opt-in full mermaid rendering (no sibling-leaf collapsing, regardless of size).
//! - `--threshold=N`: overrides the default sibling-leaf collapsing threshold for mermaid rendering.
//!   Mutually exclusive with `--full` (both named at once is a usage error, not a silent pick).
//! - `<out>` given: writes the selected output (JSON or mermaid) there instead of stdout. A stderr
//!   summary line (node counts, threshold, whether summarization occurred, the overall capability
//!   verdict) is always printed, mirroring `fst-health`'s own post-run summary.

use std::fs;

use pg_foma::plan_diagram::{build_plan_document, render_mermaid, NodeVerdict, RenderMode};

pub fn run_plan_diagram(args: &[String]) -> Result<(), String> {
    let mut json = false;
    let mut full = false;
    let mut threshold: Option<usize> = None;
    let mut positional: Vec<&str> = Vec::new();

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--full" => full = true,
            "--threshold" => {
                let v = it.next().ok_or("--threshold requires a value")?;
                threshold = Some(v.parse().map_err(|_| format!("invalid --threshold: {v}"))?);
            }
            s if s.starts_with("--threshold=") => {
                let v = &s["--threshold=".len()..];
                threshold = Some(v.parse().map_err(|_| format!("invalid --threshold: {v}"))?);
            }
            s => positional.push(s),
        }
    }

    if full && threshold.is_some() {
        return Err(
            "--full and --threshold are mutually exclusive (--full already means \"no \
             collapsing at all\")"
                .to_string(),
        );
    }

    let (grammar_path, out_path): (&str, Option<&str>) = match positional[..] {
        [g] => (g, None),
        [g, o] => (g, Some(o)),
        _ => {
            return Err(
                "usage: plan-diagram <grammar> [--json] [--full] [--threshold=N] [<out>]"
                    .to_string(),
            );
        }
    };

    let (grammar, warnings) = crate::load_grammar(grammar_path)?;
    crate::print_grammar_warnings(&warnings);

    let doc = build_plan_document(&grammar);

    let mode = if full {
        RenderMode::Full
    } else {
        match threshold {
            Some(t) => RenderMode::Summarized { threshold: t },
            None => RenderMode::default(),
        }
    };

    let output = if json {
        doc.to_json()
            .map_err(|e| format!("serialize plan document: {e}"))?
    } else {
        render_mermaid(&doc, mode).mermaid
    };

    match out_path {
        Some(path) => fs::write(path, &output).map_err(|e| format!("write {path}: {e}"))?,
        None => print!("{output}"),
    }

    let overall = match &doc.overall_verdict {
        NodeVerdict::Admit => "Admit".to_string(),
        NodeVerdict::ConfirmOnly => "ConfirmOnly".to_string(),
        NodeVerdict::Refuse { diagnostics } => {
            format!("Refuse ({} diagnostic(s))", diagnostics.len())
        }
    };
    eprintln!(
        "plan-diagram complete: {} node(s), overall capability verdict={overall}.",
        doc.nodes.len()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PlanDiagramCliFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    fn scratch_path(tag: &str, ext: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("pangloss-cli-plan-diagram-test-{tag}-{n}.{ext}"))
    }

    fn write_fixture_grammar(tag: &str) -> std::path::PathBuf {
        let path = scratch_path(tag, "xml");
        fs::write(&path, XML).expect("write fixture grammar");
        path
    }

    #[test]
    fn plan_diagram_cli_writes_mermaid_by_default() {
        let grammar_path = write_fixture_grammar("mermaid");
        let out_path = scratch_path("mermaid-out", "mmd");

        run_plan_diagram(&[
            grammar_path.to_str().unwrap().to_string(),
            out_path.to_str().unwrap().to_string(),
        ])
        .expect("plan-diagram must succeed");

        let text = fs::read_to_string(&out_path).expect("read output");
        assert!(text.contains("flowchart TD"));
        assert!(text.contains("nodes emitted"));

        let _ = fs::remove_file(&grammar_path);
        let _ = fs::remove_file(&out_path);
    }

    #[test]
    fn plan_diagram_cli_writes_json_with_flag() {
        let grammar_path = write_fixture_grammar("json");
        let out_path = scratch_path("json-out", "json");

        run_plan_diagram(&[
            "--json".to_string(),
            grammar_path.to_str().unwrap().to_string(),
            out_path.to_str().unwrap().to_string(),
        ])
        .expect("plan-diagram --json must succeed");

        let text = fs::read_to_string(&out_path).expect("read output");
        let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        assert!(value.get("schema_version").is_some());
        assert!(value.get("nodes").is_some());

        let _ = fs::remove_file(&grammar_path);
        let _ = fs::remove_file(&out_path);
    }

    #[test]
    fn plan_diagram_cli_rejects_full_and_threshold_together() {
        let grammar_path = write_fixture_grammar("conflict");
        let err = run_plan_diagram(&[
            "--full".to_string(),
            "--threshold=5".to_string(),
            grammar_path.to_str().unwrap().to_string(),
        ])
        .expect_err("must reject --full combined with --threshold");
        assert!(err.contains("mutually exclusive"));

        let _ = fs::remove_file(&grammar_path);
    }
}
