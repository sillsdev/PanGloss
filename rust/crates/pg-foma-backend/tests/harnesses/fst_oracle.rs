// Included test files share support modules (`mod common;`), so each is loaded once per includer.
#![allow(clippy::duplicate_mod)]

#[path = "../cross_table_root_respelling_gate.rs"]
mod cross_table_root_respelling_gate;
#[path = "../exercises_tag_liveness.rs"]
mod exercises_tag_liveness;
#[path = "../f3_parity.rs"]
mod f3_parity;
#[path = "../f5_diacritics_gate.rs"]
mod f5_diacritics_gate;
#[path = "../f6_reduplication_peel_chain_depth.rs"]
mod f6_reduplication_peel_chain_depth;
#[path = "../flag_replace_scope.rs"]
mod flag_replace_scope;
#[path = "../oracle_work_determinism_gate.rs"]
mod oracle_work_determinism_gate;
#[path = "../permanent_variant_refusals.rs"]
mod permanent_variant_refusals;
#[path = "../pk2_eliminate_flag_oracle.rs"]
mod pk2_eliminate_flag_oracle;
