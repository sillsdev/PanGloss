// Included test files share support modules (`mod common;`), so each is loaded once per includer.
#![allow(clippy::duplicate_mod)]

#[path = "../analyze_text_cli.rs"]
mod analyze_text_cli;
#[path = "../authored_generate_request.rs"]
mod authored_generate_request;
#[path = "../batch_engine.rs"]
mod batch_engine;
#[path = "../build_context_provenance.rs"]
mod build_context_provenance;
#[path = "../compile_errors_contract.rs"]
mod compile_errors_contract;
#[path = "../cost_cli.rs"]
mod cost_cli;
#[path = "../cost_measurement.rs"]
mod cost_measurement;
#[path = "../developer_flags_contract.rs"]
mod developer_flags_contract;
#[cfg(feature = "foma-tools")]
#[path = "../four_grammar_recipe_evidence.rs"]
mod four_grammar_recipe_evidence;
#[cfg(any(unix, windows))]
#[path = "../frozen_batch_hard_links.rs"]
mod frozen_batch_hard_links;
#[path = "../fwdata_conformance_gate.rs"]
mod fwdata_conformance_gate;
#[path = "../fwdata_grammar_equivalence_gate.rs"]
mod fwdata_grammar_equivalence_gate;
#[path = "../grammar_dump_diag.rs"]
mod grammar_dump_diag;
#[path = "../grammar_health_artifacts.rs"]
mod grammar_health_artifacts;
#[path = "../grammar_health_shadowed.rs"]
mod grammar_health_shadowed;
#[path = "../guesser_conformance_gate.rs"]
mod guesser_conformance_gate;
#[path = "../inferred_segment_engine_parity_gate.rs"]
mod inferred_segment_engine_parity_gate;
#[cfg(feature = "foma-tools")]
#[path = "../recipe_optimize_continuation.rs"]
mod recipe_optimize_continuation;
#[path = "../serve_effects.rs"]
mod serve_effects;
#[path = "../serve_generate.rs"]
mod serve_generate;
#[path = "../serve_health.rs"]
mod serve_health;
#[path = "../serve_inventory.rs"]
mod serve_inventory;
#[path = "../serve_match.rs"]
mod serve_match;
#[path = "../serve_protocol.rs"]
mod serve_protocol;
#[path = "../serve_shadowed.rs"]
mod serve_shadowed;
#[cfg(target_os = "linux")]
#[path = "../stats_memory.rs"]
mod stats_memory;

#[path = "../underdefined_measurement_integrity_gate.rs"]
mod underdefined_measurement_integrity_gate;

#[path = "../underdefined_stored_keys_gate.rs"]
mod underdefined_stored_keys_gate;

#[path = "../facts_command.rs"]
mod facts_command;

#[path = "../reserved_word_boundary.rs"]
mod reserved_word_boundary;

#[path = "../slotted_suffix_left_environment.rs"]
mod slotted_suffix_left_environment;

#[path = "../wide_table_segment_identity.rs"]
mod wide_table_segment_identity;

#[path = "../why_not_cli.rs"]
mod why_not_cli;

#[path = "../diagnostic_trace.rs"]
mod diagnostic_trace;
