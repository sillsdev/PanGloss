# Can we go FST-only — drop HermitCrab verification entirely?

**Scope:** `rust/crates/hc-hybrid` (all modules) and `rust/crates/hc-fst`, read against
`docs/fst-plan/{HYBRID_FST_FEASIBILITY.md, FST_FAST_PATH_PLAN.md, FST_FULL_GRAMMAR_PLAN.md,
HERMITCRAB_FST_ADVISOR.md, HYBRID_FST_RUST_PLAN.md, LEVER_2.md, F1_QUIRK_AUDIT.md}` and
`docs/hermitcrab-rust-port-audit.md`. All measurements below were taken **this session**, on a
`--release` build, using the real Indonesian/Sena/Amharic grammars (copied read-only from the
parent repo's gitignored `samples/data/` into this worktree, since a fresh worktree checkout does
not have them) and the one real golden set that exists in this worktree
(`rust/parity-out/golden/fst-advisor/sena/`). Every number is labeled **[measured]**, **[doc
claim]**, or **[estimate]** — do not conflate them. This report cross-references
`reports/02-established-fst-libraries.md` (FST-library build-vs-buy) and
`reports/03-parse-latency-profile.md` (general latency root-causing), both written by sibling
investigations in this same worktree; it does not repeat their scope.

---

## 0. The one fact that reframes the whole question

**`hc-hybrid` — the entire propose-with-FST/verify-with-HermitCrab system this report is about —
is not in the shipped product today.** Verified independently, from the dependency graph itself:

```
hc-wasm/Cargo.toml  → hc-grammar, hc-parse, hc-realize, hc-lexicon      (no hc-hybrid, no hc-fst directly)
hc-ffi/Cargo.toml   → hc-parse, hc-grammar                              (no hc-hybrid)
hc-cli/Cargo.toml   → hc-parse, hc-grammar, hc-realize, hc-featstruct, hc-fst, hc-rules, hc-hybrid
```
(`rust/crates/hc-wasm/Cargo.toml`, `rust/crates/hc-ffi/Cargo.toml`, `rust/crates/hc-cli/Cargo.toml`
— read directly this session; independently confirmed by `reports/02-established-fst-libraries.md`
§2.2 and `reports/03-parse-latency-profile.md`'s executive summary.)

`hc-wasm` (the browser demo) and `hc-ffi` (the native/FieldWorks bridge) call
`hc_parse::Morpher::parse_word_opts` directly (`rust/crates/hc-wasm/src/lib.rs:213,242`) — the
plain, search-based HermitCrab port. `hc-hybrid` is reachable only from `hc-cli`'s experimental
`fst-*` subcommands and its own test suite. The C# side agrees with this framing by design: "It is
a grammar-tuning instrument, not a production analyzer" (`docs/fst-plan/FST_FAST_PATH_PLAN.md:73`);
"Opt-in only. Never wired into `Morpher` or any default parsing path." (`FST_FAST_PATH_PLAN.md:80`).

**Consequence for this report:** the `<1ms`/`<10MB`/`<5s` product requirements in the task brief
apply to `hc-parse`/`hc-wasm`/`hc-ffi`, not to `hc-hybrid`. The "worst words ~100ms" figure in the
brief is almost certainly a plain-engine number (`reports/03` measures the plain engine's real tail
at p95 = 1.6s, p99 = 2.95s, max 4.7s on a partial Sena sample — far worse than 100ms, and
independently root-caused there). Dropping HermitCrab verification from `hc-hybrid` today would
change **zero** shipped behavior, because nothing shipped calls `hc-hybrid` in the first place. The
real, separate question — "should the *shipped* engine become FST-based" — is addressed in §7
below, briefly, because it's the one that actually matters for the product's latency budget; the
bulk of this report answers the question as asked (is the *existing* hybrid architecture's
verification step removable) because that is what determines whether `hc-hybrid` could ever
graduate to being the shipped engine.

---

## 1. Executive summary

- **FST-only is not safely feasible today**, for both correctness and performance reasons, and the
  gap is larger than "delete the verify call" — it requires *finishing* several compiler
  subsystems that do not exist yet, to a soundness bar nothing has tested them against.
- **What verification currently catches, precisely** (§2): metathesis rules (literally
  unimplemented — `compiler.rs` hard-codes every metathesis rule to `IdentitySkip`), clitics and
  process/simulfix morphs (no FST proposer exists for either — zero coverage from the FST alone),
  MPR features / allomorph environments / stem names (never checked while building trie arcs —
  confirmed by grep, zero hits, this session), α-variables (one representative binding probed, not
  enumerated — real cascades with true multi-binding agreement are approximated), and — the
  mathematical floor — unbounded reduplication (provably not a regular language; peeled outside the
  automaton, not compiled into it).
- **A lot is already compiled into the FST**, and it is not small: a shared unification-arc trie
  (547/18,871/6,672 states on Indonesian/Sena/Amharic — all **[measured]** this session, matching
  the doc's own figures for Indonesian/Sena), a bounded 2-root compound loop, junction-probed
  surface variants, a beam-capped NFA walk, and **two independent per-rule phonology compilers**
  (a fast, deliberately-buggy v1 merged automaton, and a general per-rule inverse-transducer chain
  that **provably recovers a real feeding/opacity cascade** — Indonesian's `meN-` assimilation +
  deletion — via lazy lockstep composition, not eager composition).
- **Where it would blow up, quantified:** the *provably* exponential/impossible constructs are
  narrow and already excluded from the automaton (unbounded copy, eager `lexicon ∘ rules`
  composition, materialized root×affix tables — all independently confirmed blowups, one still
  cited by number: 5s/45s at depth 2/3 on a 2,283-entry C# grammar). Most of what is *not* yet
  compiled in (MPR features, allomorph environments, stem names) is **linear**, not exponential —
  my own census found at most 72 affected allomorphs out of 1,702 on the largest grammar (Sena) —
  and was left undone for engineering-priority reasons, not because it is hard.
- **The one genuinely new, load-bearing finding of this investigation:** the performance risk in
  this system is **not concentrated in verification**. On Sena — the *easiest* of the three
  grammars (zero phonological rules, zero advisor escapes, "Tier 1 — fully FST-able") — the **bare
  trie walk alone**, with no sibling proposers and no verify step at all, measured a **median of
  27–30ms and a tail past 200–800ms per word** on a 200-word sample **[measured]**, driven by
  ordinary lexical/morphotactic ambiguity (hundreds of legitimate candidate segmentations per word)
  walked non-deterministically over a trie that is *deliberately never determinized* (determinizing
  across unification arcs would merge distinct analyses — `FST_FAST_PATH_PLAN.md:108`). This
  corroborates `reports/03`'s independent finding that propose and verify are roughly evenly split
  (52.6%/47.4% of aggregate time on the same 200-word sample). **Removing verify would remove at
  most half the cost and all of the correctness backstop.**
- **Verdict:** the hybrid is genuinely necessary in its current form. The verification step already
  *is* close to minimal (§2.3) — a single restricted re-analysis, one root + the candidate's own
  rules pinned, everything else (phonology, templates, strata) left open. The productive next step
  is not shrinking verification further; it's (a) finishing the missing compilers (§5) if FST-only
  coverage is ever wanted, and, independently, (b) fixing the walk's own cost (§6), which dropping
  verify does not touch at all.

---

## 2. What verification currently does

### 2.1 The mechanism: restricted re-analysis, not "double-checking"

`replay::confirm`/`confirm_checked` (`rust/crates/hc-hybrid/src/replay.rs:118-192`) takes one FST
candidate (a root + an ordered morpheme list) and runs the **real** `hc_parse::Morpher` on the word,
with two filters pinned (`replay.rs:169-177`, mirroring C#'s `FstReplay.cs:73-79`, confirmed
identical by `docs/fst-plan/F1_QUIRK_AUDIT.md` quirk 8):

```rust
let lex_entry_filter = |le: LexEntryId| le == root_entry || extra_roots.contains(&le);
let rule_filter = |r: RuleRef| match r {
    RuleRef::Stratum(_) | RuleRef::Template(_) => true,
    RuleRef::MRule(id) => rules.contains(&id)
        || (!extra_roots.is_empty() && matches!(g.mrules[id.0 as usize], MorphRuleDef::Compounding(_))),
};
```

Crucially, **phonological rules are never gated at all** — the Rust encoding of "always open" is
that `hc_rules::rewrite`/`metathesis` never consult `rule_filter` in the first place
(`replay.rs:33-38`). So verification is not "check the FST's phonology guess against the engine" —
it is "run the engine's *own, real* phonological cascade (feeding, bleeding, α-variable binding,
MPR gating, obligatoriness — everything) on the one candidate's root+rules, and see whether it
reproduces the same morpheme sequence." Because the filters can only **remove** search paths, never
fabricate one (`replay.rs`'s own doc, mirroring `HYBRID_FST_FEASIBILITY.md` §3.2 point 1), a
confirmed analysis is *definitionally* a real, complete HermitCrab analysis — soundness is
inherited from the engine, not reimplemented in the FST.

### 2.2 Inventory: what only verify catches (precisely, not from the docs' own claims)

| Construct | FST-side state today | Verified by code, this session |
|---|---|---|
| **Metathesis rules** | `compiler.rs`'s `compile_metathesis_stub` (`rust/crates/hc-hybrid/src/compiler.rs:129-150`) makes every `MetathesisRuleDef` an unconditional identity-only `IdentitySkip` — no swap logic exists at all. Confirmed by the module doc (`compiler.rs:6-17`) and independently by `KNOWN_GAPS.md` item 5. None of the three reference grammars declares one, so this is untested against anything real. | A grammar with a real metathesis rule gets **zero** correct candidates from any FST mechanism; 100% of that construct's coverage is verify (via engine fallback) or nothing. |
| **Clitics / process-simulfix morphs** (`MorphOp::Clitic`/`Process`) | No proposer exists for either (`README.md:89`, `KNOWN_GAPS.md`; `proposers.rs` has no clitic/process struct). | Words needing these fall to the engine entirely — not "verify catches an FST mistake," but "the FST proposes nothing at all." |
| **MPR features, allomorph environments, stem names** | Never consulted while building trie arcs. **Confirmed by grep this session**: `grep -n "required_mpr\|excluded_mpr\|\.environments\|stem_name" rust/crates/hc-hybrid/src/trie.rs` returns **zero hits**. | A candidate that violates an MPR co-occurrence rule, an allomorph environment, or a stem-name restriction is proposed anyway; only verify's real engine run (which does check these — `AnalysisAffixProcessRule.cs`-equivalent gating) rejects it. Documented as a "precision gap, not a soundness gap" (`FST_FAST_PATH_PLAN.md` Phase-4 construct sweep) — true, but it means every one of these checks is *currently implemented exactly once*, inside verify. |
| **α-variables in phonological rules** | `compiler.rs`'s probe machinery finds ONE representative feature-value binding per environment/target (`build_probe_representative`, `compiler.rs:396-409`) and reports the `"alpha-variable"` reason, tiering the rule `Permissive`, never enumerating other bindings (`F1_QUIRK_AUDIT.md` quirk 5, confirmed against `compiler.rs:348-354`). Indonesian's Nasal assimilation and Amharic's CV-merger rules are both affected — **[measured, this session]** Indonesian tier report: `Nasal assimilation Permissive alpha-variable`; Amharic: `Consonant-Vowel merger at morpheme boundaries IdentitySkip alpha-variable,no-effect`. | Any word whose correct analysis needs a *different* variable binding than the one probe representative chose is either missed by the FST (undercoverage, silently — "superset, never silent skip" is a design goal, not a proof) or produced with the wrong feature value, caught only because verify re-runs the real rule with real per-token bindings. |
| **Chain phonology on a *feeding/opacity cascade* — proven to work, but only on toy grammars** | The general per-rule chain (lazy lockstep composition, `walk.rs`'s chain half) demonstrably recovers a real two-rule feeding chain (`chain_recovers_two_rule_feeding_chain_mid_root` passed **[measured, this session — `cargo test -p hc-hybrid --release`]**) and long-distance harmony (`chain_recovers_long_distance_harmony_suffix_vowel_agrees_with_first_root_vowel`, also passed). But on the **real** Indonesian grammar, this mechanism is opt-in and not exercised by any of the three reference-grammar corpora going through a genuine multi-rule cascade end-to-end (confirmed: Indonesian's own real `meN-` cascade is closed via `SurfacePhonology` junction-baking, not via the chain — `HYBRID_FST_FEASIBILITY.md` §7 worked example). | Real-grammar coverage of interacting-rule cascades rests on junction-probing (a narrower, grammar-shape-specific mechanism, §3.2) plus verify, not on the general chain compiler, which is proven only in isolation. |
| **Unbounded reduplication** (`w → ww`) | Provably not regular (pumping lemma). Handled entirely outside the automaton by `ReduplicationProposer`'s O(n²) peel (`proposers.rs:85-98`), verify-gated like everything else. | This is not a coverage gap that verify "fixes" — it is a permanent, correct architectural choice; listed here for completeness, not as a criticism. |

**Net reading:** verification is not primarily "catching FST arithmetic mistakes on things the FST
mostly gets right." For a non-trivial slice of real HermitCrab constructs (metathesis, clitics,
process morphs, MPR/environment/stem-name gating, true multi-binding α-variables), verification is
the *only* mechanism that implements the construct's semantics at all today. Removing it does not
shrink a safety margin — it deletes functionality.

### 2.3 Verify is already close to minimal

One important, orthogonal point: verification is **not** a second unrestricted search. Pinning the
root and the candidate's own rules collapses HermitCrab's combinatorial fan-out to (in practice) a
single or near-single path — `HYBRID_FST_FEASIBILITY.md` §3.2 point 2's claim, and it is consistent
with what I measured: on Indonesian, propose+verify p50 = 149–225µs vs walk-only (propose alone)
p50 = 30–35µs **[measured, this session]** — verify adds low-hundreds-of-microseconds typically. On
Sena, `reports/03` independently measured verify at 47.4% of aggregate per-word time, with **no
work budget at all** (`replay.rs:107-110`'s doc: "C#'s verify has no work budget of its own
(feasibility report §10.7, an acknowledged open architectural gap)"). So the "minimal verification
step" the task asks about is close to what's already implemented: one root, the candidate's own
morphological rules, everything else open. The available lever is not "verify less" but "verify
cheaper" — `reports/03` item 4 (§5.2 there) proposes sharing one segmentation/memo scope across a
word's candidates during verify (`replay.rs:136-192` calls `Morpher::parse_word_selected` once per
candidate, each rebuilding a fresh `AnalysisScope` and re-segmenting the identical word,
`morpher.rs:265-397,291,354-355`) — a real, scoped, unattempted optimization, not a correctness
trade-off.

---

## 3. What's already compiled into the FST

| Layer | Mechanism | Where | Bound / cost |
|---|---|---|---|
| Concatenative morphotactics (lexicon, affixes, templates, derivation) | One shared, prefix-sharing unification-arc trie, built eagerly | `trie.rs` (1,152 lines) | Additive: states ≈ lexicon + affix inventory. **[measured]** 547 / 18,871 / 6,672 states (Indonesian/Sena/Amharic). |
| Compounding | A bounded 2-root loop, `build_compound_loop` (`trie.rs:859`) | `trie.rs` | Bounded at 2 roots by construction; a genuine graph **cycle** (not a DAG, contra an earlier, corrected claim — `HYBRID_FST_FEASIBILITY.md` §8.5), terminates only because every lap consumes ≥1 input segment, not because of acyclicity. |
| Boundary-conditioned surface variants (junction probing) | `SurfacePhonology::variants`/`deletion_junctions` — runs the **real** synthesis cascade once per affix at build time and bakes the observed surface forms as trie arcs | `surface.rs` | Nested `for c1 in alphabet { for c2 in alphabet {...} }` (`surface.rs:219-224`) — **literally O(alphabet²) per affix**. **[measured, this session]**: on Amharic (420-segment alphabet), this costs **~25s of a ~26s total trie build** (junction probing ON: 25,977ms; OFF: 1,013ms; **state count identical either way, 6,672** — every one of those 25 seconds found *zero* junctions). Indonesian (32-segment alphabet): 39.6ms ON vs 8.8ms OFF (+24 states, ~4.6%). Sena (43-segment alphabet, 0 phonological rules): 75ms vs 53ms. |
| Phonology, v1 (default, faster) | `PhonologyRuleCompiler`-equivalent merged single automaton, bug-for-bug (Segment-only alphabet — rejects any rule whose environment needs a boundary marker) | `compiler_v1.rs` | **[measured]** Indonesian: `unsupported_rule_count = 5` (all 5 real rules unsupported by v1's narrow shape — confirmed by the crate's own test `indonesian_v1_compiles_without_panicking_and_reports_unsupported_subrules`); Amharic: 6. |
| Phonology, general (opt-in, `useChainPhonology`) | Per-rule inverse transducers (`RuleInverseCompiler`-equivalent), walked in lockstep with the trie via a shared product-configuration walker (never eagerly composed) | `compiler.rs` (732 lines), `env_nfa.rs` (339 lines), `inverse.rs` (189 lines), `walk.rs`'s chain half | **[measured]** tier reports: Indonesian `Exact=2, Permissive=3, IdentitySkip=0`; Amharic `Exact=2, Permissive=4, IdentitySkip=1`; Sena `Exact=0, Permissive=0, IdentitySkip=0` (zero phonological rules) — all three byte-match the frozen C# goldens where goldens exist (Sena's `stats.txt` golden, confirmed this session: `rust/parity-out/golden/fst-advisor/sena/stats.txt:2` = `18871`, matching). |
| Environments (rule-internal, not allomorph-selection) | `env_nfa.rs` compiles bounded AND unbounded environments (`Quantifier{min,max:None}` → an NFA loop-back epsilon, `env_nfa.rs:154-159`) — this is exactly Kaplan & Kay's point: long-distance harmony is regular and gets a real, finite, loop-shaped NFA fragment, not an escape. **[measured, this session]**: `chain_recovers_long_distance_harmony_suffix_vowel_agrees_with_first_root_vowel` passes. | `env_nfa.rs` | Anchors (word-edge) are the one thing this layer cannot check (states carry no position) — dropped, reasoned `"anchor"`, tiers the rule `Permissive` (`env_nfa.rs:25-30`). |
| Non-regular copying (reduplication, infixation) | Runtime peel: strip the copy/infix, re-walk the residual through the trie, wrap the result | `proposers.rs` (668 lines) | O(word-length²) scan, ≤2 applications (a fixed cap, not derived from the grammar). |
| Beam control | A two-axis work-unit budget, latching on overflow, never a hang | `walk.rs:75-111` | `DEFAULT_MAX_BEAM_WORK = 1_000_000` (`walk.rs:75`), calibrated by a 3-point sweep on Sena per its own doc — a safety valve, not a performance target. |

**Total engineering surface**, measured this session: `hc-hybrid`'s `src/` is **7,918 lines across
16 modules** (`wc -l rust/crates/hc-hybrid/src/*.rs`), on top of the C# original it ports
(`HYBRID_FST_RUST_PLAN.md` §2.1's own inventory: ~7,300 new C# lines + ~7,000 test lines). This is a
second, independent, approximate encoding of HermitCrab's grammar semantics, not a thin wrapper —
and it must be kept a strict *superset* of the real engine's language by discipline, not by proof
(§5's "superset, never silent skip" contract). The nine documented "bug-for-bug" quirks in
`F1_QUIRK_AUDIT.md` (e.g. quirk 9: the exact arc-insertion order of a binary-search comparer with an
always-tied comparator had to be reverse-engineered to a closed form — `arc_insert_index`,
`trie.rs` — just to get candidate *emission order* byte-identical) are themselves a complexity
signal: a meaningful fraction of this system's engineering cost goes to matching an existing
implementation's incidental behavior precisely, which is orthogonal to grammar coverage.

---

## 4. Blowup analysis

### 4.1 Confirmed exponential / impossible constructs (excluded from the automaton, correctly)

| Construct | Why it blows up | Evidence |
|---|---|---|
| Eager `lexicon ∘ rule₁ ∘ … ∘ ruleₙ` composition | Multiplicative state growth; determinizing/minimizing across unification arcs (needed to keep it small) merges genuinely distinct analysis paths, destroying multi-analysis enumeration | "Eager composition without minimization was tried on this branch and exploded, exactly as theory predicts" (`HYBRID_FST_FEASIBILITY.md` §5.2) — **[doc claim]**, not independently re-run this session (the Rust port never implements this path at all — nothing to measure). |
| Materialized root×affix-permutation tables (`ForwardSynthesisProposer`) | Scales `roots × affixes^depth` | **[doc claim, with a specific number]**: "5 s build at depth 2, 45 s at depth 3 on a 2,283-entry grammar" (`FST_FAST_PATH_PLAN.md:101`). The Rust port never built this proposer at all (`KNOWN_GAPS.md` item 1) — it is a stub, opt-in flag with no effect — so there is nothing to independently re-measure; the C# number stands as the only evidence. |
| Phonology inversion over the bare surface *before* the morphotactic walk | Without morpheme-boundary arcs to gate them, boundary-conditioned rules fire everywhere | "the recorded `ⁿmeⁿnⁿpuⁿlis` garbage" (`FST_FAST_PATH_PLAN.md:105`) — **[doc claim]**; the fix (lockstep composition, keeping boundary markers on the shared tape) is what `hc-hybrid` actually implements. |
| Unbounded copy reduplication | Not a regular language at all (pumping lemma) — no FST of any size represents `{ww}` | Mathematical fact, not an engineering limitation (`HYBRID_FST_FEASIBILITY.md` §5.4). |

### 4.2 Confirmed costly-but-NOT-exponential (a real bug, quantified this session)

**The single largest quantified cost in this whole investigation is not combinatorial at all — it's
a wasted-work bug.** `SurfacePhonology::deletion_junctions` probes every `(c1, c2)` pair in the
grammar's *raw segment alphabet* (`surface.rs:219-224`), i.e. **O(alphabet²) per affix**, with no
feature-quotienting (bucketing segments that share every feature relevant to the rules that could
possibly delete something). On Amharic's 420-segment syllabary, this is 420² ≈ 176,400 probes per
affix, run once per rule-bearing grammar build:

| Grammar | Alphabet size | Junction probing ON | Junction probing OFF | States (ON) | States (OFF) |
|---|---|---|---|---|---|
| Indonesian | 32 | 39.6 ms | 8.8 ms | 547 | 523 |
| Sena | 43 | 75.4 ms | 52.8 ms | 18,871 | 18,871 |
| Amharic | 420 | **25,977 ms** | 1,013 ms | 6,672 | **6,672 (identical)** |

(All **[measured, this session]**, `--release`.) Amharic's state count is **byte-identical** with
junction probing on or off — every one of those ~25 extra seconds found nothing. This matches, and
quantifies more precisely than, the C# feasibility report's own §8.3 note ("~112 s, almost all of
it `DeletionJunctions` probes that found *zero* junctions"). **This is polynomial (quadratic in
alphabet size), not exponential, and it is avoidable by a well-understood fix already named in the
plan** (feature-quotient the probe alphabet — bucket by the features any deletion rule can actually
distinguish, `HYBRID_FST_FEASIBILITY.md` §8.3(b)) — it was left unfixed for engineering-priority
reasons (three queued mitigations, none executed — `HYBRID_FST_FEASIBILITY.md` §8.3), not because
it's hard. On a grammar with a large alphabet (any syllabary/abjad language, or any grammar sharing
one table across many natural classes), this alone would blow the product's `<5s build` budget by
5×, **and it has nothing to do with verification** — it is 100% build-time trie/junction-probing
cost, present whether or not you keep verify.

### 4.3 The build-time-ungated constructs: a real census, not a guess

The docs describe MPR features, allomorph environments, and stem names as "not build-time gated ...
a precision gap, not a soundness gap" (`FST_FAST_PATH_PLAN.md` Phase-4 sweep). I censused all three
grammars directly against the loaded `Grammar` object model this session:

| Grammar | Total allomorphs | MPR-gated affix allomorphs | Allomorph-environment-gated | Stem-named | MPR-gated lex entries |
|---|---|---|---|---|---|
| Indonesian | 79 | 0 | 0 | 0 | 4 |
| Sena | 1,702 | 0 | **72** | 0 | 0 |
| Amharic | 170 | 0 | 1 | 0 | 0 |

(**[measured, this session]**, via `g.mrules`/`g.entries` census over `AffixAllomorphDef`/
`RootAllomorphDef`'s `required_mpr`/`excluded_mpr`/`environments`/`stem_name` fields,
`hc-grammar/src/model.rs:414-415,624,648,656-658,764,770`.) Sena's 72 environment-gated allomorphs
(4.2% of 1,702) is the largest real number found across all three grammars — and building an extra
arc-guard per gated allomorph (check the neighboring morph's boundary segment against the
environment pattern, exactly the same machinery `env_nfa.rs` already uses for phonological rule
environments) is a **linear** cost in the number of gated allomorphs, not exponential. This is
squarely in the "avoided out of caution / engineering priority," not "would explode," bucket.

### 4.4 The new finding: the walk itself, not verification, is the dominant unresolved cost

This was not anticipated going in and is the report's central empirical result. I measured
propose-only (the bare FST walk, `walk::analyze_word`, no sibling proposers, no verify) against the
full propose+verify pipeline, on real word samples, `--release`:

| Grammar | Sample | Walk-only p50 | Walk-only p95 | Walk-only max | Propose+verify p50 | Propose+verify p95 | Propose+verify max |
|---|---|---|---|---|---|---|---|
| Indonesian | 121 words (full corpus) | 30–35 µs | 62–74 µs | 79–115 µs | 128–225 µs | 1.5–3.0 ms | 15–24 ms |
| Sena | 200 words (first 200 of 7,121) | **27.4 ms** | **208–425 ms** | **251–800 ms** | 37–46 ms | 557–648 ms | 741ms–2.16s |
| Amharic | 200 words (first 200 of 673) | 33–50 µs | 0.84–15.4 ms | 1.3–2.0 ms (per-run variance) | 33–50 µs | 12.9–19.4 ms | 1.04–1.21 s |

(All **[measured, this session]**, `--release`, via a temporary instrumented test deleted after
use.) On Sena — the grammar the advisor itself rates **Tier 1, zero escapes, "fully FST-able"**
(`advisor::analyze` output, this session: `"Tier 1 candidate — fully FST-able"`) — the **median**
word costs 27ms in the bare walk alone, with individual words (`"mphangwa"`, 589 legitimate
candidate analyses; `"kakamwe"`, which genuinely overflows the 1,000,000-work-unit beam budget)
taking 200–800ms. None of this is phonology, MPR gating, or verification — it is plain
morphotactic/lexical ambiguity (many roots and templates matching overlapping segmentations of an
ordinary Bantu verb form) explored by a **non-deterministic** ε-closure walk over an **18,871-state,
never-determinized, unification-arc** trie. This is not a fixable-by-dropping-verify problem,
because verify is not where the time goes: `reports/03-parse-latency-profile.md` independently
measured, on the same 200-word Sena sample, **propose 52.6% / verify 47.4%** of aggregate time — the
two are comparable, and propose is if anything the larger share. **Removing verify removes at most
half the cost and none of the correctness guarantee; the walk's own cost is untouched by this
report's question either way.**

Why the walk can't simply be sped up the same way a classical FST toolkit would: determinizing the
trie is explicitly forbidden by the architecture (`FST_FAST_PATH_PLAN.md:108`, `Determinize`/
`Minimize` across unification arcs "destroys multi-analysis enumeration") because HermitCrab
*analysis* is enumerate-all-parses, not accept/reject — the one optimization that would obviously
help (make the walk deterministic) is foreclosed by the product's own correctness requirement (every
valid segmentation must be recoverable, not just one). The plan permits determinizing the
plain-symbol *lexicon* layer specifically (`FST_FAST_PATH_PLAN.md:108`, "Determinizing the
plain-symbol lexicon trie layer is fine") — an unexploited, real lever, but distinct from the
question this report was asked to answer.

---

## 5. FST-only design sketch — what it would take, concretely

If one wanted `hc-hybrid` to answer authoritatively with no engine fallback (the closest reading of
"FST-only"), the gap is not "delete `replay.rs`." It is finishing, to full soundness, subsystems
that today either don't exist or are deliberately approximate:

1. **A real metathesis compiler.** `compiler.rs:129-150`'s stub would need the bounded-window-swap
   transducer the C# `RuleInverseCompiler.CompileMetathesisRule` implements (256-combination cap,
   per `HYBRID_FST_RUST_PLAN.md` §2.1's own C#-line-count table) — unstarted in Rust.
2. **Clitic and process/simulfix proposers.** No design exists yet beyond "extend `FstTemplateAnalyzer`
   arcs" / "compile like affix allomorphs with a modification transducer" (`HYBRID_FST_FEASIBILITY.md`
   §10.2) — these are sketches, not specs.
3. **Build-time MPR/environment/stem-name gating**, promoted from "sound because verify checks it"
   to "must be exactly right, because nothing else will." Per §4.3, the *engineering* cost is
   linear and small on today's grammars (≤72 allomorphs on the largest one) — but "small on the
   grammars we have" is not the same claim as "will stay small on every FieldWorks grammar," and
   the FST-only bar requires the latter.
4. **α-variable per-binding enumeration**, replacing the one-representative-binding probe
   (`compiler.rs:396-409`) with a real enumeration over the feature-equivalence classes a rule can
   distinguish — needed for both Indonesian's Nasal assimilation and Amharic's CV-merger to be
   *exact* rather than *Permissive* (approximate-but-verify-safe).
5. **True multi-rule cascade composition on real grammars, not just toy ones.** The mechanism is
   proven in principle (a real feeding/opacity chain recovers correctly via lazy lockstep
   composition, `LEVER_2.md`'s spike, reconfirmed by the Rust chain-walker tests this session) but
   has never been exercised end-to-end on a real grammar's actual multi-rule interaction — Indonesian's
   own `meN-` cascade is closed by junction-probing, a different, narrower, grammar-shape-specific
   mechanism, not by the general chain. Promoting the chain to authoritative-without-fallback status
   requires it to actually carry the real cascade, which is untested.
6. **A verify-side or walk-side termination/soundness argument that no longer has an engine
   backstop.** Today, if any of 1–5 has a bug, the failure mode is "this word falls to the engine or
   is reported as an FST gap" (visible, safe). Under FST-only, the same bug is a **silently wrong or
   silently missing analysis**, because there is no second opinion. This raises the correctness bar
   on every one of 1–5 categorically, not incrementally.

**A realistic size estimate for closing this gap**, extrapolating from the C# line counts already
recorded for comparable subsystems in this codebase (`RuleInverseCompiler.cs` 1,067 lines,
`GrammarFstAdvisor.cs` 614, `EnvNfaCompiler.cs` 191 — `HYBRID_FST_RUST_PLAN.md` §2.1): building
items 1, 2, and 4 to the same rigor as the existing per-rule compiler is **[estimate]** comparable in
size to the existing `compiler.rs` + `env_nfa.rs` + `proposers.rs` combined (roughly another
1,000–1,500 lines of new compiler logic, plus a proportional toy-grammar test suite, based on how
those existing modules scaled) — before any of it has been validated against a real grammar that
actually exercises it, which none of the three references does for metathesis/clitics/process.

---

## 6. Losses and risks if FST-only were adopted as-is (without closing §5)

- **Silent correctness loss on any grammar using metathesis, clitics, or process morphs** — not a
  degraded-but-safe fallback; those constructs get zero FST candidates today, so "no engine
  fallback" means "no analysis at all" for those words, full stop.
- **Silent wrongness on MPR/environment/stem-name-restricted forms** — today these are proposed and
  then correctly rejected by verify; without verify, they would need build-time gating (§5 item 3)
  or they'd be wrongly *accepted*.
- **Silent undercoverage on any word needing a non-representative α-variable binding** — today
  masked because verify would reject the FST's wrong-binding guess and the engine's real analysis,
  if reachable another way, still gets found by verify's re-run of the *real* rule; without verify,
  the FST's single-representative approximation is final.
- **No safety net for the walk's own performance cliff** — §4.4's finding holds regardless of the
  verify question; an FST-only deployment inherits Sena-scale worst-case latency (hundreds of ms to
  ~1s on ordinary, non-pathological words) with no engine escape hatch and, per the beam cap's own
  design, a small chance of *reporting no analysis* on legitimately parseable words that overflow
  the budget (`walk.rs`'s `BeamBudget`, latches on overflow — `"kakamwe"` did this in this session's
  Sena sample).
- **The one loss that is not actually a loss:** unbounded reduplication. This was never going to be
  "in the FST" under any design — it's mathematically excluded — so FST-only changes nothing here;
  the peel mechanism (already outside the automaton) stays exactly as it is either way.

Checking these losses against `docs/fst-plan/F1_QUIRK_AUDIT.md` and the conformance framework: none
of the three reference grammars (Indonesian, Sena, Amharic) exercises metathesis, clitics, or
process morphs, so **today's measured coverage numbers would look unchanged** if verify were
removed for these three grammars specifically — which is exactly the trap: the reference corpora
are not adversarial for this question, and a real FieldWorks grammar with any of these constructs
would silently regress the moment verify is removed.

---

## 7. The larger, unasked-but-relevant question: should the *shipped* engine become FST-based?

Out of this report's literal scope (which is about `hc-hybrid`'s internal verify step), but worth
stating plainly since it's very likely the underlying motivation: **no evidence gathered this
session supports replacing the shipped `hc-parse::Morpher` with an FST-only pipeline today.**
`hc-hybrid`'s own bare-walk cost (§4.4) is already, on a real grammar, comparable in its bad tail to
the plain engine's own bad tail that `reports/03` independently profiled (Sena p95 ≈ 620ms–1.6s for
the plain engine vs. 208–648ms for hybrid propose/propose+verify on the same corpus slice) — an
FST-based replacement would not obviously be faster on the words that currently hurt, and would
(per §6) be less correct on any grammar using several real HermitCrab constructs. If the product
goal is genuinely sub-millisecond production latency, the more promising, already-scoped levers are
the ones in `HYBRID_FST_RUST_PLAN.md` §12 items 6–7 (precache the top-N frequent word forms at
build time; a word→analysis-set cache, since analysis is a pure function of `(grammar, word)`) and
the plain-engine algorithmic fix `reports/03` identifies (the affix-matcher's handling of
"Optional-flooded" shapes) — neither of which is "go FST-only."

---

## 8. Verdict

**FST-only is not feasible today, and the reasons are not primarily about exponential blowup.**

- The provably-exponential/impossible constructs (eager composition, materialized forward-synthesis
  tables, unbounded copy) are already correctly excluded from the automaton — this part of the
  design is sound and does not need revisiting.
- What blocks FST-only is a mix of (a) genuinely unbuilt or approximate compiler subsystems
  (metathesis, clitics/process, α-variable enumeration, build-time MPR/environment/stem-name
  gating) whose correctness today depends entirely on verify's real-engine backstop, and (b) one
  quantified, fixable-but-unfixed polynomial cost bug (junction-probing's O(alphabet²) build-time
  blowup, up to ~26s on a 420-segment alphabet, contributing zero new states).
- **The genuinely surprising result of this investigation is that removing verification would not
  even solve the performance problem it might be assumed to solve**: on the easiest of the three
  real grammars, the bare trie walk alone already costs a 27ms median and an 800ms tail, and
  independent measurement (`reports/03`) puts propose and verify at roughly 53%/47% of total time —
  the walk, not verification, is the larger or comparable share, and it is architecturally
  resistant to the standard fix (determinization) because HermitCrab's analysis semantics require
  enumerating every valid segmentation, not just one.
- **The hybrid architecture is genuinely necessary in its current form**, and the verification step
  is already close to the minimal shape it could take: one restricted re-analysis per candidate,
  root + the candidate's own rules pinned, everything else (phonology, templates, strata) left
  open, no eager search anywhere. The productive next steps, if this system's speed matters (it
  currently doesn't, for the shipped product — §0), are (1) sharing the segmentation/memo scope
  across a word's verify candidates (`reports/03` item 4, concrete and unattempted), and (2) fixing
  the walk's own non-determinism-driven cost and the junction-probing quadratic-alphabet bug — not
  removing the engine backstop.

## Files referenced

- `rust/crates/hc-hybrid/src/replay.rs` (verify: `confirm`/`confirm_checked`, quirk-8 mapping)
- `rust/crates/hc-hybrid/src/compiler.rs` (general per-rule inverse compiler; metathesis stub)
- `rust/crates/hc-hybrid/src/compiler_v1.rs` (bug-for-bug v1 merged automaton)
- `rust/crates/hc-hybrid/src/composite.rs` (fixed proposer order + dedup)
- `rust/crates/hc-hybrid/src/proposers.rs` (reduplication/infix peels, composed/lockstep/chain phonology)
- `rust/crates/hc-hybrid/src/advisor.rs` (static tier/escape linter)
- `rust/crates/hc-hybrid/src/env_nfa.rs` (environment pattern → NFA fragment)
- `rust/crates/hc-hybrid/src/inverse.rs` (inverse-phonology transducer substrate)
- `rust/crates/hc-hybrid/src/trie.rs` (shared trie, compound loop, junction-skip arcs)
- `rust/crates/hc-hybrid/src/walk.rs` (bare + chain walkers, beam budget)
- `rust/crates/hc-hybrid/src/surface.rs` (junction probing, O(alphabet²) cost)
- `rust/crates/hc-hybrid/KNOWN_GAPS.md`, `rust/crates/hc-hybrid/README.md`
- `rust/crates/hc-wasm/Cargo.toml`, `rust/crates/hc-ffi/Cargo.toml`, `rust/crates/hc-cli/Cargo.toml`,
  `rust/crates/hc-wasm/src/lib.rs` (production dependency graph)
- `docs/fst-plan/HYBRID_FST_FEASIBILITY.md`, `FST_FAST_PATH_PLAN.md`, `HYBRID_FST_RUST_PLAN.md`,
  `LEVER_2.md`, `F1_QUIRK_AUDIT.md`, `HERMITCRAB_FST_ADVISOR.md`
- `rust/parity-out/golden/fst-advisor/sena/stats.txt` (the one real golden set present in this worktree)
- `reports/02-established-fst-libraries.md`, `reports/03-parse-latency-profile.md` (sibling investigations, cross-referenced)
