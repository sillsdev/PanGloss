// Included test files share support modules (`mod common;`), so each is loaded once per includer.
#![allow(clippy::duplicate_mod)]

#[path = "../n0_gloss_gate.rs"]
mod n0_gloss_gate;
#[path = "../n1_ir_gate.rs"]
mod n1_ir_gate;
#[path = "../n2_realize_gate.rs"]
mod n2_realize_gate;
