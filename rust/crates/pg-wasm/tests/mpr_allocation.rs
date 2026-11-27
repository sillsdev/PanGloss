#[path = "../../../test-support/mpr_allocation.rs"]
mod allocation;

#[test]
fn untrusted_mpr_ids_do_not_expand_before_host_validation() {
    let fixture =
        pg_conformance_fixtures::require_fixture("edge-cases", "mpr-wide-inflection-classes");
    let xml = fixture.load_grammar_xml();
    let grammar = std::sync::Arc::new(pg_grammar::load(&xml).unwrap());
    assert_eq!(grammar.mpr_features.len(), 303);
    let runtime = pg_lexicon::SuppliedLexiconRuntime::with_policy(
        grammar,
        &xml,
        pg_lexicon::AnalysisPolicy::without_limits(),
    )
    .unwrap();
    for id in [0, 67_108_863, 268_435_455] {
        let json = allocation::request(id);
        let (result, largest) = allocation::measured(|| {
            let request = serde_json::from_value(json).unwrap();
            runtime.generate_host_analysis(request)
        });
        assert!(largest < 65_536, "ID {id} allocated {largest} bytes");
        assert_eq!(result.unwrap_err().code, "revision_conflict");
    }
    for id in [303, 67_108_863, 268_435_455, u32::MAX] {
        for location in 0..3 {
            let mut json = allocation::request_at(id, location);
            json["revision"] = serde_json::to_value(runtime.snapshot().revision()).unwrap();
            let (result, largest) = allocation::measured(|| {
                let request = serde_json::from_value(json).unwrap();
                runtime.generate_host_analysis(request)
            });
            assert!(
                largest < 65_536,
                "ID {id} at {location} allocated {largest} bytes"
            );
            assert_eq!(result.unwrap_err().code, "invalid_mpr_id");
        }
    }
}
