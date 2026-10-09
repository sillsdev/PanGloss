use std::path::{Path, PathBuf};

use pg_parse::Morpher;
use pg_snapshot::{PhonContext, PhonologicalRule};

const WORD_BOUNDARY_GUID: &str = "7db635e0-9ef3-4167-a594-12551ed89aaa";
const MORPHEME_BOUNDARY_GUID: &str = "00000000-0000-0000-0000-000000000014";
const ORDINARY_HASH_MARKER_GUID: &str = "00000000-0000-0000-0000-000000000062";

fn replace_exactly(source: &mut String, old: &str, new: &str, expected: usize) {
    let found = source.matches(old).count();
    assert_eq!(found, expected, "fixture replacement count for {old:?}");
    *source = source.replace(old, new);
}

fn add_phoneme_feature(source: &mut String, phoneme: &str, fs: &str) {
    let start = source
        .find(&format!("<rt class=\"PhPhoneme\" guid=\"{phoneme}\""))
        .expect("fixture has the phoneme record");
    let end = start
        + source[start..]
            .find("</rt>")
            .expect("phoneme record closes")
        + "</rt>".len();
    source.insert_str(
        end - "</rt>".len(),
        &format!("<Features><objsur guid=\"{fs}\" t=\"o\" /></Features>\n"),
    );
}

fn reserved_boundary_fixture(dir: &Path, literal_hash_context: bool) -> PathBuf {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../pg-fwdata/tests/data/fixture.fwdata");
    let mut source = std::fs::read_to_string(fixture).expect("read synthetic FWData fixture");

    // Distinct feature values keep the rewrite target and result apart.
    replace_exactly(
        &mut source,
        "<rt class=\"FsFeatureSystem\" guid=\"00000000-0000-0000-0000-000000000005\" ownerguid=\"00000000-0000-0000-0000-000000000001\">\n</rt>",
        r#"<rt class="FsFeatureSystem" guid="00000000-0000-0000-0000-000000000005" ownerguid="00000000-0000-0000-0000-000000000001"><Features><objsur guid="00000000-0000-0000-0000-000000000100" t="o" /></Features></rt>
<rt class="FsClosedFeature" guid="00000000-0000-0000-0000-000000000100" ownerguid="00000000-0000-0000-0000-000000000005"><Abbreviation><AUni ws="en">place</AUni></Abbreviation><Name><AUni ws="en">Place</AUni></Name><Values><objsur guid="00000000-0000-0000-0000-000000000101" t="o" /><objsur guid="00000000-0000-0000-0000-000000000102" t="o" /><objsur guid="00000000-0000-0000-0000-000000000103" t="o" /></Values></rt>
<rt class="FsSymFeatVal" guid="00000000-0000-0000-0000-000000000101" ownerguid="00000000-0000-0000-0000-000000000100"><Abbreviation><AUni ws="en">a</AUni></Abbreviation><Name><AUni ws="en">a</AUni></Name></rt>
<rt class="FsSymFeatVal" guid="00000000-0000-0000-0000-000000000102" ownerguid="00000000-0000-0000-0000-000000000100"><Abbreviation><AUni ws="en">r</AUni></Abbreviation><Name><AUni ws="en">r</AUni></Name></rt>
<rt class="FsSymFeatVal" guid="00000000-0000-0000-0000-000000000103" ownerguid="00000000-0000-0000-0000-000000000100"><Abbreviation><AUni ws="en">t</AUni></Abbreviation><Name><AUni ws="en">t</AUni></Name></rt>"#,
        1,
    );
    for (phoneme, fs) in [
        (
            "00000000-0000-0000-0000-000000000010",
            "00000000-0000-0000-0000-000000000110",
        ),
        (
            "00000000-0000-0000-0000-000000000012",
            "00000000-0000-0000-0000-000000000111",
        ),
        (
            "00000000-0000-0000-0000-00000000001b",
            "00000000-0000-0000-0000-000000000112",
        ),
    ] {
        add_phoneme_feature(&mut source, phoneme, fs);
    }

    replace_exactly(
        &mut source,
        "<AUni ws=\"fx\">t</AUni>",
        "<AUni ws=\"fx\">r</AUni>",
        2,
    );
    replace_exactly(
        &mut source,
        "<AUni ws=\"fx\">d</AUni>",
        "<AUni ws=\"fx\">t</AUni>",
        2,
    );
    replace_exactly(
        &mut source,
        "<AUni ws=\"fx\">kat</AUni>",
        "<AUni ws=\"fx\">a</AUni>",
        2,
    );
    replace_exactly(&mut source, "<SuffixSlots>", "<PrefixSlots>", 1);
    replace_exactly(&mut source, "</SuffixSlots>", "</PrefixSlots>", 1);
    replace_exactly(
        &mut source,
        "<objsur guid=\"d7f713dd-e8cf-11d3-9764-00c04f186933\" t=\"r\" />",
        "<objsur guid=\"d7f713db-e8cf-11d3-9764-00c04f186933\" t=\"r\" />",
        1,
    );
    replace_exactly(
        &mut source,
        "<PartOfSpeech>\n<objsur guid=\"00000000-0000-0000-0000-00000000000b\" t=\"r\" />\n</PartOfSpeech>",
        "<PartOfSpeech>\n<objsur guid=\"00000000-0000-0000-0000-00000000000c\" t=\"r\" />\n</PartOfSpeech>",
        1,
    );
    replace_exactly(
        &mut source,
        "<PhoneEnv>\n<objsur guid=\"00000000-0000-0000-0000-000000000017\" t=\"r\" />\n<objsur guid=\"00000000-0000-0000-0000-0000000000ff\" t=\"r\" />\n</PhoneEnv>\n",
        "",
        1,
    );
    let process_start = source
        .find("<rt class=\"MoAffixAllomorph\" guid=\"00000000-0000-0000-0000-000000000051\"")
        .expect("fixture has its affix allomorph");
    let process_end = process_start
        + source[process_start..]
            .find("</rt>")
            .expect("affix allomorph record closes")
        + "</rt>".len();
    source.replace_range(
        process_start..process_end,
        r#"<rt class="MoAffixProcess" guid="00000000-0000-0000-0000-000000000051" ownerguid="00000000-0000-0000-0000-000000000050">
<Input><objsur guid="00000000-0000-0000-0000-000000000070" t="o" /></Input>
<IsAbstract val="False" />
<MorphType><objsur guid="d7f713db-e8cf-11d3-9764-00c04f186933" t="r" /></MorphType>
<Output>
<objsur guid="00000000-0000-0000-0000-000000000071" t="o" />
<objsur guid="00000000-0000-0000-0000-000000000072" t="o" />
</Output>
</rt>"#,
    );
    replace_exactly(
        &mut source,
        "<BoundaryMarkers>\n<objsur guid=\"00000000-0000-0000-0000-000000000014\" t=\"o\" />\n</BoundaryMarkers>",
        "<BoundaryMarkers>\n<objsur guid=\"00000000-0000-0000-0000-000000000014\" t=\"o\" />\n<objsur guid=\"00000000-0000-0000-0000-000000000062\" t=\"o\" />\n<objsur guid=\"7db635e0-9ef3-4167-a594-12551ed89aaa\" t=\"o\" />\n</BoundaryMarkers>",
        1,
    );
    replace_exactly(
        &mut source,
        "<LeftContext>\n<objsur guid=\"00000000-0000-0000-0000-00000000001d\" t=\"o\" />\n</LeftContext>\n<StrucChange>",
        "<LeftContext>\n<objsur guid=\"00000000-0000-0000-0000-000000000060\" t=\"o\" />\n</LeftContext>\n<RightContext>\n<objsur guid=\"00000000-0000-0000-0000-00000000001d\" t=\"o\" />\n</RightContext>\n<StrucChange>",
        1,
    );

    let marker_records = r#"
<rt class="PhBdryMarker" guid="7db635e0-9ef3-4167-a594-12551ed89aaa" ownerguid="00000000-0000-0000-0000-00000000000f">
<Codes><objsur guid="00000000-0000-0000-0000-000000000061" t="o" /></Codes>
<Name><AUni ws="en">word boundary</AUni></Name>
</rt>
<rt class="PhCode" guid="00000000-0000-0000-0000-000000000061" ownerguid="7db635e0-9ef3-4167-a594-12551ed89aaa">
<Representation><AUni ws="fx">#</AUni></Representation>
</rt>
<rt class="PhBdryMarker" guid="00000000-0000-0000-0000-000000000062" ownerguid="00000000-0000-0000-0000-00000000000f">
<Codes><objsur guid="00000000-0000-0000-0000-000000000063" t="o" /></Codes>
<Name><AUni ws="en">literal hash</AUni></Name>
</rt>
<rt class="PhCode" guid="00000000-0000-0000-0000-000000000063" ownerguid="00000000-0000-0000-0000-000000000062">
<Representation><AUni ws="fx">#</AUni></Representation>
</rt>
<rt class="PhVariable" guid="00000000-0000-0000-0000-000000000070" ownerguid="00000000-0000-0000-0000-000000000051" />
<rt class="MoInsertPhones" guid="00000000-0000-0000-0000-000000000071" ownerguid="00000000-0000-0000-0000-000000000051">
<Content><objsur guid="00000000-0000-0000-0000-000000000012" t="r" /></Content>
</rt>
<rt class="MoCopyFromInput" guid="00000000-0000-0000-0000-000000000072" ownerguid="00000000-0000-0000-0000-000000000051">
<Content><objsur guid="00000000-0000-0000-0000-000000000070" t="r" /></Content>
</rt>
"#;
    let marker_anchor = "<rt class=\"PhNCSegments\" guid=\"00000000-0000-0000-0000-000000000016\"";
    let marker_at = source
        .find(marker_anchor)
        .expect("fixture has its natural class");
    source.insert_str(marker_at, marker_records);

    let context_marker = if literal_hash_context {
        ORDINARY_HASH_MARKER_GUID
    } else {
        WORD_BOUNDARY_GUID
    };
    let context_record = format!(
        "<rt class=\"PhSimpleContextBdry\" guid=\"00000000-0000-0000-0000-000000000060\" ownerguid=\"00000000-0000-0000-0000-00000000001a\">\n<FeatureStructure><objsur guid=\"{context_marker}\" t=\"r\" /></FeatureStructure>\n</rt>\n"
    );
    let context_anchor = "<rt class=\"LexEntry\" guid=\"00000000-0000-0000-0000-000000000030\"";
    let context_at = source
        .find(context_anchor)
        .expect("fixture has its first lexical entry");
    source.insert_str(context_at, &context_record);

    let phoneme_features = r#"
<rt class="FsFeatStruc" guid="00000000-0000-0000-0000-000000000110" ownerguid="00000000-0000-0000-0000-000000000010"><FeatureSpecs><objsur guid="00000000-0000-0000-0000-000000000120" t="o" /></FeatureSpecs></rt>
<rt class="FsClosedValue" guid="00000000-0000-0000-0000-000000000120" ownerguid="00000000-0000-0000-0000-000000000110"><Feature><objsur guid="00000000-0000-0000-0000-000000000100" t="r" /></Feature><Value><objsur guid="00000000-0000-0000-0000-000000000101" t="r" /></Value></rt>
<rt class="FsFeatStruc" guid="00000000-0000-0000-0000-000000000111" ownerguid="00000000-0000-0000-0000-000000000012"><FeatureSpecs><objsur guid="00000000-0000-0000-0000-000000000121" t="o" /></FeatureSpecs></rt>
<rt class="FsClosedValue" guid="00000000-0000-0000-0000-000000000121" ownerguid="00000000-0000-0000-0000-000000000111"><Feature><objsur guid="00000000-0000-0000-0000-000000000100" t="r" /></Feature><Value><objsur guid="00000000-0000-0000-0000-000000000102" t="r" /></Value></rt>
<rt class="FsFeatStruc" guid="00000000-0000-0000-0000-000000000112" ownerguid="00000000-0000-0000-0000-00000000001b"><FeatureSpecs><objsur guid="00000000-0000-0000-0000-000000000122" t="o" /></FeatureSpecs></rt>
<rt class="FsClosedValue" guid="00000000-0000-0000-0000-000000000122" ownerguid="00000000-0000-0000-0000-000000000112"><Feature><objsur guid="00000000-0000-0000-0000-000000000100" t="r" /></Feature><Value><objsur guid="00000000-0000-0000-0000-000000000103" t="r" /></Value></rt>
"#;
    replace_exactly(
        &mut source,
        "</languageproject>",
        &format!("{phoneme_features}\n</languageproject>"),
        1,
    );

    let path = dir.join(if literal_hash_context {
        "literal-hash.fwdata"
    } else {
        "reserved-boundary.fwdata"
    });
    std::fs::write(&path, source).expect("write FWData variant");
    path
}

#[test]
fn reserved_word_boundary_record_is_a_word_anchor_and_literal_hash_remains_literal() {
    let dir = tempfile::tempdir().expect("create fixture directory");
    for (literal_hash_context, expected_context) in [
        (false, PhonContext::WordBoundary),
        (
            true,
            PhonContext::Boundary {
                marker: ORDINARY_HASH_MARKER_GUID.to_string(),
            },
        ),
    ] {
        let path = reserved_boundary_fixture(dir.path(), literal_hash_context);
        let (snapshot, _) = pg_fwdata::import_file(&path).expect("fixture imports");
        assert_eq!(snapshot.phonology.boundary_markers.len(), 2);
        assert!(snapshot
            .phonology
            .boundary_markers
            .iter()
            .any(|marker| marker.guid == MORPHEME_BOUNDARY_GUID));
        assert!(snapshot
            .phonology
            .boundary_markers
            .iter()
            .any(|marker| marker.guid == ORDINARY_HASH_MARKER_GUID
                && marker.representations.iter().any(|rep| rep.form == "#")));
        assert!(snapshot
            .phonology
            .boundary_markers
            .iter()
            .all(|marker| marker.guid != WORD_BOUNDARY_GUID));
        let PhonologicalRule::Rewrite(rule) = &snapshot.phonology.rules[0] else {
            panic!("fixture has one rewrite rule");
        };
        let rhs = &rule.right_hand_sides[0];
        assert_eq!(rhs.left_context.as_ref(), Some(&expected_context));
        assert!(matches!(
            rhs.right_context,
            Some(PhonContext::NaturalClass { .. })
        ));

        let (grammar, _) = pg_grammar::compile_project(&snapshot).expect("fixture compiles");
        let outcome = Morpher::new(&grammar, usize::MAX).parse_word("ta");
        if literal_hash_context {
            assert!(
                outcome.analyses.is_empty(),
                "a literal # does not match word edge"
            );
        } else {
            assert!(
                !outcome.analyses.is_empty(),
                "reserved # must parse prefix r + root a as ta"
            );
        }
    }
}
