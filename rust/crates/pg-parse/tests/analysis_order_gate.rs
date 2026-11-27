//! Pins the order of one word's analyses (#64): distinct analyses come out in the order they are first produced, so the lexicon's entry order decides ties and no hash table does. The guess-root morph-count sort is pinned by `guesser_gate.rs`.

use pg_grammar::load;
use pg_parse::Morpher;

fn xml(glosses: &[&str]) -> String {
    let entries: String = glosses
        .iter()
        .enumerate()
        .map(|(i, g)| {
            format!(
                r#"<LexicalEntry id="e{i}"><Allomorphs><Allomorph id="a{i}"><PhoneticShape>ga</PhoneticShape></Allomorph></Allomorphs><Gloss>{g}</Gloss></LexicalEntry>"#
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>Order</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="t1"><Name>Main</Name><SegmentDefinitions>
<SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
</SegmentDefinitions></CharacterDefinitionTable>
<Strata><Stratum characterDefinitionTable="t1"><Name>S</Name><LexicalEntries>{entries}</LexicalEntries></Stratum></Strata>
</Language></HermitCrabInput>"#
    )
}

fn analyses(glosses: &[&str]) -> Vec<String> {
    let g = load(&xml(glosses)).unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
    let m = Morpher::new(&g, usize::MAX);
    m.parse_word("ga")
        .analyses
        .into_iter()
        .map(|(a, _)| a)
        .collect()
}

#[test]
fn homophones_come_out_in_lexicon_order_whatever_the_order_is() {
    // Many entries so a hash-ordered result could not match both orders by luck.
    let forward: Vec<String> = (0..24).map(|i| format!("g{i:02}")).collect();
    let reversed: Vec<String> = forward.iter().rev().cloned().collect();
    let fwd_refs: Vec<&str> = forward.iter().map(String::as_str).collect();
    let rev_refs: Vec<&str> = reversed.iter().map(String::as_str).collect();

    let got_forward = analyses(&fwd_refs);
    let got_reversed = analyses(&rev_refs);

    assert_eq!(got_forward.len(), 24, "{got_forward:?}");
    assert_eq!(
        got_reversed.iter().rev().cloned().collect::<Vec<_>>(),
        got_forward,
        "reversing the lexicon must reverse the analyses"
    );
    assert_eq!(analyses(&fwd_refs), got_forward, "same input, same order");
}
