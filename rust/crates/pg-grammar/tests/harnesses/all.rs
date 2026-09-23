// Included test files share support modules (`mod common;`), so each is loaded once per includer.
#![allow(clippy::duplicate_mod)]

#[path = "../circumfix_conditioning_parity.rs"]
mod circumfix_conditioning_parity;
#[path = "../compile_refusal_gate.rs"]
mod compile_refusal_gate;
#[path = "../conversion_inventory_gate.rs"]
mod conversion_inventory_gate;
#[path = "../lossless_conversion_gate.rs"]
mod lossless_conversion_gate;
#[path = "../measure_only_confinement_gate.rs"]
mod measure_only_confinement_gate;
#[path = "../p5_closure_property.rs"]
mod p5_closure_property;
