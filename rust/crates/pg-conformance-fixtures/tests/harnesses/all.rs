// Included test files share support modules (`mod common;`), so each is loaded once per includer.
#![allow(clippy::duplicate_mod)]

#[path = "../build_command_contract.rs"]
mod build_command_contract;
#[path = "../case_set_schema.rs"]
mod case_set_schema;
#[path = "../producibility_marking_gate.rs"]
mod producibility_marking_gate;
#[path = "../three_language_case_set_lock.rs"]
mod three_language_case_set_lock;
