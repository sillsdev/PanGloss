//! Whether a deletion can empty a morph before a later stratum adds an affix. Rust HC, like C#, keeps a morph whose segments were all deleted as an empty annotation, and the next affix copy orders it after the copied stem (`R0+PR0+SU1` for prefix `PR0` emptied in an earlier stratum than suffix `SU1`); the surface skeleton tags morphs in their lexical order, so it misses that analysis. The test is conservative: any literal whose segments the rules of its own or a later stratum, before the last one that adds morphology, could all delete counts, whatever its role or context, and so does any segment a rule could first change into one they delete.

use std::collections::HashSet;

use pg_grammar::chardef::{CharDefKind, CharDefTable};
use pg_grammar::model::{
    Grammar, MRuleId, MorphRuleDef, OutputAction, PartRef, PatternNode, PhonRuleDef, RewriteRuleDef,
};
use pg_shape::{NodeKind, Shape, NO_CHAR_DEF};

/// The segments the rules of some strata can delete.
#[derive(Clone)]
enum Deletable {
    Nothing,
    Segments(HashSet<u32>),
    Any,
}

impl Deletable {
    /// Whether a literal holding these segments (`None` for one with no single spelling) can lose all of them.
    fn empties(&self, segments: &[Option<u32>]) -> bool {
        if segments.is_empty() {
            return false;
        }
        match self {
            Deletable::Nothing => false,
            Deletable::Any => true,
            Deletable::Segments(set) => segments
                .iter()
                .all(|segment| segment.is_none_or(|c| set.contains(&c))),
        }
    }

    fn union(self, other: &Deletable) -> Deletable {
        match (self, other) {
            (Deletable::Any, _) | (_, Deletable::Any) => Deletable::Any,
            (Deletable::Nothing, other) => other.clone(),
            (this, Deletable::Nothing) => this,
            (Deletable::Segments(mut a), Deletable::Segments(b)) => {
                a.extend(b);
                Deletable::Segments(a)
            }
        }
    }
}

/// `true` when the rules of the stratum that adds a root, an affix piece or one half of a circumfix, or of a later one, can delete every segment of it before a still later stratum lists a morphological rule or template.
pub(crate) fn deletion_can_empty_morph_before_later_affix(g: &Grammar) -> bool {
    let Some(last) = g
        .strata
        .iter()
        .rposition(|s| !s.mrules.is_empty() || !s.templates.is_empty())
    else {
        return false;
    };
    // Walking back from the last stratum, `from[k]` is what strata `k..last` can delete, closed over every rule that can change a segment before a deletion.
    let mut from = vec![Deletable::Nothing; last + 1];
    for k in (0..last).rev() {
        from[k] = absorb_stratum(g, k, from[k + 1].clone());
    }
    for (k, stratum) in g.strata.iter().enumerate().take(last) {
        let deletable = &from[k];
        let roots = stratum.entries.iter().any(|&entry| {
            g.entries[entry.0 as usize]
                .allomorphs
                .iter()
                .any(|allomorph| deletable.empties(&segments_of(&allomorph.shape.shape)))
        });
        let affixes = listed_rules(g, k).any(|id| rule_can_be_emptied(g, id, deletable));
        if roots || affixes {
            return true;
        }
    }
    // An entry no stratum lists is checked against every deletion.
    let placed: HashSet<u32> = g
        .strata
        .iter()
        .flat_map(|s| s.entries.iter().map(|id| id.0))
        .collect();
    g.entries.iter().enumerate().any(|(index, entry)| {
        !placed.contains(&(index as u32))
            && entry
                .allomorphs
                .iter()
                .any(|allomorph| from[0].empties(&segments_of(&allomorph.shape.shape)))
    })
}

/// `true` when a deletion or an insertion before the last affixing stratum can change which morph a later affix copy orders first without emptying any literal: a deleted node left of the capture stops the boundary or optional fold (C# CopyFromInput.cs:28-48), and an inserted node at a morph's edge belongs to no morph, so a segmentless morph beside it is emptied.
pub(crate) fn shape_edit_can_reorder_morphs_before_later_affix(g: &Grammar) -> bool {
    let Some(last) = g
        .strata
        .iter()
        .rposition(|s| !s.mrules.is_empty() || !s.templates.is_empty())
    else {
        return false;
    };
    let mut from = Deletable::Nothing;
    for k in (0..last).rev() {
        from = absorb_stratum(g, k, from);
    }
    let deletes = !matches!(from, Deletable::Nothing);
    let inserts = g.strata.iter().take(last).any(|stratum| {
        stratum
            .prules
            .iter()
            .any(|id| match &g.prules[id.0 as usize] {
                PhonRuleDef::Rewrite(rule) => rule
                    .subrules
                    .iter()
                    .any(|sub| sub.rhs.nodes.len() > rule.lhs.nodes.len()),
                _ => false,
            })
    });
    let foldable = can_hold_skippable_node(g, last);
    let segmentless_morph = (0..g.strata.len()).any(|k| {
        listed_rules(g, k).any(|id| {
            rule_outputs(g, id).into_iter().any(|rhs| {
                !rhs.iter().any(|action| match action {
                    OutputAction::InsertSegments { shape, .. } => {
                        !segments_of(&shape.shape).is_empty()
                    }
                    OutputAction::InsertContext(_) => true,
                    _ => false,
                })
            })
        })
    });
    (deletes && foldable) || (inserts && segmentless_morph)
}

/// Whether some root or affix literal in a stratum before the last affixing one, the literal being one an affix writes before it copies anything, or a rewrite output can put a boundary or an optional node at the start of a word, the only place the affix copy's left fold can reach; a boundary after a copy is led by a node some literal keeps unless a literal can lose every segment, which the emptied-morph test already refuses.
fn can_hold_skippable_node(g: &Grammar, last: usize) -> bool {
    let plain = |shape: &Shape| {
        shape
            .interior()
            .all(|(_, kind, _, flags)| kind == NodeKind::Segment && !flags.is_optional())
    };
    // A pattern allomorph is matched, never synthesised; a guessed root is plain segments.
    let roots = g.strata.iter().take(last).any(|stratum| {
        stratum.entries.iter().any(|id| {
            g.entries[id.0 as usize]
                .allomorphs
                .iter()
                .any(|a| !a.is_pattern && !plain(&a.shape.shape))
        })
    });
    let literals = (0..last).any(|k| {
        listed_rules(g, k).any(|id| {
            rule_outputs(g, id).into_iter().any(|rhs| {
                rhs.iter()
                    .take_while(|action| !matches!(action, OutputAction::Copy(_)))
                    .any(|action| {
                        matches!(action, OutputAction::InsertSegments { shape, .. } if !plain(&shape.shape))
                    })
            })
        })
    });
    let segment_only = |node: &PatternNode| match node {
        PatternNode::Context(_) => true,
        PatternNode::CharDef(id) => g
            .char_tables
            .iter()
            .all(|t| (id.0 as usize) < t.len() && t.get(*id).kind() == CharDefKind::Segment),
        _ => false,
    };
    let rewrites = g.prules.iter().any(|rule| match rule {
        PhonRuleDef::Rewrite(rule) => rule
            .subrules
            .iter()
            .any(|sub| !sub.rhs.nodes.iter().all(segment_only)),
        PhonRuleDef::Metathesis(_) => false,
    });
    roots || literals || rewrites
}

/// Whether some subrule of a rewrite writes fewer nodes than it matches, which is what this crate calls a deleting rewrite (an empty output, or a narrowing such as `o t -> t`).
pub(crate) fn rewrite_deletes(rule: &RewriteRuleDef) -> bool {
    let lhs = rule.lhs.nodes.len();
    rule.subrules.iter().any(|sub| sub.rhs.nodes.len() < lhs)
}

/// What stratum `k` and the strata after it can delete, given `later` for the strata after it. Walking stratum `k`'s rewrites last to first, a rewrite that deletes adds what it matches; a rewrite that can change a segment while some later rule deletes adds what it matches too, since the segment it changes may become one a later rule deletes; a morphological rule that modifies copied segments while any deletion follows makes every segment deletable. A node this test cannot read, or a grammar with more than one character table, deletes any segment.
fn absorb_stratum(g: &Grammar, k: usize, later: Deletable) -> Deletable {
    let stratum = &g.strata[k];
    let table = &g.char_tables[stratum.table.0 as usize];
    let mut out = later;
    for id in stratum.prules.iter().rev() {
        let PhonRuleDef::Rewrite(rule) = &g.prules[id.0 as usize] else {
            continue;
        };
        let lhs = rule.lhs.nodes.len();
        let deletes = rewrite_deletes(rule);
        let changes = lhs > 0 && rule.subrules.iter().any(|sub| !sub.rhs.nodes.is_empty());
        if !deletes && !(changes && !matches!(out, Deletable::Nothing)) {
            continue;
        }
        out = out.union(&rule_inputs(g, table, &rule.lhs.nodes));
    }
    let modifies = listed_rules(g, k).any(|id| rule_modifies(g, id));
    if modifies && !matches!(out, Deletable::Nothing) {
        return Deletable::Any;
    }
    out
}

/// Every segment a rewrite's input nodes can match.
fn rule_inputs(g: &Grammar, table: &CharDefTable, nodes: &[PatternNode]) -> Deletable {
    if g.char_tables.len() != 1 {
        return Deletable::Any;
    }
    let mut out = HashSet::new();
    for node in nodes {
        match crate::metathesis_span::node_segments(g, table, node) {
            Some(segments) => out.extend(segments),
            None => return Deletable::Any,
        }
    }
    Deletable::Segments(out)
}

/// Every morphological rule stratum `k` lists, directly or in a template slot.
fn listed_rules(g: &Grammar, k: usize) -> impl Iterator<Item = MRuleId> + '_ {
    let stratum = &g.strata[k];
    let templated = stratum
        .templates
        .iter()
        .flat_map(|t| g.templates[t.0 as usize].slots.iter())
        .flat_map(|slot| slot.rules.iter().copied());
    stratum.mrules.iter().copied().chain(templated)
}

/// The output of every subrule of a listed rule.
fn rule_outputs(g: &Grammar, id: MRuleId) -> Vec<&[OutputAction]> {
    let rule = &g.mrules[id.0 as usize];
    match rule.affix_allomorphs() {
        Some(allomorphs) => allomorphs.iter().map(|a| a.rhs.as_slice()).collect(),
        None => match rule {
            MorphRuleDef::Compounding(def) => {
                def.subrules.iter().map(|s| s.rhs.as_slice()).collect()
            }
            _ => Vec::new(),
        },
    }
}

/// Whether a listed rule changes the features of a copied part.
fn rule_modifies(g: &Grammar, id: MRuleId) -> bool {
    rule_outputs(g, id)
        .into_iter()
        .flatten()
        .any(|action| matches!(action, OutputAction::Modify(..)))
}

/// Whether a listed rule inserts a literal the deletions can empty; an inserted natural class or a part copied twice counts whenever any deletion exists.
fn rule_can_be_emptied(g: &Grammar, id: MRuleId, deletable: &Deletable) -> bool {
    rule_outputs(g, id).into_iter().any(|rhs| {
        let mut copied: Vec<&PartRef> = Vec::new();
        rhs.iter().any(|action| match action {
            OutputAction::InsertSegments { shape, .. } => {
                deletable.empties(&segments_of(&shape.shape))
            }
            OutputAction::InsertContext(_) => deletable.empties(&[None]),
            OutputAction::Copy(part) => {
                let twice = copied.contains(&part);
                copied.push(part);
                twice && deletable.empties(&[None])
            }
            OutputAction::Modify(..) => false,
        })
    })
}

/// Each segment of `shape`, or `None` for one with no single spelling; boundaries are skipped.
fn segments_of(shape: &Shape) -> Vec<Option<u32>> {
    shape
        .interior()
        .filter(|(_, kind, _, _)| *kind == NodeKind::Segment)
        .map(|(_, _, char_def, _)| (char_def != NO_CHAR_DEF).then_some(char_def))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metathesis_span::tests::{affix, change, deletion, grammar, metathesis, stratum};

    /// Stratum `S0` deletes `a` and lists `inner` (with roots `roots`); stratum `S1` lists suffix `SU` when `later_suffix`.
    fn two_strata(inner: &[(&str, &str, &str)], roots: &[&str], later_suffix: bool) -> bool {
        let ids: Vec<&str> = inner.iter().map(|(id, _, _)| *id).collect();
        let defs: String = inner.iter().map(|(id, b, a)| affix(id, b, a)).collect();
        let mut strata = stratum("S0", &ids.join(" "), "d", &defs, roots);
        if later_suffix {
            strata += &stratum("S1", "SU", "", &affix("SU", "", "p"), &[]);
        }
        deletion_can_empty_morph_before_later_affix(&grammar(&deletion("d", "a"), &strata))
    }

    #[test]
    fn an_emptied_prefix_or_circumfix_half_before_a_later_suffix_is_found() {
        assert!(two_strata(&[("PR", "a", "")], &["ps"], true));
        assert!(two_strata(&[("CI", "a", "t")], &["ps"], true));
        assert!(two_strata(&[("PR", "aa", "")], &["ps"], true));
    }

    #[test]
    fn an_emptied_root_before_a_later_suffix_is_found() {
        assert!(two_strata(&[("PR", "p", "")], &["a"], true));
    }

    #[test]
    fn a_literal_that_keeps_a_segment_or_no_later_affix_is_not_found() {
        assert!(!two_strata(&[("PR", "pa", "")], &["ps"], true));
        assert!(!two_strata(&[("PR", "a", "")], &["ps"], false));
    }

    #[test]
    fn a_literal_added_after_the_deleting_stratum_is_not_found() {
        let strata = stratum("S0", "", "d", "", &["ps"])
            + &stratum("S1", "PR", "", &affix("PR", "a", ""), &["a"])
            + &stratum("S2", "SU", "", &affix("SU", "", "p"), &[]);
        assert!(!deletion_can_empty_morph_before_later_affix(&grammar(
            &deletion("d", "a"),
            &strata
        )));
    }

    #[test]
    fn a_deletion_after_the_last_affixing_stratum_is_not_found() {
        let defs = [affix("PR", "a", ""), affix("SU", "", "p")].concat();
        let same = stratum("S0", "PR SU", "d", &defs, &["ps"]);
        assert!(!deletion_can_empty_morph_before_later_affix(&grammar(
            &deletion("d", "a"),
            &same
        )));
        let later = stratum("S0", "PR SU", "", &defs, &["ps"]) + &stratum("S1", "", "d", "", &[]);
        assert!(!deletion_can_empty_morph_before_later_affix(&grammar(
            &deletion("d", "a"),
            &later
        )));
    }

    /// Stratum `S0` lists prefix `PR` = `p` and applies `prules` (defined by `defs`); stratum `S1` lists suffix `SU`.
    fn prefix_p_under(defs: &str, prules: &str) -> bool {
        let strata = stratum("S0", "PR", prules, &affix("PR", "p", ""), &["ps"])
            + &stratum("S1", "SU", "", &affix("SU", "", "a"), &[]);
        deletion_can_empty_morph_before_later_affix(&grammar(defs, &strata))
    }

    #[test]
    fn a_change_that_feeds_a_deletion_empties_its_input() {
        let defs = [change("c", "p", "t"), deletion("d", "t")].concat();
        assert!(prefix_p_under(&defs, "c d"));
    }

    #[test]
    fn a_change_chain_that_feeds_a_deletion_empties_its_first_input() {
        let defs = [
            change("c", "p", "s"),
            change("e", "s", "t"),
            deletion("d", "t"),
        ]
        .concat();
        assert!(prefix_p_under(&defs, "c e d"));
    }

    #[test]
    fn a_change_after_every_deletion_does_not_empty_its_input() {
        let defs = [change("c", "p", "t"), deletion("d", "t")].concat();
        assert!(!prefix_p_under(&defs, "d c"));
    }

    #[test]
    fn a_change_in_one_stratum_feeds_a_deletion_in_a_middle_stratum() {
        let defs = [change("c", "p", "t"), deletion("d", "t")].concat();
        let strata = stratum("S0", "PR", "c", &affix("PR", "p", ""), &["ps"])
            + &stratum("S1", "", "d", "", &[])
            + &stratum("S2", "SU", "", &affix("SU", "", "a"), &[]);
        assert!(deletion_can_empty_morph_before_later_affix(&grammar(
            &defs, &strata
        )));
    }

    #[test]
    fn a_deletion_in_a_middle_stratum_empties_an_earlier_literal() {
        let strata = stratum("S0", "PR", "", &affix("PR", "a", ""), &["ps"])
            + &stratum("S1", "", "d", "", &[])
            + &stratum("S2", "SU", "", &affix("SU", "", "p"), &[]);
        assert!(deletion_can_empty_morph_before_later_affix(&grammar(
            &deletion("d", "a"),
            &strata
        )));
    }

    #[test]
    fn a_prefix_in_a_template_slot_is_checked() {
        let template = r#"</MorphologicalRuleDefinitions><AffixTemplates><AffixTemplate requiredPartsOfSpeech="v"><Name>T</Name><Slot optional="true" morphologicalRules="PR"><Name>s</Name></Slot></AffixTemplate></AffixTemplates>"#;
        let slotted = |prefix: &str| {
            let strata = stratum("S0", "", "d", &affix("PR", prefix, ""), &["ps"])
                .replace("</MorphologicalRuleDefinitions>", template)
                + &stratum("S1", "SU", "", &affix("SU", "", "p"), &[]);
            deletion_can_empty_morph_before_later_affix(&grammar(&deletion("d", "a"), &strata))
        };
        assert!(slotted("a"));
        assert!(!slotted("ta"));
    }

    #[test]
    fn a_literal_empties_only_when_every_segment_is_deletable() {
        let set = Deletable::Segments(HashSet::from([1, 2]));
        assert!(set.empties(&[Some(1), Some(2)]));
        assert!(!set.empties(&[Some(1), Some(3)]));
        assert!(!set.empties(&[]));
        assert!(set.empties(&[None]));
        assert!(!Deletable::Nothing.empties(&[Some(1)]));
        assert!(Deletable::Any.empties(&[Some(9)]));
    }

    /// Stratum `S0` applies `prules` and lists `prefix` (a literal, empty for none-inserting) over root `ps`; stratum `S1` lists suffix `SU`.
    fn edit_before_suffix(defs: &str, prules: &str, prefix: &str, later: bool) -> bool {
        let mut strata = stratum("S0", "PR", prules, &affix("PR", prefix, ""), &["ps"]);
        if later {
            strata += &stratum("S1", "SU", "", &affix("SU", "", "a"), &[]);
        }
        shape_edit_can_reorder_morphs_before_later_affix(&grammar(defs, &strata))
    }

    #[test]
    fn a_deletion_before_a_later_affix_with_a_boundary_prefix_can_reorder() {
        assert!(edit_before_suffix(&deletion("d", "p"), "d", "+", true));
    }

    #[test]
    fn a_deletion_with_nothing_for_the_fold_to_span_does_not() {
        assert!(!edit_before_suffix(&deletion("d", "p"), "d", "t", true));
        assert!(!edit_before_suffix(&deletion("d", "p"), "d", "+", false));
    }

    #[test]
    fn a_deletion_after_the_last_affixing_stratum_is_not_ahead_of_an_affix_copy() {
        let strata = stratum("S0", "PR", "", &affix("PR", "+", ""), &["ps"])
            + &stratum("S1", "SU", "d", &affix("SU", "", "a"), &[]);
        assert!(!shape_edit_can_reorder_morphs_before_later_affix(&grammar(
            &deletion("d", "p"),
            &strata
        )));
    }

    #[test]
    fn an_insertion_beside_a_segmentless_morph_before_a_later_affix_can_reorder() {
        assert!(edit_before_suffix(&change("e", "a", "pp"), "e", "", true));
        assert!(!edit_before_suffix(&change("e", "a", "pp"), "e", "t", true));
    }

    #[test]
    fn a_deletion_and_a_metathesis_with_a_boundary_prefix_are_refused() {
        let defs = [deletion("d", "p"), metathesis("m", "ppst")].concat();
        assert!(edit_before_suffix(&defs, "d m", "+", true));
    }

    #[test]
    fn a_boundary_written_after_the_copy_is_not_at_the_word_start() {
        let strata = stratum("S0", "SX", "d", &affix("SX", "", "+"), &["ps"])
            + &stratum("S1", "SU", "", &affix("SU", "", "a"), &[]);
        let g = grammar(&deletion("d", "p"), &strata);
        assert!(!shape_edit_can_reorder_morphs_before_later_affix(&g));
    }

    #[test]
    fn a_boundary_literal_in_the_last_affixing_stratum_is_not_ahead_of_an_affix_copy() {
        let strata = stratum("S0", "PR", "d", &affix("PR", "t", ""), &["ps"])
            + &stratum("S1", "SU", "", &affix("SU", "+", ""), &[]);
        let g = grammar(&deletion("d", "p"), &strata);
        assert!(!shape_edit_can_reorder_morphs_before_later_affix(&g));
        let earlier = stratum("S0", "PR", "d", &affix("PR", "+", ""), &["ps"])
            + &stratum("S1", "SU", "", &affix("SU", "", "a"), &[]);
        assert!(shape_edit_can_reorder_morphs_before_later_affix(&grammar(
            &deletion("d", "p"),
            &earlier
        )));
    }
}
