//! Policies `compile_project_with` resolves once, up front, instead of threading FieldWorks-shaped
//! parser/cap state through the compiler (see [`SubstratePolicy::resolve`]'s own doc for why no
//! `ActiveParser`/XAMPLE cap value survives past that one call).

use pg_snapshot::ActiveParser;

/// How the compiler should treat a phonological substrate the source project never declared as a
/// closed inventory.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SubstratePolicy {
    /// Resolves to [`ResolvedSubstratePolicy::CompleteFromUsage`] for an XAmple-configured project
    /// or one that explicitly accepted unspecified graphemes; [`ResolvedSubstratePolicy::Strict`]
    /// otherwise. See [`SubstratePolicy::resolve`].
    #[default]
    Auto,
    Strict,
    CompleteFromUsage,
}

/// [`SubstratePolicy::Auto`]'s resolved reading for one project -- never itself carries `Auto`, so
/// every later compiler stage matches on exactly two cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedSubstratePolicy {
    Strict,
    CompleteFromUsage,
}

impl SubstratePolicy {
    /// Resolves `Auto` against the two facts that decide it, then discards both: an XAmple-shaped
    /// project never required a closed grapheme declaration, and an HC project that authored
    /// `AcceptUnspecifiedGraphemes` said the same thing about its own inventory. Neither fact is
    /// retained anywhere past this call -- see [`CompileOptions`].
    pub fn resolve(
        self,
        active_parser: ActiveParser,
        accept_unspecified_graphemes: bool,
    ) -> ResolvedSubstratePolicy {
        match self {
            SubstratePolicy::Strict => ResolvedSubstratePolicy::Strict,
            SubstratePolicy::CompleteFromUsage => ResolvedSubstratePolicy::CompleteFromUsage,
            SubstratePolicy::Auto => {
                if matches!(active_parser, ActiveParser::XAmple) || accept_unspecified_graphemes {
                    ResolvedSubstratePolicy::CompleteFromUsage
                } else {
                    ResolvedSubstratePolicy::Strict
                }
            }
        }
    }
}

/// Whether an incomplete (semantically lossy) conversion may still produce a `Grammar`, for
/// measurement, or must be refused outright. `MeasureOnly` exists only for the structural
/// inventory gate; see [`CompileOptions`] for who must use `Refuse`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SemanticLossPolicy {
    #[default]
    Refuse,
    MeasureOnly,
}

/// Every policy [`super::compile_project_with`] resolves before compiling. Production CLI/worker
/// and XAMPLE result-comparator HC callers use `Refuse`. Deliberately carries no `ActiveParser`/
/// XAMPLE cap state -- see [`SubstratePolicy::resolve`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompileOptions {
    pub substrate: SubstratePolicy,
    pub semantic_loss: SemanticLossPolicy,
}

#[cfg(test)]
mod tests;
