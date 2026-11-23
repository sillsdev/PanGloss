//! Compiles a grammar to the cascade backend's layers (fst-2 lane, plan sections 2 and 3, W5).
//!
//! W5's feature set: no phonology, affix allomorphs that only add material at the edges
//! (prefixes, suffixes, circumfix halves, null allomorphs, boundaries), templates of such rules,
//! realizational rules, one character table. Every other construct is refused by name, never
//! guessed (see [`refusal`]). The layers are `Lex | zone per stratum | Repr`; nothing here calls
//! foma, and every table is built under a [`BuildMeter`](pg_foma_runtime::cascade::BuildMeter).

pub mod build;
pub mod lex;
pub mod refusal;
pub mod repr;
pub mod spell;
pub mod zone;

pub use build::{build_cascade, Accounting};
pub use refusal::{check_all, BuildError, Refusal};
