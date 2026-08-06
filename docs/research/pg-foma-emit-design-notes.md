# pg-foma emit.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/src/emit.rs` implementation comments so the
source can carry a one-line pointer instead of the full argument. Each section corresponds to one
call site; the site names the function/constant so this doc can be found from either direction.

## Compound-chain depth budget is its own dimension, not `ComposeBudget::chain_depth_cap`

`DEFAULT_COMPOUND_CHAIN_DEPTH_BUDGET` reuses `ComposeBudget::check_chain_depth`'s existing
`ChainDepthExceeded` outcome/shape rather than inventing a second budget mechanism, but it is
deliberately its own dimension rather than a reuse of `ComposeBudget::chain_depth_cap`'s shared
field / `HC_COMPOSE_CHAIN_DEPTH_BUDGET`. That field is `crate::peel::ReduplicationPeeler`'s
per-word, apply-time recursion-depth counter (opt-in, off by default). This loop's depth is a
compile-time, one-shot static-unrolling count for the whole grammar — a different quantity that
merely happens to share the "how many times does a chain repeat" shape. Reusing the same field/env
var for both would let calibrating one silently move the other, the same trap
`DEFAULT_COMPOUND_PAIR_BUDGET`'s own doc (`crate::compose_budget`) names for keeping that dimension
separate from `DEFAULT_TUPLE_BUDGET`. Unlike `chain_depth_cap`, this dimension defaults on: never
blowing up on any grammar is a standing requirement, not an opt-in safety net.

The value (200) is generous headroom above the DTD's practical `multipleApplication` ceiling (9,
also this crate's own `recursive-endocentric-compounding` fixture's ceiling), while still catching
a pathological or merely mistaken co-feeding-rule sum before the loop's own linear-in-depth lexc
emission grows unreasonably large. `CompoundingRuleDef::max_apps` is a bare `u16` with no clamp
enforced anywhere in this crate's loader, and multiple co-feeding rules sum their `max_apps` into
the bound, so this is checked eagerly, before any of the chain's own lexc text is written.

## `classify_affix`: `CircumfixPrefix` beats both `Infix` and `Reduplication`

An RHS that is simultaneously circumfixing (insert before the first `Copy`, insert after the last)
and reduplicating (some part echoed by two or more `Copy` actions) must classify `CircumfixPrefix`,
not `Reduplication` or `Infix`. Both alternative shapes are DTD-reachable, not vacuous:
`MorphologicalOutput` is declared as an unconstrained repeated choice group
(`HermitCrabInput.dtd:420`) and the loader places no uniqueness constraint on a `CopyFromInput`'s
`index`.

`build_structural_composites` is the architecturally correct, unconditionally-guaranteed home for
this shape: `struct_extend` calls `pg_rules::morph::synthesize` directly, replaying every
`OutputAction` in RHS document order with no reference to `Role` and no assumption that a `Copy`
run is contiguous or occurs only once per part.

Neither alternative mechanism is safe to leave in control instead:

- `crate::preexpand` happens to also resynthesize correctly today, but its module doc scopes its
  mechanism to interdigitation/boundary-fusion, never circumfix — relying on it here depends on a
  property that module was never designed around. The capability layer
  (`crate::capability::CircumfixStructuralCompositePredicate`) reads `is_structural_rule` as ground
  truth for coverage, so a rule misclassified `Infix` here would make that predicate refuse on a
  grammar `preexpand` already covers.
- `crate::peel::ReduplicationPeeler`'s four scan kinds are each a one-sided surface-string match;
  none searches for a repeated span with independent material on both sides at once, which is
  exactly what a circumfix-plus-reduplication surface has. Leaving `Reduplication` in control would
  hand the shape to a mechanism structurally unable to recall it, not merely mis-attribute a still-
  correct construction.

Ownership handoff is clean because both mechanisms key off this same function:
`crate::preexpand::candidate_rules` selects by `rule_role` (which calls `classify_affix` on the
rule's first allomorph), so the moment `classify_affix` returns `CircumfixPrefix` for a
simultaneously-shaped allomorph 0, `preexpand`'s candidate set drops the rule the same instant
`is_structural_rule` picks it up — no rule is ever claimed by both mechanisms or by neither.

`is_reduplicating` is checked after the leading/trailing circumfix test but before the
interior-action (`Infix`) test: circumfix beats both, reduplication beats infix.

## Diacritics: NFD combining-mark runs must be declared as lexc multichar symbols

Real 100%-recall bug, not a reference-grammar gap. `pg_grammar::nfd::nfd` NFD-normalizes every
surface string this crate emits and the query word `crate::analyzer` feeds to `apply_up`, so a
precomposed accented letter like é (one codepoint) becomes two codepoints — "e" + COMBINING ACUTE
ACCENT — in every lexc literal this emitter writes.

Root cause: `vendor/foma`'s lexc tokenizer (`lexcread.rs::lexc_string_to_tokens`) has no special
handling for this — absent a declared multichar symbol, it emits one symbol per codepoint, so the
compiled network gets two separate arcs. But `vendor/foma`'s `apply.rs` (`sigmatch_array`
construction) unconditionally merges any base codepoint with its immediately following run of
combining codepoints into one query-side token and forces it to `IDENTITY`, which only ever matches
a network's `?` (`UNKNOWN`) wildcard arc, never two ordinary literal arcs. A network with no
declared multichar symbol for the pair has no arc `IDENTITY` can match at that position: total
non-match for any word containing a base+combining-mark sequence, independent of affixation (bare
roots too), and never triggered by scripts whose letters don't NFD-decompose into base+combining
pairs.

Fix: declare each such run (e.g. `"e\u{301}"`) as its own `Multichar_Symbols` entry
(`combining_run_symbols`). This makes `lexcread.rs`'s `first_mc_prefix` match the whole run as one
symbol at compile time, and symmetrically makes `apply.rs`'s initial sigma-trie walk (which runs
before the combining-merge check) match the same whole run as one known symbol, so the merge
check's `is_combining` probe on the remainder finds nothing left to merge. Both sides then agree on
one token for the pair.

Scope: `combining_run_symbols` only catches a run entirely inside one char-def's own
representation — true for every reference/edge-case grammar's convention of modeling an accented
letter as a single segment. A grammar that instead models a combining mark as its own standalone
char-def (a base segment immediately followed by an unrelated "tone mark" segment) needs
`boundary_combining_run_symbols`: apply's flat character-by-character retokenization never sees a
char-def boundary, so the same merge bug reappears across it. That function declares
`trailing_combining_run(P) ++ M` for every Segment representation `P` and every mark-initial
Segment representation `M`, plus the length-2 chain `P·M1·M2` for two mark-initial reps — capped at
chain length 2, since a length-N chain needs the cartesian product of N mark-initial reps and no
known grammar stacks three or more standalone combining-mark morphemes. `P` ranges over every
char-def in the grammar, not just ones provably adjacent to an `M` in some actual entry — an
upward-safe over-declaration, since an unused declared symbol changes nothing about which arcs
exist, and far simpler than re-deriving actual adjacency from every root/affix's authored text.
Dormant today (no reference/edge-case grammar uses a standalone combining-mark char-def) but real.

## Bare-root phonology enrichment uses `generate_words`, not `probe_surface`

A grammar with real phonological rules can obligatorily change a root's own surface even with no
morphological rule applied at all (post-nasal voicing, vowel coalescence, gradation/epenthesis at a
root-internal consonant cluster). `surface_variants`/`pattern_variants` only ever re-segment the
authored literal text, never running it through the phonological cascade — so `collect_roots` unions
in the real post-cascade surface as an extra, upward-safe spelling.

`Morpher::generate_words` is tried first, not `probe_surface`: `probe_surface` ultimately calls
`pg_rules::rewrite::probe_apply_rule_cached`, which applies every phonological rule in the stratum
unconditionally (`FeatureStruct::EMPTY` as the "current word", ported faithfully from C#
`SurfacePhonology`'s POS-blind probing design). For a bare root specifically that is actively wrong
whenever the grammar scopes different phonological rules to different POS in the same stratum: the
`polysynthetic-stratal-derivation-chain` fixture's `prDelReins` rule is `requiredPartsOfSpeech=
"posDelR"` only, but `probe_surface` applies it to `posMDC`'s "buiibuii" too (an empty
`FeatureStruct` vacuously satisfies every POS gate), returning "bubu" — the other probe root's
answer — instead of "buuubuuu". `generate_words` has no such blind spot, since it gates on the
entry's actual `syn_fs`; it is used whenever a `Morpher` is available (always true here), with
`probe_surface` remaining the fallback for the defensive case a caller runs this with
`morpher = None`.
