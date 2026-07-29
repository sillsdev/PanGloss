//! A single importer/snapshot-validation warning: a stable short `code` alongside its existing
//! human-readable `message` prose (`openspec/changes/add-grammar-assessment` task 3.8).
//!
//! `pangloss compare` diffs two assessment reports' warnings **by code and count only, never by
//! message text** — so that rewording a message's prose is never itself reported as a change in
//! the grammar's context. Every emission site across `pg-fwdata` and `pg-snapshot::validate`
//! picks a code that names the actual situation it detected (e.g. `"fwdata.dangling-reference"`,
//! `"snapshot.reference-out-of-scope"`); identical situations at different call sites
//! intentionally share a code — no taxonomy of codes is designed up front (see that task's own
//! wording), grouping instead follows each site's actual meaning.
//!
//! # Caller compatibility
//!
//! [`Warning`] implements [`std::fmt::Display`] and [`std::ops::Deref`]`<Target = str>` against
//! `message` alone, printing/comparing exactly the same text every pre-existing bare-`String`
//! warning did. Every caller that only ever printed a warning or pattern-matched its prose (e.g.
//! `w.contains("...")`) keeps compiling and behaving identically without any change; only call
//! sites that want the new `code` field need to be touched.
use std::fmt;
use std::ops::Deref;

/// One importer or snapshot-validation warning. See the module doc for the `code`/`message`
/// contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// Short, stable, dotted identifier naming the situation this warning reports (e.g.
    /// `"fwdata.dangling-reference"`). This is the field [`compare`](crate) diffs on — it must
    /// stay the same across a reword of `message`, and two structurally different situations must
    /// never share one.
    pub code: &'static str,
    /// Human-readable prose, exactly as it read before codes existed. Free to reword at any time
    /// without being a diff-relevant change.
    pub message: String,
}

impl Warning {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Warning {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Lets every existing `&str`-shaped call (`.contains(...)`, `.is_empty()`, ...) keep working
/// unchanged against `message` — see the module doc's "Caller compatibility" section.
///
/// # Why this `Deref` exists despite the API guidelines
///
/// Rust's API guidelines discourage `Deref` on a type that is not a smart pointer, because the
/// implicit coercion is surprising: here, a `&Warning` will silently behave as its prose and lose
/// the code. That is a real cost and it was accepted deliberately, for one reason — the alternative
/// was changing every existing call site that only ever printed or substring-matched a warning, and
/// that churn would have swamped the actual change in noise while adding no information.
///
/// The cost is bounded because the one place losing the code would matter — the assessment report,
/// where `compare` diffs by code — does **not** go through this coercion: `load_grammar_coded`
/// keeps `Warning` values intact for exactly that path. If a future change carries codes all the
/// way through `load_grammar` too, this impl stops paying for itself and should be removed along
/// with the `Vec<String>` shape it exists to preserve.
impl Deref for Warning {
    type Target = str;

    fn deref(&self) -> &str {
        &self.message
    }
}

impl From<Warning> for String {
    fn from(w: Warning) -> String {
        w.message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_exactly_the_message() {
        let w = Warning::new("fwdata.dangling-reference", "phoneme \"00...\" does not resolve");
        assert_eq!(w.to_string(), "phoneme \"00...\" does not resolve");
    }

    #[test]
    fn deref_supports_str_methods_like_contains() {
        let w = Warning::new("fwdata.dangling-reference", "dangling reference to Foo abc-123");
        assert!(w.contains("abc-123"));
    }

    #[test]
    fn code_is_independent_of_message_reword() {
        let original = Warning::new("fwdata.dangling-reference", "old wording of the same fact");
        let reworded = Warning::new("fwdata.dangling-reference", "new wording, same situation");
        assert_eq!(original.code, reworded.code);
        assert_ne!(original.message, reworded.message);
    }

    #[test]
    fn different_situations_get_different_codes() {
        let dangling = Warning::new("fwdata.dangling-reference", "X does not resolve");
        let unexpected_class = Warning::new("fwdata.unexpected-class", "X has unexpected class Y");
        assert_ne!(dangling.code, unexpected_class.code);
    }
}
