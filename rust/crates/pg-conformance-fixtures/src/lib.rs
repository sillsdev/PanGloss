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

    /// `grammar.xml` loaded with the fixture's `morpher:` block applied, so a `pg_parse::Morpher`
    /// built from it runs under the settings the fixture's expectations were observed under.
    /// Loading the XML with `pg_grammar::load` directly runs every fixture at the defaults;
    /// [`replay_against_oracle`] reports that as a mismatch for a fixture with a block.
    pub fn load_grammar(&self) -> pg_grammar_model::model::Grammar {
        self.load_grammar_with_settings(&self.load_words_yaml().morpher)
    }

    /// Load the source grammar and apply exactly one run's settings. Callers use this separately
    /// for each variant so variants always start at engine defaults and never inherit base values.
    pub fn load_grammar_with_settings(
        &self,
        settings: &MorpherSettings,
    ) -> pg_grammar_model::model::Grammar {
        let mut grammar = pg_grammar::load(&self.load_grammar_xml())
            .unwrap_or_else(|e| panic!("{}: grammar failed to load: {e}", self.label()));
        settings.apply_to(&mut grammar);
        grammar
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

// Validate nested payloads; tolerate unknown top-level metadata.

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
    /// The `morpher:` block (`PROTOCOL.md` section 8); empty when absent, meaning the defaults.
    pub morpher: MorpherSettings,
    pub words: Vec<WordEntry>,
    /// Optional independent settings runs. Each variant carries a complete expected word list.
    pub variants: Vec<SettingsVariant>,
}

/// The `requires` capability a fixture with base or variant `morpher` settings must declare
/// (`PROTOCOL.md` section 5).
pub const MORPHER_SETTINGS_CAPABILITY: &str = "morpher-settings";

/// A fixture's `morpher:` block: `Morpher` host settings its expectations were observed under,
/// keyed by the FieldWorks `<ParserParameters><HC>` element names (`PROTOCOL.md` section 8). `None`
/// means the C# constructor default. Parsing refuses any other key, an empty YAML block, an
/// out-of-range count and a non-boolean flag, because a setting this harness cannot apply must
/// not fall back to the default it was meant to override. The adapter JSON file allows `{}` to
/// represent a materialized default run. `MaxAlternatives` and `MergeMSAs` are deliberately
/// unsupported; neither is mapped to an HC alternative cap or another setting.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MorpherSettings {
    /// `DelReapps` -> `Grammar::deletion_reapplications`.
    pub del_reapps: Option<u32>,
    /// `MaxRoots` (at least 1) -> `Grammar::max_stem_count`.
    pub max_roots: Option<u32>,
    /// `MergeAnalyses` -> `Grammar::merge_equivalent_analyses`.
    pub merge_analyses: Option<bool>,
    /// `MakePartialsFinal` -> `Grammar::always_enforce_final_templates`.
    pub make_partials_final: Option<bool>,
    /// `GuessRoots` -> per-call `ParseOptions::guess_root`; omission keeps adapter-mode guessing off.
    pub guess_roots: Option<bool>,
}

impl MorpherSettings {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Writes every declared value onto `grammar`, where `pg_parse::Morpher::new` reads it.
    pub fn apply_to(&self, grammar: &mut pg_grammar_model::model::Grammar) {
        if let Some(value) = self.del_reapps {
            grammar.deletion_reapplications = value;
        }
        if let Some(value) = self.max_roots {
            grammar.max_stem_count = value;
        }
        if let Some(value) = self.merge_analyses {
            grammar.merge_equivalent_analyses = value;
        }
        if let Some(value) = self.make_partials_final {
            grammar.always_enforce_final_templates = value;
        }
        if let Some(value) = self.guess_roots {
            grammar.guess_roots = value;
        }
    }

    /// Each declared key whose value `morpher` does not run with, as `key: declared X, morpher Y`.
    pub fn unapplied(&self, morpher: &pg_parse::Morpher) -> Vec<String> {
        let mut unapplied = Vec::new();
        let mut check = |key: &str, declared: Option<String>, actual: String| {
            if let Some(declared) = declared {
                if declared != actual {
                    unapplied.push(format!("{key}: declared {declared}, morpher {actual}"));
                }
            }
        };
        check(
            "DelReapps",
            self.del_reapps.map(|v| v.to_string()),
            morpher.deletion_reapplications().to_string(),
        );
        check(
            "MaxRoots",
            self.max_roots.map(|v| v.to_string()),
            morpher.max_stem_count().to_string(),
        );
        check(
            "MergeAnalyses",
            self.merge_analyses.map(|v| v.to_string()),
            morpher.merge_equivalent_analyses().to_string(),
        );
        check(
            "MakePartialsFinal",
            self.make_partials_final.map(|v| v.to_string()),
            morpher.always_enforce_final_templates().to_string(),
        );
        check(
            "GuessRoots",
            self.guess_roots.map(|v| v.to_string()),
            morpher.guess_roots().to_string(),
        );
        unapplied
    }

    /// Parse a JSON object of FieldWorks settings for the standalone adapter. It uses the same
    /// strict key/type/range checks as `words.yaml`; `MergeMSAs` and `MaxAlternatives` remain
    /// deliberately unsupported because this engine has no faithful mapping for them.
    pub fn from_json_str(text: &str) -> Result<Self, String> {
        let json: JsonSettingsMap =
            serde_json::from_str(text).map_err(|e| format!("morpher JSON: {e}"))?;
        let yaml = serde_yaml::to_value(serde_json::Value::Object(json.0))
            .map_err(|e| format!("morpher JSON: {e}"))?;
        Self::from_yaml_value(&yaml, true)
    }

    fn from_yaml(value: &serde_yaml::Value) -> Result<Self, String> {
        Self::from_yaml_value(value, false)
    }

    fn from_yaml_value(value: &serde_yaml::Value, allow_empty: bool) -> Result<Self, String> {
        let serde_yaml::Value::Mapping(map) = value else {
            return Err("morpher: expected a mapping".to_string());
        };
        if map.is_empty() && !allow_empty {
            return Err("morpher: an empty block means the defaults; omit it instead".to_string());
        }
        let mut settings = Self::default();
        for (key, value) in map {
            let Some(key) = key.as_str() else {
                return Err(format!("morpher: keys must be strings, got {key:?}"));
            };
            let count = |minimum: u64| {
                value
                    .as_u64()
                    .filter(|v| *v >= minimum && *v <= i32::MAX as u64)
                    .map(|v| v as u32)
                    .ok_or_else(|| {
                        format!("morpher.{key}: expected a whole number from {minimum} through {}, got {value:?}", i32::MAX)
                    })
            };
            let flag = || {
                value
                    .as_bool()
                    .ok_or_else(|| format!("morpher.{key}: expected true or false, got {value:?}"))
            };
            match key {
                "DelReapps" => settings.del_reapps = Some(count(0)?),
                "MaxRoots" => settings.max_roots = Some(count(1)?),
                "MergeAnalyses" => settings.merge_analyses = Some(flag()?),
                "MakePartialsFinal" => settings.make_partials_final = Some(flag()?),
                "GuessRoots" => settings.guess_roots = Some(flag()?),
                other => {
                    return Err(format!(
                        "morpher: unknown key '{other}' (allowed: DelReapps, MakePartialsFinal, \
                         MaxRoots, MergeAnalyses, GuessRoots; MaxAlternatives and MergeMSAs are \
                         unsupported)"
                    ))
                }
            }
        }
        Ok(settings)
    }
}

/// Duplicate keys make a settings object ambiguous, so preserve them while decoding.
struct JsonSettingsMap(serde_json::Map<String, serde_json::Value>);

impl<'de> Deserialize<'de> for JsonSettingsMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SettingsVisitor;

        impl<'de> serde::de::Visitor<'de> for SettingsVisitor {
            type Value = JsonSettingsMap;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON object containing morpher settings")
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut object = serde_json::Map::new();
                while let Some((key, value)) = access.next_entry::<String, serde_json::Value>()? {
                    if object.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate setting key {key:?}"
                        )));
                    }
                    object.insert(key, value);
                }
                Ok(JsonSettingsMap(object))
            }
        }

        deserializer.deserialize_map(SettingsVisitor)
    }
}

/// A complete alternate parser run. Settings start from engine defaults; words repeat the base
/// fixture's exact ordered inputs and replace its expected outputs for this run.
#[derive(Debug, Clone)]
pub struct SettingsVariant {
    pub name: String,
    pub morpher: MorpherSettings,
    pub words: Vec<WordEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsVariantWire {
    name: String,
    morpher: Option<serde_yaml::Value>,
    words: Vec<WordEntryWire>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct WordEntryWire {
    word: String,
    #[serde(default, deserialize_with = "deserialize_present")]
    note: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    provenance: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    expect_fail: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_present")]
    blocked_by: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    neutralizes: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    expect_skip: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_present")]
    exercises: Option<Vec<String>>,
    #[serde(default)]
    claimed_cells: Vec<ClaimedCellWire>,
    #[serde(default)]
    parses: Vec<ParseEntryWire>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParseEntryWire {
    signature: String,
    #[serde(default, deserialize_with = "deserialize_present")]
    gloss: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    rules: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    exercises: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    guess: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
struct ClaimedCellWire {
    cell: String,
    #[serde(default, deserialize_with = "deserialize_present")]
    severing: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    before: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    after: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    distinct_from: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    proof: Option<String>,
}

fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

impl WordEntryWire {
    fn validate(&self, path: &str, variant: bool, require_metadata: bool) -> Result<(), String> {
        if self.word.is_empty() {
            return Err(format!("{path}.word must be nonempty"));
        }
        if self.note.as_ref().is_some_and(String::is_empty) {
            return Err(format!("{path}.note must be nonempty"));
        }
        if require_metadata && self.note.is_none() {
            return Err(format!("{path}: missing field `note`"));
        }
        validate_unique_strings(&format!("{path}.blocked_by"), self.blocked_by.as_deref())?;
        validate_unique_strings(&format!("{path}.neutralizes"), self.neutralizes.as_deref())?;
        validate_unique_strings(&format!("{path}.exercises"), self.exercises.as_deref())?;

        let expect_fail = self.expect_fail.unwrap_or(false);
        let expect_skip = self.expect_skip.unwrap_or(false);
        if expect_fail && expect_skip {
            return Err(format!(
                "{path} may not set both expect_fail and expect_skip"
            ));
        }
        if (expect_fail || expect_skip) && !self.parses.is_empty() {
            return Err(format!(
                "{path}.parses must be empty when expect_fail or expect_skip is true"
            ));
        }
        if self.blocked_by.is_some() && !expect_fail {
            return Err(format!("{path}.blocked_by requires expect_fail: true"));
        }
        if !expect_fail && !expect_skip && self.parses.is_empty() {
            return Err(format!(
                "{path} requires at least one parse unless expect_fail or expect_skip is true"
            ));
        }

        for (index, parse) in self.parses.iter().enumerate() {
            parse.validate(
                &format!("{path}.parses[{index}]"),
                variant,
                require_metadata,
            )?;
        }

        let mut unique_claims = std::collections::HashSet::new();
        for (index, claim) in self.claimed_cells.iter().enumerate() {
            claim.validate(&format!("{path}.claimed_cells[{index}]"))?;
            if !unique_claims.insert(claim) {
                return Err(format!("{path}.claimed_cells contains a duplicate entry"));
            }
        }

        if variant
            && (self
                .blocked_by
                .as_ref()
                .is_some_and(|values| !values.is_empty())
                || self
                    .neutralizes
                    .as_ref()
                    .is_some_and(|values| !values.is_empty())
                || self
                    .exercises
                    .as_ref()
                    .is_some_and(|values| !values.is_empty())
                || !self.claimed_cells.is_empty())
        {
            return Err(format!(
                "{path} may not declare blocked_by, neutralizes, exercises, or claimed_cells; variant coverage belongs to the base run"
            ));
        }
        Ok(())
    }

    fn into_entry(self) -> WordEntry {
        WordEntry {
            word: self.word,
            note: self.note,
            provenance: self.provenance,
            expect_fail: self.expect_fail.unwrap_or(false),
            blocked_by: self.blocked_by.unwrap_or_default(),
            neutralizes: self.neutralizes.unwrap_or_default(),
            expect_skip: self.expect_skip.unwrap_or(false),
            exercises: self.exercises.unwrap_or_default(),
            claimed_cells: self
                .claimed_cells
                .into_iter()
                .map(ClaimedCellWire::into_value)
                .collect(),
            parses: self
                .parses
                .into_iter()
                .map(ParseEntryWire::into_entry)
                .collect(),
        }
    }
}

impl ParseEntryWire {
    fn validate(&self, path: &str, variant: bool, require_metadata: bool) -> Result<(), String> {
        if self.signature.is_empty() {
            return Err(format!("{path}.signature must be nonempty"));
        }
        if require_metadata && self.rules.is_none() {
            return Err(format!("{path}: missing field `rules`"));
        }
        if let Some(rules) = &self.rules {
            validate_nonempty_strings(&format!("{path}.rules"), rules)?;
        }
        validate_unique_strings(&format!("{path}.exercises"), self.exercises.as_deref())?;
        if variant
            && self
                .exercises
                .as_ref()
                .is_some_and(|values| !values.is_empty())
        {
            return Err(format!(
                "{path}.exercises must be empty in a settings variant"
            ));
        }
        Ok(())
    }

    fn into_entry(self) -> ParseEntry {
        ParseEntry {
            signature: self.signature,
            gloss: self.gloss,
            rules: self.rules.unwrap_or_default(),
            exercises: self.exercises.unwrap_or_default(),
            guess: self.guess.unwrap_or(false),
        }
    }
}

impl ClaimedCellWire {
    fn validate(&self, path: &str) -> Result<(), String> {
        if self.cell.is_empty() {
            return Err(format!("{path}.cell must be nonempty"));
        }
        for (name, value) in [
            ("severing", self.severing.as_deref()),
            ("before", self.before.as_deref()),
            ("after", self.after.as_deref()),
            ("distinct_from", self.distinct_from.as_deref()),
            ("proof", self.proof.as_deref()),
        ] {
            if value.is_some_and(str::is_empty) {
                return Err(format!("{path}.{name} must be nonempty"));
            }
        }
        let review_bundle = [
            self.severing.is_some(),
            self.before.is_some(),
            self.after.is_some(),
            self.proof.is_some(),
        ];
        if review_bundle.iter().any(|present| *present)
            && review_bundle.iter().any(|present| !present)
        {
            return Err(format!(
                "{path} requires severing, before, after, and proof together"
            ));
        }
        Ok(())
    }

    fn into_value(self) -> serde_yaml::Value {
        let mut map = serde_yaml::Mapping::new();
        let mut insert = |key: &str, value: &str| {
            map.insert(
                serde_yaml::Value::String(key.to_string()),
                serde_yaml::Value::String(value.to_string()),
            );
        };
        insert("cell", &self.cell);
        for (key, value) in [
            ("severing", self.severing),
            ("before", self.before),
            ("after", self.after),
            ("distinct_from", self.distinct_from),
            ("proof", self.proof),
        ] {
            if let Some(value) = value {
                insert(key, &value);
            }
        }
        serde_yaml::Value::Mapping(map)
    }
}

fn validate_nonempty_strings(path: &str, values: &[String]) -> Result<(), String> {
    if let Some(index) = values.iter().position(String::is_empty) {
        return Err(format!("{path}[{index}] must be nonempty"));
    }
    Ok(())
}

fn validate_unique_strings(path: &str, values: Option<&[String]>) -> Result<(), String> {
    if let Some(values) = values {
        validate_nonempty_strings(path, values)?;
        let mut unique = std::collections::HashSet::new();
        if values.iter().any(|value| !unique.insert(value)) {
            return Err(format!("{path} contains a duplicate entry"));
        }
    }
    Ok(())
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
    #[serde(default, deserialize_with = "deserialize_present")]
    budget_ms: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_present")]
    fieldworks_producible: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_present")]
    fieldworks_producible_notes: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    morpher: Option<serde_yaml::Value>,
    words: Vec<WordEntryWire>,
    #[serde(default, deserialize_with = "deserialize_present")]
    variants: Option<Vec<SettingsVariantWire>>,
}

impl TryFrom<WordsYamlWire> for WordsYaml {
    type Error = String;

    fn try_from(wire: WordsYamlWire) -> Result<Self, Self::Error> {
        if wire.language.is_empty() {
            return Err("language must be nonempty".to_string());
        }
        validate_nonempty_strings("inspired_by", &wire.inspired_by)?;
        validate_nonempty_strings("sources", &wire.sources)?;
        validate_unique_strings("requires", Some(&wire.requires))?;
        if wire.words.is_empty() {
            return Err("words must contain at least one entry".to_string());
        }
        for (index, word) in wire.words.iter().enumerate() {
            word.validate(&format!("words[{index}]"), false, wire.variants.is_some())?;
        }
        if wire.budget_ms == Some(0) {
            return Err("budget_ms must be at least 1".to_string());
        }
        if wire
            .fieldworks_producible_notes
            .as_ref()
            .is_some_and(String::is_empty)
        {
            return Err("fieldworks_producible_notes must be nonempty when present".to_string());
        }
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
        let morpher = match &wire.morpher {
            None => MorpherSettings::default(),
            Some(value) => MorpherSettings::from_yaml(value)?,
        };
        let base_inputs: Vec<&str> = wire.words.iter().map(|word| word.word.as_str()).collect();
        let mut names = std::collections::HashSet::new();
        let (variants_present, variant_wires) = match wire.variants {
            None => (false, Vec::new()),
            Some(variants) if variants.is_empty() => {
                return Err("variants must contain at least one entry when present".to_string())
            }
            Some(variants) => (true, variants),
        };
        let mut variants = Vec::with_capacity(variant_wires.len());
        for variant in variant_wires {
            validate_variant_name(&variant.name)?;
            if variant.name == "default" {
                return Err("variants: name 'default' is reserved for the base run".to_string());
            }
            if !names.insert(variant.name.clone()) {
                return Err(format!("variants: duplicate name {:?}", variant.name));
            }
            let variant_morpher = variant.morpher.as_ref().ok_or_else(|| {
                format!(
                    "variants.{}.morpher: a nonempty settings mapping is required",
                    variant.name
                )
            })?;
            let variant_morpher = MorpherSettings::from_yaml(variant_morpher)
                .map_err(|error| format!("variants.{}.{}", variant.name, error))?;
            if variant.words.is_empty() {
                return Err(format!(
                    "variants.{}.words must contain at least one entry",
                    variant.name
                ));
            }
            for (index, word) in variant.words.iter().enumerate() {
                word.validate(
                    &format!("variants.{}.words[{index}]", variant.name),
                    true,
                    true,
                )?;
            }
            let variant_inputs: Vec<&str> = variant
                .words
                .iter()
                .map(|word| word.word.as_str())
                .collect();
            if variant_inputs != base_inputs {
                return Err(format!(
                    "variants.{}.words must contain the exact same ordered word inputs as base words",
                    variant.name
                ));
            }
            variants.push(SettingsVariant {
                name: variant.name,
                morpher: variant_morpher,
                words: variant
                    .words
                    .into_iter()
                    .map(WordEntryWire::into_entry)
                    .collect(),
            });
        }
        let requires_settings = wire
            .requires
            .iter()
            .any(|r| r == MORPHER_SETTINGS_CAPABILITY);
        let settings_present = !morpher.is_empty() || variants_present;
        if requires_settings != settings_present {
            return Err(format!(
                "requires lists {MORPHER_SETTINGS_CAPABILITY} exactly when base or variant \
                 morpher settings are present (requires {:?}, settings {})",
                wire.requires,
                if settings_present {
                    "present"
                } else {
                    "absent"
                }
            ));
        }
        Ok(WordsYaml {
            language: wire.language,
            inspired_by: wire.inspired_by,
            sources: wire.sources,
            requires: wire.requires,
            expect_crash: wire.expect_crash,
            budget_ms: wire.budget_ms,
            fieldworks_producible,
            morpher,
            words: wire
                .words
                .into_iter()
                .map(WordEntryWire::into_entry)
                .collect(),
            variants,
        })
    }
}

fn validate_variant_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(format!(
            "variants: invalid name {name:?}; expected nonempty lowercase [a-z0-9-]+"
        ));
    }
    Ok(())
}

impl WordsYaml {
    /// Copy the fixture metadata into an independent run using one variant's settings and answers.
    pub fn for_variant(&self, variant: &SettingsVariant) -> Self {
        Self {
            language: self.language.clone(),
            inspired_by: self.inspired_by.clone(),
            sources: self.sources.clone(),
            requires: self.requires.clone(),
            expect_crash: self.expect_crash,
            budget_ms: self.budget_ms,
            fieldworks_producible: self.fieldworks_producible.clone(),
            morpher: variant.morpher.clone(),
            words: variant.words.clone(),
            variants: Vec::new(),
        }
    }

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
    #[serde(default)]
    pub neutralizes: Vec<String>,
    /// The oracle throws `InvalidShapeException` — batch status `SKIPPED`, not `ok`.
    #[serde(default)]
    pub expect_skip: bool,
    #[serde(default)]
    pub exercises: Vec<String>,
    #[serde(default)]
    pub claimed_cells: Vec<serde_yaml::Value>,
    #[serde(default)]
    pub parses: Vec<ParseEntry>,
}

impl WordEntry {
    /// `PROTOCOL.md` section 3's legacy adapter-mode omission rule. A generic run without an
    /// explicit `GuessRoots` value skips words carrying any `guess: true` parse.
    pub fn adapter_visible(&self) -> bool {
        self.adapter_visible_for_guess_roots(false)
    }

    /// Variant-aware visibility: an explicitly supplied GuessRoots setting makes guessed words
    /// part of that run's complete expected word list, whether the value is true or false.
    pub fn adapter_visible_for_guess_roots(&self, guess_roots_explicit: bool) -> bool {
        guess_roots_explicit || !self.parses.iter().any(|p| p.guess)
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
    /// A `Guesser`/`LexicalGuess` analysis (`PROTOCOL.md` section 3) — omitted by legacy adapter
    /// runs, but included when that run explicitly supplies `GuessRoots`.
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

/// One independently evaluated run for a fixture. `is_base` marks the only run included in the
/// fixture coverage total; named settings variants are replayed without changing fixture coverage.
#[derive(Debug)]
pub struct NamedOracleReplay {
    pub name: String,
    pub is_base: bool,
    pub replay: OracleReplay,
}

/// Replay the base run and every declared settings variant. Each grammar is freshly loaded from
/// `grammar.xml`; the variant does not inherit the base run's `morpher:` settings.
pub fn replay_fixture_runs(fixture: &FixtureRef, words_yaml: &WordsYaml) -> Vec<NamedOracleReplay> {
    let mut runs = Vec::with_capacity(1 + words_yaml.variants.len());
    let run = |name: String, words: &WordsYaml, is_base: bool| {
        let grammar = fixture.load_grammar_with_settings(&words.morpher);
        let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
        let mut replay = replay_against_oracle(words, &morpher);
        if replay.checked == 0 {
            replay.mismatches.push(OracleMismatch {
                word: "<run>".to_string(),
                what: "run compared no adapter-visible words",
                got: "0 comparisons".to_string(),
                expected: "at least one comparison; declare GuessRoots to include guessed words"
                    .to_string(),
            });
        }
        NamedOracleReplay {
            name,
            is_base,
            replay,
        }
    };

    runs.push(run(fixture.label(), words_yaml, true));
    for variant in &words_yaml.variants {
        let variant_words = words_yaml.for_variant(variant);
        runs.push(run(
            format!("{}#variant={}", fixture.label(), variant.name),
            &variant_words,
            false,
        ));
    }
    runs
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
    for unapplied in words_yaml.morpher.unapplied(morpher) {
        replay.mismatches.push(OracleMismatch {
            word: "<morpher:>".to_string(),
            what:
                "the fixture's morpher: setting is not applied to this Morpher (build its grammar \
                   with FixtureRef::load_grammar)",
            got: unapplied,
            expected: format!("{:?}", words_yaml.morpher),
        });
    }
    let parse_options = pg_parse::ParseOptions::default()
        .with_guess_root(words_yaml.morpher.guess_roots.unwrap_or(false));
    for w in &words_yaml.words {
        if !w.adapter_visible_for_guess_roots(words_yaml.morpher.guess_roots.is_some()) {
            continue; // self-check-only (guess:true parse), PROTOCOL.md section 3
        }
        let outcome = morpher.parse_word_opts(&w.word, &parse_options);
        replay.checked += 1;
        if outcome.capped || outcome.timed_out {
            replay.mismatches.push(OracleMismatch {
                word: w.word.clone(),
                what: "incomplete parse cannot satisfy an oracle expectation",
                got: format!("capped={}, timed_out={}", outcome.capped, outcome.timed_out),
                expected: "complete parse".to_string(),
            });
            continue;
        }
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

/// Render PanGloss results in the Machine adapter's canonical five-column TSV form. The settings
/// file is a direct JSON mapping of supported FieldWorks morpher settings; unsupported settings
/// such as `MergeMSAs` and `MaxAlternatives` are refused instead of silently ignored.
pub fn render_adapter_tsv(
    grammar_xml: &str,
    words_text: &str,
    morpher_json: &str,
) -> Result<String, String> {
    let settings = MorpherSettings::from_json_str(morpher_json)?;
    let mut grammar = pg_grammar::load(grammar_xml).map_err(|e| format!("grammar: {e}"))?;
    settings.apply_to(&mut grammar);
    let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
    let options =
        pg_parse::ParseOptions::default().with_guess_root(settings.guess_roots.unwrap_or(false));

    let mut output = String::new();
    for (index, word) in words_text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
    {
        if word.contains('\t') {
            return Err(format!(
                "word at adapter row {index} contains a tab and cannot be represented in TSV"
            ));
        }
        let started = std::time::Instant::now();
        let outcome = morpher.parse_word_opts(word, &options);
        let elapsed_ms = started.elapsed().as_millis();
        if outcome.capped || outcome.timed_out {
            return Err(format!(
                "incomplete parse at adapter row {index} ({word:?}): capped={}, timed_out={}",
                outcome.capped, outcome.timed_out
            ));
        }
        let (status, signature) = if outcome.invalid_shape {
            ("SKIPPED", "-".to_string())
        } else {
            ("ok", outcome.signature())
        };
        output.push_str(&format!(
            "{index}\t{word}\t{elapsed_ms}\t{status}\t{signature}\n"
        ));
    }
    Ok(output)
}

#[cfg(test)]
mod tests;
