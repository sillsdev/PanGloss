use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pg_conformance_fixtures::render_adapter_tsv;

#[derive(Default)]
struct Paths {
    grammar: Option<PathBuf>,
    words: Option<PathBuf>,
    output: Option<PathBuf>,
    morpher: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run(std::env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pg-conformance-adapter: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let mut args = args.peekable();
    let mut paths = Paths::default();
    while let Some(arg) = args.next() {
        let flag = arg
            .to_str()
            .ok_or_else(|| format!("non-UTF-8 option name {arg:?}"))?;
        let slot = match flag {
            "--grammar" => &mut paths.grammar,
            "--words" => &mut paths.words,
            "--output" => &mut paths.output,
            "--morpher" => &mut paths.morpher,
            _ => return Err(format!("unknown argument {flag:?}")),
        };
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} requires a path"))?;
        if slot.is_some() {
            return Err(format!("{flag} was specified more than once"));
        }
        *slot = Some(PathBuf::from(value));
    }

    let grammar_path = required(paths.grammar, "--grammar")?;
    let words_path = required(paths.words, "--words")?;
    let output_path = required(paths.output, "--output")?;
    let morpher_path = required(paths.morpher, "--morpher")?;

    let grammar = read(&grammar_path, "grammar")?;
    let words = read(&words_path, "words")?;
    let morpher = read(&morpher_path, "morpher JSON")?;
    let tsv = render_adapter_tsv(&grammar, &words, &morpher)?;
    std::fs::write(&output_path, tsv)
        .map_err(|error| format!("write {}: {error}", output_path.display()))
}

fn required(path: Option<PathBuf>, flag: &str) -> Result<PathBuf, String> {
    path.ok_or_else(|| format!("required argument {flag} is missing"))
}

fn read(path: &Path, description: &str) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("read {description} {}: {error}", path.display()))
}
