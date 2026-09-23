//! Which compiler backend realizes a candidate — plain data the pack format and the Runtime also
//! read (a compiled pack names the backend it came from). `pg_foma::enumerate` re-exports this
//! type at its historical path; that module's own doc covers why this is a separate axis from a
//! `Plan`'s assembly shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmissionStrategy {
    /// Interpret `plan` with `build::build_controllable`: the only strategy honouring a plan's assembly shape and expressing a permutation, but it builds the controllable subtree only, omitting whatever marker leaves contribute.
    #[default]
    PlanComposed,
    /// `emit::emit` (surface-probed) + rules: whole-grammar, every construct covered, ignoring `plan` entirely since this compiler derives its own topology.
    TunedSurfaceProbed,
    /// `emit::emit_underlying_templated` + a compiled rewrite cascade: whole-grammar, composite-free, also ignoring `plan` for the same reason.
    TemplatedUnderlyingTokens,
}

impl EmissionStrategy {
    /// Whether this strategy realizes the whole grammar rather than the controllable subtree only.
    /// A caller comparing candidates across strategies needs this: a `PlanComposed` candidate on a
    /// marker-carrying grammar is not measuring the same object as either whole-grammar strategy.
    pub fn is_whole_grammar(self) -> bool {
        !matches!(self, Self::PlanComposed)
    }

    /// Stable identifier for reports and backend ids.
    pub fn label(self) -> &'static str {
        match self {
            Self::PlanComposed => "plan-composed",
            Self::TunedSurfaceProbed => "tuned-surface-probed",
            Self::TemplatedUnderlyingTokens => "templated-underlying-tokens",
        }
    }

    /// The inverse of [`Self::label`]. The single owner of the label<->strategy mapping, so a
    /// caller parsing an external route string (a worker wire frame, a CLI flag) never re-derives
    /// its own copy of this correspondence.
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "plan-composed" => Some(Self::PlanComposed),
            "tuned-surface-probed" => Some(Self::TunedSurfaceProbed),
            "templated-underlying-tokens" => Some(Self::TemplatedUnderlyingTokens),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
