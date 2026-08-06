//! Diagnostic dump printing both pipelines' grammar structure so a human can diff where the new (.fwdata) grammar diverges from the legacy (HC-XML oracle) grammar; `#[ignore]`d, self-skipping when the FieldWorks checkout/oracle files are absent.

use std::path::PathBuf;

use pg_grammar::model::{Grammar, MorphRuleDef};

fn project_fwdata(project_dir_name: &str) -> Option<PathBuf> {
    let base = std::env::var("PANGLOSS_FW_PROJECTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(r"C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects")
        });
    let path = base
        .join(project_dir_name)
        .join(format!("{project_dir_name}.fwdata"));
    path.exists().then_some(path)
}

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn rule_label(g: &Grammar, mid: pg_grammar::model::MRuleId) -> String {
    match &g.mrules[mid.0 as usize] {
        MorphRuleDef::AffixProcess(d) => {
            let gloss = g.morphemes[d.morpheme.0 as usize]
                .gloss
                .as_deref()
                .unwrap_or("-");
            format!(
                "affix name={:?} gloss={:?} subrules={} partial={} template_rule={}",
                d.name.as_deref().unwrap_or(""),
                gloss,
                d.allomorphs.len(),
                d.partial,
                d.is_template_rule
            )
        }
        MorphRuleDef::Realizational(d) => {
            let gloss = g.morphemes[d.morpheme.0 as usize]
                .gloss
                .as_deref()
                .unwrap_or("-");
            format!("realizational gloss={gloss:?}")
        }
        MorphRuleDef::Compounding(d) => format!("compounding name={:?}", d.name),
    }
}

fn dump(tag: &str, g: &Grammar) {
    eprintln!(
        "##### {tag}: {} mrules, {} templates, {} entries, {} morphemes",
        g.mrules.len(),
        g.templates.len(),
        g.entries.len(),
        g.morphemes.len()
    );
    for (si, s) in g.strata.iter().enumerate() {
        eprintln!(
            "--- stratum {si} {:?}: {} mrules, {} templates, {} entries, {} prules",
            s.name,
            s.mrules.len(),
            s.templates.len(),
            s.entries.len(),
            s.prules.len()
        );
        for &mid in &s.mrules {
            eprintln!("  rule: {}", rule_label(g, mid));
        }
        for &tid in &s.templates {
            let t = &g.templates[tid.0 as usize];
            eprintln!(
                "  template {:?} final={} slots={}",
                t.name,
                t.is_final,
                t.slots.len()
            );
            for slot in &t.slots {
                eprintln!("    slot {:?} optional={}", slot.name, slot.optional);
                for &mid in &slot.rules {
                    eprintln!("      {}", rule_label(g, mid));
                }
            }
        }
    }
}

#[test]
#[ignore] // diagnostic only; run with --ignored --nocapture
fn dump_amharic_grammars() {
    let Some(fwdata_path) = project_fwdata("Amharic") else {
        eprintln!("skipping: Amharic FieldWorks project not present");
        return;
    };
    let Some(oracle_path) = sample_path("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present");
        return;
    };
    let (snapshot, _report) = pg_fwdata::import_file(&fwdata_path).expect("import");
    let (new_grammar, _warnings) = pg_grammar::compile_project(&snapshot).expect("compile");
    let legacy_grammar =
        pg_grammar::load(&std::fs::read_to_string(&oracle_path).expect("read oracle"))
            .expect("load oracle");
    dump("NEW (fwdata)", &new_grammar);
    dump("LEGACY (oracle)", &legacy_grammar);
}

#[test]
#[ignore] // diagnostic only; run with --ignored --nocapture
fn dump_sena_grammars() {
    let Some(fwdata_path) = project_fwdata("Sena 3") else {
        eprintln!("skipping: Sena 3 FieldWorks project not present");
        return;
    };
    let Some(oracle_path) = sample_path("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present");
        return;
    };
    let (snapshot, _report) = pg_fwdata::import_file(&fwdata_path).expect("import");
    let (new_grammar, _warnings) = pg_grammar::compile_project(&snapshot).expect("compile");
    let legacy_grammar =
        pg_grammar::load(&std::fs::read_to_string(&oracle_path).expect("read oracle"))
            .expect("load oracle");
    dump("NEW (fwdata)", &new_grammar);
    dump("LEGACY (oracle)", &legacy_grammar);
}
