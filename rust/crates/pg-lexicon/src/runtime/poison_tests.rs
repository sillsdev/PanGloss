use super::*;

const XML: &str = r#"<HermitCrabInput><Language><Name>PoisonTest</Name><PartsOfSpeech><PartOfSpeech id="p"><Name>N</Name></PartOfSpeech></PartsOfSpeech><CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions><SegmentDefinition id="a"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable><Strata><Stratum characterDefinitionTable="t"><Name>S</Name><LexicalEntries><LexicalEntry id="a" partOfSpeech="p"><Allomorphs><Allomorph id="aa"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#;

#[test]
fn poisoned_snapshot_lock_recovers_the_old_snapshot() {
    let grammar = Arc::new(pg_grammar::load(XML).unwrap());
    let runtime = SuppliedLexiconRuntime::new(grammar, XML).unwrap();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.force_state_lock_panic_for_test()
    }))
    .is_err());
    assert_eq!(
        serde_json::to_value(runtime.snapshot().revision()).unwrap(),
        "rev_0"
    );
}
