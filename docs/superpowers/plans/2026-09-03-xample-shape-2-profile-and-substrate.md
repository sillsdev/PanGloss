# XAmple-shape, Plan 2 of 4: parser profile, substrate synthesis, analysis caps on the grammar

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `pg_grammar::compile` resolves a parser profile from the snapshot, compiles an XAmple-configured project the way XAmple would (rules dropped, no default compounding, caps attached to the grammar), and synthesizes the segmental substrate XAmple never needed, reporting every invented segment and every lost restriction as typed notices instead of free strings.

**Architecture:** A new `compile::options` module holds the option/report types. `compile_project_with(snapshot, &CompileOptions) -> CompileOutput` is the real entry; the existing `compile_project(snapshot)` becomes a thin wrapper with `ProfileChoice::Auto` (FieldWorks semantics everywhere, spec §7). Substrate synthesis is an iterative "segment, on failure add the text element, retry" pass in `compile::substrate`, run before the `Ctx` is built so every later phase sees the widened table. The report is a `RefCell<SubstrateReport>` on `Ctx` so the existing skip/fallback sites can record what they dropped without changing their signatures. `AnalysisCaps` lives on `Grammar` so every `Morpher` (direct or FST-confirm) sees it (Plan 3 enforces it).

**Tech Stack:** Rust. Builds/tests only via `rust/tools/pg.ps1` (`-Mode check`, `-Mode quick -Package pg-grammar`, `-Mode test -Package pg-grammar`).

Depends on Plan 1 (`ActiveParser`, `XAmpleParameters`, `Project.exemplar_characters`).
Spec: `docs/superpowers/specs/2026-09-03-xample-shape-grammars.md` §2–§6.

---

## File map

- Create `rust/crates/pg-grammar/src/compile/options.rs` — `ProfileChoice`, `ParserProfile`, `SubstratePolicy`, `CompileOptions`, `Severity`, `Notice`, `SubstrateReport`, `CompileOutput`, notice codes.
- Create `rust/crates/pg-grammar/src/compile/substrate.rs` — alphabet walk + iterative table widening.
- Modify `rust/crates/pg-grammar/src/model.rs` — `AnalysisCaps` struct; `Grammar.analysis_caps: Option<AnalysisCaps>`.
- Modify `rust/crates/pg-grammar/src/load.rs:550` — `analysis_caps: None` in the `Grammar { .. }` literal.
- Modify `rust/crates/pg-grammar/src/compile/mod.rs` — `compile_project_with`, profile resolution, `Ctx.substrate`, wiring.
- Modify `rust/crates/pg-grammar/src/compile/chardef.rs` — `build` returns the `Vec<RawCharDef>` too (so substrate can extend it) via a `CharDefBuild.raw_defs` field.
- Modify `rust/crates/pg-grammar/src/compile/compounding.rs` — `build` takes `allow_default_compounding: bool`.
- Modify `rust/crates/pg-grammar/src/compile/environment.rs`, `lexicon.rs`, `affixes.rs`, `templates.rs` — record skips/fallbacks in `ctx.substrate`.
- Modify `rust/crates/pg-grammar/src/lib.rs` — `pub use compile::{compile_project, compile_project_with, options::*}`.
- Tests: `rust/crates/pg-grammar/src/compile/tests.rs` (existing `fixture()` helper returns `(Snapshot, Fixture)`).

---

### Task 1: `AnalysisCaps` on `Grammar`

**Files:**
- Modify: `rust/crates/pg-grammar/src/model.rs` (near `pub struct Grammar`, ~line 1082)
- Modify: `rust/crates/pg-grammar/src/load.rs:550`

- [ ] **Step 1: Failing test** (in `compile/tests.rs`):

```rust
#[test]
fn xml_loaded_grammars_carry_no_analysis_caps() {
    let g = crate::load::load(MINIMAL_XML).expect("load");
    assert!(g.analysis_caps.is_none());
}
```

`MINIMAL_XML`: reuse whatever minimal HC-XML string `load.rs`'s or `pg-cli`'s tests already define (grep `LexicalEntry` in `rust/crates/pg-grammar/src/load.rs` tests; if none, take the "minimal self-contained grammar" string from `rust/crates/pg-cli/src/main.rs:910` and paste it as a `const`).

- [ ] **Step 2: Run** `& .\rust\tools\pg.ps1 -Mode quick -Package pg-grammar` → compile error (`analysis_caps` missing).

- [ ] **Step 3: Implement.** In `model.rs` before `pub struct Grammar`:

```rust
/// XAmple's per-word analysis caps, present only on a grammar compiled under the XAmple parser
/// profile (`crate::compile::options::ParserProfile::XAmple`). Counting rules are positional and
/// live in `pg_rules`/`pg_parse`; this is data. `max_analyses_to_return == None` means unlimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisCaps {
    pub max_nulls: u32,
    pub max_prefixes: u32,
    pub max_infixes: u32,
    pub max_suffixes: u32,
    pub max_interfixes: u32,
    pub max_roots: u32,
    pub max_analyses_to_return: Option<u32>,
}
```

Add `pub analysis_caps: Option<AnalysisCaps>,` as the last field of `Grammar`. In `load.rs:550`'s literal add `analysis_caps: None,`. `Grammar` derives whatever it derives today; `AnalysisCaps` is `Copy`, so nothing else changes.

- [ ] **Step 4: Run** `& .\rust\tools\pg.ps1 -Mode check` → every `Grammar { .. }` literal in the workspace compiles (grep `Grammar {` outside pg-grammar; `compile/mod.rs` is fixed in Task 4). Then `-Mode quick -Package pg-grammar` → PASS.

- [ ] **Step 5: Commit** `git commit -am "model: AnalysisCaps on Grammar (None for XML-loaded grammars)"`

---

### Task 2: options and report types

**Files:**
- Create: `rust/crates/pg-grammar/src/compile/options.rs`
- Modify: `rust/crates/pg-grammar/src/compile/mod.rs` (`pub mod options;`), `rust/crates/pg-grammar/src/lib.rs`

- [ ] **Step 1: Failing test** (in `options.rs` test module):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use pg_snapshot::ActiveParser;

    #[test]
    fn auto_follows_the_snapshot_and_explicit_choices_override_it() {
        assert_eq!(ProfileChoice::Auto.resolve(ActiveParser::XAmple), ParserProfile::XAmple);
        assert_eq!(ProfileChoice::Auto.resolve(ActiveParser::Hc), ParserProfile::Hc);
        assert_eq!(ProfileChoice::Hc.resolve(ActiveParser::XAmple), ParserProfile::Hc);
        assert_eq!(ProfileChoice::XAmple.resolve(ActiveParser::Hc), ParserProfile::XAmple);
    }

    #[test]
    fn substrate_auto_is_synthesize_for_xample_or_the_flag_and_strict_otherwise() {
        assert_eq!(SubstratePolicy::Auto.resolve(ParserProfile::XAmple, false), ResolvedSubstrate::Synthesize);
        assert_eq!(SubstratePolicy::Auto.resolve(ParserProfile::Hc, true), ResolvedSubstrate::Synthesize);
        assert_eq!(SubstratePolicy::Auto.resolve(ParserProfile::Hc, false), ResolvedSubstrate::Strict);
        assert_eq!(SubstratePolicy::Strict.resolve(ParserProfile::XAmple, true), ResolvedSubstrate::Strict);
    }

    #[test]
    fn report_is_empty_by_default_and_counts_everything() {
        let mut r = SubstrateReport::default();
        assert!(r.is_empty());
        r.invented_segments.push("q".into());
        assert!(!r.is_empty());
    }
}
```

- [ ] **Step 2: Run** → compile error.

- [ ] **Step 3: Implement** (`options.rs`):

```rust
//! Caller-facing options and reports for `super::compile_project_with`. Profile semantics:
//! `docs/superpowers/specs/2026-09-03-xample-shape-grammars.md` §2–§6.

use pg_snapshot::ActiveParser;

/// How to pick the parser profile: follow the snapshot (FieldWorks' rule: an absent
/// `ActiveParser` is XAmple), or force one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProfileChoice {
    #[default]
    Auto,
    Hc,
    XAmple,
}

/// The profile a compilation actually ran under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserProfile {
    Hc,
    XAmple,
}

impl ProfileChoice {
    pub fn resolve(self, active: ActiveParser) -> ParserProfile {
        match self {
            ProfileChoice::Hc => ParserProfile::Hc,
            ProfileChoice::XAmple => ParserProfile::XAmple,
            ProfileChoice::Auto => match active {
                ActiveParser::XAmple => ParserProfile::XAmple,
                ActiveParser::Hc => ParserProfile::Hc,
            },
        }
    }
}

/// Whether to widen the character table so every listed form segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SubstratePolicy {
    #[default]
    Auto,
    Strict,
    Synthesize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedSubstrate {
    Strict,
    Synthesize,
}

impl SubstratePolicy {
    pub fn resolve(self, profile: ParserProfile, accept_unspecified_graphemes: bool) -> ResolvedSubstrate {
        match self {
            SubstratePolicy::Strict => ResolvedSubstrate::Strict,
            SubstratePolicy::Synthesize => ResolvedSubstrate::Synthesize,
            SubstratePolicy::Auto => {
                if profile == ParserProfile::XAmple || accept_unspecified_graphemes {
                    ResolvedSubstrate::Synthesize
                } else {
                    ResolvedSubstrate::Strict
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompileOptions {
    pub profile: ProfileChoice,
    pub substrate: SubstratePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Printed under a banner by every CLI command; never suppressed.
    Loud,
    Info,
}

/// A typed fact about how the grammar was compiled. `code` is stable and grep-able; see the
/// `codes` module for the closed list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
}

pub mod codes {
    pub const ACTIVE_PARSER_ABSENT: &str = "xample.active-parser-absent";
    pub const RULES_DROPPED: &str = "xample.rules-dropped";
    pub const DEFAULT_COMPOUNDING_OFF: &str = "xample.default-compounding-off";
    pub const CAPS_ENFORCED: &str = "xample.caps-enforced";
    pub const SEGMENTS_INVENTED: &str = "substrate.segments-invented";
    pub const BOUNDARIES_INVENTED: &str = "substrate.boundaries-invented";
    pub const CLASS_UNRESOLVED: &str = "substrate.class-unresolved";
    pub const ALLOMORPH_UNRESTRICTED: &str = "substrate.allomorph-unrestricted";
    pub const ALLOMORPH_SKIPPED: &str = "substrate.allomorph-skipped";
}

/// What substrate synthesis did, and what still could not be represented.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubstrateReport {
    /// Text elements added as featureless segments, NFD, in the order they were discovered.
    pub invented_segments: Vec<String>,
    /// Text elements added as boundaries.
    pub invented_boundaries: Vec<String>,
    /// `(allomorph guid, environment representation)` whose `[X]` could not be resolved.
    pub unresolved_classes: Vec<(String, String)>,
    /// Allomorph guids that lost an environment restriction (HCLoader's blank-environment fallback).
    pub unrestricted_allomorphs: Vec<String>,
    /// `(allomorph guid, reason)` for allomorphs dropped from the grammar entirely.
    pub skipped_allomorphs: Vec<(String, String)>,
}

impl SubstrateReport {
    pub fn is_empty(&self) -> bool {
        self.invented_segments.is_empty()
            && self.invented_boundaries.is_empty()
            && self.unresolved_classes.is_empty()
            && self.unrestricted_allomorphs.is_empty()
            && self.skipped_allomorphs.is_empty()
    }

    /// One `Notice` per non-empty category, so the CLI prints counts and the first few items.
    pub fn notices(&self) -> Vec<Notice> {
        fn summarize<T: std::fmt::Debug>(code: &'static str, what: &str, items: &[T]) -> Option<Notice> {
            if items.is_empty() {
                return None;
            }
            let head: Vec<String> = items.iter().take(8).map(|i| format!("{i:?}")).collect();
            let more = if items.len() > 8 { format!(" (+{} more)", items.len() - 8) } else { String::new() };
            Some(Notice {
                code,
                severity: Severity::Loud,
                message: format!("{}: {} {what}: {}{more}", code, items.len(), head.join(", ")),
            })
        }
        [
            summarize(codes::SEGMENTS_INVENTED, "segments invented from listed forms", &self.invented_segments),
            summarize(codes::BOUNDARIES_INVENTED, "boundaries invented from listed forms", &self.invented_boundaries),
            summarize(codes::CLASS_UNRESOLVED, "environment classes unresolved", &self.unresolved_classes),
            summarize(codes::ALLOMORPH_UNRESTRICTED, "allomorphs lost an environment restriction", &self.unrestricted_allomorphs),
            summarize(codes::ALLOMORPH_SKIPPED, "allomorphs dropped", &self.skipped_allomorphs),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

pub struct CompileOutput {
    pub grammar: crate::model::Grammar,
    pub warnings: Vec<String>,
    pub notices: Vec<Notice>,
    pub substrate: SubstrateReport,
    pub profile: ParserProfile,
}
```

`lib.rs`: `pub use compile::{compile_project, compile_project_with}; pub use compile::options::{CompileOptions, CompileOutput, Notice, ParserProfile, ProfileChoice, Severity, SubstratePolicy, SubstrateReport};` (`compile_project_with` arrives in Task 4; add its re-export then).

- [ ] **Step 4: Run** `& .\rust\tools\pg.ps1 -Mode quick -Package pg-grammar` → PASS.

- [ ] **Step 5: Commit** `git commit -am "compile: options, profile resolution, typed notices and SubstrateReport"`

---

### Task 3: substrate synthesis (pure over `RawCharDef`s)

**Files:**
- Modify: `rust/crates/pg-grammar/src/compile/chardef.rs` — `CharDefBuild` gains `pub raw_defs: Vec<RawCharDef>` (clone `raw_defs` before `from_raw` consumes it) so the substrate pass can rebuild the table.
- Create: `rust/crates/pg-grammar/src/compile/substrate.rs`

- [ ] **Step 1: Failing tests** (in `substrate.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_elements_keep_combining_marks_with_their_base() {
        assert_eq!(text_element_at("ka\u{0303}n", 1), "a\u{0303}");
        assert_eq!(text_element_at("ka\u{0303}n", 2), "a\u{0303}", "landing on the mark backs up to the base");
        assert_eq!(text_element_at("kan", 2), "n");
    }

    #[test]
    fn literal_runs_of_an_environment_exclude_classes_anchors_and_optional_marks() {
        let runs = environment_literals("/ [C] a _ # (b)");
        assert_eq!(runs, vec!["a".to_string(), "b".to_string()]);
        assert!(environment_literals("/_[V]").is_empty());
    }

    #[test]
    fn classify_uses_exemplars_when_present_and_alphanumeric_otherwise() {
        let ex = vec!["a".to_string(), "\u{02BC}".to_string()];
        assert_eq!(classify("a", &ex), CharDefKind::Segment);
        assert_eq!(classify("q", &ex), CharDefKind::Boundary, "not in a known exemplar set");
        assert_eq!(classify("q", &[]), CharDefKind::Segment);
        assert_eq!(classify("-", &[]), CharDefKind::Boundary);
        assert_eq!(classify("\u{02BC}", &[]), CharDefKind::Segment, "modifier letter apostrophe is alphabetic");
    }
}
```

- [ ] **Step 2: Run** → compile error.

- [ ] **Step 3: Implement** (`substrate.rs`):

```rust
//! Widens the character-definition table until every listed form segments, the way FieldWorks'
//! `HCLoader.Segment` does under `AcceptUnspecifiedGraphemes` (HCLoader.cs:2532-2560): segment,
//! and on failure add the text element at the failure position and retry. Spec §4.

use unicode_normalization::UnicodeNormalization;

use pg_snapshot::Snapshot;

use crate::chardef::{CharDefKind, CharDefTable, RawCharDef};
use crate::featsys::PhonFeatureSystem;
use crate::nfd::nfd;
use crate::GrammarError;

use super::options::SubstrateReport;

/// Every string the compiler will later segment against the table: allomorph forms (default
/// vernacular writing system, spaces already mapped to `.`) and environment literals. Lexical
/// patterns (`[`, `(`, `*` in a form) are skipped like HCLoader's `IsLexicalPattern` does.
pub(crate) fn segmentable_texts(snapshot: &Snapshot) -> Vec<String> {
    let default_ws = snapshot.project.vernacular_writing_systems.first().map(String::as_str);
    let mut out = Vec::new();
    for entry in &snapshot.lexicon.entries {
        for allo in entry.all_allomorphs() {
            if allo.is_abstract {
                continue;
            }
            if let Some(form) = super::best_ws(&allo.forms, default_ws) {
                let form = super::format_form(form);
                if form.is_empty() || form.contains('[') || form.contains('(') || form.contains('*') {
                    continue;
                }
                out.push(form);
            }
        }
    }
    for env in &snapshot.phonology.environments {
        out.extend(environment_literals(&env.representation));
    }
    out
}

/// Literal segment runs of an environment string: everything outside `[...]`, minus `/`, `_`,
/// `#`, `(`, `)` and whitespace, split on those separators.
pub(crate) fn environment_literals(representation: &str) -> Vec<String> {
    let mut runs = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    for c in representation.chars() {
        match c {
            '[' => {
                depth += 1;
                if !cur.is_empty() { runs.push(std::mem::take(&mut cur)); }
            }
            ']' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            '/' | '_' | '#' | '(' | ')' | '~' => {
                if !cur.is_empty() { runs.push(std::mem::take(&mut cur)); }
            }
            c if c.is_whitespace() => {
                if !cur.is_empty() { runs.push(std::mem::take(&mut cur)); }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() { runs.push(cur); }
    runs
}

/// The text element (base char plus following combining marks) at `char_index` of `s`; if the
/// index lands on a combining mark, backs up to its base. Mirrors `StringInfo.GetNextTextElement`
/// plus HCLoader's "unknown diacritic" branch.
pub(crate) fn text_element_at(s: &str, char_index: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let mut start = char_index.min(chars.len() - 1);
    while start > 0 && is_combining(chars[start]) {
        start -= 1;
    }
    let mut end = start + 1;
    while end < chars.len() && is_combining(chars[end]) {
        end += 1;
    }
    chars[start..end].iter().collect()
}

fn is_combining(c: char) -> bool {
    unicode_normalization::char::is_combining_mark(c)
}

/// Segment when the element is a known exemplar (or, with no exemplar data, when its base
/// character is alphanumeric); boundary otherwise. Punctuation and symbols become boundaries.
pub(crate) fn classify(element: &str, exemplars: &[String]) -> CharDefKind {
    if !exemplars.is_empty() {
        return if exemplars.iter().any(|e| nfd(e) == nfd(element)) {
            CharDefKind::Segment
        } else {
            CharDefKind::Boundary
        };
    }
    match element.chars().next() {
        Some(c) if c.is_alphanumeric() => CharDefKind::Segment,
        _ => CharDefKind::Boundary,
    }
}

/// Widen `raw_defs` until every text in `texts` segments; returns the rebuilt table. Each
/// invented definition is recorded in `report`. Bounded: one definition per distinct element,
/// so the loop terminates after at most (distinct elements) rebuilds.
pub(crate) fn synthesize(
    mut raw_defs: Vec<RawCharDef>,
    phon: &PhonFeatureSystem,
    texts: &[String],
    exemplars: &[String],
    report: &mut SubstrateReport,
) -> Result<(CharDefTable, Vec<RawCharDef>), GrammarError> {
    let mut table = CharDefTable::from_raw("main".to_string(), None, raw_defs.clone(), phon)?;
    let mut known: hashbrown::HashSet<String> =
        raw_defs.iter().flat_map(|d| d.representations.iter().map(|r| nfd(r))).collect();
    for text in texts {
        loop {
            match crate::segment::segment(&table, text) {
                Ok(_) => break,
                Err(err) => {
                    let element = text_element_at(text, err.position);
                    let key = nfd(&element);
                    if element.is_empty() || known.contains(&key) {
                        // The table already has it and still cannot segment here: a genuine
                        // pattern-syntax or ordering problem, not a missing grapheme. Leave it to
                        // the compile phase that owns this text to report the skip.
                        break;
                    }
                    let kind = classify(&element, exemplars);
                    match kind {
                        CharDefKind::Segment => report.invented_segments.push(key.clone()),
                        CharDefKind::Boundary => report.invented_boundaries.push(key.clone()),
                    }
                    known.insert(key.clone());
                    raw_defs.push(RawCharDef {
                        xml_id: format!("__synth__{}", hex_of(&key)),
                        kind,
                        representations: vec![element],
                        feature_values: Vec::new(),
                    });
                    table = CharDefTable::from_raw("main".to_string(), None, raw_defs.clone(), phon)?;
                }
            }
        }
    }
    Ok((table, raw_defs))
}

fn hex_of(s: &str) -> String {
    s.chars().map(|c| format!("{:x}", c as u32)).collect::<Vec<_>>().join("_")
}
```

`entry.all_allomorphs()`: check `pg_snapshot::lexicon::LexEntry` for how allomorphs are exposed (likely `lexeme_form: Allomorph` plus `alternate_forms: Vec<Allomorph>`, or a single `allomorphs: Vec<Allomorph>`); write a local iterator over whichever it is instead of assuming a method. `unicode_normalization::char::is_combining_mark` exists in the `unicode-normalization` crate (already a pg-grammar dependency).

Add `pub(crate) mod substrate;` to `compile/mod.rs`. Change `chardef::build` to also return `raw_defs` (clone before `from_raw`).

- [ ] **Step 4: Run** `& .\rust\tools\pg.ps1 -Mode quick -Package pg-grammar` → PASS.

- [ ] **Step 5: Commit** `git commit -am "compile: substrate synthesis pass over the character table"`

---

### Task 4: `compile_project_with` — profile, substrate, caps, notices

**Files:**
- Modify: `rust/crates/pg-grammar/src/compile/mod.rs`
- Modify: `rust/crates/pg-grammar/src/compile/compounding.rs` (`build` gains `allow_default_compounding: bool`)
- Modify: `rust/crates/pg-grammar/src/lib.rs` (re-export `compile_project_with`)

- [ ] **Step 1: Failing tests** (in `compile/tests.rs`, using the existing `fixture()`):

```rust
use crate::compile::options::{CompileOptions, ParserProfile, ProfileChoice, SubstratePolicy};
use pg_snapshot::ActiveParser;

#[test]
fn absent_active_parser_resolves_to_xample_and_says_so_loudly() {
    let (snapshot, _f) = fixture();
    assert_eq!(snapshot.morphology.parser_parameters.active_parser, ActiveParser::XAmple);
    let out = crate::compile_project_with(&snapshot, &CompileOptions::default()).unwrap();
    assert_eq!(out.profile, ParserProfile::XAmple);
    assert!(out.notices.iter().any(|n| n.code == crate::compile::options::codes::ACTIVE_PARSER_ABSENT));
    assert!(out.grammar.analysis_caps.is_some());
}

#[test]
fn hc_profile_is_todays_behaviour() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::Hc;
    let out = crate::compile_project_with(&snapshot, &CompileOptions::default()).unwrap();
    assert_eq!(out.profile, ParserProfile::Hc);
    assert!(out.grammar.analysis_caps.is_none());
    assert!(out.notices.is_empty());
    let (legacy, _w) = crate::compile_project(&snapshot).unwrap();
    assert_eq!(legacy.prules.len(), out.grammar.prules.len());
}

#[test]
fn xample_profile_drops_rules_loudly_and_disables_default_compounding() {
    let (mut snapshot, _f) = fixture_with_one_rewrite_rule(); // see note below
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.morphology.parser_parameters.no_default_compounding = false;
    snapshot.morphology.compound_rules.clear();
    let out = crate::compile_project_with(&snapshot, &CompileOptions::default()).unwrap();
    assert!(out.grammar.prules.is_empty());
    let dropped = out.notices.iter().find(|n| n.code == crate::compile::options::codes::RULES_DROPPED).unwrap();
    assert!(dropped.message.contains('1'), "{}", dropped.message);
    assert!(out.notices.iter().any(|n| n.code == crate::compile::options::codes::DEFAULT_COMPOUNDING_OFF));
    assert!(
        !out.grammar.mrules.iter().any(|r| matches!(r, crate::model::MorphRuleDef::Compounding(_))),
        "no synthesized default compound rules under the XAmple profile"
    );
    let (hc, _w) = crate::compile_project_with(&snapshot, &CompileOptions { profile: ProfileChoice::Hc, ..Default::default() })
        .map(|o| (o.grammar, o.warnings)).unwrap();
    assert_eq!(hc.prules.len(), 1);
}

#[test]
fn caps_take_fieldworks_defaults_when_the_block_is_absent() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.compound_rules.clear();
    let out = crate::compile_project_with(&snapshot, &CompileOptions { profile: ProfileChoice::XAmple, ..Default::default() }).unwrap();
    let caps = out.grammar.analysis_caps.unwrap();
    assert_eq!(caps.max_nulls, 1);
    assert_eq!(caps.max_prefixes, 5);
    assert_eq!(caps.max_suffixes, 5);
    assert_eq!(caps.max_infixes, 1);
    assert_eq!(caps.max_interfixes, 0);
    assert_eq!(caps.max_roots, 1);
    assert_eq!(caps.max_analyses_to_return, Some(20));
}

#[test]
fn caps_follow_the_block_and_sub_one_analyses_means_unlimited() {
    let (mut snapshot, _f) = fixture();
    let x = &mut snapshot.morphology.parser_parameters.xample;
    x.max_nulls = Some(0);
    x.max_prefixes = Some(1);
    x.max_roots = Some(3);
    x.max_analyses_to_return = Some(-1);
    let out = crate::compile_project_with(&snapshot, &CompileOptions { profile: ProfileChoice::XAmple, ..Default::default() }).unwrap();
    let caps = out.grammar.analysis_caps.unwrap();
    assert_eq!((caps.max_nulls, caps.max_prefixes, caps.max_roots), (0, 1, 3));
    assert_eq!(caps.max_analyses_to_return, None);
}

#[test]
fn accept_unspecified_graphemes_now_acts_the_inert_control_gate() {
    // Red before this plan: the flag was parsed and read by nothing (CLAUDE.md, "a control that cannot act").
    let (mut snapshot, f) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::Hc;
    snapshot.morphology.parser_parameters.accept_unspecified_graphemes = true;
    set_stem_form(&mut snapshot, &f, "qat"); // `q` is not in the fixture's phoneme set — verify with an assert on snapshot.phonology.phonemes
    let out = crate::compile_project_with(&snapshot, &CompileOptions::default()).unwrap();
    assert!(out.substrate.invented_segments.contains(&"q".to_string()));
    assert!(out.grammar.entries.iter().any(|e| e.authored_id == f.stem_entry), "the stem survived");
    // And strict mode still drops it, recording the skip in the report rather than only in a string.
    snapshot.morphology.parser_parameters.accept_unspecified_graphemes = false;
    let strict = crate::compile_project_with(&snapshot, &CompileOptions::default()).unwrap();
    assert!(strict.substrate.skipped_allomorphs.iter().any(|(_, why)| why.contains("cannot segment")));
}

#[test]
fn exemplars_decide_segment_versus_boundary() {
    let (mut snapshot, f) = fixture();
    snapshot.project.exemplar_characters = vec!["a".into(), "t".into()];
    set_stem_form(&mut snapshot, &f, "q-at");
    let out = crate::compile_project_with(&snapshot, &CompileOptions { profile: ProfileChoice::XAmple, ..Default::default() }).unwrap();
    assert!(out.substrate.invented_boundaries.contains(&"q".to_string()), "q is not an exemplar");
    assert!(out.substrate.invented_boundaries.contains(&"-".to_string()));
}
```

Helper notes for the implementer: `fixture_with_one_rewrite_rule()` — if `tests.rs` already has a test that builds a snapshot with a `PhonologicalRule::Rewrite`, extract its construction into this helper; otherwise build the smallest `pg_snapshot::phonology::RewriteRule` the existing rewrite tests use. `set_stem_form(&mut snapshot, &f, form)` — find `f.stem_entry` in `snapshot.lexicon.entries` and set its lexeme form's `forms[0].text` (adapt to `WsForm`'s field names) to `form`. Both are ordinary test helpers; write them at the top of the test module.

- [ ] **Step 2: Run** `& .\rust\tools\pg.ps1 -Mode quick -Package pg-grammar` → compile errors / failures.

- [ ] **Step 3: Implement.** In `compile/mod.rs`:

1. New public entry, and the wrapper:

```rust
pub fn compile_project(snapshot: &Snapshot) -> Result<(Grammar, Vec<String>), GrammarError> {
    let out = compile_project_with(snapshot, &CompileOptions::default())?;
    let mut warnings = out.warnings;
    warnings.extend(out.notices.iter().map(|n| n.message.clone()));
    Ok((out.grammar, warnings))
}

pub fn compile_project_with(snapshot: &Snapshot, opts: &CompileOptions) -> Result<CompileOutput, GrammarError> {
    let mut warnings: Vec<String> = Vec::new();
    let mut notices: Vec<Notice> = Vec::new();
    let pp = &snapshot.morphology.parser_parameters;
    let profile = opts.profile.resolve(pp.active_parser);
    if profile == ParserProfile::XAmple && opts.profile == ProfileChoice::Auto {
        notices.push(Notice {
            code: codes::ACTIVE_PARSER_ABSENT,
            severity: Severity::Loud,
            message: format!(
                "{}: the project's ActiveParser is XAmple (FieldWorks treats an absent element as XAmple); \
                 compiling under the XAmple profile — pass an explicit profile to override",
                codes::ACTIVE_PARSER_ABSENT
            ),
        });
    }
    let substrate_mode = opts.substrate.resolve(profile, pp.accept_unspecified_graphemes);
    let substrate = std::cell::RefCell::new(SubstrateReport::default());
    // ... existing body, with the changes below ...
}
```

(`ACTIVE_PARSER_ABSENT` is emitted whenever Auto resolved to XAmple; the snapshot cannot distinguish "absent" from an explicit `XAmple`, and both mean the same thing to FieldWorks.)

2. After `chardef::build`, before natclass: when `substrate_mode == ResolvedSubstrate::Synthesize`:

```rust
    let (char_table, _raw_defs) = if substrate_mode == ResolvedSubstrate::Synthesize {
        let texts = substrate::segmentable_texts(snapshot);
        substrate::synthesize(raw_defs, &phon_features, &texts, &snapshot.project.exemplar_characters, &mut substrate.borrow_mut())?
    } else {
        (char_table, raw_defs)
    };
```

(`raw_defs` comes from the widened `CharDefBuild`.)

3. `Ctx` gains `pub substrate: &'a std::cell::RefCell<SubstrateReport>,`; set it when building `ctx`.

4. Compounding: `compounding::build(snapshot, &ctx, &mut acc, &mut morphology_mrules, profile == ParserProfile::Hc, &mut warnings)?;` — inside `compounding::build`, the condition becomes `if snapshot.morphology.compound_rules.is_empty() && allow_default_compounding && !snapshot.morphology.parser_parameters.no_default_compounding`. When `profile == XAmple && compound_rules.is_empty() && !no_default_compounding`, push a `DEFAULT_COMPOUNDING_OFF` Loud notice: `"xample.default-compounding-off: HermitCrab would synthesize 2 default compound rules for a project with none; XAmple emits none, so none were created"`.

5. Rules: 

```rust
    let (prules, morphology_prules, clitic_prules) = match profile {
        ParserProfile::Hc => rules::build(snapshot, &ctx, &mut warnings)?,
        ParserProfile::XAmple => {
            let n = snapshot.phonology.rules.len();
            if n > 0 {
                notices.push(Notice {
                    code: codes::RULES_DROPPED,
                    severity: Severity::Loud,
                    message: format!("{}: {n} phonological rule(s) dropped — XAmple never applies them", codes::RULES_DROPPED),
                });
            }
            (Vec::new(), Vec::new(), Vec::new())
        }
    };
```

6. Caps:

```rust
    let analysis_caps = match profile {
        ParserProfile::Hc => None,
        ParserProfile::XAmple => {
            let x = &pp.xample;
            let has_compounds = !snapshot.morphology.compound_rules.is_empty();
            let caps = AnalysisCaps {
                max_nulls: x.max_nulls.unwrap_or(1),
                max_prefixes: x.max_prefixes.unwrap_or(5),
                // FieldWorks emits 1 when the project's morph-type list contains "infix", which every standard project's does.
                max_infixes: x.max_infixes.unwrap_or(1),
                max_suffixes: x.max_suffixes.unwrap_or(5),
                max_interfixes: x.max_interfixes.unwrap_or(0),
                // FieldWorks: explicit value; else 3 with a linker, 2 with any compound rule, else 1. The snapshot has no linker field, so 2/1 only.
                max_roots: x.max_roots.unwrap_or(if has_compounds { 2 } else { 1 }),
                max_analyses_to_return: match x.max_analyses_to_return {
                    None => Some(20),
                    Some(n) if n < 1 => None,
                    Some(n) => Some(n as u32),
                },
            };
            notices.push(Notice {
                code: codes::CAPS_ENFORCED,
                severity: Severity::Loud,
                message: format!("{}: enforcing XAmple analysis caps {caps:?}", codes::CAPS_ENFORCED),
            });
            Some(caps)
        }
    };
```

Set `analysis_caps` in the `Grammar { .. }` literal.

7. Return: after the compaction passes,

```rust
    let substrate = substrate.into_inner();
    notices.extend(substrate.notices());
    Ok(CompileOutput { grammar, warnings, notices, substrate, profile })
```

8. Record skips/fallbacks: in `lexicon.rs:265` (and the sibling at ~558), beside the existing `warnings.push(...)` add `ctx.substrate.borrow_mut().skipped_allomorphs.push((allo.guid.clone(), e.clone()));`. In `affixes.rs` wherever an affix allomorph is skipped for "cannot segment" (grep `; skipped` in that file), do the same with that allomorph's guid. In `environment.rs::resolve_environment_defs`'s `Err(e)` arm add `ctx.substrate.borrow_mut().unrestricted_allomorphs.push(allo_guid.to_string()); ctx.substrate.borrow_mut().unresolved_classes.push((allo_guid.to_string(), env.representation.clone()));` — only push `unresolved_classes` when `e` mentions a natural class (the error text from `load_environment_pattern` for an unknown `[X]`; read that function and match on its wording, or return a small enum from `parse_environment` and match on the variant — the enum is better).

- [ ] **Step 4: Run** `& .\rust\tools\pg.ps1 -Mode check` (all crates still compile — `Ctx` literal sites are all inside `compile/mod.rs`), then `& .\rust\tools\pg.ps1 -Mode test -Package pg-grammar` → all PASS, including the two red gates.

- [ ] **Step 5: Commit** `git commit -am "compile: parser profile (XAmple drops rules, no default compounding, caps), substrate synthesis wired, typed notices"`

---

### Task 5: classify every `compile_project` caller

**Files:** every path listed by `rg -n 'compile_project\(' rust/crates --glob '!**/target/**'`.

- [ ] **Step 1:** For each call site decide, and change only these to explicit `ProfileChoice::Hc`:
  - `rust/crates/pg-cli/tests/fwdata_conformance_gate.rs:252` — compares HC-Rust to a committed `hc.dll` oracle over Sena: **Hc**.
  - `rust/crates/pg-cli/tests/fwdata_grammar_equivalence_gate.rs:58` — compares the snapshot compile to the legacy HC XML export: **Hc**.
  - `rust/crates/pg-foma/tests/five_language_backend_reports_gate.rs:15`, `mbugwe_corpus_smoke_gate.rs:13` — record what each backend does on the real projects: **Auto** (they measure the product; Sena now announces XAmple). Read each gate's assertions first; if one asserts a Sena analysis count that caps would change, switch that gate to Hc and say so in its doc comment.
  - `rust/crates/pg-cli/src/main.rs:298,309`, `pg-foma/src/worker.rs:286`, every example — **Auto** (Plan 3 adds the CLI flag; the worker gets the profile from its request in Plan 3).

  Change pattern:
  ```rust
  let out = pg_grammar::compile_project_with(&snapshot, &pg_grammar::CompileOptions { profile: pg_grammar::ProfileChoice::Hc, ..Default::default() }).expect("compile_project must succeed");
  let grammar = out.grammar;
  ```

- [ ] **Step 2: Run** `& .\rust\tools\pg.ps1 -Mode test` (full suite; ~2 minutes of execution after compile). Every previously green test stays green. Any Sena-dependent test that flips is either an HC-parity test (→ Hc, Step 1) or a real consequence to record in the spec's §7 table — do not "fix" it by weakening an assertion.

- [ ] **Step 3: Commit** `git commit -am "gates: HC-parity gates compile under an explicit Hc profile"`

---

## Self-review notes

- Spec §3.1 rules dropped: Task 4 step 5. §3.2 compounding: step 4. §3.3 substrate: Tasks 3–4. §3.4 caps + defaults: step 6 (linker refinement documented as unavailable). §4 report: Tasks 2–4. §6 notice codes: Task 2. §7 Sena: Task 5. §9 gate 1: Task 4's `accept_unspecified_graphemes_now_acts_the_inert_control_gate`.
- Names used by Plans 3–4: `pg_grammar::{compile_project_with, CompileOptions, ProfileChoice, ParserProfile, SubstratePolicy, Notice, Severity, SubstrateReport, CompileOutput}`, `pg_grammar::model::AnalysisCaps`, `Grammar.analysis_caps`.
