//! A single importer/snapshot-validation warning: a stable short `code` (which `compare` diffs by, never by `message` prose) alongside its existing human-readable `message`; `Display`/`Deref<Target = str>` keep every pre-existing bare-`String`-shaped caller compiling unchanged.
use std::fmt;
use std::ops::Deref;

/// One importer or snapshot-validation warning. See the module doc for the `code`/`message` contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// Short, stable, dotted identifier naming the situation (e.g. `"fwdata.dangling-reference"`); must stay the same across a `message` reword, and two structurally different situations must never share one.
    pub code: &'static str,
    /// Human-readable prose, free to reword at any time without being a diff-relevant change.
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

/// Lets existing `&str`-shaped callers keep working against `message`.
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
        let w = Warning::new(
            "fwdata.dangling-reference",
            "phoneme \"00...\" does not resolve",
        );
        assert_eq!(w.to_string(), "phoneme \"00...\" does not resolve");
    }

    #[test]
    fn deref_supports_str_methods_like_contains() {
        let w = Warning::new(
            "fwdata.dangling-reference",
            "dangling reference to Foo abc-123",
        );
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
