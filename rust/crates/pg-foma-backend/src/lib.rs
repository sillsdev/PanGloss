#![forbid(unsafe_code)]

macro_rules! workbench_module {
    ($(#[$meta:meta])* pub mod $name:ident $($body:tt)*) => {
        $(#[$meta])*
        #[cfg(feature = "test-support")]
        pub mod $name $($body)*
        #[cfg(not(feature = "test-support"))]
        #[allow(dead_code)]
        pub(crate) mod $name $($body)*
    };
}

#[cfg(test)]
mod test_support;

pub use pg_foma::oracle;
pub use pg_foma::{
    advice_catalog, analyzer, backend_selection, build, capability, capability_gate,
    characterization, composite, emit, enumerate, gate, grammar_semantics, junctions, plan,
    replace, strategy_coverage, structural_allomorph, uflexc,
};
#[cfg(feature = "test-support")]
pub use pg_foma::{lower, precision, profile};
pub use pg_foma_runtime::{candidate_filter, compose_budget, confirm, peel, tags, word_timer};

workbench_module! {
    pub mod backend_accuracy;
}
mod backend_cards_data;
workbench_module! {
    pub mod backend_cards {
        pub use super::backend_cards_data::{
            catalog, checked_in_relative_path, render_markdown, BackendCard, BigO, Envelope,
            EnvelopeControl, CARD_SCHEMA_VERSION,
        };
    }
}
workbench_module! {
    pub mod backend_mechanism;
}
pub mod backend;
pub mod backend_optimizer;
pub mod backend_registry;
pub mod backend_runtime;
workbench_module! {
    pub mod completed_build;
}
pub mod backend_space;
pub mod conformance_coverage;
pub mod coverage_ledger;
workbench_module! {
    pub mod coverage_seam;
}
workbench_module! {
    pub mod e2_infix_probe;
}
workbench_module! {
    pub mod faithfulness_coverage;
}
pub mod health;
pub mod health_evaluator;
workbench_module! {
    pub mod mechanism_provider;
}
workbench_module! {
    pub mod net_shape;
}
workbench_module! {
    pub mod ordering_witnesses;
}
workbench_module! {
    pub mod parity;
}
pub mod plan_diagram;
pub mod plan_interaction_coverage;
workbench_module! {
    pub mod production_admission;
}
workbench_module! {
    pub mod scoreboard;
}
workbench_module! {
    pub mod strategy_coverage_join;
}
workbench_module! {
    pub mod templated_compile;
}
workbench_module! {
    pub mod witnessed_coverage;
}
#[cfg(not(target_arch = "wasm32"))]
pub mod worker;
pub(crate) mod worker_contract;
