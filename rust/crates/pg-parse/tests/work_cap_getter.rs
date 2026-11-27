//! `Morpher::work_cap` reports the cap the morpher was built with, so a caller can refuse an unbounded one.

use pg_grammar::load;
use pg_parse::Morpher;

const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>Cap</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="t1"><Name>Main</Name><SegmentDefinitions>
<SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
</SegmentDefinitions></CharacterDefinitionTable>
<Strata><Stratum characterDefinitionTable="t1"><Name>S</Name><LexicalEntries>
<LexicalEntry id="e0"><Allomorphs><Allomorph id="a0"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs><Gloss>g</Gloss></LexicalEntry>
</LexicalEntries></Stratum></Strata>
</Language></HermitCrabInput>"#;

#[test]
fn work_cap_reports_the_configured_cap() {
    let g = load(XML).unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
    assert_eq!(Morpher::new(&g, 12_345).work_cap(), 12_345);
    assert_eq!(Morpher::new(&g, usize::MAX).work_cap(), usize::MAX);
}
