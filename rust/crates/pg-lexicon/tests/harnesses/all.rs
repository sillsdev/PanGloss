// Included test files share support modules (`mod common;`), so each is loaded once per includer.
#![allow(clippy::duplicate_mod)]

#[path = "../analysis_orchestration.rs"]
mod analysis_orchestration;
#[path = "../class_catalog.rs"]
mod class_catalog;
#[path = "../classification.rs"]
mod classification;
#[path = "../persistence_runtime.rs"]
mod persistence_runtime;
#[path = "../supplied_store.rs"]
mod supplied_store;
