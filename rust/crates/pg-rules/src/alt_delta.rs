//! `HC_ALT_DELTA_STATS=1` diagnostic (env-gated, off by default): task T5(a) --
//! `docs/research/memory-measurement-repair.md`. For each stored `Word::alternatives` entry, measures
//! the byte cost the CURRENT full-clone representation pays versus the byte cost a delta
//! representation would need. `Word::expand_alternatives` (`crate::word`) already reconstructs an
//! alternative from its `source` spine plus a shape + trail-suffix (`mrule_apps` beyond the source's
//! own length) + non-head-suffix + a real-fs diff + a root-allomorph delta, so that delta shape is
//! not invented here, only measured. This module changes no storage format; it only measures whether
//! one would be worth building.

use std::cell::{Cell, RefCell};

use crate::word::{estimate_fs_bytes, estimate_shape_bytes, estimate_word_bytes};
use crate::Word;

thread_local! {
    static ENABLED: Cell<Option<bool>> = const { Cell::new(None) };
    static FULL_BYTES: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    static DELTA_BYTES: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    /// Out of the 10 fields `identical_field_count` compares, per alternative.
    static IDENTICAL_FIELDS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

pub fn enabled() -> bool {
    ENABLED.with(|c| {
        if let Some(v) = c.get() {
            return v;
        }
        let v = std::env::var("HC_ALT_DELTA_STATS").is_ok();
        c.set(Some(v));
        v
    })
}

/// Fields compared, excluding ones expected to always differ (`shape`, `real_fs`, `mrule_apps`, `non_heads`, `source`, `alternatives`, `trace`).
const COMPARABLE_FIELD_COUNT: u32 = 10;

fn identical_field_count(canonical: &Word, alt: &Word) -> u32 {
    let mut n = 0;
    if alt.stratum == canonical.stratum {
        n += 1;
    }
    if alt.syn_fs == canonical.syn_fs {
        n += 1;
    }
    if alt.mpr == canonical.mpr {
        n += 1;
    }
    if alt.morphs == canonical.morphs {
        n += 1;
    }
    if alt.non_head_app_index == canonical.non_head_app_index {
        n += 1;
    }
    if alt.root_allomorph == canonical.root_allomorph {
        n += 1;
    }
    if alt.root_runtime_id == canonical.root_runtime_id {
        n += 1;
    }
    if alt.obligatory == canonical.obligatory {
        n += 1;
    }
    if alt.unapplied_rule_counts == canonical.unapplied_rule_counts {
        n += 1;
    }
    if alt.flags == canonical.flags {
        n += 1;
    }
    n
}

/// The delta `Word::expand_alternatives` would need: shape + `mrule_apps`/`non_heads` suffixes + a real-fs diff + a root-allomorph delta; falls back to the full cost with no `source`.
fn estimate_alt_delta_bytes(alt: &Word) -> usize {
    let Some(src) = alt.source.as_ref() else {
        return estimate_word_bytes(alt);
    };
    let mut n = estimate_shape_bytes(&alt.shape);
    if alt.mrule_apps.len() > src.mrule_apps.len() {
        n += (alt.mrule_apps.len() - src.mrule_apps.len())
            * std::mem::size_of::<Option<pg_grammar::model::MRuleId>>();
    }
    if alt.non_heads.len() > src.non_heads.len() {
        for nh in &alt.non_heads[src.non_heads.len()..] {
            n += estimate_word_bytes(nh);
        }
    }
    if alt.real_fs != src.real_fs {
        n += estimate_fs_bytes(&pg_featstruct::subtract(&alt.real_fs, &src.real_fs));
    }
    if alt.root_allomorph != src.root_allomorph {
        n += std::mem::size_of::<Option<pg_grammar::model::AllomorphId>>()
            + alt.root_runtime_id.as_ref().map_or(0, String::len);
    }
    n
}

/// Record every direct `alternatives` entry of `canonical` (not the recursively-expanded set --
/// `Word::expand_alternatives`'s own output -- but the entries actually stored on the `Word`).
pub fn record(canonical: &Word) {
    if !enabled() || canonical.alternatives.is_empty() {
        return;
    }
    for alt in &canonical.alternatives {
        let full = estimate_word_bytes(alt) as u64;
        let delta = estimate_alt_delta_bytes(alt) as u64;
        let fields = identical_field_count(canonical, alt);
        FULL_BYTES.with(|v| v.borrow_mut().push(full));
        DELTA_BYTES.with(|v| v.borrow_mut().push(delta));
        IDENTICAL_FIELDS.with(|v| v.borrow_mut().push(fields));
    }
}

/// Aggregate ratio (sum(delta) / sum(full)) and per-alternative sample count -- `pg-cli` reads this
/// once at end-of-parse (mirroring `word_stats::record_memo_snapshot`'s own call shape) and prints
/// distribution percentiles from the raw samples.
pub struct AltDeltaSnapshot {
    pub samples: usize,
    pub full_bytes_total: u64,
    pub delta_bytes_total: u64,
    pub mean_identical_fields: f64,
}

pub fn snapshot() -> AltDeltaSnapshot {
    let full_bytes_total = FULL_BYTES.with(|v| v.borrow().iter().sum());
    let delta_bytes_total = DELTA_BYTES.with(|v| v.borrow().iter().sum());
    let (samples, field_total) = IDENTICAL_FIELDS.with(|v| {
        let v = v.borrow();
        (v.len(), v.iter().map(|&n| n as u64).sum::<u64>())
    });
    let mean_identical_fields = if samples > 0 {
        field_total as f64 / samples as f64 / COMPARABLE_FIELD_COUNT as f64
    } else {
        0.0
    };
    AltDeltaSnapshot {
        samples,
        full_bytes_total,
        delta_bytes_total,
        mean_identical_fields,
    }
}

/// Percentile (0.0-1.0) over `(delta_bytes / full_bytes)` ratios, one per sampled alternative;
/// `full_bytes == 0` samples are excluded (nothing to ratio against; not observed in practice since
/// every `Word` carries a nonzero `base` cost, kept as a defensive skip rather than a divide-by-zero).
pub fn delta_full_ratio_percentile(p: f64) -> f64 {
    let mut ratios: Vec<f64> = FULL_BYTES.with(|full| {
        DELTA_BYTES.with(|delta| {
            full.borrow()
                .iter()
                .zip(delta.borrow().iter())
                .filter(|(&f, _)| f > 0)
                .map(|(&f, &d)| d as f64 / f as f64)
                .collect()
        })
    });
    if ratios.is_empty() {
        return 0.0;
    }
    ratios.sort_by(|a, b| a.partial_cmp(b).expect("ratios are never NaN"));
    let idx = ((ratios.len() - 1) as f64 * p).round() as usize;
    ratios[idx]
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_grammar::model::StratumId;
    use pg_shape::ShapeBuilder;
    use std::rc::Rc;

    fn w() -> Word {
        Word::new(ShapeBuilder::new().finish(), StratumId(0))
    }

    #[test]
    fn record_is_a_no_op_when_disabled() {
        if enabled() {
            return; // env var already set by another test in this process; skip rather than false-fail.
        }
        let mut canonical = w();
        canonical.alternatives.push(Rc::new(w()));
        record(&canonical);
        assert_eq!(snapshot().samples, 0);
    }

    #[test]
    fn identical_field_count_matches_a_field_by_field_walk() {
        let canonical = w();
        let alt = w();
        // A freshly-built alternative with no divergence at all matches every comparable field.
        assert_eq!(
            identical_field_count(&canonical, &alt),
            COMPARABLE_FIELD_COUNT
        );
    }

    #[test]
    fn delta_falls_back_to_full_cost_with_no_source() {
        let mut alt = w();
        alt.morphs = vec![crate::word::MorphRecord::new(
            pg_grammar::model::AllomorphId(1),
            pg_grammar::model::MorphemeId(2),
            0,
        )];
        assert!(alt.source.is_none());
        assert_eq!(estimate_alt_delta_bytes(&alt), estimate_word_bytes(&alt));
    }
}
