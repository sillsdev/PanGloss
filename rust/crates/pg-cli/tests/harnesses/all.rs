// Included test files share support modules (`mod common;`), so each is loaded once per includer.
#![allow(clippy::duplicate_mod)]

#[path = "../agent_docs_resolve_gate.rs"]
mod agent_docs_resolve_gate;
#[path = "../build_context_provenance.rs"]
mod build_context_provenance;
#[path = "../compile_errors_contract.rs"]
mod compile_errors_contract;
#[path = "../developer_flags_contract.rs"]
mod developer_flags_contract;
#[path = "../divergence_catalogue_gate.rs"]
mod divergence_catalogue_gate;
#[path = "../fixture_pins_never_self_skip.rs"]
mod fixture_pins_never_self_skip;
#[cfg(feature = "foma-tools")]
#[path = "../four_grammar_recipe_evidence.rs"]
mod four_grammar_recipe_evidence;
#[cfg(unix)]
#[path = "../frozen_batch_hard_links.rs"]
mod frozen_batch_hard_links;
#[path = "../fwdata_conformance_gate.rs"]
mod fwdata_conformance_gate;
#[path = "../fwdata_grammar_equivalence_gate.rs"]
mod fwdata_grammar_equivalence_gate;
#[path = "../grammar_dump_diag.rs"]
mod grammar_dump_diag;
#[path = "../guesser_conformance_gate.rs"]
mod guesser_conformance_gate;
#[path = "../inferred_segment_engine_parity_gate.rs"]
mod inferred_segment_engine_parity_gate;
#[cfg(feature = "foma-tools")]
#[path = "../recipe_optimize_continuation.rs"]
mod recipe_optimize_continuation;
#[path = "../skills_never_instruct_bare_cargo.rs"]
mod skills_never_instruct_bare_cargo;

#[path = "../default_dependency_closure.rs"]
mod default_dependency_closure;

#[path = "../underdefined_measurement_integrity_gate.rs"]
mod underdefined_measurement_integrity_gate;

#[path = "../underdefined_stored_keys_gate.rs"]
mod underdefined_stored_keys_gate;
