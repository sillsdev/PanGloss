//! Stable identifiers for query-time runtime operations a compiled pack can require. A compiled
//! grammar's required set (`pg_pack::compat::RequiredRuntimeFeatures::runtime_operations`) is
//! checked against the Runtime's own provided set at load time (ADR 0004's required ⊆ provided
//! containment); both sides need the same stable string, so it lives here rather than only on the
//! compiler side that produces it.

/// The required-runtime-feature identifier `pg_foma::peel`'s reduplication-peel operation
/// contributes to a compiled pack's required set: only constructs needing a runtime operation
/// (e.g. reduplication → the query-time peel op) contribute. A grammar with no reduplication
/// rules needs nothing from this at all — most constructs are fully lowered and impose no runtime
/// requirement.
///
/// Declared here so the compiler (which decides whether a grammar needs it) and the Runtime
/// (which must provide it) read the same constant; `pg_foma::peel` re-exports it at its
/// historical path.
pub const RUNTIME_FEATURE_REDUPLICATION_PEEL: &str = "reduplication.peel";
