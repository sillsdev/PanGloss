//! Partition-k / MPR-POS subrule gating builder. Generalizes the hand-authored gating shape into a
//! parameterized-over-`k` builder: `k` INDEPENDENT gated `<PhonologicalRule>`s (each with exactly
//! one `<PhonologicalSubrule requiredMPRFeatures="mprJ">`), plus `2^k` lexical entries realizing
//! EVERY possible gating-key combination (entry `i`'s own `ruleFeatures` is exactly the subset of
//! `{mpr0..mprK-1}` corresponding to `i`'s own bits) -- `pg_foma::gate::partition_entries` must
//! therefore find exactly `2^k` distinct groups, one per entry.
//!
//! ## Why each gated rule targets its OWN dedicated marker segment, not a shared one
//! This builder's own gate (`pg-foma/tests/phase_c_partition_k.rs`) needs the COMPILED net's
//! actual per-entry OUTPUT to be independently verifiable. Giving each gated rule `j` its own private
//! "off_j -> on_j" segment
//! pair (never reused by any other rule, never reused by any other entry's non-marker material)
//! means rule `j`'s LHS can only ever match at entry `i`'s own marker-`j` position -- no cross-rule
//! interference, no dependence on rule/entry ordering, and the expected surface form for entry `i`
//! is mechanically derivable from `i`'s own bit pattern alone: `{base}{on_0 or off_0}{on_1 or
//! off_1}...{on_{k-1} or off_{k-1}}`, matching a bit exactly to whether rule `j` fires for entry `i`
//! (module doc's own `ruleFeatures` correspondence).
//!
//! Needs `1 + 2*k` distinct segments from its own table (1 shared base character + one off/on pair
//! per gated rule) -- `crate::render::render_indexed`'s caller is responsible for sizing
//! `segment_inventory` up to at least this (mirrors `crate::build::circumfix`'s own "pad the
//! table" convention for its own affix material).

use crate::build::tables::TableSpec;
use crate::ids::IdMinter;

/// Everything `build` produces: the `k` gated `<PhonologicalRule>`s, the
/// `<MorphologicalPhonologicalRuleFeatures>` block declaring their `mprJ` ids, and `2^k`
/// `<LexicalEntry>` elements realizing every gating-key combination.
#[derive(Debug, Clone)]
pub struct GatingBuild {
    /// `k` `<PhonologicalRule>` elements, each gated by its own `mprJ`.
    pub prules_xml: String,
    /// The `k` rules' own minted xml ids, in `j` order (rule `j` == `rule_xml_ids[j]`).
    pub rule_xml_ids: Vec<String>,
    /// `<MorphologicalPhonologicalRuleFeatures>` declaring `mpr0..mprK-1`.
    pub mpr_features_xml: String,
    /// `2^k` `<LexicalEntry>` elements, document order == bit-pattern order (entry `i`'s own
    /// `<MorphemeId>` is `GATE{i}`).
    pub entries_xml: String,
    /// The `2^k` entries' own minted xml ids, `entry_xml_ids[i]` realizing gating key `i`
    /// (bit `j` of `i` == whether this entry carries `mprJ`, i.e. whether gated rule `j` fires for
    /// it) -- a gate resolves its own entries back out of the loaded `Grammar` by these ids
    /// (`tests/common/gate_template.rs`'s `entry_id_of` convention) and derives each entry's own
    /// expected surface form directly from its OWN index `i` via `expected_marker_state`.
    pub entry_xml_ids: Vec<String>,
}

/// `true` iff gated rule `j` fires for gating key `i` (bit `j` of `i` is set) -- shared by both the
/// builder (deciding `ruleFeatures`/marker spelling) and a gate (deriving entry `i`'s expected
/// post-gating surface form independently, without re-deriving this bit convention by hand).
pub fn bit_set(i: usize, j: usize) -> bool {
    (i >> j) & 1 != 0
}

/// Build `k` (`>= 1`) independent gated rules and `2^k` entries realizing every combination,
/// referencing part of speech `pos_xml_id` and drawing marker/base material from `table`'s own
/// segments (must have at least `1 + 2*k` -- module doc; panics otherwise, mirroring
/// `crate::build::circumfix::build_circumfixes`'s own non-empty-affix-material precondition).
pub fn build(k: usize, pos_xml_id: &str, table: &TableSpec, ids: &mut IdMinter) -> GatingBuild {
    assert!(k >= 1, "build_gating: k must be >= 1");
    let needed = 1 + 2 * k;
    assert!(
        table.segments.len() >= needed,
        "build_gating: table has {} segments, needs at least {needed} (1 base + 2 per gated rule, k={k})",
        table.segments.len()
    );

    let base_ch = table.segments[0].ch;
    // marker[j] = (off_seg, on_seg) for gated rule j, segments[1 + 2j] / segments[2 + 2j].
    let marker_segs: Vec<(
        &crate::build::tables::SegmentSpec,
        &crate::build::tables::SegmentSpec,
    )> = (0..k)
        .map(|j| (&table.segments[1 + 2 * j], &table.segments[2 + 2 * j]))
        .collect();

    let mut mpr_feature_defs = String::new();
    let mut mpr_ids = Vec::with_capacity(k);
    for j in 0..k {
        let mpr_id = format!("mprGate{j}");
        mpr_feature_defs.push_str(&format!(
            "\n      <MorphologicalPhonologicalRuleFeature id=\"{mpr_id}\">gate{j}</MorphologicalPhonologicalRuleFeature>"
        ));
        mpr_ids.push(mpr_id);
    }
    let mpr_features_xml = format!(
        "\n    <MorphologicalPhonologicalRuleFeatures>{mpr_feature_defs}\n    </MorphologicalPhonologicalRuleFeatures>"
    );

    let mut prules_xml = String::new();
    let mut rule_xml_ids = Vec::with_capacity(k);
    for j in 0..k {
        let rule_xml_id = ids.next("pruleGate");
        let (off_seg, on_seg) = marker_segs[j];
        prules_xml.push_str(&format!(
            "\n      <PhonologicalRule id=\"{rule_xml_id}\">\n        <Name>gate{j}</Name>\n        \
             <PhoneticInput><PhoneticSequence><Segment segment=\"{off_id}\" /></PhoneticSequence></PhoneticInput>\n        \
             <PhonologicalSubrules>\n          <PhonologicalSubrule requiredMPRFeatures=\"{mpr}\">\n            \
             <PhoneticOutput><PhoneticSequence><Segment segment=\"{on_id}\" /></PhoneticSequence></PhoneticOutput>\n          \
             </PhonologicalSubrule>\n        </PhonologicalSubrules>\n      </PhonologicalRule>",
            off_id = off_seg.xml_id,
            on_id = on_seg.xml_id,
            mpr = mpr_ids[j],
        ));
        rule_xml_ids.push(rule_xml_id);
    }

    let n_entries = 1usize << k;
    let mut entries_xml = String::new();
    let mut entry_xml_ids = Vec::with_capacity(n_entries);
    for i in 0..n_entries {
        let entry_xml_id = ids.next("entryGate");
        let allo_xml_id = ids.next("alloGate");
        let morpheme_id = format!("GATE{i}");
        let mut shape = String::new();
        shape.push(base_ch);
        let mut bits: Vec<&str> = Vec::new();
        for j in 0..k {
            let (off_seg, on_seg) = marker_segs[j];
            if bit_set(i, j) {
                shape.push(on_seg.ch);
                bits.push(&mpr_ids[j]);
            } else {
                shape.push(off_seg.ch);
            }
        }
        let rule_features_attr = if bits.is_empty() {
            String::new()
        } else {
            format!(" ruleFeatures=\"{}\"", bits.join(" "))
        };
        entries_xml.push_str(&format!(
            "\n          <LexicalEntry id=\"{entry_xml_id}\" partOfSpeech=\"{pos_xml_id}\"{rule_features_attr}>\n            \
             <Allomorphs><Allomorph id=\"{allo_xml_id}\"><PhoneticShape>{shape}</PhoneticShape></Allomorph></Allomorphs>\n            \
             <MorphemeId>{morpheme_id}</MorphemeId>\n          </LexicalEntry>"
        ));
        entry_xml_ids.push(entry_xml_id);
    }

    GatingBuild {
        prules_xml,
        rule_xml_ids,
        mpr_features_xml,
        entries_xml,
        entry_xml_ids,
    }
}

/// The UNGATED (pre-rule) spelling every entry carries in its own `<PhoneticShape>` before any
/// gated rule fires: `{base}{off_0}{off_1}...{off_{k-1}}` -- shared by a gate that wants to assert
/// against the entry's OWN declared spelling, not just the post-synthesis oracle word.
pub fn base_shape(table: &TableSpec, k: usize) -> String {
    let mut s = String::new();
    s.push(table.segments[0].ch);
    for j in 0..k {
        s.push(table.segments[1 + 2 * j].ch);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::tables;

    #[test]
    fn two_gated_rules_realize_four_combinations() {
        let mut ids = IdMinter::new();
        let tb = tables::build(1, 5, false, false, &mut ids);
        let gb = build(2, "posV", &tb.tables[0], &mut ids);
        assert_eq!(gb.rule_xml_ids.len(), 2);
        assert_eq!(gb.entry_xml_ids.len(), 4);
        assert!(
            gb.entries_xml
                .contains(r#"ruleFeatures="mprGate0 mprGate1""#),
            "entry 3 (bits 0,1 set) must carry both mpr features:\n{}",
            gb.entries_xml
        );
        // Entry 0 (no bits set) must carry no ruleFeatures attribute at all.
        let entry0_id = &gb.entry_xml_ids[0];
        let entry0_pos = gb
            .entries_xml
            .find(entry0_id.as_str())
            .expect("entry0 present");
        let entry0_line = &gb.entries_xml[entry0_pos..entry0_pos + 120];
        assert!(
            !entry0_line.contains("ruleFeatures"),
            "entry 0 must have no ruleFeatures: {entry0_line}"
        );
    }

    #[test]
    fn bit_set_matches_shift_convention() {
        assert!(!bit_set(0, 0));
        assert!(bit_set(1, 0));
        assert!(!bit_set(1, 1));
        assert!(bit_set(2, 1));
        assert!(bit_set(3, 0));
        assert!(bit_set(3, 1));
    }
}
