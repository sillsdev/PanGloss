//! Re-exports `pg-health`'s FST compilation-health finding schema at its historical
//! `pg_foma::health` path. The types themselves — `HealthReport`, `HealthFinding`, `Severity`,
//! `FindingCode`, `FindingClass`, `Phase`, `Metric`, `MetricValue`, `ValueProvenance`, `Remedy`,
//! `AdmissionByClass`, and `IDEAL_MAX_BYTES`/`severity_for_size_bytes`/`HEALTH_SCHEMA_VERSION` —
//! now live in `pg_health::health` so the Runtime (`pg-wasm`) and the pack format (`pg-pack`) can
//! read a compile's health report without depending on this compiler crate.

pub use pg_health::health::*;
