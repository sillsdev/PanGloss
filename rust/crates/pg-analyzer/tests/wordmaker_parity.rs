use std::{fs, path::Path, process::Command};

#[test]
fn pinned_wordmaker_reference_and_negative_controls() {
    let harness =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dotnet-harness/WordMakerParity");
    let cases = harness.join("cases.json");
    let reference = harness.join("reference-icu70.json");
    let run = |path: &Path| {
        Command::new(env!("CARGO_BIN_EXE_parity"))
            .arg(&cases)
            .arg(path)
            .status()
            .unwrap()
            .success()
    };
    assert!(run(&reference), "pinned WordMaker reference must agree");
    let temporary = std::env::temp_dir().join(format!("pg-wordmaker-{}", std::process::id()));
    fs::create_dir_all(&temporary).unwrap();
    assert!(!run(&temporary.join("missing.json")));
    let mut data: serde_json::Value =
        serde_json::from_slice(&fs::read(reference).unwrap()).unwrap();
    data["cases"][0]["words"][0]["end"] = 999.into();
    let altered = temporary.join("altered.json");
    fs::write(&altered, serde_json::to_vec(&data).unwrap()).unwrap();
    assert!(!run(&altered), "altered boundary must fail");
    fs::remove_dir_all(temporary).unwrap();
}
