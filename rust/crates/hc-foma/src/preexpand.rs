//! Rule-application pre-expansion + boundary-fusion composite probing (plan
//! `docs/fst-plan/foma-fst-plan.md` P1d): closes the two structural miss classes the P1c
//! investigation found on Amharic (32/32 misses classified, no third class) --
//!
//! 1. **Interdigitation** (`Role::Infix` rules -- Amharic's `-pfv-`/`-conv-`, 24/32 of the P1c
//!    misses): a standalone rule whose RHS interleaves `InsertSegments` actions AROUND a `Copy` of
//!    the root's own material has no literal string a plain lexc entry can express -- the inserted
//!    "ä" sits INSIDE the root's own copied consonants (root "ውልድ" + `-pfv-` -> "ውäልäድ"), so there
//!    is no boundary a two-entry (root-entry, then-continue-to-affix-entry) encoding can cut apart.
//! 2. **Ge'ez boundary fusion** (ordinary `Role::Prefix`/`Role::Suffix` rules whose adjacency to a
//!    SPECIFIC root's own final/initial glyph coalesces into a DIFFERENT glyph, 8/32): the existing
//!    deletion-only junction model ([`crate::junctions::PhonologyProbe`]) can express "one
//!    neighbouring segment vanishes outright" but not "two adjacent segments merge into a
//!    differently-spelled third segment" -- Ge'ez being an abugida, adjacent consonant+vowel glyphs
//!    at a morph boundary regularly do this (root "ልጅ" + pl suffix "+ዮች" -> "ልጆች", never the
//!    literal "ልጅዮች").
//!
//! ## Shared mechanism
//! Both classes are closed by the SAME technique -- **rule-application pre-expansion**: seed a
//! `hc_rules::word::Word` from one root allomorph's own FEATURE-BEARING shape (re-segmented with
//! features exactly the way `hc_parse::Morpher`'s own lexical lookup does it --
//! `hc_rules::shape_feat::segment_with_features` on the allomorph's stored TEXT, NOT the loader's
//! feature-LESS `RootAllomorphDef::shape.shape` directly -- using the stored shape makes every
//! natural-class LHS check fail silently, a real bug this stage's investigation caught: 0/76 roots
//! matched any of the three infix rules until the fix, 36/76 after), apply the REAL rule via
//! [`hc_rules::morph::synthesize`] (the exact function the engine's own per-word synthesis pipeline
//! calls -- not a re-implementation), then run the REAL phonological cascade over the result via
//! [`hc_rules::surface_probe::probe_synthesize`] (the same probe machinery
//! [`crate::junctions::PhonologyProbe`] already drives) to get the TRUE, phonology-resolved surface
//! spelling. When that surface differs from a naive (pre-phonology) rendering of the very same
//! synthesized shape -- ALWAYS true for an `Infix` rule (there is no non-interleaved literal to even
//! compare against), SOMETIMES true for a `Prefix`/`Suffix` rule (fusion) -- emit ONE lexc entry
//! carrying BOTH the root's tag and the rule's tag, in the ENGINE'S OWN morph order.
//!
//! That order is COMPUTED, never assumed: [`morph_order_tags`] replays
//! `hc_parse::Morpher::allomorphs_in_morph_order`'s own algorithm (sort the synthesized `Word`'s
//! `morphs` by `order`, keep only the first occurrence of each distinct `AllomorphId`) over the
//! synthesized `Word` -- so this is correct regardless of whether the rule is a leading, trailing,
//! or interior insertion, with NO per-role special-casing: a `Prefix` composite naturally comes out
//! rule-tag-then-root-tag, a `Suffix`/`Infix` composite root-tag-then-rule-tag, because that is
//! genuinely where each one's own first surface material sits (root is always seeded at `order = 0`,
//! `hc-parse/src/morpher.rs:564`'s convention, mirrored here). Verified directly against the real
//! engine (this stage's investigation): `hc_parse::Morpher` analyzing "ሄደ" ("go.pfv.3m") returns
//! `morpheme_ids = [entry43(go), mrule13(-pfv-), mrule18(pfv.3m)]` -- root first, matching what this
//! module's own `morph_order_tags` computes independently for the same (root, rule) pair.
//!
//! Generic by construction (plan §0: "do not special-case Amharic"): this runs for EVERY
//! `Role::{Infix, Prefix, Suffix}` rule in the grammar against EVERY root allomorph (and,
//! recursively, against every chain stem -- see "Chaining" below), gated only by the SAME
//! `required_syn_fs` unifiability check `hc_rules::morph::synth_syn_fs` applies internally (a
//! cheap, behavior-PRESERVING pre-filter: `synthesize` would reject an incompatible pair anyway,
//! this just skips building a `Word`/compiling the rule's LHS FST for one that provably cannot
//! match). Measured on Amharic (release build): depth-0 alone is 6,612 raw (root, rule)
//! combinations pre-filtered to 1,389; with depth-3 chaining the total is ~305k pairs probed,
//! yielding 2,930 interdigitation + 51,023 fusion composite entries in ~30-47s of emit wall time
//! (the dominant emit cost -- see SCALE BRIDGE below). A grammar with zero `Infix` rules and zero
//! phonological rules at all (Sena) computes zero pairs and emits zero composites --
//! [`should_run`] short-circuits before touching a single entry, which is what keeps Sena's
//! emitted lexc source byte-for-byte unchanged (its own regression gate depends on this).
//! Indonesian (real phonology, no infix rules, no coalescence) probes 457 pairs and emits ZERO
//! composites -- every junction it has is already reachable through the existing deletion-junction
//! model, verified by the redundancy check below.
//!
//! ## Chaining (a composite may itself need a further composite — through CLEAN steps too)
//! An interdigitated or fused stem is not always word-final: Amharic's pfv/conv stems obligatorily
//! take a subject-agreement suffix (`root + -pfv- + pfv.3m`), and that agreement suffix ITSELF fuses
//! with the composite stem's own final glyph (`"ሄድ" (root+`-pfv-`) + "+ä" (pfv.3m) -> "ሄደ"`, not the
//! literal `"ሄድä"`) — discovered empirically by this stage's own recall gate (a first single-level
//! implementation still missed "ሄደ" outright). Worse, a fusion can follow a byte-CLEAN step:
//! "ሌባዎቹ" is `root ሌባ + def.m (clean: "ሌባው") + pl (ው+o fuse: "ሌባዎች") + poss.3m (ች+u fuse:
//! "ሌባዎቹ")` — caught by the gate at 31/32. [`extend`] therefore recurses on EVERY successful rule
//! application (dirty or clean), bounded by [`MAX_EXTRA_RULES`], EMITTING a composite (all of the
//! chain's tags, engine morph order) only for dirty steps: a clean step is already realized by the
//! ordinary per-rule lexc entries, so emitting it would only duplicate paths, but its output word
//! must still be explored for deeper fusions. Dirtiness at every depth is judged by the SAME
//! [`reachable_via_ordinary_emission`] check: the "one side" baseline is the root's own
//! spellings/stripped-spellings at depth 0, and the previous level's single rendered surface
//! (plus its stripped form ONLY if that level was clean — a dirty stem exists only as a composite
//! entry, which has no `Stripped` sibling) at depth >= 1.
//!
//! ## Avoiding redundant entries with the existing junction model
//! A `Prefix`/`Suffix` composite is only emitted when the fused spelling is NOT already reachable
//! through the ORDINARY two-entry path (`emit.rs`'s own literal root/affix entries, enriched by
//! [`PhonologyProbe::variants`]/[`PhonologyProbe::deletion_junctions`] when the grammar has any
//! phonological rules) -- [`reachable_via_ordinary_emission`] recomputes that same candidate string
//! set (every affix spelling × every root spelling, plus the deletion-junction × stripped-root
//! combination `emit.rs` itself wires) and checks membership before minting a new composite. This is
//! what keeps Indonesian's `meN+tulis -> menulis` (already correctly produced by the EXISTING
//! deletion-junction mechanism: "men" + stripped "ulis") from ALSO growing a redundant joint
//! composite entry, while Ge'ez's true fusions (inexpressible by literal concatenation on EITHER
//! side, in ANY combination the existing model offers) do get one. An `Infix` rule has no "ordinary
//! two-entry path" to compare against at all (that is the whole reason it was routed to `uncovered`
//! upstream), so it is always emitted once a matching (root, rule) pair is found.
//!
//! ## SCALE BRIDGE (plan §0 scale mandate)
//! This is an O(roots × rules^depth) enumeration -- workable at Amharic's 76 entries × 87 rules ×
//! depth 3 (measured: ~305k pairs, ~54k entries, ~30-47s emit, all within the gate's soft budget)
//! but decidedly NOT at FLEx scale (10⁴-10⁵ entries, hundreds of rules), and every probe here
//! recompiles its rule's LHS FST from scratch ([`hc_rules::morph::synthesize`]; the
//! `RuleCache`-aware `synthesize_cached_traced` is `pub(crate)` to `hc-rules` and unavailable
//! here). The **P6 successor** (replace-rule compilation, `docs/fst-plan/foma-fst-plan.md` P6
//! item 1) retires this bridge by compiling interdigitating/fusing rules as real foma
//! replace-calculus rules over root natural-class patterns, composed directly into the network
//! instead of enumerated per root and per chain -- exactly the same successor already named for
//! [`crate::junctions::PhonologyProbe`]'s own enumeration bridge.

use hc_featstruct::{is_unifiable, FsId};
use hc_grammar::chardef::CharDefTable;
use hc_grammar::model::{AllomorphId, Grammar, MRuleId, MorphRuleDef, MorphemeId, OutputAction};
use hc_rules::cache::RuleCache;
use hc_rules::morph::synthesize;
use hc_rules::surface_probe;
use hc_rules::word::{MorphRecord, Word};

use crate::emit::{rule_role, stripped_variants, surface_variants, Role};
use crate::junctions::PhonologyProbe;
use crate::tags;

/// One rule-application/fusion composite: an extra "root-like" lexc entry whose upper tape carries
/// MULTIPLE tag symbols (root + 1..=[`MAX_EXTRA_RULES`] rules) instead of one, in the engine's own morph order
/// ([`morph_order_tags`]). Wired by `emit.rs` into one shared `Composites` lexicon reachable from
/// every roots-lexicon emission site (bare `Root`, `TLRoots`, each `G{gi}Roots`) and continuing
/// into `CompositeExit` (the union of every post-root continuation), so an interdigitated/fused
/// stem can still take ordinary prefixes/suffixes around it (plan P1d interaction item 4:
/// root-section replacement, not bare-only).
pub(crate) struct CompositeRec {
    /// The root morpheme this composite is anchored to (bookkeeping/diagnostics only; kept for a
    /// future gate/debug print that wants to group composite counts by root without re-deriving it
    /// from `tag_lexc`).
    #[allow(dead_code)]
    pub morpheme: MorphemeId,
    /// Every morpheme whose tag appears in `tag_lexc`, as `(is_root, id)` — `emit.rs` declares each
    /// in `Multichar_Symbols` (an Infix rule's morpheme is in NO deriv layer or slot, so no other
    /// collection site would declare it).
    pub chain_morphemes: Vec<(bool, MorphemeId)>,
    /// The escaped, ALREADY-CONCATENATED upper-tape tag string (all the chain's tags, in engine
    /// morph order).
    pub tag_lexc: String,
    /// The rendered, phonology-resolved surface spelling(s) (usually one; kept as a `Vec` for
    /// symmetry with `RootRec::variants` and in case a rule's own disjunctive allomorphs produce
    /// more than one distinct rendering for the same tag pair).
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CompositeReport {
    /// (root allomorph, candidate rule) pairs actually attempted (after the cheap required-FS
    /// pre-filter) -- the module doc's scale-bridge number.
    pub pairs_probed: usize,
    /// Composite entries emitted for `Role::Infix` rules (miss class 5a).
    pub interdigitation_entries: usize,
    /// Composite entries emitted for `Role::Prefix`/`Role::Suffix` rules whose fused surface differs
    /// from what the ordinary two-entry emission already reaches (miss class 5b).
    pub fusion_entries: usize,
    /// `g.mrules` indices of `Role::Infix` rules that produced at least one composite entry --
    /// `emit.rs` suppresses the "standalone rule classifies as Infix; not representable" uncovered
    /// routing for exactly these (the construct IS representable now, via this module); an infix
    /// rule that matched zero roots stays uncovered, honestly.
    pub covered_infix_rules: std::collections::BTreeSet<u32>,
}

/// Whether this grammar can possibly need either mechanism at all -- `false` short-circuits
/// [`build_composites`] to a zero-cost, zero-entry no-op (module doc: what keeps Sena's gate
/// byte-for-byte).
pub(crate) fn should_run(g: &Grammar, phon: Option<&PhonologyProbe>) -> bool {
    phon.is_some() || any_infix_rule(g)
}

fn any_infix_rule(g: &Grammar) -> bool {
    (0..g.mrules.len())
        .any(|i| rule_role(g, MRuleId(i as u32)) == Role::Infix)
}

/// Every rule id whose PRIMARY allomorph classifies `Infix`/`Prefix`/`Suffix` (mirrors `emit.rs`'s
/// own `rule_role` convention for "how this rule is treated" everywhere else in the emitter).
/// `Reduplication` (peel's job, D6), `CircumfixPrefix`/`CircumfixSuffix` (P1d item 3, not exercised
/// by any reference-grammar corpus fixture at this stage), `Process`, and `None` are out of this
/// stage's scope.
fn candidate_rules(g: &Grammar) -> Vec<(MRuleId, Role)> {
    let mut out = Vec::new();
    for (i, r) in g.mrules.iter().enumerate() {
        if matches!(r, MorphRuleDef::Compounding(_)) {
            continue;
        }
        let mid = MRuleId(i as u32);
        let role = rule_role(g, mid);
        if matches!(role, Role::Prefix | Role::Suffix | Role::Infix) {
            out.push((mid, role));
        }
    }
    out
}

/// `(required_syn_fs, out_syn_fs, owning morpheme)` for the two rule kinds that carry allomorphs
/// (never called on `Compounding` -- [`candidate_rules`] never includes one).
fn rule_fs_and_morpheme(rule: &MorphRuleDef) -> (FsId, MorphemeId) {
    match rule {
        MorphRuleDef::AffixProcess(def) => (def.required_syn_fs, def.morpheme),
        MorphRuleDef::Realizational(def) => (def.required_syn_fs, def.morpheme),
        MorphRuleDef::Compounding(_) => unreachable!("candidate_rules excludes Compounding"),
    }
}

/// Replays `hc_parse::Morpher::allomorphs_in_morph_order`'s own algorithm (sort `Word::morphs` by
/// `order`, keep the FIRST occurrence of each distinct `AllomorphId`) over a freshly-synthesized
/// composite `Word`, then maps each surviving record to its pre-rendered tag string via `known` --
/// so composite tag order is COMPUTED from the exact same bookkeeping the real engine uses, never
/// assumed from any rule's role. Generic over chain length (`known` may name the root plus ANY
/// number of applied rules -- see the module doc's "Chaining" section). Returns `None` if a
/// surviving record's morpheme isn't in `known` (defensive; should never happen for a word seeded
/// with exactly one root record and only rules from this same chain applied to it).
fn morph_order_tags(w: &Word, known: &[(MorphemeId, String)]) -> Option<String> {
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

/// Module doc's "Avoiding redundant entries": does the ORDINARY two-entry emission (literal root
/// spelling(s), literal affix spelling(s), optionally enriched by [`PhonologyProbe`]) already reach
/// `fused` through some combination? Mirrors `emit.rs`'s own two routing rules exactly:
/// `PhonologyProbe::variants` spellings concatenate with a FULL root spelling; `deletion_junctions`
/// spellings concatenate with a root spelling that has had its OWN leading segment stripped
/// ([`stripped_variants`]) -- the `{roots}Stripped` mechanism `emit.rs`'s `build_deriv_chain` wires,
/// PREFIX-only (there is no suffix-side equivalent in `emit.rs` today, so a `Suffix` rule only ever
/// checks the plain `variants` × full-root combination).
#[allow(clippy::too_many_arguments)]
fn reachable_via_ordinary_emission(
    table: &CharDefTable,
    phon: Option<&PhonologyProbe>,
    root_variants: &[String],
    root_stripped: &[String],
    rule: &MorphRuleDef,
    is_prefix: bool,
    fused: &str,
) -> bool {
    let allomorphs = match rule {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs,
        MorphRuleDef::Realizational(def) => &def.allomorphs,
        MorphRuleDef::Compounding(_) => return false,
    };
    for allo in allomorphs {
        let Some(text) = allo.rhs.iter().find_map(|a| match a {
            OutputAction::InsertSegments { shape, .. } => Some(shape.text.as_str()),
            _ => None,
        }) else {
            continue;
        };
        let mut ordinary: Vec<String> = surface_variants(table, text).map(|(v, _)| v).unwrap_or_default();
        if let Some(p) = phon {
            ordinary.extend(p.variants(text));
        }
        for a in &ordinary {
            for r in root_variants {
                let concat = if is_prefix { format!("{a}{r}") } else { format!("{r}{a}") };
                if concat == fused {
                    return true;
                }
            }
        }
        if is_prefix {
            if let Some(p) = phon {
                for a in p.deletion_junctions(text) {
                    for r in root_stripped {
                        if format!("{a}{r}") == fused {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Bound on total composite chain length beyond the root (module doc, "Chaining"): a root plus at
/// most this many applied rules in one composite entry. `3` is the longest chain the recall gate
/// actually demanded (Amharic "ሌባዎቹ": root + def.m (CLEAN concatenation) + pl (fuses with def.m's
/// own ው) + poss.3m (fuses again) — the clean first step is why [`extend`] recurses through
/// non-dirty steps too, not just dirty ones); a grammar that genuinely needs a fourth stacked
/// fusion would show up as a recall-gate miss with an otherwise-empty class, at which point this
/// constant (or the P6 replace-rule successor, module doc) is the fix, not silently raising it
/// speculatively.
const MAX_EXTRA_RULES: usize = 3;

/// One in-progress composite chain step's context, threaded through [`extend`]'s recursion.
struct ExtendCtx<'a> {
    g: &'a Grammar,
    root_table: &'a CharDefTable,
    rules: &'a [(MRuleId, Role)],
    cache: &'a RuleCache,
    phon: Option<&'a PhonologyProbe<'a>>,
}

/// [`extend`]'s output accumulator: the composite records, a `(tag_lexc, spelling)` dedup set
/// (multiple root allomorphs, rule orders, or disjunctive rule allomorphs can converge on a
/// byte-identical entry — a hash set, not an `O(n²)` scan, since chains at [`MAX_EXTRA_RULES`]` = 3`
/// visit thousands of candidates), and the counts report.
struct Acc {
    recs: Vec<CompositeRec>,
    seen: rustc_hash::FxHashSet<(String, String)>,
    report: CompositeReport,
}

/// Try extending `base_word` (already carrying `chain`'s tags, `chain.last()` being the most
/// recently applied step) with every remaining candidate rule; recurse up to
/// [`MAX_EXTRA_RULES`]. `redundancy_variants`/`redundancy_stripped` are the "one side" strings the
/// ORDINARY (non-composite) lexc path would concatenate the OTHER side's literal spelling against
/// at THIS level: at depth 0 that is the root's own [`surface_variants`]/[`stripped_variants`]; at
/// depth ≥ 1 it is the SINGLE rendered surface of the composite chain built so far (the previous
/// level's own lexc entry is a fixed, already-decided string by the time a further rule's ordinary
/// entry would concatenate against it) — [`build_composites`]/this function's own recursive call
/// construct the right one for each depth, so [`reachable_via_ordinary_emission`] is checked
/// UNIFORMLY at every depth, never skipped by a `pre == post` shortcut (module doc's investigation:
/// a shortcut there is unsound whenever a rule's OWN LHS pattern silently drops part of what it
/// matched — e.g. Amharic's "ላ" ("to") rule's LHS consumes-but-does-not-copy the pronoun root's
/// leading glottal segment, so `pre` (the rule's own output) and `post` (after phonology) already
/// agree with EACH OTHER while both still differ from what `emit.rs`'s literal, whole-root-text
/// concatenation would produce — exactly the gap this composite mechanism exists to close).
#[allow(clippy::too_many_arguments)]
fn extend(
    ctx: &ExtendCtx,
    base_word: &Word,
    chain: &[(MorphemeId, String)],
    redundancy_variants: &[String],
    redundancy_stripped: &[String],
    depth: usize,
    width: usize,
    acc: &mut Acc,
) {
    if depth >= MAX_EXTRA_RULES {
        return;
    }
    let base_fs = base_word.syn_fs.clone();
    for &(mid, role) in ctx.rules {
        let rule = &ctx.g.mrules[mid.0 as usize];
        let (req, rule_morpheme) = rule_fs_and_morpheme(rule);
        // A rule already in this chain cannot apply again in the SAME composite (every reference
        // grammar's rules default `multipleApplication = 1`; a cheap guard against re-exploring the
        // same step, not a correctness requirement `synthesize` itself would enforce here).
        if chain.iter().any(|(m, _)| *m == rule_morpheme) {
            continue;
        }
        let req_fs = ctx.g.fs_interner.get(req);
        // Cheap pre-filter (module doc): the SAME unifiability check `hc_rules::morph::synth_syn_fs`
        // makes internally -- skip building/compiling for a pair that provably cannot match.
        if !req_fs.is_empty() && !is_unifiable(req_fs, &base_fs) {
            continue;
        }
        acc.report.pairs_probed += 1;

        for w in synthesize(ctx.g, base_word, rule) {
            let Some(segs) = surface_probe::probe_synthesize(ctx.g, &w.shape, ctx.cache) else {
                continue;
            };
            let Some(post) = surface_probe::render_nodes(ctx.root_table, &segs) else {
                continue;
            };
            if post.is_empty() {
                continue;
            }
            let is_infix = role == Role::Infix;
            let dirty = is_infix
                || !reachable_via_ordinary_emission(
                    ctx.root_table,
                    ctx.phon,
                    redundancy_variants,
                    redundancy_stripped,
                    rule,
                    role == Role::Prefix,
                    &post,
                );

            let mut next_chain = chain.to_vec();
            next_chain.push((rule_morpheme, tags::morph_tag_lexc(rule_morpheme, width)));

            // A dirty step is emitted as a composite carrying the WHOLE chain's tags; a clean step
            // is NOT emitted (the ordinary lexc entries already realize it correctly) but is still
            // recursed through below — the recall gate's "ሌባዎቹ" chain fuses only at steps 2 and 3,
            // with a byte-clean step 1 in between (see MAX_EXTRA_RULES's doc).
            if dirty {
                if let Some(tag_lexc) = morph_order_tags(&w, &next_chain) {
                    if acc.seen.insert((tag_lexc.clone(), post.clone())) {
                        acc.recs.push(CompositeRec {
                            morpheme: next_chain[0].0,
                            // `next_chain[0]` is always the seeding root; later elements are rules.
                            chain_morphemes: next_chain
                                .iter()
                                .enumerate()
                                .map(|(i, (m, _))| (i == 0, *m))
                                .collect(),
                            tag_lexc,
                            variants: vec![post.clone()],
                        });
                        if is_infix {
                            acc.report.interdigitation_entries += 1;
                        } else {
                            acc.report.fusion_entries += 1;
                        }
                    }
                    if is_infix {
                        acc.report.covered_infix_rules.insert(mid.0);
                    }
                }
            }

            // Recurse (module doc, "Chaining") — dirty or clean. The ordinary-emission redundancy
            // baseline one level deeper is THIS level's own rendered surface — and, ONLY when this
            // step was CLEAN, also its stripped (first-segment-removed) form: a clean stem is
            // realized by ordinary entries whose root half DOES have a `{roots}Stripped` sibling,
            // so a deletion-junction prefix one level up (Indonesian `meN` over a suffixed stem:
            // `tuliskan -> menuliskan`) is ordinary-reachable and must not read as dirty (measured:
            // without this, Indonesian grew 42 spurious fusion composites; with it, zero). After a
            // DIRTY step the stem exists ONLY as a composite entry, which has no Stripped sibling —
            // offering a stripped baseline there could mark a genuinely-needed deeper composite
            // clean, a downward (recall-losing) error, the plan's one forbidden direction.
            let deeper_variants = vec![post.clone()];
            let deeper_stripped = if dirty {
                Vec::new()
            } else {
                stripped_variants(ctx.root_table, &post)
                    .map(|(v, _)| v)
                    .unwrap_or_default()
            };
            extend(
                ctx,
                &w,
                &next_chain,
                &deeper_variants,
                &deeper_stripped,
                depth + 1,
                width,
                acc,
            );
        }
    }
}

/// Build every rule-application/fusion composite for `g` (module doc). `width` is the same tag
/// digit width [`crate::emit::emit`] computes; `phon` is the SAME [`PhonologyProbe`] instance
/// `emit.rs` already builds once per grammar (`None` for a grammar with no phonological rules at
/// all).
pub(crate) fn build_composites(
    g: &Grammar,
    width: usize,
    phon: Option<&PhonologyProbe>,
) -> (Vec<CompositeRec>, CompositeReport) {
    if !should_run(g, phon) {
        return (Vec::new(), CompositeReport::default());
    }
    let mut acc = Acc {
        recs: Vec::new(),
        seen: rustc_hash::FxHashSet::default(),
        report: CompositeReport::default(),
    };

    let rules = candidate_rules(g);
    let cache = RuleCache::build(g);

    for sd in &g.strata {
        for &entry_id in &sd.entries {
            let entry = &g.entries[entry_id.0 as usize];
            let root_stratum = g.morphemes[entry.morpheme.0 as usize].stratum;
            let root_table = &g.char_tables[g.strata[root_stratum.0 as usize].table.0 as usize];
            let entry_fs = g.fs_interner.get(entry.syn_fs);

            for allo in &entry.allomorphs {
                if allo.is_pattern {
                    continue;
                }
                let Some((root_variants, _)) = surface_variants(root_table, &allo.shape.text) else {
                    continue; // unsegmentable -- collect_roots already reports this once.
                };
                let root_stripped = stripped_variants(root_table, &allo.shape.text)
                    .map(|(v, _)| v)
                    .unwrap_or_default();

                let Ok(shape) =
                    hc_rules::shape_feat::segment_with_features(g, root_table, &allo.shape.text)
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
                // A fresh `ExtendCtx` per root: `root_table` is the OWNING stratum's table, which
                // can in principle differ per root in a multi-table grammar (module doc's
                // `f3_amharic_gate.rs`-documented hazard 2 — a non-issue for Amharic BY CONTENT,
                // since all 3 strata share one table, but not assumed here).
                let root_ctx = ExtendCtx {
                    g,
                    root_table,
                    rules: &rules,
                    cache: &cache,
                    phon,
                };
                extend(
                    &root_ctx,
                    &word,
                    &chain0,
                    &root_variants,
                    &root_stripped,
                    0,
                    width,
                    &mut acc,
                );
            }
        }
    }

    (acc.recs, acc.report)
}
