# XAMPLE-to-HC groundwork: snapshot fields, parser metadata, `.fwbackup` input

> **Status (2026-09-04): RETAINED AND IMPLEMENTED.** The data imported here is provenance,
> substrate evidence, and differential-comparison input. It does not select an XAMPLE runtime
> profile. Continued work follows `2026-09-04-xample-projects-on-hc.md`.

> This is an implementation record, not an executable current plan. Unchecked boxes preserve the
> historical test-first sequence; do not resume them from this file.

**Goal:** Preserve which parser a FieldWorks project selected, its raw XAMPLE containment settings,
and the writing system's exemplar characters so PanGloss can complete HC's character substrate and
explain measured migration differences without using the settings as HC grammar semantics.

**Architecture:** `pg-snapshot` gains three data-only additions (`ActiveParser`, `XAmpleParameters`, `Project.exemplar_characters`). `pg-fwdata::parser_params::parse` reads the two new XML pieces from the same `<ParserParameters>` string it already parses, following liblcm's rule that an absent `<ActiveParser>` means XAmple. `pg_fwdata::import_file` accepts a `.fwbackup` zip, extracts the `.fwdata` entry and the `WritingSystemStore/*.ldml` files in memory, and fills the exemplar set.

**Tech Stack:** Rust, serde, `quick-xml` (already a dependency), `zip` crate (new, add `zip = { version = "2", default-features = false, features = ["deflate"] }` to the workspace). Builds/tests ONLY via `rust/tools/pg.ps1` (never bare cargo): `-Mode check` for the inner loop, `-Mode quick`/`-Mode test -Package <crate>` to run tests.

Current spec: `docs/superpowers/specs/2026-09-03-xample-shape-grammars.md` §2, §4, §7. This plan
predates the 2026-09-04 contract revision; its imported fields remain valid even where later use
changed.

---

## File map

- Modify `rust/crates/pg-snapshot/src/morphology.rs` — `ParserParameters` gains `active_parser: ActiveParser` and `xample: XAmpleParameters`; new enum and struct.
- Modify `rust/crates/pg-snapshot/src/project.rs` — `Project` gains `exemplar_characters: Vec<String>`.
- Modify `rust/crates/pg-snapshot/src/lib.rs` — re-export the new types (follow the existing `pub use morphology::{...}` line).
- Modify `rust/crates/pg-fwdata/src/parser_params.rs` — parse `<ActiveParser>` and `<XAmple>`.
- Modify `rust/crates/pg-fwdata/src/lib.rs` — `import_file` dispatches on `.fwbackup`; new `import_fwbackup`.
- Create `rust/crates/pg-fwdata/src/fwbackup.rs` — zip handling + LDML exemplar extraction (pure functions, unit-tested on strings).
- Modify `rust/crates/pg-fwdata/src/xml.rs` — factor `parse_fwdata(path)` into `parse_fwdata_reader<R: BufRead>(reader)` so bytes from a zip entry can be parsed without a temp file.
- Modify `rust/crates/pg-fwdata/tests/data/fixture.fwdata` — add an `<XAmple>` block and an `<ActiveParser>` to its `ParserParameters` string (keep everything else).
- Modify `rust/Cargo.toml` (workspace deps) and `rust/crates/pg-fwdata/Cargo.toml` — add `zip`.

Every new serde field uses `#[serde(default)]` so existing `.json` snapshots still load; `FORMAT_VERSION` stays 1 (that is how `strata` and `compound_rule_max_applications` were added).

---

### Task 1: `ActiveParser` and `XAmpleParameters` in the snapshot

**Files:**
- Modify: `rust/crates/pg-snapshot/src/morphology.rs` (the `ParserParameters` struct at ~line 366 and its `Default` at ~line 390)
- Modify: `rust/crates/pg-snapshot/src/lib.rs` (re-exports)
- Test: `rust/crates/pg-snapshot/src/morphology.rs` (unit tests at the bottom of the file, or `tests.rs` if the crate has one — grep for `#[cfg(test)]` in `src/`)

- [ ] **Step 1: Write the failing test** (append to the crate's existing test module; if `morphology.rs` has none, add `#[cfg(test)] mod tests { use super::*; ... }` at the end):

```rust
#[test]
fn parser_parameters_default_is_xample_with_fieldworks_default_caps() {
    let p = ParserParameters::default();
    assert_eq!(p.active_parser, ActiveParser::XAmple);
    assert_eq!(p.xample, XAmpleParameters::default());
    assert_eq!(p.xample.max_nulls, None);
    assert_eq!(p.xample.max_analyses_to_return, None);
}

#[test]
fn parser_parameters_round_trips_through_json_and_old_json_still_loads() {
    let mut p = ParserParameters::default();
    p.active_parser = ActiveParser::Hc;
    p.xample.max_prefixes = Some(3);
    let json = serde_json::to_string(&p).unwrap();
    let back: ParserParameters = serde_json::from_str(&json).unwrap();
    assert_eq!(back, p);
    // A snapshot written before these fields existed must still deserialize.
    let old = r#"{"notOnClitics":true,"acceptUnspecifiedGraphemes":false,"noDefaultCompounding":false}"#;
    let back: ParserParameters = serde_json::from_str(old).unwrap();
    assert_eq!(back.active_parser, ActiveParser::XAmple);
    assert_eq!(back.xample, XAmpleParameters::default());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `& .\rust\tools\pg.ps1 -Mode quick -Package pg-snapshot`
Expected: compile error, `ActiveParser` not found.

- [ ] **Step 3: Implement.** In `morphology.rs`, add above `ParserParameters`:

```rust
/// Which parser FieldWorks runs for this project, from `/ParserParameters/ActiveParser`.
/// FieldWorks defaults an absent element to `"XAmple"`. The retained implementation also defaulted
/// malformed XML, but the current contract requires malformed/unknown values to be reported as
/// invalid source rather than silently classified as XAMPLE-authored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ActiveParser {
    #[default]
    XAmple,
    Hc,
}

/// The `<ParserParameters><XAmple>` block: raw XAmple containment metadata. `None` means the
/// element was absent. HC does not apply FieldWorks/XAMPLE defaults; only comparison tooling may
/// interpret them, and malformed values must remain visible in the import report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct XAmpleParameters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_nulls: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_prefixes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_infixes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_suffixes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_interfixes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_roots: Option<u32>,
    /// `MaxAnalysesToReturn`; FieldWorks treats a value below 1 as "no limit"
    /// (`XAmpleParser.cs:126-133`). Stored raw; interpretation belongs only to XAMPLE
    /// comparison tooling, never the HC compiler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_analyses_to_return: Option<i32>,
}
```

Add to `ParserParameters` (after `compound_rule_max_applications`):

```rust
    #[serde(default)]
    pub active_parser: ActiveParser,
    #[serde(default)]
    pub xample: XAmpleParameters,
```

and to its `Default` impl: `active_parser: ActiveParser::XAmple, xample: XAmpleParameters::default(),`.

In `lib.rs`, extend the existing `pub use morphology::{ ... }` line with `ActiveParser, XAmpleParameters`.

- [ ] **Step 4: Run tests**

Run: `& .\rust\tools\pg.ps1 -Mode quick -Package pg-snapshot`
Expected: PASS. Then `& .\rust\tools\pg.ps1 -Mode check` — every crate that constructs `ParserParameters { .. }` literally must now list the two new fields (grep `ParserParameters {` across `rust/crates`; `pg-fwdata/src/parser_params.rs` is one — fix it by adding `active_parser: ActiveParser::XAmple, xample: XAmpleParameters::default(),` for now; Task 3 replaces that).

- [ ] **Step 5: Commit**

```
git add rust/crates/pg-snapshot rust/crates/pg-fwdata/src/parser_params.rs
git commit -m "snapshot: carry ActiveParser and the XAmple cap block"
```

---

### Task 2: `Project.exemplar_characters`

**Files:**
- Modify: `rust/crates/pg-snapshot/src/project.rs`

- [ ] **Step 1: Failing test** (same test module style as Task 1, in `project.rs`):

```rust
#[test]
fn project_without_exemplars_deserializes_to_empty() {
    let old = r#"{"name":"P","vernacularWritingSystems":["xx"],"analysisWritingSystems":["en"]}"#;
    let p: Project = serde_json::from_str(old).unwrap();
    assert!(p.exemplar_characters.is_empty());
}
```

- [ ] **Step 2: Run** `& .\rust\tools\pg.ps1 -Mode quick -Package pg-snapshot` → compile error.

- [ ] **Step 3: Implement.** Add to `Project`:

```rust
    /// Word-forming text elements of the default vernacular writing system, from the LDML
    /// `characters/exemplarCharacters` main set when the input was a `.fwbackup` (a bare `.fwdata`
    /// carries no LDML). Each entry is one text element (`a`, `ch`, `a\u{0303}`), NFD, as written.
    /// Empty means "unknown", never "none".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exemplar_characters: Vec<String>,
```

Fix every literal `Project { .. }` construction (grep `Project {` in `rust/crates`; `pg-fwdata/src/extract/mod.rs` builds one) by adding `exemplar_characters: Vec::new(),`.

- [ ] **Step 4: Run** `& .\rust\tools\pg.ps1 -Mode check` then `-Mode quick -Package pg-snapshot` → PASS.

- [ ] **Step 5: Commit** `git commit -am "snapshot: Project.exemplar_characters for LDML-derived alphabets"`

---

### Task 3: parse `<ActiveParser>` and `<XAmple>` in `pg-fwdata`

**Files:**
- Modify: `rust/crates/pg-fwdata/src/parser_params.rs`
- Test: unit tests at the bottom of `parser_params.rs` (add a `#[cfg(test)] mod tests` if absent)

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use pg_snapshot::ActiveParser;

    #[test]
    fn absent_active_parser_means_xample_like_liblcm() {
        let p = parse(Some("<ParserParameters><HC><NotOnClitics>true</NotOnClitics></HC></ParserParameters>"));
        assert_eq!(p.active_parser, ActiveParser::XAmple);
        assert_eq!(p.xample, XAmpleParameters::default());
    }

    #[test]
    fn hc_active_parser_is_read() {
        let p = parse(Some("<ParserParameters><HC/><ActiveParser>HC</ActiveParser></ParserParameters>"));
        assert_eq!(p.active_parser, ActiveParser::Hc);
    }

    #[test]
    fn unknown_active_parser_text_falls_back_to_xample() {
        let p = parse(Some("<ParserParameters><ActiveParser>Toneparser</ActiveParser></ParserParameters>"));
        assert_eq!(p.active_parser, ActiveParser::XAmple);
    }

    #[test]
    fn xample_block_is_read_field_by_field() {
        let p = parse(Some(
            "<ParserParameters><XAmple><MaxNulls>0</MaxNulls><MaxPrefixes>1</MaxPrefixes>\
             <MaxInfixes>0</MaxInfixes><MaxRoots>1</MaxRoots><MaxSuffixes>0</MaxSuffixes>\
             <MaxInterfixes>0</MaxInterfixes><MaxAnalysesToReturn>20</MaxAnalysesToReturn></XAmple>\
             <ActiveParser>XAmple</ActiveParser></ParserParameters>",
        ));
        assert_eq!(p.xample.max_nulls, Some(0));
        assert_eq!(p.xample.max_prefixes, Some(1));
        assert_eq!(p.xample.max_infixes, Some(0));
        assert_eq!(p.xample.max_roots, Some(1));
        assert_eq!(p.xample.max_suffixes, Some(0));
        assert_eq!(p.xample.max_interfixes, Some(0));
        assert_eq!(p.xample.max_analyses_to_return, Some(20));
    }

    #[test]
    fn partial_xample_block_leaves_missing_values_none() {
        let p = parse(Some("<ParserParameters><XAmple><MaxNulls>1</MaxNulls></XAmple></ParserParameters>"));
        assert_eq!(p.xample.max_nulls, Some(1));
        assert_eq!(p.xample.max_roots, None);
        assert_eq!(p.xample.max_analyses_to_return, None);
    }

    #[test]
    fn negative_max_analyses_is_kept_raw() {
        let p = parse(Some("<ParserParameters><XAmple><MaxAnalysesToReturn>-1</MaxAnalysesToReturn></XAmple></ParserParameters>"));
        assert_eq!(p.xample.max_analyses_to_return, Some(-1));
    }
}
```

- [ ] **Step 2: Run** `& .\rust\tools\pg.ps1 -Mode quick -Package pg-fwdata` → FAIL (fields not parsed).

- [ ] **Step 3: Implement.** In `parse`, after `let hc = params_elem.child("HC");` add:

```rust
    let active_parser = match params_elem
        .child("ActiveParser")
        .map(|n| n.text.trim())
    {
        Some("HC") => ActiveParser::Hc,
        // liblcm's getter returns "XAmple" for anything else, including an absent element.
        _ => ActiveParser::XAmple,
    };

    let xa = params_elem.child("XAmple");
    fn u32_child(n: Option<&crate::node::Node>, tag: &str) -> Option<u32> {
        n.and_then(|n| n.child(tag)).and_then(|c| c.text.trim().parse::<u32>().ok())
    }
    let xample = XAmpleParameters {
        max_nulls: u32_child(xa, "MaxNulls"),
        max_prefixes: u32_child(xa, "MaxPrefixes"),
        max_infixes: u32_child(xa, "MaxInfixes"),
        max_suffixes: u32_child(xa, "MaxSuffixes"),
        max_interfixes: u32_child(xa, "MaxInterfixes"),
        max_roots: u32_child(xa, "MaxRoots"),
        max_analyses_to_return: xa
            .and_then(|n| n.child("MaxAnalysesToReturn"))
            .and_then(|c| c.text.trim().parse::<i32>().ok()),
    };
```

Update the imports to `use pg_snapshot::{ActiveParser, CompoundRuleMaxApplications, ParserParameters, XAmpleParameters};` and the final struct literal to include `active_parser, xample,`. Update the module doc's first line to mention `<ActiveParser>` and `<XAmple>` (one sentence).

- [ ] **Step 4: Run** `& .\rust\tools\pg.ps1 -Mode quick -Package pg-fwdata` → PASS.

- [ ] **Step 5: Commit** `git commit -am "fwdata: read ActiveParser (absent = XAmple) and the XAmple cap block"`

---

### Task 4: fixture `.fwdata` carries the new block; fixture test asserts it

**Files:**
- Modify: `rust/crates/pg-fwdata/tests/data/fixture.fwdata` (find the `<ParserParameters><Uni>` element; it is HTML-escaped XML inside `<Uni>`)
- Modify: `rust/crates/pg-fwdata/tests/fixture_tests.rs`

- [ ] **Step 1: Failing test** (append to `fixture_tests.rs`, reusing its existing `fixture_path()`/import helper):

```rust
#[test]
fn fixture_reports_active_parser_and_xample_caps() {
    let (snapshot, _report) = pg_fwdata::import_file(&fixture_path()).unwrap();
    let pp = &snapshot.morphology.parser_parameters;
    assert_eq!(pp.active_parser, pg_snapshot::ActiveParser::Hc);
    assert_eq!(pp.xample.max_prefixes, Some(2));
    assert_eq!(pp.xample.max_analyses_to_return, Some(10));
}
```

- [ ] **Step 2: Run** `& .\rust\tools\pg.ps1 -Mode test -Package pg-fwdata -TestTarget fixture_tests` → FAIL.

- [ ] **Step 3: Edit the fixture.** Inside the existing `<Uni>` text of `ParserParameters`, before `&lt;/ParserParameters&gt;`, insert (escaped exactly like the surrounding text):

```
&lt;XAmple&gt;&lt;MaxNulls&gt;1&lt;/MaxNulls&gt;&lt;MaxPrefixes&gt;2&lt;/MaxPrefixes&gt;&lt;MaxAnalysesToReturn&gt;10&lt;/MaxAnalysesToReturn&gt;&lt;/XAmple&gt;&lt;ActiveParser&gt;HC&lt;/ActiveParser&gt;
```

If the fixture has no `ParserParameters` element at all, add one to its `MoMorphData` record: `<ParserParameters><Uni>&lt;ParserParameters&gt;&lt;HC/&gt;...(the text above)...&lt;/ParserParameters&gt;</Uni></ParserParameters>`.

- [ ] **Step 4: Run** the same test → PASS. Run the whole `pg-fwdata` package (`-Mode test -Package pg-fwdata`) to confirm no other fixture assertion changed.

- [ ] **Step 5: Commit** `git commit -am "fwdata fixture: ActiveParser and XAmple caps"`

---

### Task 5: parse `.fwdata` from any `BufRead`

**Files:**
- Modify: `rust/crates/pg-fwdata/src/xml.rs` (`parse_fwdata` at ~line 125)

- [ ] **Step 1: Failing test** (in `xml.rs`'s test module or a new one):

```rust
#[test]
fn parse_fwdata_reader_accepts_in_memory_bytes() {
    let xml = br#"<?xml version="1.0"?><languageproject><rt class="LangProject" guid="00000000-0000-0000-0000-000000000001"/></languageproject>"#;
    let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
    assert!(graph.get("00000000-0000-0000-0000-000000000001").is_some());
}
```

- [ ] **Step 2: Run** `& .\rust\tools\pg.ps1 -Mode quick -Package pg-fwdata` → compile error.

- [ ] **Step 3: Implement.** Rename the body of `parse_fwdata` into

```rust
pub fn parse_fwdata_reader<R: std::io::BufRead>(reader: R) -> Result<RawGraph, ImportError> {
    let mut reader = Reader::from_reader(reader);
    reader.config_mut().trim_text(true);
    // ... existing loop, unchanged ...
}

pub fn parse_fwdata(path: &Path) -> Result<RawGraph, ImportError> {
    let file = File::open(path).map_err(ImportError::Io)?;
    parse_fwdata_reader(BufReader::new(file))
}
```

- [ ] **Step 4: Run** `& .\rust\tools\pg.ps1 -Mode test -Package pg-fwdata` → PASS.

- [ ] **Step 5: Commit** `git commit -am "fwdata: parse from any BufRead"`

---

### Task 6: LDML exemplar extraction (pure function)

**Files:**
- Create: `rust/crates/pg-fwdata/src/fwbackup.rs`
- Modify: `rust/crates/pg-fwdata/src/lib.rs` (`mod fwbackup;`)

- [ ] **Step 1: Failing tests** (in `fwbackup.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const LDML: &str = r#"<?xml version="1.0"?><ldml><identity><language type="mgz"/></identity>
<characters><exemplarCharacters>[ABD-PR-WYabd-pr-wy\u0190\u0254{CH}{a\u0303}{ch}]</exemplarCharacters></characters></ldml>"#;

    #[test]
    fn exemplars_expand_ranges_escapes_and_braced_clusters() {
        let ex = exemplar_characters_from_ldml(LDML);
        assert!(ex.contains(&"a".to_string()));
        assert!(ex.contains(&"d".to_string()));
        assert!(ex.contains(&"e".to_string()), "range d-p includes e");
        assert!(ex.contains(&"p".to_string()));
        assert!(!ex.contains(&"q".to_string()), "q is outside every range");
        assert!(ex.contains(&"\u{0190}".to_string()));
        assert!(ex.contains(&"CH".to_string()));
        assert!(ex.contains(&"a\u{0303}".to_string()));
        assert!(!ex.iter().any(|s| s.contains('{') || s.contains('}') || s.contains('[')));
    }

    #[test]
    fn missing_characters_element_yields_empty() {
        assert!(exemplar_characters_from_ldml("<ldml/>").is_empty());
    }

    #[test]
    fn nfd_is_applied() {
        let ex = exemplar_characters_from_ldml(
            "<ldml><characters><exemplarCharacters>[\u{00E9}]</exemplarCharacters></characters></ldml>",
        );
        assert_eq!(ex, vec!["e\u{0301}".to_string()]);
    }
}
```

- [ ] **Step 2: Run** → compile error.

- [ ] **Step 3: Implement** (`fwbackup.rs`):

```rust
//! `.fwbackup` support: the zip FieldWorks writes holds one `<project>.fwdata` plus
//! `WritingSystemStore/<tag>.ldml`; the LDML `characters/exemplarCharacters` main set is the only
//! record of which characters the vernacular treats as word-forming, which a bare `.fwdata` lacks.

use unicode_normalization::UnicodeNormalization;

/// Text elements of the LDML main exemplar set, NFD. UnicodeSet syntax as FieldWorks writes it:
/// `[` ... `]`, literal chars, `a-z` ranges, `\uXXXX` escapes, and `{...}` multi-character clusters.
pub(crate) fn exemplar_characters_from_ldml(ldml: &str) -> Vec<String> {
    let Some(set) = exemplar_set_text(ldml) else {
        return Vec::new();
    };
    parse_unicode_set(&set)
        .into_iter()
        .map(|s| s.nfd().collect::<String>())
        .collect()
}

fn exemplar_set_text(ldml: &str) -> Option<String> {
    // Only the *main* set: the element with no `type` attribute (auxiliary/index/punctuation carry one).
    let mut rest = ldml;
    while let Some(start) = rest.find("<exemplarCharacters") {
        let after = &rest[start + "<exemplarCharacters".len()..];
        let close = after.find('>')?;
        let attrs = &after[..close];
        let body_start = close + 1;
        let end = after[body_start..].find("</exemplarCharacters>")?;
        let body = &after[body_start..body_start + end];
        if !attrs.contains("type=") {
            return Some(body.trim().to_string());
        }
        rest = &after[body_start + end..];
    }
    None
}

fn parse_unicode_set(set: &str) -> Vec<String> {
    let inner = set.trim().trim_start_matches('[').trim_end_matches(']');
    let chars: Vec<char> = inner.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    // Reads one atom (a literal char or a \uXXXX escape) at `i`, returning it and the next index.
    let read_atom = |i: usize| -> Option<(char, usize)> {
        let c = *chars.get(i)?;
        if c == '\\' {
            if chars.get(i + 1) == Some(&'u') {
                let hex: String = chars.get(i + 2..i + 6)?.iter().collect();
                let cp = u32::from_str_radix(&hex, 16).ok()?;
                return Some((char::from_u32(cp)?, i + 6));
            }
            return Some((*chars.get(i + 1)?, i + 2));
        }
        Some((c, i + 1))
    };
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '{' {
            let mut j = i + 1;
            let mut cluster = String::new();
            while j < chars.len() && chars[j] != '}' {
                match read_atom(j) {
                    Some((ch, nj)) => {
                        cluster.push(ch);
                        j = nj;
                    }
                    None => break,
                }
            }
            if !cluster.is_empty() {
                out.push(cluster);
            }
            i = j + 1;
            continue;
        }
        let Some((lo, ni)) = read_atom(i) else { break };
        if chars.get(ni) == Some(&'-') {
            if let Some((hi, nj)) = read_atom(ni + 1) {
                if hi >= lo {
                    for cp in (lo as u32)..=(hi as u32) {
                        if let Some(ch) = char::from_u32(cp) {
                            out.push(ch.to_string());
                        }
                    }
                    i = nj;
                    continue;
                }
            }
        }
        out.push(lo.to_string());
        i = ni;
    }
    out.sort();
    out.dedup();
    out
}
```

Add `mod fwbackup;` to `lib.rs`. `unicode_normalization` is already a workspace dependency (pg-grammar uses it); add it to `pg-fwdata/Cargo.toml` as `unicode-normalization = { workspace = true }` if not present (check the workspace `Cargo.toml` `[workspace.dependencies]` for its exact key).

- [ ] **Step 4: Run** `& .\rust\tools\pg.ps1 -Mode quick -Package pg-fwdata` → PASS.

- [ ] **Step 5: Commit** `git commit -am "fwdata: LDML exemplar-set parser for .fwbackup input"`

---

### Task 7: `.fwbackup` import

**Files:**
- Modify: `rust/Cargo.toml` — under `[workspace.dependencies]` add `zip = { version = "2", default-features = false, features = ["deflate"] }`
- Modify: `rust/crates/pg-fwdata/Cargo.toml` — `zip = { workspace = true }`
- Modify: `rust/crates/pg-fwdata/src/fwbackup.rs`, `rust/crates/pg-fwdata/src/lib.rs`
- Test: `rust/crates/pg-fwdata/tests/fwbackup_tests.rs` (new)

- [ ] **Step 1: Failing test.** The test builds a `.fwbackup` in a temp dir from the committed `fixture.fwdata` plus a one-line LDML, then imports it:

```rust
use std::io::Write;

fn fixture_fwdata() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fixture.fwdata")
}

fn write_backup(dir: &std::path::Path) -> std::path::PathBuf {
    let out = dir.join("Proj 1.fwbackup");
    let file = std::fs::File::create(&out).unwrap();
    let mut z = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    z.start_file("Proj 1.fwdata", opts).unwrap();
    z.write_all(&std::fs::read(fixture_fwdata()).unwrap()).unwrap();
    z.start_file("WritingSystemStore/xx.ldml", opts).unwrap();
    z.write_all(br#"<ldml><identity><language type="xx"/></identity><characters><exemplarCharacters>[a-c{ch}]</exemplarCharacters></characters></ldml>"#).unwrap();
    z.start_file("BackupSettings/BackupSettings.xml", opts).unwrap();
    z.write_all(b"<BackupSettings/>").unwrap();
    z.finish().unwrap();
    out
}

#[test]
fn fwbackup_imports_the_embedded_fwdata_and_exemplars() {
    let dir = tempfile::tempdir().unwrap();
    let backup = write_backup(dir.path());
    let (snapshot, _report) = pg_fwdata::import_file(&backup).unwrap();
    assert_eq!(snapshot.project.name, "Proj 1");
    // Same lexicon as importing the .fwdata directly.
    let (direct, _) = pg_fwdata::import_file(&fixture_fwdata()).unwrap();
    assert_eq!(snapshot.lexicon, direct.lexicon);
    // The exemplar set belongs to the *default vernacular* ws. The fixture's default vernacular tag
    // is whatever `direct.project.vernacular_writing_systems[0]` is; the test wrote `xx.ldml`, so
    // exemplars are present only if that tag is `xx`. Assert the rule, not a guess:
    if direct.project.vernacular_writing_systems.first().map(String::as_str) == Some("xx") {
        assert_eq!(snapshot.project.exemplar_characters, vec!["a", "b", "c", "ch"]);
    } else {
        assert!(snapshot.project.exemplar_characters.is_empty());
    }
}

#[test]
fn fwbackup_without_fwdata_entry_is_a_hard_error() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("empty.fwbackup");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&out).unwrap());
    z.start_file("BackupSettings/BackupSettings.xml", zip::write::SimpleFileOptions::default()).unwrap();
    z.write_all(b"<BackupSettings/>").unwrap();
    z.finish().unwrap();
    let err = pg_fwdata::import_file(&out).unwrap_err();
    assert!(err.to_string().contains(".fwdata"), "{err}");
}
```

Add `tempfile` and `zip` to `pg-fwdata`'s `[dev-dependencies]` (check the workspace for `tempfile`; it is used by other crates' tests).

After the first test, read the fixture's default vernacular tag and, if it is not `xx`, rename the LDML entry in `write_backup` to `<that tag>.ldml` and make the assertion unconditional. The final committed test must assert `vec!["a","b","c","ch"]` unconditionally.

- [ ] **Step 2: Run** `& .\rust\tools\pg.ps1 -Mode test -Package pg-fwdata -TestTarget fwbackup_tests` → FAIL (`.fwbackup` treated as XML: `NotFwdata` or `Xml` error).

- [ ] **Step 3: Implement.** In `lib.rs`:

```rust
pub fn import_file(path: &Path) -> Result<(Snapshot, ImportReport), ImportError> {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("fwbackup"))
    {
        return fwbackup::import_fwbackup(path);
    }
    let graph = xml::parse_fwdata(path)?;
    let filename_stem = file_stem(path);
    let (snapshot, warnings) = extract::extract(&graph, &filename_stem);
    Ok((snapshot, ImportReport { warnings }))
}

pub(crate) fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}
```

Add an `ImportError` variant: `#[error("not a .fwbackup: {0}")] Backup(String),`.

In `fwbackup.rs`:

```rust
use std::io::Read;
use std::path::Path;

use pg_snapshot::Snapshot;

use crate::{extract, xml, ImportError, ImportReport};

pub(crate) fn import_fwbackup(path: &Path) -> Result<(Snapshot, ImportReport), ImportError> {
    let file = std::fs::File::open(path).map_err(ImportError::Io)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ImportError::Backup(format!("{}: {e}", path.display())))?;

    let mut fwdata_name: Option<String> = None;
    let mut ldml: Vec<(String, String)> = Vec::new(); // (ws tag, ldml text)
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ImportError::Backup(e.to_string()))?;
        let name = entry.name().to_string();
        if name.ends_with(".fwdata") && !name.contains('/') {
            fwdata_name = Some(name);
        } else if let Some(tag) = name
            .strip_prefix("WritingSystemStore/")
            .and_then(|n| n.strip_suffix(".ldml"))
            .filter(|n| !n.contains('/'))
        {
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .map_err(|e| ImportError::Backup(format!("{name}: {e}")))?;
            ldml.push((tag.to_string(), text));
        }
    }
    let fwdata_name = fwdata_name.ok_or_else(|| {
        ImportError::Backup(format!("{}: no top-level .fwdata entry", path.display()))
    })?;

    let graph = {
        let entry = archive
            .by_name(&fwdata_name)
            .map_err(|e| ImportError::Backup(e.to_string()))?;
        xml::parse_fwdata_reader(std::io::BufReader::new(entry))?
    };
    let stem = crate::file_stem(Path::new(&fwdata_name));
    let (mut snapshot, warnings) = extract::extract(&graph, &stem);

    if let Some(default_ws) = snapshot.project.vernacular_writing_systems.first().cloned() {
        if let Some((_, text)) = ldml.iter().find(|(tag, _)| *tag == default_ws) {
            snapshot.project.exemplar_characters = exemplar_characters_from_ldml(text);
        }
    }
    Ok((snapshot, ImportReport { warnings }))
}
```

Keep `exemplar_characters_from_ldml` and its helpers from Task 6 in the same file.

- [ ] **Step 4: Run** `& .\rust\tools\pg.ps1 -Mode test -Package pg-fwdata` → PASS, all targets.

- [ ] **Step 5: Manual check against the real backup on this machine** (it is language data; never commit it or its contents):

```
& .\rust\tools\pg.ps1 -Mode run -Bin pangloss -- import "C:\Users\johnm\Documents\repos\PanGloss\Mbugwe LizzieHC practice 2026-08-10 1414 version to send to John Lambert.fwbackup" $env:TEMP\mbugwe-snapshot.json
```

Expected: exit 0; the JSON's `project.exemplarCharacters` is non-empty and `morphology.parserParameters.activeParser` is `"hc"`, `xample.maxPrefixes` is 1. (The CLI does not need a code change for this: `run_import` already calls `import_file`.)

- [ ] **Step 6: Commit** `git commit -am "fwdata: import .fwbackup zips and their LDML exemplar sets"`

---

### Task 8: CLI `load_grammar` accepts `.fwbackup`

**Files:**
- Modify: `rust/crates/pg-cli/src/main.rs` (`load_grammar`, the `"fwdata" =>` arm at ~line 302)

- [ ] **Step 1:** Change the match arm to `"fwdata" | "fwbackup" =>` (the body already calls `pg_fwdata::import_file`). Update the usage text where `.fwdata` is listed as an accepted grammar input (grep `fwdata` in the usage banner strings) to say `.fwdata/.fwbackup`.

- [ ] **Step 2: Run** `& .\rust\tools\pg.ps1 -Mode check` → clean. Then `& .\rust\tools\pg.ps1 -Mode run -Bin pangloss -- parse "<the Mbugwe .fwbackup path>" kuja` → parses (warnings on stderr are fine).

- [ ] **Step 3: Commit** `git commit -am "cli: accept .fwbackup wherever .fwdata is accepted"`

---

## Self-review notes

- Historical implementation map: parser default/metadata in Task 3; cap fields in Tasks 1, 3, 4;
  LDML extraction in Tasks 6–7. The current end-to-end census and refusal gates are in
  `2026-09-04-xample-projects-on-hc.md` Tasks 1–2 and 6.
- Types used later: `pg_snapshot::ActiveParser::{XAmple, Hc}`, `pg_snapshot::XAmpleParameters` (fields `max_nulls, max_prefixes, max_infixes, max_suffixes, max_interfixes, max_roots: Option<u32>`, `max_analyses_to_return: Option<i32>`), `Project.exemplar_characters: Vec<String>`.
