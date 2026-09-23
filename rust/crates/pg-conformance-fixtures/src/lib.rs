//! Shared dual-root discovery + `words.yaml` parsing + oracle-replay helper for this repo's
//! conformance fixtures, per `docs/conformance-staging-plan.md`. One helper, used by every test
//! that walks `machine/conformance/**` and/or `conformance-staging/**`, so the path logic and the
//! `words.yaml` schema are defined exactly once rather than duplicated per test file (the plan
//! doc's explicit requirement).
//!
//! The two roots:
//! - `machine/conformance/**` — the `sillsdev/machine` submodule (`conformance-framework` branch),
//!   the eventual permanent home for every fixture.
//! - `conformance-staging/**` — this repo, committed, for fixtures that pin a bug/pathology
//!   immediately, ahead of upstream acceptance. See `docs/conformance-staging-plan.md`.
//!
//! Either root may be absent (a fresh clone with the `machine` submodule not initialized still
//! discovers staged fixtures fine); `discover` tolerates both independently.
//!
//! # A run must claim its scope
//! `discover` covers whichever roots the run CLAIMED, via `SCOPE_ENV`, and there is no default:
//! `local` is this repo's staged fixtures alone, `all` is those plus every upstream fixture. Those
//! are different claims, so an unclaimed run panics instead of picking one -- read `SCOPE_ENV`'s
//! own doc for why silently picking either is worse than refusing.

pub mod case_set;
pub mod corpus;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Which of the two fixture roots a fixture was found under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Root {
    /// `machine/conformance/**`.
    Machine,
    /// `conformance-staging/**`.
    Staging,
}

impl Root {
    pub fn label(self) -> &'static str {
        match self {
            Root::Machine => "machine",
            Root::Staging => "staging",
        }
    }
}

/// One discovered fixture directory (holds `grammar.xml` + `words.yaml`, and — staged fixtures
/// only — `STAGING.md`).
#[derive(Debug, Clone)]
pub struct FixtureRef {
    pub root: Root,
    /// `"edge-cases"` or `"languages"`.
    pub category: String,
    pub name: String,
    pub dir: PathBuf,
}

impl FixtureRef {
    /// The `(category, name)` identity the graduation guard keys on — a fixture accepted upstream
    /// under the same category+name as a staged copy is a collision regardless of root.
    pub fn key(&self) -> (String, String) {
        (self.category.clone(), self.name.clone())
    }

    pub fn grammar_path(&self) -> PathBuf {
        self.dir.join("grammar.xml")
    }

    pub fn words_yaml_path(&self) -> PathBuf {
        self.dir.join("words.yaml")
    }

    pub fn staging_md_path(&self) -> PathBuf {
        self.dir.join("STAGING.md")
    }

    /// A stable label for assertion messages: `"<root>:<category>/<name>"`.
    pub fn label(&self) -> String {
        format!("{}:{}/{}", self.root.label(), self.category, self.name)
    }

    pub fn load_grammar_xml(&self) -> String {
        std::fs::read_to_string(self.grammar_path())
            .unwrap_or_else(|e| panic!("{}: read grammar.xml: {e}", self.dir.display()))
    }

    pub fn load_words_yaml(&self) -> WordsYaml {
        let text = std::fs::read_to_string(self.words_yaml_path())
            .unwrap_or_else(|e| panic!("{}: read words.yaml: {e}", self.dir.display()));
        serde_yaml::from_str(&text)
            .unwrap_or_else(|e| panic!("{}: parse words.yaml: {e}", self.dir.display()))
    }

    /// This fixture's `# oracle-provenance:` marker, if `words.yaml` carries one. `None` for
    /// unreadable text or an absent/unrecognized marker — see [`OracleProvenance`] for why this
    /// lives in a YAML comment rather than a schema field.
    pub fn oracle_provenance(&self) -> Option<OracleProvenance> {
        let text = std::fs::read_to_string(self.words_yaml_path()).ok()?;
        parse_oracle_provenance_marker(&text)
    }
}

/// Which oracle a staged fixture's `words.yaml` signatures were checked against — the provenance
/// ratchet CLAUDE.md's "oracle hierarchy" section requires (`.claude/skills/conformance-grammars/
/// SKILL.md`'s "Oracle discipline" step).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OracleProvenance {
    /// Checked against `SIL.Machine.Morphology.HermitCrab`'s C# self-check (`hc-conformance.exe`,
    /// see `rust/tools/oracle-conformance.ps1`) or its `hc-dotnet-wrapper.sh` adapter, and matched.
    FoundingOracle,
    /// Legacy: authored/verified only against `pg_parse::Morpher` (HC-Rust). Records HC-Rust's own
    /// behavior, not correctness — never re-verified against the founding oracle.
    RustOnly,
    /// At least one word records the answer a hand-traced FORWARD synthesis proves, because the
    /// founding oracle is believed wrong there. Such a fixture is deliberately red against
    /// `hc.dll` until an upstream fix lands, so this value is a claim that the disagreement is
    /// known and argued, never that the oracle was not consulted — see
    /// `.claude/skills/conformance-grammars/SKILL.md`'s "Oracle discipline" step for the four
    /// obligations that come with it.
    ForwardSynthesis,
}

/// Parses a `# oracle-provenance: founding-oracle` / `# oracle-provenance: rust-only` marker line
/// out of raw `words.yaml` text. This is a plain YAML comment, not a schema field, because the C#
/// harness's `WordsYamlLoader` is a STRICT parser that hard-errors on any front-matter key outside
/// its fixed vocabulary (`language`/`inspired_by`/`sources`/`requires`/`budget_ms`/`expect_crash`/
/// `words`) — a new schema field would make every staged fixture unloadable by the founding oracle
/// itself, the one thing this marker exists to let a gate check against. Comments are invisible to
/// both that loader and this crate's own `serde_yaml` parsing, so this is the only channel available
/// that doesn't break either one.
pub fn parse_oracle_provenance_marker(words_yaml_text: &str) -> Option<OracleProvenance> {
    for line in words_yaml_text.lines() {
        let Some(rest) = line.trim_start().strip_prefix("# oracle-provenance:") else {
            continue;
        };
        return match rest.split_whitespace().next()? {
            "founding-oracle" => Some(OracleProvenance::FoundingOracle),
            "rust-only" => Some(OracleProvenance::RustOnly),
            "forward-synthesis" => Some(OracleProvenance::ForwardSynthesis),
            _ => None,
        };
    }
    None
}

/// Repo root, two levels up from this crate's manifest dir (`rust/crates/pg-conformance-fixtures`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

pub fn machine_conformance_root() -> PathBuf {
    repo_root().join("machine").join("conformance")
}

pub fn staging_root() -> PathBuf {
    repo_root().join("conformance-staging")
}

fn scan_one_root(root_dir: &Path, root: Root, out: &mut Vec<FixtureRef>) {
    if !root_dir.is_dir() {
        return;
    }
    for category in ["edge-cases", "languages"] {
        scan_one_category(root_dir, root, category, out);
    }
}

fn scan_one_category(root_dir: &Path, root: Root, category: &str, out: &mut Vec<FixtureRef>) {
    let cat_dir = root_dir.join(category);
    let Ok(read) = std::fs::read_dir(&cat_dir) else {
        return;
    };
    let mut entries: Vec<_> = read.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() && path.join("grammar.xml").is_file() && path.join("words.yaml").is_file()
        {
            out.push(FixtureRef {
                root,
                category: category.to_string(),
                name: entry.file_name().to_string_lossy().into_owned(),
                dir: path,
            });
        }
    }
}

/// `conformance-staging/filter-passes/**` — a THIRD staging-only category (candidate-filter-pass
/// fixtures) that neither `discover`/`discover_scoped` nor the upstream C# harness's
/// `Fixture.DiscoverAll` ever scans (both are fixed to `languages`/`edge-cases`). A caller needing
/// every staged fixture regardless of category must combine this with [`discover_scoped`]; see
/// [`all_staged_fixtures`].
pub fn discover_filter_passes() -> Vec<FixtureRef> {
    let mut out = Vec::new();
    scan_one_category(&staging_root(), Root::Staging, "filter-passes", &mut out);
    out
}

/// Every fixture under `conformance-staging/**`, across all three categories
/// (`edge-cases`/`languages`/`filter-passes`) — the complete staged set a provenance/ratchet gate
/// needs, which plain [`discover_scoped`] (`ConformanceScope::Local`) alone does not cover.
pub fn all_staged_fixtures() -> Vec<FixtureRef> {
    let mut fixtures = discover_scoped(ConformanceScope::Local);
    fixtures.extend(discover_filter_passes());
    fixtures
}

/// Which fixtures a conformance run covers. There is deliberately **no default**: see
/// [`SCOPE_ENV`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConformanceScope {
    /// `conformance-staging/**` only — this repo's own fixtures, nothing from upstream.
    Local,
    /// Both roots.
    All,
}

impl ConformanceScope {
    /// The wire name a caller claims, and what `SCOPE_ENV` is set to.
    pub fn label(self) -> &'static str {
        match self {
            ConformanceScope::Local => "local",
            ConformanceScope::All => "all",
        }
    }

    /// Parses a claimed scope. No catch-all arm, and no fallback for an unrecognized value.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim() {
            "local" => Ok(ConformanceScope::Local),
            "all" => Ok(ConformanceScope::All),
            other => Err(format!(
                "{SCOPE_ENV} is set to {other:?}, which is not a scope. Set it to \"local\" \
                 (conformance-staging only) or \"all\" (both roots)."
            )),
        }
    }
}

/// The variable a conformance run claims its scope in. **Unset is an error, never a default.**
///
/// A conformance result is only meaningful alongside what it covered: "green" over this repo's
/// staged fixtures alone and "green" over those plus every upstream fixture are different claims,
/// and silently picking either one lets a run report the stronger claim while having done the
/// weaker work. So there is no fallback — an unclaimed run refuses rather than guessing, the same
/// fail-closed rule the corpus gate applies to its own declared inputs.
///
/// `rust/tools/pg.ps1 -Mode conformance-test -Scope local|all` sets this; `-Mode test` and
/// `-Mode corpus-test` claim `all` explicitly and record it in the preflight, which is a claim made
/// by the mode rather than a default hidden in here.
pub const SCOPE_ENV: &str = "PANGLOSS_CONFORMANCE_SCOPE";

/// The scope this run claimed. Panics if unclaimed or unrecognized — a test binary reaching
/// fixture discovery without a scope has no correct set to return, and returning either one would
/// be the silent guess this exists to prevent.
pub fn claimed_scope() -> ConformanceScope {
    scope_from_env_value(std::env::var(SCOPE_ENV).ok().as_deref())
        .unwrap_or_else(|error| panic!("{error}"))
}

/// The claim decision, as a pure function of what the environment held: `None` is an absent
/// variable. Split out from [`claimed_scope`] so both refusals — unclaimed and unrecognized — are
/// testable without mutating a process-wide variable under a parallel test runner.
pub fn scope_from_env_value(value: Option<&str>) -> Result<ConformanceScope, String> {
    match value {
        Some(value) => ConformanceScope::parse(value),
        None => Err(format!(
            "{SCOPE_ENV} is not set, so this run has not said which fixtures it covers. Run it \
             through `rust/tools/pg.ps1 -Mode conformance-test -Scope local` (this repo's staged \
             fixtures only) or `-Scope all` (those plus every upstream fixture). There is no \
             default on purpose: a green run must say what it covered."
        )),
    }
}

/// Discover every fixture in scope, sorted deterministically within each root/category. The scope
/// is the one this run claimed ([`SCOPE_ENV`]); this function panics rather than guess when none
/// was claimed.
pub fn discover() -> Vec<FixtureRef> {
    discover_scoped(claimed_scope())
}

/// Discover every fixture under `scope`, sorted deterministically within each root/category. Takes
/// the scope explicitly, for callers that already know it and for testing the scoping itself.
pub fn discover_scoped(scope: ConformanceScope) -> Vec<FixtureRef> {
    let mut out = Vec::new();
    match scope {
        ConformanceScope::Local => {}
        ConformanceScope::All => {
            scan_one_root(&machine_conformance_root(), Root::Machine, &mut out)
        }
    }
    scan_one_root(&staging_root(), Root::Staging, &mut out);
    out
}

/// The fixture with this `(category, name)`, panicking — never skipping — when it is absent.
/// Why it fails rather than skips, and why it ignores the claimed scope: docs/design/fixture-pins.md
pub fn require_fixture(category: &str, name: &str) -> FixtureRef {
    let all = discover_scoped(ConformanceScope::All);
    if let Some(found) = all
        .iter()
        .find(|f| f.category == category && f.name == name)
    {
        return found.clone();
    }
    let mut available: Vec<String> = all.iter().map(|f| f.label()).collect();
    available.sort();
    panic!(
        "no conformance fixture `{category}/{name}` exists under either root.\n\n\
         This test names a specific fixture, so an absent one is a failure, not a reason to skip. \
         Either the fixture moved (the v1 -> v2 migration renamed many, e.g. \
         `loader/n1-isactive` -> `edge-cases/loader-isactive`), or it was never carried over and \
         this pin needs a replacement authored against the C# founding oracle.\n\n\
         {} fixture(s) discovered:\n  {}",
        all.len(),
        available.join("\n  ")
    );
}

/// The "graduation guard": fixture `(category, name)` identities that exist under BOTH roots.
/// Non-empty means a staged fixture has been accepted upstream (its name now also exists under
/// `machine/conformance/`) and the staged copy must be deleted in the same change — see
/// `docs/conformance-staging-plan.md`'s "graduation guard" mechanism.
pub fn graduation_guard_violations(fixtures: &[FixtureRef]) -> Vec<(String, String)> {
    use std::collections::HashSet;
    let machine_keys: HashSet<(String, String)> = fixtures
        .iter()
        .filter(|f| f.root == Root::Machine)
        .map(|f| f.key())
        .collect();
    let mut violations: Vec<(String, String)> = fixtures
        .iter()
        .filter(|f| f.root == Root::Staging && machine_keys.contains(&f.key()))
        .map(|f| f.key())
        .collect();
    violations.sort();
    violations.dedup();
    violations
}

/// The three populations a claimed `FieldworksProducibility` partitions fixtures into, each sorted
/// by label — a caller reporting "fixtures covered" must show these separately rather than folding
/// `EngineOnly`/`Unmarked` fixtures into a FieldWorks-facing coverage count.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ProducibilityCensus {
    pub producible: Vec<String>,
    pub engine_only: Vec<String>,
    pub unmarked: Vec<String>,
}

impl ProducibilityCensus {
    /// The one bucket-count rendering shared by every gate that prints this census.
    pub fn summary_line(&self) -> String {
        format!(
            "{} producible, {} engine-only, {} unmarked",
            self.producible.len(),
            self.engine_only.len(),
            self.unmarked.len()
        )
    }
}

/// Partitions already-[`discover`]ed fixtures by their `words.yaml` producibility verdict. Loads
/// each fixture's `words.yaml` itself (no separate walker) — see [`FieldworksProducibility`] for
/// what each bucket means.
pub fn producibility_census(fixtures: &[FixtureRef]) -> ProducibilityCensus {
    let mut census = ProducibilityCensus::default();
    for fixture in fixtures {
        let label = fixture.label();
        match fixture.load_words_yaml().fieldworks_producible {
            FieldworksProducibility::Producible => census.producible.push(label),
            FieldworksProducibility::EngineOnly { .. } => census.engine_only.push(label),
            FieldworksProducibility::Unmarked => census.unmarked.push(label),
        }
    }
    census.producible.sort();
    census.engine_only.sort();
    census.unmarked.sort();
    census
}

/// A fixture's `words.yaml` may declare whether it is reachable from a real FieldWorks project
/// (`fieldworks_producible`, `PROTOCOL.md` section 9) — separate from correctness: `false` names an
/// HC-engine-only regression fixture, never a claim that anything is wrong with it, and never
/// evidence of FieldWorks-facing conformance coverage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldworksProducibility {
    /// `fieldworks_producible: true` — every construct this fixture exercises has an `HCLoader`
    /// code path from a real FieldWorks project.
    Producible,
    /// `fieldworks_producible: false` — `notes` names the offending construct(s) with an
    /// `HCLoader` citation, required non-empty at parse time (mirrors Machine's own loader).
    EngineOnly { notes: String },
    /// The field is absent from this fixture's `words.yaml`. Never treated as [`Self::Producible`]
    /// — silence must not read as a claim, and this is the honest state for every fixture under a
    /// submodule pin that predates the field existing upstream.
    Unmarked,
}

// words.yaml schema: only the fields this repo's tests consume are modeled; no deny_unknown_fields, so this tolerates upstream schema additions without breaking.

#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "WordsYamlWire")]
pub struct WordsYaml {
    pub language: String,
    pub inspired_by: Vec<String>,
    pub sources: Vec<String>,
    pub requires: Vec<String>,
    /// Edge-cases-only: this fixture's founding-oracle run crashed — see `PROTOCOL.md`'s
    /// "expect_crash" section. A generic replay skips such fixtures (there is no signature to
    /// diff — the oracle died before producing one).
    pub expect_crash: bool,
    /// Edge-cases-only: C#'s own `isPathological` gate (`RunnerV2.RunOneSelfCheck`) — excluded
    /// from a default self-check run, exercised only with `--include-pathological`. A generic
    /// replay in this repo's default (<60s) suite skips these the same way.
    pub budget_ms: Option<u64>,
    pub fieldworks_producible: FieldworksProducibility,
    pub words: Vec<WordEntry>,
}

/// Raw front matter, resolved into [`FieldworksProducibility`] by `TryFrom` below.
#[derive(Debug, Clone, Deserialize)]
struct WordsYamlWire {
    language: String,
    #[serde(default)]
    inspired_by: Vec<String>,
    #[serde(default)]
    sources: Vec<String>,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default)]
    expect_crash: bool,
    #[serde(default)]
    budget_ms: Option<u64>,
    #[serde(default)]
    fieldworks_producible: Option<bool>,
    #[serde(default)]
    fieldworks_producible_notes: Option<String>,
    words: Vec<WordEntry>,
}

impl TryFrom<WordsYamlWire> for WordsYaml {
    type Error = String;

    fn try_from(wire: WordsYamlWire) -> Result<Self, Self::Error> {
        let fieldworks_producible = match wire.fieldworks_producible {
            None => FieldworksProducibility::Unmarked,
            Some(true) => FieldworksProducibility::Producible,
            Some(false) => {
                let notes = wire.fieldworks_producible_notes.unwrap_or_default();
                if notes.trim().is_empty() {
                    return Err("fieldworks_producible: false requires a non-empty \
                         fieldworks_producible_notes naming the offending construct(s)"
                        .to_string());
                }
                FieldworksProducibility::EngineOnly { notes }
            }
        };
        Ok(WordsYaml {
            language: wire.language,
            inspired_by: wire.inspired_by,
            sources: wire.sources,
            requires: wire.requires,
            expect_crash: wire.expect_crash,
            budget_ms: wire.budget_ms,
            fieldworks_producible,
            words: wire.words,
        })
    }
}

impl WordsYaml {
    /// Reason to skip this fixture entirely in a generic, always-on replay test — `None` means
    /// "replay it". See the field docs above for why `expect_crash`/`budget_ms` are excluded.
    pub fn skip_in_generic_replay(&self) -> Option<&'static str> {
        if self.expect_crash {
            return Some("expect_crash fixture (no signature ground truth to diff against)");
        }
        if self.budget_ms.is_some() {
            return Some(
                "budget_ms/pathological fixture (opt-in only, mirrors C#'s isPathological gate)",
            );
        }
        None
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WordEntry {
    pub word: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub provenance: Option<String>,
    /// Well-formed input, zero valid analyses (the oracle's `-` signature).
    #[serde(default)]
    pub expect_fail: bool,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    /// The oracle throws `InvalidShapeException` — batch status `SKIPPED`, not `ok`.
    #[serde(default)]
    pub expect_skip: bool,
    #[serde(default)]
    pub exercises: Vec<String>,
    #[serde(default)]
    pub parses: Vec<ParseEntry>,
}

impl WordEntry {
    /// `PROTOCOL.md` section 3's adapter-mode omission rule: a word carrying any `guess: true`
    /// parse is invisible to adapter-mode (non-self-check) engines — `pangloss`'s plain
    /// `Morpher::parse_word` (`guess_root: false`) included. A generic replay skips such words
    /// entirely rather than asserting on them.
    pub fn adapter_visible(&self) -> bool {
        !self.parses.iter().any(|p| p.guess)
    }

    /// The `PROTOCOL.md` section 2/3 expected `signature` column for this word: `-` if the word
    /// has no (adapter-visible) parses, else the ordinally-sorted, `;`-joined set of declared
    /// `parses[].signature` values — the same algorithm `pg_parse::ParseOutcome::signature`
    /// implements for the engine's actual output, so the two are directly comparable.
    pub fn expected_signature(&self) -> String {
        if self.parses.is_empty() {
            return "-".to_string();
        }
        self.expected_multiset().join(";")
    }

    /// The sorted, non-deduped multiset `expected_signature` joins — declared parses are never
    /// deduped (two `parses` entries with the identical `signature` string, e.g.
    /// `edge-cases/mpr-gated-exception`'s `mentanukam`, mean the oracle really produced two
    /// analyses, per `PROTOCOL.md` section 4 rule 3), so exposing this separately lets a caller
    /// compare COUNTS, not just the joined string.
    pub fn expected_multiset(&self) -> Vec<String> {
        let mut sigs: Vec<String> = self.parses.iter().map(|p| p.signature.clone()).collect();
        sigs.sort();
        sigs
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParseEntry {
    pub signature: String,
    #[serde(default)]
    pub gloss: Option<String>,
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub exercises: Vec<String>,
    /// A `Guesser`/`LexicalGuess` analysis (`PROTOCOL.md` section 3) — self-check-only, never
    /// producible through the adapter contract.
    #[serde(default)]
    pub guess: bool,
}

// Oracle replay: drives pg_parse::Morpher (this repo's own full engine) as the adapter — pangloss IS the oracle for anything authored/verified against it, rather than the C# founding oracle.

/// Replay every adapter-visible word in `words_yaml` against `morpher`, asserting the
/// `PROTOCOL.md` status+signature match (`expect_skip` -> `invalid_shape`, else the sorted-joined
/// signature). Panics (via `assert_eq!`/`assert!`) on the first mismatch, naming `fixture_label`.
/// Returns the number of words actually checked, so callers can additionally assert that count
/// against an expected total (catches a fixture silently shrinking to zero checked words).
pub fn assert_matches_oracle(
    fixture_label: &str,
    words_yaml: &WordsYaml,
    morpher: &pg_parse::Morpher,
) -> usize {
    let replay = replay_against_oracle(words_yaml, morpher);
    if let Some(m) = replay.mismatches.first() {
        panic!(
            "{fixture_label}: word {:?} {}\n  left (HC-Rust): {:?}\n right (oracle):  {:?}",
            m.word, m.what, m.got, m.expected
        );
    }
    replay.checked
}

/// One word where HC-Rust and the committed oracle expectation disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleMismatch {
    pub word: String,
    pub what: &'static str,
    pub got: String,
    pub expected: String,
}

/// The outcome of replaying every adapter-visible word: how many were compared, and every disagreement.
#[derive(Debug, Default)]
pub struct OracleReplay {
    pub checked: usize,
    pub mismatches: Vec<OracleMismatch>,
}

/// The non-panicking core of `assert_matches_oracle`: replays every adapter-visible word and collects
/// each disagreement instead of stopping at the first, so a gate can compare the whole set against a
/// declared known-divergence list and fail on a divergence that is missing as well as one that is new.
///
/// The signature check compares the produced and declared analyses as **multisets**, not as the
/// joined signature string: two `parses` entries (or two produced analyses) with the identical
/// rendered signature are two distinct analyses, not one repeated twice —
/// `machine/conformance/PROTOCOL.md` section 4 rule 3 ("a genuine difference ... in how many times
/// a given entry appears IS a failure"), witnessed by `edge-cases/mpr-gated-exception`'s
/// `mentanukam` and `edge-cases/head-ambiguous-compounding`'s `dakimo`. `pg_parse::result_multiset`
/// and [`WordEntry::expected_multiset`] are the same sort-without-dedup each side's `signature()`
/// already joins, called directly here rather than reparsed out of the joined string, so this
/// cannot drift from what `signature()`/`expected_signature()` report.
pub fn replay_against_oracle(words_yaml: &WordsYaml, morpher: &pg_parse::Morpher) -> OracleReplay {
    let mut replay = OracleReplay::default();
    for w in &words_yaml.words {
        if !w.adapter_visible() {
            continue; // self-check-only (guess:true parse), PROTOCOL.md section 3
        }
        let outcome = morpher.parse_word(&w.word);
        replay.checked += 1;
        if w.expect_skip {
            if !outcome.invalid_shape {
                replay.mismatches.push(OracleMismatch {
                    word: w.word.clone(),
                    what: "expected SKIPPED (invalid shape) but engine produced a result",
                    got: outcome.signature(),
                    expected: "<skipped>".to_string(),
                });
            }
            continue;
        }
        if outcome.invalid_shape {
            replay.mismatches.push(OracleMismatch {
                word: w.word.clone(),
                what: "unexpectedly SKIPPED (invalid shape)",
                got: "<skipped>".to_string(),
                expected: w.expected_signature(),
            });
            continue;
        }
        // See this function's doc comment: multiset, not joined-string, comparison.
        let got_multiset = pg_parse::result_multiset(&outcome.analyses);
        let expected_multiset = w.expected_multiset();
        if got_multiset != expected_multiset {
            replay.mismatches.push(OracleMismatch {
                word: w.word.clone(),
                what: "analysis multiset mismatch vs oracle (HC-Rust must reproduce the oracle's \
                       exact derivation count per entry, not only its set of distinct analyses)",
                got: format!("{got_multiset:?}"),
                expected: format!("{expected_multiset:?}"),
            });
        }
    }
    replay
}

#[cfg(test)]
mod tests;
