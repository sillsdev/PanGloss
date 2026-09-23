// Included test files share support modules (`mod common;`), so each is loaded once per includer.
#![allow(clippy::duplicate_mod)]

#[path = "../admission_single_owner_gate.rs"]
mod admission_single_owner_gate;
#[path = "../backend_accuracy_gate.rs"]
mod backend_accuracy_gate;
#[path = "../backend_capability_cards_contract.rs"]
mod backend_capability_cards_contract;
#[path = "../backend_emission_strategy_gate.rs"]
mod backend_emission_strategy_gate;
#[path = "../backend_mechanism_graph.rs"]
mod backend_mechanism_graph;
#[path = "../backend_optimizer_calibration.rs"]
mod backend_optimizer_calibration;
#[path = "../backend_partition_refinement_gate.rs"]
mod backend_partition_refinement_gate;
#[path = "../backend_promoted_fixtures.rs"]
mod backend_promoted_fixtures;
#[path = "../backend_registry_census.rs"]
mod backend_registry_census;
#[path = "../backend_scoreboard_gate.rs"]
mod backend_scoreboard_gate;
#[path = "../backend_seam_gate.rs"]
mod backend_seam_gate;
#[path = "../backend_selection_contract.rs"]
mod backend_selection_contract;
#[path = "../bare_root_compile_time_discharge.rs"]
mod bare_root_compile_time_discharge;
#[path = "../partial_fst_production_admission_gate.rs"]
mod partial_fst_production_admission_gate;
#[path = "../strategy_aware_capability_gate.rs"]
mod strategy_aware_capability_gate;
#[path = "../strategy_coverage_join_gate.rs"]
mod strategy_coverage_join_gate;
#[path = "../witnessed_strategy_coverage_gate.rs"]
mod witnessed_strategy_coverage_gate;
