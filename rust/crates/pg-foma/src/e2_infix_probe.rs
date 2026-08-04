//! A standalone feasibility probe, exercised only by this module's own tests/examples: does
//! splicing Amharic's `Role::Infix` rules
//! (root-and-pattern interdigitation: `-ipfv-`/`-conv-`/`-pfv-`) into UNDERLYING token-space
//! composite entries — via the plain, non-recursive `pg_rules::morph::synthesize`, not
//! `crate::preexpand`'s real-phonology probe — then composing with the already-proven replace-rule
//! cascade ([`crate::replace`]), reach 100% propose recall on Amharic's corpus?
//!
//! ## Why this question needed asking
//! No earlier prototype exercises Amharic's actual
//! morphotactics end-to-end for recall. A census run against the real engine oracle
//! (`examples/e2_amharic_census.rs`, `examples/e2_template_census.rs`) found: Amharic's 3 Infix
//! rules sit in ZERO template slots (pure standalone rules — `emit.rs`'s own `build_deriv_chain`
//! cannot place them in a Prefix or Suffix zone either) and, on a 300-word corpus sample, 43/79
//! (54%) of engine-analyzed words have an analysis that NEEDS one, with NO non-infix escape route
//! for any of those 43. Losing them is not an acceptable "Partial tier" outcome (plan's recall gate
//! is 100%, zero losses) — so this module exists to test, empirically, whether a bounded,
//! non-recursive splice mechanism can close that gap without reintroducing the O(roots ×
//! rules^depth) enumeration `crate::preexpand`'s own module doc names as the exact problem this
//! whole effort (P6/E2) exists to retire.
//!
//! ## Design
//! Mirrors `emit.rs`'s structural topology (root/template-group/slot-chain/deriv-chain wiring,
//! Composites/CompositeExit sharing) but LEAN and UNDERLYING-mode-only: junction/composite-fusion/
//! precision machinery is unconditionally absent (real phonology is the downstream replace-rule
//! cascade's job here, not this emitter's — advisor guidance recorded in the E2 build session:
//! "turned OFF in underlying mode, not refit"). Leaf text is
//! [`crate::replace::SegAlphabet::encode_shape`] (one PUA token per char-def; no
//! representation-variant cartesian product needed at all — token space already collapses that
//! dimension, `crate::replace`'s own module doc). This is a fresh, self-contained implementation
//! (not a refactor of `emit.rs`) precisely because it must stay decoupled from `emit.rs`'s mainline
//! behavior while the GO/NO-GO call is still open — see the E2 task's own log for why touching
//! `emit.rs` itself was deferred to the (not-yet-authorized) mainline build step.
//!
//! ## The Infix splice mechanism ([`build_infix_composites`])
//! For each (root allomorph × Infix rule) pair: seed a `pg_rules::word::Word` EXACTLY the way
//! `crate::preexpand::process_root_work` does (feature-bearing re-segmentation via
//! `pg_rules::shape_feat::segment_with_features`, the entry's own REAL `syn_fs`/`mpr` — never
//! `FeatureStruct::EMPTY`, the documented `crate::emit::probe_surface` blind spot this module
//! deliberately avoids by construction), call `pg_rules::morph::synthesize` (the plain sibling of
//! what `preexpand.rs` uses — no phonological cascade, no `RuleCache`, no big-stack thread: this
//! runs entirely in underlying/shape space, no surface to resolve), iterate over EVERY returned
//! `Word` (ambiguous-match fan-out — `preexpand.rs`'s own `extend` does the same, `for w in
//! synthesize_cached(..)`, never just the first), and encode each resulting `.shape` via
//! `SegAlphabet::encode_shape` as one composite lexc entry feeding the SAME shared
//! `Composites`/`CompositeExit` continuation ordinary roots use — so an interdigitated stem can
//! still take further prefixes/suffixes through the ordinary (replace-rule-cascaded) concatenative
//! path. This is O(roots × infix-rule-count), one non-recursive `synthesize` call per pair — NOT
//! the O(roots × rules^depth) recursive chaining `preexpand.rs` needs for boundary fusion (which
//! needs the phonological probe to resolve a SURFACE spelling; underlying mode has none to
//! resolve).

use std::collections::BTreeSet;

use pg_featstruct::is_unifiable;
use pg_grammar::model::{
    AffixAllomorphDef, AffixTemplateDef, AllomorphId, Grammar, MRuleId, MorphRuleDef, MorphemeId,
    OutputAction, PartRef, SlotDef,
};
use pg_rules::word::{MorphRecord, Word};

use crate::emit::{allomorphs_of, classify_affix, owning_morpheme, rule_role, Role};
use crate::replace::SegAlphabet;
use crate::tags;

pub struct UProbeResult {
    pub lexc_source: String,
    /// Non-fatal skip reports (mirrors `emit.rs`'s `uncovered` convention loosely — this is a probe,
    /// so it's a flat diagnostic list, not a structured `UncoveredItem`).
    pub uncovered: Vec<String>,
    pub root_count: usize,
    /// How many mrules [`special_rules_u`] routed into the splice mechanism at all (Infix +
    /// structural/truncating + process-morph rules — module doc there).
    pub special_rule_count: usize,
    /// [`build_splice_composites`]'s own output counts: composite entries emitted, (word, rule)
    /// pairs actually attempted (after the required-FS pre-filter, at every recursion depth), and
    /// how many of those pairs produced MORE than one `Word` from `synthesize` (ambiguous-match
    /// fan-out — the E2 build session's risk #2).
    pub splice_composite_count: usize,
    pub splice_pairs_probed: usize,
    pub splice_ambiguous_pairs: usize,
}

fn write_lexicon_header(out: &mut String, name: &str) {
    out.push_str("\nLEXICON ");
    out.push_str(name);
    out.push('\n');
}

fn write_bare(out: &mut String, continuation: &str, lines: &mut usize) {
    out.push_str(continuation);
    out.push_str(" ;\n");
    *lines += 1;
}

/// One tagged entry: upper = tag symbol only, lower = underlying token text (already PUA
/// codepoints — never collide with lexc's ASCII-only special chars, so no escaping is needed here,
/// unlike `emit.rs::escape_lexc_text`'s literal-surface-text case).
fn write_tag_entry(
    out: &mut String,
    tag_lexc: &str,
    underlying: &str,
    continuation: &str,
    lines: &mut usize,
) {
    out.push_str(tag_lexc);
    out.push(':');
    if underlying.is_empty() {
        out.push('0');
    } else {
        out.push_str(underlying);
    }
    out.push(' ');
    out.push_str(continuation);
    out.push_str(" ;\n");
    *lines += 1;
}

struct RootRecU {
    morpheme: MorphemeId,
    underlying: String,
}

fn collect_roots_u(
    g: &Grammar,
    alphabet: &SegAlphabet<'_>,
    uncovered: &mut Vec<String>,
) -> Vec<RootRecU> {
    let mut roots = Vec::new();
    for sd in &g.strata {
        for &entry_id in &sd.entries {
            let entry = &g.entries[entry_id.0 as usize];
            for (allo_idx, allo) in entry.allomorphs.iter().enumerate() {
                if allo.is_pattern {
                    uncovered.push(format!(
                        "entry{}#allo{allo_idx} pattern-allomorph (skipped)",
                        entry_id.0
                    ));
                    continue;
                }
                let underlying = alphabet.encode_shape(&allo.shape.shape);
                roots.push(RootRecU {
                    morpheme: entry.morpheme,
                    underlying,
                });
            }
        }
    }
    roots
}

// Internal emitter helper: `out`/`uncovered`/`lines` are accumulators threaded through this
// probe's whole emission pass (the same grouping every sibling emit fn in this file uses), and
// `g`/`alphabet`/`mid`/`zone_role`/`width`/`next` are the per-call context needed to emit one
// rule's allomorphs. Splitting these into a struct would touch both call sites for no behavior
// change, so the lint is silenced here rather than "fixed".
#[allow(clippy::too_many_arguments)]
fn emit_rule_allomorphs_u(
    out: &mut String,
    g: &Grammar,
    alphabet: &SegAlphabet<'_>,
    mid: MRuleId,
    zone_role: Role,
    width: usize,
    next: &str,
    uncovered: &mut Vec<String>,
    lines: &mut usize,
) {
    let morpheme = owning_morpheme(g, mid);
    let tag_lexc = tags::morph_tag_lexc(morpheme, width);
    for (allo_idx, allo) in allomorphs_of(g, mid).iter().enumerate() {
        let label = format!("mrule{}#allo{allo_idx}", mid.0);
        if allo.rhs.iter().any(|a| {
            matches!(
                a,
                OutputAction::Modify(_, _) | OutputAction::InsertContext(_)
            )
        }) {
            uncovered.push(format!("{label} process-morph (skipped)"));
            continue;
        }
        let role = classify_affix(&allo.rhs);
        if role != zone_role && role != Role::None {
            uncovered.push(format!(
                "{label} role={role:?} zone={zone_role:?} mismatch (skipped)"
            ));
            continue;
        }
        let insert_shape = allo.rhs.iter().find_map(|a| match a {
            OutputAction::InsertSegments { shape, .. } => Some(shape),
            _ => None,
        });
        match insert_shape {
            None => write_tag_entry(out, &tag_lexc, "", next, lines),
            Some(seg_text) => {
                let underlying = alphabet.encode_shape(&seg_text.shape);
                write_tag_entry(out, &tag_lexc, &underlying, next, lines);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_deriv_chain_u(
    out: &mut String,
    g: &Grammar,
    alphabet: &SegAlphabet<'_>,
    prefix: &str,
    zone_role: Role,
    rules: &[MRuleId],
    width: usize,
    exit: &str,
    uncovered: &mut Vec<String>,
    lines: &mut usize,
) -> String {
    let entry_name = format!("{prefix}0");
    if rules.is_empty() {
        write_lexicon_header(out, &entry_name);
        write_bare(out, exit, lines);
        return entry_name;
    }
    let depth = rules.len().max(2); // DERIV_DEPTH_MIN, emit.rs's own floor.
    for level in 0..depth {
        let name = format!("{prefix}{level}");
        write_lexicon_header(out, &name);
        let is_final = level + 1 == depth;
        let next = if is_final {
            exit.to_string()
        } else {
            format!("{prefix}{}", level + 1)
        };
        write_bare(out, &next, lines);
        for &mid in rules {
            emit_rule_allomorphs_u(
                out, g, alphabet, mid, zone_role, width, &next, uncovered, lines,
            );
        }
    }
    entry_name
}

fn slot_role_u(g: &Grammar, slot: &SlotDef) -> Role {
    let mut has_zero = false;
    for &mid in &slot.rules {
        if matches!(g.mrules[mid.0 as usize], MorphRuleDef::Compounding(_)) {
            continue;
        }
        let role = rule_role(g, mid);
        if role == Role::Prefix || role == Role::Suffix {
            return role;
        }
        if role == Role::None {
            has_zero = true;
        }
    }
    if has_zero {
        Role::Suffix
    } else {
        Role::None
    }
}

fn classify_template_u<'g>(
    g: &'g Grammar,
    template: &'g AffixTemplateDef,
) -> (Vec<&'g SlotDef>, Vec<&'g SlotDef>) {
    let mut prefix = Vec::new();
    let mut suffix = Vec::new();
    for slot in &template.slots {
        match slot_role_u(g, slot) {
            Role::Prefix => prefix.push(slot),
            Role::Suffix => suffix.push(slot),
            _ => {}
        }
    }
    prefix.reverse();
    (prefix, suffix)
}

fn required_category_u(g: &Grammar, mid: MRuleId) -> pg_featstruct::FsId {
    match &g.mrules[mid.0 as usize] {
        MorphRuleDef::AffixProcess(def) => def.required_syn_fs,
        MorphRuleDef::Realizational(def) => def.required_syn_fs,
        MorphRuleDef::Compounding(_) => pg_featstruct::FsId(0),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_slot_chain_u(
    out: &mut String,
    g: &Grammar,
    alphabet: &SegAlphabet<'_>,
    prefix: &str,
    slots: &[&SlotDef],
    zone_role: Role,
    template_category: pg_featstruct::FsId,
    width: usize,
    exit: &str,
    uncovered: &mut Vec<String>,
    lines: &mut usize,
) -> String {
    let entry_name = format!("{prefix}0");
    if slots.is_empty() {
        write_lexicon_header(out, &entry_name);
        write_bare(out, exit, lines);
        return entry_name;
    }
    for (si, slot) in slots.iter().enumerate() {
        let name = format!("{prefix}{si}");
        write_lexicon_header(out, &name);
        let next = if si + 1 == slots.len() {
            exit.to_string()
        } else {
            format!("{prefix}{}", si + 1)
        };
        if slot.optional {
            write_bare(out, &next, lines);
        }
        for &mid in &slot.rules {
            if matches!(g.mrules[mid.0 as usize], MorphRuleDef::Compounding(_)) {
                continue;
            }
            let req = required_category_u(g, mid);
            let req_fs = g.fs_interner.get(req);
            if !req_fs.is_empty() {
                let tmpl_fs = g.fs_interner.get(template_category);
                if !is_unifiable(tmpl_fs, req_fs) {
                    continue;
                }
            }
            emit_rule_allomorphs_u(
                out, g, alphabet, mid, zone_role, width, &next, uncovered, lines,
            );
        }
    }
    entry_name
}

/// Replays `pg_parse::Morpher::allomorphs_in_morph_order`'s own algorithm — same as
/// `crate::preexpand`'s private `morph_order_tags` (re-derived here for the same "can't import a
/// private fn from another module" reason; kept intentionally byte-for-byte identical in shape).
fn morph_order_tags_u(w: &Word, known: &[(MorphemeId, String)]) -> Option<String> {
    let mut ms = w.morphs.clone();
    ms.sort_by_key(|m| m.order);
    let mut seen: Vec<AllomorphId> = Vec::new();
    let mut out = String::new();
    for m in ms {
        if seen.contains(&m.allomorph) {
            continue;
        }
        seen.push(m.allomorph);
        match known.iter().find(|(mid, _)| *mid == m.morpheme) {
            Some((_, tag)) => out.push_str(tag),
            None => return None,
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Mirrors `crate::emit::rhs_drops_lhs_material` exactly (that fn is private to `emit.rs`, not even
/// `pub(crate)` — re-derived here rather than widening its visibility, same "probe stays decoupled
/// from mainline" reasoning as the rest of this module): does `allo`'s RHS drop at least one of its
/// own LHS's top-level parts — i.e. some `PartRef::Input(i)` in `0..allo.lhs.len()` never appears as
/// an `OutputAction::Copy` anywhere in `allo.rhs`? Amharic's mrule3/"to" is exactly this shape: LHS
/// has 2 parts (a fixed onset segment, then "the rest"), RHS copies only the second part — the
/// onset is consumed by the match but never copied into the output, replaced by the inserted "ላ".
fn rhs_drops_lhs_material_u(a: &AffixAllomorphDef) -> bool {
    if a.lhs.len() <= 1 {
        return false;
    }
    let copied: BTreeSet<u16> = a
        .rhs
        .iter()
        .filter_map(|act| match act {
            OutputAction::Copy(PartRef::Input(i)) => Some(*i),
            _ => None,
        })
        .collect();
    (0..a.lhs.len() as u16).any(|i| !copied.contains(&i))
}

/// Mirrors `crate::emit::is_structural_rule` (same visibility reasoning as
/// [`rhs_drops_lhs_material_u`]). Scoped to `Role::None`/`Prefix`/`Suffix` (an `Infix` rule is
/// always in [`special_rules_u`]'s set on its own account; `Role::CircumfixPrefix` is
/// unconditionally structural per emit.rs's doc, but Amharic has zero of those, verified by the E2
/// census — kept for parity, costs nothing to check).
fn is_structural_rule_u(g: &Grammar, mid: MRuleId) -> bool {
    match rule_role(g, mid) {
        Role::None | Role::Prefix | Role::Suffix => {
            allomorphs_of(g, mid).iter().any(rhs_drops_lhs_material_u)
        }
        Role::CircumfixPrefix => true,
        _ => false,
    }
}

/// Mirrors `crate::emit`'s `has_unemittable_action` (same visibility reasoning): an allomorph whose
/// RHS carries a `Modify`/`InsertContext` action has NO literal text at all — `plan §2`'s "not
/// compilable as strings". Dispositive finding (E2 build session, `e2_mainline_check` probe):
/// mainline's ACTUAL production path (`crate::preexpand`, preexpand ON) covers Amharic's mrule31
/// ("conv.1.sg", a `Modify`/ablaut rule) via exactly this route — `synthesize` correctly EXECUTES a
/// `Modify` action (it changes the real `Shape`/feature identity, it doesn't need a literal string
/// at all), the limitation is only in `crate::emit`'s STATIC leaf (`emit_rule_allomorphs`'s
/// insert-text-only leaf), not in the underlying mechanism. So this rule is NOT a pre-existing,
/// architecture-independent gap the way the E2 build session first assumed from a doc comment alone
/// — it is squarely in scope for the SAME splice mechanism as Infix/structural rules, and dropping
/// it would be a real recall regression relative to mainline, not a deferrable gap.
fn has_unemittable_action_u(a: &AffixAllomorphDef) -> bool {
    a.rhs.iter().any(|act| {
        matches!(
            act,
            OutputAction::Modify(_, _) | OutputAction::InsertContext(_)
        )
    })
}

/// Bound on a splice chain's length beyond the root — same rationale/value as
/// `crate::preexpand::MAX_EXTRA_RULES` (module doc there): Amharic's "ሰብሬ" needs depth 2 (Infix
/// mrule6 "-conv-", then the Modify-bearing mrule31 "conv.1.sg" — itself gated on mrule6's own
/// `out_syn_fs`, i.e. it can ONLY ever attach to an already-formed conv-stem, never to a bare root
/// directly); 3 leaves the same headroom `preexpand.rs`'s own constant does.
const SPLICE_MAX_EXTRA_RULES: usize = 3;

/// Every mrule that needs the splice mechanism at all — Infix (root-and-pattern, module doc),
/// structural/truncating (`is_structural_rule_u`), or process-morph (`has_unemittable_action_u`,
/// the "ሰብሬ" finding above). **Deliberately NOT `crate::preexpand::candidate_rules`'s full
/// Prefix/Suffix/Infix set** — that set is EVERY affix rule in the grammar (~85 of Amharic's 88
/// mrules), and recursively chaining depth 3 over a set that large is exactly the O(roots ×
/// rules^depth) enumeration this whole effort exists to retire (`preexpand.rs`'s own module doc
/// names it explicitly). The insight this module's own recall-gate investigation surfaced: an
/// ORDINARY Prefix/Suffix rule (the vast majority) is already correctly representable by the plain
/// concatenative deriv-chain/slot-chain leaf (`emit_rule_allomorphs_u`) and REACHES this splice
/// mechanism's own composite output for free via the shared `Composites`/`CompositeExit`
/// continuation (exactly how "ሰብሬ"'s final ordinary suffix, poss.1s/mrule75, attaches after the
/// depth-2 special-rule composite) — so chaining only needs to range over the SMALL, per-grammar
/// set of genuinely non-concatenative rules, never the full rule inventory. This keeps the
/// recursion's own branching factor bounded by a near-constant (a handful of rules) rather than
/// growing with the grammar's overall rule count, which is the actual scaling property that
/// matters for "build for full-scale grammars".
fn special_rules_u(g: &Grammar) -> Vec<MRuleId> {
    (0..g.mrules.len() as u32)
        .map(MRuleId)
        .filter(|&mid| {
            if matches!(g.mrules[mid.0 as usize], MorphRuleDef::Compounding(_)) {
                return false;
            }
            let role = rule_role(g, mid);
            if role == Role::Infix {
                return true;
            }
            if !matches!(role, Role::None | Role::Prefix | Role::Suffix) {
                return false; // Reduplication/Process/CircumfixSuffix: out of this probe's scope.
            }
            allomorphs_of(g, mid)
                .iter()
                .any(|a| rhs_drops_lhs_material_u(a) || has_unemittable_action_u(a))
                || is_structural_rule_u(g, mid)
        })
        .collect()
}

/// Bound on [`encode_shape_variants`]'s cartesian product — mirrors
/// `crate::preexpand::MAX_RENDER_VARIANTS` (same value, same rationale: that module's own doc
/// measured Ge'ez vowel-quality ambiguity at ~30% of all probed segments on Amharic, "the ORDINARY
/// case for this templatic language family, not a rare exception", so the cap must stay SMALL, not
/// generous).
const SPLICE_MAX_RENDER_VARIANTS: usize = 4;

/// Every token-space rendering of a synthesized `Shape` — handles the common case (every node still
/// concretely identified: one token each, the fast path) AND the fallback case a `Modify`/ablaut
/// action can produce: `crate::preexpand`'s own module doc, "a post-rewrite node whose identity was
/// cleared by a feature-changing rule" (`char_def == pg_shape::NO_CHAR_DEF`), or a node whose OWN
/// char-def no longer unifies with its current (rewritten) lanes. In that case there is no single
/// preferred token — fall through to every table `Segment` char-def whose feature lanes unify with
/// the node's CURRENT lanes (mirrors `crate::preexpand::matching_reps_local` exactly, but in TOKEN
/// space rather than literal-representation space), cartesian-producting across every such
/// ambiguous position, capped at [`SPLICE_MAX_RENDER_VARIANTS`].
///
/// **How this was found**: mrule31's `ModifyFromInput` (a `Suffix`-classified rule that changes the
/// stem's own final-consonant identity — gemination/ablaut) produces exactly this shape once
/// spliced via `pg_rules::morph::synthesize`. The ORIGINAL, ungated `SegAlphabet::encode_shape`
/// PANICS on it (`replace.rs`'s own overflow guard firing on `NO_CHAR_DEF`'s `u32::MAX` sentinel,
/// "char table too large for the PUA token scheme") rather than silently mis-rendering — which is
/// how this gap surfaced loudly instead of passing quietly into a wrong lexc entry.
fn encode_shape_variants(alphabet: &SegAlphabet<'_>, shape: &pg_shape::Shape) -> Vec<String> {
    let table = alphabet.table();
    let mut variants: Vec<String> = vec![String::new()];
    for (i, _, char_def, _) in shape.interior() {
        let lanes = shape.node_lanes(i);
        let fast_path = if char_def != pg_shape::NO_CHAR_DEF {
            let cd = table.get(pg_grammar::chardef::CharDefId(char_def));
            if pg_featstruct::flat_unifiable(lanes, cd.feature_lanes()) {
                Some(alphabet.token(pg_grammar::chardef::CharDefId(char_def)))
            } else {
                None
            }
        } else {
            None
        };
        let toks: Vec<char> = match fast_path {
            Some(t) => vec![t],
            None => {
                let mut ts = Vec::new();
                for (id, cd) in table.iter() {
                    if cd.kind() != pg_grammar::chardef::CharDefKind::Segment {
                        continue;
                    }
                    if pg_featstruct::flat_unifiable(lanes, cd.feature_lanes()) {
                        ts.push(alphabet.token(id));
                    }
                }
                ts
            }
        };
        if toks.is_empty() {
            // No matching char-def at all (defensive; shouldn't happen for a real `synthesize`
            // result) -- drop this variant set entirely rather than emitting truncated/wrong text.
            return Vec::new();
        }
        let mut next = Vec::with_capacity(variants.len() * toks.len());
        'grow: for v in &variants {
            for &t in &toks {
                if next.len() >= SPLICE_MAX_RENDER_VARIANTS {
                    break 'grow;
                }
                let mut nv = v.clone();
                nv.push(t);
                next.push(nv);
            }
        }
        variants = next;
    }
    variants
}

/// Recursive splice chaining (mirrors `crate::preexpand::extend`'s shape, minus the phonological
/// probe/redundancy-check machinery — module doc: underlying-token mode has no surface to resolve,
/// so there is nothing for a probe to do, and no "already reachable via the ordinary path" check to
/// make since every composite this emits is a pure ADDITION, never a replacement, of what the
/// ordinary emission already writes — upward-safe per the plan's iron rule regardless of whether it
/// duplicates an ordinary-path candidate). Emits a composite at EVERY successful step (not just
/// "dirty" steps the way `preexpand.rs` optimizes for entry-count — this probe doesn't need that
/// optimization, and always-emit is simpler and still upward-safe), then recurses through `rules`
/// again (skipping any rule already in `chain`, same `multipleApplication = 1` default guard
/// `preexpand.rs` uses) up to [`SPLICE_MAX_EXTRA_RULES`].
#[allow(clippy::too_many_arguments)]
fn splice_extend_u(
    g: &Grammar,
    alphabet: &SegAlphabet<'_>,
    rules: &[MRuleId],
    base_word: &Word,
    chain: &[(MorphemeId, String)],
    depth: usize,
    width: usize,
    out: &mut Vec<(String, String)>,
    pairs_probed: &mut usize,
    ambiguous_pairs: &mut usize,
) {
    if depth >= SPLICE_MAX_EXTRA_RULES {
        return;
    }
    let base_fs = base_word.syn_fs.clone();
    for &mid in rules {
        let rule = &g.mrules[mid.0 as usize];
        let (req, rule_morpheme) = match rule {
            MorphRuleDef::AffixProcess(def) => (def.required_syn_fs, def.morpheme),
            MorphRuleDef::Realizational(def) => (def.required_syn_fs, def.morpheme),
            MorphRuleDef::Compounding(_) => continue,
        };
        if chain.iter().any(|(m, _)| *m == rule_morpheme) {
            continue;
        }
        let req_fs = g.fs_interner.get(req);
        if !req_fs.is_empty() && !is_unifiable(req_fs, &base_fs) {
            continue;
        }
        *pairs_probed += 1;
        let rule_tag = tags::morph_tag_lexc(rule_morpheme, width);
        let mut next_chain = chain.to_vec();
        next_chain.push((rule_morpheme, rule_tag));

        let results = pg_rules::morph::synthesize(g, base_word, rule);
        if results.len() > 1 {
            *ambiguous_pairs += 1;
        }
        for w in &results {
            let underlying_variants = encode_shape_variants(alphabet, &w.shape);
            if let Some(tag_lexc) = morph_order_tags_u(w, &next_chain) {
                for underlying in &underlying_variants {
                    out.push((tag_lexc.clone(), underlying.clone()));
                }
            }
            splice_extend_u(
                g,
                alphabet,
                rules,
                w,
                &next_chain,
                depth + 1,
                width,
                out,
                pairs_probed,
                ambiguous_pairs,
            );
        }
    }
}

/// The splice mechanism's outer loop: seed a real-`syn_fs` `Word` per root allomorph (same
/// convention `crate::preexpand::process_root_work` uses), then recurse via [`splice_extend_u`]
/// over [`special_rules_u`]'s small candidate set. Returns `(composites, pairs_probed,
/// ambiguous_pairs)`.
fn build_splice_composites(
    g: &Grammar,
    alphabet: &SegAlphabet<'_>,
    splice_rules: &[MRuleId],
    width: usize,
) -> (Vec<(String, String)>, usize, usize) {
    let mut out = Vec::new();
    let mut pairs_probed = 0usize;
    let mut ambiguous_pairs = 0usize;
    if splice_rules.is_empty() {
        return (out, pairs_probed, ambiguous_pairs);
    }
    for sd in &g.strata {
        for &entry_id in &sd.entries {
            let entry = &g.entries[entry_id.0 as usize];
            let root_stratum = g.morphemes[entry.morpheme.0 as usize].stratum;
            let table_id = g.strata[root_stratum.0 as usize].table.0;
            let root_table = &g.char_tables[table_id as usize];
            let entry_fs = g.fs_interner.get(entry.syn_fs);
            for allo in &entry.allomorphs {
                if allo.is_pattern {
                    continue;
                }
                let Ok(shape) =
                    pg_rules::shape_feat::segment_with_features(g, root_table, &allo.shape.text)
                else {
                    continue;
                };
                let mut word = Word::new(shape, root_stratum);
                word.syn_fs = entry_fs.clone();
                word.mpr = entry.mpr;
                word.root_allomorph = Some(allo.id);
                word.morphs = vec![MorphRecord::new(allo.id, entry.morpheme, 0)];
                let root_tag = tags::root_tag_lexc(entry.morpheme, width);
                let chain0 = vec![(entry.morpheme, root_tag)];
                splice_extend_u(
                    g,
                    alphabet,
                    splice_rules,
                    &word,
                    &chain0,
                    0,
                    width,
                    &mut out,
                    &mut pairs_probed,
                    &mut ambiguous_pairs,
                );
            }
        }
    }
    (out, pairs_probed, ambiguous_pairs)
}

/// Emit the full underlying-form lexc source for `g` (module doc). Written specifically against
/// Amharic's actual grammar shape (verified: `has_compounding_rules` is `true`, which makes
/// `emit.rs`'s own per-group root-eligibility filter collapse to "admit every root to every
/// group" for THIS grammar — replicated directly here rather than re-deriving the general filter,
/// since a probe should match what mainline actually does for this grammar, not build unexercised
/// generality). Compounding's own extra-root loop (`TLCmp`/`G{gi}Cmp`) is omitted here
/// deliberately: build minimal, look at misses, add exactly what's needed.
pub fn emit_underlying_amharic_probe(g: &Grammar, alphabet: &SegAlphabet<'_>) -> UProbeResult {
    let width = tags::tag_width(g.morphemes.len());
    let mut uncovered: Vec<String> = Vec::new();
    let mut lines = 0usize;

    let roots = collect_roots_u(g, alphabet, &mut uncovered);

    let mut deriv_prefix: Vec<MRuleId> = Vec::new();
    let mut deriv_suffix: Vec<MRuleId> = Vec::new();
    for sd in &g.strata {
        for &mid in &sd.mrules {
            if matches!(g.mrules[mid.0 as usize], MorphRuleDef::Compounding(_)) {
                continue;
            }
            match rule_role(g, mid) {
                // Ordinary literal-text zones (module doc: superset — a structural/process-morph
                // rule ALSO routed here gets a (harmless, never-matching) wrong entry in addition
                // to its correct `special_rules_u`-driven composite; mirrors `crate::emit`'s own
                // "deliberate supersets" convention exactly).
                Role::Prefix => deriv_prefix.push(mid),
                Role::Suffix => deriv_suffix.push(mid),
                Role::None => {
                    deriv_prefix.push(mid);
                    deriv_suffix.push(mid);
                }
                // Infix has no ordinary zone at all -- handled entirely by `special_rules_u` +
                // `build_splice_composites` below, never "uncovered" (it IS representable now).
                Role::Infix => {}
                other => uncovered.push(format!(
                    "mrule{} standalone role={other:?} not representable by this probe",
                    mid.0
                )),
            }
        }
    }

    let mut group_keys: Vec<pg_featstruct::FsId> = Vec::new();
    let mut group_templates: Vec<Vec<usize>> = Vec::new();
    for (ti, t) in g.templates.iter().enumerate() {
        match group_keys.iter().position(|&k| k == t.required_syn_fs) {
            Some(gi) => group_templates[gi].push(ti),
            None => {
                group_keys.push(t.required_syn_fs);
                group_templates.push(vec![ti]);
            }
        }
    }

    let has_template_less_section = !deriv_prefix.is_empty() || !deriv_suffix.is_empty();
    let has_templates = !g.templates.is_empty();

    // The splice mechanism's own candidate set (module doc on `special_rules_u`): Infix +
    // structural/truncating + process-morph rules ONLY — a small, near-constant-size subset of
    // the grammar's total rule count, never the full Prefix/Suffix/Infix inventory
    // `crate::preexpand::candidate_rules` uses (that set recursively chained to depth 3 would
    // reintroduce the O(roots × rules^depth) enumeration this whole effort exists to retire).
    let special_rules = special_rules_u(g);
    let (composites, splice_pairs_probed, splice_ambiguous_pairs) =
        build_splice_composites(g, alphabet, &special_rules, width);
    let has_composites = !composites.is_empty();

    let mut out = String::new();

    let mut symbols: BTreeSet<(bool, u32)> = BTreeSet::new();
    for r in &roots {
        symbols.insert((true, r.morpheme.0));
    }
    for &mid in deriv_prefix
        .iter()
        .chain(deriv_suffix.iter())
        .chain(special_rules.iter())
    {
        symbols.insert((false, owning_morpheme(g, mid).0));
    }
    for t in &g.templates {
        for slot in &t.slots {
            for &mid in &slot.rules {
                if !matches!(g.mrules[mid.0 as usize], MorphRuleDef::Compounding(_)) {
                    symbols.insert((false, owning_morpheme(g, mid).0));
                }
            }
        }
    }
    out.push_str("Multichar_Symbols\n");
    for &(is_root, id) in &symbols {
        let lexc = if is_root {
            tags::root_tag_lexc(MorphemeId(id), width)
        } else {
            tags::morph_tag_lexc(MorphemeId(id), width)
        };
        out.push_str(&lexc);
        out.push('\n');
    }

    write_lexicon_header(&mut out, "Root");
    for r in &roots {
        let tag_lexc = tags::root_tag_lexc(r.morpheme, width);
        write_tag_entry(&mut out, &tag_lexc, &r.underlying, "#", &mut lines);
    }
    if has_composites {
        write_bare(&mut out, "Composites", &mut lines);
    }
    if has_template_less_section {
        write_bare(&mut out, "TLPfx0", &mut lines);
    }
    if has_templates {
        write_bare(&mut out, "OuterPfx0", &mut lines);
    }

    if has_templates {
        build_deriv_chain_u(
            &mut out,
            g,
            alphabet,
            "OuterPfx",
            Role::Prefix,
            &deriv_prefix,
            width,
            "TmplDispatch",
            &mut uncovered,
            &mut lines,
        );
        write_lexicon_header(&mut out, "TmplDispatch");
        let mut dispatch_lines: BTreeSet<String> = BTreeSet::new();
        for (gi, tis) in group_templates.iter().enumerate() {
            for &ti in tis {
                let (prefix_slots, _) = classify_template_u(g, &g.templates[ti]);
                if prefix_slots.is_empty() {
                    dispatch_lines.insert(format!("G{gi}PfxD0"));
                } else {
                    dispatch_lines.insert(format!("T{ti}P0"));
                }
            }
        }
        for line in &dispatch_lines {
            write_bare(&mut out, line, &mut lines);
        }
        build_deriv_chain_u(
            &mut out,
            g,
            alphabet,
            "OuterSfx",
            Role::Suffix,
            &deriv_suffix,
            width,
            "#",
            &mut uncovered,
            &mut lines,
        );
    }

    if has_template_less_section {
        build_deriv_chain_u(
            &mut out,
            g,
            alphabet,
            "TLPfx",
            Role::Prefix,
            &deriv_prefix,
            width,
            "TLRoots",
            &mut uncovered,
            &mut lines,
        );
        write_lexicon_header(&mut out, "TLRoots");
        for r in &roots {
            let tag_lexc = tags::root_tag_lexc(r.morpheme, width);
            write_tag_entry(&mut out, &tag_lexc, &r.underlying, "TLPost", &mut lines);
        }
        if has_composites {
            write_bare(&mut out, "Composites", &mut lines);
        }
        write_lexicon_header(&mut out, "TLPost");
        write_bare(&mut out, "TLSfx0", &mut lines);
        build_deriv_chain_u(
            &mut out,
            g,
            alphabet,
            "TLSfx",
            Role::Suffix,
            &deriv_suffix,
            width,
            "#",
            &mut uncovered,
            &mut lines,
        );
    }

    for (gi, _key) in group_keys.iter().enumerate() {
        let mut join_lines: BTreeSet<String> = BTreeSet::new();
        for &ti in &group_templates[gi] {
            let template = &g.templates[ti];
            let (_, suffix_slots) = classify_template_u(g, template);
            if suffix_slots.is_empty() {
                join_lines.insert("OuterSfx0".to_string());
            } else {
                let entry = build_slot_chain_u(
                    &mut out,
                    g,
                    alphabet,
                    &format!("T{ti}Z"),
                    &suffix_slots,
                    Role::Suffix,
                    template.required_syn_fs,
                    width,
                    "OuterSfx0",
                    &mut uncovered,
                    &mut lines,
                );
                join_lines.insert(entry);
            }
        }
        let join_name = format!("G{gi}Join");
        write_lexicon_header(&mut out, &join_name);
        for line in &join_lines {
            write_bare(&mut out, line, &mut lines);
        }

        let sfx_deriv_entry = build_deriv_chain_u(
            &mut out,
            g,
            alphabet,
            &format!("G{gi}SfxD"),
            Role::Suffix,
            &deriv_suffix,
            width,
            &join_name,
            &mut uncovered,
            &mut lines,
        );

        let post_name = format!("G{gi}Post");
        write_lexicon_header(&mut out, &post_name);
        write_bare(&mut out, &sfx_deriv_entry, &mut lines);

        let roots_name = format!("G{gi}Roots");
        write_lexicon_header(&mut out, &roots_name);
        for r in &roots {
            let tag_lexc = tags::root_tag_lexc(r.morpheme, width);
            write_tag_entry(&mut out, &tag_lexc, &r.underlying, &post_name, &mut lines);
        }
        if has_composites {
            write_bare(&mut out, "Composites", &mut lines);
        }

        build_deriv_chain_u(
            &mut out,
            g,
            alphabet,
            &format!("G{gi}PfxD"),
            Role::Prefix,
            &deriv_prefix,
            width,
            &roots_name,
            &mut uncovered,
            &mut lines,
        );

        for &ti in &group_templates[gi] {
            let template = &g.templates[ti];
            let (prefix_slots, _) = classify_template_u(g, template);
            if prefix_slots.is_empty() {
                continue;
            }
            build_slot_chain_u(
                &mut out,
                g,
                alphabet,
                &format!("T{ti}P"),
                &prefix_slots,
                Role::Prefix,
                template.required_syn_fs,
                width,
                &format!("G{gi}PfxD0"),
                &mut uncovered,
                &mut lines,
            );
        }
    }

    if has_composites {
        write_lexicon_header(&mut out, "Composites");
        for (tag_lexc, underlying) in &composites {
            write_tag_entry(&mut out, tag_lexc, underlying, "CompositeExit", &mut lines);
        }
        write_lexicon_header(&mut out, "CompositeExit");
        write_bare(&mut out, "#", &mut lines);
        if has_template_less_section {
            write_bare(&mut out, "TLPost", &mut lines);
        }
        for gi in 0..group_keys.len() {
            write_bare(&mut out, &format!("G{gi}Post"), &mut lines);
        }
    }

    let _ = lines; // diagnostic only; not currently surfaced in UProbeResult.

    UProbeResult {
        lexc_source: out,
        uncovered,
        root_count: roots.len(),
        special_rule_count: special_rules.len(),
        splice_composite_count: composites.len(),
        splice_pairs_probed,
        splice_ambiguous_pairs,
    }
}
