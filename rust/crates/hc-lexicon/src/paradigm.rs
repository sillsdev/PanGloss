//! Disambiguating-form generation (add-to-dictionary design doc, Sub-project 1, component 3).
//!
//! For a user-typed shape and a set of candidate inflection classes, synthesize a small set of
//! forms per class (bare stem + a few inflected forms) so the user can compare against text
//! they've actually seen and pick the right class. Compute-only: no grammar rebuild, no mutation —
//! every form comes from [`hc_parse::Morpher::synthesize_guessed_stem`], the same
//! `AllomorphId::GUESSED` fabrication `hc_parse::guess::lexical_guess` uses for unparsed words.

use std::collections::{HashMap, HashSet};

use hc_featstruct::FeatureValue;
use hc_grammar::model::{AffixProcessRuleDef, Grammar, MRuleId, MorphRuleDef, MprSet};
use hc_parse::Morpher;
use serde::Serialize;

use crate::classes::{resolve_entry_by_xml_key, ClassCandidate};

/// The generated forms for one candidate class, per [`disambiguating_forms`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClassForms {
    /// The [`ClassCandidate::key`] these forms belong to.
    pub class_key: String,
    /// Bare stem first (when synthesizable), then up to `max_per_class - 1` inflected forms
    /// ranked by how well they disambiguate this class from the others being compared.
    pub forms: Vec<String>,
}

/// v1 algorithm (add-to-dictionary design doc):
/// 1. For each candidate class, fabricate the stem word (`AllomorphId::GUESSED`, the class's own
///    `syn_fs`/`mpr`, taken from its exemplar entry) and synthesize the bare-stem form.
/// 2. For each inflectional (`AffixProcess`) rule in the grammar whose MPR requirements are
///    compatible with the class's `MprSet` and whose own required syntactic FS's POS component (if
///    any) overlaps the class's POS, fabricate stem+that-one-rule and synthesize.
/// 3. Rank affixes by how well they disambiguate: forms unique to one class among those compared
///    sort first; forms that recur across multiple classes (so seeing them tells the user nothing)
///    sort last. Cap at `max_per_class` total forms per class (bare stem counts toward the cap).
/// 4. If a class yields nothing beyond its bare stem, its `forms` is just that bare stem — an
///    honest empty paradigm, never a fabricated string outside the synthesis pipeline.
pub fn disambiguating_forms(
    grammar: &Grammar,
    morpher: &Morpher<'_>,
    shape: &str,
    candidates: &[ClassCandidate],
    max_per_class: usize,
) -> Vec<ClassForms> {
    let cap = max_per_class.max(1);

    struct Working<'a> {
        candidate: &'a ClassCandidate,
        bare: Vec<String>,
        /// `(mrule document index, forms not already covered by the bare stem)`.
        rule_forms: Vec<(usize, Vec<String>)>,
    }

    let mut working: Vec<Working> = Vec::with_capacity(candidates.len());
    for c in candidates {
        let exemplar = resolve_entry_by_xml_key(grammar, &c.exemplar_xml_key);
        let Some(le) = exemplar else {
            // Unresolvable exemplar (stale candidate against a since-changed grammar) -- honest
            // empty paradigm, no forms at all.
            working.push(Working {
                candidate: c,
                bare: Vec::new(),
                rule_forms: Vec::new(),
            });
            continue;
        };

        let bare = morpher.synthesize_guessed_stem(le, shape, None);

        let entry = &grammar.entries[le.0 as usize];
        let class_mpr = entry.mpr;
        let class_pos_bits = match grammar.fs_interner.get(entry.syn_fs).get(grammar.syn_features.pos) {
            Some(FeatureValue::Symbolic(bits)) => Some(*bits),
            _ => None,
        };

        let mut rule_forms = Vec::new();
        for (ridx, mrule) in grammar.mrules.iter().enumerate() {
            let MorphRuleDef::AffixProcess(def) = mrule else {
                continue; // Compounding/Realizational rules are out of scope for v1.
            };
            if !rule_applies_to_class(grammar, def, class_mpr, class_pos_bits) {
                continue;
            }
            let mrid = MRuleId(ridx as u32);
            let forms = morpher.synthesize_guessed_stem(le, shape, Some(mrid));
            let distinct: Vec<String> = forms.into_iter().filter(|f| !bare.contains(f)).collect();
            if !distinct.is_empty() {
                rule_forms.push((ridx, distinct));
            }
        }

        working.push(Working {
            candidate: c,
            bare,
            rule_forms,
        });
    }

    // Cross-class occurrence count: how many DIFFERENT classes' rule_forms contain a given
    // surface string. A form unique to one class is maximally useful for disambiguation.
    let mut occurrence: HashMap<String, usize> = HashMap::new();
    for w in &working {
        let mut seen: HashSet<&str> = HashSet::new();
        for (_, forms) in &w.rule_forms {
            for f in forms {
                if seen.insert(f.as_str()) {
                    *occurrence.entry(f.clone()).or_insert(0) += 1;
                }
            }
        }
    }

    let mut out = Vec::with_capacity(working.len());
    for w in working {
        let mut forms: Vec<String> = Vec::new();
        if let Some(first) = w.bare.first() {
            forms.push(first.clone());
        }

        let mut scored: Vec<(usize, usize, String)> = Vec::new();
        for (ridx, fs) in &w.rule_forms {
            for f in fs {
                let occ = occurrence.get(f).copied().unwrap_or(0);
                scored.push((occ, *ridx, f.clone()));
            }
        }
        // Fewer cross-class occurrences first (more disambiguating); ties broken by document
        // order (`ridx`) then the string itself, for determinism.
        scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

        for (_, _, f) in scored {
            if forms.len() >= cap {
                break;
            }
            if !forms.contains(&f) {
                forms.push(f);
            }
        }

        out.push(ClassForms {
            class_key: w.candidate.key.clone(),
            forms,
        });
    }
    out
}

/// Whether `def` (an `AffixProcess` morphological rule) is a plausible inflectional candidate for
/// a class with the given MPR set / POS bits. MPR: at least one of the rule's subrule allomorphs
/// must accept `class_mpr` (`Grammar::mpr_group_ok`, the same group-aware gate `hc-rules`' own
/// synthesis path uses). POS: the rule's OWN `required_syn_fs` (subrules carry no POS component,
/// per `AffixAllomorphDef`'s doc — "no POS at this level in C#") must either declare no POS
/// constraint at all, or overlap the class's POS bit. This is a coarse prefilter, not a full
/// unifier — the real gate is the synthesis pipeline `synthesize_guessed_stem` runs; a rule that
/// passes here but is truly inapplicable (e.g. a head-feature mismatch this prefilter can't see)
/// simply yields no forms and is silently dropped from the results.
fn rule_applies_to_class(
    grammar: &Grammar,
    def: &AffixProcessRuleDef,
    class_mpr: MprSet,
    class_pos_bits: Option<hc_featstruct::SymbolBits>,
) -> bool {
    let fs = grammar.fs_interner.get(def.required_syn_fs);
    if let Some(FeatureValue::Symbolic(rule_bits)) = fs.get(grammar.syn_features.pos) {
        match class_pos_bits {
            Some(cb) if (cb.raw() & rule_bits.raw()) != 0 => {}
            _ => return false,
        }
    }
    def.allomorphs
        .iter()
        .any(|allo| grammar.mpr_group_ok(allo.required_mpr, allo.excluded_mpr, class_mpr))
}
