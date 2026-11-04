// Included test files share support modules (`mod common;`), so each is loaded once per includer.
#![allow(clippy::duplicate_mod)]

#[path = "../phase_c_compounding.rs"]
mod phase_c_compounding;
#[path = "../phase_c_strata_depth.rs"]
mod phase_c_strata_depth;
