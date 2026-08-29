use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LockDocument {
    schema: String,
    schema_version: u32,
    languages: Vec<LanguageLock>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LanguageLock {
    language: String,
    case_set_id: String,
    source: String,
    source_sha256: String,
    grammar_source: String,
    grammar_sha256: String,
    declared_count: usize,
    selected_backend: String,
    selection_policy: String,
    cases: Vec<CaseLock>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CaseLock {
    case_id: String,
    source_line: usize,
}

fn lock_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/three-language-case-sets.json")
}

fn load() -> LockDocument {
    let path = lock_path();
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "read privacy-safe case-set lock {}: {error}",
            path.display()
        )
    });
    serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "parse privacy-safe case-set lock {}: {error}",
            path.display()
        )
    })
}

fn expected_amharic_lines() -> Vec<usize> {
    (5..=62)
        .chain(66..=70)
        .chain(76..=90)
        .chain(92..=104)
        .chain(106..=214)
        .collect()
}

fn expected_aweti_lines() -> Vec<usize> {
    vec![
        1, 2, 3, 5, 6, 7, 8, 9, 10, 12, 15, 18, 19, 22, 23, 24, 30, 31, 32, 33, 35, 41, 45, 48, 49,
        50, 53, 55, 58, 59, 62, 64, 65, 66, 67, 69, 75, 76, 78, 79, 80, 82, 87, 88, 90, 92, 93, 99,
        100, 103, 104, 106, 109, 111, 113, 114, 115, 116, 117, 118, 119, 122, 124, 126, 133, 135,
        137, 139, 140, 144, 145, 146, 148, 150, 151, 157, 158, 159, 160, 163, 164, 165, 166, 167,
        168, 169, 170, 177, 178, 182, 183, 184, 185, 186, 187, 188, 191, 192, 197, 198, 203, 204,
        205, 206, 207, 208,
    ]
}

#[test]
fn locks_private_safe_three_language_denominators_and_routes() {
    let lock = load();
    assert_eq!(lock.schema, "pangloss.private-case-set-lock");
    assert_eq!(lock.schema_version, 1);
    assert_eq!(lock.languages.len(), 3);

    let expected = [
        (
            "indonesian",
            "indonesian-valid-120-v1",
            "indonesian-words.txt",
            "sha256:004d6aa362b8c4fbbed863ac5ac580c4a8609bf3a2017b95732233668c19ba73",
            "indonesian-hc.xml",
            "sha256:e450110eac48a80d802b46d8162b3e832e4acbb615395fd34cf6eab3ba8c9cd3",
            "tuned-surface-probed",
            "all alphabet-valid source lines; line 100 is a complete empty semantic set; line 119 is reported separately as invalid shape",
            (1..=118).chain(120..=121).collect::<Vec<_>>(),
        ),
        (
            "amharic",
            "amharic-200-v1",
            "amharic-words.txt",
            "sha256:33124870ea1ef148759220c3dbf513042682934e94131443bb4b4e53db6ee57e",
            "amharic-hc.xml",
            "sha256:d5156ea82c6c92c169fd6cabcab1ebfb19ec5d95e9d8dea21938ff421f5fff70",
            "templated-underlying-tokens",
            "first 200 alphabet-encodable source lines after the four-line header",
            expected_amharic_lines(),
        ),
        (
            "aweti",
            "aweti-oracle-bearing-106-v1",
            "aweti-words.txt",
            "sha256:e888ce23f92638ddd5a3d5a21e5f88f645d51ccf1d27ad6cf2e42e21dc58b4ec",
            "aweti.json",
            "sha256:f4d5426f177b8c296f5ba78068d0591a72a9f081e330df81dedc8c0455cc31f6",
            "templated-underlying-tokens",
            "historical 106 oracle-bearing source lines; current exact complete analysis sets remain required",
            expected_aweti_lines(),
        ),
    ];

    for (language, expected) in lock.languages.iter().zip(expected) {
        let (name, set_id, source, source_hash, grammar, grammar_hash, backend, policy, lines) =
            expected;
        assert_eq!(language.language, name);
        assert_eq!(language.case_set_id, set_id);
        assert_eq!(language.source, source);
        assert_eq!(language.source_sha256, source_hash);
        assert_eq!(language.grammar_source, grammar);
        assert_eq!(language.grammar_sha256, grammar_hash);
        assert_eq!(language.selected_backend, backend);
        assert_eq!(language.selection_policy, policy);
        assert_eq!(language.declared_count, lines.len());
        assert_eq!(language.cases.len(), lines.len());
        assert_eq!(
            language
                .cases
                .iter()
                .map(|case| case.source_line)
                .collect::<Vec<_>>(),
            lines
        );
        for case in &language.cases {
            assert_eq!(
                case.case_id,
                format!("{}-line-{:04}", language.language, case.source_line)
            );
        }
    }
}
