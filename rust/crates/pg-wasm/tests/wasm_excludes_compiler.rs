//! `pg-wasm`/`pg-pack` must never reach `pg-foma` or `foma` on wasm32; pinned by the tests below.

use std::collections::{HashMap, HashSet, VecDeque};
use std::process::Command;

use serde_json::Value;

fn workspace_manifest_path() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml").to_string()
}

/// Fails with the exact stderr on a spawn or non-zero-exit failure, never silently skips.
fn wasm32_metadata() -> Value {
    let manifest_path = workspace_manifest_path();
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--filter-platform",
            "wasm32-unknown-unknown",
            "--manifest-path",
            &manifest_path,
        ])
        .output()
        .unwrap_or_else(|error| panic!("failed to spawn `cargo metadata`: {error}"));
    assert!(
        output.status.success(),
        "cargo metadata --filter-platform wasm32-unknown-unknown failed (status {:?}):\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata must emit valid JSON")
}

/// True iff `dep` carries a non-`"dev"` `dep_kinds` entry (a crate's own dev-deps always appear on its resolve node, reachable or not).
fn dep_edge_reaches_downstream_builds(dep: &Value) -> bool {
    dep["dep_kinds"]
        .as_array()
        .expect("dep.dep_kinds must be an array")
        .iter()
        .any(|dep_kind| !matches!(dep_kind["kind"].as_str(), Some("dev")))
}

/// Every package name reachable from `root_package_name`'s resolved wasm32 node, transitively.
fn reachable_package_names(metadata: &Value, root_package_name: &str) -> HashSet<String> {
    let packages = metadata["packages"]
        .as_array()
        .expect("metadata.packages must be an array");
    let name_by_id: HashMap<&str, &str> = packages
        .iter()
        .map(|package| {
            (
                package["id"].as_str().expect("package.id must be a string"),
                package["name"]
                    .as_str()
                    .expect("package.name must be a string"),
            )
        })
        .collect();

    let nodes = metadata["resolve"]["nodes"].as_array().expect(
        "metadata.resolve.nodes must be an array -- cargo metadata must not be run with --no-deps",
    );
    let node_by_id: HashMap<&str, &Value> = nodes
        .iter()
        .map(|node| (node["id"].as_str().expect("node.id must be a string"), node))
        .collect();

    let root_id = packages
        .iter()
        .find(|package| package["name"].as_str() == Some(root_package_name))
        .unwrap_or_else(|| {
            panic!("workspace package {root_package_name:?} not found in cargo metadata")
        })["id"]
        .as_str()
        .expect("package.id must be a string");

    let mut visited: HashSet<&str> = HashSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();
    queue.push_back(root_id);
    while let Some(id) = queue.pop_front() {
        if !visited.insert(id) {
            continue;
        }
        let Some(node) = node_by_id.get(id) else {
            continue;
        };
        let deps = node["deps"].as_array().expect("node.deps must be an array");
        for dep in deps {
            // Skip dev-only edges: they never reach a downstream consumer's own build.
            if !dep_edge_reaches_downstream_builds(dep) {
                continue;
            }
            let dep_id = dep["pkg"].as_str().expect("dep.pkg must be a string");
            if !visited.contains(dep_id) {
                queue.push_back(dep_id);
            }
        }
    }

    visited
        .into_iter()
        .map(|id| name_by_id.get(id).copied().unwrap_or(id).to_string())
        .collect()
}

fn assert_excludes_compiler(root_package_name: &str) {
    let metadata = wasm32_metadata();
    let reachable = reachable_package_names(&metadata, root_package_name);
    assert!(
        !reachable.contains("foma"),
        "{root_package_name}'s wasm32 dependency graph must never reach the `foma` FST compiler \
         library, but it does: {reachable:?}"
    );
    assert!(
        !reachable.contains("pg-foma"),
        "{root_package_name}'s wasm32 dependency graph must never reach the `pg-foma` compiler \
         crate, but it does: {reachable:?}"
    );
}

#[test]
fn pg_wasm_wasm32_graph_excludes_the_compiler() {
    assert_excludes_compiler("pg-wasm");
}

#[test]
fn pg_pack_wasm32_graph_excludes_the_compiler() {
    assert_excludes_compiler("pg-pack");
}
