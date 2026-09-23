// Included test files share support modules (`mod common;`), so each is loaded once per includer.
#![allow(clippy::duplicate_mod)]

#[path = "../compile_real_projects_gate.rs"]
mod compile_real_projects_gate;
#[path = "../fixture_tests.rs"]
mod fixture_tests;
#[path = "../fwbackup_tests.rs"]
mod fwbackup_tests;
#[path = "../measured_import_parity.rs"]
mod measured_import_parity;
#[path = "../real_projects.rs"]
mod real_projects;
