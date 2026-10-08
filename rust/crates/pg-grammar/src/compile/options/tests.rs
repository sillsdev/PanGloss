use super::*;

#[test]
fn compile_options_carries_only_semantic_loss() {
    let CompileOptions { semantic_loss } = CompileOptions::default();
    assert_eq!(semantic_loss, SemanticLossPolicy::Refuse);
}
