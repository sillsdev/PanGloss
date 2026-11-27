//! `batch --result-cache`: replays rows of an unchanged grammar, options and word instead of parsing them again.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const DEFAULT_MAX_MIB: u64 = 256;
const MAX_SEGMENTS: usize = 16;
/// Written in a cache root this cache created or adopted; eviction never touches a root without it.
const ROOT_MARKER: &str = ".pangloss-result-cache";
/// Written in every key directory this cache created; eviction removes only directories that hold it.
const KEY_MARKER: &str = ".pangloss-result-cache-key";
const ROW_PREFIX_HEAD: &str = "{\"schema\":\"fieldworks-parse-analysis/v1\",\"index\":";

/// Options that cannot be replayed from stored rows.
const REFUSED: &[&str] = &[
    "--stats",
    "--cache",
    "--partials",
    "--start",
    "--prefilter",
    "--engine",
    "--cost-column",
    "--phase-marks",
    "--trace-",
];

/// One cached word: its TSV row after the index, and its sidecar row after `"index":N,`.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Entry {
    w: String,
    t: String,
    a: String,
}

struct Plan<'a> {
    grammar: &'a str,
    words: &'a str,
    out: &'a str,
    analyses: String,
    root: PathBuf,
    dedicated_root: bool,
    max_bytes: u64,
    inner: Vec<String>,
    key_tokens: Vec<String>,
}

pub(crate) fn requested(args: &[String]) -> bool {
    args.iter()
        .any(|a| a == "--result-cache" || a.starts_with("--result-cache="))
}

fn default_root() -> Result<PathBuf, String> {
    let base = match std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => match std::env::var_os("HOME").filter(|v| !v.is_empty()) {
            Some(home) => PathBuf::from(home).join(".cache"),
            None => PathBuf::from(
                std::env::var_os("LOCALAPPDATA")
                    .filter(|v| !v.is_empty())
                    .ok_or("--result-cache auto needs XDG_CACHE_HOME, HOME or LOCALAPPDATA")?,
            ),
        },
    };
    Ok(base.join("pangloss").join("batch-results"))
}

pub(crate) fn refuse_env(set: Option<&str>) -> Result<(), String> {
    match set {
        Some(name) => Err(format!(
            "--result-cache cannot be used while {name} is set: it changes or profiles the parse, and the cache neither keys nor replays it"
        )),
        None => Ok(()),
    }
}

/// Every word has a row; checked before any output file is created or truncated.
pub(crate) fn require_rows<V>(words: &[String], have: &HashMap<String, V>) -> Result<(), String> {
    if words.iter().any(|w| !have.contains_key(w)) {
        return Err("batch produced fewer rows than words".into());
    }
    Ok(())
}

fn plan(args: &[String]) -> Result<Plan<'_>, String> {
    refuse_env(crate::parse_affecting_env_set_for_cache())?;
    let [grammar, words, out, rest @ ..] = args else {
        return Err("--result-cache needs the three positional arguments first".into());
    };
    if [grammar, words, out].iter().any(|a| a.starts_with("--")) {
        return Err(
            "--result-cache needs <grammar> <words.txt> <out.tsv> before any option".into(),
        );
    }
    let (mut root, mut max_mib, mut analyses) = (None, DEFAULT_MAX_MIB, None);
    let mut dedicated_root = false;
    let mut inner = Vec::new();
    let mut key_tokens = Vec::new();
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        if let Some(v) = crate::option_value(arg, "--result-cache-max-mib", &mut it)? {
            max_mib = v
                .parse()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| format!("invalid --result-cache-max-mib: {v}"))?;
        } else if let Some(v) = crate::option_value(arg, "--result-cache", &mut it)? {
            dedicated_root = v == "auto";
            root = Some(if v == "auto" {
                default_root()?
            } else {
                PathBuf::from(v)
            });
        } else if let Some(v) = crate::option_value(arg, "--analyses", &mut it)? {
            analyses = Some(v.to_string());
        } else if arg == "--threads" {
            inner.push(arg.clone());
            inner.push(it.next().ok_or("--threads requires a value")?.clone());
        } else if arg.starts_with("--threads=") || arg == "--no-flush" {
            inner.push(arg.clone());
        } else {
            if REFUSED.iter().any(|p| arg.starts_with(p)) {
                return Err(format!("--result-cache cannot be combined with {arg}"));
            }
            inner.push(arg.clone());
            key_tokens.push(arg.clone());
        }
    }
    let analyses = analyses.ok_or("--result-cache requires --analyses")?;
    crate::reject_analysis_path_collisions(grammar, words, out, &analyses, None)?;
    crate::reject_tsv_path_collisions(grammar, words, out)?;
    Ok(Plan {
        grammar,
        words,
        out,
        analyses,
        root: root.ok_or("--result-cache requires a directory or auto")?,
        dedicated_root,
        max_bytes: max_mib.saturating_mul(1 << 20),
        inner,
        key_tokens,
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 of the grammar file's current bytes and of every side input its import reads (a `.fwdata`'s sibling `WritingSystemStore/*.ldml`, by name).
fn grammar_digest(grammar: &str) -> Result<String, String> {
    let bytes = fs::read(grammar).map_err(|e| format!("--result-cache reads {grammar}: {e}"))?;
    let mut hasher = Sha256::new();
    hasher.update(Sha256::digest(&bytes));
    let side = pg_fwdata::import_side_inputs(Path::new(grammar)).map_err(|e| {
        format!("--result-cache reads the writing-system files beside {grammar}: {e}")
    })?;
    for (name, content) in side {
        hasher.update(b"\nside:");
        hasher.update(name.len().to_string());
        hasher.update(b":");
        hasher.update(name);
        hasher.update(Sha256::digest(&content));
    }
    Ok(hex(&hasher.finalize()))
}

/// SHA-256 of the running executable, so a rebuilt or locally modified binary never replays another build's rows.
fn executable_digest() -> Result<String, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("--result-cache cannot locate the running executable: {e}"))?;
    let bytes =
        fs::read(&exe).map_err(|e| format!("--result-cache reads {}: {e}", exe.display()))?;
    Ok(hex(&Sha256::digest(&bytes)))
}

/// The key covers the grammar's content, the executable's content and every option that is not a path or a thread count.
pub(crate) fn cache_key(
    grammar_digest: &str,
    executable_digest: &str,
    key_tokens: &[String],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"pangloss-batch-result-cache/2\n");
    hasher.update(env!("CARGO_PKG_VERSION"));
    hasher.update(b"\n");
    hasher.update(executable_digest);
    hasher.update(b"\n");
    hasher.update(grammar_digest);
    for token in key_tokens {
        hasher.update(b"\n");
        hasher.update(token);
    }
    hex(&hasher.finalize())
}

fn is_key_name(name: &str) -> bool {
    name.len() == 64 && name.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Prepares the root: creates it, or adopts it when it is empty or already ours. A non-empty root without the marker is someone else's folder, so nothing is written or removed there. The default folder (`auto`) is ours by construction, so a root from before the marker existed is adopted and its key directories marked.
fn prepare_root(root: &Path, dedicated: bool) -> Result<(), String> {
    let marker = root.join(ROOT_MARKER);
    if marker.is_file() {
        return Ok(());
    }
    let occupied = match fs::read_dir(root) {
        Ok(mut entries) => entries.next().is_some(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(format!("read {}: {e}", root.display())),
    };
    if occupied && !dedicated {
        return Err(format!(
            "--result-cache {} is not empty and is not a result cache (it has no {ROOT_MARKER} file); choose an empty or new folder, or `auto`",
            root.display()
        ));
    }
    fs::create_dir_all(root).map_err(|e| format!("create {}: {e}", root.display()))?;
    if occupied {
        for entry in fs::read_dir(root).into_iter().flatten().flatten() {
            let path = entry.path();
            let ours = entry.file_type().is_ok_and(|t| t.is_dir())
                && is_key_name(&entry.file_name().to_string_lossy())
                && fs::read_dir(&path).is_ok_and(|files| {
                    files.flatten().all(|f| {
                        let name = f.file_name().to_string_lossy().into_owned();
                        name == ".used"
                            || Path::new(&name)
                                .extension()
                                .is_some_and(|x| x == "jsonl" || x == "tmp")
                    })
                });
            if ours {
                let _ = fs::write(path.join(KEY_MARKER), b"");
            }
        }
    }
    fs::write(&marker, b"").map_err(|e| format!("write {}: {e}", marker.display()))
}

fn segments(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    found.sort();
    found
}

/// Removes `.tmp` segments a killed run left behind, once they are an hour old.
fn sweep_stale_tmp(dir: &Path) {
    let hour = std::time::Duration::from_secs(3600);
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let stale = path.extension().is_some_and(|x| x == "tmp")
            && entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > hour);
        if stale {
            let _ = fs::remove_file(path);
        }
    }
}

fn load(dir: &Path) -> (HashMap<String, Entry>, Vec<PathBuf>) {
    let files = segments(dir);
    let mut map = HashMap::new();
    for file in &files {
        for line in fs::read_to_string(file).unwrap_or_default().lines() {
            if let Ok(entry) = serde_json::from_str::<Entry>(line) {
                map.insert(entry.w.clone(), entry);
            }
        }
    }
    (map, files)
}

fn write_segment(dir: &Path, entries: &[&Entry], name: &str) -> Result<u64, String> {
    let tmp = dir.join(format!("{name}.tmp"));
    let mut file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut size = 0;
    for entry in entries {
        let mut line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
        line.push('\n');
        size += line.len() as u64;
        file.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    }
    file.flush().map_err(|e| e.to_string())?;
    fs::rename(&tmp, dir.join(format!("{name}.jsonl"))).map_err(|e| e.to_string())?;
    Ok(size)
}

fn dir_size(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Removes the least recently used key directories this cache created (a 64-hex name holding its marker) until the cache fits, never `keep`.
fn evict(root: &Path, keep: &Path, max_bytes: u64) {
    let mut dirs: Vec<(std::time::SystemTime, u64, PathBuf)> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_type().is_ok_and(|t| t.is_dir())
                && is_key_name(&e.file_name().to_string_lossy())
                && e.path().join(KEY_MARKER).is_file()
        })
        .map(|e| {
            let path = e.path();
            let used = fs::metadata(path.join(".used"))
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (used, dir_size(&path), path)
        })
        .collect();
    let mut total: u64 = dirs.iter().map(|d| d.1).sum();
    dirs.sort();
    for (_, size, path) in dirs {
        if total <= max_bytes {
            break;
        }
        if path != keep && fs::remove_dir_all(&path).is_ok() {
            total -= size;
        }
    }
}

/// Removes a key directory that holds no stored rows, so a run that stored nothing leaves nothing.
fn drop_if_empty(dir: &Path) {
    if segments(dir).is_empty() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn split_analysis(line: &str) -> Option<(usize, &str)> {
    let rest = line.strip_prefix(ROW_PREFIX_HEAD)?;
    let (index, tail) = rest.split_once(',')?;
    Some((index.parse().ok()?, tail))
}

pub(crate) fn run(args: &[String], counter: &crate::BatchParseCounter) -> Result<(), String> {
    let plan = plan(args)?;
    let grammar_before = grammar_digest(plan.grammar)?;
    let key = cache_key(&grammar_before, &executable_digest()?, &plan.key_tokens);
    let dir = plan.root.join(&key);
    prepare_root(&plan.root, plan.dedicated_root)?;
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let _ = fs::write(dir.join(KEY_MARKER), b"");
    let _ = fs::write(dir.join(".used"), b"");
    sweep_stale_tmp(&dir);
    let source = fs::read_to_string(plan.words).map_err(|e| format!("read {}: {e}", plan.words))?;
    let words: Vec<String> = source
        .lines()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect();
    let (cached, old_files) = load(&dir);
    let mut have: HashMap<String, Entry> = HashMap::new();
    let mut misses: Vec<&String> = Vec::new();
    let mut miss_set: std::collections::HashSet<&String> = std::collections::HashSet::new();
    for word in &words {
        if let Some(entry) = cached.get(word) {
            have.insert(word.clone(), entry.clone());
        } else if miss_set.insert(word) {
            misses.push(word);
        }
    }
    let mut fresh: Vec<Entry> = Vec::new();
    if !misses.is_empty() {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let (tmp_words, tmp_out, tmp_an) = (
            tmp.path().join("words.txt"),
            tmp.path().join("out.tsv"),
            tmp.path().join("analyses.jsonl"),
        );
        let list: String = misses.iter().map(|w| format!("{w}\n")).collect();
        fs::write(&tmp_words, list).map_err(|e| e.to_string())?;
        let mut inner = vec![
            plan.grammar.to_string(),
            tmp_words.to_string_lossy().into_owned(),
            tmp_out.to_string_lossy().into_owned(),
            "--analyses".to_string(),
            tmp_an.to_string_lossy().into_owned(),
        ];
        inner.extend(plan.inner.iter().cloned());
        crate::run_batch_with_counter(&inner, &crate::BatchParseCounter::default())?;
        for _ in &misses {
            counter.record();
        }
        let mut tsv: HashMap<usize, String> = HashMap::new();
        let out_text = fs::read_to_string(&tmp_out).map_err(|e| e.to_string())?;
        for line in out_text.lines() {
            let fields: Vec<&str> = line.splitn(4, '\t').collect();
            if fields.len() == 3 && fields[2] == "STARTED" {
                continue;
            }
            if let (Some(idx), Some((_, rest))) = (fields.first(), line.split_once('\t')) {
                if let Ok(idx) = idx.parse() {
                    tsv.insert(idx, rest.to_string());
                }
            }
        }
        let an_text = fs::read_to_string(&tmp_an).map_err(|e| e.to_string())?;
        for line in an_text.lines() {
            let (idx, tail) = split_analysis(line).ok_or("unreadable sidecar row")?;
            let row: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
            let (Some(word), Some(t)) = (misses.get(idx), tsv.get(&idx)) else {
                return Err(format!("batch row {idx} has no matching TSV row"));
            };
            let entry = Entry {
                w: (*word).clone(),
                t: t.clone(),
                a: tail.to_string(),
            };
            // A timeout depends on the clock, so it is reported but never replayed.
            if row["timedOut"] != true {
                fresh.push(entry.clone());
            }
            have.insert(entry.w.clone(), entry);
        }
    }
    require_rows(&words, &have)?;
    let mut out = std::io::BufWriter::new(
        fs::File::create(plan.out).map_err(|e| format!("create {}: {e}", plan.out))?,
    );
    let mut sidecar = std::io::BufWriter::new(
        fs::File::create(&plan.analyses).map_err(|e| format!("create {}: {e}", plan.analyses))?,
    );
    for (i, word) in words.iter().enumerate() {
        let entry = have.get(word).ok_or("missing cached row")?;
        writeln!(out, "{i}\t{}", entry.t).map_err(|e| e.to_string())?;
        writeln!(sidecar, "{ROW_PREFIX_HEAD}{i},{}", entry.a).map_err(|e| e.to_string())?;
    }
    out.flush().map_err(|e| e.to_string())?;
    sidecar.flush().map_err(|e| e.to_string())?;
    let parsed_rows = words.iter().filter(|w| miss_set.contains(w)).count();
    eprintln!(
        "RESULTCACHE\thit_rows={}\tparsed_rows={parsed_rows}\tnew_entries={}\tkey={key}",
        words.len() - parsed_rows,
        fresh.len()
    );
    if !fresh.is_empty() {
        match grammar_digest(plan.grammar) {
            Ok(after) if after == grammar_before => {}
            Ok(_) => {
                eprintln!(
                    "RESULTCACHE\tskipped: the grammar changed during the run, so nothing was stored"
                );
                fresh.clear();
            }
            Err(e) => {
                eprintln!("RESULTCACHE\twarning: could not re-read the grammar, so nothing was stored: {e}");
                fresh.clear();
            }
        }
    }
    if !fresh.is_empty() {
        let refs: Vec<&Entry> = fresh.iter().collect();
        let name = format!(
            "seg-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let size = match write_segment(&dir, &refs, &name) {
            Ok(size) => size,
            Err(e) => {
                let _ = fs::remove_file(dir.join(format!("{name}.tmp")));
                eprintln!(
                    "RESULTCACHE\twarning: could not store {} rows: {e}",
                    refs.len()
                );
                drop_if_empty(&dir);
                evict(&plan.root, &dir, plan.max_bytes);
                return Ok(());
            }
        };
        if dir_size(&dir) > plan.max_bytes {
            let _ = fs::remove_file(dir.join(format!("{name}.jsonl")));
            eprintln!("RESULTCACHE\tskipped: {size} new bytes exceed --result-cache-max-mib");
        } else if old_files.len() + 1 > MAX_SEGMENTS {
            let (merged, files) = load(&dir);
            let all: Vec<&Entry> = merged.values().collect();
            if write_segment(&dir, &all, &format!("seg-merged-{name}")).is_ok() {
                for file in files {
                    let _ = fs::remove_file(file);
                }
            }
        }
    }
    drop_if_empty(&dir);
    evict(&plan.root, &dir, plan.max_bytes);
    Ok(())
}
