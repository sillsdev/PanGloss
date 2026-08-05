//! The FST precision knob, step 1 only: the [`ConstraintCatalog`] for the
//! GATE-CONSTRAINT **ENVIRONMENT** family (allomorph-selection environments,
//! where v1's emitter today ([`crate::emit`]) emits every allomorph permissively regardless of its
//! declared `RequiredEnvironments`/`ExcludedEnvironments` and lets HC confirm prune the
//! wrong-environment candidates), [`PrecisionAction`]/[`PrecisionConfig`], and the `AllFlags`
//! preset's flag-emission hookup `crate::emit::emit_with_precision` drives.
//!
//! ## Architecture reminder (design §0)
//! FST proposes, HC confirm always replays the real engine and prunes. This knob is
//! **performance-only** — it can never change *which* analyses come out, only how many false
//! candidates confirm must kill and how big the network is. Recall must stay 100% at every
//! setting; `PrecisionConfig::Strip` (v1's existing fully-permissive behavior) is the default so
//! nothing changes unless a caller explicitly opts in to `AllFlags`
//! (`tests/pk1_precision_recall_invariance.rs` is the harness proving this).
//!
//! ## Why only *some* environment instances are covered this step
//! `pg_grammar::model::{RootAllomorphDef, AffixAllomorphDef}.environments: Vec<EnvironmentDef>`
//! is the ENVIRONMENT family's real surface (verified: `crate::emit` never reads this field at
//! all today — every allomorph is emitted regardless of its environments, i.e. v1 is `Strip`
//! everywhere already). A `RequiredEnvironments`/`ExcludedEnvironments` entry's `left`/`right` are
//! PHONETIC patterns matched against the surrounding word shape at the real engine's
//! `pg_rules::validity::environments_ok` (see that module) — genuinely diverse: literal segment
//! runs, natural-class references, word-edge anchors, or combinations. Encoding an environment as
//! a flag diacritic is only SAFE (equivalence-preserving, never a recall risk) for a narrow shape
//! this step covers; everything else is deliberately left `Strip` (i.e. behaves exactly as v1
//! already does) and reported as `Unsupported` with a reason, per the task's "report rather than
//! approximate downward" instruction. Three findings drove the cut line (a fourth, PK2's
//! oracle-verified U/R/D-vs-E/N/C/P restriction, is inherited directly from
//! `tests/pk2_eliminate_flag_oracle.rs` and is why this module never emits anything but
//! require/set — no unify, no equal, and (finding 4) no disallow either:
//!
//! 1. **OR, not AND (`pg_rules::validity::environments_ok`: `envs.iter().any(...)`).** An
//!    allomorph is valid if AT LEAST ONE of its declared environments holds. Gating each
//!    `EnvironmentDef` on an allomorph with >1 environment independently, each with its own
//!    require flag, would silently turn that disjunction into a conjunction — a genuine recall
//!    bug the recall-invariance harness might not even catch if the corpus never exercises the
//!    second disjunct. [`classify`] therefore requires `sibling_count == 1` (the owning
//!    allomorph's *entire* `environments` list is this one instance) before considering any other
//!    shape check; every multi-environment allomorph is `Unsupported { reason: "or-ambiguous" }`
//!    regardless of how simple its individual environments look.
//! 2. **Right context needs a mechanism this step does not build.** A `require` flag can only
//!    ever inspect PAST state (whatever an earlier symbol on the same path already set) — by the
//!    time an allomorph's own entry is being walked, nothing to its right has been read yet. A
//!    right-environment constraint is realizable via the standard "positive-set now, disallow on
//!    every non-matching downstream entry" technique, but that requires reaching into EVERY
//!    possible immediately-following entry across the whole network (however many lexicons away
//!    once optional/epsilon derivation levels are accounted for) — exactly the kind of
//!    wide-blast-radius, hard-to-verify transform this step declines, per the task's explicit
//!    permission to leave anything not soundly verifiable as `Strip`. Left context has no such
//!    problem: the conditioning material has ALREADY been consumed (and so already had the chance
//!    to set a flag) by the time this allomorph's own `@R@`/`@D@` is checked.
//! 3. **Word-edge anchors are not flag-representable at all**, not just deferred. An environment
//!    whose pattern is *only* `PatternNode::Anchor` (Sena's `/ _ #`, "must be word-final") is a
//!    claim about the ABSENCE of further input, not about any state a flag can carry forward — no
//!    `@R@`/`@D@` check, wherever placed, can express "nothing else may follow" (every accepted
//!    word eventually reaches *some* accepting state, so any flag "set at every accept point" is
//!    vacuously true). The only exact encoding is structural (routing this allomorph's own
//!    continuation directly to `#`, i.e. `PrecisionAction::Eliminate`, not `KeepFlag`) — out of
//!    scope for the `AllFlags` preset this step implements (design §3: "every environment
//!    constraint as a KeepFlag"). [`classify`] reports these `Unsupported { reason:
//!    "anchor-or-compound-left" }` (folded into the same left-shape check as any other non-literal
//!    left pattern) rather than silently mis-encoding them.
//! 4. **`ExcludedEnvironments` (a left `@D@` disallow) is left out of THIS step's scope**, even
//!    though the corrected adjacency-correct encoding below (unconditional per-entry `@P@`
//!    overwrite, never a bare "seen once, stays set" flag) would make a symmetric `@D.ENV{id}.y@`
//!    disallow just as sound as the `@R@` require this step DOES emit — the exact same y/n verdict
//!    that feeds `@R@` would feed `@D@` equally correctly. This step still declines it
//!    (`require == false` → `Unsupported { reason: "exclude-left-persistent-unsound" }`) on
//!    conservative scope grounds: a recall-invariance corpus with zero exclude environments (Sena:
//!    144 requires, 0 excludes; Indonesian: 0 environments) cannot exercise a newly-added exclude
//!    arm at all, so adding it here would be an UNTESTED change riding along with a verified one.
//!    Reason tag kept unchanged so a future step that lifts this restriction only needs to touch
//!    [`classify`]'s one `if !env.require` line, not rename anything already in the field.
//!
//! Left-literal, single-environment, REQUIRE instances (`/mb_`-shaped: `RequiredEnvironments` whose
//! `LeftEnvironment` is one plain `<Segments><PhoneticShape>` run, no `RightEnvironment`) are the
//! one shape left standing, and they ARE soundly flag-representable: see [`EnvCoverage::LeftLiteral`]
//! and the "Emission mechanism" section below.
//!
//! ## Two failed encodings, and why (the adjacency finding)
//! An HC left-environment is an ADJACENCY constraint — "this morph is immediately preceded by
//! material ending in literal `L`" — not "L appeared somewhere earlier in the word." Step 1's
//! first two attempts both got this wrong, in opposite directions:
//!
//! 1. **Whole-literal persistent flag** (set `@P.ENV.y@` only on an entry whose surface ends with
//!    the FULL literal `L`, verbatim): UNDER-generated. The engine's left context can be assembled
//!    across a morpheme boundary — `L = "mi"` completed by a preceding morpheme "m" then a
//!    morpheme "i", where NO single emitted entry's surface is the whole string "mi" — so an
//!    `ends_with(L)` test on one entry's own text misses every context split this way. Found via
//!    Indonesian's "miseru": Strip confirmed an analysis AllFlags did not. Fatal (recall is the one
//!    invariant this knob may never break).
//! 2. **All-suffixes breadth + a synthesized micro-lexicon per occurrence** (the fix that
//!    over-corrected): to close the boundary-splitting gap, the set-side test was broadened to
//!    "surface ends with ANY non-empty suffix of `L`" (recall-safe — closes the miseru gap exactly
//!    since the entry adjacent to the boundary always ends in some suffix of `L`), but the flag
//!    itself was spliced in via a freshly synthesized `LEXICON PkGate{n}` per MATCHING ENTRY
//!    OCCURRENCE (not deduped, not shared). Recall held, but the network exploded: Sena's Bantu
//!    morphology means nearly every morpheme ends in `-a`, so almost every entry got its own
//!    private one-line lexicon, and the compile blew up to ~1.5 GB. Independently, the exact
//!    symbol strings this version emitted (`@R.ENV.nnnn@` / `@P.ENV.nnnn.1@`, embedding a literal
//!    `.` inside the flag NAME field) turned out to be a second, silent bug: foma-rs's `flag_check`
//!    DFA (`crates/foma/src/flags.rs`, verified empirically against the real crate) treats every
//!    dot-delimited run after the type letter as ANOTHER field, so `@P.ENV.nnnn.1@` (three fields
//!    after `P`) fails `flag_check` OUTRIGHT (not a flag at all — an ordinary literal multichar
//!    symbol no real surface text can ever match), while `@R.ENV.nnnn@` (two fields, legal for the
//!    value-optional R/D grammar) silently parses as name **"ENV"** with value **"nnnn"** — every
//!    constraint sharing the SAME flag name "ENV", distinguished only by value. Never caught
//!    because the size blowup was the loud failure that got investigated first.
//! 3. **A THIRD trap, found while building the corrected inline encoding below**: splicing a flag
//!    symbol directly onto a surface's LOWER-tape text (`"seru@P.ENV10.n@"`) FAILS TO MATCH AT ALL
//!    — for ANY surface — whenever the flag's own text contains a literal `0` digit ANYWHERE,
//!    even when lexc-escaped `%0` the way `crate::tags::lexc_tag` escapes tag numerals (verified
//!    empirically, bisecting a real compiled network down to a single symbol: `@P.ENV10.n@` and
//!    `@P.ENV1%0.n@` both fail identically; `@P.ENV1Z.n@`, zero-free, works). `%`-escaping is only
//!    proven for a tag symbol occupying an entire lexc side ALONE — the ONLY way this crate had
//!    ever used it before this module — not for a symbol appended after ordinary characters on the
//!    SAME side, which is what every set-side flag here does. [`flag_id`] avoids the digit
//!    entirely rather than escaping it.
//!
//! The corrected design (below) fixes all three: it keeps the all-suffixes-superset SET-side
//! breadth's *successor* — a same-adjacency, over-approximated y/n test per entry (§ "Emission
//! mechanism") — but drops the per-occurrence micro-lexicon (inline flags instead, §ibid.), drops
//! the dotted flag-name format, AND drops every literal `0` digit ([`flag_id`]: `ENV{id}`, `0`
//! digits replaced, never escaped) — so every constraint gets a distinct, `flag_check`-valid,
//! zero-free name, never sharing state with another and never silently failing to match.
//!
//! ## Emission mechanism (the `AllFlags` preset, [`PrecisionEmit`])
//! For each covered [`EnvConstraint`] (`EnvCoverage::LeftLiteral { literal_variants }`), three
//! flag symbols are minted from the constraint's own `id` (never from `attr`, which embeds a `.`
//! for human-readable reporting only — see [`flag_id`]'s doc for why that must never reach an
//! actual flag symbol): a require `@R.ENV{id}.y@` and two positive-set symbols `@P.ENV{id}.y@` /
//! `@P.ENV{id}.n@`. `crate::emit::write_tag_entry` is the single choke point every literal
//! spelling in the whole emitter passes through (verified by inspection: roots, affix derivation/
//! slot chains, and P1d composite entries all call it, nothing writes a tagged entry any other
//! way) — [`PrecisionEmit::tagged_lower`] is what it calls to build the entry's LOWER-tape text:
//!
//! - **Set side, EVERY entry, unconditionally when its surface is non-empty**: for EACH covered
//!   constraint, appends EXACTLY ONE of `@P.ENV{id}.y@` (the entry's surface [`could_satisfy`] the
//!   context — an over-approximated "yes, adjacency-wise this could be the immediately-preceding
//!   material") or `@P.ENV{id}.n@` (definitely not) — never neither. Because `@P@` (positive set)
//!   OVERWRITES the attribute's value unconditionally every time it fires, and every non-empty
//!   entry fires exactly one of `y`/`n`, the value visible at any later point is always the MOST
//!   RECENT non-empty morph's own verdict — true adjacency, not "ever seen." An entry with an
//!   EMPTY surface emits NEITHER symbol (module doc requirement, and correct: a zero morph has no
//!   phonetic shape of its own, so it must leave whatever the previous real morph already set
//!   untouched, exactly matching the real engine, which only ever inspects the assembled PHONETIC
//!   shape).
//! - **Owner side**: the OWNING allomorph's own entries (identified the same way already
//!   threaded above — `Some(allo.id)` from `crate::emit::emit_rule_allomorphs`,
//!   `Some(root.id)` from `write_root_entries`/`write_stripped_root_entries`) additionally get
//!   `@R.ENV{id}.y@` PREPENDED (require: only `y` is ever meaningful here — exclude is
//!   declined, finding 4).
//!   `@R@` reads whatever the immediately preceding non-empty entry's `@P@` last set — at word
//!   start nothing has set anything, so `@R@` correctly fails there (a left-literal environment can
//!   never hold with no left context at all).
//! - **The y-test ([`could_satisfy`]) over-approximates, never under-approximates**: an entry's
//!   surface gets `y` if, for ANY of the constraint's literal variants `L`, the surface
//!   `ends_with(L)` OR the surface is a PROPER SUFFIX of `L` (i.e. shorter than `L` and `L`
//!   `ends_with` the surface) — the second disjunct is exactly the boundary-splitting case that
//!   broke "miseru" under encoding 1 above (a preceding morpheme "m" then a morph "i" jointly
//!   spell `L = "mi"`; the "i" entry alone is a proper suffix of "mi", so it gets `y`). Every
//!   caller already writes ONE lexc entry per rendered spelling variant (`surface_variants`/
//!   `pattern_variants`/`PhonologyProbe::variants`'s enumeration, each its own
//!   `crate::emit::write_tag_entry` call) — so "any of the entry's own rendered variants" is
//!   already handled by construction; [`could_satisfy`] only needs to range over the
//!   CONSTRAINT's own literal variants for the ONE surface it is called with.
//!
//! **Why this is safe to splice directly into the entry's own upper:lower string** (unlike stage
//! 1's per-occurrence micro-lexicon, which existed SPECIFICALLY to avoid this): lexc's alignment
//! (`lexc_pad`, `foma-rs`'s `crates/foma/src/lexcread.rs`) pads the SHORTER of the upper/lower
//! token sequences with trailing epsilons once it runs out. Since every entry's UPPER side here is
//! the tag symbol ALONE (module doc, "Tag tape convention") — always 1 token, always shorter than
//! or equal to the LOWER (surface + flags) side whenever the surface is non-empty — every flag
//! token appended to the LOWER text lands paired with an upper-side EPSILON (or, for a 1-character
//! surface, possibly paired with the tag itself; either way the flag symbol always sits on the
//! LOWER tape). That is exactly the side `apply`'s UP-direction matching reads real input against
//! (`foma-rs::apply::apply_follow_next_arc`: `symin = l_out` in UP mode) — the side flag-diacritic
//! recognition and `apply_match_length`'s ZERO-WIDTH consumption both key off. Placement (start,
//! middle, end of the LOWER string) therefore doesn't matter for correctness — verified empirically
//! against a real compiled network (require+set+adjacency+empty-morph-preserves-value, all four
//! cases) during this step's implementation, not merely reasoned about; see this module's own
//! `tests` for the equivalent property tests. `escape_lexc_text` is applied ONLY to the real
//! surface text, never to a flag symbol (flag symbols are built already lexc-safe by
//! construction, ASCII letters/digits/`@`/`.` only, escaped zeros — see [`flag_id`]).
//!
//! Everything above is gated on [`PrecisionEmit::build`] only ever populating its lookup tables
//! when `config` is [`PrecisionConfig::AllFlags`] — under the default [`PrecisionConfig::Strip`],
//! [`PrecisionEmit::tagged_lower`] always returns exactly what the pre-precision-knob emitter wrote
//! (`escaped` unchanged, or lexc's `0` epsilon marker for an empty surface), which is what makes
//! `crate::emit::emit`'s byte-identical-to-before guarantee a property of the CODE PATH itself
//! (one implementation, exercised both ways) rather than something a second, forked emitter would
//! need to be kept in sync with by hand. No `LEXICON` blocks are ever synthesized by this module —
//! network size grows by AT MOST `entries × coverable_constraints` extra inline symbol tokens,
//! linearly, by construction (never a new state/lexicon per occurrence, the mechanism that
//! blew Sena's compile up in the second failed encoding above).
//!
//! ## `@P`/`@R`-typed flags are NOT eliminable (PK2's finding, inherited here)
//! `tests/pk2_eliminate_flag_oracle.rs`'s headline finding: foma-rs's `flag_build` decision table
//! (`crates/foma/src/flags.rs`, a bug-for-bug port of the real C table) has rows only for
//! eliminated-type U/R/D — eliminating an E/N/C/P-typed attribute silently degrades to STRIP
//! (illegal paths become reachable) while still calling itself "eliminated". This module's flags
//! are `@P@` (set) and `@R@` (require) — R is in the safe (U/R/D) list, but **P is not**, and the
//! two are only ever eliminated TOGETHER (removing the require without the matching set values, or
//! vice versa, is meaningless) — so an ENVIRONMENT-family constraint may only ever be assigned
//! [`PrecisionAction::KeepFlag`] or [`PrecisionAction::Strip`]. A future `PrecisionTuner` (design
//! §8 item (3)) must never route a [`ConstraintFamily::Environment`] instance to
//! [`PrecisionAction::Eliminate`] — [`ConstraintCatalog::decide`] already only ever produces
//! `KeepFlag`/`Strip` for this family (no code path assigns `Eliminate` today), so this is
//! currently true by omission; this note exists so a later step's tuner doesn't "fix" that by
//! adding one.
//!
//! ## Deliberately out of scope this step (extensibility, not a limitation of the *design*)
//! - Every OTHER gate-constraint family design §2 lists (MPR gating, stem names, HeadFeatures
//!   re-check, compounding FS gates, morpheme/allomorph co-occurrence, bound-root, obligatory
//!   features, W3.2 free-fluctuation, circumfix pairing) and the whole rewrite-rule arm
//!   (`Compose`/`Optionalize`/`Skip`) are represented only as enum variants below, unpopulated —
//!   [`PrecisionAction`] and [`ConstraintFamily`] carry the extra variants so a later step's types
//!   don't need an enum-breaking change, but nothing in this crate ever produces them yet.
//! - `PrecisionConfig::FullCompile`/`Auto` and `PrecisionTuner`/measured before/after network
//!   sizes in `PrecisionReport` (design §3): later steps (design §8 items (3)-(5)). `crate::emit`
//!   treats every config other than `AllFlags` identically to `Strip`.

use std::collections::BTreeMap;

use pg_grammar::model::{
    AllomorphId, EnvironmentDef, Grammar, MRuleId, MorphRuleDef, Pattern, PatternNode, PhonRuleDef,
    TableId,
};

use crate::emit::{allomorphs_of, surface_variants};

// --- Mechanism families (design §2's table) -----------------------------------------------------

/// Which mechanism family a gate-constraint (or rewrite-rule) instance belongs to. Only
/// [`ConstraintFamily::Environment`] is ever populated by [`ConstraintCatalog::build`] this step;
/// the rest exist so later steps' catalogs can grow into this same enum without a breaking change
/// (module doc, "Deliberately out of scope").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintFamily {
    /// Allomorph-selection environments (`RequiredEnvironments`/`ExcludedEnvironments`) — the
    /// only family this step populates.
    Environment,
    /// MPR feature gating on an allomorph (`required_mpr`/`excluded_mpr`). Not populated.
    Mpr,
    /// `StemName` restriction on a root allomorph. Not populated.
    StemName,
    /// `HeadFeatures` re-check. Not populated.
    HeadFeatures,
    /// Compounding-rule FS gates (head/non-head/output). Not populated.
    CompoundingFs,
    /// `MorphemeCoOccurrenceRule`. Not populated.
    MorphemeCoOccurrence,
    /// `AllomorphCoOccurrenceRule`. Not populated.
    AllomorphCoOccurrence,
    /// Bound-root ("cannot be the word's only allomorph") gate. Not populated.
    BoundRoot,
    /// `outputObligatoryFeatures`. Not populated.
    ObligatoryFeatures,
    /// W3.2 free-fluctuation / disjunctive-allomorph re-check. Not populated.
    FreeFluctuation,
    /// Circumfix prefix/suffix pairing. Not populated.
    Circumfix,
}

/// Which allomorph registry an environment instance is attached to (mirrors
/// `pg_grammar::model`'s two `environments: Vec<EnvironmentDef>` sites).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvOwnerKind {
    Root,
    Rule,
}

/// Whether, and how, this step can soundly encode one [`EnvironmentDef`] instance as a flag
/// diacritic (module doc's three findings).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvCoverage {
    /// A single-environment, `require == true`, no-right, single-literal-segment-run left context
    /// (no natural class / anchor / quantifier) — safe to encode as a real `KeepFlag` (module doc
    /// findings 1-4: every other shape, and every exclude, is `Unsupported`). `literal_variants` is
    /// every accepted spelling of the left pattern's literal text ([`surface_variants`] — the SAME
    /// representation-cartesian-product/NFD-normalization convention `crate::emit`'s own root/
    /// affix spellings use, so a plain `str::ends_with` comparison against an emitted entry's
    /// surface is apples-to-apples).
    LeftLiteral { literal_variants: Vec<String> },
    /// Declined this step — always behaves as `Strip` under every preset. `reason` is a short,
    /// machine-stable tag (module doc): `"or-ambiguous"`, `"exclude-left-persistent-unsound"`,
    /// `"right-context"`, `"anchor-or-compound-left"`, `"non-literal-left"`, `"no-pattern"`,
    /// `"overflow"`, `"unsegmentable"`, `"prule-tail-rewrite-risk"` (new finding 5:
    /// [`prule_tail_rewrite_risk`] — a phonological rewrite rule's output could plausibly combine
    /// into the literal in a way this emitter's purely textual surface spellings would never show).
    Unsupported { reason: &'static str },
}

/// One gate-constraint instance of the ENVIRONMENT family: one [`EnvironmentDef`] entry on one
/// root or rule allomorph, with a stable, deterministic id/attribute name (design §3: "Stable IDs
/// ⇒ deterministic tuning and diffable reports").
#[derive(Debug, Clone)]
pub struct EnvConstraint {
    /// Stable, deterministic (assignment order = a fixed grammar walk, module doc /
    /// [`ConstraintCatalog::build`] — never re-sorted or hashed) — the SAME grammar always
    /// produces the SAME ids in the SAME order.
    pub id: u32,
    /// The flag-attribute name (design §2/§3: `ENV.nnnn`, zero-padded to at least 4 digits — the
    /// design's own worked example, `ENV.0017`).
    pub attr: String,
    pub family: ConstraintFamily,
    pub owner_kind: EnvOwnerKind,
    /// The allomorph this [`EnvironmentDef`] is declared on.
    pub allomorph: AllomorphId,
    /// Index into the owning allomorph's own `environments` list.
    pub env_index: usize,
    /// `true` = `RequiredEnvironments` (`ConstraintType::Require`), `false` =
    /// `ExcludedEnvironments`.
    pub require: bool,
    /// The owning allomorph's TOTAL environment count (module doc finding 1 — `> 1` forces
    /// `Unsupported` regardless of this instance's own shape).
    pub sibling_count: usize,
    pub coverage: EnvCoverage,
}

impl EnvConstraint {
    /// Whether the `AllFlags` preset assigns this instance `KeepFlag` (vs. `Strip`).
    pub fn is_coverable(&self) -> bool {
        matches!(self.coverage, EnvCoverage::LeftLiteral { .. })
    }
}

/// Walks a [`Grammar`] and enumerates every gate-constraint instance of the ENVIRONMENT family
/// (design §3). Extensible to the other families design §2 lists (see [`ConstraintFamily`]) —
/// only `env` is populated this step.
#[derive(Debug, Clone, Default)]
pub struct ConstraintCatalog {
    pub env: Vec<EnvConstraint>,
}

impl ConstraintCatalog {
    /// Deterministic walk order: strata (document order) → each stratum's entries (document
    /// order) → each entry's allomorphs (document order) → each allomorph's `environments`
    /// (document order) for roots; then `g.mrules` in `Vec` index order → each rule's allomorphs
    /// (document order, via [`allomorphs_of`]) → `environments` (document order) for rules. Same
    /// convention `crate::emit::collect_roots`/`emit_rule_allomorphs` already walk in, so a
    /// grammar's ids never depend on anything but the grammar itself.
    pub fn build(g: &Grammar) -> Self {
        let mut env = Vec::new();
        let mut next_id = 0u32;

        for sd in &g.strata {
            for &entry_id in &sd.entries {
                let entry = &g.entries[entry_id.0 as usize];
                for allo in &entry.allomorphs {
                    push_instances(
                        &mut env,
                        &mut next_id,
                        g,
                        EnvOwnerKind::Root,
                        allo.id,
                        &allo.environments,
                    );
                }
            }
        }

        for mid in (0..g.mrules.len() as u32).map(MRuleId) {
            if matches!(g.mrules[mid.0 as usize], MorphRuleDef::Compounding(_)) {
                // No allomorph environments live on a compounding rule (module doc; `allomorphs_of`
                // already returns `&[]` for this variant, but skip explicitly for clarity).
                continue;
            }
            for allo in allomorphs_of(g, mid) {
                push_instances(
                    &mut env,
                    &mut next_id,
                    g,
                    EnvOwnerKind::Rule,
                    allo.id,
                    &allo.environments,
                );
            }
        }

        ConstraintCatalog { env }
    }

    /// Every instance the `AllFlags` preset can soundly cover this step.
    pub fn coverable(&self) -> impl Iterator<Item = &EnvConstraint> {
        self.env.iter().filter(|c| c.is_coverable())
    }
}

fn push_instances(
    out: &mut Vec<EnvConstraint>,
    next_id: &mut u32,
    g: &Grammar,
    owner_kind: EnvOwnerKind,
    allomorph: AllomorphId,
    envs: &[EnvironmentDef],
) {
    for (env_index, env) in envs.iter().enumerate() {
        let id = *next_id;
        *next_id += 1;
        out.push(EnvConstraint {
            id,
            attr: format!("ENV.{id:04}"),
            family: ConstraintFamily::Environment,
            owner_kind,
            allomorph,
            env_index,
            require: env.require,
            sibling_count: envs.len(),
            coverage: classify(g, env, envs.len()),
        });
    }
}

/// One [`EnvironmentDef`]'s coverage classification (module doc's findings, in order).
fn classify(g: &Grammar, env: &EnvironmentDef, sibling_count: usize) -> EnvCoverage {
    if sibling_count != 1 {
        return EnvCoverage::Unsupported {
            reason: "or-ambiguous",
        };
    }
    // Finding 4 (module doc): exclude-left is out of THIS step's scope (conservative — the
    // recall-invariance corpus has zero exclude environments to test an exclude arm against), not
    // because the corrected mechanism below couldn't represent it.
    if !env.require {
        return EnvCoverage::Unsupported {
            reason: "exclude-left-persistent-unsound",
        };
    }
    if env.right.is_some() {
        return EnvCoverage::Unsupported {
            reason: "right-context",
        };
    }
    let Some(left) = &env.left else {
        return EnvCoverage::Unsupported {
            reason: "no-pattern",
        };
    };
    if left.nodes.len() != 1 {
        return EnvCoverage::Unsupported {
            reason: "anchor-or-compound-left",
        };
    }
    let PatternNode::Segments {
        table: seg_table,
        shape,
    } = &left.nodes[0]
    else {
        return EnvCoverage::Unsupported {
            reason: "non-literal-left",
        };
    };
    let literal_table = &g.char_tables[seg_table.0 as usize];
    match surface_variants(literal_table, &shape.text) {
        Some((variants, false)) if !variants.is_empty() => {
            // New finding 5 (module doc): decline whenever a phonological rewrite rule could
            // plausibly rewrite word-internal material into (a suffix of) this literal — this
            // emitter's textual surface spellings would never show that, so the y-test below could
            // silently under-set `y` for a real context. Declining is always recall-safe (Strip).
            if prule_tail_rewrite_risk(g, &variants) {
                EnvCoverage::Unsupported {
                    reason: "prule-tail-rewrite-risk",
                }
            } else {
                EnvCoverage::LeftLiteral {
                    literal_variants: variants,
                }
            }
        }
        Some((_, true)) => EnvCoverage::Unsupported { reason: "overflow" },
        _ => EnvCoverage::Unsupported {
            reason: "unsegmentable",
        },
    }
}

/// New finding 5 (module doc): `true` when we cannot cheaply PROVE that no phonological rewrite
/// rule in `g` could ever rewrite word-internal material into (a suffix of) one of
/// `literal_variants` — the safe default whenever this can't be decided cheaply, per the
/// architecture's "approximate only upward, never guess downward" rule (a wrong `y`/`n` from
/// [`could_satisfy`] IS a downward approximation risk, unlike an over-eager `y`, which is always
/// safe). `false` (no risk at all) for the common case of a grammar with zero phonological rules
/// (Sena) — the loop below is then a no-op.
fn prule_tail_rewrite_risk(g: &Grammar, literal_variants: &[String]) -> bool {
    for prule in &g.prules {
        let PhonRuleDef::Rewrite(rule) = prule else {
            // A `MetathesisRule` has no literal RHS pattern at all to render (it reorders/
            // feature-unions existing spans) -- cannot cheaply prove it can't produce the literal.
            return true;
        };
        for subrule in &rule.subrules {
            if subrule.rhs.nodes.is_empty() {
                continue; // Pure deletion: produces no new text at all -- no risk.
            }
            let Some(rendered) = render_pattern_literal(g, &subrule.rhs) else {
                return true; // Non-literal RHS shape: cannot cheaply prove no overlap.
            };
            if rendered.iter().any(|text| {
                !text.is_empty() && literal_variants.iter().any(|l| l.contains(text.as_str()))
            }) {
                return true;
            }
        }
    }
    false
}

/// Attempts to render `pattern`'s literal spelling for [`prule_tail_rewrite_risk`]'s substring
/// check. `Some(variants)` only when EVERY node is a [`PatternNode::Segments`] sharing the SAME
/// char-def table (the same literal shape [`classify`]'s own left-pattern check already accepts) —
/// `None` (cannot cheaply render, caller must decline) for a `Context`/`CharDef`/`Quantifier`/
/// `Anchor` node, mixed tables, or a representation-variant overflow.
fn render_pattern_literal(g: &Grammar, pattern: &Pattern) -> Option<Vec<String>> {
    let mut table_id: Option<TableId> = None;
    let mut text = String::new();
    for node in &pattern.nodes {
        let PatternNode::Segments { table, shape } = node else {
            return None;
        };
        match table_id {
            None => table_id = Some(*table),
            Some(t) if t == *table => {}
            Some(_) => return None,
        }
        text.push_str(&shape.text);
    }
    let table = &g.char_tables[table_id?.0 as usize];
    match surface_variants(table, &text) {
        Some((variants, false)) => Some(variants),
        _ => None, // Overflow or unsegmentable: can't cheaply prove no overlap either.
    }
}

/// Lexc-safe embedding of a constraint's `id` for use INSIDE a flag diacritic's name field
/// (`@[R|P].ENV{id}.[y|n]@`) — NEVER `EnvConstraint::attr`, which embeds a `.` for human-readable
/// reporting only. Two independent reasons this must be dot-free AND zero-digit-free:
/// - foma-rs's `flag_check` DFA (`crates/foma/src/flags.rs`, verified empirically against the
///   real crate) treats every dot-delimited run after the type letter as ANOTHER field: a name
///   containing a literal `.` (e.g. the old `"ENV.0007"`) makes a P/U/N/E-typed symbol (exactly 2
///   fields allowed) INVALID — not a flag at all, silently degrading to an ordinary literal
///   multichar symbol no real surface text can ever match — while an R/D-typed symbol (value
///   optional) silently SPLITS at the embedded dot, giving every constraint the SAME flag name
///   ("ENV") distinguished only by value, i.e. one shared piece of cross-constraint state instead
///   of independent ones. This step's format (`ENV{id}`, no dot) keeps every constraint's name
///   distinct and always `flag_check`-valid.
/// - A literal `0` digit ANYWHERE in a flag symbol's text breaks matching for that WHOLE symbol
///   once it is spliced next to other text on the same lexc tape (verified empirically against a
///   real compiled network during this step: `@P.ENV10.n@` and even the lexc-escaped
///   `@P.ENV1%0.n@` — `crate::tags::lexc_tag`'s own zero-escaping convention — BOTH fail to match
///   at all when appended after a surface like `"seru"`; `@P.ENV1Z.n@`, with the zero digit
///   replaced, works correctly. `crate::tags::lexc_tag`'s `%0` convention is only proven for a tag
///   symbol occupying an ENTIRE lexc side alone (its only use before this module) — a symbol
///   spliced onto the END of ordinary surface text is a materially different case this crate had
///   never exercised, and `%`-escaping does not fix it there. [`flag_id`] therefore avoids the
///   digit `0` altogether: `Z` substitutes for it (never itself produced by `u32::to_string`, so
///   the substitution is injective — no two ids can ever collide).
fn flag_id(id: u32) -> String {
    id.to_string().replace('0', "Z")
}

/// The y-test (module doc, "Emission mechanism"): does `surface` (one entry's own concrete,
/// already-rendered spelling — every caller already writes one lexc entry PER variant, so no
/// further variant enumeration is needed here) satisfy the adjacency context for ANY of
/// `literal_variants`? Two disjuncts, both upward-safe:
/// - `surface.ends_with(l)`: the whole literal is spelled out within this one entry's own text.
/// - `surface` is a PROPER SUFFIX of `l` (shorter than `l`, and `l.ends_with(surface)`): the
///   literal's context is completed ACROSS a morpheme boundary by whatever comes before this entry
///   (the "miseru" cross-boundary case — module doc, "Two failed encodings").
///   Empty literal variants never match (an empty environment literal is meaningless and would
///   match trivially/vacuously otherwise). When in doubt this returns `true` — never narrows.
fn could_satisfy(surface: &str, literal_variants: &[String]) -> bool {
    literal_variants.iter().any(|l| {
        !l.is_empty()
            && (surface.ends_with(l.as_str()) || (surface.len() < l.len() && l.ends_with(surface)))
    })
}

// --- PrecisionAction / PrecisionConfig / PrecisionReport (design §3) ----------------------------

/// One of the three positions (design §1) a gate-constraint instance can be assigned, plus the
/// rewrite-rule arm's three positions (unwired this step — module doc, "Deliberately out of scope").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrecisionAction {
    // Gate-constraint arm (design §1, position list 1).
    /// Compiled into network topology (`eliminate flag`) — exact, zero lookup cost. Never assigned
    /// by anything in this crate yet (design §8 item (2), the PK2 oracle gate, and item (3), the
    /// tuner, are later steps) — kept as a variant so [`PrecisionReport`]'s decision type doesn't
    /// need to change shape when they land.
    Eliminate,
    /// Flag stays in the network; `apply_up` obeys it at runtime. This step's `AllFlags` preset
    /// assigns this to every [`EnvConstraint::is_coverable`] instance.
    KeepFlag,
    /// Fully permissive — no flag at all (v1's existing, only-ever behavior). Assigned to every
    /// `Unsupported` instance under every preset, and to everything under
    /// [`PrecisionConfig::Strip`].
    Strip,
    // Rewrite-rule arm (design §1, position list 2) — stubbed, unwired (design §8 item (4)).
    Compose,
    Optionalize,
    Skip,
}

/// The global precision knob (design §3). [`PrecisionConfig::Strip`] is v1's existing,
/// fully-permissive emitter behavior and is the `Default` so nothing changes anywhere unless a
/// caller explicitly opts in to `AllFlags` (`crate::emit::emit` always passes `Strip`;
/// `crate::emit::emit_with_precision` is the opt-in entry point).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrecisionConfig {
    #[default]
    Strip,
    /// "Simplest FST" (design §3): every `AllFlags`-coverable environment constraint (this step:
    /// [`EnvCoverage::LeftLiteral`], a require-only left-literal instance) is emitted as a real
    /// `@R@`+`@P@` flag scheme (`crate::precision::PrecisionEmit`); everything else stays `Strip`,
    /// exactly as today.
    AllFlags,
    /// "All FST" (design §3) — later step (design §8 item (3), the tuner). `crate::emit` treats
    /// this identically to `Strip` for now (no `Eliminate` arm exists yet).
    FullCompile,
    /// The half-and-half budget dial (design §3) — later step. `crate::emit` treats this
    /// identically to `Strip` for now.
    Auto(u32),
}

/// One constraint's decision record (design §3's `PrecisionReport`, step-1 shape — no measured
/// before/after network sizes yet; that needs `PrecisionTuner`'s trial-elimination machinery,
/// design §8 item (3), not built this step).
#[derive(Debug, Clone)]
pub struct ConstraintDecision {
    pub id: u32,
    pub attr: String,
    pub family: ConstraintFamily,
    pub action: PrecisionAction,
    /// `Some` for a `Strip` decision reached because [`EnvCoverage::Unsupported`] said so (the
    /// `Unsupported::reason` tag); `None` for a `KeepFlag`/`Eliminate` decision, or a `Strip`
    /// decision reached only because the active [`PrecisionConfig`] is `Strip` itself.
    pub reason: Option<&'static str>,
}

/// An auto-generated Karttunen-style table (design §3) for one grammar under one config —
/// currently just the per-constraint decisions (no measured sizes; see [`ConstraintDecision`]'s
/// doc).
#[derive(Debug, Clone, Default)]
pub struct PrecisionReport {
    pub decisions: Vec<ConstraintDecision>,
}

impl ConstraintCatalog {
    /// Decide every catalogued instance's [`PrecisionAction`] under `config` (design §3's
    /// `PrecisionTuner`, minus the actual trial-elimination auction — step 1 has exactly one
    /// non-default preset, `AllFlags`, and its decision rule is static: `KeepFlag` iff coverable,
    /// `Strip` otherwise; every other config is `Strip` for everything).
    pub fn decide(&self, config: PrecisionConfig) -> PrecisionReport {
        let decisions = self
            .env
            .iter()
            .map(|c| {
                let (action, reason) = match (&config, &c.coverage) {
                    (PrecisionConfig::AllFlags, EnvCoverage::LeftLiteral { .. }) => {
                        (PrecisionAction::KeepFlag, None)
                    }
                    (_, EnvCoverage::Unsupported { reason }) => {
                        (PrecisionAction::Strip, Some(*reason))
                    }
                    (_, EnvCoverage::LeftLiteral { .. }) => (PrecisionAction::Strip, None),
                };
                ConstraintDecision {
                    id: c.id,
                    attr: c.attr.clone(),
                    family: c.family,
                    action,
                    reason,
                }
            })
            .collect();
        PrecisionReport { decisions }
    }
}

// --- Emission runtime (the `AllFlags` preset's flag scheme; module doc "Emission mechanism") ----

/// One [`EnvConstraint::is_coverable`] instance's runtime flag symbols + literal test data
/// (module doc, "Emission mechanism").
struct EnvSetRule {
    sym_y: String,
    sym_n: String,
    literal_variants: Vec<String>,
}

/// `crate::emit`'s runtime companion: derived once from a [`ConstraintCatalog`] +
/// [`PrecisionConfig`], then threaded through every entry-writing call alongside `EmitCounts`
/// (same threading convention). Holds no [`Grammar`] reference of its own — only the small derived
/// lookup tables [`Self::tagged_lower`] needs. Unlike step 1's first implementation, this holds NO
/// scratch buffer and synthesizes NO `LEXICON` blocks — every flag is inline text on the entry's
/// own LOWER tape (module doc: "linear by construction").
pub(crate) struct PrecisionEmit {
    /// Keyed by the OWNING allomorph id → the `@R.ENV{id}.y@` require symbol to PREPEND to that
    /// allomorph's own entries' LOWER text (require only — exclude is declined, module doc finding
    /// 4). Empty unless `config == AllFlags` (so `tagged_lower` is a pure passthrough under `Strip`).
    owner_require: BTreeMap<AllomorphId, String>,
    /// Every covered constraint's set-y/set-n symbols + literal variants, in catalog (id-ascending)
    /// order — [`Self::tagged_lower`] appends ONE of `sym_y`/`sym_n` per rule, per non-empty-surface
    /// entry, in this fixed order (module doc: every entry gets AT MOST one symbol per constraint).
    /// Empty unless `config == AllFlags`.
    set_rules: Vec<EnvSetRule>,
    /// Every flag symbol this instance can emit, for `crate::emit`'s `Multichar_Symbols` section
    /// (lexc requires every multi-character token used in an entry to be pre-declared there, the
    /// same convention the emitter's own tag symbols already follow). Empty unless `AllFlags`.
    pub(crate) flag_symbols: Vec<String>,
}

impl PrecisionEmit {
    /// Build the runtime lookup tables for `config` against `catalog`. A `Strip` (or any config
    /// other than `AllFlags`) build leaves every table empty — [`Self::tagged_lower`] is then a
    /// pure passthrough for every call, which is what makes `crate::emit::emit`'s
    /// byte-identical-to-before guarantee hold by construction rather than by a second,
    /// hand-kept-in-sync code path.
    pub(crate) fn build(catalog: &ConstraintCatalog, config: PrecisionConfig) -> Self {
        let mut owner_require = BTreeMap::new();
        let mut set_rules = Vec::new();
        let mut flag_symbols = Vec::new();
        if matches!(config, PrecisionConfig::AllFlags) {
            for c in catalog.coverable() {
                let EnvCoverage::LeftLiteral { literal_variants } = &c.coverage else {
                    continue;
                };
                // Only `require == true` is ever coverable ([`classify`], module doc finding 4 — a
                // persistent-flag `@D@` disallow is declined, this step's scope), so a covered
                // instance's gate is always `@R@`; the assertion documents that invariant at the
                // emission seam.
                debug_assert!(
                    c.require,
                    "only require==true environments are coverable this step (exclude-left is \
                     declined by classify); saw a coverable exclude for {}",
                    c.attr
                );
                let fid = flag_id(c.id);
                let req = format!("@R.ENV{fid}.y@");
                let sym_y = format!("@P.ENV{fid}.y@");
                let sym_n = format!("@P.ENV{fid}.n@");
                owner_require.insert(c.allomorph, req.clone());
                flag_symbols.push(req);
                flag_symbols.push(sym_y.clone());
                flag_symbols.push(sym_n.clone());
                set_rules.push(EnvSetRule {
                    sym_y,
                    sym_n,
                    literal_variants: literal_variants.clone(),
                });
            }
        }
        PrecisionEmit {
            owner_require,
            set_rules,
            flag_symbols,
        }
    }

    /// Builds one entry's LOWER-tape text (`crate::emit::write_tag_entry`'s call): the owning
    /// allomorph's `@R@` require prefix (if any — only when `owner` is `Some` and that allomorph
    /// carries a covered constraint), then `escaped` (or lexc's `0` epsilon marker when `surface`
    /// is empty), then — only when `surface` is non-empty — every coverable constraint's set-y/
    /// set-n symbol in turn (module doc: adjacency via unconditional `@P@` overwrite; an empty
    /// surface emits NEITHER, preserving whatever the previous non-empty entry already set).
    /// Placement of the flag text relative to `escaped` doesn't affect correctness (module doc,
    /// "Why this is safe to splice directly...") — prefix/suffix is chosen only for readability.
    /// Under `Strip` (`set_rules`/`owner_require` both empty) this returns exactly `escaped`, or
    /// `"0"` when `escaped` is empty — byte-identical to the pre-precision-knob emitter.
    pub(crate) fn tagged_lower(
        &self,
        surface: &str,
        escaped: &str,
        owner: Option<AllomorphId>,
    ) -> String {
        let prefix = owner.and_then(|id| self.owner_require.get(&id));
        if surface.is_empty() {
            match prefix {
                Some(p) => format!("{p}0"),
                None => "0".to_string(),
            }
        } else {
            let mut out = String::with_capacity(
                escaped.len() + prefix.map_or(0, String::len) + self.set_rules.len() * 24,
            );
            if let Some(p) = prefix {
                out.push_str(p);
            }
            out.push_str(escaped);
            for rule in &self.set_rules {
                let sym = if could_satisfy(surface, &rule.literal_variants) {
                    &rule.sym_y
                } else {
                    &rule.sym_n
                };
                out.push_str(sym);
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_sample(name: &str) -> Option<Grammar> {
        let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../samples/data")
            .join(name);
        let xml = std::fs::read_to_string(&full).ok()?;
        Some(pg_grammar::load(&xml).unwrap())
    }

    fn load_conformance(path: &str) -> Grammar {
        let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../machine/conformance")
            .join(path);
        let xml =
            std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()));
        pg_grammar::load(&xml).unwrap()
    }

    /// Sena has 144 `<RequiredEnvironments>` elements in its XML (grep-verified): mostly
    /// right-context (`/_ [V]`, deferred) or word-edge anchors, but a real handful are single,
    /// literal-left, no-right instances the catalog must classify `LeftLiteral` — both on ROOT
    /// allomorphs (`/ma_`/`/na_`, the dominant coverable shape here) and on one rule allomorph
    /// (`/mb_`, `msubrule60`). Everything else stays `Unsupported`.
    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
    fn sena_catalog_finds_the_expected_left_literal_instances() {
        let Some(g) = load_sample("sena-hc.xml") else {
            eprintln!("skipping: sena-hc.xml not present on disk");
            return;
        };
        let catalog = ConstraintCatalog::build(&g);
        assert!(
            !catalog.env.is_empty(),
            "Sena declares real environments; catalog must see them"
        );
        let coverable: Vec<&EnvConstraint> = catalog.coverable().collect();
        assert!(
            coverable.len() >= 2,
            "expected at least the root-side /ma_//na_ instances plus the rule-side /mb_ one, \
             got {} coverable: {coverable:?}",
            coverable.len()
        );
        assert!(
            coverable.iter().all(|c| c.require),
            "every coverable Sena instance is a RequiredEnvironments (none Excluded), got {coverable:?}"
        );
        assert!(
            coverable
                .iter()
                .any(|c| c.owner_kind == EnvOwnerKind::Root
                    && matches!(&c.coverage, EnvCoverage::LeftLiteral { literal_variants }
                        if literal_variants.iter().any(|v| v == "ma") || literal_variants.iter().any(|v| v == "na"))),
            "expected a root-side /ma_ or /na_ instance among {coverable:?}"
        );
        assert!(
            coverable.iter().any(|c| {
                c.owner_kind == EnvOwnerKind::Rule
                    && matches!(&c.coverage, EnvCoverage::LeftLiteral { literal_variants }
                        if literal_variants.iter().any(|v| v == "mb"))
            }),
            "expected the rule-side /mb_ instance (msubrule60) among {coverable:?}"
        );

        // Ids are stable/deterministic across repeated builds of the SAME grammar.
        let catalog2 = ConstraintCatalog::build(&g);
        let ids: Vec<u32> = catalog.env.iter().map(|c| c.id).collect();
        let ids2: Vec<u32> = catalog2.env.iter().map(|c| c.id).collect();
        assert_eq!(
            ids, ids2,
            "catalog ids must be deterministic across rebuilds"
        );
        // Attribute names are zero-padded ENV.nnnn per the design's own worked example.
        assert!(catalog.env[0].attr.starts_with("ENV."));
        assert_eq!(catalog.env[0].attr.len(), "ENV.".len() + 4);
    }

    /// Indonesian has zero `<RequiredEnvironments>`/`<ExcludedEnvironments>` elements at all (grep-
    /// verified) — the catalog must be empty, and `AllFlags`'s decision table is then trivially all
    /// `Strip` (nothing to cover) rather than erroring.
    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
    fn indonesian_catalog_is_empty() {
        let Some(g) = load_sample("indonesian-hc.xml") else {
            eprintln!("skipping: indonesian-hc.xml not present on disk");
            return;
        };
        let catalog = ConstraintCatalog::build(&g);
        assert!(
            catalog.env.is_empty(),
            "Indonesian declares no environments at all"
        );
        let report = catalog.decide(PrecisionConfig::AllFlags);
        assert!(report.decisions.is_empty());
    }

    /// A multi-environment allomorph (module doc finding 1, "OR, not AND") must be `Unsupported {
    /// reason: "or-ambiguous" }` for EVERY one of its environments, even ones that would otherwise
    /// look like a trivially coverable `LeftLiteral` shape in isolation.
    #[test]
    fn multi_environment_allomorph_is_or_ambiguous_even_if_individually_simple() {
        // `edge-cases/disjunctive-recheck` is the one fixture the model doc (`pg-rules/src/
        // validity.rs`) explicitly names for the W3.2 free-fluctuation/disjunctive-allomorph
        // re-check; load it defensively and skip if it doesn't declare environments (its own
        // fixture is scoped to a different gate, so this is a best-effort structural probe rather
        // than a load-bearing assertion about that specific file).
        let g = load_conformance("edge-cases/disjunctive-recheck/grammar.xml");
        let catalog = ConstraintCatalog::build(&g);
        for c in &catalog.env {
            if c.sibling_count > 1 {
                assert!(
                    matches!(
                        c.coverage,
                        EnvCoverage::Unsupported {
                            reason: "or-ambiguous"
                        }
                    ),
                    "constraint {c:?} has sibling_count > 1 but wasn't marked or-ambiguous"
                );
            }
        }
    }

    fn one_constraint_catalog(id: u32, owner: AllomorphId, literal: &str) -> ConstraintCatalog {
        ConstraintCatalog {
            env: vec![EnvConstraint {
                id,
                attr: format!("ENV.{id:04}"),
                family: ConstraintFamily::Environment,
                owner_kind: EnvOwnerKind::Rule,
                allomorph: owner,
                env_index: 0,
                require: true,
                sibling_count: 1,
                coverage: EnvCoverage::LeftLiteral {
                    literal_variants: vec![literal.to_string()],
                },
            }],
        }
    }

    /// [`PrecisionEmit::tagged_lower`] is a byte-identical passthrough under `Strip` regardless of
    /// `owner` (the property `crate::emit`'s default path depends on): non-empty surface returns
    /// `escaped` unchanged, empty surface returns lexc's `"0"` epsilon marker.
    #[test]
    fn precision_emit_tagged_lower_is_passthrough_under_strip() {
        let catalog = one_constraint_catalog(0, AllomorphId(7), "mb");
        let pk = PrecisionEmit::build(&catalog, PrecisionConfig::Strip);
        assert!(pk.flag_symbols.is_empty());
        assert_eq!(
            pk.tagged_lower("tumba", "tumba", Some(AllomorphId(7))),
            "tumba"
        );
        assert_eq!(pk.tagged_lower("", "", Some(AllomorphId(7))), "0");
        assert_eq!(pk.tagged_lower("", "", None), "0");
    }

    /// Under `AllFlags`, the flag symbols are dot-free in the name field (`flag_id`, not `attr`)
    /// and follow the `@[R|P].ENV{id}.[y|n]@` shape.
    #[test]
    fn precision_emit_flag_symbols_are_dot_free_in_the_name_field() {
        let catalog = one_constraint_catalog(7, AllomorphId(3), "mb");
        let pk = PrecisionEmit::build(&catalog, PrecisionConfig::AllFlags);
        assert_eq!(
            pk.flag_symbols,
            vec![
                "@R.ENV7.y@".to_string(),
                "@P.ENV7.y@".to_string(),
                "@P.ENV7.n@".to_string()
            ]
        );
    }

    /// Under `AllFlags`, the owner allomorph's LOWER text gets the `@R@` require prefix, and EVERY
    /// non-empty-surface entry (owner or not) gets exactly one of `@P@` y/n appended, matching
    /// [`could_satisfy`]. No flag text is emitted for an empty surface.
    #[test]
    fn precision_emit_tagged_lower_gates_owner_and_sets_on_every_entry() {
        let catalog = one_constraint_catalog(7, AllomorphId(3), "mb");
        let pk = PrecisionEmit::build(&catalog, PrecisionConfig::AllFlags);

        // The owner's own entry (surface "i", allomorph id 3): @R@ prefix + surface + its OWN
        // set-flag (based on ITS OWN surface "i", which does not end in "mb" -> "n").
        let owner_lower = pk.tagged_lower("i", "i", Some(AllomorphId(3)));
        assert_eq!(owner_lower, "@R.ENV7.y@i@P.ENV7.n@");

        // An unrelated entry (no owner) whose surface ends in "mb" -> "y".
        let setter_lower = pk.tagged_lower("tumb", "tumb", None);
        assert_eq!(setter_lower, "tumb@P.ENV7.y@");

        // An unrelated entry that does NOT end in "mb" (and isn't a suffix of it) -> "n".
        let plain_lower = pk.tagged_lower("kucita", "kucita", None);
        assert_eq!(plain_lower, "kucita@P.ENV7.n@");

        // An EMPTY surface gets no set flag at all, even for the owner (only the @R@ prefix).
        let empty_owner_lower = pk.tagged_lower("", "", Some(AllomorphId(3)));
        assert_eq!(empty_owner_lower, "@R.ENV7.y@0");
        let empty_plain_lower = pk.tagged_lower("", "", None);
        assert_eq!(empty_plain_lower, "0");
    }

    /// [`could_satisfy`]'s two disjuncts: whole-literal `ends_with`, and the boundary-spanning
    /// "proper suffix of the literal" case (module doc: the "miseru" cross-boundary recall break).
    #[test]
    fn could_satisfy_covers_whole_literal_and_boundary_spanning_suffix() {
        // Whole literal spelled within one entry.
        assert!(could_satisfy("tumb", &["mb".to_string()]));
        // Boundary-spanning: "i" is a PROPER suffix of "mi" (shorter, and "mi" ends with "i").
        assert!(could_satisfy("i", &["mi".to_string()]));
        // Not a match either way.
        assert!(!could_satisfy("ku", &["mi".to_string()]));
        // Representation variants: matches if ANY literal variant matches (here "an" is a proper
        // suffix of both "man" and "nan").
        assert!(could_satisfy("an", &["man".to_string(), "nan".to_string()]));
        // An entry EQUAL to the literal itself still satisfies (ends_with is reflexive).
        assert!(could_satisfy("mb", &["mb".to_string()]));
        // Empty literal variants never match.
        assert!(!could_satisfy("mb", &[String::new()]));
    }

    /// [`flag_id`] never contains the digit `0` (verified empirically: a `0` breaks matching for
    /// the WHOLE symbol once spliced onto a surface, `%`-escaped or not — module doc, [`flag_id`]'s
    /// own doc) nor a `.` (the `flag_check` DFA finding — module doc, "Two failed encodings"), and
    /// the `0`->`Z` substitution stays injective (distinct ids never collide).
    #[test]
    fn flag_id_has_no_zero_digit_and_never_contains_a_dot() {
        assert_eq!(flag_id(7), "7");
        assert_eq!(flag_id(70), "7Z");
        assert_eq!(flag_id(700), "7ZZ");
        assert_ne!(flag_id(7), flag_id(70));
        for id in [0, 7, 10, 70, 700, 1007] {
            let fid = flag_id(id);
            assert!(!fid.contains('.'), "flag_id({id}) must never contain a dot");
            assert!(
                !fid.contains('0'),
                "flag_id({id}) must never contain the digit 0, got {fid:?}"
            );
        }
    }

    /// New finding 5: a grammar with zero phonological rules (the common case, e.g. Sena) is never
    /// at risk — the loop is a no-op.
    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
    fn prule_tail_rewrite_risk_is_false_with_no_phonological_rules() {
        let Some(g) = load_sample("sena-hc.xml") else {
            eprintln!("skipping: sena-hc.xml not present on disk");
            return;
        };
        assert!(
            g.prules.is_empty(),
            "Sena is the zero-phonological-rules reference grammar"
        );
        assert!(!prule_tail_rewrite_risk(&g, &["ma".to_string()]));
    }
}
