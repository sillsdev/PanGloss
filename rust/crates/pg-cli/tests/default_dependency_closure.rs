use std::collections::{HashMap, HashSet};
use std::process::Command;

#[test]
fn default_workspace_dependency_closure_keeps_pg_fst_without_whole_grammar_foma() {
    let cli_manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let rust_root = cli_manifest.parent().unwrap().parent().unwrap();
    let metadata = Command::new("cargo")
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--manifest-path",
        ])
        .arg(rust_root.join("Cargo.toml"))
        .output()
        .expect("run cargo metadata for the workspace dependency graph");
    assert!(
        metadata.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&metadata.stderr)
    );

    let graph: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let roots: HashSet<String> = graph["workspace_default_members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_string())
        .collect();
    let names: HashMap<String, String> = graph["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|package| {
            (
                package["id"].as_str().unwrap().to_string(),
                package["name"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let edges: HashMap<String, Vec<String>> = graph["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| {
            let dependencies = node["deps"]
                .as_array()
                .unwrap()
                .iter()
                .map(|dependency| dependency["pkg"].as_str().unwrap().to_string())
                .collect();
            (node["id"].as_str().unwrap().to_string(), dependencies)
        })
        .collect();

    let mut closure = roots.clone();
    let mut pending: Vec<String> = roots.into_iter().collect();
    while let Some(package) = pending.pop() {
        for dependency in edges.get(&package).into_iter().flatten() {
            if closure.insert(dependency.clone()) {
                pending.push(dependency.clone());
            }
        }
    }
    let closure_names: HashSet<&str> = closure
        .iter()
        .filter_map(|package| names.get(package).map(String::as_str))
        .collect();

    assert!(
        closure_names.contains("pg-fst"),
        "pg-fst left the native runtime graph"
    );
    for forbidden in ["pg-foma", "pg-foma-runtime", "pg-foma-backend", "foma"] {
        assert!(
            !closure_names.contains(forbidden),
            "whole-grammar Foma package {forbidden} remains in the default workspace closure"
        );
    }
}
