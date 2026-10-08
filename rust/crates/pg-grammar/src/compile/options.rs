//! Controls whether a semantically lossy conversion may return a measurement grammar.

/// Whether an incomplete (semantically lossy) conversion may still produce a `Grammar`, for
/// measurement, or must be refused outright. `MeasureOnly` exists only for the structural
/// inventory gate; see [`CompileOptions`] for who must use `Refuse`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SemanticLossPolicy {
    #[default]
    Refuse,
    MeasureOnly,
}

/// Conversion policy for `compile_project_with`; provisional definitions are unconditional.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompileOptions {
    pub semantic_loss: SemanticLossPolicy,
}

impl CompileOptions {
    /// Return the stable JSON projection used to identify these options in a facts artifact.
    /// Keeping policy tags here lets production consumers record the compiler configuration
    /// without depending on measurement-only policy details.
    pub fn canonical_projection_json(self) -> String {
        let semantic_loss = match self.semantic_loss {
            SemanticLossPolicy::Refuse => "refuse",
            SemanticLossPolicy::MeasureOnly => "measureOnly",
        };
        format!("{{\"semanticLossPolicy\":\"{semantic_loss}\"}}")
    }
}

#[cfg(test)]
mod tests;
