//! Natural-phrases N1 (`docs/natural-phrases-plan.md` N1): sidecar mapping config from raw
//! grammar-author gloss strings to `crate::ir`'s typed feature vocabulary.
//!
//! **File format decision (orchestrator, N1):** a hand-rolled, restricted TOML *subset* — not
//! the `toml` crate. The workspace is deliberately zero-external-deps
//! (`docs/natural-phrases-plan.md`'s non-negotiable constraint 3), and the actual format need
//! only express: full-line comments (`#`), exactly one `[features]` section header, and
//! `key = "value"` lines where `key` is either a bare identifier (letters/digits/`_`/`-`) or a
//! double-quoted string (for keys containing characters a bare identifier can't hold, notably
//! the `.` in gloss strings like `"poss.1s"`), and `value` is always a double-quoted string. No
//! multi-line values, no escape sequences, no other section headers, no nested tables, no
//! arrays — pulling in a real TOML parser (or hand-rolling one) for that generality bought
//! nothing this crate needs. Anything outside this subset is a load-time error carrying the
//! 1-indexed source line number, so a typo in a sidecar file (a stray section, a bad quote, an
//! unrecognized `Feature:Value` pair) fails loudly instead of silently becoming an unmapped
//! gloss (which degrades *gracefully but silently* to `extras` — exactly the failure mode a
//! line-numbered load error is meant to prevent for sidecar typos specifically; see
//! `RealizeMap::parse`).
//!
//! N2 (`docs/natural-phrases-plan.md` N2) reuses this exact subset for `crate::table::
//! TableRealizer`'s two embedded assets (`templates.toml`'s `[cells]`, `lexicon.toml`'s
//! `[plural_exceptions]`) rather than writing a third parser — `parse_section` is the
//! generalized (any single section name) core `RealizeMap::parse` itself now sits on top of.
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fmt;

use crate::ir::{CaseRole, Num, Poss};

/// One parsed `"Feature:Value"` (or bare `"Ignore"`) right-hand side — the common currency
/// between a sidecar TOML value and a morpheme's `realize` property (`docs/natural-phrases-
/// plan.md` N1's mapping-source priority chain: property, then sidecar, share this parser).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeatureAssignment {
    Num(Num),
    Poss(Poss),
    Case(CaseRole),
    /// Explicitly mapped to nothing (e.g. a purely morphophonological gloss) — drops the token
    /// from `crate::ir::GlossIr::extras` entirely, rather than letting it fall through as an
    /// unmapped fallback string.
    Ignore,
}

/// A sidecar-file or `realize`-property load/parse error, always carrying the 1-indexed source
/// line number it was found on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for MapError {}

/// A loaded gloss-string -> feature-assignment lookup table, built from a sidecar TOML-subset
/// file (`samples/data/<grammar>-realize.toml`) via `RealizeMap::parse`, or empty when a
/// grammar has no sidecar (`RealizeMap::empty()` — the sena no-sidecar path).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RealizeMap {
    entries: HashMap<String, FeatureAssignment>,
}

impl RealizeMap {
    /// A map with no entries — every gloss falls through to `extras`. This is the correct
    /// no-sidecar path (no `samples/data/sena-realize.toml` exists, by design), not a degraded
    /// fallback.
    pub fn empty() -> Self {
        RealizeMap {
            entries: HashMap::new(),
        }
    }

    /// Look up a raw gloss string's mapped feature assignment, if any.
    pub fn lookup(&self, gloss: &str) -> Option<FeatureAssignment> {
        self.entries.get(gloss).cloned()
    }

    /// Parse a sidecar file's text (the restricted TOML subset documented at module level) into
    /// a `RealizeMap`. Fails loudly, with a 1-indexed line number, on anything outside the
    /// subset or on an unknown feature name/value — sidecar typos must not silently become
    /// `extras`.
    pub fn parse(text: &str) -> Result<RealizeMap, MapError> {
        let mut entries: HashMap<String, FeatureAssignment> = HashMap::new();

        for (line_no, key, raw_value) in parse_section(text, "features")? {
            let assignment = parse_feature_assignment(&raw_value).map_err(|message| MapError {
                line: line_no,
                message: format!("{key:?}: {message}"),
            })?;

            // Last entry for a given key in the same file wins (a plain overwrite, no
            // conflict-to-extras behavior here -- that rule is about multiple *tokens* in one
            // parse mapping to the same IR feature at to_ir time, not about duplicate sidecar
            // keys, which is authoring config, not runtime data).
            entries.insert(key, assignment);
        }

        Ok(RealizeMap { entries })
    }

    /// Insert or overwrite one gloss key's feature assignment. Crate-private: `entries` is a
    /// private field of this module, so same-crate map builders that aren't `parse` (namely
    /// `crate::infer::infer_english`) need this narrow door rather than reaching into the
    /// field directly.
    pub(crate) fn insert(&mut self, gloss: String, assignment: FeatureAssignment) {
        self.entries.insert(gloss, assignment);
    }

    /// Merge `overrides` into `self`, per gloss key: every key present in `overrides` replaces
    /// (or adds) that key's assignment here; keys only in `self` are left untouched. This is the
    /// small additive method the `pg-wasm` precedence rule needs ("inferred map is the base; a
    /// sidecar `realize.toml`, when present, overrides per gloss key") — `self` is expected to be
    /// an `crate::infer::infer_english` base map and `overrides` a parsed sidecar, but this
    /// method itself is agnostic to which side is which; it just merges, sidecar-wins ordering is
    /// the caller's responsibility (call it as `base.extend_overriding(sidecar)`, not the other
    /// way around).
    pub fn extend_overriding(&mut self, overrides: RealizeMap) {
        self.entries.extend(overrides.entries);
    }
}

/// Parse one named section (`"[{section}]"`) of the restricted TOML subset documented at module
/// level into its `key = "value"` entries, each tagged with its 1-indexed source line number.
///
/// Generalizes `RealizeMap::parse`'s original single-purpose `[features]` reader so
/// `docs/natural-phrases-plan.md` N2's `TableRealizer` asset loaders (`templates.toml`'s
/// `[cells]`, `lexicon.toml`'s `[plural_exceptions]`) can reuse the exact same subset and error
/// behavior instead of a third hand-rolled parser — same restrictions (full-line `#` comments,
/// exactly one matching section header, bare-or-quoted keys, always-quoted values), same
/// line-numbered failure mode for anything outside the subset. The line number travels with each
/// entry so a caller whose *value* needs further validation (`RealizeMap::parse`'s
/// `Feature:Value` parsing, `TableRealizer`'s cell-key/template-slot validation) can still report
/// a precise line on that later failure, not just on the raw key/value shape.
pub(crate) fn parse_section(
    text: &str,
    section: &str,
) -> Result<Vec<(usize, String, String)>, MapError> {
    let header = format!("[{section}]");
    let mut entries: Vec<(usize, String, String)> = Vec::new();
    let mut seen_section = false;

    for (idx, raw_line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line == header {
            if seen_section {
                return Err(MapError {
                    line: line_no,
                    message: format!("duplicate {header} section header"),
                });
            }
            seen_section = true;
            continue;
        }

        if line.starts_with('[') {
            return Err(MapError {
                line: line_no,
                message: format!("unsupported section header {line:?}; only {header} is supported"),
            });
        }

        if !seen_section {
            return Err(MapError {
                line: line_no,
                message: format!("key/value line before the {header} section header"),
            });
        }

        let eq_pos = line.find('=').ok_or_else(|| MapError {
            line: line_no,
            message: format!("malformed line (expected key = \"value\"): {line:?}"),
        })?;
        let (key_part, val_part) = line.split_at(eq_pos);
        let val_part = &val_part[1..]; // skip the '='

        let key = parse_key(key_part.trim(), line_no)?;
        let value = parse_quoted_value(val_part.trim(), line_no)?;
        entries.push((line_no, key, value));
    }

    Ok(entries)
}

/// Parse a `key` token: either a bare identifier (`[A-Za-z0-9_-]+`) or a double-quoted string
/// (for keys needing characters a bare identifier can't hold, e.g. `.`).
fn parse_key(raw: &str, line_no: usize) -> Result<String, MapError> {
    if let Some(inner) = raw.strip_prefix('"') {
        match inner.strip_suffix('"') {
            Some(unquoted) if !unquoted.contains('"') => Ok(unquoted.to_string()),
            _ => Err(MapError {
                line: line_no,
                message: format!("unterminated or malformed quoted key: {raw:?}"),
            }),
        }
    } else if !raw.is_empty()
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Ok(raw.to_string())
    } else {
        Err(MapError {
            line: line_no,
            message: format!(
                "invalid bare key {raw:?}; use double quotes for keys with characters other \
                 than letters/digits/'_'/'-' (e.g. \"poss.1s\")"
            ),
        })
    }
}

/// Parse a `value` token: must be a double-quoted string (no bare, unquoted values).
fn parse_quoted_value(raw: &str, line_no: usize) -> Result<String, MapError> {
    let inner = raw.strip_prefix('"').ok_or_else(|| MapError {
        line: line_no,
        message: format!("value must be a double-quoted string: {raw:?}"),
    })?;
    match inner.strip_suffix('"') {
        Some(unquoted) if !unquoted.contains('"') => Ok(unquoted.to_string()),
        _ => Err(MapError {
            line: line_no,
            message: format!("unterminated or malformed quoted value: {raw:?}"),
        }),
    }
}

/// Parse a `"Feature:Value"` string (or the bare literal `"Ignore"`) into a
/// `FeatureAssignment`. Shared between sidecar-file loading (`RealizeMap::parse`, where a
/// failure is a line-numbered load error) and morpheme `realize` properties (`crate::ir::to_ir`,
/// where a failure degrades to unmapped -- "parse errors in properties degrade to unmapped,
/// they are grammar-author data", per `docs/natural-phrases-plan.md` N1's mapping priority
/// order).
pub(crate) fn parse_feature_assignment(raw: &str) -> Result<FeatureAssignment, String> {
    if raw == "Ignore" {
        return Ok(FeatureAssignment::Ignore);
    }
    let (feature, value) = raw.split_once(':').ok_or_else(|| {
        format!("expected \"Feature:Value\" or the literal \"Ignore\", got {raw:?}")
    })?;
    match feature {
        "Num" => Num::parse(value)
            .map(FeatureAssignment::Num)
            .ok_or_else(|| format!("unknown Num value {value:?} (expected Unspec/Sg/Pl)")),
        "Poss" => Poss::parse(value)
            .map(FeatureAssignment::Poss)
            .ok_or_else(|| {
                format!(
                    "unknown Poss value {value:?} (expected None/P1Sg/P1Pl/P2SgM/P2SgF/P2Pl/\
                 P3SgM/P3SgF/P3Pl)"
                )
            }),
        "Case" => CaseRole::parse(value)
            .map(FeatureAssignment::Case)
            .ok_or_else(|| format!("unknown Case value {value:?} (expected None/Loc/Abl/All)")),
        other => Err(format!(
            "unknown feature name {other:?} (expected Num/Poss/Case, or the literal \"Ignore\")"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{CaseRole, Num, Poss};

    #[test]
    fn parses_amharic_sidecar_syntax_shape() {
        let text = r#"
# a comment
[features]
"pl" = "Num:Pl"
"poss.1s" = "Poss:P1Sg"
"at" = "Case:Loc"
bare_key = "Ignore"
"#;
        let map = RealizeMap::parse(text).expect("should parse");
        assert_eq!(map.lookup("pl"), Some(FeatureAssignment::Num(Num::Pl)));
        assert_eq!(
            map.lookup("poss.1s"),
            Some(FeatureAssignment::Poss(Poss::P1Sg))
        );
        assert_eq!(
            map.lookup("at"),
            Some(FeatureAssignment::Case(CaseRole::Loc))
        );
        assert_eq!(map.lookup("bare_key"), Some(FeatureAssignment::Ignore));
        assert_eq!(map.lookup("nope"), None);
    }

    #[test]
    fn rejects_unknown_feature_name_with_line_number() {
        let text = "[features]\n\"x\" = \"Bogus:Sg\"\n";
        let err = RealizeMap::parse(text).unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.message.contains("Bogus"), "{}", err.message);
    }

    #[test]
    fn rejects_unknown_feature_value_with_line_number() {
        let text = "[features]\n\"x\" = \"Num:Triple\"\n";
        let err = RealizeMap::parse(text).unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.message.contains("Triple"), "{}", err.message);
    }

    #[test]
    fn rejects_junk_line_with_line_number() {
        let text = "[features]\nthis is not valid\n";
        let err = RealizeMap::parse(text).unwrap_err();
        assert_eq!(err.line, 2);
    }

    #[test]
    fn rejects_unquoted_value() {
        let text = "[features]\n\"x\" = Num:Pl\n";
        let err = RealizeMap::parse(text).unwrap_err();
        assert_eq!(err.line, 2);
    }

    #[test]
    fn rejects_bare_key_needing_quotes() {
        let text = "[features]\npl.foo = \"Num:Pl\"\n";
        let err = RealizeMap::parse(text).unwrap_err();
        assert_eq!(err.line, 2);
    }

    #[test]
    fn rejects_second_section_header() {
        let text = "[features]\n[other]\n";
        let err = RealizeMap::parse(text).unwrap_err();
        assert_eq!(err.line, 2);
    }

    #[test]
    fn rejects_duplicate_features_section() {
        let text = "[features]\n[features]\n";
        let err = RealizeMap::parse(text).unwrap_err();
        assert_eq!(err.line, 2);
    }

    #[test]
    fn rejects_entry_before_section_header() {
        let text = "\"pl\" = \"Num:Pl\"\n[features]\n";
        let err = RealizeMap::parse(text).unwrap_err();
        assert_eq!(err.line, 1);
    }

    #[test]
    fn empty_map_has_no_entries() {
        let map = RealizeMap::empty();
        assert_eq!(map.lookup("anything"), None);
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let text = "\n# comment\n\n[features]\n\n# another\n\"pl\" = \"Num:Pl\"\n\n";
        let map = RealizeMap::parse(text).expect("should parse");
        assert_eq!(map.lookup("pl"), Some(FeatureAssignment::Num(Num::Pl)));
    }

    #[test]
    fn extend_overriding_replaces_shared_keys_adds_new_keys_keeps_untouched_keys() {
        let mut base =
            RealizeMap::parse("[features]\n\"pl\" = \"Num:Pl\"\n\"loc\" = \"Case:Loc\"\n")
                .expect("valid base");
        let overrides =
            RealizeMap::parse("[features]\n\"loc\" = \"Case:Abl\"\n\"abl\" = \"Case:Abl\"\n")
                .expect("valid overrides");

        base.extend_overriding(overrides);

        // Untouched key survives.
        assert_eq!(base.lookup("pl"), Some(FeatureAssignment::Num(Num::Pl)));
        // Shared key: override wins.
        assert_eq!(
            base.lookup("loc"),
            Some(FeatureAssignment::Case(CaseRole::Abl))
        );
        // New key from overrides is added.
        assert_eq!(
            base.lookup("abl"),
            Some(FeatureAssignment::Case(CaseRole::Abl))
        );
    }
}
