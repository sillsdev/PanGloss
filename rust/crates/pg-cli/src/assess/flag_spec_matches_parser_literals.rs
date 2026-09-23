//! Cross-checks each `run_*` command's `.flag("...")` reads against its own `CommandSpec`.
use crate::surface::find_command;

fn function_source(signature: &str) -> &'static str {
    let source = include_str!("../assess.rs");
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("signature must exist in this file: {signature}"));
    let body = &source[start..];
    let mut depth = 0i32;
    let mut end = None;
    for (i, c) in body.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    &body[..end.unwrap_or_else(|| panic!("no matching closing brace for: {signature}"))]
}

/// The bare names passed to `.flag("...")` within `source`.
fn flag_call_literals(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find(".flag(\"") {
        let after = &rest[start + ".flag(\"".len()..];
        let Some(end) = after.find('"') else { break };
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    out.sort();
    out.dedup();
    out
}

/// `--report` is only read inside the shared `emit` helper, so every scan includes its body too.
const EMIT_SIGNATURE: &str =
    "fn emit(args: &Args, value: &serde_json::Value) -> Result<(), CliError> {";

fn assert_flags_match(command: &str, signature: &str) {
    let spec = find_command(command).unwrap_or_else(|| panic!("{command} must be in COMMANDS"));
    let mut literals = flag_call_literals(function_source(signature));
    literals.extend(flag_call_literals(function_source(EMIT_SIGNATURE)));
    literals.sort();
    literals.dedup();
    for name in &literals {
        let flag_name = format!("--{name}");
        assert!(
            spec.flag(&flag_name).is_some(),
            "{signature} reads .flag(\"{name}\") but the `{command}` CommandSpec doesn't \
                 declare {flag_name}"
        );
    }
    for flag in spec.flags {
        let name = flag.name.trim_start_matches("--");
        assert!(
            literals.contains(&name.to_string()),
            "{command} CommandSpec declares {} but {signature} never reads .flag(\"{name}\")",
            flag.name
        );
    }
}

#[test]
fn compare_flags_match_its_spec() {
    assert_flags_match(
        "compare",
        "pub fn run_compare(args: &[String]) -> Result<(), CliError> {",
    );
}

#[test]
fn golden_diff_flags_match_its_spec() {
    assert_flags_match(
        "golden-diff",
        "pub fn run_golden_diff(args: &[String]) -> Result<(), CliError> {",
    );
}

#[test]
fn investigate_flags_match_its_spec() {
    assert_flags_match(
        "investigate",
        "pub fn run_investigate(args: &[String]) -> Result<(), CliError> {",
    );
}
