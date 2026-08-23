//! `pangloss calibrate`: measures per-kind `op_cost` constants over the conformance suite and writes a committed, diffable file with provenance.
//! See `docs/research/pangloss-stats-attribution-and-aggregation-spec.md`'s "Time" section for the Σns/Σwork estimator this implements.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pg_conformance_fixtures::ConformanceScope;
use pg_parse::{Morpher, ParseOptions};
use serde::{Deserialize, Serialize};

/// Which `op_cost` bucket a measurement belongs to; `as_str` matches `pg_stats::ObjectKind`'s kind key.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum CalibKind {
    MorphRule,
    PhonRule,
    LexEntry,
    RootIndex,
    Guesser,
    Overlay,
}

impl CalibKind {
    pub(crate) const ALL: [CalibKind; 6] = [
        CalibKind::MorphRule,
        CalibKind::PhonRule,
        CalibKind::LexEntry,
        CalibKind::RootIndex,
        CalibKind::Guesser,
        CalibKind::Overlay,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            CalibKind::MorphRule => "morph_rule",
            CalibKind::PhonRule => "phon_rule",
            CalibKind::LexEntry => "lex_entry",
            CalibKind::RootIndex => "root_index",
            CalibKind::Guesser => "guesser",
            CalibKind::Overlay => "overlay",
        }
    }

    fn from_stats_row_kind(k: pg_rules::stats::ObjectKind) -> Self {
        match k {
            pg_rules::stats::ObjectKind::MorphRule => CalibKind::MorphRule,
            pg_rules::stats::ObjectKind::PhonRule => CalibKind::PhonRule,
            pg_rules::stats::ObjectKind::LexEntry => CalibKind::LexEntry,
            pg_rules::stats::ObjectKind::RootIndex => CalibKind::RootIndex,
            pg_rules::stats::ObjectKind::Guesser => CalibKind::Guesser,
            pg_rules::stats::ObjectKind::Overlay => CalibKind::Overlay,
        }
    }

    /// Which `CostBucket` this kind's constant is drawn from -- see that type's doc.
    fn bucket(self) -> CostBucket {
        match self {
            CalibKind::RootIndex => CostBucket::RootIndex,
            CalibKind::MorphRule
            | CalibKind::PhonRule
            | CalibKind::LexEntry
            | CalibKind::Guesser
            | CalibKind::Overlay => CostBucket::Default,
        }
    }
}

/// Which of two calibration buckets a kind's constant is drawn from: per-kind constants did not discriminate, except for `root_index`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum CostBucket {
    /// Every per-object match/rewrite/lookup kind: `morph_rule`, `phon_rule`, `lex_entry`, `guesser`, `overlay`.
    Default,
    /// The shared per-stratum trie walk -- distinctly cheaper, and shared rather than per-object.
    RootIndex,
}

impl CostBucket {
    fn as_str(self) -> &'static str {
        match self {
            CostBucket::Default => "default",
            CostBucket::RootIndex => "root_index",
        }
    }
}

/// One kind's measured (or unmeasured) constant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct OpCostEntry {
    /// `None` means zero timed observations for this kind — an absent measurement, never a silent zero cost.
    pub(crate) ns_per_unit: Option<f64>,
    pub(crate) work_observed: u64,
    pub(crate) provisional: bool,
    pub(crate) note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct Provenance {
    pub(crate) tool_version: String,
    pub(crate) cpu_model: String,
    pub(crate) measured_utc: String,
    pub(crate) fixtures: Vec<String>,
    /// States the bucket collapse; absent (`#[serde(default)]`) on a file written before it existed.
    #[serde(default)]
    pub(crate) calibration_model: String,
}

/// What `Provenance.calibration_model` says on every freshly written file -- see `CostBucket`'s doc.
const CALIBRATION_MODEL_NOTE: &str = "two-bucket: 'default' (morph_rule, phon_rule, lex_entry, guesser, overlay) and 'root_index' (its own bucket); per-kind constants for the first group were measured within 7% of each other and do not discriminate";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CalibrationConstants {
    pub(crate) schema_version: u32,
    pub(crate) provenance: Provenance,
    pub(crate) kinds: BTreeMap<String, OpCostEntry>,
}

pub(crate) const CALIBRATION_SCHEMA_VERSION: u32 = 1;

impl CalibrationConstants {
    /// The measured constant for `kind`, or an error naming the gap, never a silent `Ok(0.0)`.
    /// Pinned by `missing_kind_is_surfaced_not_defaulted_to_zero`.
    #[allow(dead_code)]
    pub(crate) fn ns_per_unit(&self, kind: &str) -> Result<f64, String> {
        let entry = self.kinds.get(kind).ok_or_else(|| {
            format!("op_cost: kind {kind:?} is absent from this calibration file")
        })?;
        entry.ns_per_unit.ok_or_else(|| {
            format!(
                "op_cost: kind {kind:?} has no measured constant ({})",
                entry.note
            )
        })
    }
}

/// Σns / Σwork — never the mean of per-item rates, which a pathological fixture would distort.
pub(crate) fn work_weighted_ns_per_unit(total_ns: u64, total_work: u64) -> Option<f64> {
    if total_work == 0 {
        None
    } else {
        Some(total_ns as f64 / total_work as f64)
    }
}

/// Below this many work units, a kind's constant is marked `provisional`: real, but thin.
const PROVISIONAL_WORK_FLOOR: u64 = 1_000;

fn cpu_model() -> String {
    let sys = sysinfo::System::new_all();
    sys.cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn measured_utc_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

fn default_out_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/stats_op_cost.json")
}

/// Runtime candidates for the calibration file, tried in order: beside the running executable first, then `default_out_path`'s compile-time-baked checkout path (correct only for `cargo run`/tests).
fn runtime_op_cost_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join("data").join("stats_op_cost.json"));
        }
    }
    out.push(default_out_path());
    out
}

/// Loads calibration constants for `batch --stats`; absent, unreadable, or unparseable at every candidate is `None`, never an error, so a stats run cannot fail because a calibration file is missing.
pub(crate) fn load_op_cost_constants() -> Option<CalibrationConstants> {
    for path in runtime_op_cost_candidates() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(constants) = serde_json::from_str(&text) {
            return Some(constants);
        }
    }
    None
}

const CALIBRATE_USAGE: &str = "usage: calibrate [--out FILE]";

pub(crate) fn run_calibrate(args: &[String]) -> Result<(), String> {
    let mut out_path: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => out_path = Some(it.next().ok_or("--out requires a value")?.clone()),
            s if s.starts_with("--out=") => out_path = Some(s["--out=".len()..].to_string()),
            other => return Err(format!("{CALIBRATE_USAGE}\nunrecognized argument: {other}")),
        }
    }
    let out_path = out_path.map(PathBuf::from).unwrap_or_else(default_out_path);

    let fixtures = pg_conformance_fixtures::discover_scoped(ConformanceScope::All);
    let constants = measure_conformance_suite(&fixtures);

    let json = serde_json::to_string_pretty(&constants)
        .map_err(|e| format!("serialize calibration constants: {e}"))?;
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(&out_path, format!("{json}\n"))
        .map_err(|e| format!("write {}: {e}", out_path.display()))?;

    println!(
        "calibrate: {} fixtures contributed, wrote {}",
        constants.provenance.fixtures.len(),
        out_path.display()
    );
    for kind in CalibKind::ALL {
        let entry = &constants.kinds[kind.as_str()];
        match entry.ns_per_unit {
            Some(ns) => println!(
                "  {:<10} ns_per_unit={ns:.3} work_observed={} provisional={}",
                kind.as_str(),
                entry.work_observed,
                entry.provisional
            ),
            None => println!("  {:<10} unmeasured ({})", kind.as_str(), entry.note),
        }
    }
    Ok(())
}

/// Collapses per-kind totals into `CostBucket`-shared constants; zero own work stays unmeasured.
fn bucketed_kind_entries(
    ns_by_kind: &BTreeMap<CalibKind, u64>,
    work_by_kind: &BTreeMap<CalibKind, u64>,
) -> BTreeMap<String, OpCostEntry> {
    let mut ns_by_bucket: BTreeMap<CostBucket, u64> = BTreeMap::new();
    let mut work_by_bucket: BTreeMap<CostBucket, u64> = BTreeMap::new();
    for kind in CalibKind::ALL {
        *ns_by_bucket.entry(kind.bucket()).or_default() +=
            ns_by_kind.get(&kind).copied().unwrap_or(0);
        *work_by_bucket.entry(kind.bucket()).or_default() +=
            work_by_kind.get(&kind).copied().unwrap_or(0);
    }

    let mut kinds = BTreeMap::new();
    for kind in CalibKind::ALL {
        let own_work = work_by_kind.get(&kind).copied().unwrap_or(0);
        let entry = if own_work > 0 {
            let bucket = kind.bucket();
            let bucket_work = work_by_bucket.get(&bucket).copied().unwrap_or(0);
            let bucket_ns = ns_by_bucket.get(&bucket).copied().unwrap_or(0);
            let provisional = bucket_work < PROVISIONAL_WORK_FLOOR;
            OpCostEntry {
                ns_per_unit: work_weighted_ns_per_unit(bucket_ns, bucket_work),
                work_observed: own_work,
                provisional,
                note: if provisional {
                    format!(
                        "'{}' bucket thinly represented in this run's conformance suite",
                        bucket.as_str()
                    )
                } else {
                    format!(
                        "'{}' bucket, self-timed at the real per-word tick site during an ordinary parse",
                        bucket.as_str()
                    )
                },
            }
        } else {
            OpCostEntry {
                ns_per_unit: None,
                work_observed: 0,
                provisional: true,
                note: "no per-call self-time instrumentation is wired for this kind in this build"
                    .to_string(),
            }
        };
        kinds.insert(kind.as_str().to_string(), entry);
    }
    kinds
}

/// Real, single-threaded measurement: every kind's self-time comes from the same ordinary parse `batch --stats` would run.
fn measure_conformance_suite(
    fixtures: &[pg_conformance_fixtures::FixtureRef],
) -> CalibrationConstants {
    let mut ns_by_kind: BTreeMap<CalibKind, u64> = BTreeMap::new();
    let mut work_by_kind: BTreeMap<CalibKind, u64> = BTreeMap::new();
    let mut fixture_labels = Vec::new();

    for fixture in fixtures {
        let words_yaml = fixture.load_words_yaml();
        if words_yaml.skip_in_generic_replay().is_some() {
            continue;
        }
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        let morpher = Morpher::new(&grammar, usize::MAX)
            .with_word_timeout(Some(std::time::Duration::from_millis(5_000)));

        let mut touched = false;
        for w in &words_yaml.words {
            if w.expect_skip {
                continue;
            }
            let (_outcome, _rows, calib) =
                morpher.parse_word_with_stats_and_calibration(&w.word, &ParseOptions::default());
            for (kind, totals) in calib {
                let calib_kind = CalibKind::from_stats_row_kind(kind);
                *ns_by_kind.entry(calib_kind).or_default() += totals.ns;
                *work_by_kind.entry(calib_kind).or_default() += totals.work;
            }
            touched = true;
        }
        if touched {
            fixture_labels.push(fixture.label());
        }
    }

    let kinds = bucketed_kind_entries(&ns_by_kind, &work_by_kind);

    CalibrationConstants {
        schema_version: CALIBRATION_SCHEMA_VERSION,
        provenance: Provenance {
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            cpu_model: cpu_model(),
            measured_utc: measured_utc_now(),
            fixtures: fixture_labels,
            calibration_model: CALIBRATION_MODEL_NOTE.to_string(),
        },
        kinds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimator_is_work_weighted_not_a_mean_of_rates() {
        // Sample A: 100ns/10work (rate 10). Sample B: 1_000_000ns/100work (rate 10_000).
        let total_ns = 100u64 + 1_000_000;
        let total_work = 10u64 + 100;
        let naive_mean_of_rates = (10.0 + 10_000.0) / 2.0;
        let got = work_weighted_ns_per_unit(total_ns, total_work).unwrap();
        assert!(
            (got - naive_mean_of_rates).abs() > 1.0,
            "work-weighted estimate must differ from the naive mean of per-item rates"
        );
        assert!((got - (total_ns as f64 / total_work as f64)).abs() < 1e-9);
    }

    #[test]
    fn estimator_is_none_with_zero_work() {
        assert_eq!(work_weighted_ns_per_unit(500, 0), None);
    }

    /// The three `default`-bucket kinds must share one constant, distinct from `root_index`'s.
    #[test]
    fn bucketed_kind_entries_collapses_default_kinds_to_one_shared_constant() {
        let mut ns_by_kind = BTreeMap::new();
        let mut work_by_kind = BTreeMap::new();
        ns_by_kind.insert(CalibKind::MorphRule, 4_711_358);
        work_by_kind.insert(CalibKind::MorphRule, 10_000);
        ns_by_kind.insert(CalibKind::PhonRule, 4_571_972);
        work_by_kind.insert(CalibKind::PhonRule, 10_000);
        ns_by_kind.insert(CalibKind::LexEntry, 4_389_054);
        work_by_kind.insert(CalibKind::LexEntry, 10_000);
        ns_by_kind.insert(CalibKind::RootIndex, 839_340);
        work_by_kind.insert(CalibKind::RootIndex, 10_000);

        let kinds = bucketed_kind_entries(&ns_by_kind, &work_by_kind);
        let morph = kinds["morph_rule"].ns_per_unit.unwrap();
        let phon = kinds["phon_rule"].ns_per_unit.unwrap();
        let lex = kinds["lex_entry"].ns_per_unit.unwrap();
        let root = kinds["root_index"].ns_per_unit.unwrap();

        assert_eq!(
            morph, phon,
            "morph_rule and phon_rule share the 'default' bucket's one constant"
        );
        assert_eq!(
            morph, lex,
            "lex_entry shares that same bucket constant, not its own independent rate"
        );
        let expected_default = (4_711_358.0 + 4_571_972.0 + 4_389_054.0) / (10_000.0 * 3.0);
        assert!(
            (morph - expected_default).abs() < 1e-6,
            "the shared constant is the bucket's Σns/Σwork, not any one kind's own rate"
        );
        assert!(
            root < morph / 2.0,
            "root_index must stay its own, distinctly cheaper bucket"
        );

        assert_eq!(
            kinds["guesser"].ns_per_unit, None,
            "a kind with zero of its own instrumented work must stay unmeasured, not borrow the bucket's rate"
        );
    }

    #[test]
    fn bucketed_kind_entries_work_observed_stays_per_kind() {
        let mut ns_by_kind = BTreeMap::new();
        let mut work_by_kind = BTreeMap::new();
        ns_by_kind.insert(CalibKind::MorphRule, 100_000);
        work_by_kind.insert(CalibKind::MorphRule, 200);
        ns_by_kind.insert(CalibKind::PhonRule, 50_000);
        work_by_kind.insert(CalibKind::PhonRule, 100);

        let kinds = bucketed_kind_entries(&ns_by_kind, &work_by_kind);
        assert_eq!(
            kinds["morph_rule"].work_observed, 200,
            "work_observed stays this kind's own evidence, for diagnosability"
        );
        assert_eq!(kinds["phon_rule"].work_observed, 100);
    }

    fn sample_constants() -> CalibrationConstants {
        let mut kinds = BTreeMap::new();
        kinds.insert(
            "root_index".to_string(),
            OpCostEntry {
                ns_per_unit: Some(42.5),
                work_observed: 10_000,
                provisional: false,
                note: "measured".to_string(),
            },
        );
        kinds.insert(
            "guesser".to_string(),
            OpCostEntry {
                ns_per_unit: None,
                work_observed: 0,
                provisional: true,
                note: "no per-call self-time instrumentation is wired for this kind in this build"
                    .to_string(),
            },
        );
        CalibrationConstants {
            schema_version: CALIBRATION_SCHEMA_VERSION,
            provenance: Provenance {
                tool_version: "0.0.0-test".to_string(),
                cpu_model: "Test CPU".to_string(),
                measured_utc: "unix:0".to_string(),
                fixtures: vec!["staging:languages:example".to_string()],
                calibration_model: "test".to_string(),
            },
            kinds,
        }
    }

    #[test]
    fn constants_file_round_trips_through_json() {
        let original = sample_constants();
        let json = serde_json::to_string_pretty(&original).unwrap();
        let parsed: CalibrationConstants = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn missing_kind_is_surfaced_not_defaulted_to_zero() {
        let constants = sample_constants();
        assert_eq!(constants.ns_per_unit("root_index"), Ok(42.5));
        assert!(
            constants.ns_per_unit("morph_rule").is_err(),
            "a kind absent from the file must error, not silently read as 0.0"
        );
        assert!(
            constants.ns_per_unit("guesser").is_err(),
            "a kind present but unmeasured (ns_per_unit: None) must error, not silently read as 0.0"
        );
    }

    #[test]
    fn calib_kind_as_str_matches_the_schema_kind_vocabulary() {
        let expected = [
            "morph_rule",
            "phon_rule",
            "lex_entry",
            "root_index",
            "guesser",
            "overlay",
        ];
        for (kind, name) in CalibKind::ALL.iter().zip(expected) {
            assert_eq!(kind.as_str(), name);
        }
    }
}
