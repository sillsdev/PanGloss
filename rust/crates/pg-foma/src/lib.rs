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
mod peel_compile_tests;
#[cfg(feature = "test-support")]
pub mod test_support;

pub mod advice_catalog;
pub mod analyzer;
pub mod backend_selection;
pub mod build;
/// Shared compiler classification predicates.
pub mod capability;
pub mod capability_gate;
pub mod characterization;
pub use pg_foma_runtime::candidate_filter;
#[cfg(feature = "test-support")]
pub use pg_foma_runtime::compose_budget;
#[cfg(not(feature = "test-support"))]
pub(crate) use pg_foma_runtime::compose_budget;
pub mod composite;
pub use pg_foma_runtime::confirm;
pub mod emit;
pub mod enumerate;
pub mod gate;
pub mod grammar_semantics;
pub mod junctions;
pub use pg_health::health;
workbench_module! {
    pub mod lower;
}
pub mod oracle;
pub use pg_foma_runtime::peel;
pub mod plan;
workbench_module! {
    pub mod precision;
}
pub(crate) mod preexpand;
workbench_module! {
    pub mod profile;
}
pub mod replace;
pub mod strategy_coverage;
pub mod structural_allomorph;
#[cfg(feature = "test-support")]
pub use pg_foma_runtime::tags;
#[cfg(not(feature = "test-support"))]
pub(crate) use pg_foma_runtime::tags;
pub mod uflexc;
pub(crate) mod unordered;
pub(crate) use pg_foma_runtime::morphotactics;

pub use foma as foma_runtime;
