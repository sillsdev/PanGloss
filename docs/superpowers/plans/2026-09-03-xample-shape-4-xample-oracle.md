# XAmple-shape, Plan 4 of 4: the XAmple engine as oracle of record, `grammar.xample/` emitter, two-oracle gate, fixtures

> **SUPERSEDED 2026-09-04 — DO NOT EXECUTE AS WRITTEN.** Only the DLL/result-parsing research and
> both-direction comparison principle remain useful. The hand-written emitter and its phonology-free
> `OutsideSubset` boundary are superseded: the current comparator gives the same FieldWorks project
> to the real `M3ToXAmpleTransformer` and to PanGloss. XAMPLE is a migration comparator, not the
> oracle defining production HC semantics, and ordinary HC replaces the old XAMPLE profile. The
> authoritative tasks are in `2026-09-04-xample-projects-on-hc.md`.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** For phonology-free (`requires: []`) conformance fixtures, run the real XAmple engine FieldWorks ships and compare it, both directions, against HC-Rust under the XAmple profile; stage one synthetic XAmple-shape fixture verified against both `hc.dll` and XAmple.

**Architecture:** A small Rust crate `pg-xample-oracle` (a `[[bin]]` plus a library) loads `xample64.dll` at runtime with `libloading` (so the workspace builds on machines without it and the gate self-skips), drives the C API the way `XAmpleDLLWrapper.cs` does, and parses the `<AResult>`/`<Wordform>` result into `(morph ids, surface)` signatures. A `grammar.xml → grammar.xample/` emitter in the same crate covers exactly the phonology-free subset and refuses anything else per fixture. A pg-parse-side gate discovers `requires: []` fixtures, emits, runs both engines, and reports agreement with a `NoMoreThan` ratchet in each direction.

**Tech Stack:** Rust, `libloading`, the FieldWorks build at `C:\Users\johnm\Documents\repos\FieldWorks\DistFiles` (`xample64.dll`, `Language Explorer/Configuration/Grammar/XAmplecd.tab`). Windows-only oracle; the gate self-skips elsewhere. Builds/tests via `rust/tools/pg.ps1` only.

Depends on Plans 1–3. Spec §8, §9 gate 5. Protocol: `machine/conformance/PROTOCOL.md` §5–§6 (the `--capabilities ""` engine and the `grammar.xample/` per-fixture representation are already reserved there).

---

## File map

- Create `rust/crates/pg-xample-oracle/Cargo.toml`, `src/lib.rs`, `src/ffi.rs` (DLL binding), `src/result.rs` (result XML → signatures), `src/emit.rs` (HC-XML grammar → XAmple files), `src/bin/xample-oracle.rs` (PROTOCOL §1 adapter CLI: `<grammar.xample dir> <words.txt> <out.tsv>`).
- Create `rust/crates/pg-parse/tests/two_oracle_agreement_gate.rs`.
- Create `conformance-staging/edge-cases/xample-shape-substrate/{grammar.xml,words.yaml,STAGING.md,grammar.xample/}`.
- Modify `rust/tools/oracle-conformance.ps1` — optional `-XAmple` switch that also runs the XAmple adapter over `requires: []` fixtures (thin; the Rust gate is the real check).
- Modify `docs/adr/` — new `000N-xample-oracle-jurisdiction.md` (short).

---

### Task 1: crate skeleton and DLL discovery

**Files:**
- Create: `rust/crates/pg-xample-oracle/Cargo.toml`:

```toml
[package]
name = "pg-xample-oracle"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
libloading = "0.8"
pg-grammar = { path = "../pg-grammar" }
thiserror = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
```

Add the crate to the workspace `members` in `rust/Cargo.toml`.

- Create: `src/lib.rs` with `pub mod ffi; pub mod result; pub mod emit;` and:

```rust
/// Where the XAmple engine lives on this machine. `PANGLOSS_XAMPLE_DIR` overrides; the default is
/// FieldWorks' `DistFiles`, which carries both the DLL and the fixed code table.
pub fn xample_dir() -> Option<std::path::PathBuf> {
    if let Ok(d) = std::env::var("PANGLOSS_XAMPLE_DIR") {
        let p = std::path::PathBuf::from(d);
        return p.join("xample64.dll").exists().then_some(p);
    }
    let default = std::path::Path::new(r"C:\Users\johnm\Documents\repos\FieldWorks\DistFiles");
    default.join("xample64.dll").exists().then(|| default.to_path_buf())
}

pub fn code_table_path(dist_files: &std::path::Path) -> std::path::PathBuf {
    dist_files.join("Language Explorer").join("Configuration").join("Grammar").join("XAmplecd.tab")
}
```

- [ ] **Step 1: Test** (`lib.rs` tests): `xample_dir_is_none_when_env_points_nowhere` — set `PANGLOSS_XAMPLE_DIR` to a temp dir and assert `None`; `code_table_path_joins_the_fieldworks_layout`.
- [ ] **Step 2–4:** run `& .\rust\tools\pg.ps1 -Mode quick -Package pg-xample-oracle` → PASS. Commit `git commit -am "xample-oracle: crate skeleton and engine discovery"`.

---

### Task 2: FFI binding (mirrors `XAmpleDLLWrapper.cs`)

**Files:** `src/ffi.rs`

The exports (all `cdecl`, all returning a `char*` the DLL owns until the next call): `AmpleCreateSetup() -> *mut c_void`, `AmpleDeleteSetup(setup) -> *const c_char`, `AmpleReset(setup)`, `AmpleSetParameter(setup, name, value)`, `AmpleLoadControlFiles(setup, adctl, cdtab, orthochange_or_null, textctl_or_null)`, `AmpleLoadDictionary(setup, path, "u")`, `AmpleLoadGrammarFile(setup, path)`, `AmpleParseText(setup, text_bytes, "n")`, `AmpleReportVersion(setup)`. Returned strings beginning with `<error` are failures (`ThrowIfError` in the C# wrapper checks for that prefix — read `XAmpleDLLWrapper.cs`'s `ThrowIfError` for the exact test and copy it).

- [ ] **Step 1: Failing test** (self-skipping when the DLL is absent; every test in this crate that touches the DLL starts with `let Some(dir) = crate::xample_dir() else { eprintln!("skip: xample64.dll not found"); return; };`):

```rust
#[test]
fn engine_loads_and_reports_a_version() {
    let Some(dir) = crate::xample_dir() else { eprintln!("skip: xample64.dll not found"); return; };
    let engine = XAmple::load(&dir).unwrap();
    let v = engine.report_version().unwrap();
    assert!(v.to_lowercase().contains("ample"), "{v}");
}

#[test]
fn a_fieldworks_generated_grammar_parses_a_word() {
    let Some(dir) = crate::xample_dir() else { return; };
    let data = std::path::Path::new(r"C:\Users\johnm\Documents\repos\FieldWorks\Src\LexText\ParserCore\ParserCoreTests\M3ToXAmpleTransformerTestsDataFiles");
    if !data.join("StemName3adctl.txt").exists() { eprintln!("skip: FieldWorks test data not found"); return; }
    let mut engine = XAmple::load(&dir).unwrap();
    engine.load_files(&crate::code_table_path(&dir), data, "StemName3").unwrap();
    let xml = engine.parse_word("Hello").unwrap();
    assert!(xml.contains("<Wordform") || xml.contains("<AResult"), "{xml}");
}
```

- [ ] **Step 2: Run** → compile error.

- [ ] **Step 3: Implement** (`ffi.rs`):

```rust
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;

use libloading::{Library, Symbol};

#[derive(Debug, thiserror::Error)]
pub enum XAmpleError {
    #[error("cannot load xample64.dll: {0}")]
    Load(#[from] libloading::Error),
    #[error("xample: {0}")]
    Engine(String),
    #[error("path is not valid UTF-8/C string: {0}")]
    Path(String),
}

type Setup = *mut c_void;
type FnCreate = unsafe extern "C" fn() -> Setup;
type FnDelete = unsafe extern "C" fn(Setup) -> *const c_char;
type FnReset = unsafe extern "C" fn(Setup) -> *const c_char;
type FnSetParam = unsafe extern "C" fn(Setup, *const c_char, *const c_char) -> *const c_char;
type FnLoadCtl = unsafe extern "C" fn(Setup, *const c_char, *const c_char, *const c_char, *const c_char) -> *const c_char;
type FnLoadDict = unsafe extern "C" fn(Setup, *const c_char, *const c_char) -> *const c_char;
type FnLoadGram = unsafe extern "C" fn(Setup, *const c_char) -> *const c_char;
type FnParse = unsafe extern "C" fn(Setup, *const c_char, *const c_char) -> *const c_char;
type FnVersion = unsafe extern "C" fn(Setup) -> *const c_char;

pub struct XAmple {
    _lib: Library,
    setup: Setup,
    delete: FnDelete,
    reset: FnReset,
    set_param: FnSetParam,
    load_ctl: FnLoadCtl,
    load_dict: FnLoadDict,
    load_gram: FnLoadGram,
    parse: FnParse,
    version: FnVersion,
}

fn c(s: &str) -> Result<CString, XAmpleError> {
    CString::new(s).map_err(|_| XAmpleError::Path(s.to_string()))
}

fn take(p: *const c_char) -> String {
    if p.is_null() { return String::new(); }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

fn check(s: String) -> Result<String, XAmpleError> {
    // XAmpleDLLWrapper.ThrowIfError (verified 2026-09-03): an error is any reply containing
    // "<error" other than the SGML-ish success marker "<error code=none>".
    if s.contains("<error") && !s.contains("<error code=none>") { Err(XAmpleError::Engine(s)) } else { Ok(s) }
}

impl XAmple {
    pub fn load(dist_files: &Path) -> Result<Self, XAmpleError> {
        let lib = unsafe { Library::new(dist_files.join("xample64.dll")) }?;
        unsafe {
            let create: Symbol<FnCreate> = lib.get(b"AmpleCreateSetup\0")?;
            let setup = create();
            let delete = *lib.get::<FnDelete>(b"AmpleDeleteSetup\0")?;
            let reset = *lib.get::<FnReset>(b"AmpleReset\0")?;
            let set_param = *lib.get::<FnSetParam>(b"AmpleSetParameter\0")?;
            let load_ctl = *lib.get::<FnLoadCtl>(b"AmpleLoadControlFiles\0")?;
            let load_dict = *lib.get::<FnLoadDict>(b"AmpleLoadDictionary\0")?;
            let load_gram = *lib.get::<FnLoadGram>(b"AmpleLoadGrammarFile\0")?;
            let parse = *lib.get::<FnParse>(b"AmpleParseText\0")?;
            let version = *lib.get::<FnVersion>(b"AmpleReportVersion\0")?;
            Ok(XAmple { _lib: lib, setup, delete, reset, set_param, load_ctl, load_dict, load_gram, parse, version })
        }
    }

    pub fn report_version(&self) -> Result<String, XAmpleError> {
        check(take(unsafe { (self.version)(self.setup) }))
    }

    pub fn set_parameter(&mut self, name: &str, value: &str) -> Result<(), XAmpleError> {
        check(take(unsafe { (self.set_param)(self.setup, c(name)?.as_ptr(), c(value)?.as_ptr()) })).map(|_| ())
    }

    /// `XAmpleDLLWrapper.LoadFiles`: `<db>adctl.txt`, `<db>lex.txt`, `<db>gram.txt` in `dir`, plus the fixed code table.
    pub fn load_files(&mut self, code_table: &Path, dir: &Path, db: &str) -> Result<(), XAmpleError> {
        let adctl = c(&dir.join(format!("{db}adctl.txt")).to_string_lossy())?;
        let lex = c(&dir.join(format!("{db}lex.txt")).to_string_lossy())?;
        let gram = c(&dir.join(format!("{db}gram.txt")).to_string_lossy())?;
        let cdtab = c(&code_table.to_string_lossy())?;
        unsafe {
            check(take((self.reset)(self.setup)))?;
            // Same fixed options XAmpleDLLWrapper.SetOptions sends before loading.
            for (k, v) in [("MaxMorphnameLength", "40"), ("MaxTrieDepth", "3"), ("RootGlosses", "FALSE"), ("TraceAnalysis", "OFF"), ("CheckMorphReferences", "FALSE"), ("OutputDecomposition", "TRUE"), ("OutputOriginalWord", "TRUE")] {
                check(take((self.set_param)(self.setup, c(k)?.as_ptr(), c(v)?.as_ptr())))?;
            }
            check(take((self.load_ctl)(self.setup, adctl.as_ptr(), cdtab.as_ptr(), std::ptr::null(), std::ptr::null())))?;
            check(take((self.load_dict)(self.setup, lex.as_ptr(), c("u")?.as_ptr())))?;
            check(take((self.load_gram)(self.setup, gram.as_ptr())))?;
        }
        Ok(())
    }

    /// Raw result XML for one word (`AmpleParseText(setup, word, "n")`).
    pub fn parse_word(&mut self, word: &str) -> Result<String, XAmpleError> {
        check(take(unsafe { (self.parse)(self.setup, c(word)?.as_ptr(), c("n")?.as_ptr()) }))
    }
}

impl Drop for XAmple {
    fn drop(&mut self) {
        unsafe { (self.delete)(self.setup); }
    }
}
```

Read `XAmpleDLLWrapper.cs` lines 440-500 (`SetOptions`) and the `ParseString` method for the exact parameter list and the second argument to `AmpleParseText` (the C# passes `pszUseTextIn`; confirm whether it is `"n"` or something else), and correct the constants above to match — that file is the primary source.

- [ ] **Step 4: Run** `& .\rust\tools\pg.ps1 -Mode test -Package pg-xample-oracle` → PASS on this machine.

- [ ] **Step 5: Commit** `git commit -am "xample-oracle: runtime binding to xample64.dll"`

---

### Task 3: result XML → signatures

**Files:** `src/result.rs`

FieldWorks' `XAmpleParser.ProcessParseResults` (`XAmpleParser.cs:178-230`, verified 2026-09-03) first rewrites the raw reply: `DB_REF_HERE` → `'0'` and `<...>` → `[...]`, then parses it as XML. Root `<Wordform DbRef=... Form="...">`; a `<Exception code="ReachedMaxAnalyses" totalAnalyses="N"/>` child means XAmple stopped at `MaxAnalysesToReturn` (record this — the two-oracle gate compares as a subset when it is present); an `<Error>` child carries a failure message; analyses are every descendant `<WfiAnalysis>`, each with descendant `<Morph>` elements holding `<MoForm DbRef="…"/>` (the allomorph id from `\a form {id}`) and `<MSI DbRef="…"/>` (the `\lx` morpheme id). Do the same two string rewrites before parsing.

- [ ] **Step 1: Failing test** with a literal result string captured from Task 2's `StemName3` run (paste the actual XML the engine returned for one word into the test as the fixture, so the parser is tested against a real reply):

```rust
#[test]
fn signatures_are_morph_ids_joined_by_plus_and_the_surface() {
    let xml = include_str!("../tests/data/stemname3-hello.xml"); // saved from the engine in Task 2
    let sigs = signatures_from_result(xml).unwrap();
    assert!(!sigs.is_empty());
    for (morphs, surface) in &sigs {
        assert!(morphs.contains('+') || !morphs.is_empty());
        assert!(!surface.is_empty());
    }
}

#[test]
fn a_failure_reply_yields_no_signatures() {
    assert!(signatures_from_result("<AResult><error code=\"analysisFailure\">x</error></AResult>").unwrap().is_empty());
}
```

- [ ] **Step 2–3:** Implement `pub fn signatures_from_result(xml: &str) -> Result<Vec<(String, String)>, XAmpleError>` with a minimal hand-rolled tag scanner (the reply is small and regular; no XML crate) that yields, per analysis, the ordered morph ids and the surface form; render `morphs` as `ids.join("+")`. `pg_parse::result_signature(&sigs)` (in `pg-parse/src/lib.rs:79`) is the shared renderer for the joined, sorted signature string — depend on `pg-parse` for that one function rather than re-implementing it.

- [ ] **Step 4–5:** tests PASS; commit `git commit -am "xample-oracle: result XML to signatures"`.

---

### Task 4: `grammar.xml → grammar.xample/` emitter for the phonology-free subset

**Files:** `src/emit.rs`

Input: a `pg_grammar::model::Grammar` loaded by `pg_grammar::load` from a fixture's `grammar.xml`. Output: `<db>adctl.txt`, `<db>lex.txt`, `<db>gram.txt` in a directory, `db = "fixture"`. Reference for every field: `FxtM3ParserToXAmpleADCtl.xsl` and `FxtM3ParserToXAmpleLex.xsl` in FieldWorks, plus the generated examples in `M3ToXAmpleTransformerTestsDataFiles/` (e.g. `CliticAdCtl.txt`, `CliticEnvsLexicon.txt`). Copy their exact spellings.

Refuse (return `Err(EmitError::OutsideSubset(reason))`) when the grammar has: any `prules`; any `MorphRuleDef::Realizational`; any affix rule whose RHS is not "copy input, insert one literal string" on one side (prefix/suffix only in v1: no infix, no circumfix, no reduplication); MPR features; stem names; more than one stratum with content. Each refusal names the construct.

Emit:
- `adctl`: `\ca` per POS; `\maxnull 1 \maxprops 255 \maxp 5 \maxi 0 \maxs 5 \maxr 1 \maxn 0` (v1 fixed; a caps fixture overrides via the fixture's own `grammar.xample/` when needed); `\mp`/`\ap` empty; `\mcc` per `MorphemeCoOccurrenceRuleDef` and `\ancc` per `AllomorphCoOccurrenceRuleDef` using the exact `+/ A ... ~_` / `+/ ~_ ... A` spellings `ADCtl.xsl:213-364` produces for each `CoOccurrenceAdjacency` (read `SetEllipsisValue`, `ADCtl.xsl:712-729`); `\scl <id> | <name>` then a line listing each segment class's representations; the fixed `\pt/\st/\ft` test bodies copied verbatim from `CliticAdCtl.txt` (they are grammar-independent).
- `lex`: one `\lx <morpheme id>` entry per morpheme with `\g`, `\c <pos>` for roots or `\c <from>/<to>` for affixes, `\wc`, `\a <form> {<allomorph id>}` per allomorph followed by ` / <left> _ <right>` per required environment (literals as characters, classes as `[<class id>]`, word boundary `#`) and `~/ ...` per excluded environment, `\eType prefix|suffix` for affixes.
- `gram`: the smallest PC-PATR word grammar FLEx's `FxtM3ParserToToXAmpleGrammar.xsl` produces for a prefix/suffix-only project — take `StemName3gram.txt` as the template and keep only the rules that reference no feature the fixture lacks. If a fixture uses templates, refuse in v1 and record it.

- [ ] **Step 1: Failing test:** emit the machine fixture `suffixing-evidential-adjacency-chain` (`requires: []`) and load the result into XAmple; parse the first five words of its `words.yaml` and assert every one that has `parses:` gets ≥1 analysis and every `expect_fail`/no-parse word gets 0.
- [ ] **Step 2–5:** implement, iterate against the engine's `<error>` replies (they name the offending line), commit `git commit -am "xample-oracle: emit AMPLE control/dictionary files for the phonology-free subset"`.

---

### Task 5: adapter binary and the two-oracle agreement gate

**Files:**
- Create: `src/bin/xample-oracle.rs` — `xample-oracle <fixture-dir> <words.txt> <out.tsv>`: if `<fixture-dir>/grammar.xample/` exists use it, else emit from `grammar.xml` into a temp dir; write `word\tsignature` rows using `pg_parse::result_signature`.
- Create: `rust/crates/pg-parse/tests/two_oracle_agreement_gate.rs`.

- [ ] **Step 1: The gate:**

```rust
//! Both directions, over every `requires: []` fixture: words XAmple analyses and HC-Rust (XAmple
//! profile) does not, and the reverse. Ratcheted with `NoMoreThan`; self-skips without the engine.

use pg_conformance_fixtures::{discover_scoped, ConformanceScope};

const XAMPLE_ONLY_ALLOWED: usize = 0; // words XAmple accepts that HC-Rust refuses
const HC_ONLY_ALLOWED: usize = 0;     // words HC-Rust accepts that XAmple refuses

#[test]
fn xample_and_hc_rust_agree_on_phonology_free_fixtures() {
    let Some(dir) = pg_xample_oracle::xample_dir() else { eprintln!("skip: xample64.dll not found"); return; };
    let mut xample_only = Vec::new();
    let mut hc_only = Vec::new();
    let mut compared = 0usize;
    for f in discover_scoped(ConformanceScope::All) {
        let words = f.load_words_yaml().unwrap();
        if !words.requires.is_empty() { continue; }
        let xml = f.load_grammar_xml().unwrap();
        let g = pg_grammar::load(&xml).unwrap();
        let emitted = match pg_xample_oracle::emit::emit_to_temp(&g) {
            Ok(d) => d,
            Err(e) => { eprintln!("{}: outside the XAmple subset: {e}", f.label()); continue; }
        };
        let mut engine = pg_xample_oracle::ffi::XAmple::load(&dir).unwrap();
        engine.load_files(&pg_xample_oracle::code_table_path(&dir), emitted.path(), "fixture").unwrap();
        let morpher = pg_parse::Morpher::new(&g, usize::MAX);
        for w in words.words.iter().filter(|w| w.adapter_visible()) {
            let x = pg_xample_oracle::result::signatures_from_result(&engine.parse_word(&w.word).unwrap()).unwrap();
            let h = morpher.parse_word(&w.word).analyses;
            compared += 1;
            match (x.is_empty(), h.is_empty()) {
                (false, true) => xample_only.push(format!("{}:{}", f.label(), w.word)),
                (true, false) => hc_only.push(format!("{}:{}", f.label(), w.word)),
                _ => {}
            }
        }
    }
    eprintln!("two-oracle: compared {compared} words; xample-only {} hc-only {}", xample_only.len(), hc_only.len());
    assert!(compared > 0, "the gate compared nothing — an emitter refusal on every fixture is not agreement");
    assert!(xample_only.len() <= XAMPLE_ONLY_ALLOWED, "XAmple accepts, HC-Rust refuses: {xample_only:?}");
    assert!(hc_only.len() <= HC_ONLY_ALLOWED, "HC-Rust accepts, XAmple refuses: {hc_only:?}");
}
```

Acceptance is compared as accept/refuse first (morph-id spaces differ between the two engines); a second pass comparing morph *counts* per analysis is the follow-on once the first is green. Set the two `*_ALLOWED` constants to the measured counts on the first green run and record them in the fixture notes; never raise them without a `words.yaml` entry naming the word.

- [ ] **Step 2–3:** `& .\rust\tools\pg.ps1 -Mode test -Package pg-parse -TestTarget two_oracle_agreement_gate` (needs `PANGLOSS_CONFORMANCE_SCOPE=all` — `pg.ps1 -Mode test` sets it). Commit `git commit -am "gates: two-oracle agreement over phonology-free fixtures"`.

---

### Task 6: the XAmple-shape fixture

**Files:** `conformance-staging/edge-cases/xample-shape-substrate/{grammar.xml,words.yaml,STAGING.md}`

Follow `.claude/skills/conformance-grammars/SKILL.md` exactly (synthetic data only; no language names). Content: a phoneme table that omits one grapheme used by a stem (`q`), one suffix with two allomorphs selected by a `[V]`/`[C]` segment class on the left, one entry whose second allomorph has no environment and must lose to the first by disjunction, one `MorphemeCoOccurrenceRule` exclusion, zero rules, `requires: []`. `words.yaml` carries `# oracle-provenance: founding-oracle ...` (verify with `rust/tools/oracle-conformance.ps1`) **and** a second comment line `# xample-oracle: verified <date> xample64.dll <version>` written by hand after running the adapter binary. Note in `STAGING.md` that under HC-XML load the grammar has no profile (it is an XML fixture), so the substrate test lives in `pg-grammar`'s snapshot tests (Plan 2) while this fixture pins environments, disjunction order and co-occurrence for both oracles.

- [ ] Verify both oracles, stage, run `& .\rust\tools\pg.ps1 -Mode conformance-test -Scope local`, commit `git commit -am "conformance: xample-shape fixture verified against hc.dll and xample64.dll"`.

---

### Task 7: ADR

- [ ] Create `docs/adr/000N-xample-oracle-jurisdiction.md` (next free number): context (two engines from one model; XAmple never applies phonology), decision (XAmple is oracle of record for XAmple-shape parity on `requires: []` fixtures; `hc.dll` remains founding oracle for everything else; disagreements recorded in `words.yaml`, never resolved silently), consequences (the two-oracle gate; the `# xample-oracle:` comment line). Link from `docs/research/xample-grammars-on-hc-rust.md` §5.4. Commit.

---

## Self-review notes

- Spec §8: Tasks 1–5, 7. §9 gate 5: Task 5. Fixture: Task 6.
- The emitter is deliberately narrow; every refusal names its construct so widening it is a measured step, not a guess.
