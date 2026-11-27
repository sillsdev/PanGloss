//! Boundary-initial narrowing must contribute its inserted segment to affix variants.

use foma::{apply::apply_init, lexcread::fsm_lexc_parse_string, options::FomaOptions};
use pg_foma::junctions::PhonologyProbe;

#[test]
fn boundary_initial_narrowing_keeps_the_inserted_affix_variant() {
    let grammar = pg_grammar::load(include_str!("../fixtures/boundary-narrowing.xml"))
        .expect("fixture loads");
    let probe = PhonologyProbe::new(&grammar).expect("fixture has phonology");
    let variants = probe.variants("+bo");
    assert_eq!(variants, ["bo", "mo"]);
}

#[test]
fn expansion_after_the_right_neighbour_does_not_copy_it_into_the_affix() {
    let original = include_str!("../fixtures/boundary-narrowing.xml");
    let grammar_xml = original
        .replace(
            "<BoundaryMarker boundary=\"bPlus\" />\n            <Segment segment=\"cB\" />",
            "<Segment segment=\"cN\" />",
        )
        .replace(
            "<Segment segment=\"cM\" />\n              </PhoneticSequence>",
            "<Segment segment=\"cN\" />\n                <Segment segment=\"cM\" />\n              </PhoneticSequence>",
        )
        .replace(
            "<Environment>\n              <LeftEnvironment>\n                <PhoneticTemplate>\n                  <PhoneticSequence>\n                    <SimpleContext naturalClass=\"ncNas\" />\n                  </PhoneticSequence>\n                </PhoneticTemplate>\n              </LeftEnvironment>\n            </Environment>",
            "",
        );
    assert_ne!(grammar_xml, original);
    let grammar = pg_grammar::load(&grammar_xml).expect("expansion fixture loads");
    let variants = PhonologyProbe::new(&grammar)
        .expect("fixture has phonology")
        .variants("+bo");
    assert_eq!(variants, ["bo"], "neighbour leaked into affix");
}

#[test]
fn ordinary_emission_reaches_the_variant_with_or_without_the_fusion_composite() {
    let grammar = pg_grammar::load(include_str!("../fixtures/boundary-narrowing.xml"))
        .expect("fixture loads");
    let emitted = pg_foma::emit::emit(&grammar);
    assert!(emitted.report.counts.composite_fusion_entries > 0);
    assert!(emitted.lexc_source.contains("Composites ;\n"));
    let without_composites = emitted.lexc_source.replace("Composites ;\n", "");
    for source in [&emitted.lexc_source, &without_composites] {
        let net = fsm_lexc_parse_string(&FomaOptions::default(), None, source)
            .expect("emitted lexc compiles");
        let mut handle = apply_init(&net);
        assert!(handle.up("nmo").next().is_some(), "nmo must be proposed");
    }
}
