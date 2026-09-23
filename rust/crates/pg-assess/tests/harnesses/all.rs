// Included test files share support modules (`mod common;`), so each is loaded once per includer.
#![allow(clippy::duplicate_mod)]

#[path = "../certification_ledger.rs"]
mod certification_ledger;
#[path = "../duplicate_count_determinism.rs"]
mod duplicate_count_determinism;
#[path = "../identity_projection.rs"]
mod identity_projection;
#[path = "../schema_conformance.rs"]
mod schema_conformance;
