//! The gate's tiled-stage decision: a free prospect, a budgeted build, and the sample's rejections against the walk cost.

use super::decide::{Calibration, Ctx};
use super::{
    parse_cost, GateError, StageState, Tiled, TILED_BUILD_SHARE, TILED_POTENTIAL_SHARE, WARMUP,
};
use crate::admission::{
    build_tiled_from, tiled_prospect, RuleAdmission, ADMIT_STEP_WEIGHT, TILED_STEP_WEIGHT,
    TILED_UNIT_WEIGHT,
};
use foma::options::FomaOptions;
use pg_parse::ParseOutcome;

pub(super) struct TiledDecision {
    pub(super) state: StageState,
    pub(super) tiled: Option<Tiled>,
    /// Construction units the build spent, whether or not it was kept.
    pub(super) units: u64,
    /// Walks on the sample made only to decide.
    pub(super) extra: u64,
    /// Projected saving over the rest of the batch, before construction.
    pub(super) net: i128,
}

impl TiledDecision {
    fn off(reason: impl Into<String>) -> Self {
        Self {
            state: StageState::Off(reason.into()),
            tiled: None,
            units: 0,
            extra: 0,
            net: 0,
        }
    }
}

/// Decides the tiled stage on the sample: the warm-up words, whose admitted rules are read here, and the calibration words.
pub(super) fn tiled_stage(
    ctx: &Ctx<'_, '_>,
    admission: &RuleAdmission,
    (words, outcomes): (&[String], &[(usize, ParseOutcome)]),
    (cal, projected_plain): (&Calibration, u64),
) -> Result<TiledDecision, GateError> {
    let prospect = tiled_prospect(ctx.grammar, admission);
    if let Some(reason) = prospect.cannot_be_active() {
        return Ok(TiledDecision::off(reason));
    }
    let mut extra = 0;
    let warm: Vec<Vec<bool>> = outcomes[..WARMUP]
        .iter()
        .map(|&(j, _)| {
            let mut steps = 0;
            let admitted = admission.admit_counted(&words[j], &mut steps);
            extra += steps * ADMIT_STEP_WEIGHT;
            admitted
        })
        .collect();
    let mut sample: Vec<(&str, &[bool], &ParseOutcome)> = Vec::new();
    for (admitted, (j, outcome)) in warm.iter().zip(&outcomes[..WARMUP]) {
        sample.push((&words[*j], admitted, outcome));
    }
    for (admitted, (j, outcome)) in cal.sample.iter().zip(&outcomes[WARMUP..]) {
        sample.push((&words[*j], admitted, outcome));
    }
    let estimate = |outcome: &ParseOutcome| -> u64 {
        let plain = parse_cost(outcome);
        match cal.plain_total {
            0 => 0,
            total => {
                (u128::from(plain) * u128::from(cal.restricted_total) / u128::from(total)) as u64
            }
        }
    };
    let candidates = |(_, admitted, outcome): &(&str, &[bool], &ParseOutcome)| {
        (outcome.analyses.is_empty() || outcome.guessed)
            && !(outcome.capped || outcome.timed_out || outcome.capture_capped)
            && prospect.applies(admitted, &ctx.stage)
    };
    let potential: u64 = sample
        .iter()
        .filter(|s| candidates(s))
        .map(|s| estimate(s.2))
        .sum();
    if potential == 0 {
        return Ok(TiledDecision {
            extra,
            ..TiledDecision::off("no sample word without an analysis to which the stage applies")
        });
    }
    let rest = (words.len() - outcomes.len()) as i128;
    let projected = (i128::from(potential) * rest / sample.len() as i128) as u64;
    let budget_cost = (projected_plain / TILED_BUILD_SHARE).min(projected / TILED_POTENTIAL_SHARE);
    let tiled = build_tiled_from(
        &FomaOptions::default(),
        ctx.grammar,
        prospect,
        budget_cost / TILED_UNIT_WEIGHT,
    );
    let units = tiled.construction_units();
    if let Some(reason) = tiled.inactive_reason() {
        return Ok(TiledDecision {
            units,
            extra,
            ..TiledDecision::off(reason)
        });
    }
    let (mut gain, mut steps) = (0u64, 0u64);
    for &(word, admitted, outcome) in &sample {
        if tiled.rejects_counted(word, admitted, &ctx.stage, &mut steps) {
            if !outcome.analyses.is_empty() && !outcome.guessed {
                return Err(GateError::Disagreement {
                    word: word.to_owned(),
                    stage: "tiled rejection",
                });
            }
            gain += estimate(outcome);
        }
    }
    let walk = steps * TILED_STEP_WEIGHT;
    extra += walk;
    if gain <= walk {
        return Ok(TiledDecision {
            units,
            extra,
            ..TiledDecision::off(format!(
                "sample rejection saves {gain}, the walk over the sample costs {walk}"
            ))
        });
    }
    Ok(TiledDecision {
        state: StageState::On,
        tiled: Some(tiled),
        units,
        extra,
        net: (i128::from(gain) - i128::from(walk)) * rest / sample.len() as i128,
    })
}
