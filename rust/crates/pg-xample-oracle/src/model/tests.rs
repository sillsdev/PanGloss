use super::*;

fn sig(morphemes: &[&str], msa_ids: &[&str], surface: &str) -> AnalysisSignature {
    AnalysisSignature {
        morphemes: morphemes.iter().map(|s| s.to_string()).collect(),
        msa_ids: msa_ids.iter().map(|s| s.to_string()).collect(),
        category_id: None,
        surface_nfd: surface.to_string(),
    }
}

#[test]
fn identical_signatures_count_rather_than_collapse() {
    let mut analyses = BTreeMap::new();
    let a = sig(&["P1", "K"], &["guid-p1", "guid-k"], "xk");
    *analyses.entry(a.clone()).or_insert(0) += 1;
    *analyses.entry(a.clone()).or_insert(0) += 1;
    assert_eq!(analyses.len(), 1, "one distinct signature key");
    assert_eq!(analyses[&a], 2, "counted twice, never deduplicated to 1");
}

#[test]
fn xample_result_partial_eq_must_keep_comparing_reached_max_analyses() {
    // A fence against a future hand-written XampleResult comparison dropping this field the way AnalysisSignature's derive once dropped its morphemes exclusion.
    let mut analyses = BTreeMap::new();
    analyses.insert(sig(&["P1", "K"], &["guid-p1", "guid-k"], "xk"), 1);
    let capped = XampleResult {
        analyses: analyses.clone(),
        reached_max_analyses: Some(1),
        engine_error: None,
    };
    let uncapped = XampleResult {
        analyses,
        reached_max_analyses: None,
        engine_error: None,
    };
    assert_ne!(
        capped, uncapped,
        "a capped result must never compare equal to an uncapped one with the same members"
    );
}

#[test]
fn xample_result_partial_eq_must_keep_comparing_engine_error() {
    // Same fence as above, for engine_error: an empty analyses set alone must never look like success.
    let empty_success = XampleResult {
        analyses: BTreeMap::new(),
        reached_max_analyses: None,
        engine_error: None,
    };
    let empty_failure = XampleResult {
        analyses: BTreeMap::new(),
        reached_max_analyses: None,
        engine_error: Some("LoadFiles failed".to_string()),
    };
    assert_ne!(
        empty_success, empty_failure,
        "an engine error must never read as an ordinary empty result"
    );
}

#[test]
fn signatures_differing_only_in_display_morphemes_are_equal_and_merge_counts() {
    let a = sig(&["P1", "K"], &["guid-p1", "guid-k"], "xk");
    let b = sig(
        &["different-label", "other-label"],
        &["guid-p1", "guid-k"],
        "xk",
    );
    assert_eq!(
        a, b,
        "morphemes must never participate in AnalysisSignature identity"
    );
    let mut analyses = BTreeMap::new();
    *analyses.entry(a).or_insert(0) += 1;
    *analyses.entry(b).or_insert(0) += 1;
    assert_eq!(
        analyses.len(),
        1,
        "the two arrivals must collapse into one multiset entry"
    );
    assert_eq!(
        *analyses.values().next().unwrap(),
        2,
        "and their counts must sum"
    );
}

#[test]
fn signatures_differing_only_in_category_id_are_equal_and_merge_counts() {
    // category_id must not participate in identity: XAMPLE's own categoryId is unpopulated.
    let mut xample_side = sig(&["K"], &["guid-k"], "k");
    xample_side.category_id = None;
    let mut hc_side = sig(&["K"], &["guid-k"], "k");
    hc_side.category_id = Some("pos-guid".to_string());
    assert_eq!(
        xample_side, hc_side,
        "category_id must never participate in AnalysisSignature identity"
    );
    let mut analyses = BTreeMap::new();
    *analyses.entry(xample_side).or_insert(0) += 1;
    *analyses.entry(hc_side).or_insert(0) += 1;
    assert_eq!(
        analyses.len(),
        1,
        "the two arrivals must collapse into one multiset entry"
    );
    assert_eq!(
        *analyses.values().next().unwrap(),
        2,
        "and their counts must sum"
    );
}

#[test]
fn error_can_coexist_with_non_empty_analyses() {
    // The other shape the type must not forbid: a partial result alongside a reported failure.
    let mut analyses = BTreeMap::new();
    analyses.insert(sig(&["K"], &["guid-k"], "k"), 1);
    let partial_then_failed = XampleResult {
        analyses,
        reached_max_analyses: None,
        engine_error: Some("engine exception mid-word".to_string()),
    };
    assert!(!partial_then_failed.analyses.is_empty());
    assert!(partial_then_failed.engine_error.is_some());
}
