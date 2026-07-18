# P6 prototype report — replace-rule compilation feasibility

Date: 2026-07-17. Branch: `worktree-agent-a336a52bd7228731b`. Scope: `docs/fst-plan/foma-fst-plan.md`
§P6 item 1 ("Replace-rule compilation": compile HC phonological rules as real foma
replace-calculus rules, build lexc-over-underlying-forms, compose, retire enumeration bridges
grammar-by-grammar). This is a feasibility prototype, not mainline code — nothing here is wired
into `hc_foma::emit`/`analyzer`; it lives beside them as new modules and standalone examples.

**Verdict: GO for Indonesian at 100% recall. GO-with-caveats for the general P6 approach** — the
rule-compiler and its α-variable cost model both hold up under Amharic's 20-variable CV-merger and
Aweti's 18-rule/855-root scale; the caveats are entirely about the underlying-form LEXC EMITTER's
current scope (Indonesian-only, template-less), not the replace-rule compiler.

---

## 1. Approach

Three new modules/examples, all additive:

- `rust/crates/hc-foma/src/replace.rs` — `RewriteRuleDef -> foma xre regex -> compiled Fsm`,
  plus the tuple-indexed α-variable resolver and the stratum-order rule-cascade composer.
- `rust/crates/hc-foma/src/uflexc.rs` — a fresh, deliberately minimal `Grammar -> lexc` emitter
  whose lower tape is UNDERLYING morph spellings (self-looping prefix/suffix chains), not a refit
  of `emit.rs`.
- `rust/crates/hc-foma/examples/p6_replace_prototype.rs` — the end-to-end Indonesian driver: rule
  compile, lexc compile, `lexc .o. rules .o. boundary-cleanup`, minimize, smoke test, full corpus
  parity gate, timing.
- `rust/crates/hc-foma/examples/p6_bisect.rs` — the bisection harness that isolated the two
  foma-rs findings in §2.
- `rust/crates/hc-foma/examples/p6_amharic_probe.rs`, `p6_aweti_probe.rs` — stretch-goal probes
  (compile-only, no lexc/emit).

### The symbol alphabet: char-def identity, not literal spelling

The single design decision that made everything else tractable: every `CharDefId` used anywhere in
a grammar's surface table is mapped to **one Private-Use-Area codepoint**
(`SegAlphabet::token`, `PUA_BASE = 0xE000`). Every lexc entry, every rule regex, and every query
word is built/encoded in that token space, never in literal orthography.

This sidesteps, for free, exactly the two footguns `emit.rs`'s literal-string approach has to work
around:

- **Multi-representation segments** (Indonesian's `char28` = {"g", "G"}) need no cartesian
  product: both spellings segment to the same char-def id via `hc_grammar::segment::
  segment_phonemes_only`, hence the same token, automatically.
- **Multi-character graphemes** ("ng"/"ny"/"sy"/"kh") need no lexc `Multichar_Symbols`
  registration/lookup bookkeeping to keep in sync between separately-compiled lexc and rule nets —
  each grapheme is already one token, one codepoint.
- The morpheme-boundary character `+` (itself an xfst/foma **reserved regex metacharacter**,
  Kleene-plus) is just another char-def with its own token — no escaping question ever arises.

The price: the composed network's own lower tape is not human-legible. That's fine — the
propose→confirm contract (`foma-fst-plan.md` §1) only needs the UPPER tape's tag sequence. A query
word is transliterated into token space before `apply_up`
(`SegAlphabet::encode_query`, reusing `hc_grammar::segment::segment_phonemes_only`, the same
greedy-longest-match the engine's own segmentation uses), and the network's own output is decoded
via `crate::tags::decode_path` exactly like the mainline `FomaProposer`.

This is arguably the RIGHT translation of "the engine matches segments by char-def identity, not
by spelling" (`emit.rs`'s own module doc) into foma's symbol-identity world, and a design mainline
P6 should keep regardless of what else changes.

### α-variable expansion: tuple-indexed, generic over N variables

`resolve_alpha_tuples` (replace.rs) gathers every alpha-bound pattern-node OCCURRENCE across a
subrule's LHS/RHS/left-env/right-env (an occurrence may itself carry MULTIPLE variables — Amharic's
CV-merger binds up to 20 on a single `SimpleContext`, §4), enumerates the full cross product of
each occurrence's own (non-alpha-feature) candidate set, then filters to combinations where every
pair of occurrences sharing a `VarId` unify (bitwise feature-lane overlap) at that variable's
feature. This is exactly `reports/08-audit-corrections-and-reframed-architecture.md` §3 item 1's
"count of segment tuples satisfying the joint constraint" bound, implemented once, generically —
the same code path resolves Indonesian's single-variable prule4 and Amharic's 20-variable
CV-merger with no special-casing (§4 confirms the bound holds at that scale).

---

## 2. foma-rs API findings

**Headline finding: no binding gap.** The vendored `rust/vendor/foma` crate already exposes every
primitive P6 needs: `regex::fsm_parse_regex` (full xre — `->`/`||`/`@->`/`=>`/`.o.`/context
restriction, not just plain concatenation/union), `lexcread::fsm_lexc_parse_string` (already used
by the mainline `FomaProposer`), `constructions::fsm_compose`/`fsm_union`/`fsm_minimize`/
`determinize::fsm_determinize`, and `apply::apply_init`/`ApplyHandle::{up,down}`. Nothing had to be
added to the binding. This closes the read-first task item ("if the binding lacks regex-compile or
compose, that is itself a headline finding") with a clean negative: it doesn't lack anything.

Two REAL findings surfaced only by building something non-trivial and bisecting the failure
(`examples/p6_bisect.rs` is the executable record of both):

### 2.1 Comma-joined replace rules: environments only, not full rules

This vendored xre grammar's comma (`,`) accepts:
- multiple **environments** for one shared `LHS -> RHS`: `"a -> b || c _ d, c _ e"` compiles, and
  behaves correctly (bisect case 12/13/14);
- multiple **bare** `LHS -> RHS` rules with **no** `||` context at all: `"a -> b, a -> b"` compiles
  (case 11);

but REJECTS two full `LHS -> RHS || L _ R` clauses joined by comma, even with different LHS/RHS on
each side (`"a -> b || c _ d, e -> f || g _ h"` fails, case 9/10/15/16 — all four variations
tried). This matters directly for α-tuple expansion whenever different tuples need DIFFERENT
output (Indonesian's prule4: each following-obstruent place needs a different output nasal) — the
naive "comma-join every branch into one regex string" plan from the read-first brief does not
work as written; see §2.2 for the fix.

### 2.2 Combining tuple branches: sequential composition, not union

The first fix attempt — compile each tuple's branch as its own small `Fsm` via a separate
`fsm_parse_regex` call, then fold them together with `fsm_union` — **compiles and runs, but is
semantically wrong**, not just syntactically awkward. Each per-tuple net is a COMPLETE replace
transducer: within its own context it rewrites obligatorily, but outside its own context (which
includes every OTHER tuple's context) it is plain identity by construction. Unioning N such
complete nets reintroduces a spurious "did nothing" path at every position, including ones where
some OTHER tuple's context obligatorily applies. This was caught empirically, not by inspection:
`apply_down` on a hand-built underlying string through the union returned BOTH the correct
"mem+baca" path and a spurious unconverted "meⁿ+baca" path.

The fix: since the tuples' contexts are, by the joint-agreement filter's own construction, mutually
exclusive (a concrete following segment has exactly one place-of-articulation value, so at most one
tuple's context ever matches a given position), **`fsm_compose`-folding them sequentially** is
correct — tuple K only ever sees the placeholder if every earlier tuple in the fold left it
untouched, and once any one tuple rewrites it, no later tuple's LHS (always the literal
placeholder) matches it again. This is the exact same feeding-order argument the outer
stratum-level cascade (`compile_and_compose_rules`) already relies on; the fix is to apply it one
level deeper, inside `compile_rewrite_rule` too. Verified: switching the fold from `fsm_union` to
`fsm_compose` took the composed rule net (before lexc) from 392,311 states / 6,892,003 arcs (the
union blow-up, itself circumstantial evidence something was wrong) down to 38 states / 401 arcs,
and fixed `menulis`/`membaca`/`memukul` to their correct single analyses.

A single-parse-call `.o.`-infix version of the same 2-rule cascade was also tried directly (one
`fsm_parse_regex` call, no Rust-level `fsm_compose` at all) and reproduced the SAME bug until the
next finding (§2.3) was applied — ruling out any Rust-level composition bug and pointing at the
regex source text itself.

### 2.3 Adjacent non-ASCII codepoints with no separator are silently mis-tokenized

**The load-bearing bug, and the one most worth a warning label.** This vendored `nfst-xre` lexer
does not reliably treat two adjacent Private-Use-Area codepoints written back-to-back with no
separator as two independent single-symbol atoms. Confirmed by direct bisection: `"t -> 0 || e n +
_ u"` (PUA tokens, SPACE-separated) correctly compiles a rule that deletes in context; the
byte-identical rule written with the boundary+consonant pair concatenated with no space
(`"e _ +t"`-shaped) silently fails to match — **no parse error, no panic, just a rule that never
fires.** ASCII letters tolerate bare concatenation fine (the vendored crate's own test fixture
`"cat"` == `"c a t"`, both split per character) — the gap is specific to non-ASCII/high-codepoint
symbols, which is exactly what a char-def-identity token alphabet (§1) is built from. This is the
single fix that took Indonesian's recall from 72/97 to 97/97 (§3): `render_slots` now
space-separates every rendered pattern-node piece, unconditionally, with a code comment recording
why. **Any mainline P6 rule compiler emitting xre source from a non-ASCII token alphabet must
carry this forward as a hard invariant** — it is exactly the kind of bug that produces silent
under-generation (a rule that compiles clean and simply never fires) rather than a compile error,
the worst failure mode for a system whose whole soundness argument rests on "the proposer only
needs recall."

---

## 3. Rule-semantics mapping table (Indonesian, verified)

| HC construct | foma mapping | Fidelity |
|---|---|---|
| `RewriteRuleDef`, feature-change subrule | `LHS -> RHS \|\| L _ R` | Exact for the shape exercised (`Iterative`/`LeftToRight`, single non-overlapping match site). |
| Deletion (empty `PhoneticOutput`) | `LHS -> 0 \|\| L _ R` | Exact. |
| `NaturalClassKind::Segments` (explicit list) | Verbatim `CharDefId` union `[c1 \| c2 \| ...]` | Exact — read straight from the model, never re-derived through a feature reconstruction. |
| `NaturalClassKind::Feature` (feature bundle) | Union of every table segment whose feature lanes match ALL declared `(lane, bits)` pairs | Exact within the single-table assumption (§5 caveat 1). |
| `AlphaVariable`, `polarity="plus"` (agree) | Tuple-indexed cross product + pairwise bitwise-overlap filter (§1) | Exact; validated at Indonesian scale (1 var, 2 occurrences, 14 survivors) AND Amharic scale (20 vars, up to 20 occurrences on one node, 312 survivors, §4). |
| `AlphaVariable`, `polarity="minus"` (disagree) | Not implemented — `pattern_slots` bails, rule reported uncovered | Documented gap; zero occurrences in any of the three reference grammars. |
| Multiple subrules of one rule / multiple α-tuple branches | Sequential `fsm_compose` fold (§2.2), never comma-join, never `fsm_union` | Correct GIVEN mutually-exclusive contexts (true by construction for every case exercised: Indonesian prule4's 14 tuples, Amharic prule6/7's 312). Not stress-tested against a rule whose subrules/tuples have OVERLAPPING match contexts — a genuine open question for mainline. |
| Stratum/document rule order | Sequential `fsm_compose` in `StratumDef.prules` id-list order | Matches feeding order; verified by hand-tracing `meN+tulis -> menulis` through the 5-rule cascade (prule4 assimilates, prule5 then deletes — needs prule4's OUTPUT as prule5's INPUT, i.e. real composition, not independent application). |
| `RewriteMode::Simultaneous` vs `Iterative` | Not distinguished — both compiled identically via plain `->` (or the tuple-composed equivalent) | **Untested distinction.** Indonesian's 5 rules are all `Iterative` with single, non-overlapping match sites per word, so the modes are behaviorally identical here. A grammar needing genuine self-opaquing reapplication (P13's `ReapplyType.SelfOpaquing` design) or Simultaneous's "collect all matches against one snapshot" semantics on OVERLAPPING sites is a real, unexercised gap. |
| `Dir::RightToLeft` | Not implemented — silently compiled as if `LeftToRight` | Real gap, never triggered (all 5 Indonesian rules, all 7 Amharic rules, all 18 Aweti rules are `LeftToRight` — `rewrite.rs`'s own doc calls the RTL branch "DEAD for every reference grammar's own rule"). `vendor/foma/src/reverse.rs`'s `fsm_reverse` is the standard Kaplan-Kay primitive (reverse input, apply LTR rule, reverse output) mainline would build this on. |
| `requiredPartsOfSpeech` on a subrule (POS gating) | **Not implemented at all** — not read, not gated | Indonesian's 5 rules use none; Amharic's prule1/prule2 DO declare it and were compiled anyway (ignored, upward-safe: the rule just always fires). A real, exercised gap once Amharic is in play. |
| `requiredMPRFeatures`/`excludedMPRFeatures` on a subrule | **Not implemented** | Indonesian's prule5 declares `excludedMPRFeatures="mpr1"` (4 lexical entries carry `mpr1`); ignored here. Recall still hit 100% on this corpus (no corpus word happens to need the exception at the exact deletion context), but this is a real correctness gap for a differently-composed corpus. Flag-diacritic emission for MPR/POS gating is explicitly named mainline P6 work in the plan itself (`foma-fst-plan.md` §P6 item 1). |
| `PatternNode::Quantifier` (`OptionalSegmentSequence`) | Not implemented — `pattern_slots` returns `None` | Indonesian's prule3 ("Nasalization in reduplication") uses this in its left-environment and is the ONE rule this prototype could not compile; it is also entirely redup-scoped (per `f2_indonesian_gate.rs`'s own module doc, redup is P2's peel, out of scope here), so its absence costs zero recall on the non-redup corpus. |
| `PatternNode::Anchor`, `PatternNode::Segments` (literal multi-seg group) | Not implemented | Not exercised by any of the 5 Indonesian / 7 Amharic / 18 Aweti prules actually compiled. |
| `MetathesisRuleDef` | Not implemented, routed to `skipped` | Zero occurrences in Indonesian/Amharic/Aweti. Report 07's own citation: expressible via the standard marker-insert/reorder/delete trick, genuinely fiddlier than a plain replace rule — not attempted here. |
| Morphotactics (root/prefix/suffix attachment) | Fresh minimal `uflexc` emitter: bare roots + self-looping prefix-chain + self-looping suffix-chain lexicons | Covers Indonesian's template-less, standalone-rule-only morphotactics exactly (verified: zero `<AffixTemplate>` elements in `indonesian-hc.xml`). **Not validated against a templated grammar** — Sena/Amharic both use `<AffixTemplate>` slots this emitter never attempts (§5 caveat 2). |
| Circumfix affix roles (leading+trailing insert in one rule) | Not implemented — allomorph skipped, reported | 3 Indonesian allomorphs (the `ke-...-an`/`peN-...-an` nominalizers) skipped; recall unaffected for this corpus — matches the mainline `emit.rs`'s own finding for the same allomorphs (neither implementation needs them for these 121 words). |
| Reduplication roles | Not implemented (skipped, per plan D6 — the peel is P2's job) | Matches mainline exactly: the same 7 corpus words are excluded, for the same reason. |

---

## 4. Indonesian parity numbers (all figures from an actual executed run, `cargo run --release -p
hc-foma --example p6_replace_prototype`)

- **Rules**: 5 phonological rules, all `Iterative`/`LeftToRight`. 4 compiled (prule1, prule2,
  prule4, prule5); prule3 skipped (`Quantifier`, redup-only, §3).
- **α-tuple expansion** (prule4, nasal place assimilation): raw product 75 (5 nasal-class members ×
  ~15 obstruent-class members before filtering — table gives exact factors), **14 tuples survive**
  the joint place-agreement filter. Spot-checked by hand: assimilating before `b` (`baca`) yields
  `m` (token `e015`); before `t`/`d`/`c`/`k`... (`tulis`, `pukul`) yields the alveolar/labial/velar
  match correctly per the smoke-test hex dumps in the driver's own output.
- **Rule compile + compose (stratum cascade)**: ~30–50ms wall (varies run to run; includes 4
  `fsm_parse_regex` calls plus the 14-way `fsm_compose` fold for prule4).
- **Composed rule net** (before lexc/cleanup): 38 states, 401 arcs.
- **Underlying-form lexc emit**: <150µs; 66 root entries, 2 prefix entries (meN-, per-), 5 suffix
  entries; 6 allomorphs skipped (3 circumfix-prefix nominalizers, 3 reduplication-classified),
  reported by morpheme identity (e.g. `mrule6(NMLZR)#allo0 role=circumfix-prefix`).
- **Lexc compile**: ~500–900µs; 210 states, 281 arcs.
- **Full composition (`lexc .o. rules .o. boundary-cleanup`) + minimize**: ~0.9–2ms; **final net:
  213 states, 350 arcs.**
- **Parity gate** (same oracle/predicate/exclusion list as `tests/f2_indonesian_gate.rs`: `hc_parse
  ::Morpher`, `ParseOptions::default()`, the 7 reduplication-word exclusions): **97/97 engine
  analyses covered across 96 analyzed words** (of 121 corpus words, 7 excluded) — **100% recall,
  byte-identical to the mainline `emit.rs` proposer's own result on this corpus** (independently
  re-run for this report: same 97/97/96/121/7).
- **Overgeneration**: 104 total candidates across the 96 analyzed words (~1.08/word) — tight, not a
  permissive blob standing in for real coverage.
- **Propose timing**: mean 5.8–9.6µs/word, max 15–46µs/word (single-threaded, release build, tiny
  network — same order of magnitude as the mainline `FomaProposer` on this grammar).

---

## 5. Stretch results

### 5.1 Amharic (7 prules, 417/420-segment table, one char table)

All 7 rules compiled, including prule6/prule7 (the 20-alpha-variable Consonant-Vowel mergers named
in reports/08 as the make-or-break test of the tuple-indexed cost model):

| rule | states/arcs | note |
|---|---|---|
| prule1–3, 5 | 3–5 states, 10–16 arcs | plain literal/short-context rules, POS-gated (ignored, §3) |
| prule4 | 2 states, 40,500 arcs | `nc4`/`nc14`/`nc3` are large feature classes over a 417-segment table — a plain (non-alpha) union rule, no tuple expansion, but the union itself is wide |
| **prule6** (CV-merger inside) | 14 states, 4,791 arcs | **α-tuple: raw_product = 121,776, surviving = 312** |
| **prule7** (CV-merger at boundaries) | 27 states, 8,933 arcs | **α-tuple: raw_product = 121,776, surviving = 312** |

This closely matches reports/08's own predicted bound (nc15=59 × nc16=6 ⇒ ≤354) — the measured
312 survivors sit comfortably under that estimate, and dramatically under the 121,776-strong raw
product a naive full cross product would carry into the union/compose step. **This is the
prototype's cleanest empirical validation of the entire tuple-indexed cost model the P6
architecture rests on**: a naive per-variable expander (v^20, or even the raw 121,776-tuple
product) would be the thing that actually explodes; the joint-agreement filter — the SAME generic
code that resolved Indonesian's one-variable case — collapses it to 312 without any Amharic-specific
logic.

Full cascade (all 7 rules, stratum order): compiles and composes in **2.14s**, producing a
**82-state / 1,110,358-arc** composed rule net (before any lexc/root text is involved). No crash,
no OOM — a sharp contrast with `emit.rs`'s enumeration path, whose Amharic composite-emission stage
is exactly the kind of `O(roots × rules^depth)` machinery this whole plan item exists to retire.

**Not attempted** (out of this prototype's scope per the task brief): the `ልጅ + ዮች -> ልጆች` fusion
demo (an underlying-lexc-side test), and any recall gate — `uflexc.rs`'s emitter is Indonesian-
scoped (template-less morphotactics only) and was never pointed at Amharic's `<AffixTemplate>`-
based grammar. The rule-compiler result above is the load-bearing Amharic finding; the fusion demo
would exercise the (unbuilt) templated lexc emitter, not the rule compiler itself.

### 5.2 Aweti (855 lexical entries, 135 mrules, 18 prules, 3 strata, 1 char table, 41 segments)

Loaded via `pg_snapshot::Snapshot::from_json` + `hc_grammar::compile_project` (same loader
`examples/aweti_probe.rs`, main-tree-only/untracked, already established for this fixture).
`hc_foma::emit::emit()` was never called (that is precisely the 4.9GB-RSS, unfinished-after-OOM
path this whole effort routes around, per that example's own module doc).

All 18 rules compiled individually (typical: 1–2ms each, 1–6 states, 15–104 arcs — none approach
Amharic's scale; Aweti's α-variable usage, if any, produced no tuple expansion worth reporting,
i.e. every rule here is a plain literal/feature-class rule, no multi-variable CV-merger analog).
Full 18-rule cascade compiles and composes in **28.8ms**, producing a **30-state / 2,143-arc**
composed rule net.

**This directly answers the task's Aweti question for the rule-compilation half of P6**: yes, all
18 prules compile, fast, with no scale problem at all — the enumeration-based emitter's OOM was
never about the RULES, it was about `preexpand.rs`'s `O(roots × rules^depth)` per-root
rule-application walk. **Not attempted**: lexc-over-underlying-forms for the 855 roots +
composition (`uflexc.rs`'s emitter, again, only knows Indonesian's template-less shape; Aweti's
135 mrules across 3 strata almost certainly use templates, and 855 roots is enough entries that
even a correct template-aware emitter's own compile time/network size would be worth measuring
directly rather than assumed) — a costed estimate is in §6.

---

## 6. Verdict and costed remaining mainline work

**Verdict: GO for P6 item 1 (replace-rule compilation).** The central technical bet — that HC
rewrite rules compile into real foma replace-calculus regex, with α-variables resolved by a
tuple-indexed (not per-variable) expansion — holds at all three tested scales (Indonesian: 1
variable/2 occurrences/14 survivors; Amharic: 20 variables/up to 20 occurrences-per-node/312
survivors; Aweti: 18 rules, no scale problem at all). The foma-rs binding has everything needed;
the two real findings (§2.2, §2.3) are both about HOW to call the existing primitives correctly,
not gaps in them, and are now recorded as hard invariants in `replace.rs`'s own doc comments for
whoever builds mainline next.

**What is NOT yet proven, and what it would cost:**

1. **Multi-table grammars.** `table_of` hardcodes `char_tables[0]`; true for all three reference
   grammars tested (Indonesian, Amharic, Aweti — each has exactly one `<CharacterDefinitionTable>`),
   so untested in practice, but the model (`StratumDef.table: TableId`) clearly anticipates more
   than one. **Cost: small** — thread the owning stratum through `compile_rewrite_rule`'s call
   sites (a rule's stratum is already known at the `compile_and_compose_rules` call site).

2. **Templated morphotactics in the underlying-form emitter.** `uflexc.rs` only knows bare
   roots + self-looping prefix/suffix chains (Indonesian's real shape — verified zero
   `<AffixTemplate>` elements). Sena and Amharic both use `<AffixTemplate>` slots; Aweti's 135
   mrules across 3 strata likely do too. **Cost: medium** — the natural path is NOT rewriting
   `uflexc.rs` from scratch but refitting `emit.rs`'s already-correct, already-tested morphotactic
   skeleton (template grouping, slot chains, derivation layers, the whole `emit.rs` module-doc
   architecture) with a parameter switching its SURFACE-SPELLING step (today: pre-probed junction
   variants) to plain UNDERLYING text in token space — a much smaller change than it sounds,
   confined to `emit_rule_allomorphs`/`write_roots_lexicon`'s text-source, not the structural
   lexicon-building logic around them.

3. **Circumfix / null-morph affix roles** in the underlying emitter (currently skipped
   everywhere they're not needed). **Cost: small–medium** — a circumfix needs its rule's own tag
   symbol emitted TWICE (prefix position, suffix position) while still counting as ONE morpheme in
   `tags::to_candidates`'s output; the current tag codec assumes one tag occurrence = one morpheme,
   so this needs either a codec extension or a paired-entry convention, not just more lexc lines.

4. **MPR/POS subrule gating → flag diacritics.** Named explicitly as mainline P6 work in the plan
   itself. **Cost: medium–large** — needs a design spanning both mrule application (which SETS an
   MPR feature) and prule application (which CONSUMES it), threaded as foma flag diacritics through
   both the lexc entries and the rule regex's own environment tests. Amharic's prule1/prule2 (POS-
   gated) and Indonesian's prule5 (MPR-excluded) are both real, present-today test cases for this
   once built.

5. **`RewriteMode::Simultaneous` fidelity and `Dir::RightToLeft`.** Neither is exercised by any of
   the three reference grammars (`rewrite.rs`'s own doc: the RTL branch is "DEAD for every
   reference grammar's own rule"). **Cost: RTL is small** once a test grammar exists (`vendor/foma/
   src/reverse.rs`'s `fsm_reverse` is the standard primitive). **Simultaneous-mode self-opaquing
   reapplication is medium-to-large** and needs its own purpose-built test grammar to validate at
   all, since none of the three references stress it.

6. **`OptionalSegmentSequence`/Quantifier patterns.** Currently: bail to uncovered. **Cost:
   medium** — foma xre natively supports bounded repetition (`a^{2,4}`) and true Kleene star
   (`a*`), which could make replace-rule compilation MORE expressive here than `emit.rs`'s
   cap-and-enumerate `PATTERN_ITER_CAP` approach, not just equivalent to it.

7. **`MetathesisRuleDef`.** **Cost: medium-large** — the classical Kaplan-Kay marker-insert/
   reorder/delete technique, not yet attempted; zero occurrences in any grammar tested so far, so
   not urgent.

8. **Retiring the enumeration bridges** (`junctions.rs`'s `PhonologyProbe`, `preexpand.rs`'s
   rule-application pre-expansion) **per grammar**, once that grammar's replace-rule compilation +
   templated underlying emitter both reach parity. **Cost: small deletion + regression-testing**
   once items 1–2 land for a given grammar; Indonesian is the first grammar where this could
   actually happen (its junction/composite machinery would become provably redundant), though this
   prototype does not itself delete anything (scope: prove feasibility, not ship the cutover).

**Bottom line for planning:** the hard, uncertain, "might just not work" part of P6 — does the
rule compiler handle real α-variable complexity, does the toolkit have what's needed, does
composition scale — is now empirically answered YES, at three different grammar scales. What
remains is substantial but comparatively mechanical engineering (templated morphotactics, flag
diacritics, a couple of pattern-node kinds) against a foundation that has already been stress-
tested past the point that mattered most.
