//! Which compiler backend realizes a candidate — plain data the pack format and the Runtime also
//! read (a compiled pack names the backend it came from). `pg_foma::enumerate` re-exports this
//! type at its historical path; that module's own doc covers why this is a separate axis from a
//! `Plan`'s assembly shape.
//!
//! Only one backend ships. The enum stays an enum, with its label table and `crate::backend` trait
//! in `pg-foma-backend`, so another backend can be added back or compared by adding a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmissionStrategy {
    /// `emit::emit` (surface-probed) + rules: whole-grammar, every construct covered, ignoring `plan` entirely since this compiler derives its own topology.
    #[default]
    TunedSurfaceProbed,
}

impl EmissionStrategy {
    /// Stable identifier for reports and backend ids.
    pub fn label(self) -> &'static str {
        match self {
            Self::TunedSurfaceProbed => "tuned-surface-probed",
        }
    }

    /// The inverse of [`Self::label`]. The single owner of the label<->strategy mapping, so a
    /// caller parsing an external route string (a worker wire frame, a CLI flag) never re-derives
    /// its own copy of this correspondence.
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "tuned-surface-probed" => Some(Self::TunedSurfaceProbed),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
