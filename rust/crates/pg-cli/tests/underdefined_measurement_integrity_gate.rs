//! Native measurements bind exact project bytes, independently of checkout line endings.

use std::path::Path;

use serde_json::Value;

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("read measurement evidence"))
        .expect("measurement evidence JSON")
}

fn assert_recorded_hash(path: &Path, expected: &Value) {
    let actual = pg_assess::sha256_bytes(&std::fs::read(path).expect("read measured source"));
    assert_eq!(
        actual,
        format!("sha256:{}", expected.as_str().expect("recorded SHA-256")),
        "native measurement bytes changed: {}",
        path.display()
    );
}

#[test]
fn measured_underdefined_projects_preserve_native_hashes_and_word_observations() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../conformance-staging/underdefined");
    let mut cases = std::fs::read_dir(&root)
        .expect("measured staging exists")
        .map(|entry| entry.expect("read staged case").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    cases.sort();
    assert!(!cases.is_empty(), "missing evidence cannot pass");
    for case in cases {
        let measurement = read_json(&case.join("measurement.json"));
        let author = read_json(&case.join("author-response.json"));
        assert_recorded_hash(
            &case.join("fieldworks-authored/project.fwdata"),
            &author["projectSha256"],
        );
        assert_recorded_hash(&case.join("grammar.xml"), &author["grammarSha256"]);
        for (mode, directory) in [
            ("control", "fieldworks-control"),
            ("off", "fieldworks"),
            ("on", "fieldworks-on"),
        ] {
            let expected = &measurement["projectSha256"][mode];
            assert_recorded_hash(&case.join(directory).join("project.fwdata"), expected);
            for (name, hashes) in measurement["writingSystemSha256"][mode]
                .as_object()
                .expect("recorded writing systems")
            {
                assert_recorded_hash(
                    &case.join(directory).join("WritingSystemStore").join(name),
                    &hashes["stagedSha256"],
                );
            }
            for engine_file in ["response.json", "xample.json", "hc.json"] {
                let capture = read_json(&case.join("measurements").join(mode).join(engine_file));
                assert_eq!(
                    &capture["sourceSha256"], expected,
                    "engine capture must name the measured {mode} project"
                );
            }
        }
        let words = std::fs::read_to_string(case.join("words.yaml")).expect("read staged words");
        let words = words
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        let words: Value = serde_json::from_str(&words).expect("stored-key words schema");
        let words = words["words"].as_array().expect("staged word rows");
        let observed = measurement["modes"]["off"]["words"]
            .as_array()
            .expect("native OFF word rows");
        assert_eq!(words.len(), observed.len());
        for (word, observed) in words.iter().zip(observed) {
            for field in ["word", "xample", "csharp_hc"] {
                assert_eq!(
                    word[field], observed[field],
                    "staged {field} must retain the native OFF observation"
                );
            }
            assert_eq!(
                word["pangloss"]["minimum_stored_analysis_keys"],
                observed["xample"]["stored_analysis_keys"],
                "minimum must retain every recorded XAMPLE key"
            );
        }
    }
}
