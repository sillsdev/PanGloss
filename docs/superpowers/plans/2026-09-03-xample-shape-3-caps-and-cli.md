# XAmple-shape, Plan 3 of 4: enforce analysis caps in HC-Rust, CLI profile flag and loud banner, census gate

> **SUPERSEDED 2026-09-04 — DO NOT EXECUTE.** XAMPLE caps are resource-containment and comparison
> metadata, not HC validity predicates. There will be no `--parser-profile`, cap-aware `Grammar`, or
> XAMPLE-profile banner. See `2026-09-04-xample-projects-on-hc.md`.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A grammar carrying `analysis_caps` yields, from HC-Rust and from every FST propose + HC-confirm path, only analyses XAmple's caps admit; the CLI lets a user pick the profile, prints XAmple/substrate notices under an unmissable banner, and a census gate records which real projects resolve to which profile.

**Architecture:** Counting is a pure function in `pg_rules::caps` over a finished `Word` (spans derived exactly as `pg_rules::validity` derives them). `Morpher::is_word_valid_traced` gains one clause that calls it when `g.analysis_caps` is `Some`, so the FST-confirm path (which builds an uncapped `Morpher`) is covered automatically. `MaxAnalysesToReturn` truncates the final list after a deterministic sort by signature in `parse_word_core_selected`. The CLI grows `--parser-profile auto|hc|xample` and a banner printer.

**Tech Stack:** Rust. Builds/tests only via `rust/tools/pg.ps1`.

Depends on Plans 1–2 (`Grammar.analysis_caps`, `compile_project_with`, `Notice`).
Spec: `docs/superpowers/specs/2026-09-03-xample-shape-grammars.md` §5, §6, §7, §9.

---

## File map

- Create `rust/crates/pg-rules/src/caps.rs` — `MorphCounts`, `count_morphs(g, w)`, `caps_violation(caps, &counts) -> Option<CapViolation>`.
- Modify `rust/crates/pg-rules/src/lib.rs` — `pub mod caps;`.
- Modify `rust/crates/pg-rules/src/trace.rs` — `FailureReason::AnalysisCap(CapViolation)`.
- Modify `rust/crates/pg-parse/src/morpher.rs` — the clause in `is_word_valid_traced`; the truncation after `ordered_matches`; `ParseOutcome.truncated_to_cap: bool`.
- Create `rust/crates/pg-parse/tests/xample_caps_gate.rs` — the caps gate over a synthetic snapshot.
- Modify `rust/crates/pg-cli/src/main.rs` — `--parser-profile`, banner, `load_grammar` returns notices.
- Modify `rust/crates/pg-foma/src/worker.rs:286` and its request type — carry the profile choice.
- Create `rust/crates/pg-cli/tests/xample_profile_census_gate.rs` — self-skipping census over real projects.

---

### Task 1: morph counting over a finished word

**Files:**
- Create: `rust/crates/pg-rules/src/caps.rs`
- Modify: `rust/crates/pg-rules/src/lib.rs`

Spans: `pg_rules::validity` derives each `MorphRecord`'s span as `[order_i, order_{i+1} - 1]` over records sorted by `order`, the last running to the shape's last interior index — copy that derivation (read `validity.rs`'s "Morph-span derivation" section and the helper it uses; if the helper is private, make it `pub(crate)` and call it rather than duplicating).

- [ ] **Step 1: Failing tests** (in `caps.rs`; build `MorphRecord`s by hand with `MorphRecord::new` and set `status`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::word::MorphStatus;

    // Root-ness comes from `g.allomorph_owners`; the tests use a tiny fake: a closure `is_root(AllomorphId) -> bool`.
    fn counts(records: &[(u32 /*allomorph*/, u32 /*order*/, bool /*root*/, MorphStatus)], last_interior: u32) -> MorphCounts {
        let recs: Vec<MorphRecord> = records
            .iter()
            .map(|(a, o, _, st)| {
                let mut r = MorphRecord::new(AllomorphId(*a), MorphemeId(*a), *o);
                r.status = *st;
                r
            })
            .collect();
        let roots: Vec<bool> = records.iter().map(|r| r.2).collect();
        count_morph_records(&recs, |a| roots[a.0 as usize], last_interior)
    }

    #[test]
    fn prefix_root_suffix_are_classified_by_position() {
        // shape: p0 p1 | r2 r3 r4 | s5 ; records ordered by `order`
        let c = counts(&[(0, 0, false, MorphStatus::Real), (1, 2, true, MorphStatus::Real), (2, 5, false, MorphStatus::Real)], 5);
        assert_eq!((c.prefixes, c.roots, c.suffixes, c.infixes, c.interfixes, c.nulls), (1, 1, 1, 0, 0, 0));
    }

    #[test]
    fn a_morph_between_two_roots_is_an_interfix_and_inside_a_root_is_an_infix() {
        // r0 r1 | n2 | r3 r4  -> interfix ; roots split by an infix appear as two root records tiling around it
        let c = counts(&[(0, 0, true, MorphStatus::Real), (1, 2, false, MorphStatus::Real), (2, 3, true, MorphStatus::Real)], 4);
        assert_eq!((c.roots, c.interfixes), (2, 1));
        // r0 | i1 | r2 (same root allomorph twice = one root split by an infix)
        let c = counts(&[(0, 0, true, MorphStatus::Real), (1, 1, false, MorphStatus::Real), (0, 2, true, MorphStatus::Real)], 2);
        assert_eq!((c.roots, c.infixes), (1, 1), "the two pieces of one allomorph are one root");
    }

    #[test]
    fn non_real_affix_records_count_as_nulls_only() {
        let c = counts(&[(0, 0, true, MorphStatus::Real), (1, u32::MAX, false, MorphStatus::Floating), (2, 0, false, MorphStatus::SubsumedFirst)], 2);
        assert_eq!(c.nulls, 2);
        assert_eq!(c.prefixes + c.suffixes + c.infixes + c.interfixes, 0);
    }

    #[test]
    fn violation_names_the_first_exceeded_cap() {
        let caps = pg_grammar::model::AnalysisCaps { max_nulls: 0, max_prefixes: 1, max_infixes: 0, max_suffixes: 5, max_interfixes: 0, max_roots: 1, max_analyses_to_return: None };
        let ok = MorphCounts { nulls: 0, prefixes: 1, infixes: 0, suffixes: 2, interfixes: 0, roots: 1 };
        assert_eq!(caps_violation(&caps, &ok), None);
        let two_prefixes = MorphCounts { prefixes: 2, ..ok };
        assert_eq!(caps_violation(&caps, &two_prefixes), Some(CapViolation::Prefixes { count: 2, max: 1 }));
        let one_null = MorphCounts { nulls: 1, ..ok };
        assert_eq!(caps_violation(&caps, &one_null), Some(CapViolation::Nulls { count: 1, max: 0 }));
    }
}
```

- [ ] **Step 2: Run** `& .\rust\tools\pg.ps1 -Mode quick -Package pg-rules` → compile error.

- [ ] **Step 3: Implement** (`caps.rs`):

```rust
//! XAmple's per-word analysis caps, counted positionally over a finished word (spec §5). A root is
//! a `Real` record whose allomorph is owned by a lexical entry; every other `Real` record is a
//! prefix, suffix, interfix, or infix by where its span sits relative to the root spans; a non-root
//! record that owns no output nodes is a null.

use pg_grammar::model::{AllomorphId, AllomorphOwner, AnalysisCaps, Grammar};

use crate::word::{MorphRecord, MorphStatus, Word};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MorphCounts {
    pub nulls: u32,
    pub prefixes: u32,
    pub infixes: u32,
    pub suffixes: u32,
    pub interfixes: u32,
    pub roots: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapViolation {
    Nulls { count: u32, max: u32 },
    Prefixes { count: u32, max: u32 },
    Infixes { count: u32, max: u32 },
    Suffixes { count: u32, max: u32 },
    Interfixes { count: u32, max: u32 },
    Roots { count: u32, max: u32 },
}

pub fn count_morphs(g: &Grammar, w: &Word) -> MorphCounts {
    let last_interior = w.shape.len().saturating_sub(1) as u32; // adapt: the same "last interior index" expression `validity.rs` uses
    count_morph_records(
        &w.morphs,
        |a| matches!(g.allomorph_owners.get(a.0 as usize), Some(AllomorphOwner::Root(..))),
        last_interior,
    )
}

/// `is_root` answers for an allomorph id; `last_interior` is the shape's last interior index.
pub fn count_morph_records(
    morphs: &[MorphRecord],
    is_root: impl Fn(AllomorphId) -> bool,
    last_interior: u32,
) -> MorphCounts {
    let mut counts = MorphCounts::default();
    let mut real: Vec<&MorphRecord> = morphs.iter().filter(|m| m.status == MorphStatus::Real).collect();
    real.sort_by_key(|m| m.order);
    // spans, tiled exactly as `validity.rs` derives them
    let spans: Vec<(u32, u32)> = real
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let end = real.get(i + 1).map(|n| n.order.saturating_sub(1)).unwrap_or(last_interior);
            (m.order, end)
        })
        .collect();
    let root_flags: Vec<bool> = real.iter().map(|m| is_root(m.allomorph)).collect();
    let root_spans: Vec<(u32, u32)> = spans.iter().zip(&root_flags).filter(|(_, r)| **r).map(|(s, _)| *s).collect();
    // Distinct root allomorph+runtime identity: a root split by an infix is one root.
    let mut seen_roots: Vec<AllomorphId> = Vec::new();
    for (m, r) in real.iter().zip(&root_flags) {
        if *r && !seen_roots.contains(&m.allomorph) {
            seen_roots.push(m.allomorph);
        }
    }
    counts.roots = seen_roots.len() as u32;
    let first_root_start = root_spans.iter().map(|s| s.0).min();
    let last_root_end = root_spans.iter().map(|s| s.1).max();
    for ((span, is_root), _) in spans.iter().zip(&root_flags).zip(real.iter()) {
        if *is_root {
            continue;
        }
        match (first_root_start, last_root_end) {
            (Some(fs), Some(le)) if span.1 < fs => counts.prefixes += 1,
            (Some(_), Some(le)) if span.0 > le => counts.suffixes += 1,
            (Some(_), Some(_)) => {
                // between roots, or inside one?
                let between = root_spans.windows(2).any(|p| span.0 > p[0].1 && span.1 < p[1].0);
                if between { counts.interfixes += 1 } else { counts.infixes += 1 }
            }
            // No root at all (a guessed or supplied root not marked Root): treat as suffix-less prefix run.
            _ => counts.prefixes += 1,
        }
    }
    counts.nulls = morphs
        .iter()
        .filter(|m| m.status != MorphStatus::Real && !is_root(m.allomorph))
        .count() as u32;
    counts
}

pub fn caps_violation(caps: &AnalysisCaps, c: &MorphCounts) -> Option<CapViolation> {
    if c.nulls > caps.max_nulls { return Some(CapViolation::Nulls { count: c.nulls, max: caps.max_nulls }); }
    if c.prefixes > caps.max_prefixes { return Some(CapViolation::Prefixes { count: c.prefixes, max: caps.max_prefixes }); }
    if c.infixes > caps.max_infixes { return Some(CapViolation::Infixes { count: c.infixes, max: caps.max_infixes }); }
    if c.suffixes > caps.max_suffixes { return Some(CapViolation::Suffixes { count: c.suffixes, max: caps.max_suffixes }); }
    if c.interfixes > caps.max_interfixes { return Some(CapViolation::Interfixes { count: c.interfixes, max: caps.max_interfixes }); }
    if c.roots > caps.max_roots { return Some(CapViolation::Roots { count: c.roots, max: caps.max_roots }); }
    None
}
```

Check `root_spans.windows(2)` ordering (root spans are in ascending order because `real` is sorted). Compounding non-heads: `Word.non_heads` children carry their own `morphs`; confirm with `validity.rs` whether the finished head word's `morphs` already includes non-head material (the C# port flattens at `attribute_morphs`); if not, fold `w.non_heads.iter().flat_map(|nh| &nh.morphs)` into the count and write a test with a compound.

- [ ] **Step 4: Run** `& .\rust\tools\pg.ps1 -Mode quick -Package pg-rules` → PASS.

- [ ] **Step 5: Commit** `git commit -am "rules: positional morph counting and XAmple cap violations"`

---

### Task 2: enforce in `is_word_valid_traced`, truncate in the result assembly

**Files:**
- Modify: `rust/crates/pg-rules/src/trace.rs` (`FailureReason`)
- Modify: `rust/crates/pg-parse/src/morpher.rs` (`is_word_valid_traced` ~line 773; result assembly ~lines 476-501; `ParseOutcome`)
- Create: `rust/crates/pg-parse/tests/xample_caps_gate.rs`

- [ ] **Step 1: Failing gate.** Build a synthetic snapshot in the test (copy the shape of `pg-grammar/src/compile/tests.rs::fixture()`; if that helper is not reachable from an integration test, make a `pub fn synthetic_prefixing_snapshot()` in a new `pg-grammar/src/compile/test_support.rs` behind `#[cfg(any(test, feature = "test-support"))]`, or simplest: build the snapshot inline). The grammar: stem `ta`; two prefix slots with prefixes `ki-` and `mu-` (both optional, in one template); one suffix `-ni`; one null suffix (empty form) `-∅` in a second optional slot. Words to parse: `kimutani` (2 prefixes, 1 suffix), `kita` (1 prefix), `ta` (0 affixes; with the null suffix it also has a 1-null analysis).

```rust
use pg_grammar::{compile_project_with, CompileOptions, ProfileChoice};
use pg_parse::Morpher;

fn compile(snapshot: &pg_snapshot::Snapshot, profile: ProfileChoice) -> pg_grammar::model::Grammar {
    compile_project_with(snapshot, &CompileOptions { profile, ..Default::default() }).unwrap().grammar
}

fn parses(g: &pg_grammar::model::Grammar, word: &str) -> Vec<String> {
    let m = Morpher::new(g, usize::MAX);
    let out = m.parse_word(word);
    let mut sigs: Vec<String> = out.analyses.iter().map(|(m, s)| format!("{m}|{s}")).collect();
    sigs.sort();
    sigs
}

#[test]
fn prefix_cap_admits_at_the_cap_and_refuses_above_it() {
    let mut snap = synthetic_prefixing_snapshot();
    snap.morphology.parser_parameters.xample.max_prefixes = Some(2);
    let g = compile(&snap, ProfileChoice::XAmple);
    assert!(!parses(&g, "kimutani").is_empty(), "two prefixes allowed at MaxPrefixes 2");
    snap.morphology.parser_parameters.xample.max_prefixes = Some(1);
    let g = compile(&snap, ProfileChoice::XAmple);
    assert!(parses(&g, "kimutani").is_empty(), "two prefixes refused at MaxPrefixes 1");
    assert!(!parses(&g, "kita").is_empty());
}

#[test]
fn null_cap_zero_removes_the_null_suffix_analysis() {
    let mut snap = synthetic_prefixing_snapshot();
    snap.morphology.parser_parameters.xample.max_nulls = Some(1);
    let g = compile(&snap, ProfileChoice::XAmple);
    let with_null = parses(&g, "ta");
    snap.morphology.parser_parameters.xample.max_nulls = Some(0);
    let g = compile(&snap, ProfileChoice::XAmple);
    let without = parses(&g, "ta");
    assert!(with_null.len() > without.len(), "{with_null:?} vs {without:?}");
    assert_eq!(without.len(), 1);
}

#[test]
fn hc_profile_ignores_the_block_entirely() {
    let mut snap = synthetic_prefixing_snapshot();
    snap.morphology.parser_parameters.xample.max_prefixes = Some(0);
    let g = compile(&snap, ProfileChoice::Hc);
    assert!(!parses(&g, "kimutani").is_empty());
}

#[test]
fn max_analyses_to_return_truncates_deterministically() {
    let mut snap = synthetic_prefixing_snapshot();
    snap.morphology.parser_parameters.xample.max_nulls = Some(1);
    snap.morphology.parser_parameters.xample.max_analyses_to_return = Some(1);
    let g = compile(&snap, ProfileChoice::XAmple);
    let m = Morpher::new(&g, usize::MAX);
    let out = m.parse_word("ta");
    assert_eq!(out.analyses.len(), 1);
    assert!(out.truncated_to_cap);
    // Deterministic: the survivor is the smallest signature.
    let all = parses(&compile(&{ let mut s = snap.clone(); s.morphology.parser_parameters.xample.max_analyses_to_return = Some(-1); s }, ProfileChoice::XAmple), "ta");
    assert_eq!(format!("{}|{}", out.analyses[0].0, out.analyses[0].1), all[0]);
}

#[test]
fn fst_confirm_path_honours_the_caps() {
    // pg-foma's confirm builds `Morpher::new(g, usize::MAX)`; caps must still bite because they live on the grammar.
    let mut snap = synthetic_prefixing_snapshot();
    snap.morphology.parser_parameters.xample.max_prefixes = Some(1);
    let g = compile(&snap, ProfileChoice::XAmple);
    let m = Morpher::new(&g, usize::MAX);
    // `parse_word_selected` with no filters is the confirm entry point's shape.
    let out = m.parse_word_selected("kimutani", None, None);
    assert!(out.analyses.is_empty());
}
```

(Adapt `parse_word_selected`'s exact signature from `morpher.rs:276`.)

- [ ] **Step 2: Run** `& .\rust\tools\pg.ps1 -Mode test -Package pg-parse -TestTarget xample_caps_gate` → FAIL (caps not enforced; `truncated_to_cap` missing).

- [ ] **Step 3: Implement.**

`trace.rs`: add `AnalysisCap(crate::caps::CapViolation),` to `FailureReason`; update any exhaustive `match` on it (grep `FailureReason::` in `pg-rules`, `pg-parse`, `pg-cli/src/trace_render.rs`) with a `Display`-style arm, e.g. `FailureReason::AnalysisCap(v) => write!(f, "XAmple analysis cap exceeded: {v:?}")`.

`morpher.rs`, in `is_word_valid_traced` before the final `allomorphs_valid_cached_traced` call:

```rust
        if let Some(caps) = &self.g.analysis_caps {
            let counts = pg_rules::caps::count_morphs(self.g, w);
            if let Some(v) = pg_rules::caps::caps_violation(caps, &counts) {
                if trace.is_tracing() {
                    trace.failed(parent, w, FailureReason::AnalysisCap(v));
                }
                return false;
            }
        }
```

Result assembly: replace `(matches.into_values().collect(), false)` with a sorted, optionally truncated list:

```rust
        } else {
            let mut ordered: Vec<Word> = matches.into_values().collect();
            (ordered, false)
        };
        let mut truncated_to_cap = false;
        if let Some(limit) = self.g.analysis_caps.and_then(|c| c.max_analyses_to_return) {
            // XAmple stops its search at the cap; which analyses survive there is search-order
            // dependent. Here the survivors are the smallest signatures, so the result is deterministic.
            let mut keyed: Vec<(String, Word)> = ordered_matches
                .into_iter()
                .map(|w| (format!("{}|{}", self.morpheme_join(&w), self.surface_of(&w)), w))
                .collect();
            keyed.sort_by(|a, b| a.0.cmp(&b.0));
            if keyed.len() > limit as usize {
                keyed.truncate(limit as usize);
                truncated_to_cap = true;
            }
            ordered_matches = keyed.into_iter().map(|(_, w)| w).collect();
        }
```

(`ordered_matches` must become `let mut`.) Add `pub truncated_to_cap: bool` to `ParseOutcome` and set it; every other `ParseOutcome { .. }` literal in the workspace gets `truncated_to_cap: false` (grep).

- [ ] **Step 4: Run** the gate → PASS; then `& .\rust\tools\pg.ps1 -Mode test -Package pg-parse` and `-Package pg-foma -TestTarget confirm_*` (or the whole pg-foma test set if time allows) → green. Grammars without caps take the `None` path and are byte-for-byte unchanged in behaviour.

- [ ] **Step 5: Commit** `git commit -am "parse: enforce XAmple analysis caps at the shared validity seam; deterministic MaxAnalysesToReturn"`

---

### Task 3: CLI `--parser-profile`, notices, banner

**Files:**
- Modify: `rust/crates/pg-cli/src/main.rs` (`load_grammar` ~line 288, `print_grammar_warnings` ~line 325, the flag parsers of `parse`/`batch`/`generate`/`fst-health` and the usage strings)

- [ ] **Step 1: Failing test.** `pg-cli` has CLI integration tests that run the binary (`run_batch_tsv` at ~line 965). Add:

```rust
#[test]
fn xample_profile_banner_is_printed_and_hc_flag_silences_it() {
    // Uses the pg-fwdata fixture: its ActiveParser is HC (Plan 1), so force xample to see the banner.
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../pg-fwdata/tests/data/fixture.fwdata");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args(["parse", fixture, "ta", "--parser-profile=xample"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("==== XAMPLE PROFILE:"), "{stderr}");
    assert!(stderr.contains("xample.caps-enforced"), "{stderr}");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args(["parse", fixture, "ta", "--parser-profile=hc"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("==== XAMPLE PROFILE:"), "{stderr}");
}
```

- [ ] **Step 2: Run** `& .\rust\tools\pg.ps1 -Mode test -Package pg-cli -TestTarget <the test target this landed in>` → FAIL (unknown flag).

- [ ] **Step 3: Implement.**

```rust
pub(crate) struct LoadedGrammar {
    pub grammar: Grammar,
    pub warnings: Vec<String>,
    pub notices: Vec<pg_grammar::Notice>,
}

pub(crate) fn parse_profile_flag(v: &str) -> Result<pg_grammar::ProfileChoice, String> {
    match v {
        "auto" => Ok(pg_grammar::ProfileChoice::Auto),
        "hc" => Ok(pg_grammar::ProfileChoice::Hc),
        "xample" => Ok(pg_grammar::ProfileChoice::XAmple),
        other => Err(format!("invalid --parser-profile: {other} (expected auto|hc|xample)")),
    }
}

pub(crate) fn load_grammar_with(path: &str, profile: pg_grammar::ProfileChoice) -> Result<LoadedGrammar, String> {
    let opts = pg_grammar::CompileOptions { profile, ..Default::default() };
    // .json / .fwdata / .fwbackup arms call `pg_grammar::compile_project_with(&snapshot, &opts)` and
    // return `LoadedGrammar { grammar: out.grammar, warnings, notices: out.notices }`;
    // the XML arm returns `notices: Vec::new()`.
    ...
}

/// Kept for callers that do not care about the profile; `Auto` is FieldWorks' own rule.
pub(crate) fn load_grammar(path: &str) -> Result<(Grammar, Vec<String>), String> {
    let l = load_grammar_with(path, pg_grammar::ProfileChoice::Auto)?;
    let mut warnings = l.warnings;
    warnings.extend(l.notices.iter().map(|n| n.message.clone()));
    Ok((l.grammar, warnings))
}

/// Loud notices first, under a banner nothing else prints; then ordinary warnings.
pub(crate) fn print_notices_and_warnings(l: &LoadedGrammar) {
    let loud: Vec<&pg_grammar::Notice> = l.notices.iter().filter(|n| n.severity == pg_grammar::Severity::Loud).collect();
    if !loud.is_empty() {
        eprintln!("==== XAMPLE PROFILE: {} notice(s) ====", loud.len());
        for n in &loud {
            eprintln!("!! {}", n.message);
        }
        eprintln!("==== end XAMPLE PROFILE ====");
    }
    for n in l.notices.iter().filter(|n| n.severity == pg_grammar::Severity::Info) {
        eprintln!("notice: {}", n.message);
    }
    print_grammar_warnings(&l.warnings);
}
```

In `run_parse`, `run_batch`, `run_generate`, and `fst-health` flag loops add `s if s.starts_with("--parser-profile=") => profile = parse_profile_flag(&s["--parser-profile=".len()..])?,` (default `Auto`), switch them to `load_grammar_with(path, profile)` + `print_notices_and_warnings(&loaded)`, and add `[--parser-profile auto|hc|xample]` to each usage string and the module doc.

- [ ] **Step 4: Run** the test → PASS. `& .\rust\tools\pg.ps1 -Mode test -Package pg-cli` → green (the `print_grammar_warnings` path for XML grammars is unchanged).

- [ ] **Step 5: Commit** `git commit -am "cli: --parser-profile and the XAMPLE PROFILE banner"`

---

### Task 4: the FST compile worker carries the profile

**Files:**
- Modify: `rust/crates/pg-foma/src/worker.rs:286` and the request struct it deserializes (grep `CompileWorkerRequest`).

- [ ] **Step 1:** Add `#[serde(default)] pub parser_profile: ParserProfileChoice` to the request (a small serde enum `auto|hc|xample` mirroring `pg_grammar::ProfileChoice`, with `From` both ways), thread it to `compile_project_with`, and emit the notices into whatever diagnostic channel the worker already uses for compile warnings (grep `_warnings` at that line — today they are dropped; forward `notices` messages the same way warnings should be, and if warnings are genuinely dropped there, forward both now).
- [ ] **Step 2:** The worker's existing round-trip test (grep `CompileWorkerRequest` in `pg-foma/tests`) gets one case with `"parserProfile":"hc"`; a request without the field deserializes to `Auto`.
- [ ] **Step 3:** `& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget <that test>` → PASS. Commit `git commit -am "worker: compile requests carry the parser profile"`.

---

### Task 5: census gate over real projects

**Files:**
- Create: `rust/crates/pg-cli/tests/xample_profile_census_gate.rs`

Mirror `fwdata_conformance_gate.rs::project_fwdata` for locating projects (`PANGLOSS_FW_PROJECTS_DIR`, self-skip with `eprintln!` when absent — that is the established pattern; keep the gate un-`#[ignore]`d so it runs and skips loudly).

- [ ] **Step 1: Write the gate:**

```rust
//! Records which parser profile each real FieldWorks project on this machine resolves to, and the
//! caps it would run under, so the spec's §7 table cannot drift silently. Self-skips per project.

use pg_grammar::{compile_project_with, CompileOptions, ParserProfile};

const PROJECTS: &[(&str, ParserProfile)] = &[
    ("sena", ParserProfile::XAmple),     // no ActiveParser element -> XAmple (liblcm default)
    ("amharic", ParserProfile::Hc),
    ("indonesian", ParserProfile::Hc),
    ("aweti", ParserProfile::Hc),
];

#[test]
fn real_projects_resolve_to_the_recorded_profiles() {
    let mut checked = 0;
    for (name, expected) in PROJECTS {
        let Some(path) = project_fwdata(name) else {
            eprintln!("skipping {name}: project not present");
            continue;
        };
        let (snapshot, _) = pg_fwdata::import_file(&path).unwrap();
        let out = compile_project_with(&snapshot, &CompileOptions::default()).unwrap();
        assert_eq!(out.profile, *expected, "{name}: profile changed — update the spec §7 table deliberately");
        if out.profile == ParserProfile::XAmple {
            let caps = out.grammar.analysis_caps.expect("XAmple profile carries caps");
            eprintln!("{name}: XAmple caps {caps:?}; {} loud notices", out.notices.len());
        }
        checked += 1;
    }
    eprintln!("census checked {checked} of {} projects", PROJECTS.len());
}
```

(`project_fwdata` copied from `fwdata_conformance_gate.rs:23-31`.) For Sena, also assert the caps equal `AnalysisCaps { max_nulls: 1, max_prefixes: 5, max_infixes: 1, max_suffixes: 5, max_interfixes: 0, max_roots: 1, max_analyses_to_return: Some(10) }` — the values read from `sena.fwdata` on 2026-09-03.

- [ ] **Step 2: Run** `& .\rust\tools\pg.ps1 -Mode test -Package pg-cli -TestTarget xample_profile_census_gate` → PASS when the projects are present (`samples/data/*.fwdata` in the main checkout; set `PANGLOSS_FW_PROJECTS_DIR` to that directory), skips otherwise.

- [ ] **Step 3: Commit** `git commit -am "gates: parser-profile census over the real projects"`

---

## Self-review notes

- Spec §5 counting and seam: Tasks 1–2. §5 truncation determinism: Task 2. §6 banner/codes: Task 3. §7 census: Task 5. §9 gate 4: Task 2's gate.
- Names: `pg_rules::caps::{MorphCounts, CapViolation, count_morphs, caps_violation}`, `FailureReason::AnalysisCap`, `ParseOutcome.truncated_to_cap`, `pg_cli::load_grammar_with`, `LoadedGrammar`, `print_notices_and_warnings`.
