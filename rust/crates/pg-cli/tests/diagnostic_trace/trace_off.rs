use std::io::ErrorKind;
use std::process::Command;

use super::{fs, success, Path, BIN, GRAMMAR};

const OPEN: &[u8] = b"diagnostic test: temporary opened\n";
const SUMMARY: &[u8] = b"rejection trace: ";

#[test]
fn ordinary_bytes_and_diagnostic_file_opens() {
    let temp = tempfile::tempdir().unwrap();
    let shim = temp.path().join("trace_off.so");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/diagnostic_trace/trace_off.c");
    match Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&shim)
        .arg(source)
        .arg("-ldl")
        .output()
    {
        Ok(compile) => success(&compile),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            eprintln!("diagnostic file-open audit needs a C compiler: {error}");
            return;
        }
        Err(error) => panic!("start diagnostic file-open audit compiler: {error}"),
    }
    let grammar = temp.path().join("grammar.xml");
    let words = temp.path().join("words");
    let tsv = temp.path().join("rows.tsv");
    let analyses = temp.path().join("analyses.jsonl");
    let artifact = temp.path().join("trace.jsonl");
    fs::write(&grammar, GRAMMAR).unwrap();
    fs::write(&words, "kib\nkibes\nzzz\nkib\n").unwrap();
    let gram = grammar.to_str().unwrap();
    let batch = [
        "batch",
        gram,
        words.to_str().unwrap(),
        tsv.to_str().unwrap(),
        "--threads",
        "1",
        "--work-columns",
        "--no-flush",
        "--analyses",
        analyses.to_str().unwrap(),
    ];
    for args in [&["parse", gram, "kib"][..], &batch[..]] {
        let run = |binary: &Path, traced: bool| {
            let mut command = Command::new(binary);
            command.args(args).env("LD_PRELOAD", &shim);
            if traced {
                command.arg("--trace-rejections").arg(&artifact);
            }
            let output = command.output().unwrap();
            success(&output);
            let lines: Vec<_> = output
                .stderr
                .split_inclusive(|&byte| byte == b'\n')
                .collect();
            assert_eq!(
                lines.iter().filter(|&&line| line == OPEN).count(),
                usize::from(traced)
            );
            assert_eq!(artifact.exists(), traced);
            let stderr: Vec<_> = lines
                .into_iter()
                .filter(|&line| line != OPEN && !line.starts_with(SUMMARY))
                .flatten()
                .copied()
                .collect();
            let sidecars = (args[0] == "batch")
                .then(|| (fs::read(&tsv).unwrap(), fs::read(&analyses).unwrap()));
            (output.status, output.stdout, stderr, sidecars)
        };
        let plain = run(Path::new(BIN), false);
        if let Some(baseline) = std::env::var_os("PANGLOSS_TRACE_OFF_BASELINE") {
            assert_eq!(run(Path::new(&baseline), false), plain);
        }
        assert_eq!(run(Path::new(BIN), true), plain);
        fs::remove_file(&artifact).unwrap();
    }
}

#[test]
#[ignore = "needs a pre-change optimized CLI and Valgrind Callgrind"]
fn evaluator_instruction_count_matches_the_trace_off_baseline() {
    let baseline = std::env::var_os("PANGLOSS_TRACE_OFF_BASELINE")
        .expect("set PANGLOSS_TRACE_OFF_BASELINE to a managed optimized pre-change CLI");
    let temp = tempfile::tempdir().unwrap();
    let grammar = temp.path().join("grammar.xml");
    let words = temp.path().join("words");
    fs::write(&grammar, GRAMMAR).unwrap();
    fs::write(
        &words,
        "kib\nkibes\nkibut\nkibon\nzzz\nfom\ntoy\n".repeat(400),
    )
    .unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/instruction-compare.ps1");
    let result = Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(script)
        .arg("-Before")
        .arg(baseline)
        .arg("-After")
        .arg(BIN)
        .arg("-Grammars")
        .arg(grammar)
        .arg("-Words")
        .arg(words)
        .args(["-Backend", "callgrind", "-Repeats", "2"])
        .output()
        .unwrap();
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(
        result.status.code() == Some(10) && output.contains("NO-GO (no change)"),
        "{output}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
