//! Every pruned field, injected into real records, must leave the import output unchanged.

use super::PRUNED_FIELDS;

const TAIL: &str = "</languageproject>";

fn fixture() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fixture.fwdata"),
    )
    .unwrap()
}

fn fields_for(class: &str, inject: bool) -> String {
    if !inject {
        return String::new();
    }
    PRUNED_FIELDS
        .iter()
        .filter(|(c, _)| *c == class)
        .map(|(_, f)| {
            format!("<{f} val=\"1\"><AUni ws=\"en\">x</AUni><Run ws=\"en\">y</Run></{f}>")
        })
        .collect()
}

/// Fixture records of every class that has pruned fields, with or without those fields.
fn variant(inject: bool) -> String {
    let mut out = String::new();
    let mut rest = fixture();
    for class in ["LexEntry", "LexSense"] {
        let open = format!("<rt class=\"{class}\" ");
        let mut done = String::new();
        while let Some(at) = rest.find(&open) {
            let end = at + rest[at..].find('>').unwrap() + 1;
            done.push_str(&rest[..end]);
            done.push_str(&fields_for(class, inject));
            rest = rest[end..].to_string();
        }
        done.push_str(&rest);
        rest = done;
    }
    let extra = format!(
        "<rt class=\"LexEntryRef\" guid=\"00000000-0000-0000-0000-0000000000f1\" ownerguid=\"00000000-0000-0000-0000-000000000030\">{}</rt>\n\
         <rt class=\"WfiWordform\" guid=\"00000000-0000-0000-0000-0000000000f2\">{}</rt>\n",
        fields_for("LexEntryRef", inject),
        fields_for("WfiWordform", inject),
    );
    out.push_str(&rest.replacen(TAIL, &format!("{extra}{TAIL}"), 1));
    out
}

#[test]
fn injecting_every_pruned_field_changes_neither_snapshot_nor_report() {
    // Same file stem in two directories: the stem becomes the project name.
    let dir = tempfile::tempdir().unwrap();
    let (plain_dir, injected_dir) = (dir.path().join("a"), dir.path().join("b"));
    std::fs::create_dir_all(&plain_dir).unwrap();
    std::fs::create_dir_all(&injected_dir).unwrap();
    let plain = plain_dir.join("p.fwdata");
    let injected = injected_dir.join("p.fwdata");
    std::fs::write(&plain, variant(false)).unwrap();
    std::fs::write(&injected, variant(true)).unwrap();
    let text = variant(true);
    for (class, field) in PRUNED_FIELDS {
        assert!(
            text.contains(&format!("<{field} val=")),
            "{class}.{field} was not injected"
        );
    }

    for lean in [false, true] {
        let load = |path: &std::path::Path| {
            if lean {
                crate::import_file_lean(path).unwrap()
            } else {
                crate::import_file(path).unwrap()
            }
        };
        let (plain_snapshot, plain_report) = load(&plain);
        let (injected_snapshot, injected_report) = load(&injected);
        assert!(
            plain_snapshot == injected_snapshot,
            "snapshot differs, lean={lean}"
        );
        assert!(
            plain_report == injected_report,
            "report differs, lean={lean}"
        );
    }
}
