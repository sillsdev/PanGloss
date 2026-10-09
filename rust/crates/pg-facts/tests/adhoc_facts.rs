use std::path::Path;

use pg_facts::ProducerIdentity;
use rusqlite::Connection;

const MORPH_DATA_GUID: &str = "5a540ed8-d95e-4a2b-805b-33b98c08c438";
const GROUP_GUID: &str = "00000000-0000-0000-0000-000000000060";
const RULE_GUID: &str = "00000000-0000-0000-0000-000000000061";
const NESTED_GROUP_GUID: &str = "00000000-0000-0000-0000-000000000065";
const PRIMARY_MSA_GUID: &str = "d71a9c35-5dd1-4659-997a-b3ad043e06f2";
const OTHER_MSA_GUID: &str = "74f02ea2-b426-42b3-b564-4b82b48fb845";

fn copy_writing_system_store(base_path: &Path, destination: &Path) {
    let source = base_path.parent().unwrap().join("WritingSystemStore");
    let destination = destination.join("WritingSystemStore");
    std::fs::create_dir_all(&destination).unwrap();
    let mut copied = 0;
    for item in std::fs::read_dir(source).unwrap() {
        let item = item.unwrap().path();
        if item.extension().and_then(|extension| extension.to_str()) == Some("ldml") {
            std::fs::copy(&item, destination.join(item.file_name().unwrap())).unwrap();
            copied += 1;
        }
    }
    assert!(copied > 0, "F1 fixture has WritingSystemStore LDML files");
}

#[test]
fn grouped_fwdata_rationale_reaches_the_facts_artifact() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../../machine/conformance/edge-cases/deep-optional-affix-nesting/fieldworks/project.fwdata",
    );
    let mut source = std::fs::read_to_string(&fixture).expect("read the F1 grouped-rule base");
    let morph_data_start = source
        .find("<rt class=\"MoMorphData\"")
        .expect("F1 fixture has MorphologicalData");
    let morph_data_end = morph_data_start
        + source[morph_data_start..]
            .find("</rt>")
            .expect("MorphologicalData closes");
    source.insert_str(
        morph_data_end,
        &format!(
            "<AdhocCoProhibitions><objsur guid=\"{GROUP_GUID}\" t=\"o\" /></AdhocCoProhibitions>"
        ),
    );
    let records = format!(
        r#"
<rt class="MoMorphAdhocProhib" guid="{RULE_GUID}" ownerguid="{NESTED_GROUP_GUID}">
<Adjacency val="0" /><Disabled val="False" />
<FirstMorpheme><objsur guid="{PRIMARY_MSA_GUID}" t="r" /></FirstMorpheme>
<RestOfMorphs><objsur guid="{OTHER_MSA_GUID}" t="r" /></RestOfMorphs>
</rt>
<rt class="MoAdhocProhibGr" guid="{GROUP_GUID}" ownerguid="{MORPH_DATA_GUID}">
<Name><AUni ws="en">Group rationale</AUni><AUni ws="es">Razon</AUni></Name>
<Description><AStr ws="en"><Run ws="en">English explanation</Run></AStr><AStr ws="es"><Run ws="es">Explicacion local</Run></AStr></Description>
<Members><objsur guid="{NESTED_GROUP_GUID}" t="o" /><objsur guid="{RULE_GUID}" t="o" /></Members>
</rt>
<rt class="MoAdhocProhibGr" guid="{NESTED_GROUP_GUID}" ownerguid="{GROUP_GUID}">
<Name><AUni ws="en">Nested rationale</AUni></Name>
<Description><AStr ws="en"><Run ws="en">Nested explanation</Run></AStr></Description>
<Members><objsur guid="{RULE_GUID}" t="o" /></Members>
</rt>
"#
    );
    let project_end = source
        .rfind("</languageproject>")
        .expect("F1 fixture closes its project");
    source.insert_str(project_end, &records);

    let temp = tempfile::tempdir().unwrap();
    copy_writing_system_store(&fixture, temp.path());
    let fwdata_path = temp.path().join("grouped.fwdata");
    std::fs::write(&fwdata_path, source).expect("write grouped F1 fixture");
    let (snapshot, _) = pg_fwdata::import_file(&fwdata_path).expect("grouped F1 fixture imports");
    let groups = snapshot
        .morphology
        .adhoc_prohibition_groups
        .as_ref()
        .expect("current importer publishes ad hoc groups");
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].guid, GROUP_GUID);
    assert_eq!(
        groups[0].members,
        vec![RULE_GUID.to_string(), NESTED_GROUP_GUID.to_string()]
    );
    assert_eq!(groups[0].name.len(), 2);
    assert_eq!(groups[0].description.len(), 2);
    assert_eq!(groups[1].guid, NESTED_GROUP_GUID);
    assert_eq!(groups[1].members, vec![RULE_GUID.to_string()]);

    let context = br#"{"format":"pangloss-facts-context","version":1,"baselineToken":{"id":"grouped-fwdata"},"inputKind":"baseline","dryRunDigest":null}"#;
    let output_path = temp.path().join("grammar-facts.sqlite");
    let facts = pg_facts::write_facts(
        snapshot.to_json().as_bytes(),
        context,
        &output_path,
        ProducerIdentity {
            compiler_version: "0.6.1",
            source_revision: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            build_identity: "pangloss/0.6.1+aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        },
    )
    .expect("facts export succeeds");
    assert_eq!(facts.schema_version, 8);

    let db = Connection::open(output_path).unwrap();
    let group_count: i64 = db
        .query_row("SELECT COUNT(*) FROM adhoc_group", [], |row| row.get(0))
        .unwrap();
    assert_eq!(group_count, 2);
    let (kind, primary, target_kind): (String, String, String) = db
        .query_row(
            "SELECT kind, primary_guid, target_kind FROM adhoc_prohibition WHERE prohibition_guid=?1",
            [RULE_GUID],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (kind.as_str(), primary.as_str(), target_kind.as_str()),
        ("morpheme", PRIMARY_MSA_GUID, "msa")
    );
    let (field, writing_system, text): (String, String, String) = db
        .query_row(
            "SELECT field, writing_system, text FROM adhoc_group_text WHERE group_guid=?1 AND writing_system='es' AND field='description'",
            [GROUP_GUID],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (field.as_str(), writing_system.as_str(), text.as_str()),
        ("description", "es", "Explicacion local")
    );
    let member_guids: Vec<String> = db
        .prepare(
            "SELECT member_guid FROM adhoc_group_member WHERE group_guid=?1 ORDER BY member_guid",
        )
        .unwrap()
        .query_map([GROUP_GUID], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        member_guids,
        vec![RULE_GUID.to_string(), NESTED_GROUP_GUID.to_string()]
    );
    let nested_member: String = db
        .query_row(
            "SELECT member_guid FROM adhoc_group_member WHERE group_guid=?1",
            [NESTED_GROUP_GUID],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(nested_member, RULE_GUID);
    let section_status: String = db
        .query_row(
            "SELECT status FROM artifact_section WHERE section='adhoc_groups'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(section_status, "complete");
}
