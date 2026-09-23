//! Unicode NFD normalization, matching C# `CharacterDefinitionTable`'s
//! `Normalize(NormalizationForm.FormD)` (CharacterDefinitionTable.cs:59,112,278). The plan's
//! layer-1 NFD parity gate (§8) diffs this against .NET's output over every corpus word and
//! grammar string rep to catch Unicode-version skew before it becomes a parse difference.

use unicode_normalization::UnicodeNormalization;

/// Canonical decomposition (NFD) of `s`, matching .NET `String.Normalize(FormD)`.
pub fn nfd(s: &str) -> String {
    s.nfd().collect()
}

/// Whether `s` is already in NFD, matching .NET `String.IsNormalized(FormD)`.
///
/// Used by `crate::segment::segment` to replicate the C# `GetShapeNodes` error-position
/// remap: the remap only fires when the input string was *not* already NFD.
pub fn is_nfd(s: &str) -> bool {
    unicode_normalization::is_nfd(s)
}

#[cfg(test)]
mod tests;
