//! Reduplication peel (plan D6, `docs/fst-plan/foma-fst-plan.md` P2): a fresh port of
//! `hc-hybrid/src/proposers.rs::ReduplicationProposer` (`ReduplicationProposer.cs`'s four scan
//! kinds — prefix-copy, suffix-copy, separator+tail-copy, separator+suffix-peel), with the
//! recursion target swapped from the trie-based bare walker to the caller's foma proposer (plan §2:
//! "Redup peel is proposer-agnostic ... only needs a `fn(&str) -> Vec<Candidate>` to recurse
//! residuals into").
//!
//! Reuses [`crate::emit`]'s own port of `hc-hybrid/src/token.rs`'s `MorphOp`/`ClassifyAffix`
//! (`Role`/`classify_affix`, made `pub(crate)` there for exactly this reason) plus its
//! `owning_morpheme`/`surface_table` helpers, rather than re-porting the same classification logic
//! a second time in this module — both the emitter and this peel need the identical affix-role
//! answer, and `hc-hybrid` itself is being sunset (plan D8), so neither may depend on it.

use hc_grammar::chardef::{CharDefId, CharDefTable};
use hc_grammar::model::{Grammar, MRuleId, MorphRuleDef, OutputAction};
use hc_shape::{NodeKind, Shape};

use crate::emit::{classify_affix, owning_morpheme, surface_table, Role};
use crate::tags::Candidate;

/// C# `ReduplicationProposer.IsReduplication` (`ReduplicationProposer.cs:233-247`): **only** an
/// `AffixProcessRule` is ever checked — a `RealizationalAffixProcessRule` is never considered for
/// reduplication classification at all, even if one of its allomorphs would classify as
/// `Role::Reduplication` — a real, faithfully-preserved C# quirk (ported from
/// `hc-hybrid/src/proposers.rs::is_reduplication_rule` verbatim, `.any()` over EVERY allomorph,
/// unlike `crate::emit::rule_role`'s "first allomorph only" — a deliberately different aggregation
/// for a deliberately different question: emit's `rule_role` asks "how does this rule's PRIMARY
/// allomorph route in the morphotactic chain", this asks "does ANY allomorph of this rule
/// reduplicate").
fn is_reduplication_rule(def: &MorphRuleDef) -> bool {
    match def {
        MorphRuleDef::AffixProcess(d) => d
            .allomorphs
            .iter()
            .any(|a| classify_affix(&a.rhs) == Role::Reduplication),
        _ => false,
    }
}

/// C# `ReduplicationProposer.RenderSurfaceOnly` (`ReduplicationProposer.cs:113-130`): render only
/// the Segment-kind nodes of `shape` through `table`'s FIRST representation, `None` the instant any
/// Segment node has no representation (the underlying representation may carry boundary characters
/// that must not appear in the rendered surface text). Ported from
/// `hc-hybrid/src/proposers.rs::render_surface_only` verbatim.
fn render_surface_only(table: &CharDefTable, shape: &Shape) -> Option<String> {
    let mut out = String::new();
    for (_, kind, cd, _flags) in shape.interior() {
        if kind != NodeKind::Segment {
            continue;
        }
        match table.get(CharDefId(cd)).representations().first() {
            Some(rep) if !rep.is_empty() => out.push_str(rep),
            _ => return None,
        }
    }
    Some(out)
}

/// Grammar-only rule discovery for the redup peel — ported from
/// `hc-hybrid/src/proposers.rs::ReduplicationProposer`'s fields + `new` (`proposers.rs:90-139`).
/// Built once per grammar (identical every call, unlike the ephemeral `Trie`/beam-work params the
/// original's constructor also took: this port needs neither, since residuals recurse through the
/// caller's `propose` closure instead of a shared trie/walker).
pub struct ReduplicationPeeler {
    /// `AffixProcessRule`s whose RHS classifies as reduplication, in grammar document order
    /// (stratum order, then `stratum.mrules` order).
    redup_rules: Vec<MRuleId>,
    /// `(suffix surface text, owning rule)` pairs for every ordinary SUFFIX-classified allomorph in
    /// the grammar (`AffixProcess` or `Realizational`), document order — the separator+suffix-peel
    /// scan's search list.
    suffix_surfaces: Vec<(String, MRuleId)>,
}

impl ReduplicationPeeler {
    pub fn new(g: &Grammar) -> Self {
        let table = surface_table(g);
        let mut redup_rules = Vec::new();
        let mut suffix_surfaces = Vec::new();
        for stratum in &g.strata {
            for &mrule_id in &stratum.mrules {
                let def = &g.mrules[mrule_id.0 as usize];
                if is_reduplication_rule(def) {
                    redup_rules.push(mrule_id);
                    continue;
                }
                let Some(allomorphs) = def.affix_allomorphs() else {
                    continue; // CompoundingRule: not a MorphemicMorphologicalRule in C# either.
                };
                for allomorph in allomorphs {
                    if classify_affix(&allomorph.rhs) != Role::Suffix {
                        continue;
                    }
                    let Some(insert_shape) = allomorph.rhs.iter().find_map(|a| match a {
                        OutputAction::InsertSegments { shape, .. } => Some(shape),
                        _ => None,
                    }) else {
                        continue;
                    };
                    if let Some(surface_text) = render_surface_only(table, &insert_shape.shape) {
                        if !surface_text.is_empty() {
                            suffix_surfaces.push((surface_text, mrule_id));
                        }
                    }
                }
            }
        }
        ReduplicationPeeler {
            redup_rules,
            suffix_surfaces,
        }
    }

    /// Whether this grammar has any reduplication rule at all — [`Self::peel_candidates`] already
    /// early-returns empty when this is `false` (mirroring the original's own early-out), exposed
    /// separately so a caller (e.g. [`crate::composite::FomaAnalyzer`]) can skip building a
    /// `propose` closure entirely for a no-redup grammar like Sena.
    pub fn has_redup_rules(&self) -> bool {
        !self.redup_rules.is_empty()
    }

    /// C# `ReduplicationProposer.AnalyzeWord` (`ReduplicationProposer.cs:134-209`), recursion target
    /// swapped to the caller's `propose` closure (plan D6) instead of the trie-based bare walker.
    /// Operates on `char`s (Rust's `char` == a Unicode scalar value; every reference grammar's
    /// alphabet is BMP-only, where C#'s UTF-16 `string.Length`/`Substring` indexing and a
    /// `Vec<char>`'s indexing coincide exactly), so this never panics on a non-ASCII grammar's
    /// multi-byte UTF-8 word.
    pub fn peel_candidates(
        &self,
        g: &Grammar,
        word: &str,
        propose: &mut dyn FnMut(&str) -> Vec<Candidate>,
    ) -> Vec<Candidate> {
        let mut out = Vec::new();
        if self.redup_rules.is_empty() {
            return out;
        }
        let chars: Vec<char> = word.chars().collect();
        let len = chars.len();
        let max_copy_len = len / 2;

        for l in 1..=max_copy_len {
            // Prefix copy: chars[0..l] repeats immediately (chars[l..2l]) -- strip it.
            if chars[0..l] == chars[l..2 * l] {
                let residual: String = chars[l..len].iter().collect();
                self.propose_for_residual(g, &residual, None, propose, &mut out);
            }
            // Suffix copy: the last l chars repeat the l chars before them -- strip the trailing copy.
            if chars[len - l..len] == chars[len - 2 * l..len - l] {
                let residual: String = chars[0..len - l].iter().collect();
                self.propose_for_residual(g, &residual, None, propose, &mut out);
            }
        }

        // Separator + tail copy, and separator + suffix-peel + tail copy.
        for sep_pos in 1..len.saturating_sub(1) {
            let before = &chars[0..sep_pos];
            let copy = &chars[sep_pos + 1..len];
            if copy.is_empty() {
                continue;
            }
            if before.len() >= copy.len() && before[before.len() - copy.len()..] == *copy {
                let residual: String = before.iter().collect();
                self.propose_for_residual(g, &residual, None, propose, &mut out);
                continue; // plain tail matched -- do not also try the suffix-peel fallback.
            }
            for (suffix_text, suffix_rule) in &self.suffix_surfaces {
                let suffix_chars: Vec<char> = suffix_text.chars().collect();
                if suffix_chars.len() > copy.len() {
                    continue;
                }
                if copy[copy.len() - suffix_chars.len()..] != suffix_chars[..] {
                    continue;
                }
                let stripped_len = copy.len() - suffix_chars.len();
                if stripped_len == 0 {
                    continue;
                }
                let stripped_copy = &copy[..stripped_len];
                if before.len() >= stripped_copy.len()
                    && before[before.len() - stripped_copy.len()..] == *stripped_copy
                {
                    let residual: String = before.iter().collect();
                    self.propose_for_residual(
                        g,
                        &residual,
                        Some(*suffix_rule),
                        propose,
                        &mut out,
                    );
                }
            }
        }
        out
    }

    /// C# `ProposeForResidual` (`ReduplicationProposer.cs:211-231`): recurse `residual` through the
    /// caller's proposer, then wrap every returned base candidate with the reduplication morpheme
    /// (and, for the separator+suffix-peel path, the peeled suffix morpheme afterward) --
    /// `root_index` is unchanged (the added morphemes are appended after the base's own morphemes,
    /// matching HC's `root … RED suffix` application order).
    fn propose_for_residual(
        &self,
        g: &Grammar,
        residual: &str,
        extra_suffix: Option<MRuleId>,
        propose: &mut dyn FnMut(&str) -> Vec<Candidate>,
        out: &mut Vec<Candidate>,
    ) {
        let base_candidates = propose(residual);
        for base in &base_candidates {
            for &redup in &self.redup_rules {
                let mut morphemes = base.morphemes.clone();
                morphemes.push(owning_morpheme(g, redup));
                if let Some(suf) = extra_suffix {
                    morphemes.push(owning_morpheme(g, suf));
                }
                out.push(Candidate {
                    morphemes,
                    root_index: base.root_index,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_grammar::model::Grammar;

    fn sample_path(name: &str) -> Option<std::path::PathBuf> {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("../../../samples/data").join(name);
        path.exists().then_some(path)
    }

    fn load_indonesian() -> Option<Grammar> {
        let path = sample_path("indonesian-hc.xml")?;
        let xml = std::fs::read_to_string(&path).expect("read grammar");
        Some(hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
    }

    fn load_sena() -> Option<Grammar> {
        let path = sample_path("sena-hc.xml")?;
        let xml = std::fs::read_to_string(&path).expect("read grammar");
        Some(hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
    }

    /// Sena has no reduplication rules at all -- the peeler must be a true no-op (empty `redup_rules`,
    /// `peel_candidates` short-circuits to empty for any word without ever calling `propose`).
    #[test]
    fn sena_has_no_redup_rules() {
        let Some(g) = load_sena() else {
            eprintln!("skipping: sena-hc.xml not present on disk");
            return;
        };
        let peeler = ReduplicationPeeler::new(&g);
        assert!(!peeler.has_redup_rules());
        let mut calls = 0usize;
        let mut propose = |_: &str| {
            calls += 1;
            Vec::new()
        };
        let out = peeler.peel_candidates(&g, "mbali", &mut propose);
        assert!(out.is_empty());
        assert_eq!(calls, 0, "no-redup grammar must never invoke the propose closure");
    }

    /// Indonesian's redup rules recover "membagi-bagi" (a known corpus word) when the residual
    /// "membagi" is handed a stub proposer that returns one fixed base candidate.
    #[test]
    fn reduplication_recovers_known_corpus_word() {
        let Some(g) = load_indonesian() else {
            eprintln!("skipping: indonesian-hc.xml not present on disk");
            return;
        };
        let peeler = ReduplicationPeeler::new(&g);
        assert!(peeler.has_redup_rules(), "Indonesian must have at least one redup rule");

        let root = g.entries[0].morpheme;
        let mut seen_residuals: Vec<String> = Vec::new();
        let mut propose = |residual: &str| {
            seen_residuals.push(residual.to_string());
            if residual == "membagi" {
                vec![Candidate {
                    morphemes: vec![root],
                    root_index: 0,
                }]
            } else {
                Vec::new()
            }
        };
        let out = peeler.peel_candidates(&g, "membagi-bagi", &mut propose);
        assert!(
            !out.is_empty(),
            "expected at least one reduplication candidate for membagi-bagi"
        );
        assert!(seen_residuals.iter().any(|r| r == "membagi"));
        for c in &out {
            assert_eq!(c.root_index, 0);
            assert!(c.morphemes.len() >= 2, "expected root + at least the redup morpheme");
        }
    }
}
