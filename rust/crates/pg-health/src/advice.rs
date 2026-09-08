//! Estimated remedy effort — the one piece of `pg_foma::advice_catalog`'s vocabulary the pack
//! format (`pg-pack`) needs to carry alongside an advice reference. The catalog itself (parsing,
//! validation, rendering, the embedded TOML) stays in `pg-foma`, which re-exports this type at
//! `pg_foma::advice_catalog::RemedyEffort`.

use serde::{Deserialize, Serialize};

/// Estimated effort for one remedy applied to one shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RemedyEffort {
    Easy,
    Medium,
    Hard,
}
