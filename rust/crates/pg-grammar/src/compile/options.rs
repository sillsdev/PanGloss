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
mod tests {
    use super::*;

    #[test]
    fn auto_completes_xample_authored_or_explicitly_accepted_unspecified_graphemes() {
        assert_eq!(
            SubstratePolicy::Auto.resolve(ActiveParser::XAmple, false),
            ResolvedSubstratePolicy::CompleteFromUsage
        );
        assert_eq!(
            SubstratePolicy::Auto.resolve(ActiveParser::XAmple, true),
            ResolvedSubstratePolicy::CompleteFromUsage
        );
        assert_eq!(
            SubstratePolicy::Auto.resolve(ActiveParser::Hc, true),
            ResolvedSubstratePolicy::CompleteFromUsage
        );
        assert_eq!(
            SubstratePolicy::Auto.resolve(ActiveParser::Hc, false),
            ResolvedSubstratePolicy::Strict
        );
    }

    #[test]
    fn strict_and_complete_from_usage_are_fixed_points_regardless_of_the_two_facts() {
        for (active_parser, accept) in [
            (ActiveParser::XAmple, false),
            (ActiveParser::XAmple, true),
            (ActiveParser::Hc, false),
            (ActiveParser::Hc, true),
        ] {
            assert_eq!(
                SubstratePolicy::Strict.resolve(active_parser, accept),
                ResolvedSubstratePolicy::Strict
            );
            assert_eq!(
                SubstratePolicy::CompleteFromUsage.resolve(active_parser, accept),
                ResolvedSubstratePolicy::CompleteFromUsage
            );
        }
    }

    /// No `..` rest pattern -- a new field here fails to compile until this test names it too.
    #[test]
    fn compile_options_carries_exactly_substrate_and_semantic_loss() {
        let CompileOptions {
            substrate,
            semantic_loss,
        } = CompileOptions::default();
        assert_eq!(substrate, SubstratePolicy::Auto);
        assert_eq!(semantic_loss, SemanticLossPolicy::Refuse);
    }
}
