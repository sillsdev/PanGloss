//! Differential gate for `compile_project`'s fatal-import-issue refusal, both directions, over every fixture reachable.

use std::path::{Path, PathBuf};

use pg_grammar::compile::{CompileOptions, SemanticLossPolicy};
use pg_grammar::{compile_project, compile_project_measured, compile_project_with};
use pg_snapshot::IssueClass;

/// Every `.fwdata`/`.fwbackup` this repository has committed -- the only valid gate input.
fn checked_in_fixtures() -> Vec<(&'static str, PathBuf)> {
    vec![(
        "pg-fwdata/tests/data/fixture.fwdata",
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../pg-fwdata/tests/data/fixture.fwdata"),
    )]
}

/// A real FieldWorks project outside this repo, reported only -- mirrors `pg-cli`'s own lookup.
fn real_corpus(project_dir_name: &str) -> Option<PathBuf> {
    let base = std::env::var("PANGLOSS_FW_PROJECTS_DIR").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(r"C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects")
    });
    let path = base.join(project_dir_name).join(format!("{project_dir_name}.fwdata"));
    path.exists().then_some(path)
}

/// Ratchet: today's measured count of checked-in fixtures newly refused, not a target.
const MAX_NEWLY_REFUSING: usize = 1;

#[test]
fn compile_project_refusal_differential_gate() {
    let mut newly_refusing = Vec::new();
    let mut newly_permitted = Vec::new();
    let mut agree = 0usize;

    for (name, path) in checked_in_fixtures() {
        assert!(path.exists(), "checked-in fixture missing: {}", path.display());
        let (snapshot, _report) =
            pg_fwdata::import_file(&path).unwrap_or_else(|e| panic!("{name}: must import: {e}"));

        // Untouched by the new refusal, so this stands in for what compile_project did before it.
        let before_ok = compile_project_measured(&snapshot).is_ok();
        let after_ok = compile_project(&snapshot).is_ok();
        println!("{name}: before(compile_project_measured)={before_ok} after(compile_project)={after_ok}");

        match (before_ok, after_ok) {
            (true, false) => newly_refusing.push(name),
            (false, true) => newly_permitted.push(name),
            _ => agree += 1,
        }
    }

    println!(
        "checked-in fixtures: {agree} agree, {} newly refusing, {} newly permitted",
        newly_refusing.len(),
        newly_permitted.len()
    );

    // Real corpora live outside this repo and are never a gate input; reported, never silently omitted.
    for project in ["Sena 3", "Amharic"] {
        match real_corpus(project) {
            Some(path) => {
                let (snapshot, _report) = pg_fwdata::import_file(&path)
                    .unwrap_or_else(|e| panic!("{project}: must import: {e}"));
                let before_ok = compile_project_measured(&snapshot).is_ok();
                let after_ok = compile_project(&snapshot).is_ok();
                println!(
                    "{project} (reported only -- outside this repo, environment-dependent, never a gate input): \
                     before={before_ok} after={after_ok}"
                );
            }
            None => println!(
                "{project}: not present on this machine (checked PANGLOSS_FW_PROJECTS_DIR and the default \
                 sibling-checkout path) -- reported as absent rather than silently skipped"
            ),
        }
    }

    assert!(
        newly_permitted.is_empty(),
        "compile_project newly PERMITS a fixture it used to refuse: {newly_permitted:?} -- a capability regression"
    );
    assert!(
        newly_refusing.len() <= MAX_NEWLY_REFUSING,
        "compile_project newly REFUSES {} checked-in fixture(s) (ratchet {MAX_NEWLY_REFUSING}): {newly_refusing:?}",
        newly_refusing.len()
    );
}

/// Pins the import/compile cross-layer severity disagreement over the fixture's dangling environment reference.
#[test]
fn import_and_compile_layers_disagree_about_the_dangling_environment_reference() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../pg-fwdata/tests/data/fixture.fwdata");
    let (snapshot, _report) =
        pg_fwdata::import_file(&path).expect("checked-in fixture must import");

    // The import stage marks this guid's dangling PhEnvironment reference fatal; that is what makes compile_project refuse it.
    let dangling_guid = "00000000-0000-0000-0000-0000000000ff";
    let import_verdict = snapshot
        .conversion_provenance
        .import_issues
        .iter()
        .find(|issue| {
            issue.source.as_ref().is_some_and(|s| s.kind == "PhEnvironment" && s.id == dangling_guid)
        })
        .expect("import stage must record a fatal issue for the dangling PhEnvironment reference");
    assert!(import_verdict.fatal, "import layer's verdict on this guid must be fatal");
    assert_eq!(import_verdict.class, IssueClass::InvalidSource);

    // The compile stage (affixes::resolve_environments, for this affix allomorph) independently downgrades the SAME guid to a non-fatal warning; MeasureOnly bypasses Refuse so that downgrade stays observable rather than being masked by the refusal.
    let measure_only = compile_project_with(
        &snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
            ..CompileOptions::default()
        },
    )
    .expect("MeasureOnly must never refuse, so the compile layer's own verdict stays observable");
    let compile_verdict = measure_only
        .issues
        .iter()
        .find(|issue| issue.message.contains(dangling_guid) && issue.code != "fwdata.dangling-reference")
        .expect("compile layer must still surface a warning about the identical guid, distinct from the carried-over import issue");
    assert!(
        !compile_verdict.fatal,
        "compile layer's verdict on this guid must be non-fatal today -- if this fails, the \
         compile-side site started agreeing with import's fatal verdict and this test's \
         expectations need revisiting, not just the assertion: {compile_verdict:?}"
    );

    let refuse = compile_project(&snapshot);
    assert!(refuse.is_err(), "the import layer's fatal verdict must still govern compile_project's default outcome");
}
