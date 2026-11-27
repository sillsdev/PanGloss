//! Explicit CLI engine selection and opt-in completion metadata.

use pg_parse::ParseOutcome;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Engine {
    Hc,
    Fst,
}

impl Engine {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "hc" => Ok(Self::Hc),
            "fst" => {
                if cfg!(feature = "foma-tools") {
                    Ok(Self::Fst)
                } else {
                    Err("--engine=fst requires a binary built with foma-tools (this binary was built without foma-tools)".into())
                }
            }
            _ => Err(format!("invalid --engine: {value} (expected hc|fst)")),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Completion {
    engine: Engine,
    capped: bool,
    timed_out: bool,
    capture_capped: bool,
    word_mem_capped: bool,
    invalid_shape: bool,
}

impl Completion {
    pub(crate) fn new(engine: Engine, outcome: &ParseOutcome) -> Self {
        Self {
            engine,
            capped: outcome.capped,
            timed_out: outcome.timed_out,
            capture_capped: outcome.capture_capped,
            word_mem_capped: outcome.word_mem_capped,
            invalid_shape: outcome.invalid_shape,
        }
    }
}
