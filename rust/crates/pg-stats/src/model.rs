//! Plain data types moved across the write and read paths.
//!
//! None of these know how to talk to SQLite; `crate::cache` and `crate::report` do the binding.

use std::str::FromStr;

/// The kinds of object a fact row can be attributed to.
///
/// `root_index`, `guesser`, and `overlay` have no authored counterpart in the grammar and get a
/// synthetic stable id (see `IdentityQuality`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    MorphRule,
    PhonRule,
    LexEntry,
    RootIndex,
    Guesser,
    Overlay,
}

impl ObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectKind::MorphRule => "morph_rule",
            ObjectKind::PhonRule => "phon_rule",
            ObjectKind::LexEntry => "lex_entry",
            ObjectKind::RootIndex => "root_index",
            ObjectKind::Guesser => "guesser",
            ObjectKind::Overlay => "overlay",
        }
    }
}

impl std::fmt::Display for ObjectKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when a stored `kind` or `identity_quality` string does not match a known variant.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unrecognized value `{0}`")]
pub struct UnknownVariant(pub String);

impl FromStr for ObjectKind {
    type Err = UnknownVariant;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "morph_rule" => Ok(ObjectKind::MorphRule),
            "phon_rule" => Ok(ObjectKind::PhonRule),
            "lex_entry" => Ok(ObjectKind::LexEntry),
            "root_index" => Ok(ObjectKind::RootIndex),
            "guesser" => Ok(ObjectKind::Guesser),
            "overlay" => Ok(ObjectKind::Overlay),
            other => Err(UnknownVariant(other.to_string())),
        }
    }
}

/// Which pass produced a fact row: unapplying the surface form toward a root (`Analysis`), or
/// reapplying rules forward to build/confirm a surface form (`Synthesis`). Part of the fact key,
/// stored inline rather than interned -- a fixed two-value tag has no locator or label to look up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Analysis,
    Synthesis,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Analysis => "analysis",
            Direction::Synthesis => "synthesis",
        }
    }
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Direction {
    type Err = UnknownVariant;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "analysis" => Ok(Direction::Analysis),
            "synthesis" => Ok(Direction::Synthesis),
            other => Err(UnknownVariant(other.to_string())),
        }
    }
}

/// How trustworthy an object's `key` is as something a human can look up in FLEx.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdentityQuality {
    /// Traceable straight to an authored id (a GUID, an xml id) in the source grammar.
    Authored,
    /// A locator built from structural position (stratum index, allomorph index), not an id.
    Structural,
    /// No authored counterpart exists at all; the id is invented by the collector.
    Synthetic,
}

impl IdentityQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            IdentityQuality::Authored => "authored",
            IdentityQuality::Structural => "structural",
            IdentityQuality::Synthetic => "synthetic",
        }
    }
}

impl std::fmt::Display for IdentityQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for IdentityQuality {
    type Err = UnknownVariant;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "authored" => Ok(IdentityQuality::Authored),
            "structural" => Ok(IdentityQuality::Structural),
            "synthetic" => Ok(IdentityQuality::Synthetic),
            other => Err(UnknownVariant(other.to_string())),
        }
    }
}

/// A structural locator for a `stratum` or `allomorph` dimension row: a key plus a display label.
///
/// `None` in a `FactRecord` means the sentinel row (`stratum_id`/`allomorph_id` 0) applies —
/// not applicable, or no allomorph, respectively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralLocator {
    pub key: String,
    pub label: String,
}

impl StructuralLocator {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
        }
    }
}

/// Everything recorded once per `batch --stats` invocation.
#[derive(Debug, Clone)]
pub struct RunMetadata {
    pub build_info: String,
    pub fwdata_path: String,
    pub grammar_hash: String,
    pub engine: String,
    pub options_hash: String,
    pub options_json: String,
    pub created_utc: String,
}

/// The seven counters for one `(object, stratum, allomorph)` combination inside one word.
///
/// Counters are `u64` here, matching the collector; a value too large for SQLite's signed
/// `INTEGER` storage is an error rather than a silent wraparound, pinned by
/// `huge_counter_round_trips_and_overflow_is_rejected`.
#[derive(Debug, Clone)]
pub struct FactRecord {
    pub object_key: String,
    pub object_kind: ObjectKind,
    pub object_label: String,
    pub identity_quality: IdentityQuality,
    pub stratum: Option<StructuralLocator>,
    pub allomorph: Option<StructuralLocator>,
    /// The morpheme this object's `lex_entry` realizes, so a report can group scattered entries
    /// and allomorphs back to one morpheme. `None` for every other `ObjectKind` -- a rule/root
    /// index/guesser/overlay row names no single morpheme.
    pub morpheme: Option<StructuralLocator>,
    pub direction: Direction,
    pub attempts: u64,
    pub work: u64,
    pub outputs: u64,
    pub not_applied: u64,
    pub no_root: u64,
    pub surface_mismatch: u64,
    pub uses: u64,
    /// Measured wall-clock self time for this object (rule application, allomorph attempt, or
    /// lexicon lookup); always collected whenever `--stats` is on, no derived constant involved.
    pub self_time_ns: u64,
}

/// One analyzed word and every fact row it produced.
#[derive(Debug, Clone)]
pub struct WordRecord {
    pub form: String,
    pub elapsed_ns: u64,
    pub attempts: u64,
    pub passes: u64,
    pub capped: bool,
    pub timed_out: bool,
    pub invalid_shape: bool,
    pub facts: Vec<FactRecord>,
}
