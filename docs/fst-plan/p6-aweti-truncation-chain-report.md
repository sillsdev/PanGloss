# P6-Aweti: chain restriction (shipped) + truncation semantics (designed, NOT shipped)

Follow-up to `p6-prototype-report.md` (§P6 items 1/2) and the P6-Aweti design investigation that
produced `dfb5025` (`emit_underlying_templated` + the Aweti gate). That investigation found: the
composed net is acyclic but has `PATHCOUNT_OVERFLOW`-scale finite ambiguity; true corpus recall
(composition-based) is 65/101 (equivalently 68/104 — see §4); 16/36 misses were *hypothesized* to
be missing truncation-drop semantics (41 "structural" mrules); 20/36 unexplained; `apply_up`
usable for some queries, effectively unbounded for others (`"ti"`).

This work investigated two mechanisms against that baseline. **Only one shipped.**

- **Chain restriction (§1): SHIPPED.** A dedicated-level-per-rule derivation chain under
  `TextMode::UnderlyingTokens` only. Real, verified win; no recall regression; `SurfaceProbed`
  byte-identical.
- **Truncation semantics (§2): DESIGNED, VALIDATED SOUND IN ISOLATION, NOT SHIPPED.** Its premise
  was refuted for Aweti (the 41 flagged rules are floating-consonant *realization*, not
  truncation); it earned **0/16** of the anticipated recall and it *regressed* `apply_up`
  usability, so it was stripped. The design and negative result are recorded here so a future
  grammar with genuine root-material truncation can revive the mechanism deliberately.

## 1. Chain restriction — SHIPPED, verified, no regression

`build_deriv_chain`'s legacy strategy: every level of a derivation chain offers every rule in the
zone's rule set (`rules.len()` levels, `DERIV_DEPTH_MIN` floor). A single rule's tag was therefore
choosable at ANY of those levels, independently each time — for Aweti's 11-rule prefix / 24-rule
suffix standalone sets, with each zone wired through TWO independent chain instances, a single
epsilon-yielding rule's tag was choosable up to 22x (prefix) / 48x (suffix) along one path. That is
the mechanism behind both the `PATHCOUNT_OVERFLOW` and `apply_up`'s effective non-termination on
some queries.

Fix shipped: under `TextMode::UnderlyingTokens` only (the `SurfaceProbed`/mainline `emit()` path is
completely unchanged — verified by the Indonesian/Sena/parity gates), `build_deriv_chain` now
assigns each rule its OWN dedicated level(s): `rule.max_apps()` consecutive levels, clamped to
`MAX_DEDICATED_LEVELS_PER_RULE = 4` defensively (every Aweti rule has `max_apps() == 1`, so this
cap never binds there), keeping the same epsilon skip-to-next-level arcs. Both chain instances per
zone keep the fix independently (rather than a full static split of which rule lives in which
instance — the extra 2x win was not worth the split's soundness risk over the already-achieved
22x/48x → ~2x reduction).

**Measured**: the composed net (lexc + 18-rule cascade + boundary cleanup) shrank from 35,846
states / 800,354 arcs to 14,806 states / 270,541 arcs. Compose-based recall is **unchanged, with a
byte-identical miss list**, before vs. after. `apply_up` on `"ti"` (previously: did not complete
even 500 raw results in 45s, required an external kill) now completes 2,000,000 raw results in
~2.1s. `"an"`/`"ti"` still do not surface their (compose-verified-reachable) oracle analysis within
that many raw results — an `apply_up` search-ordering gap distinct from language membership, not
fixed by this change and not pursued (documented, not hacked at).

RISK the design flagged (fixing standalone-rule relative surface order to document order might lose
a word needing the other order): did NOT materialize for Aweti — the miss list is byte-identical,
so the "acceptable weaker variant" fallback was never needed.

## 2. Truncation semantics — DESIGNED, validated sound, NOT SHIPPED (premise refuted for Aweti)

The prototype (in the now-discarded worktree) built a marker-token + generated-deletion-rule
truncation mechanism: allocate one marker codepoint per (rule, allomorph) pair that
`rhs_drops_lhs_material` flags, prepend/append it to the allomorph's underlying text, compile one
foma deletion-replace rule per marker (`pattern -> 0 || _ MARKER`), composed immediately after the
lexc net, with the marker added to the boundary-cleanup token set. Scope-verified against the live
grammar: every one of Aweti's 76 structural (rule, allomorph) pairs drops exactly one LHS part,
strictly before/after every copied part — squarely within the mechanism's designed scope, and it
rendered and composed correctly.

**And yet recall did not move by a single word — 0/16.** All 36 misses were byte-identical whether
or not the truncation cascade was composed in. Traced concretely (`"outaw"`, oracle chain
`[mrule4(prefix), root 8559b208, mrule1(suffix)]`): the root's stored allomorph text is `"uᵀ"` —
`"u"` (an ordinary vowel, RETAINED in the surface) plus `"ᵀ"` (U+1D40, one of three parallel
"floating consonant" markers this grammar declares). `mrule1`'s "dropped" LHS part is a
natural-class check on the segment PRECEDING that floating marker — used by the real synthesis
engine for allomorph SELECTION, not as material to delete. `Morpher::generate_words_from_analysis`
on this exact analysis returns `["outaw"]`: nothing is truncated from the root; `"ᵀ"` is resolved
to a concrete `"t"` by one of Aweti's own 18 phonological rules. This is floating-segment
phonological realization, not truncation.

`rhs_drops_lhs_material` was designed for, and is correct against,
`build_structural_composites`'s enumeration synthesis path (which drives the real `Copy`-selection
semantics). Reusing it as a proxy for "this templated-mode rule needs marker truncation" is a FALSE
POSITIVE for any rule whose "uncopied" LHS part is an environment/allomorph-selection check rather
than deleted material — empirically ALL 76 of Aweti's flagged pairs.

Worse, composing the (useless-here) truncation cascade regressed `apply_up` on `"parua"` from
instant to >280s. **Decision (per the repo's park-don't-merge-dead-code precedent, E5):** strip the
mechanism from the shipped change — it gains nothing for Aweti, actively regresses `apply_up`, and
its premise is refuted. The mechanism itself is sound and would help a grammar with genuine
root-material truncation (e.g. `edge-cases/truncate-morphotactic`'s `"sag"`→`"sa"`/`"ag"`);
validating it there is scheduled as a future Phase C synthetic recipe, not carried as dead code
here.

## 3. The deeper, still-unexplained gap: even a bare root can miss (OPEN)

Investigating the 20 "no structural rule needed" misses turned up something more fundamental than
truncation: `"mã"` — a BARE ROOT with ZERO affixes (single morpheme 400, root index 0) — also
fails the compose-based recall check, even with the ENTIRE 18-rule cascade removed from the
composition (lexc + boundary-cleanup only). The root's emitted lexc entry was verified byte-for-byte
to contain exactly the right tag (`<R:400>`) and token text (`"mã"`) on a direct `ROOT -> "#"`
accepting arc — the same mechanism every bare root uses, which works for ~2/3 of the corpus. The
restricted net for `"mã"` (lexc+cleanup only) is non-empty (144 states / 350 arcs), so this is not
an "empty language" issue; the specific tagged path for morpheme 400 simply is not appearing in
that reachable set as expected.

NOT root-caused within this scope. It requires a deeper dive into `fsm_lexc_parse_string`'s
handling of this entry shape, or the compose/minimize pipeline, or a bug in the compose-recall
methodology itself. Flagged as a genuinely separate workstream. It affects at least `"mã"` and
plausibly other simple misses in the 20-word bucket.

## 4. Recall gate: baseline reconciliation (65/101 vs. 68/104)

The original investigation's diagnostic excluded 3 hand-picked safety probe words
(`"parua"`/`"an"`/`"ti"`) from its own counters — all 3 are themselves recalled. The shipped gate
(`tests/p6_aweti_gate.rs`, `b_aweti_full_corpus_recall_via_compose`) counts every corpus word
uniformly: 101+3=104 words with an oracle analysis, 65+3=68 recalled — the SAME underlying result,
a more complete denominator. The gate asserts `n_recalled >= 68` (the achieved figure, not the
anticipated 84-equivalent) and separately asserts no previously-recalled word has regressed.

## Files changed (actual shipped set)

- `rust/crates/pg-foma/src/emit.rs` — `build_deriv_chain` dedicated-level-per-rule strategy
  (`TextMode::UnderlyingTokens` only); `MAX_DEDICATED_LEVELS_PER_RULE` constant. No truncation
  threading; `SurfaceProbed` path byte-identical.
- `rust/crates/pg-foma/tests/p6_aweti_gate.rs` — test (b) is the full-corpus composition-based
  recall gate (`n_recalled >= 68`, no-regression); test (c) is the `apply_up` `"parua"` spot-check
  (Morpher cap 20,000); test (a) unchanged.
- `rust/crates/pg-foma/examples/p6_aweti_q1_cycle_check.rs` — durable topsort/pathcount regression
  tool ("is the network still acyclic / how big is its language").
- `rust/crates/pg-foma/examples/p6_aweti_q3_oracle_bounds.rs` — durable oracle max-apps/repeat
  census (reference for the dedicated-level chain's `max_apps()` assumption).

**NOT shipped** (designed, validated sound, stripped — §2): `truncation.rs` and any truncation
marker threading through `emit.rs`. The prototype example composes the lexc + 18-rule cascade +
boundary cleanup only, no truncation.
