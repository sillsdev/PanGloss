//! Step 3a of `openspec/changes/reify-compilation-plans` (design.md D3): [`build_controllable`], a
//! [`crate::plan::Plan`] INTERPRETER -- the first piece of Step 3 that turns a reified `Plan` into a
//! real, live [`foma::types::Fsm`] rather than only describing one (Step 1, `crate::plan`; Step 2,
//! `crate::enumerate::enumerate_default`, which is purely data -- "no live `Fsm` is built anywhere
//! there", that module's own doc). This module walks exactly the node kinds
//! [`crate::enumerate::enumerate_default`] emits on the **controllable subtree** -- the [`crate::
//! plan::PlanNodeKind::Gate`] node and its per-group `Compose{LexiconFragment, Replace}` children --
//! and calls the SAME low-level primitives [`crate::gate::compile_gated_grammar_with_budget`] uses
//! ([`crate::uflexc::emit_underlying_filtered_with_budget`], [`crate::replace::
//! compile_and_compose_rules_gated_with_budget`], [`crate::compose_budget`]'s checked compose/union/
//! minimize wrappers). Neither `gate.rs`'s nor `replace.rs`'s bodies are touched -- this module only
//! calls their existing `pub` entry points (the task's own constraint).
//!
//! Proven equivalent to [`crate::gate::compile_gated_grammar_with_budget`]'s own direct-compile
//! output by an APPLY-based test (`equivalence_tests`, below) -- run real query words through BOTH
//! nets' `apply_up` and assert identical results, exactly the predicate a future differential oracle
//! (design.md D4) would use. This is a genuine correctness argument, not a structural-equality
//! shortcut: two networks can differ in shape (state numbering, arc order) and still be the *same
//! relation* modulo determinization/minimization choices, so `apply` is what actually matters here;
//! the module's own test additionally checks minimized state/arc counts as a cheap, meaningful
//! (not merely coincidental, given both paths run the same final `minimize_checked` on networks
//! built from the same primitives) extra signal -- but never in place of the apply comparison.
//!
//! # Scope: controllable subtree only (task's own scope call)
//! The composite-emission / structural-composite branches ([`crate::plan::FragmentSpec::
//! CompositeEmissionMarker`] / [`crate::plan::FragmentSpec::StructuralCompositeMarker`], the
//! black-box lexc `String` [`crate::emit::emit_with_budget`] produces) are OUT OF SCOPE for this
//! step: that path's artifact type is a lexc source string handed to a *separate* lexc-compile step,
//! not this module's own composed `Fsm` -- unifying the two artifact types into one interpreter
//! result is a later step's problem, not this one's. If `enumerate_default`'s plan root is a `Union`
//! carrying those markers alongside a `Gate` node (D2's own shape, `enumerate`'s module doc), this
//! module's [`build_controllable`] locates the single `Gate` child and interprets ONLY that subtree;
//! the marker leaves are checked for by kind (so a genuinely unrecognized Union child is a loud,
//! documented programmer-error panic, never a silent skip of something unexpected) but never built.
//!
//! # Task 1.4: the obstacle this step surfaced, now RESOLVED
//! Step 3a (an earlier version of this module) flagged a real interpretation obstacle here, and
//! design.md D1's "Soundness invariant" paragraph (added after Step 3a) named it precisely: **every
//! gate group's `Replace` subplan was the identical, content-addressed-SHARED [`crate::plan::
//! NodeId`]**, yet the COMPILED `Fsm` that node had to produce differed PER GROUP, because
//! [`crate::replace::compile_and_compose_rules_gated_with_budget`]'s `subrule_ok` callback is a
//! function of the *group*, not of the `Replace` node's own content. A naive content-addressed
//! interpreter that memoizes a built `Fsm` per `NodeId` would therefore have built the shared
//! `Replace` node's cascade ONCE and silently reused that WRONG network for every other group -- an
//! unsound, silent correctness bug, not a missing feature. At Step 3a, [`build_controllable`]
//! sidestepped this by being Gate-aware (re-deriving each group's `subrule_ok` from the `Gate`
//! node's own `partition`, never caching a compiled `Fsm` against the shared `Replace` `NodeId`),
//! which was correct but kept `Gate` from being "just another n-ary node."
//!
//! **Task 1.4's fix** (`crate::plan::ReplaceCascadeSpec`'s own doc, `crate::enumerate::
//! enumerate_default`'s own module doc): `enumerate_default` now builds ONE `Replace` node PER
//! GROUP, and that node's own `cascade` carries `gated_subrules` + `group_key` directly -- so a
//! group's `subrule_ok` is now fully determined by its OWN `Replace` node's content, not by which
//! `Gate` group happens to reference it. [`build_controllable`] below reflects this: it derives
//! `subrule_ok` by reading the per-group `Replace` node's own `cascade.gated_subrules`/
//! `cascade.group_key` (see [`subrule_ok_for_group`]), NOT by re-deriving it from the `Gate` node's
//! partition. The `Gate`-node walk itself is unchanged (this module still locates each group's own
//! `Compose`/`Replace` subtree by walking the `Gate` node's `children`, and still cross-checks
//! `partition.groups[group_idx].key` against the Replace node's own `group_key` as a redundant
//! sanity check -- see the loop in [`build_controllable`]), but **correctness no longer depends on
//! Gate-awareness of the Replace node**: `Replace`'s compiled artifact is now a pure function of its
//! own `NodeId`, exactly what D1's soundness invariant requires for content-addressed dedup / a
//! future `NodeId`-keyed plan-cache / the differential oracle (`crate::oracle`) to memoize safely.
//! This step does not build a generic memoizing interpreter -- that remains future work -- it only
//! removes the soundness caveat that would have made one unsound.
//!
//! # Node kinds handled (exactly what `enumerate_default` emits on the controllable path)
//! - [`crate::plan::PlanNodeKind::Gate`] -- the entry point; see the obstacle note above.
//! - [`crate::plan::PlanNodeKind::Compose`] -- each gate group's child; only
//!   [`crate::plan::ComposeStrategy::Static`] is interpreted (the only strategy `enumerate_default`
//!   ever emits) -- `Lazy`/`LazyLookahead` panic with a precise message rather than silently
//!   compiling eagerly, since no lazy-composition primitive exists anywhere in this crate yet (a
//!   real, separate Plan-model/interpreter gap, not this step's to close).
//! - [`crate::plan::PlanNodeKind::Leaf`] tagged [`crate::plan::FragmentSpec::LexiconFragment`] --
//!   read as `entries` for [`crate::uflexc::emit_underlying_filtered_with_budget`]'s own
//!   `allowed_entries` parameter (always `Some`, matching `enumerate_default`'s own invariant).
//! - [`crate::plan::PlanNodeKind::Replace`] and its [`crate::plan::FragmentSpec::RewriteRule`] Leaf
//!   children -- read and cross-validated against the `prules_in_order` slice the caller supplies
//!   (see [`validate_replace_cascade`]'s own doc for why this check exists and what it catches).
//!
//! # Visibility widened
//! [`crate::enumerate::rule_id_of`] was widened from private to `pub(crate)` so this module can reuse
//! its pointer-identity `PRuleId` recovery rather than re-deriving the same safety-relevant logic a
//! second time (see that function's own doc for why the pointer-identity approach is sound). No other
//! visibility change was needed -- every other primitive this module calls
//! ([`crate::uflexc::emit_underlying_filtered_with_budget`], [`crate::replace::
//! compile_and_compose_rules_gated_with_budget`], [`crate::compose_budget`]'s checked wrappers,
//! [`crate::gate::GatedCompileResult`]) was already `pub`/`pub(crate)`.

use std::collections::HashSet;

use foma::options::FomaOptions;
use foma::types::Fsm;

use pg_grammar::model::{Grammar, LexEntryId, PhonRuleDef};

use crate::compose_budget::{
    compose_checked, minimize_checked, union_checked, ComposeBudget, ComposeError,
};
use crate::enumerate::rule_id_of;
use crate::gate::GatedCompileResult;
use crate::plan::{
    ComposeStrategy, FragmentSpec, GatedSubruleRef, NodeId, Plan, PlanNodeKind, ReplaceCascadeSpec,
};
use crate::replace::{compile_and_compose_rules_gated_with_budget, SegAlphabet, TupleReport};
use crate::uflexc::{emit_underlying_filtered_with_budget, UEmitReport};

/// The two marker fragments [`crate::enumerate::enumerate_default`] places alongside the `Gate` node
/// when a grammar's recall depends on the composite-emission / structural-composite subtrees --
/// exactly the leaves [`find_gate_node`] skips (module doc, "Scope: controllable subtree only").
///
/// A caller that treats [`build_controllable`]'s net as if it represented the WHOLE grammar must
/// consult this first. On a grammar whose plan carries either marker, the controllable-only net omits
/// the material those subtrees contribute, and the omission is quiet: the net is smaller but
/// perfectly well-formed, `build_controllable` returns `Ok`, and no budget trips. Measured on a
/// templated real grammar, the controllable-only net was 135 states / 3309 arcs against the tuned
/// `crate::emit`-based path's 6376 states / 68693 arcs for the same grammar -- a 47x state deficit
/// that proposed nothing for 19 of 20 corpus words while the tuned net proposed correctly.
///
/// Returns the markers present, in plan iteration order, empty when the plan is fully within
/// [`build_controllable`]'s scope.
pub fn unbuildable_markers(plan: &Plan) -> Vec<FragmentSpec> {
    let mut found = Vec::new();
    for (_, kind) in plan.iter() {
        if let PlanNodeKind::Leaf { fragment, .. } = kind {
            if matches!(
                fragment,
                FragmentSpec::CompositeEmissionMarker | FragmentSpec::StructuralCompositeMarker
            ) && !found.contains(fragment)
            {
                found.push(fragment.clone());
            }
        }
    }
    found
}

/// Every token character standing for a `Boundary`-kind char-def in `table` -- the shared
/// collection both [`boundary_cleanup_net`] (which deletes every one of them, unconditionally) and
/// [`reroute_null_shaped_affix_chains`] (which needs to recognize when a lexc line's ENTIRE
/// underlying text is drawn only from this set, i.e. is about to be deleted down to nothing) must
/// agree on. Kept as one function so the two can never drift on which char-defs "boundary" means
/// here.
fn boundary_tokens(table: &pg_grammar::chardef::CharDefTable, alphabet: &SegAlphabet) -> Vec<char> {
    table
        .iter()
        .filter(|(_, cd)| cd.kind() == pg_grammar::chardef::CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect()
}

/// The boundary-token cleanup net that every caller further composing a [`build_controllable`] /
/// [`crate::gate::compile_gated_grammar_with_budget`] result must apply. `None` when `table` declares
/// no `Boundary` char-def at all (the common case for a grammar that authors no morph-juncture
/// markers).
///
/// # Why this deletes EVERY `Boundary` char-def, unconditionally, with no exceptions
/// `uflexc`'s emitted lexc leaves these tokens as required literal characters on the tape (the
/// commit message for `76cf841` confirms the pre-cleanup net was "unqueryable" -- a bare surface
/// query, which never contains a literal boundary character, matched nothing). Excluding ANY
/// `Boundary` char-def from this deletion (an earlier version of this function tried excluding
/// multi-representation ones, keyed off `CharDef::representations().len()`) makes every entry that
/// contains that char-def permanently unreachable by a real surface query -- not a narrow gap, a
/// straight recall regression (`recipe_runtime_net_is_queryable_gate.rs`'s own
/// `null_morph_prefix_does_not_collapse_to_a_free_epsilon_loop`-shaped test caught this immediately:
/// `MultiplicityMismatch { word: "s", expected: 2, actual: 1 }` -- the null-affixed analysis simply
/// vanished). So this function stays exactly what it always was: blanket, unconditional deletion of
/// every `Boundary` char-def. See [`reroute_null_shaped_affix_chains`] for where the actual fix for
/// the precision regression this used to cause now lives.
fn boundary_cleanup_net(
    opts: &FomaOptions,
    table: &pg_grammar::chardef::CharDefTable,
    alphabet: &SegAlphabet,
) -> Option<Fsm> {
    let tokens = boundary_tokens(table, alphabet);
    if tokens.is_empty() {
        return None;
    }
    let cleanup_regex = tokens
        .iter()
        .map(|c| format!("{c} -> 0"))
        .collect::<Vec<_>>()
        .join(", ");
    foma::regex::fsm_parse_regex(opts, &cleanup_regex, None, None)
}

/// The actual fix for `docs/fst-plan/large-lexicon-proposal-explosion.md`'s precision regression,
/// applied to a group's raw `uflexc` lexc source BEFORE it is compiled to an `Fsm` (i.e. before
/// [`boundary_cleanup_net`] ever runs) -- this is the "stop putting boundary tokens on the queryable
/// tape at all" mechanism the diagnosis doc's own recommendation #2 named, mirrored from
/// [`crate::emit`]'s working approach (its own module doc: "boundary characters dropped,
/// representation variants enumerated" -- never emitted onto the tape, then blanket-deleted after
/// the fact), adapted to `uflexc`'s much simpler self-looping-lexicon model instead of `emit.rs`'s
/// junction-probing one.
///
/// # The exact failure mode this closes
/// `uflexc::emit_underlying_filtered_with_budget`'s prefix/suffix continuation lexicons
/// (`PrefixChain`'s lines all point back to the self-referencing `PrefixOrRoot`/`PrefixChain` pair;
/// `SuffixChain`'s all point back to the self-referencing `SuffixOrEnd`/`SuffixChain` pair) are
/// DELIBERATELY self-looping (`uflexc`'s own module doc: "self-looping prefix/suffix chains"), an
/// upward approximation that lets real (non-empty) affixes stack arbitrarily. That is harmless for
/// an ordinary affix because taking the loop always consumes at least one real surface character, so
/// recursion depth is bounded by the query's own length. It is NOT harmless for an affix allomorph
/// whose entire underlying shape is composed only of `Boundary`-kind characters (Sena's compounding
/// allomorph `"^0+"`, 7 occurrences, all identical): once `boundary_cleanup_net` deletes every
/// character of THAT allomorph's spelling, its lexc line degenerates to a zero-width, epsilon-tagged
/// entry sitting ON the self-loop -- a free, unboundedly-repeatable insertion of that morpheme's tag
/// symbol, taken any number of times without consuming any surface text. `apply_up` enumerates
/// distinct accepting upper-tape strings (each repeat count produces a genuinely different tag
/// sequence), so it multiplies out every repeat count up to its own internal search bound: measured
/// 127 -> 53992 proposals (425x) on the same Sena 5-word slice, 99.5% on one word (`mbali`).
///
/// # Why deleting the boundary characters isn't the problem -- the CYCLE is
/// The pre-cleanup network already requires these literal boundary characters to be present in the
/// input to take the loop at all, so pre-cleanup it is already correctly rejected by every real
/// (boundary-free) surface query -- this is exactly the OLD "unqueryable net" bug for entries that
/// need those characters gone. Deletion has to happen for recall. What must not happen is deletion
/// landing a zero-width transition on a state that can be revisited: this function reroutes exactly
/// those lines, and only those, off the self-looping continuation and onto a one-shot successor that
/// cannot be re-entered -- so the null/zero-morph marker keeps behaving like an ordinary optional
/// morph that occurs AT MOST ONCE per prefix/suffix juncture (its actual grammatical meaning), never
/// like a free repeatable insertion. This preserves recall (the marker-only entry is still reachable,
/// exactly once, so a word genuinely analyzed with it still proposes and confirms) while eliminating
/// the epsilon cycle that caused the explosion (nothing left to repeat unboundedly).
///
/// # Preserving full stacking around the (at most once) marker, not just "reachable at all"
/// A first version of this function routed a null-shaped line straight to `RootBare`/`#` (no further
/// prefixes/suffixes allowed afterward at all). That is TOO narrow: `uflexc`'s self-looping chain is
/// there so ordinary affixes can combine in any order, and the ground truth (`pg_parse::Morpher`,
/// which this net is only ever an approximation OF) genuinely admits every order of a real affix
/// relative to a null one -- caught directly by this fix's own gate,
/// `null_morph_prefix_does_not_collapse_to_a_free_epsilon_loop`, which failed
/// `MultiplicityMismatch { word: "ps", expected: 3, actual: 2 }` under that narrower version: real
/// prefix's underlying "p" plus the null prefix legitimately combine in EITHER order (both surface as
/// "ps"), and routing straight to `RootBare` silently dropped whichever order took the null prefix
/// FIRST. So the successor state after a null/marker line must still admit every ORDINARY (non-null-
/// shaped) affix, in any quantity -- just never a SECOND null-shaped line (which is what would reopen
/// the epsilon cycle). Hence the duplicated `*NoNull` chain below: ordinary affixes get a second,
/// otherwise-identical lexc line whose continuation stays inside the "already used the marker"
/// universe, so they can freely combine before AND after the (at most one) marker occurrence, while
/// the marker lines themselves are never duplicated into that universe -- there is no line left for a
/// second marker occurrence to take, so the cycle stays broken.
///
/// # Mechanics
/// Scans `lexc_source`'s `PrefixChain`/`SuffixChain` lexicon bodies (the exact, fixed shape
/// `emit_underlying_filtered_with_budget` itself always produces -- this is not a general lexc
/// parser). A line whose underlying (lower-tape) text is non-empty and consists ENTIRELY of
/// characters in `boundary_tokens(table, alphabet)` (i.e. will be deleted to nothing by
/// [`boundary_cleanup_net`]) is "null-shaped"; every other non-blank entry line in those two bodies is
/// "ordinary". For the prefix side:
/// - Each null-shaped `PrefixChain` line has its continuation swapped in place: `PrefixOrRoot` ->
///   `PrefixOrRootAfterNull`.
/// - Each ordinary `PrefixChain` line gets a SECOND copy (identical tag/underlying, continuation
///   `PrefixOrRootAfterNull` instead of `PrefixOrRoot`) collected into a new `PrefixChainNoNull`
///   lexicon body.
/// - Two lexicons are appended (only if any null-shaped prefix line existed): `PrefixOrRootAfterNull`
///   offers `PrefixChainNoNull ;` (any ordinary prefix, any number of times, any order) and
///   `RootBare ;` (stop prefixing) -- but never `PrefixChain` itself, so no second null-shaped prefix
///   is reachable from here.
///   The suffix side mirrors this exactly: `SuffixOrEnd` -> `SuffixEndOnly`, `SuffixChain` ->
///   `SuffixChainNoNull`, `# ;` in place of `RootBare ;`.
///
/// Every OTHER line (root lines, lexicon headers, blank lines) passes through byte-for-byte. A
/// grammar with no `Boundary` char-def at all is a pure no-op (`boundary_tokens` is empty, so nothing
/// can ever match), keeping every existing boundary-free fixture's net byte-identical to before this
/// function existed.
fn reroute_null_shaped_affix_chains(
    lexc_source: &str,
    table: &pg_grammar::chardef::CharDefTable,
    alphabet: &SegAlphabet,
) -> String {
    let boundary_set: HashSet<char> = boundary_tokens(table, alphabet).into_iter().collect();
    if boundary_set.is_empty() {
        return lexc_source.to_string();
    }

    let mut out = String::with_capacity(lexc_source.len() + 128);
    let mut current_lexicon: Option<&str> = None;
    let mut prefix_no_null_lines: Vec<String> = Vec::new();
    let mut suffix_no_null_lines: Vec<String> = Vec::new();

    for line in lexc_source.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("LEXICON ") {
            current_lexicon = Some(name.trim());
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let side = match current_lexicon {
            Some("PrefixChain") => Some(("PrefixOrRoot", "PrefixOrRootAfterNull")),
            Some("SuffixChain") => Some(("SuffixOrEnd", "SuffixEndOnly")),
            _ => None,
        };
        if let Some((from_continuation, to_continuation)) = side {
            match reroute_line_if_null_shaped(
                line,
                &boundary_set,
                from_continuation,
                to_continuation,
            ) {
                Some(rerouted) => {
                    // Null-shaped: replace IN PLACE (module doc) -- this line never gets a
                    // `*NoNull` duplicate, which is exactly what keeps a second marker occurrence
                    // unreachable.
                    out.push_str(&rerouted);
                    out.push('\n');
                    continue;
                }
                None => {
                    // Ordinary: passes through unchanged here, AND gets a second copy queued for
                    // the `*NoNull` lexicon (continuation swapped), so it can still combine with a
                    // marker that occurred earlier in the chain.
                    if let Some(dup) = duplicate_ordinary_line_with_continuation(
                        line,
                        from_continuation,
                        to_continuation,
                    ) {
                        match current_lexicon {
                            Some("PrefixChain") => prefix_no_null_lines.push(dup),
                            Some("SuffixChain") => suffix_no_null_lines.push(dup),
                            _ => unreachable!("side is only Some for PrefixChain/SuffixChain"),
                        }
                    }
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }

    if !prefix_no_null_lines.is_empty() {
        out.push_str("\nLEXICON PrefixOrRootAfterNull\nPrefixChainNoNull ;\nRootBare ;\n");
        out.push_str("\nLEXICON PrefixChainNoNull\n");
        for l in &prefix_no_null_lines {
            out.push_str(l);
            out.push('\n');
        }
    }
    if !suffix_no_null_lines.is_empty() {
        out.push_str("\nLEXICON SuffixEndOnly\nSuffixChainNoNull ;\n# ;\n");
        out.push_str("\nLEXICON SuffixChainNoNull\n");
        for l in &suffix_no_null_lines {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

/// If `line` is an ORDINARY (non-null-shaped) `uflexc` continuation-chain entry whose continuation is
/// exactly `from_continuation`, returns a duplicate with the continuation swapped to `to_continuation`
/// -- the `*NoNull` copy [`reroute_null_shaped_affix_chains`]'s own doc describes. `None` for a blank
/// line or any line whose continuation doesn't match (nothing to duplicate).
fn duplicate_ordinary_line_with_continuation(
    line: &str,
    from_continuation: &str,
    to_continuation: &str,
) -> Option<String> {
    let mut sep_byte = None;
    let mut prev = '\0';
    for (i, c) in line.char_indices() {
        if c == ':' && prev != '%' {
            sep_byte = Some(i);
            break;
        }
        prev = c;
    }
    let sep_byte = sep_byte?;
    let tag = &line[..sep_byte];
    let after = &line[sep_byte + 1..];
    let mut fields = after.split_whitespace();
    let underlying = fields.next()?;
    let cont = fields.next()?;
    if cont != from_continuation {
        return None;
    }
    Some(format!("{tag}:{underlying} {to_continuation} ;"))
}

/// If `line` is a `uflexc`-shaped continuation-chain entry (`TAG:UNDERLYING FROM_CONTINUATION ;`)
/// whose `UNDERLYING` text is composed ENTIRELY of characters in `boundary_tokens` (so
/// [`boundary_cleanup_net`]'s later blanket deletion will reduce it to the empty string), returns
/// the same line with its continuation swapped to `to_continuation` -- moving it off the
/// self-looping chain (see [`reroute_null_shaped_affix_chains`]'s own doc). `None` for every other
/// line (ordinary non-empty-underlying entries, or any line whose continuation isn't
/// `from_continuation` to begin with) -- left completely untouched by the caller.
fn reroute_line_if_null_shaped(
    line: &str,
    boundary_tokens: &HashSet<char>,
    from_continuation: &str,
    to_continuation: &str,
) -> Option<String> {
    // `tags::lexc_tag`'s own escaping convention: the tag's own embedded colon is always spelled
    // `%:` (escaped), so the first ':' NOT immediately preceded by '%' is the real upper/lower
    // separator that `emit_underlying_filtered_with_budget`'s own `format!("{tag}:{underlying} ...")`
    // writes -- never a colon inside the tag text itself.
    let mut sep_byte = None;
    let mut prev = '\0';
    for (i, c) in line.char_indices() {
        if c == ':' && prev != '%' {
            sep_byte = Some(i);
            break;
        }
        prev = c;
    }
    let sep_byte = sep_byte?;
    let tag = &line[..sep_byte];
    let after = &line[sep_byte + 1..];
    let mut fields = after.split_whitespace();
    let underlying = fields.next()?;
    let cont = fields.next()?;
    if cont != from_continuation {
        return None;
    }
    if underlying.is_empty() || !underlying.chars().all(|c| boundary_tokens.contains(&c)) {
        return None;
    }
    Some(format!("{tag}:{underlying} {to_continuation} ;"))
}

/// Finishes a [`build_controllable`] net into one a [`crate::analyzer::FomaProposer`] can actually
/// query: composes the boundary-token cleanup net, then re-minimizes.
///
/// **This step is mandatory, not an optimization.** [`crate::gate::compile_gated_grammar_with_budget`]'s
/// own doc says so directly -- "Callers that further compose this result (every example/test driver
/// does, with a boundary-cleanup net) still need their OWN final minimize afterward" -- because the
/// composed net still carries the boundary tokens `uflexc` emitted between morphs, which a surface
/// query never contains. Skipping it does not degrade recall gracefully; it silently zeroes it. It
/// was previously open-coded only inside test drivers (`tests/p6_gate_parity.rs`), so
/// `recipe_runtime::evaluate_plans` -- the one production caller -- omitted it and measured every
/// candidate against an unqueryable net.
pub fn finish_controllable_net(
    opts: &FomaOptions,
    net: Fsm,
    table: &pg_grammar::chardef::CharDefTable,
    alphabet: &SegAlphabet,
    budget: &ComposeBudget,
) -> Result<Fsm, ComposeError> {
    let net = match boundary_cleanup_net(opts, table, alphabet) {
        Some(cleanup) => compose_checked(opts, net, cleanup, budget, "finish_controllable_net")?,
        None => net,
    };
    minimize_checked(opts, net, budget, "finish_controllable_net")
}

/// Interprets `plan`'s controllable subtree (module doc) into a real, composed `Fsm` -- the plan-walk
/// counterpart of [`crate::gate::compile_gated_grammar_with_budget`]. This function does not call
/// into `gate.rs` at all (it never re-derives the partition itself); it calls the same public/
/// `pub(crate)` low-level primitives that function itself uses. `g`/`alphabet`/`prules_in_order`/
/// `budget` are the SAME inputs
/// [`crate::enumerate::enumerate_default`] (which built `plan`) and
/// [`crate::gate::compile_gated_grammar_with_budget`] both take -- `build_controllable` does not
/// recompute grammar-derived facts `enumerate_default` already baked into `plan` (it never calls
/// `crate::gate::find_gated_subrules`/`partition_entries` itself), it only reads them back out of the
/// plan's own nodes.
///
/// # Panics
/// On any plan shape [`crate::enumerate::enumerate_default`] does not itself produce (a dangling
/// `NodeId`, a `Gate` node missing from the root/root-`Union`, a group's `Compose` node with the wrong
/// child count or a non-`Static` strategy, a `Replace` cascade that doesn't match `prules_in_order`) --
/// these are caller/plan-construction contract violations, not runtime/budget failures, so they panic
/// loudly rather than returning a `ComposeError` variant that doesn't exist for them (mirrors this
/// crate's existing convention, e.g. `crate::gate::compile_gated_grammar_with_budget`'s own
/// `unwrap_or_else(|| panic!(...))` on a lexc-compile failure, and `crate::enumerate::rule_id_of`'s own
/// panic on a caller-supplied slice not borrowed from `g.prules`).
///
/// # Errors
/// Only for the same reasons [`crate::gate::compile_gated_grammar_with_budget`] itself returns
/// `Err` -- a [`ComposeBudget`] cap tripping on the emit/compose/union/minimize primitives this
/// function calls (no NEW budget vector is introduced here; the group-count budget check (V6) that
/// `compile_gated_grammar_with_budget` runs BEFORE any per-group work is not re-run here, since
/// `plan.partition.groups.len()` was already checked at `enumerate_default` build time by that same
/// mechanism if the caller built `plan` through the production path -- `build_controllable` trusts the
/// plan it is handed, per this function's own doc above, rather than re-deriving facts already baked
/// into it).
pub fn build_controllable(
    plan: &Plan,
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    budget: &ComposeBudget,
) -> Result<GatedCompileResult, ComposeError> {
    let gate_id = find_gate_node(plan);
    let PlanNodeKind::Gate {
        partition,
        children,
    } = plan.get(gate_id).unwrap_or_else(|| {
        panic!("find_gate_node returned a NodeId {gate_id} not interned in plan")
    })
    else {
        unreachable!("find_gate_node only ever returns the id of a Gate node")
    };
    assert_eq!(
        partition.groups.len(),
        children.len(),
        "Gate node invariant (see Plan::add_node's own debug_assert): one child per partition group"
    );
    // Read once, reused per group below (`reroute_null_shaped_affix_chains` call site) -- the SAME
    // table `alphabet` was itself constructed from, per `SegAlphabet::table`'s own doc.
    let table_for_group = alphabet.table();

    let mut final_net: Option<Fsm> = None;
    let mut skipped_rules: Vec<String> = Vec::new();
    let mut skipped_allomorphs: Vec<String> = Vec::new();
    let mut tuple_reports: Vec<(String, Vec<TupleReport>)> = Vec::new();
    let mut group_reports = Vec::new();

    for (group_idx, &compose_id) in children.iter().enumerate() {
        let group_key = &partition.groups[group_idx].key;
        let (lexicon_id, replace_id) = gate_group_children(plan, compose_id);

        let entries = lexicon_fragment_entries(plan, lexicon_id);
        let entries_set: HashSet<LexEntryId> = entries.iter().copied().collect();

        // Walks THIS group's OWN Replace node's data (cascade + rule-leaf children) and
        // cross-checks it against `prules_in_order` -- see this function's own doc, module doc's
        // "task 1.4" note. Returns the cascade itself so this group's `subrule_ok` can be derived
        // straight from it below, rather than from the Gate node's partition.
        let cascade = validate_replace_cascade(plan, replace_id, g, prules_in_order);
        assert_eq!(
            &cascade.group_key, group_key,
            "this group's own Replace node's group_key must match the Gate node's own partition \
             key for the same group -- a redundant sanity check (task 1.4: subrule_ok is now \
             derived from the Replace node's own cascade, not from this Gate-node value), catching \
             an enumerator bug that desynced the two rather than a normal-path failure"
        );

        let UEmitReport {
            lexc_source,
            skipped: uskipped,
            root_entries,
            prefix_entries,
            suffix_entries,
            ..
        } = emit_underlying_filtered_with_budget(g, alphabet, Some(&entries_set), budget)?;
        skipped_allomorphs.extend(uskipped);
        group_reports.push((
            group_key.clone(),
            root_entries,
            prefix_entries,
            suffix_entries,
        ));

        if root_entries == 0 {
            // Mirrors `compile_gated_grammar_with_budget`'s own doc: an empty group (a gating key
            // combination realized by zero entries) contributes nothing.
            continue;
        }

        // Precision fix (`reroute_null_shaped_affix_chains`'s own doc): must run BEFORE compiling,
        // on the raw lexc source, so the marker-only allomorph lines never reach the compiled `Fsm`
        // sitting on `uflexc`'s self-looping continuation in the first place.
        let lexc_source = reroute_null_shaped_affix_chains(&lexc_source, table_for_group, alphabet);
        let lexc_net = foma::lexcread::fsm_lexc_parse_string(opts, None, &lexc_source)
            .unwrap_or_else(|| panic!("gated group lexc failed to compile:\n{lexc_source}"));

        // Task 1.4 (module doc): this group's own gating key, read from its OWN Replace node's
        // cascade (never re-derived from the Gate node's partition), threaded into a per-group
        // subrule_ok closure. This is now a pure read of that Replace NodeId's own content -- no
        // cross-group state, no cache to get wrong.
        let subrule_ok = subrule_ok_for_group(&cascade.gated_subrules, &cascade.group_key);

        let mut group_skipped_rules = Vec::new();
        let rules_net = compile_and_compose_rules_gated_with_budget(
            opts,
            g,
            alphabet,
            prules_in_order,
            &subrule_ok,
            &mut group_skipped_rules,
            &mut tuple_reports,
            budget,
        )?;
        for s in group_skipped_rules {
            if !skipped_rules.contains(&s) {
                skipped_rules.push(s);
            }
        }

        let group_net = match rules_net {
            Some(rules) => compose_checked(
                opts,
                lexc_net,
                rules,
                budget,
                "build_controllable lexc.o.rules",
            )?,
            None => lexc_net,
        };
        final_net = Some(match final_net {
            None => group_net,
            // Safe union: groups are lexically disjoint -- same argument as `crate::gate`'s own
            // module doc ("why the union is safe here"), unchanged by walking a plan instead of
            // recomputing the partition directly.
            Some(prev) => union_checked(
                opts,
                prev,
                group_net,
                budget,
                "build_controllable group union fold",
            )?,
        });
    }

    let final_net = match final_net {
        Some(net) => Some(minimize_checked(
            opts,
            net,
            budget,
            "build_controllable final minimize",
        )?),
        None => None,
    };

    Ok(GatedCompileResult {
        net: final_net,
        groups: partition.groups.len(),
        skipped_rules,
        skipped_allomorphs,
        tuple_reports,
        group_reports,
    })
}

/// Locates the single `Gate` node this function will interpret: `plan`'s root itself if it IS a
/// `Gate`, or -- when `enumerate_default` wrapped the root in a `Union` alongside composite/
/// structural marker leaves (D2's own shape) -- the one `Gate` child of that `Union`. Every OTHER
/// `Union` child is checked by kind: a [`FragmentSpec::CompositeEmissionMarker`]/
/// [`FragmentSpec::StructuralCompositeMarker`] leaf is the documented out-of-scope case (module
/// doc) and is silently skipped (never built); anything else is a plan shape this module does not
/// recognize and panics loudly rather than guessing.
fn find_gate_node(plan: &Plan) -> NodeId {
    let root = plan
        .root()
        .expect("build_controllable requires a Plan with a root set");
    match plan
        .get(root)
        .unwrap_or_else(|| panic!("plan root NodeId {root} is not interned in this Plan"))
    {
        PlanNodeKind::Gate { .. } => root,
        PlanNodeKind::Union { children } => {
            let mut gate_ids: Vec<NodeId> = Vec::new();
            for &child in children {
                match plan
                    .get(child)
                    .unwrap_or_else(|| panic!("dangling Union child NodeId {child}"))
                {
                    PlanNodeKind::Gate { .. } => gate_ids.push(child),
                    PlanNodeKind::Leaf { fragment, .. } => match fragment {
                        FragmentSpec::CompositeEmissionMarker
                        | FragmentSpec::StructuralCompositeMarker => {
                            // Out of scope for build_controllable v1 (module doc): these two
                            // markers resolve via a completely separate code path
                            // (`emit::emit_with_budget`) into a lexc `String`, not an `Fsm` this
                            // interpreter builds. Checked-for by kind and skipped, never silently
                            // misinterpreted as something buildable.
                        }
                        other => panic!(
                            "unexpected Union-root Leaf fragment for build_controllable: {other:?} \
                             (enumerate_default only ever places CompositeEmissionMarker/\
                             StructuralCompositeMarker leaves alongside the Gate node at the root)"
                        ),
                    },
                    other => panic!(
                        "unexpected Union-root child kind for build_controllable: {} \
                         (enumerate_default's root Union only ever contains a Gate node plus marker \
                         leaves)",
                        other.kind_name()
                    ),
                }
            }
            match gate_ids.len() {
                1 => gate_ids[0],
                0 => panic!(
                    "plan root Union carries no Gate node -- build_controllable has nothing to \
                     interpret (a composite/structural-marker-only plan is out of scope for build() \
                     v1, see this module's own doc)"
                ),
                _ => panic!(
                    "plan root Union carries more than one Gate node -- not a shape \
                     enumerate_default produces"
                ),
            }
        }
        other => panic!(
            "build_controllable expects a Gate node (optionally wrapped in a root Union alongside \
             composite/structural marker leaves) at the plan root, got {}",
            other.kind_name()
        ),
    }
}

/// One gate group's `Compose` node, resolved to its two children `(lexicon_leaf, replace_node)` --
/// `enumerate_default`'s own shape (module doc: "each group's Compose = Compose[ group's
/// LexiconFragment Leaf ..., the shared Replace node ]"). Panics on any other strategy/child-count
/// shape (module doc's "node kinds handled" list).
fn gate_group_children(plan: &Plan, compose_id: NodeId) -> (NodeId, NodeId) {
    let PlanNodeKind::Compose { children, strategy } = plan
        .get(compose_id)
        .unwrap_or_else(|| panic!("dangling Compose NodeId {compose_id} in plan"))
    else {
        panic!("expected a Compose node as a Gate group's child at {compose_id}");
    };
    assert!(
        matches!(strategy, ComposeStrategy::Static),
        "build_controllable only interprets ComposeStrategy::Static (the only strategy \
         enumerate_default ever emits); got {strategy:?} at node {compose_id} -- no lazy-composition \
         primitive exists anywhere in this crate yet, so this is a real Plan-model/interpreter gap \
         (a genuine Step-3 finding), not something safely ignorable"
    );
    assert_eq!(
        children.len(),
        2,
        "a gate-group Compose node must have exactly 2 children (LexiconFragment leaf, shared \
         Replace node) -- enumerate_default's own shape, got {} at {compose_id}",
        children.len()
    );
    (children[0], children[1])
}

/// A gate group's `LexiconFragment` leaf, resolved to its `entries` list. Panics if the leaf isn't a
/// `LexiconFragment` or if `entries` is `None` -- `enumerate_default`'s own invariant is that a
/// gate-group lexicon leaf is ALWAYS `Some(sorted group entries)`, never `None` (that module's own
/// doc, "Per-group `LexiconFragment.entries` is always `Some(...)`").
fn lexicon_fragment_entries(plan: &Plan, lexicon_id: NodeId) -> Vec<LexEntryId> {
    let PlanNodeKind::Leaf { fragment, .. } = plan
        .get(lexicon_id)
        .unwrap_or_else(|| panic!("dangling LexiconFragment NodeId {lexicon_id}"))
    else {
        panic!("expected a Leaf node as a gate-group Compose node's first child at {lexicon_id}");
    };
    let FragmentSpec::LexiconFragment { entries } = fragment else {
        panic!(
            "expected FragmentSpec::LexiconFragment on the gate-group lexicon leaf at \
             {lexicon_id}, got {fragment:?}"
        );
    };
    entries.clone().unwrap_or_else(|| {
        panic!(
            "build_controllable requires Some(entries) on every gate-group LexiconFragment leaf \
             (enumerate_default's own invariant, see that module's doc); got None at {lexicon_id}"
        )
    })
}

/// Reads a gate group's OWN `Replace` node's `cascade`/rule-leaf children and cross-validates them
/// against `prules_in_order` -- the caller-supplied slice `build_controllable` actually compiles
/// with. This is not redundant bookkeeping: it is the one place this function proves the
/// `prules_in_order` slice the CALLER passed to `build_controllable` is the SAME slice (same
/// `PRuleId`s, same order) `enumerate_default` used to build `plan` in the first place -- a mismatch
/// here means the caller handed `build_controllable` a plan and a rule slice that don't agree, which
/// would otherwise silently miscompile every group's rewrite cascade (the `subrule_ok` closure's
/// `rule_pos` indices are positions into `prules_in_order`, so a reordered/different slice changes
/// which subrules a group's key gates without any other signal). Panics loudly on any mismatch,
/// mirroring `crate::enumerate::rule_id_of`'s own panic for the identical caller-contract shape.
///
/// Returns the validated `&ReplaceCascadeSpec` itself (task 1.4: this group's `subrule_ok` is now
/// read straight off THIS return value's `gated_subrules`/`group_key` -- see
/// [`subrule_ok_for_group`] -- rather than re-derived from the `Gate` node's partition).
fn validate_replace_cascade<'a>(
    plan: &'a Plan,
    replace_id: NodeId,
    g: &Grammar,
    prules_in_order: &[&PhonRuleDef],
) -> &'a ReplaceCascadeSpec {
    let PlanNodeKind::Replace { cascade, children } = plan
        .get(replace_id)
        .unwrap_or_else(|| panic!("dangling Replace NodeId {replace_id}"))
    else {
        panic!(
            "expected a Replace node as a gate-group Compose node's second child at {replace_id}"
        );
    };
    assert_eq!(
        cascade.rules.len(),
        children.len(),
        "Replace node invariant: one RewriteRule Leaf child per cascade rule"
    );
    assert_eq!(
        cascade.rules.len(),
        prules_in_order.len(),
        "build_controllable's prules_in_order slice (len {}) does not match the plan's own Replace \
         cascade (len {}) -- the caller passed a slice this plan was not built from",
        prules_in_order.len(),
        cascade.rules.len()
    );
    for (i, &rule_id) in cascade.rules.iter().enumerate() {
        let expected = rule_id_of(g, prules_in_order[i]);
        assert_eq!(
            rule_id, expected,
            "build_controllable's prules_in_order[{i}] does not match the plan's Replace cascade at \
             that position -- the caller passed a slice this plan was not built from"
        );
        let PlanNodeKind::Leaf { fragment, .. } = plan.get(children[i]).unwrap_or_else(|| {
            panic!(
                "dangling RewriteRule Leaf NodeId {} (Replace child {i})",
                children[i]
            )
        }) else {
            panic!("expected a Leaf node as Replace child {i}");
        };
        let FragmentSpec::RewriteRule { rule } = fragment else {
            panic!("expected FragmentSpec::RewriteRule on Replace child {i}, got {fragment:?}");
        };
        assert_eq!(
            *rule, rule_id,
            "Replace node's RewriteRule Leaf child {i} must carry the same PRuleId as \
             cascade.rules[{i}]"
        );
    }
    cascade
}

/// Builds one group's `subrule_ok(rule_pos, sub_idx)` predicate from a `Replace` node's OWN
/// `cascade.gated_subrules` + `cascade.group_key` (task 1.4) -- IDENTICAL shape to `crate::gate::
/// compile_gated_grammar_with_budget`'s own inline closure (that function's body, the `subrule_ok`
/// local), just reading its inputs back out of `plan` data instead of `crate::gate::EntryGroup`/
/// `crate::gate::GatedSubrule`. Before task 1.4 this had to be re-derived from the GATE node's
/// partition instead (see the module doc's "task 1.4" note for why that was the unsound
/// arrangement) -- now it is a pure read of the Replace node's own content, matching whichever
/// `Replace` `NodeId` was resolved for this group.
fn subrule_ok_for_group<'a>(
    gated_subrules: &'a [GatedSubruleRef],
    group_key: &'a [bool],
) -> impl Fn(usize, usize) -> bool + 'a {
    move |rule_pos: usize, sub_idx: usize| -> bool {
        match gated_subrules
            .iter()
            .position(|gs| gs.rule_pos == rule_pos && gs.sub_idx == sub_idx)
        {
            None => true, // ungated subrule: always included, matches crate::gate's own convention.
            Some(gate_index) => group_key[gate_index],
        }
    }
}

#[cfg(test)]
mod equivalence_tests {
    //! The correctness argument for Step 3a (the task's own instruction: "make it semantically
    //! meaningful, not trivial"). For an in-crate gated synthetic fixture, builds BOTH (a)
    //! `compile_gated_grammar_with_budget` (today's direct-compile path) and (b)
    //! `build_controllable(enumerate_default(...))` (this module's plan-walk), then asserts the two
    //! resulting networks are EQUIVALENT BY APPLY -- `apply_up` on every distinguishing query word
    //! must yield IDENTICAL result sets. This is exactly the predicate a future differential oracle
    //! (design.md D4) would use; the module doc explains why it -- not a structural/byte-identity
    //! claim -- is the one that matters. Minimized state/arc counts are ALSO asserted equal, as a
    //! cheap and (here) meaningful extra signal, never a substitute for the apply comparison.

    use std::collections::HashSet;

    use foma::apply::apply_init;
    use foma::options::FomaOptions;
    use foma::types::Fsm;

    use pg_grammar::model::{Grammar, PhonRuleDef};

    use super::*;
    use crate::compose_budget::ComposeBudget;
    use crate::enumerate::enumerate_default;
    use crate::gate::compile_gated_grammar_with_budget;
    use crate::junctions::PhonologyProbe;

    fn load(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    fn prules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
        g.strata
            .iter()
            .flat_map(|s| &s.prules)
            .map(|&id| &g.prules[id.0 as usize])
            .collect()
    }

    /// One MPR-gated subrule (`requiredMPRFeatures="mpr1"`, `c1 -> c2`, no environment) and two
    /// entries realizing both truth values of that gate key -- the SAME shape as `enumerate.rs`'s
    /// own `gated_two_group_fixture` (private to that module's own `#[cfg(test)]` block, so
    /// duplicated here rather than exposed across a test-module boundary; both are synthetic,
    /// self-contained, and delanguaged per this repo's own conformance-grammar convention). `e0`
    /// (no `ruleFeatures`) realizes gate key `[false]` (the subrule does not apply, its underlying
    /// "p" stays "p" on the surface); `e1` (`ruleFeatures="mpr1"`) realizes `[true]` (the subrule
    /// fires, "p" surfaces as "q") -- so "p" and "q" are the two words that can only ever be
    /// analyzed by exactly one of the two gate groups, the property this test's apply comparison
    /// needs.
    fn gated_two_group_fixture_xml() -> &'static str {
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>BuildControllableGatedTwoGroupFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mpr1">f1</MorphologicalPhonologicalRuleFeature>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prule1">
        <Name>gate1</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule requiredMPRFeatures="mpr1">
            <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prule1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e0" partOfSpeech="posV">
            <Allomorphs><Allomorph id="allo0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e0</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e1" partOfSpeech="posV" ruleFeatures="mpr1">
            <Allomorphs><Allomorph id="allo1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
    }

    /// Every raw string `apply_up` yields for `word` against `net` (encoded via
    /// `alphabet.encode_query`, module doc's token-space convention) -- the full literal upper-tape
    /// output set, not a decoded/collapsed projection of it, so this comparison is at least as
    /// strict as the decoded-candidate comparisons `tests/p6_gate_parity.rs` itself uses.
    fn apply_up_results(net: &Fsm, alphabet: &SegAlphabet, word: &str) -> HashSet<String> {
        let mut out = HashSet::new();
        let Some(query) = alphabet.encode_query(word) else {
            return out;
        };
        let mut h = apply_init(net);
        for s in h.up(&query) {
            out.insert(s);
        }
        out
    }

    #[test]
    fn plan_walk_matches_direct_compile_by_apply_on_gated_two_group_fixture() {
        let g = load(gated_two_group_fixture_xml());
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let budget = ComposeBudget::unbounded();

        // (a) today's direct-compile path -- unmodified, exactly what `crate::gate`'s own tests
        // call.
        let direct = compile_gated_grammar_with_budget(&opts, &g, &alphabet, &ro, &budget)
            .expect("direct compile must succeed");
        let direct_net = direct
            .net
            .clone()
            .expect("direct compile must produce a non-empty net");

        // (b) the plan-walk this module ships.
        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let built = build_controllable(&plan, &opts, &g, &alphabet, &ro, &budget)
            .expect("plan-walk build must succeed");
        let built_net = built
            .net
            .clone()
            .expect("plan-walk build must produce a non-empty net");

        assert_eq!(
            direct.groups, built.groups,
            "direct-compile and plan-walk must agree on group count"
        );
        assert_eq!(
            direct.groups, 2,
            "fixture sanity: exactly 2 gating groups expected"
        );

        // Structural sanity (module doc: meaningful here, never a substitute for the apply
        // comparison below) -- both paths run the SAME final `minimize_checked` on networks built
        // from the same primitives, so a divergence here would itself be a real finding.
        assert_eq!(
            direct_net.statecount, built_net.statecount,
            "minimized state counts must match between direct compile and plan-walk build"
        );
        assert_eq!(
            direct_net.arccount, built_net.arccount,
            "minimized arc counts must match between direct compile and plan-walk build"
        );

        // The correctness argument itself: apply_up on every distinguishing query word must be
        // IDENTICAL between the two nets.
        for word in ["p", "q"] {
            let want = apply_up_results(&direct_net, &alphabet, word);
            let got = apply_up_results(&built_net, &alphabet, word);
            assert!(
                !want.is_empty(),
                "sanity: {word:?} must actually analyze on the direct-compile net"
            );
            assert_eq!(
                got, want,
                "apply_up results for {word:?} must match EXACTLY between direct compile and \
                 plan-walk build (want from direct compile, got from build_controllable)"
            );
        }

        // A stronger sanity check than "both nonempty": "p" and "q" must resolve to DIFFERENT
        // results on each net (proving the gate actually distinguishes the two groups on THIS
        // fixture, not that both words happen to hit the same over-permissive branch).
        assert_ne!(
            apply_up_results(&direct_net, &alphabet, "p"),
            apply_up_results(&direct_net, &alphabet, "q"),
            "fixture sanity: \"p\" and \"q\" must resolve to different analyses on the direct-compile \
             net (otherwise the gate isn't actually being exercised)"
        );
    }

    /// Task 1.4's node-purity claim, proven end-to-end on this module's own gated fixture: (a) the
    /// two gate groups' OWN `Replace` `NodeId`s must now be DISTINCT (the fix's whole point -- a
    /// single shared `Replace` node across differently-gated groups was the unsound arrangement
    /// design.md D1's "Soundness invariant" paragraph named), and (b) that distinctness changes
    /// nothing about the compiled RELATION: `build_controllable`'s plan-walk must still be
    /// apply-equivalent to the direct-compile path, exactly the load-bearing correctness argument
    /// [`plan_walk_matches_direct_compile_by_apply_on_gated_two_group_fixture`] already makes (this
    /// test does not replace or weaken that one -- it adds the NodeId-purity claim on top of it,
    /// reusing the same fixture and the same apply-comparison methodology).
    #[test]
    fn purity_differently_gated_groups_have_distinct_replace_node_ids_and_build_stays_apply_equivalent(
    ) {
        let g = load(gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let opts = FomaOptions::default();
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let budget = ComposeBudget::unbounded();

        let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());

        // (a) node purity: the two gate groups must reference DISTINCT Replace NodeIds now.
        let gate_id = find_gate_node(&plan);
        let PlanNodeKind::Gate { children, .. } = plan.get(gate_id).unwrap() else {
            unreachable!("find_gate_node only ever returns the id of a Gate node")
        };
        assert_eq!(children.len(), 2, "fixture declares exactly 2 gate groups");
        let replace_ids: Vec<NodeId> = children
            .iter()
            .map(|&compose_id| gate_group_children(&plan, compose_id).1)
            .collect();
        assert_ne!(
            replace_ids[0], replace_ids[1],
            "task 1.4: two differently-gated groups must get DISTINCT Replace NodeIds -- a node's \
             compiled artifact is now a pure function of its own NodeId (design.md D1), so no two \
             groups needing different subrule_ok may share one Replace node"
        );

        // (b) that distinctness does not change the compiled relation: the plan-walk build must
        // still be apply-equivalent to the direct-compile path.
        let direct = compile_gated_grammar_with_budget(&opts, &g, &alphabet, &ro, &budget)
            .expect("direct compile must succeed");
        let direct_net = direct
            .net
            .clone()
            .expect("direct compile must produce a non-empty net");
        let built = build_controllable(&plan, &opts, &g, &alphabet, &ro, &budget)
            .expect("plan-walk build must succeed");
        let built_net = built
            .net
            .clone()
            .expect("plan-walk build must produce a non-empty net");

        for word in ["p", "q"] {
            let want = apply_up_results(&direct_net, &alphabet, word);
            let got = apply_up_results(&built_net, &alphabet, word);
            assert!(
                !want.is_empty(),
                "sanity: {word:?} must actually analyze on the direct-compile net"
            );
            assert_eq!(
                got, want,
                "apply_up results for {word:?} must match EXACTLY between direct compile and \
                 plan-walk build despite the two groups now having distinct Replace NodeIds"
            );
        }
    }
}
