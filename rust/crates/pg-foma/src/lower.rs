//! Stage 1B (`openspec/changes/lower-fst-pattern-environments`): the shared pattern/environment →
//! FST lowering seam `openspec/changes/add-capability-characteristics-check/design.md` D3 needs
//! for `simultaneous.subrule-overlap`'s REAL automaton-intersection test, replacing
//! `capability.rs`'s prior conservative unconditional-`Refuse` fallback — see
//! [`crate::capability::SimultaneousSubruleOverlapPredicate`]'s own doc for exactly what this
//! replaces and how.
//!
//! # Scope of THIS step
//! `lower-fst-pattern-environments`'s own `design.md` asks for one lowering seam covering anchors,
//! polarity, groups, alternation, table identity, and quantifier metadata, and migrating EVERY
//! existing replacement caller (`replace.rs`/`gate.rs`) onto it (`tasks.md` 1.1-1.2, 2.1-2.3). This
//! step is narrower, scoped to exactly what D3's worked predicate needs: [`lower_span`] lowers one
//! subrule's `left_env · lhs_focus · right_env` triple (D3's own `span(s)` formula) into foma
//! acceptors, and [`spans_overlap`] tests two such spans for a non-empty intersection at the
//! shared focus position. `replace.rs`/`gate.rs` are UNTOUCHED beyond three visibility bumps
//! (`pattern_slots`/`resolve_alpha_tuples`/`render_slots` go `pub(crate)`) so this module can
//! REUSE their pattern semantics rather than re-derive it — see "What is reused" below. Migrating
//! `replace.rs`'s OWN rewrite-rule compilation onto this seam (design.md's "Migrate existing
//! replacement callers", `tasks.md` 2.1) is NOT attempted here; that is a separate, larger
//! follow-on this step does not claim. Full Stage 1B coverage (multi-table ownership, alternation,
//! quantifier metadata) is likewise future work — see [`UnsupportedPatternNode`]'s own doc for
//! exactly which node kinds this step's [`lower_span`] does and does not represent.
//!
//! # What is reused vs. newly written
//! Reused verbatim from [`crate::replace`] (visibility bumped `pub(crate)`, logic byte-for-byte
//! untouched — no duplicated pattern semantics):
//! - [`crate::replace::pattern_slots`] — `Pattern` → `Vec<Slot>`, the SAME node-kind coverage
//!   `replace.rs`'s own rewrite-rule compiler already gives LHS/RHS/environment patterns (`Some`
//!   for `CharDef`/agree-polarity `Context`, `None` on `Quantifier`/`Segments`/`Anchor`/disagree-
//!   polarity `Context`).
//! - [`crate::replace::resolve_alpha_tuples`] — the joint-agreement alpha-tuple cross product
//!   (reports/08 §3.1's bound).
//! - [`crate::replace::render_slots`] — slot list + concrete assignment → xre source text, same
//!   PUA-token space, same load-bearing space-separation convention that module's doc records.
//! - [`crate::replace::SegAlphabet`] — the char-def ↔ PUA-token codec (already `pub`, untouched).
//!
//! Newly written here: [`lower_span`] itself (how to COMBINE the reused pieces into acceptors —
//! the `Σ*`-padding construction its own doc works through), [`UnsupportedPatternNode`] (the typed
//! disposition evidence design.md's `spec.md` asks for — "a typed unsupported disposition... does
//! not omit or weaken the node"), and [`spans_overlap`] (the intersect-nonempty test over
//! `foma::constructions::{fsm_intersect, fsm_union, fsm_concat, fsm_universal}` /
//! `foma::structures::fsm_isempty` — primitives this crate had never previously called at all,
//! confirmed by grep before this step per `capability.rs`'s own prior doc: "no `lower.rs`/
//! pattern-to-`Fsm` facility anywhere in `pg-fst`/`pg-foma`").

use foma::constructions::{fsm_concat, fsm_intersect, fsm_union, fsm_universal};
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::structures::{fsm_empty_set, fsm_empty_string, fsm_isempty};
use foma::types::Fsm;

use pg_grammar::model::{Grammar, Pattern, PatternNode};

use crate::replace::{pattern_slots, render_slots, resolve_alpha_tuples, SegAlphabet};

/// A pattern node kind [`lower_span`] cannot yet represent (design.md `spec.md`'s "typed
/// unsupported disposition... does not omit or weaken the node"). Named after the `model.rs`
/// [`PatternNode`] variant (or, for the one non-node case, the [`pg_grammar::model::AlphaVar`]
/// shape) it names, so a caller's diagnostic can cite the EXACT construct rather than a generic
/// "pattern too complex" message — exactly the naming [`crate::replace::pattern_slots`]'s own doc
/// already scopes as unrendered by this crate's existing pattern compiler, carried through here as
/// a typed value instead of a silent `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedPatternNode {
    /// `PatternNode::Quantifier` (`<OptionalSegmentSequence min max>`) — unbounded/bounded-
    /// repetition group; no `Fsm` closure construction is attempted for it here (Stage 1B's own
    /// `tasks.md` 1.2 names quantifier metadata as future scope).
    Quantifier,
    /// `PatternNode::Segments` (`<Segments><PhoneticShape>`) — an inline pre-segmented literal
    /// shape group.
    Segments,
    /// `PatternNode::Anchor` (`initialBoundaryCondition`/`finalBoundaryCondition`) — a word-
    /// boundary constraint.
    Anchor,
    /// A `PatternNode::Context` carrying an [`pg_grammar::model::AlphaVar`] with `plus == false`
    /// ("disagree" polarity) — not a distinct node KIND, but the same "cannot lower faithfully"
    /// outcome [`pattern_slots`] already reports as unrendered.
    AlphaDisagreePolarity,
}

impl std::fmt::Display for UnsupportedPatternNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            UnsupportedPatternNode::Quantifier => "Quantifier (OptionalSegmentSequence)",
            UnsupportedPatternNode::Segments => "Segments (inline PhoneticShape group)",
            UnsupportedPatternNode::Anchor => "Anchor (word-boundary condition)",
            UnsupportedPatternNode::AlphaDisagreePolarity => {
                "Context with a disagree-polarity AlphaVariable"
            }
        };
        f.write_str(label)
    }
}

/// Scans `pattern` for the FIRST node [`pattern_slots`] cannot lower, to recover a typed reason
/// after `pattern_slots` has already returned `None` for it. Never called on a pattern
/// `pattern_slots` actually accepted — `unreachable!` guards that invariant rather than silently
/// reporting a wrong/default reason (this module's own conservative discipline: a diagnostic that
/// mis-names its cause is worse than no diagnostic).
fn diagnose_unsupported(pattern: &Pattern) -> UnsupportedPatternNode {
    for node in &pattern.nodes {
        match node {
            PatternNode::Quantifier { .. } => return UnsupportedPatternNode::Quantifier,
            PatternNode::Segments { .. } => return UnsupportedPatternNode::Segments,
            PatternNode::Anchor(_) => return UnsupportedPatternNode::Anchor,
            PatternNode::Context(sc) if sc.vars.iter().any(|v| !v.plus) => {
                return UnsupportedPatternNode::AlphaDisagreePolarity;
            }
            PatternNode::Context(_) | PatternNode::CharDef(_) => {}
        }
    }
    unreachable!(
        "pg_foma::lower::diagnose_unsupported called on a pattern pattern_slots did not actually \
         reject: {pattern:?} (a lower_span caller bug, not a grammar-authoring one)"
    );
}

/// Compiles `text` to an [`Fsm`] acceptor, treating an empty rendered string as the empty-string
/// language (concatenation identity) rather than an invalid regex — [`render_slots`] legitimately
/// returns `""` for an absent/empty pattern (no left-environment declared, an epenthesis-shaped
/// empty LHS, etc.), and `fsm_parse_regex` is not asked to parse that case at all.
fn parse_template(opts: &FomaOptions, text: &str) -> Fsm {
    if text.is_empty() {
        fsm_empty_string()
    } else {
        fsm_parse_regex(opts, text, None, None).unwrap_or_else(|| {
            panic!("pg_foma::lower: foma rejected a lowered span template regex {text:?}")
        })
    }
}

/// Lowers one subrule's `left_env · lhs_focus · right_env` triple (design.md D3's `span(s)`
/// formula) into a pair of foma acceptors over `alphabet`'s token space, for [`spans_overlap`]'s
/// intersection test. `focus` is `RewriteRuleDef.lhs` — shared verbatim across every subrule of
/// one rule (`RewriteSubruleDef` only supplies its own `rhs`/`left_env`/`right_env`, model.rs
/// `RewriteSubruleDef` doc).
///
/// # Why a `(left_language, focus_right_language)` PAIR, not one combined `Fsm`
/// D3 writes `span(s) = left_env · lhs_focus · right_env` and says to intersect two subrules'
/// spans. Read as a literal concatenation of the three patterns' own node sequences and compared
/// as ONE automaton, that is only sound when both subrules' `left_env`/`right_env` have the SAME
/// node count: `left_env`/`right_env` are boundary-anchored templates (they constrain the
/// segments immediately adjacent to the shared focus, not "some point in the word" — no
/// `Quantifier` closure is in scope here, so every non-`None` `left_env`/`right_env` this function
/// accepts has a fixed, statically-known length), so two subrules whose environments have
/// DIFFERENT node counts describe overlapping-but-different-length windows around the SAME anchor
/// point. A literal fixed-length concatenation, intersected whole, would (wrongly) report them as
/// non-overlapping merely because the two automata accept different string lengths — an UNSOUND
/// under-refusal (ADR 0001 forbids rounding toward `Admit`; ["`Refuse`(never) rounds toward
/// `Admit`"] is exactly backwards from the required direction).
///
/// The fix: represent `left_env` as the SUFFIX language `Σ* · left_env` (any prefix, ending in the
/// template) and fold `lhs_focus`/`right_env` into the PREFIX language `lhs_focus · right_env ·
/// Σ*` (starting with the shared focus then the template, any suffix) — each half anchored at the
/// boundary it actually describes, `Σ*` absorbing any length mismatch between the two subrules'
/// own templates. [`spans_overlap`] then intersects the two subrules' LEFT halves and FOCUS+RIGHT
/// halves SEPARATELY (not concatenated into one "contains the whole span somewhere in the word"
/// automaton) — see that function's own doc for why checking them separately is the CORRECT
/// decomposition of D3's "at a shared focus position" requirement, not merely a convenient
/// approximation of one (a single combined `Σ* · L · F · R · Σ*` "contains" automaton would
/// actually be WRONG here: it would accept a witness word where subrule i's context holds at one
/// position and subrule j's holds at an unrelated OTHER position, which is not the same-position
/// overlap D3 asks about).
///
/// # Alpha variables
/// `left_env`/`focus`/`right_env` are lowered with a FRESH, shared occurrence counter local to
/// this call — exactly mirroring how `replace.rs::compile_rewrite_rule_subset` resets
/// `next_occurrence` to `0` per subrule — and jointly resolved via the REUSED
/// [`resolve_alpha_tuples`], so an `AlphaVariable` shared between (say) `left_env` and `focus` is
/// resolved with the SAME joint-agreement semantics real rewrite-rule compilation already uses,
/// not a re-derived one. The subrule's OWN `rhs` is deliberately NOT included in this joint
/// resolution (unlike `replace.rs`'s per-subrule fold, which joins LHS+RHS+left+right together):
/// whether this SPAN can match does not depend on the subrule's RHS at all, and omitting it can
/// only ever ADD spurious alpha tuples relative to the true RHS-constrained set (never remove real
/// ones, since the RHS's own occurrences could only additionally NARROW the joint-agreement
/// filter) — a strictly SAFE, over-permissive simplification that rounds toward more overlap being
/// detected (i.e. toward `Refuse` in [`spans_overlap`]), never an unsound one.
///
/// Each resolved tuple's rendered text is [`fsm_parse_regex`]-compiled (via [`parse_template`])
/// and the per-tuple automata are `fsm_union`-folded per half (a subrule's span matches under ANY
/// of its own valid alpha assignments, not just one) — contrast
/// [`crate::replace::compile_rewrite_rule_subset`]'s per-tuple fold, which is a SEQUENTIAL
/// composition because there each tuple's compiled net is a full elsewhere-preserving REPLACE
/// transducer (that module's own doc: union would reintroduce a spurious "did nothing" path).
/// Here each tuple's compiled net is a plain ACCEPTOR with no "elsewhere" case, so union is exactly
/// the right combinator, not a divergence from that module's reasoning.
///
/// # Returns
/// `Err` names the FIRST unsupported node encountered (checked in `left_env`, `focus`, `right_env`
/// order) via [`UnsupportedPatternNode`] — the caller (`capability.rs`) rounds this to a
/// conservative `Refuse` naming the kind, per this module's own top-doc and design.md D3's "any
/// approximation rounds toward Refuse".
pub fn lower_span(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    left_env: Option<&Pattern>,
    focus: &Pattern,
    right_env: Option<&Pattern>,
) -> Result<(Fsm, Fsm), UnsupportedPatternNode> {
    let mut next_occurrence = 0usize;
    // `openspec/changes/fix-multitable-fst-compilation`: `pattern_slots`/`resolve_alpha_tuples`
    // now take an explicit table (no more implicit `g.char_tables[0]`) -- `alphabet.table()` is
    // already the correct table for this call, by this function's OWN caller contract (module
    // doc: `lower_span` is handed whichever `SegAlphabet` its caller already resolved correctly,
    // e.g. `capability.rs`'s `lower_subrule_span` now resolves it via
    // `crate::replace::owning_table` too -- see that function's own doc).
    let table = alphabet.table();

    let left_slots = match left_env {
        Some(p) => pattern_slots(g, table, p, &mut next_occurrence)
            .ok_or_else(|| diagnose_unsupported(p))?,
        None => Vec::new(),
    };
    let focus_slots = pattern_slots(g, table, focus, &mut next_occurrence)
        .ok_or_else(|| diagnose_unsupported(focus))?;
    let right_slots = match right_env {
        Some(p) => pattern_slots(g, table, p, &mut next_occurrence)
            .ok_or_else(|| diagnose_unsupported(p))?,
        None => Vec::new(),
    };

    let (assignments, _report) = resolve_alpha_tuples(
        table,
        &[
            left_slots.as_slice(),
            focus_slots.as_slice(),
            right_slots.as_slice(),
        ],
    );

    let mut left_lang: Option<Fsm> = None;
    let mut focus_right_lang: Option<Fsm> = None;
    for asg in &assignments {
        let left_text = render_slots(alphabet, &left_slots, asg);
        let focus_text = render_slots(alphabet, &focus_slots, asg);
        let right_text = render_slots(alphabet, &right_slots, asg);

        let left_tpl = parse_template(opts, &left_text);
        let focus_tpl = parse_template(opts, &focus_text);
        let right_tpl = parse_template(opts, &right_text);

        // Sigma* . left_template  (suffix language: any prefix, ending in the left template).
        let this_left = fsm_concat(opts, fsm_universal(), left_tpl);
        // focus_template . right_template . Sigma*  (prefix language: starts with the shared
        // focus then the right template, any suffix).
        let this_focus_right = fsm_concat(
            opts,
            fsm_concat(opts, focus_tpl, right_tpl),
            fsm_universal(),
        );

        left_lang = Some(match left_lang {
            None => this_left,
            Some(prev) => fsm_union(opts, prev, this_left),
        });
        focus_right_lang = Some(match focus_right_lang {
            None => this_focus_right,
            Some(prev) => fsm_union(opts, prev, this_focus_right),
        });
    }

    // `assignments` is empty only when the joint-agreement filter finds NO valid alpha tuple at
    // all (a subrule whose own environment/focus alpha constraints are mutually unsatisfiable) --
    // the empty language is the exactly-correct span for a subrule that can never match anything.
    Ok((
        left_lang.unwrap_or_else(fsm_empty_set),
        focus_right_lang.unwrap_or_else(fsm_empty_set),
    ))
}

/// design.md D3's intersection test: `true` iff subrules `a` and `b`'s spans (each a
/// `(left_language, focus_right_language)` pair from [`lower_span`]) can hold AT THE SAME shared
/// focus position — i.e. genuinely overlap.
///
/// # Why two independent intersections, not one combined automaton
/// The real actual-word content immediately LEFT of the shared focus position is ONE concrete
/// (finite) string; it satisfies subrule `a`'s left environment iff it is a member of `a`'s
/// `left_language`, and independently satisfies `b`'s iff it is a member of `b`'s `left_language`
/// — both languages describe THE SAME region of the SAME word, so the question "can some real
/// left-context simultaneously satisfy both" is exactly `intersect(left_a, left_b)` non-empty, no
/// further alignment machinery needed (the `Σ*` prefix in each already anchors the comparison at
/// the shared right edge — see [`lower_span`]'s own doc). The symmetric argument holds for the
/// content AT/RIGHT of the position via `focus_right_language`. Because the left region and the
/// focus+right region of a word are DISJOINT and freely composable (any accepted left-string
/// concatenated with any accepted focus+right-string is a valid witness word — nothing else
/// constrains them jointly once each subrule's OWN internal alpha agreement has already been
/// resolved inside [`lower_span`]), `a` and `b` can co-fire at the same position iff BOTH
/// intersections are non-empty; checking them as one combined `Σ* · L · F · R · Σ*` "contains
/// somewhere" automaton instead would be WRONG (see [`lower_span`]'s own doc for the false-overlap
/// case that construction admits).
///
/// Any imprecision [`lower_span`]'s per-subrule marginalization introduces (projecting each of a
/// subrule's OWN internally-consistent alpha tuples down to a left-only / focus+right-only piece
/// before unioning across tuples) can only ever make a language LARGER than the true "matches
/// under some single self-consistent assignment" set — i.e. can only report MORE overlap than
/// truly exists, never less — which rounds toward `Refuse`, the safe direction (ADR 0001).
pub fn spans_overlap(opts: &FomaOptions, a: &(Fsm, Fsm), b: &(Fsm, Fsm)) -> bool {
    let (left_a, focus_right_a) = a;
    let (left_b, focus_right_b) = b;

    let mut left_intersection = fsm_intersect(opts, left_a.clone(), left_b.clone());
    if fsm_isempty(opts, &mut left_intersection) {
        return false;
    }
    let mut focus_right_intersection =
        fsm_intersect(opts, focus_right_a.clone(), focus_right_b.clone());
    !fsm_isempty(opts, &mut focus_right_intersection)
}

#[cfg(test)]
mod tests {
    //! Synthetic, delanguaged fixtures only (no natural-language names), mirroring
    //! `capability.rs`'s own test-module convention.

    use pg_grammar::model::{PhonRuleDef, RewriteMode};

    use super::*;

    fn load(xml: &str) -> pg_grammar::model::Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    const OVERLAP_LOWER_PROBE_XML: &str = r#"<HermitCrabInput><Language><Name>OverlapLowerProbe</Name>
      <PhonologicalFeatureSystem>
        <SymbolicFeature id="featPlace"><Name>place</Name>
          <Symbols>
            <Symbol id="symNeutral">neutral</Symbol>
            <Symbol id="symFront">front</Symbol>
            <Symbol id="symBack">back</Symbol>
          </Symbols>
        </SymbolicFeature>
      </PhonologicalFeatureSystem>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="cStop"><Representations><Representation>p</Representation></Representations>
            <FeatureValue feature="featPlace" symbolValues="symNeutral" />
          </SegmentDefinition>
          <SegmentDefinition id="cFront"><Representations><Representation>i</Representation></Representations>
            <FeatureValue feature="featPlace" symbolValues="symFront" />
          </SegmentDefinition>
          <SegmentDefinition id="cBack"><Representations><Representation>u</Representation></Representations>
            <FeatureValue feature="featPlace" symbolValues="symBack" />
          </SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses>
        <SegmentNaturalClass id="ncStop"><Name>Stop</Name><Segment segment="cStop" /></SegmentNaturalClass>
        <FeatureNaturalClass id="ncFront"><Name>Front</Name>
          <FeatureValue feature="featPlace" symbolValues="symFront" />
        </FeatureNaturalClass>
        <FeatureNaturalClass id="ncBack"><Name>Back</Name>
          <FeatureValue feature="featPlace" symbolValues="symBack" />
        </FeatureNaturalClass>
      </NaturalClasses>
      <PhonologicalRuleDefinitions>
        <PhonologicalRule id="prNoOverlap" multipleApplicationOrder="simultaneous"><Name>noOverlap</Name>
          <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
              <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncFront" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
            </PhonologicalSubrule>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
              <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="prOverlap" multipleApplicationOrder="simultaneous"><Name>overlap</Name>
          <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
      </PhonologicalRuleDefinitions>
    </Language></HermitCrabInput>"#;

    fn rewrite_rule<'g>(
        g: &'g pg_grammar::model::Grammar,
        xml_id: &str,
    ) -> &'g pg_grammar::model::RewriteRuleDef {
        for pr in &g.prules {
            if let PhonRuleDef::Rewrite(r) = pr {
                if r.xml_id == xml_id {
                    return r;
                }
            }
        }
        panic!("rewrite rule {xml_id:?} not found");
    }

    /// Two subrules whose RIGHT environments are mutually exclusive natural classes (`Front` vs.
    /// `Back`, no overlapping segments) must lower to spans whose focus+right intersection is
    /// EMPTY -- they cannot both hold at the same position.
    #[test]
    fn lower_span_disjoint_right_environments_do_not_overlap() {
        let g = load(OVERLAP_LOWER_PROBE_XML);
        let r = rewrite_rule(&g, "prNoOverlap");
        assert_eq!(r.mode, RewriteMode::Simultaneous);
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();

        let span_a = lower_span(
            &opts,
            &g,
            &alphabet,
            r.subrules[0].left_env.as_ref(),
            &r.lhs,
            r.subrules[0].right_env.as_ref(),
        )
        .expect("prNoOverlap subrule 0 must lower (no unsupported nodes)");
        let span_b = lower_span(
            &opts,
            &g,
            &alphabet,
            r.subrules[1].left_env.as_ref(),
            &r.lhs,
            r.subrules[1].right_env.as_ref(),
        )
        .expect("prNoOverlap subrule 1 must lower (no unsupported nodes)");

        assert!(
            !spans_overlap(&opts, &span_a, &span_b),
            "Front/Back-flanked subrules must NOT overlap"
        );
    }

    /// Two subrules with IDENTICAL (unconstrained) focus/environment lower to the SAME span --
    /// their intersection is trivially non-empty.
    #[test]
    fn lower_span_identical_unconstrained_subrules_overlap() {
        let g = load(OVERLAP_LOWER_PROBE_XML);
        let r = rewrite_rule(&g, "prOverlap");
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();

        let span_a = lower_span(
            &opts,
            &g,
            &alphabet,
            r.subrules[0].left_env.as_ref(),
            &r.lhs,
            r.subrules[0].right_env.as_ref(),
        )
        .expect("prOverlap subrule 0 must lower");
        let span_b = lower_span(
            &opts,
            &g,
            &alphabet,
            r.subrules[1].left_env.as_ref(),
            &r.lhs,
            r.subrules[1].right_env.as_ref(),
        )
        .expect("prOverlap subrule 1 must lower");

        assert!(
            spans_overlap(&opts, &span_a, &span_b),
            "two unconstrained same-focus subrules must overlap"
        );
    }
}
