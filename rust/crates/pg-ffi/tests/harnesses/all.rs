// Included test files share support modules (`mod common;`), so each is loaded once per includer.
#![allow(clippy::duplicate_mod)]

#[path = "../ffi_transport_parity.rs"]
mod ffi_transport_parity;
#[path = "../generate_round_trip.rs"]
mod generate_round_trip;
#[path = "../header_abi.rs"]
mod header_abi;
#[path = "../json_api.rs"]
mod json_api;
#[path = "../parse_opts_gate.rs"]
mod parse_opts_gate;
