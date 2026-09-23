//! `pangloss plan-diagram <grammar> [--json] [--full] [--threshold=N] [<out>]` renders `pg_foma::plan_diagram`'s JSON projection of a grammar's reified `Plan`, and/or the mermaid diagram from it (default: mermaid; `--json` prints the always-complete JSON instead, ignoring the mermaid-only `--full`/`--threshold`; `<out>` writes to a file instead of stdout).

use std::fs;

use pg_foma_backend::plan_diagram::{build_plan_document, render_mermaid, NodeVerdict, RenderMode};

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
            s => {
                crate::reject_unknown_option("plan-diagram", s)?;
                positional.push(s);
            }
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
mod tests;
