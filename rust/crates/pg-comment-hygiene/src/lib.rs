use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use regex::Regex;
use serde::{Deserialize, Serialize};

pub const CATEGORY_ORDER: [&str; 10] = [
    "plan-reference",
    "step-marker",
    "wiring-status",
    "date-in-comment",
    "history-prose",
    "impl-comment-too-long",
    "unanchored-exception",
    "cross-reference-claim",
    "docs-link-broken",
    "dead-citation",
];

const MAX_BLOCK_LINES: usize = 3;

#[derive(Debug)]
pub enum ScanError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    InvalidRoot(PathBuf),
    InvalidUtf8 {
        path: PathBuf,
        source: std::string::FromUtf8Error,
    },
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::InvalidRoot(path) => {
                write!(f, "repository root does not exist: {}", path.display())
            }
            Self::InvalidUtf8 { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for ScanError {}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub counts: BTreeMap<String, usize>,
    pub hits: BTreeMap<String, Vec<String>>,
    pub total: usize,
    pub api_docs_long: usize,
    pub reference_backed: usize,
    pub claimed_exceptions: BTreeMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub list: bool,
    pub list_limit: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            list: false,
            list_limit: 400,
        }
    }
}

#[derive(Clone, Copy)]
struct SourceFile {
    extension: &'static str,
}

const RUST: SourceFile = SourceFile { extension: ".rs" };
const POWERSHELL: SourceFile = SourceFile { extension: ".ps1" };
const PYTHON: SourceFile = SourceFile { extension: ".py" };

#[derive(Clone)]
struct FileData {
    path: PathBuf,
    relative: String,
    source: SourceFile,
    lines: Vec<String>,
}

fn io(path: &Path, source: std::io::Error) -> ScanError {
    ScanError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn read_lines(path: &Path, source: SourceFile) -> Result<FileData, ScanError> {
    let bytes = fs::read(path).map_err(|e| io(path, e))?;
    let text = String::from_utf8(bytes).map_err(|e| ScanError::InvalidUtf8 {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(FileData {
        path: path.to_path_buf(),
        relative: String::new(),
        source,
        lines: text
            .strip_prefix('\u{feff}')
            .unwrap_or(&text)
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .lines()
            .map(str::to_owned)
            .collect(),
    })
}

fn walk_files(
    root: &Path,
    out: &mut Vec<(PathBuf, SourceFile)>,
    dir: &Path,
    recursive: bool,
    source: SourceFile,
) -> Result<(), ScanError> {
    let entries = fs::read_dir(dir).map_err(|e| io(dir, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| io(dir, e))?;
        let path = entry.path();
        if path.components().any(|part| {
            part.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case("target")
        }) {
            continue;
        }
        let kind = entry.file_type().map_err(|e| io(&path, e))?;
        if kind.is_dir() && recursive {
            walk_files(root, out, &path, true, source)?;
        } else if kind.is_file()
            && path
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case(&source.extension[1..]))
        {
            out.push((path, source));
        }
    }
    let _ = root;
    Ok(())
}

fn collect_files(root: &Path) -> Result<Vec<FileData>, ScanError> {
    if !root.is_dir() {
        return Err(ScanError::InvalidRoot(root.to_path_buf()));
    }
    let mut paths = Vec::new();
    let crates = root.join("rust").join("crates");
    let tools = root.join("rust").join("tools");
    let hooks = root.join(".claude").join("hooks");
    walk_files(root, &mut paths, &crates, true, RUST)?;
    walk_files(root, &mut paths, &tools, true, POWERSHELL)?;
    if hooks.is_dir() {
        walk_files(root, &mut paths, &hooks, false, PYTHON)?;
    }
    paths.sort_by(|a, b| a.0.cmp(&b.0));
    let mut files = Vec::with_capacity(paths.len());
    for (path, source) in paths {
        let mut file = read_lines(&path, source)?;
        file.relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        files.push(file);
    }
    Ok(files)
}

fn regex(pattern: &str) -> Arc<Regex> {
    thread_local! {
        static CACHE: std::cell::RefCell<std::collections::HashMap<String, Arc<Regex>>> = Default::default();
    }
    CACHE.with(|cache| {
        cache
            .borrow_mut()
            .entry(pattern.to_owned())
            .or_insert_with(|| Arc::new(Regex::new(pattern).expect("checker regex")))
            .clone()
    })
}
fn regex_i(pattern: &str) -> Arc<Regex> {
    regex(&format!("(?i:{pattern})"))
}

fn has_left_boundaried(text: &str, pattern: &Regex) -> bool {
    pattern.find_iter(text).any(|m| {
        m.start() == 0
            || text[..m.start()]
                .chars()
                .next_back()
                .is_none_or(|c| !regex(r"^[\w-]$").is_match(&c.to_string()))
    })
}

fn line_categories(line: &str) -> Vec<&'static str> {
    static OPENSPEC: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    static TASKS: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    static STEP: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    static PHASE: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    static STAGE: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    static WIRING: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    static DATE: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    static HISTORY: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    static DOCS_PLAN: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    static READINESS: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    let openspec = OPENSPEC.get_or_init(|| regex_i(r"openspec[/\\]changes"));
    let tasks = TASKS.get_or_init(|| regex_i(r"(?:tasks|design|spec)\.md"));
    let mut result = Vec::new();
    if openspec.is_match(line)
        || has_left_boundaried(line, tasks)
        || DOCS_PLAN
            .get_or_init(|| regex_i(r"docs/fst-plan"))
            .is_match(line)
        || READINESS
            .get_or_init(|| regex_i(r"IMPLEMENTATION-READINESS"))
            .is_match(line)
    {
        result.push("plan-reference");
    }
    let step = STEP.get_or_init(|| {
        regex_i(r"Step \d+ of \d+|Step \d+ \(|§P\d|task \d+(?:\.\d+)?\b|D\d+ decision")
    });
    let phase = PHASE.get_or_init(|| regex(r"Phase [A-Z]\b"));
    let stage = STAGE.get_or_init(|| regex(r"Stage \d[A-Z]?\b"));
    if step.is_match(line) || phase.is_match(line) || stage.is_match(line) {
        result.push("step-marker");
    }
    let wiring = WIRING
        .get_or_init(|| regex_i(r"purely additive|not wired|reachable from no|not yet consumed"));
    if wiring.is_match(line) {
        result.push("wiring-status");
    }
    if DATE
        .get_or_init(|| regex(r"\b20\d\d-\d\d-\d\d\b"))
        .is_match(line)
    {
        result.push("date-in-comment");
    }
    if HISTORY.get_or_init(|| regex_i(r"used to read|previously read|this paragraph|renamed from|was stale|corrected in place")).is_match(line) { result.push("history-prose"); }
    result
}

fn is_comment_start(line: &str, extension: &str) -> bool {
    static RS: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    static PS: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    static PY: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    match extension {
        ".rs" => RS
            .get_or_init(|| regex(r"^\s*(///|//!|//|/\*|\*(\s|/|$))"))
            .is_match(line),
        ".ps1" => PS.get_or_init(|| regex(r"^\s*(#|<#)")).is_match(line),
        ".py" => PY.get_or_init(|| regex(r"^\s*#")).is_match(line),
        _ => false,
    }
}

pub fn comment_line_mask(lines: &[String], extension: &str) -> Vec<bool> {
    let normalized_extension = extension.to_ascii_lowercase();
    let extension = normalized_extension.as_str();
    static REQUIRES: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    let mut mask = vec![false; lines.len()];
    let mut in_delimited = false;
    let (open, close, same) = match extension {
        ".ps1" => (Some("<#"), Some("#>"), false),
        ".py" => (Some("\"\"\""), Some("\"\"\""), true),
        _ => (None, None, false),
    };
    for (i, line) in lines.iter().enumerate() {
        let mut is_comment = is_comment_start(line, extension);
        if let (Some(open), Some(close)) = (open, close) {
            let closes = line.matches(close).count();
            if in_delimited {
                is_comment = true;
                if closes > 0 {
                    in_delimited = false;
                }
            } else if line.trim_start().starts_with(open) {
                is_comment = true;
                in_delimited = if same { closes % 2 == 1 } else { closes == 0 };
            }
        }
        if is_comment
            && (REQUIRES
                .get_or_init(|| regex_i(r"^\s*#Requires\b"))
                .is_match(line)
                || (i == 0 && line.starts_with("#!")))
        {
            is_comment = false;
        }
        mask[i] = is_comment;
    }
    mask
}

pub fn code_portion(text: &str, token: &str) -> String {
    if token.is_empty() {
        return text.trim().to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && in_string {
            i += 2;
            continue;
        }
        if chars[i] == '"' {
            in_string = !in_string;
            i += 1;
            continue;
        }
        if !in_string
            && chars[i..].iter().zip(token.chars()).all(|(a, b)| *a == b)
            && token.chars().count() <= chars.len() - i
        {
            return chars[..i].iter().collect::<String>().trim().to_owned();
        }
        i += 1;
    }
    text.trim().to_owned()
}

#[derive(Debug, Deserialize)]
pub struct ClassifyRequest {
    pub extension: String,
    pub lines: Vec<String>,
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ClassifyResponse {
    pub supported: bool,
    pub mask: Vec<bool>,
    pub code: Vec<String>,
}

pub fn classify_batch(mut request: ClassifyRequest) -> ClassifyResponse {
    request.extension.make_ascii_lowercase();
    let default_token = match request.extension.as_str() {
        ".rs" => "//",
        ".ps1" | ".py" => "#",
        _ => "",
    };
    let token = request.token.as_deref().unwrap_or(default_token);
    let mask = comment_line_mask(&request.lines, &request.extension);
    let code = request
        .lines
        .iter()
        .map(|line| code_portion(line, token))
        .collect();
    let supported = matches!(request.extension.as_str(), ".rs" | ".ps1" | ".py");
    ClassifyResponse {
        supported,
        mask,
        code,
    }
}

#[derive(Default)]
struct Metadata {
    fn_names: HashSet<String>,
    rs_basenames: HashSet<String>,
    public_module_files: HashSet<PathBuf>,
}

fn declaration_names(line: &str) -> impl Iterator<Item = String> + '_ {
    static DECL: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    DECL.get_or_init(|| {
        regex(r"\b(?:fn|struct|enum|trait|union|type|const|static|mod)\s+([A-Za-z_][A-Za-z0-9_]*)")
    })
    .captures_iter(line)
    .map(|m| m[1].to_owned())
}

fn is_rs_path(path: &Path) -> bool {
    path.extension()
        .and_then(|x| x.to_str())
        .is_some_and(|x| x.eq_ignore_ascii_case("rs"))
}

fn public_modules(files: &[FileData]) -> HashSet<PathBuf> {
    let mut result = HashSet::new();
    for file in files {
        if !is_rs_path(&file.path) {
            continue;
        }
        let Some(stem) = file.path.file_stem().and_then(|x| x.to_str()) else {
            continue;
        };
        if stem == "lib" || stem == "main" {
            result.insert(file.path.clone());
            continue;
        }
        let normalized = file.path.to_string_lossy().replace('\\', "/");
        let marked_internal = format!("/{}/", normalized);
        if regex_i(r"/(?:tests|examples|benches)/").is_match(&marked_internal) {
            continue;
        }
        let mut stem = stem.to_owned();
        let mut dir = file.path.parent().unwrap_or(Path::new(".")).to_path_buf();
        if stem == "mod" {
            if let Some(name) = dir.file_name().and_then(|x| x.to_str()) {
                stem = name.to_owned();
            }
            dir = dir.parent().unwrap_or(Path::new(".")).to_path_buf();
        }
        let candidates = [
            dir.join("mod.rs"),
            dir.join("lib.rs"),
            dir.join("main.rs"),
            dir.parent().unwrap_or(Path::new(".")).join(format!(
                "{}.rs",
                dir.file_name().and_then(|x| x.to_str()).unwrap_or_default()
            )),
        ];
        for parent in candidates {
            if !parent.is_file() {
                continue;
            }
            let Ok(text) = fs::read_to_string(&parent) else {
                continue;
            };
            let pat = format!(
                r"(?i)^\s*(pub(?:\([^)]*\))?\s+)?mod\s+{}\s*;",
                regex::escape(&stem)
            );
            if let Some(line) = text.lines().find(|line| regex(&pat).is_match(line)) {
                if line.trim_start().starts_with("pub") {
                    result.insert(file.path.clone());
                }
                break;
            }
        }
    }
    result
}

fn metadata(files: &[FileData]) -> Metadata {
    let mut data = Metadata {
        public_module_files: public_modules(files),
        ..Metadata::default()
    };
    for file in files {
        if !is_rs_path(&file.path) {
            continue;
        }
        if let Some(name) = file.path.file_name().and_then(|x| x.to_str()) {
            data.rs_basenames.insert(name.to_owned());
        }
        data.fn_names
            .extend(file.lines.iter().flat_map(|line| declaration_names(line)));
    }
    data
}

fn block_kind_rust(first: &str, all: &[String], end: usize, public: bool) -> bool {
    if first.trim_start().starts_with("//!") {
        return public;
    }
    if !first.trim_start().starts_with("///") {
        return false;
    }
    let limit = usize::min(end + 12, all.len());
    let mut j = end;
    while j < limit {
        let line = &all[j];
        if regex(r"^\s*#!?\[").is_match(line) {
            let mut depth = line.matches('[').count() as isize - line.matches(']').count() as isize;
            while depth > 0 && j < usize::min(end + 40, all.len().saturating_sub(1)) {
                j += 1;
                depth +=
                    all[j].matches('[').count() as isize - all[j].matches(']').count() as isize;
            }
            j += 1;
            continue;
        }
        if line.trim().is_empty() || regex(r"^\s*//").is_match(line) {
            j += 1;
            continue;
        }
        if regex(r"^\s*pub(?:\s|\()").is_match(line) {
            return public;
        }
        let indent = line.len() - line.trim_start().len();
        let start = j.saturating_sub(400);
        for k in (start..j).rev() {
            let candidate = &all[k];
            if candidate.trim().is_empty() || regex(r"^\s*(///|//!|//)").is_match(candidate) {
                continue;
            }
            let candidate_indent = candidate.len() - candidate.trim_start().len();
            if candidate_indent >= indent {
                continue;
            }
            if regex(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:enum|trait)\s+[A-Za-z_]").is_match(candidate)
            {
                return regex(r"^\s*pub").is_match(candidate) && public;
            }
            if regex(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|union|impl|fn|mod)\b")
                .is_match(candidate)
            {
                return false;
            }
        }
        return false;
    }
    false
}

fn block_kind_script(
    first: &str,
    text: &str,
    all: &[String],
    start: usize,
    extension: &str,
) -> bool {
    if extension == ".py" {
        if !first.trim_start().starts_with("\"\"\"") {
            return false;
        }
        for line in all[..start].iter().rev() {
            if line.trim().is_empty() || line.starts_with("#!") {
                continue;
            }
            return regex_i(r"^\s*(def|class)\s").is_match(line);
        }
        return true;
    }
    if !first.trim_start().starts_with("<#") || !regex_i(r"(?m)^\s*\.(?:SYNOPSIS|DESCRIPTION|PARAMETER|EXAMPLE|NOTES|OUTPUTS|INPUTS|LINK|COMPONENT|ROLE|FUNCTIONALITY|FORWARDHELPTARGETNAME|EXTERNALHELP)\b").is_match(text) { return false; }
    for line in all[..start].iter().rev() {
        if line.trim().is_empty() {
            continue;
        }
        return regex_i(r"^\s*function\s").is_match(line);
    }
    true
}

fn rust_marker(line: &str) -> u8 {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//!") {
        1
    } else if trimmed.starts_with("///") {
        2
    } else if trimmed.starts_with("//") {
        3
    } else {
        4
    }
}

fn is_intra_doc_link(line: &str) -> bool {
    regex(r"\[\x60[^\x60]+\x60\]").is_match(line)
}

fn has_claim_and_ident(line: &str) -> bool {
    static CLAIM: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    static IDENT: std::sync::OnceLock<Arc<Regex>> = std::sync::OnceLock::new();
    let claim = CLAIM.get_or_init(|| regex_i(r"\brefuses\b|\brejects\b|\baccepts unconditionally\b|\balready refuses\b|\bonly caller\b|\bsole caller\b|\bcalled from exactly\b|\bzero callers\b|\bno production caller\b|\bnever called\b|\bnever reached\b|\bcannot happen\b|\bcannot be reached\b|\balways returns\b|\bnever returns\b|\bstays on\b|\bis not wired\b|\bnever fires\b|\balways fires\b"));
    let unreachable = regex_i(r"\bunreachable\b");
    let claim_match = claim.is_match(line)
        || unreachable
            .find_iter(line)
            .any(|m| !line[m.end()..].starts_with('!'));
    claim_match
        && IDENT
            .get_or_init(|| regex(r"\x60[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z0-9_]+)*(?:\(\))?\x60"))
            .is_match(line)
}

fn add_hit(report: &mut Report, category: &str, value: String) {
    if let Some(hits) = report.hits.get_mut(category) {
        hits.push(value);
    }
}

fn block_anchor(text: &str, root: &Path) -> bool {
    if text.contains("\x60\x60\x60") || regex_i("include_str!").is_match(text) {
        return true;
    }
    let docs = regex(r"(?:rust/)?docs/research/[A-Za-z0-9._/-]+\.md");
    let found = docs
        .find_iter(text)
        .any(|m| existing_path(root, m.as_str()));
    found
}

fn existing_path(root: &Path, relative: &str) -> bool {
    root.join(relative).exists() || root.join("rust").join(relative).exists()
}

fn block_reference(text: &str, root: &Path, cited: bool) -> bool {
    cited || regex_i(r"https?://").is_match(text) || {
        let docs = regex(r"(?:rust/)?docs/[A-Za-z0-9._/-]+\.md");
        let found = docs
            .find_iter(text)
            .any(|m| existing_path(root, m.as_str()));
        found
    }
}

fn eval_block(
    report: &mut Report,
    block: &[String],
    start: usize,
    file: &FileData,
    root: &Path,
    meta: &Metadata,
    options: &ScanOptions,
) {
    if block.is_empty() {
        return;
    }
    let text = block.join("\n");
    let rust = file.source.extension == ".rs";
    let end = start - 1 + block.len();
    let mut cited = false;
    if rust {
        let phrase = regex_i(
            r"(?:pinned by|pins|asserted by|witnessed by|proved by|checked by)\s+`([a-z_][A-Za-z0-9_]*_[A-Za-z0-9_]*)`",
        );
        let path = regex_i(r"([A-Za-z0-9_./\\-]+\.rs)::([a-z_][A-Za-z0-9_]*)");
        let mut names: Vec<String> = phrase
            .captures_iter(&text)
            .map(|m| m[1].to_owned())
            .collect();
        for capture in path.captures_iter(&text) {
            let basename = capture[1].rsplit(['/', '\\']).next().unwrap_or_default();
            if meta.rs_basenames.contains(basename) {
                names.push(capture[2].to_owned());
            }
        }
        for name in names {
            if name.ends_with('_') {
                continue;
            }
            if meta.fn_names.contains(&name)
                || (name.len() >= 12 && meta.fn_names.iter().any(|known| known.starts_with(&name)))
            {
                cited = true;
            } else {
                *report.counts.get_mut("dead-citation").unwrap() += 1;
                if options.list {
                    add_hit(
                        report,
                        "dead-citation",
                        format!("{}:{}: cites unknown fn `{}`", file.relative, start, name),
                    );
                }
            }
        }
    }
    let api = if rust {
        block_kind_rust(
            &block[0],
            &file.lines,
            end,
            meta.public_module_files.contains(&file.path),
        )
    } else {
        block_kind_script(
            &block[0],
            &text,
            &file.lines,
            start - 1,
            file.source.extension,
        )
    };
    if api {
        if block.len() > MAX_BLOCK_LINES {
            report.api_docs_long += 1;
        }
    } else if block.len() > 1 {
        let tag = regex_i(r"(?m)^\s*(?:///|//!|//|\*|#)\s*SAFETY:").is_match(&text);
        if block.len() == 2 && block_reference(&text, root, cited) {
            report.reference_backed += 1;
        } else if !tag {
            *report.counts.get_mut("impl-comment-too-long").unwrap() += 1;
            if options.list {
                add_hit(
                    report,
                    "impl-comment-too-long",
                    format!(
                        "{}:{}: {} lines, no claim: {}",
                        file.relative,
                        start,
                        block.len(),
                        block[0].trim()
                    ),
                );
            }
        } else {
            *report.claimed_exceptions.get_mut("SAFETY:").unwrap() += 1;
            if block.len() > MAX_BLOCK_LINES && !cited && !block_anchor(&text, root) {
                *report.counts.get_mut("unanchored-exception").unwrap() += 1;
                if options.list {
                    add_hit(
                        report,
                        "unanchored-exception",
                        format!(
                            "{}:{}: {} lines, SAFETY: claimed but no anchor",
                            file.relative,
                            start,
                            block.len()
                        ),
                    );
                }
            }
        }
    }
    if rust && !cited {
        let mut documents_test = false;
        for line in file.lines.iter().skip(end).take(12) {
            if regex_i(r"^\s*#\[test\]|^\s*#\[tokio::test\]").is_match(line) {
                documents_test = true;
                break;
            }
            if regex(r"^\s*#!?\[").is_match(line) || line.trim().is_empty() {
                continue;
            }
            break;
        }
        if !documents_test {
            for line in block {
                if !is_intra_doc_link(line) && has_claim_and_ident(line) {
                    *report.counts.get_mut("cross-reference-claim").unwrap() += 1;
                    if options.list {
                        add_hit(
                            report,
                            "cross-reference-claim",
                            format!("{}:{}: {}", file.relative, start, line.trim()),
                        );
                    }
                }
            }
        }
    }
    for found in regex(r"(?:rust/)?docs/[A-Za-z0-9._/-]+\.md").find_iter(&text) {
        if !existing_path(root, found.as_str()) {
            *report.counts.get_mut("docs-link-broken").unwrap() += 1;
            if options.list {
                add_hit(
                    report,
                    "docs-link-broken",
                    format!("{}:{}: missing {}", file.relative, start, found.as_str()),
                );
            }
        }
    }
}

pub fn scan_repo(root: impl AsRef<Path>, options: ScanOptions) -> Result<Report, ScanError> {
    let root = root.as_ref();
    let files = collect_files(root)?;
    let meta = metadata(&files);
    let mut report = Report {
        counts: CATEGORY_ORDER
            .iter()
            .map(|name| ((*name).to_owned(), 0))
            .collect(),
        hits: CATEGORY_ORDER
            .iter()
            .map(|name| ((*name).to_owned(), Vec::new()))
            .collect(),
        total: 0,
        api_docs_long: 0,
        reference_backed: 0,
        claimed_exceptions: [("SAFETY:".to_owned(), 0)].into_iter().collect(),
    };
    for file in &files {
        let mask = comment_line_mask(&file.lines, file.source.extension);
        let mut block = Vec::new();
        let mut start = 0;
        let mut marker = 0;
        for (i, line) in file.lines.iter().enumerate() {
            if mask[i] {
                let this_marker = if file.source.extension == ".rs" {
                    rust_marker(line)
                } else {
                    0
                };
                if !block.is_empty() && this_marker != marker {
                    eval_block(&mut report, &block, start, file, root, &meta, &options);
                    block.clear();
                }
                if block.is_empty() {
                    start = i + 1;
                    marker = this_marker;
                }
                block.push(line.clone());
                for category in line_categories(line) {
                    *report.counts.get_mut(category).unwrap() += 1;
                    if options.list {
                        add_hit(
                            &mut report,
                            category,
                            format!("{}:{}: {}", file.relative, i + 1, line.trim()),
                        );
                    }
                }
            } else if !block.is_empty() {
                eval_block(&mut report, &block, start, file, root, &meta, &options);
                block.clear();
            }
        }
        if !block.is_empty() {
            eval_block(&mut report, &block, start, file, root, &meta, &options);
        }
    }
    report.total = report.counts.values().sum();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_delimited_power_shell_and_directives() {
        let lines = ["#Requires -Version 7", "<#", "2026-09-21", "#>", "code"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(
            comment_line_mask(&lines, ".ps1"),
            vec![false, true, true, true, false]
        );
    }

    #[test]
    fn masks_python_docstring_and_shebang() {
        let lines = [
            "#!/usr/bin/env python",
            "def f():",
            "    \"\"\"",
            "    note",
            "    \"\"\"",
            "    return 1",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        assert_eq!(
            comment_line_mask(&lines, ".py"),
            vec![false, false, true, true, true, false]
        );
    }

    #[test]
    fn code_portion_ignores_comment_token_in_string() {
        assert_eq!(
            code_portion(r#"let url = "http://x"; // note"#, "//"),
            r#"let url = "http://x";"#
        );
        assert_eq!(code_portion("  value # note  ", "#"), "value");
    }

    #[test]
    fn batch_response_uses_same_mask_and_code_classifier() {
        let response = classify_batch(ClassifyRequest {
            extension: ".rs".to_owned(),
            lines: vec![
                "let x = 1; // note".to_owned(),
                "// date 2026-09-21".to_owned(),
            ],
            token: None,
        });
        assert!(response.supported);
        assert_eq!(response.mask, vec![false, true]);
        assert_eq!(response.code, vec!["let x = 1;".to_owned(), "".to_owned()]);
    }
}
