use super::*;

#[test]
fn fst_profile_stage_label_is_stable_and_exhaustive() {
    // Closed-enum discipline: every variant has a label and there is no catch-all arm, so adding a stage breaks this match until labeled.
    for stage in [
        CompileStage::SurfaceSetup,
        CompileStage::RootCollection,
        CompileStage::PreexpandComposites,
        CompileStage::StructuralComposites,
        CompileStage::LexcConstruction,
        CompileStage::LexcParse,
    ] {
        assert!(!stage.label().is_empty());
    }
}

#[test]
fn fst_profile_builder_finish_stamps_total_elapsed_and_stages() {
    let mut builder = CompileProfileBuilder::production();
    builder.push_stage(CompileStage::SurfaceSetup, Duration::from_millis(5));
    builder.push_stage(CompileStage::RootCollection, Duration::from_millis(7));
    builder.push_group_lines(0, 42);
    builder.set_total_lexc_lines(1234);
    let profile = builder.finish(Some(100), Some(250));

    assert_eq!(profile.pipeline, PRODUCTION_PIPELINE);
    assert_eq!(profile.stages.len(), 2);
    assert_eq!(profile.stages[0].stage, CompileStage::SurfaceSetup);
    assert_eq!(profile.stages[0].elapsed_millis, 5);
    assert_eq!(profile.stages[1].elapsed_millis, 7);
    assert_eq!(
        profile.group_lines,
        vec![GroupLineCount {
            group_index: 0,
            lines: 42
        }]
    );
    assert_eq!(profile.final_state_count, Some(100));
    assert_eq!(profile.final_arc_count, Some(250));
    assert_eq!(profile.total_lexc_lines, Some(1234));
}

#[test]
fn fst_profile_finish_with_no_compiled_network_leaves_counts_none() {
    // The production path can bail out before ever reaching a compiled network -- `None`, never a fabricated `0`.
    let profile = CompileProfileBuilder::production().finish(None, None);
    assert_eq!(profile.final_state_count, None);
    assert_eq!(profile.final_arc_count, None);
    assert_eq!(profile.total_lexc_lines, None);
}

#[test]
fn fst_profile_json_round_trips() {
    let mut builder = CompileProfileBuilder::production();
    builder.push_stage(CompileStage::LexcConstruction, Duration::from_millis(12));
    let profile = builder.finish(Some(10), Some(20));
    let json = serde_json::to_string(&profile).expect("serialize");
    let parsed: CompileProfile = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed, profile);
}
