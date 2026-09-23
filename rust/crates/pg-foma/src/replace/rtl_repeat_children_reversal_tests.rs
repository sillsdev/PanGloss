//! Reproduces the shallow (pre-fix) `reversed_slots` shape compiling a wrong RTL branch net.
//! See `docs/research/pg-foma-replace-design-notes.md`, "`reversed_slots`: why it must recurse into `Slot::Repeat` children".

use std::collections::HashMap;

use foma::apply::apply_init;

use super::*;

/// The pre-fix `reversed_slots` shape: a shallow reversal that never recurses into `Slot::Repeat` children. Kept only here as a negative witness.
fn shallow_reversed_slots_pre_fix(slots: &[Slot]) -> Vec<Slot> {
    slots.iter().rev().cloned().collect()
}

/// Builds `fsm_reverse(mirror rule net)` for a rule whose only environment is `right_slots`, using whichever `reverse_fn` the caller supplies -- lets this test compare the shallow and fixed reversals in isolation (no union with `plain_net`).
fn isolated_reversed_env_net(
    opts: &FomaOptions,
    alphabet: &SegAlphabet,
    lhs_slots: &[Slot],
    rhs_slots: &[Slot],
    right_slots: &[Slot],
    asg: &AlphaAssignment,
    reverse_fn: impl Fn(&[Slot]) -> Vec<Slot>,
) -> Fsm {
    let mirror_lhs = reverse_fn(lhs_slots);
    let mirror_rhs = reverse_fn(rhs_slots);
    // Swap: the mirror's own left environment is the reversed original right environment; this fixture has no left_env, so the mirror's right environment is empty.
    let mirror_left = reverse_fn(right_slots);
    let mirror_right: Vec<Slot> = Vec::new();
    let mirror_regex = render_branch_regex(
        alphabet,
        &mirror_lhs,
        &mirror_rhs,
        &mirror_left,
        &mirror_right,
        asg,
    );
    let mirror_net = fsm_parse_regex(opts, &mirror_regex, None, None)
        .unwrap_or_else(|| panic!("foma rejected mirror regex {mirror_regex:?}"));
    fsm_reverse(mirror_net)
}

/// Every `apply_down` result for `word` against `net`; this fixture's nets are deterministic per word, so a single-element `Vec` is expected throughout.
fn apply_down_all(net: &Fsm, word: &str) -> Vec<String> {
    let mut h = apply_init(net);
    h.down(word).collect()
}

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// RTL rewrite rule `t -> d` gated by a right environment `(a b)^{1,max_attr}` -- two heterogeneous, non-palindromic children, so a correct reversal must swap them.
fn rtl_hetero_repeat_xml(max_attr: &str) -> String {
    format!(
        r#"<HermitCrabInput><Language><Name>RtlHeteroRepeat</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="ct"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cd"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <PhonologicalRuleDefinitions>
        <PhonologicalRule id="prRtlHeteroRepeat" multipleApplicationOrder="rightToLeftIterative">
          <Name>rtlHeteroRepeatDemo</Name>
          <PhoneticInput><PhoneticSequence><Segment segment="ct" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><Segment segment="cd" /></PhoneticSequence></PhoneticOutput>
              <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence>
                <OptionalSegmentSequence min="1" max="{max_attr}">
                  <Segment segment="ca" /><Segment segment="cb" />
                </OptionalSegmentSequence>
              </PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
      </PhonologicalRuleDefinitions>
      <Strata><Stratum characterDefinitionTable="t1" phonologicalRules="prRtlHeteroRepeat"><Name>S</Name></Stratum></Strata>
    </Language></HermitCrabInput>"#
    )
}

/// Shared body: builds the isolated reversed-branch net both ways and demonstrates the divergence between the shallow and fixed reversal.
fn reproduce_for_max_attr(max_attr: &str) {
    let g = load(&rtl_hetero_repeat_xml(max_attr));
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(rule.dir, Dir::RightToLeft);
    let subrule = &rule.subrules[0];
    let right_env = subrule
        .right_env
        .as_ref()
        .expect("fixture declares a right environment");

    let table = owning_table(&g, rule).expect("rule is wired into stratum S's own cascade");
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();

    let mut next_occurrence = 0usize;
    let scope = crate::lower::PatternLowerScope::RewriteRuleCompile;
    let lhs_slots = pattern_slots(&g, table, &rule.lhs, &mut next_occurrence, scope)
        .expect("fixed-segment LHS must lower");
    let rhs_slots = pattern_slots(&g, table, &subrule.rhs, &mut next_occurrence, scope)
        .expect("fixed-segment RHS must lower");
    let right_slots = pattern_slots(&g, table, right_env, &mut next_occurrence, scope).expect(
        "a well-formed 2-child quantifier group must lower (bounded: \
         compile-bounded-fst-quantifiers; max=\"-1\": build-unbounded-quantifier-support)",
    );

    // Sanity: exactly one top-level `Slot::Repeat` with 2 heterogeneous children -- the shape a shallow reversal gets wrong.
    assert_eq!(right_slots.len(), 1);
    match &right_slots[0] {
        Slot::Repeat { children, .. } => {
            assert_eq!(
                children.len(),
                2,
                "quantifier group must have exactly 2 children"
            );
        }
        _ => panic!("expected the right environment to lower to a single Slot::Repeat"),
    }

    let asg = AlphaAssignment {
        values: HashMap::new(),
    };

    // The crate's own (fixed, recursing) construction.
    let reversed_fixed = isolated_reversed_env_net(
        &opts,
        &alphabet,
        &lhs_slots,
        &rhs_slots,
        &right_slots,
        &asg,
        reversed_slots,
    );
    // The old, shallow, pre-fix construction, kept as a regression witness.
    let reversed_old_shallow = isolated_reversed_env_net(
        &opts,
        &alphabet,
        &lhs_slots,
        &rhs_slots,
        &right_slots,
        &asg,
        shallow_reversed_slots_pre_fix,
    );

    let query_tab = alphabet.encode_query("tab").expect("'tab' must segment");
    let query_tba = alphabet.encode_query("tba").expect("'tba' must segment");
    let query_dab = alphabet.encode_query("dab").expect("'dab' must segment");
    let query_dba = alphabet.encode_query("dba").expect("'dba' must segment");

    // The fixed (recursing) construction matches the rule's own stated environment: 't' followed by 'a' then 'b'.
    assert_eq!(
        apply_down_all(&reversed_fixed, &query_tab),
        vec![query_dab.clone()],
        "FIXED reversed_net: 't' followed by 'ab' satisfies the rule's own right_env -- must \
         rewrite (max={max_attr:?})"
    );
    assert_eq!(
        apply_down_all(&reversed_fixed, &query_tba),
        vec![query_tba.clone()],
        "FIXED reversed_net: 't' followed by 'ba' does NOT satisfy the rule's own right_env -- \
         must pass through unchanged (max={max_attr:?})"
    );

    // The old, shallow construction gets this backwards: it never reverses the 2 children, so it requires 'b' then 'a' -- the wrong environment.
    assert_eq!(
        apply_down_all(&reversed_old_shallow, &query_tab),
        vec![query_tab.clone()],
        "BUG (task #32, pre-fix): the shallow reversed_net does NOT rewrite 't' before 'ab' -- \
         it silently misses the rule's own real right-environment entirely, because a shallow \
         reversal never recurses into the Repeat's own children (max={max_attr:?})"
    );
    assert_eq!(
        apply_down_all(&reversed_old_shallow, &query_tba),
        vec![query_dba.clone()],
        "BUG (task #32, pre-fix): the shallow reversed_net WRONGLY rewrites 't' before 'ba' -- \
         the children order a shallow reversal leaves in DOCUMENT order instead of reversing it \
         (max={max_attr:?})"
    );

    assert_ne!(
        apply_down_all(&reversed_old_shallow, &query_tab),
        apply_down_all(&reversed_fixed, &query_tab),
        "the pre-fix shallow reversed_net's own compiled language genuinely differs from the \
         true reverse construction's -- task #32 REPRODUCED (max={max_attr:?})"
    );
}

/// Reproduction + regression pin, FINITELY bounded quantifier (`max="2"`).
#[test]
fn rtl_repeat_children_reversal_bug_reproduced_and_fixed_bounded() {
    reproduce_for_max_attr("2");
}

/// Same reproduction, genuinely unbounded quantifier (`max="-1"`): `max: None` hits the same defect and the same fix covers it.
#[test]
fn rtl_repeat_children_reversal_bug_reproduced_and_fixed_unbounded() {
    reproduce_for_max_attr("-1");
}
