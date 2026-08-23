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
}

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

    let mut kinds = BTreeMap::new();
    for kind in CalibKind::ALL {
        let work = work_by_kind.get(&kind).copied().unwrap_or(0);
        let entry = if work > 0 {
            let ns = ns_by_kind.get(&kind).copied().unwrap_or(0);
            let provisional = work < PROVISIONAL_WORK_FLOOR;
            OpCostEntry {
                ns_per_unit: work_weighted_ns_per_unit(ns, work),
                work_observed: work,
                provisional,
                note: if provisional {
                    "thinly represented in this run's conformance suite".to_string()
                } else {
                    "self-timed at the real per-word tick site during an ordinary parse".to_string()
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

    CalibrationConstants {
        schema_version: CALIBRATION_SCHEMA_VERSION,
        provenance: Provenance {
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            cpu_model: cpu_model(),
            measured_utc: measured_utc_now(),
            fixtures: fixture_labels,
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
