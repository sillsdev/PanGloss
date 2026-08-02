//! P6 feasibility prototype (`docs/fst-plan/p6-prototype-report.md`): a FRESH, deliberately
//! minimal `Grammar -> lexc` emitter whose lower tape is UNDERLYING morph spellings (in
//! [`crate::replace::SegAlphabet`] token space — NOT surface spellings), meant to be composed
//! with [`crate::replace::compile_and_compose_rules`]'s rule cascade rather than pre-probing
//! junction phonology the way [`crate::emit`] does. This is NOT a refit of `emit.rs`: it does not
//! call [`crate::junctions`] or [`crate::preexpand`], and its morphotactic structure is
//! deliberately simpler (self-looping prefix/suffix chains rather than `emit.rs`'s rule-count-
//! bounded, template-aware derivation layers) — adequate for Indonesian's template-less
//! standalone-rule morphotactics (verified: zero `<AffixTemplate>` elements in
//! `indonesian-hc.xml`), not intended to generalize to a templated grammar (Sena/Amharic) as-is.
//!
//! It DOES reuse `emit.rs`'s already-tested, purely-classificatory `pub(crate)` helpers
//! ([`crate::emit::classify_affix`], [`crate::emit::Role`]) rather than re-deriving affix-role
//! classification a second time — those are queries about the grammar's OWN structure, unrelated
//! to surface-spelling/junction machinery, so reusing them is not the "refit all of emit.rs" the
//! prototype brief warns against.
//!
//! ## Tag convention (unchanged from mainline)
//! Same as `emit.rs`: every entry's upper side is `tags::root_tag_lexc`/`tags::morph_tag_lexc`
//! (a multichar tag symbol only); lower side is the morph's UNDERLYING text, encoded through
//! [`crate::replace::SegAlphabet`] (module doc there: char-def-identity tokens, not literal
//! spelling — so a multi-representation segment or a multi-grapheme unit needs no special
//! handling here at all).
//!
//! ## What's covered / skipped (reported, never silent)
//! - Root allomorphs: every non-pattern allomorph accepted bare (module doc "Deliberate
//!   supersets" convention `emit.rs` already uses — `is_pattern` allomorphs stay uncovered, zero
//!   in Indonesian).
//! - Affix allomorphs: classified via [`crate::emit::classify_affix`] on each allomorph's own RHS
//!   (not just the rule's first allomorph, matching `emit.rs`'s per-allomorph granularity).
//!   `Role::Prefix` → prefix chain; `Role::Suffix` → suffix chain. Everything else
//!   (`Reduplication`/`Infix`/`CircumfixPrefix`/`CircumfixSuffix`/`Process`/`None`) is skipped and
//!   reported — `emit.rs`'s own recall gate for this exact corpus (`f2_indonesian_gate.rs`) proves
//!   3 such allomorphs (2 reduplication-classified, all routed to its `uncovered` list) cost zero
//!   recall on these 121 words, which is why this prototype doesn't chase circumfix/null-morph
//!   support up front — the parity gate (this module's own driver) is the source of truth on
//!   whether that holds for the underlying-form path too.
//! - Compounding: **covered** as of the bounded compound loop below. Every
//!   [`MorphRuleDef::Realizational`] rule is still reported via `skipped` (never a silent drop, this
//!   doc's own heading) rather than enumerated — this module never attempts the syntactic
//!   feature-realization mechanism `RealizationalRuleDef` needs.
//!
//! ## Bounded compound loop
//! Until this loop existed, this module's continuation graph was structurally single-root — no arc
//! from at-or-after `RootBare` back to `RootBare`/`PrefixOrRoot` — so it could never propose a
//! compound no matter what a `CompoundingRuleDef` said, and
//! [`crate::enumerate::EmissionStrategy::PlanComposed`] (whose ONLY lexicon emitter is this module,
//! `crate::build::build_controllable`) proposed ZERO candidates for any compound word. Indonesian
//! declares compounding rules but the corpus (per `f2_indonesian_gate.rs`) needs none of them for
//! its 114-word (121 − 7 redup) recall denominator, which is why this prototype's own parity gate
//! never caught it; a corpus that DOES exercise compounding (Sena `ndimwe`, whose two analyses
//! differ only in which morpheme is the root; the `compounding-non-recursive` fixture's `fasubel`)
//! did.
//!
//! The graph now is:
//! ```text
//! Root -> PrefixOrRoot -> {PrefixChain | RootBare}
//! RootBare  -> <head-eligible root lines>     -> AfterRoot
//!           -> <head-INeligible root lines>   -> SuffixOrEnd
//! AfterRoot -> {SuffixOrEnd | UCmp}
//! UCmp{k}   -> UCmp{k}Pfx0 -> {UCmp{k}Roots | self-loop over the prefix inventory}
//! UCmp{k}Roots -> <grammar-wide licensed non-head root lines> -> SuffixOrEnd (k == levels)
//!                                                             -> UCmp{k}Next (otherwise)
//! UCmp{k}Next -> {SuffixOrEnd | UCmp{k+1}}
//! ```
//! **Unrolled to `levels`, never self-looped**, so depth stays bounded by construction —
//! `levels` comes from [`crate::emit::compound_extra_levels_checked`], the SAME
//! `capability::characterize`-derived bound and `HC_COMPOUND_CHAIN_DEPTH_BUDGET` check
//! `crate::emit`'s two emitters use, and the chain itself is
//! [`crate::emit::build_compound_chain`] — the one shared unroller, now with three callers rather
//! than a third hand-rolled copy (that function's own doc explains the generic seams that made the
//! reuse possible). Head/non-head eligibility is [`crate::emit::compound_license`], and the
//! head x non-head cross product is checked against `HC_COMPOUND_PAIR_BUDGET` before any of the
//! chain's lexc text is written.
//!
//! ### The non-head root set is GRAMMAR-WIDE, the bare-root lexicon stays partitioned
//! This module is called ONCE PER GATE-PARTITION GROUP (`crate::gate`'s static-partition design;
//! `crate::build::build_controllable` passes each group's own `allowed_entries`) and the per-group
//! networks are then `fsm_union`ed. `allowed_entries` therefore restricts the BARE-ROOT lexicon,
//! and must NOT restrict the compound levels: a compound whose head is in group A and whose
//! non-head is in group B is a path through NEITHER group's network if each group only offers its
//! own entries on both sides, so the union still cannot propose it. The compound levels are built
//! from `g.entries` directly, ignoring `allowed_entries` entirely, exactly as
//! `crate::emit::compound_license`'s own sets are grammar-scoped (`emit.rs`'s own
//! "Grammar-scoped, not per-rule" note). The HEAD side needs no such treatment — head eligibility
//! is a per-entry property applied to the lines this group already emits.
//!
//! Every non-head root tag reachable only through this grammar-wide set is additionally declared in
//! `Multichar_Symbols` (a tag absent from the declaration block would be read by lexc as its
//! individual characters, silently producing a network that matches nothing).

use std::collections::HashSet;

use pg_grammar::model::{Grammar, LexEntryId, MorphRuleDef, OutputAction, SegmentedText};

use crate::compose_budget::{compound_pair_budget_from_env, ComposeBudget, ComposeError};
use crate::emit::{
    build_compound_chain, classify_affix, compound_extra_levels_checked, compound_license,
    write_bare, write_lexicon_header, EmitCounts, Role,
};
use crate::replace::SegAlphabet;
use crate::tags;

/// One emittable root/affix lexc entry in this module's own token space: the morpheme's tag symbol
/// and the [`SegAlphabet`]-encoded underlying spelling of one allomorph. Deliberately NOT
/// `crate::emit::RootRec` — that type is surface-variant/precision-oriented (it carries a
/// `Vec<String>` of representation variants, a `Stripped` sibling list, an `AllomorphId` for the
/// precision knob's owner-side gate, and a `FsId` category), none of which exists on this path;
/// [`build_compound_chain`] is generic over the root record type precisely so this module can keep
/// its own two-field one (see that function's doc).
struct TokenEntry {
    tag: String,
    underlying: String,
}

impl TokenEntry {
    /// This entry as one lexc line continuing to `continuation`. No `crate::emit::escape_lexc_text`
    /// call: `underlying` is a [`SegAlphabet`] PUA-codepoint token string (that module's own
    /// "Symbol alphabet" doc), which by construction contains none of lexc's metacharacters, and
    /// `tag` is a `crate::tags` multichar symbol.
    fn write(&self, out: &mut String, continuation: &str, counts: &mut EmitCounts) {
        out.push_str(&self.tag);
        out.push(':');
        out.push_str(&self.underlying);
        out.push(' ');
        out.push_str(continuation);
        out.push_str(" ;\n");
        counts.lexc_lines += 1;
    }
}

#[derive(Debug)]
pub struct UEmitReport {
    pub lexc_source: String,
    /// One line per skipped allomorph, e.g. `"mrule14#allo1 role=reduplication"` -- or, for an
    /// entire [`MorphRuleDef::Compounding`]/[`MorphRuleDef::Realizational`] rule this module never
    /// implements at all, one line per rule, e.g. `"mrule9(cmp1) kind=compounding-rule"` /
    /// `"mrule12/RL(-) kind=realizational-rule"` (module doc's "what's covered / skipped" section).
    /// Never a silent drop (module doc).
    pub skipped: Vec<String>,
    pub root_entries: usize,
    pub prefix_entries: usize,
    pub suffix_entries: usize,
    pub tag_width: usize,
}

/// The leading (before the first `Copy`) or trailing (after the last `Copy`) `InsertSegments`
/// text of an allomorph's RHS, mirroring the min/max-copy-index scan
/// [`crate::emit::classify_affix`] does internally (not exposed there, so re-derived here — ~10
/// lines, cheap, and avoids widening that function's public surface for one caller).
fn affix_insert_shape(rhs: &[OutputAction], leading: bool) -> Option<&SegmentedText> {
    let mut first_copy: Option<usize> = None;
    let mut last_copy: usize = 0;
    for (i, a) in rhs.iter().enumerate() {
        if matches!(a, OutputAction::Copy(_)) {
            if first_copy.is_none() {
                first_copy = Some(i);
            }
            last_copy = i;
        }
    }
    let range: std::ops::Range<usize> = match first_copy {
        None => 0..rhs.len(),
        Some(fc) => {
            if leading {
                0..fc
            } else {
                (last_copy + 1)..rhs.len()
            }
        }
    };
    for a in &rhs[range] {
        if let OutputAction::InsertSegments { shape, .. } = a {
            return Some(shape);
        }
    }
    None
}

/// Emit the underlying-form lexc source for `g` (Indonesian-scoped design, module doc).
///
/// Thin wrapper over [`emit_underlying_filtered`] with every lexical entry included (the
/// pre-gating behavior, unchanged for every existing caller).
pub fn emit_underlying(g: &Grammar, alphabet: &SegAlphabet) -> Result<UEmitReport, ComposeError> {
    emit_underlying_filtered(g, alphabet, None)
}

/// Identical to [`emit_underlying`], but when `allowed_entries` is `Some`, ONLY [`LexEntryId`]s in
/// that set get root lexc lines emitted — every other entry is silently omitted (NOT reported in
/// `skipped`: this is `crate::gate`'s static-partition design, where an entry excluded from THIS
/// group's lexicon is included in a DIFFERENT group's, so it is not a coverage gap here, unlike a
/// genuinely uncovered construct). Affix (prefix/suffix) chains are never filtered — MPR/POS gating
/// in this prototype's scope is root-only (`crate::gate`'s module doc), so every group shares the
/// identical affix lexicons.
///
/// Builds a production [`ComposeBudget`] from `HC_COMPOSE_*` env vars exactly once (mirrors
/// `crate::emit::emit_with_precision`'s own convention). Tests should call
/// [`emit_underlying_filtered_with_budget`] directly instead.
pub fn emit_underlying_filtered(
    g: &Grammar,
    alphabet: &SegAlphabet,
    allowed_entries: Option<&HashSet<LexEntryId>>,
) -> Result<UEmitReport, ComposeError> {
    let budget = ComposeBudget::from_env();
    emit_underlying_filtered_with_budget(g, alphabet, allowed_entries, &budget)
}

/// [`emit_underlying_filtered`]'s core, with the [`ComposeBudget`] threaded in explicitly rather
/// than read from env -- what `crate::gate::compile_gated_grammar_with_budget` and tests call
/// directly, so a whole gated-grammar compile shares ONE budget across every group's emission.
///
/// V4 (design doc §4 + §8 item 1): `budget.line_cap` is checked INCREMENTALLY, at each of the three
/// line-push sites below (root/prefix/suffix), so a pathological grammar bails during the very
/// first line that crosses the cap rather than after building a possibly-multi-GB `lexc_source`
/// string in full.
pub fn emit_underlying_filtered_with_budget(
    g: &Grammar,
    alphabet: &SegAlphabet,
    allowed_entries: Option<&HashSet<LexEntryId>>,
    budget: &ComposeBudget,
) -> Result<UEmitReport, ComposeError> {
    let width = tags::tag_width(g.morphemes.len());
    let mut skipped = Vec::new();
    let mut multichar: Vec<String> = Vec::new();
    let mut declared_tags: HashSet<String> = HashSet::new();
    /// The `base` name [`build_compound_chain`] derives every compound level's lexicon names from
    /// (`UCmp`, `UCmpPfx0`, `UCmpRoots`, `UCmp2Next`, ...). `crate::emit`'s own two callers use
    /// `TLCmp`/`G{gi}Cmp`; this module has no template-less/per-group split to name against, so one
    /// base for the whole emission is enough.
    const COMPOUND_BASE: &str = "UCmp";

    /// One emitted bare-root lexc line plus whether its owning entry is licensed to HEAD a compound
    /// (`crate::emit::CompoundLicense::head_eligible`) -- decided per entry while the lines are
    /// collected, consumed when the continuation is chosen at write time (the compound loop's own
    /// existence is not known until after the license and the non-head inventory are both computed).
    type RootLine = (TokenEntry, bool);

    let mut root_lines: Vec<RootLine> = Vec::new();
    let mut prefix_lines: Vec<TokenEntry> = Vec::new();
    let mut suffix_lines: Vec<TokenEntry> = Vec::new();
    let line_cap = budget.line_cap();
    let check_line_budget = |lines: usize| -> Result<(), ComposeError> {
        if lines > line_cap {
            return Err(ComposeError::EmitLineBudgetExceeded {
                lines,
                limit: line_cap,
            });
        }
        Ok(())
    };

    // GRAMMAR-WIDE (module doc, "The non-head root set is GRAMMAR-WIDE"): computed from `g` alone,
    // never from `allowed_entries`. `None` for the overwhelmingly common grammar that declares no
    // `CompoundingRuleDef` at all, in which case everything below is a pure no-op and this module's
    // emitted lexc is byte-identical to the pre-compound-loop version.
    let license = compound_license(g);

    for (ei, entry) in g.entries.iter().enumerate() {
        if let Some(allowed) = allowed_entries {
            if !allowed.contains(&LexEntryId(ei as u32)) {
                continue;
            }
        }
        let tag = tags::root_tag_lexc(entry.morpheme, width);
        let head_eligible = license
            .as_ref()
            .is_some_and(|l| l.head_eligible.contains(&LexEntryId(ei as u32)));
        let mut declared = false;
        for (ai, allo) in entry.allomorphs.iter().enumerate() {
            if allo.is_pattern {
                skipped.push(format!("entry{ei}#allo{ai} pattern-allomorph"));
                continue;
            }
            if !declared {
                multichar.push(tag.clone());
                declared_tags.insert(tag.clone());
                declared = true;
            }
            let underlying = alphabet.encode_shape(&allo.shape.shape);
            root_lines.push((
                TokenEntry {
                    tag: tag.clone(),
                    underlying,
                },
                head_eligible,
            ));
            check_line_budget(root_lines.len() + prefix_lines.len() + suffix_lines.len())?;
        }
    }

    // The compound levels' own root inventory: every grammar-wide entry `compound_license` admits
    // as a NON-HEAD stem, regardless of which gate-partition group it belongs to (module doc). A
    // pattern allomorph is skipped exactly as it is above (`encode_shape` cannot token a
    // class-reference interior node) but NOT re-reported here -- the group that owns the entry
    // already reports it, and reporting it once per group would multiply one real gap by the group
    // count.
    let mut non_head_roots: Vec<TokenEntry> = Vec::new();
    if let Some(license) = &license {
        for (ei, entry) in g.entries.iter().enumerate() {
            if !license.non_head_eligible.contains(&LexEntryId(ei as u32)) {
                continue;
            }
            let tag = tags::root_tag_lexc(entry.morpheme, width);
            for allo in &entry.allomorphs {
                if allo.is_pattern {
                    continue;
                }
                if declared_tags.insert(tag.clone()) {
                    multichar.push(tag.clone());
                }
                non_head_roots.push(TokenEntry {
                    tag: tag.clone(),
                    underlying: alphabet.encode_shape(&allo.shape.shape),
                });
            }
        }
    }

    for (mid, mrule) in g.mrules.iter().enumerate() {
        let def = match mrule {
            MorphRuleDef::AffixProcess(def) => def,
            MorphRuleDef::Compounding(_) => {
                // Handled structurally by the bounded compound loop below (module doc), not as an
                // affix chain: a `CompoundingRuleDef` has no `MorphemeId` (`model.rs`'s own doc:
                // "Not a morpheme") and no flat allomorph list (head/non-head
                // `CompoundingSubruleDef`s instead), so there is nothing for THIS loop to emit. If
                // the loop below turns out to be unable to offer the rule anything (no licensed
                // non-head root allomorph survives), that -- and only that -- is reported to
                // `skipped`, after the license is known.
                continue;
            }
            MorphRuleDef::Realizational(def) => {
                // Same whole-rule granularity as `Compounding` above, but `Realizational` DOES
                // carry a `MorphemeId` and an `AffixAllomorphDef` list shaped just like
                // `AffixProcess`'s -- reported by morpheme identity (matching the label built
                // below for `AffixProcess`) rather than by allomorph, because the reason this
                // module skips it is not "this one allomorph doesn't classify", it's "this
                // module never attempts the syntactic feature-realization mechanism
                // `RealizationalRuleDef` needs at all" (module doc).
                let morph_name = g
                    .morphemes
                    .get(def.morpheme.0 as usize)
                    .map(|mi| format!("{}({})", mi.xml_key, mi.gloss.as_deref().unwrap_or("-")))
                    .unwrap_or_else(|| format!("mrules[{mid}]"));
                skipped.push(format!("{morph_name} kind=realizational-rule"));
                continue;
            }
        };
        let tag = tags::morph_tag_lexc(def.morpheme, width);
        // Report by the morpheme's own XML identity (e.g. "mrule14/AV"), not the raw 0-based
        // index into `g.mrules` (which also counts CompoundingRuleDef entries ahead of it and so
        // does NOT line up with the grammar's own "mruleN" XML ids — a real footgun for a
        // diagnostic label, caught only by cross-checking against `MorphemeInfo::xml_key` at
        // report-writing time).
        let morph_name = g
            .morphemes
            .get(def.morpheme.0 as usize)
            .map(|mi| format!("{}({})", mi.xml_key, mi.gloss.as_deref().unwrap_or("-")))
            .unwrap_or_else(|| format!("mrules[{mid}]"));
        let mut declared = false;
        for (ai, allo) in def.allomorphs.iter().enumerate() {
            let role = classify_affix(&allo.rhs);
            match role {
                Role::Prefix => {
                    let Some(shape) = affix_insert_shape(&allo.rhs, true) else {
                        skipped.push(format!("{morph_name}#allo{ai} prefix-with-no-insert"));
                        continue;
                    };
                    if !declared {
                        multichar.push(tag.clone());
                        declared_tags.insert(tag.clone());
                        declared = true;
                    }
                    let underlying = alphabet.encode_shape(&shape.shape);
                    prefix_lines.push(TokenEntry {
                        tag: tag.clone(),
                        underlying,
                    });
                    check_line_budget(root_lines.len() + prefix_lines.len() + suffix_lines.len())?;
                }
                Role::Suffix => {
                    let Some(shape) = affix_insert_shape(&allo.rhs, false) else {
                        skipped.push(format!("{morph_name}#allo{ai} suffix-with-no-insert"));
                        continue;
                    };
                    if !declared {
                        multichar.push(tag.clone());
                        declared_tags.insert(tag.clone());
                        declared = true;
                    }
                    let underlying = alphabet.encode_shape(&shape.shape);
                    suffix_lines.push(TokenEntry {
                        tag: tag.clone(),
                        underlying,
                    });
                    check_line_budget(root_lines.len() + prefix_lines.len() + suffix_lines.len())?;
                }
                other => {
                    skipped.push(format!(
                        "{morph_name}#allo{ai} role={other_label}",
                        other_label = role_label(other)
                    ));
                }
            }
        }
    }

    // --- Bounded compound loop containment, checked BEFORE any of its lexc text is written -------
    //
    // `emit_compound` is false whenever the loop would be vacuous: no `CompoundingRuleDef` at all
    // (the common case, `license == None` -- everything below is then byte-identical to the
    // pre-compound-loop emission), or a declared rule that licenses no emittable non-head root
    // allomorph anywhere in the grammar. The second case is a REAL gap, so it is reported rather
    // than emitted as an empty, dead-end `UCmpRoots` lexicon.
    let emit_compound = license.is_some() && !non_head_roots.is_empty();
    if license.is_some() && non_head_roots.is_empty() {
        for (mid, mrule) in g.mrules.iter().enumerate() {
            if let MorphRuleDef::Compounding(def) = mrule {
                skipped.push(format!(
                    "mrule{mid}({}) kind=compounding-rule reason=no-licensed-non-head-root",
                    def.xml_id
                ));
            }
        }
    }
    let mut compound_extra_levels = 0usize;
    if emit_compound {
        // The pessimistic head-side operand is the grammar-wide root-allomorph count, not this
        // group's own `root_lines.len()`: the per-group networks are unioned, so the cost this
        // budget bounds is incurred across the whole partition, not within one group (`crate::emit`
        // uses its own `all_roots.len()` for the same reason -- the head side is never narrowed
        // below the full root bag there either).
        let heads = g
            .entries
            .iter()
            .map(|e| e.allomorphs.iter().filter(|a| !a.is_pattern).count())
            .sum::<usize>();
        let pairs = heads.saturating_mul(non_head_roots.len());
        let limit = compound_pair_budget_from_env();
        if pairs > limit {
            return Err(ComposeError::CompoundPairBudgetExceeded {
                heads,
                non_heads: non_head_roots.len(),
                pairs,
                limit,
            });
        }
        compound_extra_levels = compound_extra_levels_checked(g)?;
    }

    let mut out = String::new();
    out.push_str("Multichar_Symbols\n");
    for m in &multichar {
        out.push_str(m);
        out.push('\n');
    }
    out.push_str("\nLEXICON Root\n");
    out.push_str("PrefixOrRoot ;\n"); // Start == Root's own header lexicon: PrefixChain or bare root, see below
    out.push_str("\nLEXICON PrefixOrRoot\n");
    out.push_str("PrefixChain ;\n");
    out.push_str("RootBare ;\n");
    let mut counts = EmitCounts::default();
    out.push_str("\nLEXICON PrefixChain\n");
    for l in &prefix_lines {
        l.write(&mut out, "PrefixOrRoot", &mut counts);
    }
    out.push_str("\nLEXICON RootBare\n");
    // Head-eligibility split (`crate::emit`'s own `TLPost`/`TLPostNoCmp` shape, design.md D3 head
    // Gate): a root this grammar's compounding rules cannot head continues STRAIGHT to
    // `SuffixOrEnd`, so it is never offered the compound loop -- precision, not recall. With no
    // compound loop at all every root takes that same continuation, which is exactly the
    // pre-existing emission, byte for byte.
    let mut any_head_line = false;
    for (l, head_eligible) in &root_lines {
        let continuation = if emit_compound && *head_eligible {
            any_head_line = true;
            "AfterRoot"
        } else {
            "SuffixOrEnd"
        };
        l.write(&mut out, continuation, &mut counts);
    }

    if emit_compound && any_head_line {
        write_lexicon_header(&mut out, "AfterRoot");
        write_bare(&mut out, "SuffixOrEnd", &mut counts);
        write_bare(&mut out, COMPOUND_BASE, &mut counts);

        // This module's own SELF-LOOPING prefix chain (module doc), one copy per compound level --
        // NOT `crate::emit::build_deriv_chain`, whose rule-count-bounded dedicated-level machinery
        // and `emit_rule_allomorphs` leaf emission are exactly what this module exists not to use.
        // `build_compound_chain` takes this as a callback precisely so the two conventions can
        // coexist (that function's own doc).
        let mut prefix_hop = |out: &mut String,
                              pfx_base: &str,
                              roots_name: &str,
                              counts: &mut EmitCounts,
                              _ctx: &mut ()| {
            let entry = format!("{pfx_base}0");
            write_lexicon_header(out, &entry);
            write_bare(out, roots_name, counts);
            for l in &prefix_lines {
                l.write(out, &entry, counts);
            }
        };
        let write_root_entries = |out: &mut String,
                                  roots: &[&TokenEntry],
                                  continuation: &str,
                                  counts: &mut EmitCounts,
                                  _ctx: &mut ()| {
            for r in roots {
                r.write(out, continuation, counts);
            }
        };
        // Provably never invoked: `emit_stripped` is `false` (this module never probes junction
        // phonology, so there is no `Stripped` roots sibling to write) -- same "no real Stripped
        // sibling here" case `build_compound_chain`'s own doc names for the P6 templated path.
        let write_stripped_noop = |_out: &mut String,
                                   _roots: &[&TokenEntry],
                                   _continuation: &str,
                                   _counts: &mut EmitCounts,
                                   _ctx: &mut ()| {
            unreachable!("write_stripped_root_entries is only invoked when emit_stripped is true")
        };
        let non_head_refs: Vec<&TokenEntry> = non_head_roots.iter().collect();
        build_compound_chain(
            &mut out,
            COMPOUND_BASE,
            compound_extra_levels,
            &non_head_refs,
            &[],
            "SuffixOrEnd",
            &mut counts,
            &mut (),
            false,
            &mut prefix_hop,
            &write_root_entries,
            &write_stripped_noop,
        );
    }

    out.push_str("\nLEXICON SuffixOrEnd\n");
    out.push_str("SuffixChain ;\n");
    out.push_str("# ;\n");
    out.push_str("\nLEXICON SuffixChain\n");
    for l in &suffix_lines {
        l.write(&mut out, "SuffixOrEnd", &mut counts);
    }
    // The compound levels are the one block this function does not push line-by-line, so its own
    // incremental `check_line_budget` above cannot see them; `EmitCounts::lexc_lines` is the number
    // `build_compound_chain` itself increments, checked here against the same cap (V4's own "bail
    // during emission rather than after building a multi-GB string" intent, at the coarsest
    // granularity a shared, `EmitResult`-free helper allows).
    check_line_budget(counts.lexc_lines)?;

    Ok(UEmitReport {
        lexc_source: out,
        root_entries: root_lines.len(),
        prefix_entries: prefix_lines.len(),
        suffix_entries: suffix_lines.len(),
        tag_width: width,
        skipped,
    })
}

fn role_label(r: Role) -> &'static str {
    match r {
        Role::None => "none",
        Role::Prefix => "prefix",
        Role::Suffix => "suffix",
        Role::Infix => "infix",
        Role::Reduplication => "reduplication",
        Role::CircumfixPrefix => "circumfix-prefix",
        Role::CircumfixSuffix => "circumfix-suffix",
        Role::Process => "process",
    }
}

#[cfg(test)]
mod emit_budget_tests {
    //! `docs/fst-plan/phase-b-compose-budget-design.md` §6's own test plan for this module: 20
    //! lexical entries (one allomorph each -- one root lexc line per entry, no prefixes/suffixes at
    //! all), `line_cap=5`, must trip `EmitLineBudgetExceeded` reporting `lines: 6` -- the FIRST
    //! line count that crosses the cap (proving incremental, first-crossing detection rather than a
    //! check only after the whole lexc source is built).
    use std::fmt::Write as _;

    use super::*;
    use crate::compose_budget::ComposeBudget;
    use crate::replace::SegAlphabet;

    fn twenty_entries_fixture() -> Grammar {
        let mut entries = String::new();
        for i in 0..20u32 {
            write!(
                entries,
                r#"
          <LexicalEntry id="entry{i}" partOfSpeech="posV">
            <Allomorphs><Allomorph id="allo{i}"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e{i}</Gloss>
          </LexicalEntry>"#
            )
            .unwrap();
        }
        let xml = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>EmitLineBudgetFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>S</Name>
        <LexicalEntries>{entries}
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
        );
        pg_grammar::load(&xml)
            .unwrap_or_else(|e| panic!("failed to load 20-entry fixture: {e}\n{xml}"))
    }

    #[test]
    fn line_budget_trips_incrementally() {
        let g = twenty_entries_fixture();
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let budget =
            ComposeBudget::with_caps(usize::MAX, usize::MAX, usize::MAX, usize::MAX, 5, None);

        let err = emit_underlying_filtered_with_budget(&g, &alphabet, None, &budget)
            .expect_err("20 root lines must exceed a line_cap of 5");
        match err {
            ComposeError::EmitLineBudgetExceeded { lines, limit } => {
                assert_eq!(
                    lines, 6,
                    "must bail on the FIRST line count crossing the cap, not the final total"
                );
                assert_eq!(limit, 5);
            }
            other => panic!("expected EmitLineBudgetExceeded, got {other:?}"),
        }
    }

    #[test]
    fn unbounded_budget_never_trips_on_twenty_entries() {
        let g = twenty_entries_fixture();
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let budget = ComposeBudget::unbounded();
        let report = emit_underlying_filtered_with_budget(&g, &alphabet, None, &budget)
            .expect("unbounded budget must never trip");
        assert_eq!(report.root_entries, 20);
    }
}
