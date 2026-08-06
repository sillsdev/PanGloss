# Report 1: What did we actually do by hand? A catalogue of hand-spun FST optimisation techniques

Read-only research. No code was edited, no builds or tests were run, no git commands were run. This
document reverse-engineers what `pg-foma` **actually does today**, as opposed to what the many
`docs/fst-plan/*.md` planning documents describe wanting to do. Every claim is cited to a `file:line`
or an exact quote from a doc comment; where a plan document and the code disagree, both are cited and
the disagreement is stated.

## 0. The four grammars, confirmed from the repo

Contrary to any assumption from naming, the four hand-tuned reference grammars are **Amharic, Aweti,
Indonesian, Sena** — confirmed by the fixture files under `samples/data/`: `amharic-hc.xml`,
`amharic-realize.toml`, `amharic-worst-words.txt`; `aweti.fwdata`, `aweti.json`; `indonesian-hc.xml`,
`indonesian-realize.toml`; `sena-hc.xml`, `sena-words.txt`, `sena-worst-words.txt`. (The
"recipe-*-generic" fixtures named in some 2026-07-28 evidence documents are a *different* set — four
**synthetic promoted plan-shape fixtures**, explicitly distinguished from the four language corpora
in `docs/fst-plan/four-grammar-recipe-evidence-2026-07-28.md`'s own banner: "a naming caution: the
'four grammars' here are the four synthetic promoted plan-shape fixtures... they are not the four
language corpora.")

Rough shape, for calibration (from `docs/fst-plan/foma-fst-plan.md` and the P6/scale docs):

| Grammar | Entries/roots | Rules | Templates | Strata | Notably |
|---|---|---|---|---|---|
| Sena | ~1,369 | 0 phonological rules | 24 templates (→9 groups by shared category) | — | Largest lexicon, zero phonology; the recall-stress case is morphotactics, not junctions |
| Indonesian | ~66 roots | 5 phonological rules | 0 (`<AffixTemplate>`-free) | — | Small; the reference case for junction phonology (`meN-` nasal assimilation) and one MPR-gated subrule |
| Amharic | 76-87 entries | 7 phonological rules, up to 20 α-variables on one node | — | — | The reference case for interdigitation/infixation and POS-gated subrules |
| Aweti | 855 roots | 123 candidate rules (18 phonological), 3 strata | 14 templates | 3 | First "FLEx-scale" grammar hit; the reference case for enumeration blow-up |

## 1. THE CENTRAL FACT: two non-interoperating FST-construction pipelines, only one shipped by default

This must be stated before any per-technique catalogue, because it changes what "hand-spun for grammar
X" means for roughly half the techniques below. `docs/fst-plan/conformance-fst-measurement.md` (a
derivation-and-measurement report, itself the single most load-bearing document read for this audit)
establishes this by reading the source and by running the shipped binary:

- **What `pangloss batch|parse|fst-health|pack --engine=foma` actually compiles**: `FomaProposer::new`
  (and `new_with_budget_and_profile`) call **`emit::emit_with_budget_profiled`**, and nothing in
  `replace.rs`/`gate.rs::compile_and_compose_rules*`/`gate::compile_gated_grammar*` for phonological
  rule compilation. This is `emit.rs`/`junctions.rs`/`preexpand.rs`/`peel.rs` — call this **Path A
  (mainline)** below.
- **What `replace.rs`/`gate.rs`/`uflexc.rs`/`templated_compile.rs` build** — a genuine Kaplan & Kay
  rewrite-rule-to-automaton compiler — is reachable from a `pangloss` subcommand a user might actually
  run **only** via `pangloss recipe-optimize`'s `token-cascade-morphology` recipe
  (`recipe_registry.rs:712`, per `conformance-fst-measurement.md` §1/§6), and otherwise only from
  `pg-foma`'s own test suite and example drivers (`examples/p6_replace_prototype.rs`,
  `tests/p6_aweti_gate.rs`, `tests/cascade_vs_enumeration_experiment.rs` on a branch). Call this
  **Path B (prototype)** below.
- `replace.rs`'s own module-doc header states this outright (as read directly,
  `rust/crates/pg-foma/src/replace.rs:1`, paraphrased in the P6 prototype report): the module was
  written as a feasibility prototype and is not wired into the mainline emit/analyzer path.
- **Why this matters for a per-grammar reading**: `capability.rs`'s 19-`CharacteristicKind` taxonomy —
  the thing `pangloss fst-health`'s "capability" line reports — characterizes roughly half its
  variants (`RightToLeftRewrite`, `Metathesis`, `SimultaneousRewrite`, `QuantifierPattern`,
  `MultiTable`, half of `SubruleGating`) against **Path B**, not against what the shipped default
  engine does. `conformance-fst-measurement.md` §9 ran the actual binary on four open questions and
  found the mainline Path A independently gets several of these constructs right anyway — by a
  *different, cheaper-to-build, more confirm-work-heavy* mechanism (over-generate, let confirm prune)
  than what Path B would build. This audit tags every technique below **A** or **B** so it is never
  ambiguous which pipeline a given win/measurement describes.

Every technique in §2 is tagged `[A]` (shipped, what real `--engine=foma` runs use), `[B]` (prototype,
reachable only via `recipe-optimize`'s one recipe or test/example code), or `[A+B]` where the same
mechanism genuinely serves both.

---

## 2. Catalogue of techniques

### 2.1 `[A]` Lexc continuation-class morphotactics (the mainline compiler's spine)

**Mechanism.** `Grammar -> lexc source`: bare-root paths, a template-less derivation section, one
slot-chain per `<AffixTemplate>`, bounded-depth derivation layers, a bounded compound loop — a
faithful, upward-approximating port of the retired C# `hc-hybrid`'s trie (`rust/crates/pg-foma/src/
emit.rs:1-47`, the module's own "Structure: a faithful mirror of `hc-hybrid/src/trie.rs`'s
morphotactics" header).

**Trigger.** Universal — every grammar goes through this. No detection needed.

**What it bought.** Additive/linear cost in rule count and lexicon size (`emit.rs:1-47`); this is the
one family every measurement in this repo calls "cheap" and it is.

**Which grammars.** All four, unconditionally.

**What breaks without it.** Nothing to compare against — this is the base construction every other
technique below modifies or supplements.

---

### 2.2 `[A]` Per-template slot chains bounding zero-surface allomorphs to one tag per slot

**Mechanism.** Each template's slots classify prefix/suffix by `slot_op`, the prefix list reversed to
surface order, **each slot appears exactly once in its chain** (an optional slot adds an epsilon skip)
— replacing an earlier "depth-N bag" design that admitted a zero-surface allomorph at every level
(`emit.rs:17-23`).

**Trigger.** A grammar with `<AffixTemplate>` elements carrying slots that can realize a zero-surface
morph. Detectable directly from the grammar model (count of templates/slots is a static read).

**What it bought.** Explicitly measured and named in the module doc: the depth-N "bag" design this
replaced "overgenerated by six orders of magnitude on real Sena words — **2.5M candidates for `mbali`
vs the engine's 8**" (`emit.rs:17-23`, verbatim).

**Which grammars.** Sena is the grammar this fix is named after (24 templates). Indonesian/Amharic have
no/fewer templates (Indonesian: zero `<AffixTemplate>` elements, confirmed by
`docs/fst-plan/p6-prototype-report.md` §3). Inert (never triggers) on a template-less grammar.

**What breaks without it.** Combinatorial candidate blow-up (six orders of magnitude, measured) on any
templated grammar with zero-surface slot rules — not a refusal, a silent overgeneration disaster that
would drown confirm.

---

### 2.3 `[A]` Template grouping by shared `required_syn_fs`

**Mechanism.** Templates are grouped by their exact `required_syn_fs` `FsId` (identical authored POS
lists collapse to one id in the grammar's own interner) and share one root section per group; after
the shared root+derivation section, control joins a union of the group's templates' suffix-slot chains
(`emit.rs:50-58`).

**Trigger.** Multiple templates sharing a category — a static, computable count of distinct
`required_syn_fs` ids across all templates.

**What it bought.** Sena's 24 templates collapse to **9 groups** (`emit.rs:53`) — this is a lexc-size
reduction (avoids replicating root wiring per template the way the C# trie did, since lexc has no
graph-sharing primitive for that).

**Which grammars.** Sena (24→9 groups is Sena's own number). Indonesian/Amharic have too few templates
for this to matter (Indonesian: zero).

**What breaks without it.** Not a correctness issue — a documented *upward* approximation (a word can
combine template A's prefix slots with template B's suffix slots in the same category group, "more
paths than trie, never fewer," `emit.rs:56-58`) traded for avoiding per-template root replication.
Omitting the grouping would just replicate root text once per template (lexc-size cost, not a
correctness cost).

---

### 2.4 `[A]` Bounded derivation-layer depth = rule count (not a fixed constant)

**Mechanism.** Standalone (non-template) derivation-layer depth is the number of rules routed to that
side (floor `DERIV_DEPTH_MIN = 2`, `emit.rs:239`), not the C# trie's fixed `deriv_depth = 2`.

**Trigger.** A stratum whose standalone-rule count for one side exceeds 2 — a static count from the
grammar model, but the *soundness* of using "rule count" as the bound is a human judgement contingent
on `multipleApplication = 1` (each rule applies once, so no chain can exceed the rule count) — the
module doc names this explicitly: "a rule with `max_apps > 1` could still exceed it — none exists in
the reference grammars, and the corpus recall gate is the empirical backstop" (`emit.rs:24-32`).

**What it bought.** Found by the recall gate, not derived in advance: "Sena's corpus word `kubulukira`
stacks THREE derivational suffixes, REV + separado + APPLIC, which depth 2 silently loses" (`emit.rs:
24-32`, verbatim). This is a technique **discovered by measurement** (a failing recall gate on a real
corpus word), not by static reasoning about the construct.

**Which grammars.** Sena (the motivating case). Depth 2 would still have sufficed for Indonesian/
Amharic if their standalone-rule counts per side are ≤2 — not independently verified in this audit,
but the constant is grammar-independent so it costs nothing on grammars that don't need the extra
depth.

**What breaks without it.** Silent recall loss (an under-generation, the forbidden direction) on any
corpus word stacking more derivational suffixes than the fixed-2 bound — exactly `kubulukira`.

---

### 2.5 `[A]` Outer (post-template) derivation layers

**Mechanism.** Every template path is additionally wired through `OuterPfx`/`OuterSfx` chains — the
same rule sets as the inner (pre-suffix-slot) derivation layers — so both orderings (`[..root, ADD,
IND]` and `[..root, IND, ADD]`) exist (`emit.rs:38-44`).

**Trigger.** A grammar where a later-stratum standalone rule attaches *outside* an already-completed
template — not visible from a single-construct read of the grammar; the module doc states this was
"found by the recall gate": Sena's stratum-1 `=mbo` clitic lands AFTER the template's final-vowel
suffix slot (engine order `[.., root, IND, ADD]`), while the C# trie placed ALL strata's standalone
rules in the inner layer only (`emit.rs:38-44`).

**What it bought.** Unmeasured as a standalone number; qualitatively, this closes a positional-order
recall miss the C# trie itself did not have a construction for (an addition beyond trie's own
language, not merely a port of it).

**Which grammars.** Sena (again the corpus-word-driven fix). Discovered by measurement (a real corpus
word whose analysis has the "wrong" order relative to trie's assumption), not derivable from the
grammar's static shape alone without already knowing HermitCrab's own stratum-ordering semantics.

**What breaks without it.** A positional-order miss: the tag sequence a template-plus-outer-clitic word
actually has never appears as an emitted lexc path, so the true analysis is never proposed (silent
recall loss).

---

### 2.6 `[A]` Bounded compound loop (one extra root)

**Mechanism.** When any `CompoundingRule` exists, exactly one extra root may follow the head root, then
control passes to the suffix derivation layer — the same one-extra-root bound as the C# trie
(`emit.rs:45-47`).

**Trigger.** Presence of any `CompoundingRuleDef` — directly detectable, boolean.

**What it bought.** Unmeasured directly here; see §2.16-2.17 for the fuller compounding mechanism this
loop is part of.

**Which grammars.** Whichever of the four declares compounding rules (not independently confirmed in
this audit which of the four does; the conformance-corpus derivation doc's compounding family analysis
draws on synthetic fixtures, not the four named grammars specifically).

---

### 2.7 `[A]` Bare-root compile-time discharge (bound single-allomorph root omission)

**Mechanism.** Every root allomorph is normally admitted as a standalone bare word (a lexc entry whose
continuation is the accept state `#`) — a deliberate over-generation the C# trie's own
`bare_root_surfaces` check would have gated but this emitter cannot (that check needs a live
`Morpher`). One narrow sub-case is provably dead at compile time: an entry with **exactly one
allomorph** that is `is_bound` can never produce a valid bare-root candidate, because confirm's own
`allomorphs_valid_impl` root arm rejects `is_bound && distinct_count == 1` unconditionally, and a
bare-root candidate is *by construction* `distinct_count == 1`
(`docs/fst-plan/bare-root-compile-time-discharge.md`, `RootRec::never_valid_bare`,
`bare_admissible_roots`).

**Trigger.** `entry.allomorphs.len() == 1 && allomorph.is_bound` — fully static, directly readable off
`RootAllomorphDef`, no live `Morpher` needed. Automatically detectable, precisely because that is the
whole point of the fix (discharging a case that used to need runtime information).

**What it bought.** "Zero new lexc states, zero new flag diacritics — this removes lines... it does
not add any machinery" (the doc's own framing). Measured on a synthetic fixture only: one bound,
single-allomorph root's bare `"#"`-continuation line disappears; `EmitCounts::bare_root_arcs_pruned ==
1`. **Measured as a no-op on every real fixture**: "No reference or edge-case fixture in
`machine/conformance/` or `conformance-staging/` declares `isBound="true"` on any allomorph today" —
confirmed by grep across both trees, and the private Sena corpus this could have been measured against
is absent from the worktree that did this work. So this technique is implemented and verified correct,
but its real-world payoff on the four named grammars is **unmeasured / likely zero** (no grammar in the
current corpus uses `isBound`).

**Which grammars.** Unverified which, if any, of the four uses `isBound="true"`; the doc's own search
found none in the public corpora.

**What breaks without it.** Nothing breaks — recall is provably unaffected either way (confirm already
pruned this arc); the only cost of omitting the fix is a few extra dead lexc lines per bound root.

---

### 2.8 `[A]` Surface-variant cartesian product for multi-representation segments

**Mechanism.** `surface_variants` re-segments a root/affix's authored text against the surface char-def
table (the loader's own greedy-longest-match algorithm) and drops `Boundary`-kind matches; where a
matched char-def has multiple `<Representation>`s (Sena's `char4` = {"m","n"}), it returns the
**cartesian product** of every matched segment's representations — every spelling the engine would
accept (`emit.rs:80-100`).

**Trigger.** A char-def table declaring more than one `<Representation>` for some segment — statically
detectable (count of `<Representation>` children per `<SegmentDefinition>`).

**What it bought.** Named directly in the module doc as the fix for a specific recall miss: `"tun"` (the
authored shape of Sena's *mentir*) must also match corpus `"tum..."` — "found as **13 of the first
recall gate's 19 misses**" (`emit.rs:91-95`, verbatim) — this is a technique discovered by a failing
recall gate, then generalized.

**Which grammars.** Sena (the motivating and only multi-representation case named: `char4`). Indonesian
has an analogous case (`char28` = {"g","G"}, per `docs/fst-plan/p6-prototype-report.md` §1) handled the
same way in this pipeline.

**What breaks without it.** Silent recall loss on any corpus word whose surface uses a non-first
representation of a multi-spelling char-def.

**Guard.** Bounded by `REP_VARIANT_CAP = 64` (`emit.rs:246`); overflow is reported as an uncovered item
rather than silently dropped, "never triggered by Sena" per the module doc.

---

### 2.9 `[A]` NFD normalization alignment (query and lexc text share one normalization space)

**Mechanism.** `kept_surface_text` NFD-normalizes before matching, mirroring the real engine's own NFD
matching, and `crate::analyzer` NFD-normalizes its own query word the same way before `apply_up`
(`emit.rs:102-107`).

**Trigger.** Universal — any grammar whose corpus file happens to be NFC on disk needs this to avoid a
silent mismatch; not conditional on a grammar construct at all, but on encoding hygiene of the input
text.

**What it bought.** Unmeasured as a number, but this is exactly the class of bug named in
`docs/fst-plan/mpr-overwrite-encoding-research.md` as "the same *class* of bug" as the NFD-combining-
mark issue `tests/f5_diacritics_gate.rs` documents, and `docs/fst-plan/p6-prototype-report.md` §2.3
independently found an adjacent-non-ASCII-codepoint tokenization bug in the vendored `foma` lexer.

**Which grammars.** Amharic (Ge'ez combining marks) is the most likely beneficiary; not independently
measured per-grammar here.

**What breaks without it.** Silent zero-parse on any word whose on-disk normalization form differs from
the lexc-compiled form — a real, previously-hit bug class per the citations above.

---

### 2.10 `[A]` Junction-aware affix/root emission via `PhonologyProbe` (bounded ±1-neighbor probe)

**Mechanism.** For a grammar with real junction phonology, `crate::junctions::PhonologyProbe` (built
once per grammar, `None` for a grammar with zero phonological rules — "a true no-op for Sena,
preserving that gate byte-for-byte," `emit.rs:113-122`) drives the **real synthesis engine**
(`pg_rules::surface_probe::probe_synthesize` — the identical machinery `confirm` uses) over a
**bounded local window**: an affix's underlying insert text alone, or with exactly one alphabet
representative neighbor on either side. Every discovered surface spelling and deletion-junction
outcome is baked into literal lexc string alternatives.

**Trigger.** Presence of `PhonologicalRule`s in the grammar. Directly detectable (boolean: any
phonological rule at all). The *locality* boundary of what this can fix — does the phenomenon fit
inside a ±1-segment window — is **not** statically detectable from a construct count; it is a fact
about a specific rule's environment width that has to be checked case by case. `docs/fst-plan/
conformance-fst-measurement.md` §9's refined finding: the real fidelity boundary is not a fixed
segment-count window at all, but "whether the phenomenon needs to see material that lives in more than
one morpheme's own text at once" — a genuinely cross-morpheme environment (provably blind past
one-neighbor scope) versus material fully contained within one morpheme's own text (a bare root's
internal phonology, seen in full regardless of how many segments it spans, because for a bare root the
"window" the real oracle sees is already the complete word). This is a subtle, non-obvious distinction
that a naive "count the environment width" detector would get wrong.

**What it bought.** The load-bearing mechanism behind Indonesian's headline result: `meN+tulis ->
menulis` (`emit.rs:109-127`). Timing: foma 8×-48× faster than the full engine per-grammar
(`foma-fst-plan.md` §P3), though that number reflects the whole propose+confirm pipeline, not this
mechanism alone.

**Which grammars.** Indonesian is the grammar this is written for and named after in the module doc.
Amharic and Aweti also have phonological rules and use this mechanism; Sena (zero phonological rules)
gets a true no-op (`PhonologyProbe::new` returns `None`).

**What breaks without it.** Every real Indonesian corpus word requiring `meN-` assimilation/deletion
would be silently under-generated (no literal lexc spelling could ever match the true surface).

---

### 2.11 `[A]` Deletion-junction encoding: per-prefix-variant root partitions (`{name}Stripped` lexicons)

**Mechanism.** Rather than gating deletion-skip edges on a live FeatureStruct unification test the way
the C# trie's shared-graph trie did (`hc-hybrid` needed this because its root chains are shared graph
structure across every affix), this emitter — since lexc root text is written out per-section, not
linked as one shared graph — instead gives every root-adjacent chain's final level a `{name}Stripped`
sibling lexicon holding every root's own `stripped_variants` (root text with its first *segment*
removed), and routes every `deletion_junctions` hit there instead of to the ordinary exit
(`emit.rs:129-149`).

**Trigger.** A chain whose final level is genuinely root-adjacent (statically detectable: `next ==
exit` is a roots lexicon).

**What it bought.** Unmeasured as a standalone number (bundled into the junction-probe win above);
explicitly documented as "deliberately UNGATED by onset class — every root gets a stripped entry
regardless of whether its own initial segment would really delete... an explicit upward approximation:
the extra candidate is harmless (confirm prunes it)" (`emit.rs:143-149`) — a **human judgement call**
trading proposer looseness for avoiding a data dependency (per-junction neighbor-class lane data) this
emitter has no other use for.

**Which grammars.** Indonesian (the `meN-` deletion case is the citation).

**What breaks without it.** Would either need the trie's original neighbor-class unification data
(not otherwise present in this emitter) or lose the deletion-junction candidates entirely (recall
loss). The chosen fix trades exactness for simplicity, safely, because confirm absorbs the cost.

---

### 2.12 `[A]` Rule-application pre-expansion for interdigitation and boundary fusion (`preexpand.rs`)

**Mechanism.** For a rule whose surface effect cannot be expressed as a two-entry (root, then-continue)
lexc encoding — interior insertion interleaved with copied root material (`Role::Infix`: Amharic's
`-pfv-`/`-conv-`, e.g. root "ውልድ" + `-pfv-` → "ውäልäድ", with no cuttable boundary), or ordinary
prefix/suffix adjacency that coalesces into a differently-spelled glyph (Ge'ez boundary fusion: "ልጅ" +
"+ዮች" → "ልጆች", never literal "ልጅዮች") — this module seeds a real `pg_rules::word::Word` from a root
allomorph's own feature-bearing shape, applies the **real rule** via `pg_rules::morph::synthesize` (the
engine's own synthesis function, not a re-implementation), runs the **real phonological cascade** via
`pg_rules::surface_probe::probe_synthesize`, and — where the resulting true surface differs from a
naive pre-phonology rendering — emits ONE lexc entry carrying BOTH tags, in the engine's own computed
morph order (replayed via `morph_order_tags`, mirroring `Morpher::allomorphs_in_morph_order` exactly,
`preexpand.rs:1-45`).

**Trigger.** `Role::Infix` rules (always non-literal, "there is no non-interleaved literal to even
compare against" — deterministic), or `Role::Prefix`/`Role::Suffix` rules whose adjacency to a
*specific root's* glyph coalesces (only *sometimes* true — has to be discovered per (root, rule) pair
by actually running the real synthesis/phonology, not statically predictable from the rule's
declaration alone). The **trigger for running this expensive path at all** (`is_structural_rule`/
`should_run`) is a static, cheap check — any `Infix` role or any phonological rule present. The
**outcome per (root, rule) pair** is not statically knowable; it requires actually invoking the real
engine.

**What it bought.** Amharic (release build): depth-0 alone is 6,612 raw (root, rule) combinations
pre-filtered to 1,389 by a cheap `required_syn_fs` unifiability test; with depth-3 chaining, ~305k
pairs probed, yielding **2,930 interdigitation + 51,023 fusion composite entries** in ~30-47s of emit
wall time — "the dominant emit cost" (`preexpand.rs:53-56`). Indonesian (real phonology, no infix
rules, no coalescence) probes 457 pairs and emits **zero** composites (`preexpand.rs:60`). Sena (zero
phonological rules, zero infix rules) computes zero pairs — `should_run` short-circuits before touching
a single entry, keeping Sena's lexc byte-for-byte unchanged (`preexpand.rs:56-59`). This is also the
mechanism that closed "the two structural miss classes the P1c investigation found on Amharic (32/32
misses classified, no third class)" — a discovery-by-measurement provenance, not a from-first-
principles design.

**A real, prior bug this stage caught**: using the loader's own feature-less shape directly (rather
than re-segmenting with features the way real lexical lookup does) made "0/76 roots match[] any of the
three infix rules until the fix, 36/76 after" (`preexpand.rs:20-24`) — a concrete case of a subtle
implementation bug that would have silently zeroed out recall for the entire construct family had it
not been caught.

**Which grammars.** Amharic (the motivating and headline case). Indonesian is exercised (zero effect).
Sena and Aweti's real preexpand cost is discussed under §2.13 below (Aweti is where this SAME mechanism
becomes the blow-up).

**What breaks without it.** Every interdigitating or fusing surface would be unreachable by any literal
lexc entry — a hard recall loss, not merely a slowdown; this is not an optional optimization but a
required capability bridge.

---

### 2.13 `[A]` The Aweti blow-up: same mechanism, `O(roots × rules^depth)`, no scale guard originally

**Mechanism.** Identical to §2.12 (`preexpand::extend`) plus its sibling `emit::struct_extend`/
`build_structural_composites` for truncation/circumfix/probe-refusal composites — both recursively
chain **every** candidate rule onto every root at every depth (≤3), gated only by the cheap
`required_syn_fs` pre-filter, consulting neither `grammar.templates` nor stratum rule-order at all
(`docs/fst-plan/morphotactic-composite-pruning.md`, "Problem").

**Trigger.** A grammar large enough, with enough candidate rules, that `roots × rules^depth` at
depth-3 is intractable. This IS statically predictable in principle from `roots × rules` counts, but
the actual practical trigger is subtler: Aweti "looked ordinary on all three [pre-flight] signals"
(`docs/fst-plan/morphotactic-composite-pruning.md`, "Aweti end-to-end result" section) — the
`composite_scale_hint` pre-flight predictor did **NOT** predict this explosion in advance. So the
naive count-based trigger *exists* but was empirically shown **not sufficient** as a detector; the real
disaster predictor turned out to be the *emitted entry count during the run*, not the input-size
counts before it (see §2.15).

**What it bought (measured, negative).** Aweti (855 roots, all fusion-class, zero infix rules, against
47 fusion-eligible rules): `build_composites` OOMed past 4.9GB RSS without finishing pre-fix. After the
pruning fix (§2.14), the recursion completes in ~551s, but the emitted network is still "691,184,759
bytes (9,720,129 lines)" of lexc, **2,833,559 fusion entries + 230,476 structural entries** — "~124x
Amharic's" fusion-entry count (`docs/fst-plan/morphotactic-composite-pruning.md`, "Aweti end-to-end
result"). `FomaAnalyzer::new` (emit+compile) completes at ~774s wall (~1.2-1.3GB peak RSS) but
`analyze_word`'s very first corpus-word `propose_candidates` call then grows RSS unboundedly (1.2GB →
34GB before failing) — a crash on the first real query, every time, deterministically.

**Which grammars.** Aweti exclusively among the four — none of Sena/Indonesian/Amharic come close to
this shape (Amharic's "16x smaller at depth 0 alone," per the same doc's "Problem" section).

**What breaks without a fix.** A total crash: the grammar cannot be used via `--engine=foma batch` at
all — not a slow grammar, an unusable one.

---

### 2.14 `[A]` Morphotactic pruning: subset-construction restriction of composite chains to engine-legal adjacencies

**Mechanism.** A new module, `crate::morphotactics`, builds — once per grammar — a subset-construction
automaton (`MorphotacticIndex`) over the engine's own real morphotactics (strata fold in document
order; loose rules run Linear-or-Unordered; template slots apply only inside a template application,
in ascending slot order, with a strict "vacuous-rule-skip" rule for mandatory-but-surface-empty slots;
template entry gated by `is_unifiable` + `!root_is_partial`) — and both `preexpand::extend` and
`emit::struct_extend` consult a single `MorphotacticIndex::next_state` query immediately before
recursing on a candidate rule, restricting the flat recursion to a strict subset of engine-legal chains
(`rust/crates/pg-foma/src/morphotactics.rs:1-59`).

**Trigger.** Any grammar running the depth≤3 composite recursion at all (§2.12/§2.13's own trigger) —
this is a universal *guard* applied unconditionally to that path, not a separately-detected construct.
The subtle correctness point (a human judgement, carefully argued, not a mechanical derivation) is
`rule_may_be_vacuous`'s STRICT exact-copy-in-order test for when a mandatory slot may be jumped —
deliberately narrower than a looser, unsound version tried in a throwaway example
(`morphotactics.rs:41-53`).

**What it bought (measured).** Amharic A/B (`morphotactics.rs`/`docs/fst-plan/
morphotactic-composite-pruning.md`, "Final verification"):

| | pairs_probed | wall time | composite entries |
|---|---|---|---|
| Flat (pre-fix) | 305,621 | 7.34s | 134,539 |
| Pruned (production) | 104,605 | 1.06s | 61,029 |
| Shrink | **2.92×** | **6.9×** | pruned ⊆ flat (verified: zero pruned entries missing from flat) |

The doc's own framing: this realized shrink is *below* the plan's own static upper-bound projection
(up to 3.9×) — "pruning's real payoff is bounded by how much of the flat recursion the DYNAMIC filters
... were already cutting before this change, which for Amharic was substantial." Static reasoning
alone over-predicted the win; the real number needed measurement.

**Which grammars.** Amharic is the only grammar this A/B was measured against directly. Sena/Indonesian
are provably unaffected (`should_run` is `false`, byte-for-byte unchanged, per the same doc's "Final
verification" table). Aweti: the pruning fixed the *original OOM crash* (§2.13's `build_composites`
now completes in bounded time) but — critically — **did not fix the end-to-end usability problem**:
"pruning is necessary but not sufficient" — the emitted network is still unusably large (§2.13's
numbers). This is a technique that is a strict, verified improvement everywhere, but insufficient by
itself for the one grammar it was built to rescue.

**What breaks without it.** The Aweti OOM crash (unbounded recursion exploring engine-illegal rule
orders the real synthesis path could never produce).

---

### 2.15 `[A]` `EnumerationBudget`: default-on, fail-fast refusal on the enumeration path

**Mechanism.** A shared, cross-thread `AtomicUsize`-latched budget (rayon-parallel per-root workers
share it) tracking two cumulative counts during `preexpand::build_composites_with_mode` +
`emit::build_structural_composites`: (1) composite lexc entries emitted (fusion+interdigitation+
structural combined — the primary, disaster-predicting measure), and (2) (root, rule) pairs probed (a
lower-hit-rate backstop). Crossing either latches a shared flag every recursive call checks at entry,
so enumeration is aborted **during** the recursion, not measured after the fact
(`docs/fst-plan/morphotactic-composite-pruning.md`, "Addendum: Fix 1").

**Trigger.** Universal guard, default-**ON** in production (unlike its diagnostic-only sibling
`ProbeBudget`, which panics and is off by default) — applied unconditionally to every grammar on the
enumeration path, no per-grammar detection needed for the guard itself to be active.

**What it bought (measured, and the key finding: pure probe-count is not the right predictor).** "A
pure 'pairs probed' cap... does NOT catch Aweti early enough: Aweti probes 'only' ~8.37 million pairs
— large, but not obviously catastrophic in isolation — before its composite-entry count explodes. The
number that actually predicts the disaster is the RESULT of those probes: composite lexc entries
emitted" (verbatim). This is a genuine, measurement-driven design correction: the first-instinct metric
(search-tree size) was insufficient; the entry-count metric was found by actually watching Aweti
explode. `DEFAULT_ENTRY_BUDGET = 200_000` (Amharic's real 22,775 fusion entries sit **~8.8× under** it;
Aweti's 2,833,559 crosses it after "roughly 7%" of its own enumeration). `DEFAULT_PROBE_BUDGET =
3_000_000` (Amharic ~305k pairs, ~9.8× margin; Aweti ~8.37M, over the cap). Tripping on Aweti with
production defaults costs "on the order of a couple of seconds," not 551s.

**Which grammars.** Calibrated directly against Amharic (the "keep working" reference) and Aweti (the
"must refuse honestly, fast" reference). Sena/Indonesian never approach either threshold.

**What breaks without it.** Exactly §2.13's disaster: 13 minutes of emission followed by a crash on the
first query, with no typed error at any point.

---

### 2.16 `[A]` Compounding: MPR-bitset lexicon partition + bounded-depth chain

**Mechanism.** `compound_license` computes `head_eligible`/`non_head_eligible` lexicon subsets by an
`O(N × rules × subrules)` set-membership scan against every `CompoundingRuleDef`'s MPR-feature gates
(bitset overlap, `O(1)` per test). `build_compound_chain` then emits a bounded-depth chain of lexc
continuation classes — one `{base}{k}Roots` section per level, restricted to the license-filtered
subset, continuing to `exit` or a next-level dispatcher — "explicitly documented as linear... never
exponential in emitted TEXT size" (`docs/fst-plan/conformance-fst-measurement.md` §5, citing
`emit.rs:1600-1603`).

**Trigger.** Presence of any `CompoundingRuleDef` — directly detectable, boolean.

**What it bought.** `O(N × depth_budget)` in the emitted artifact size — linear in both lexicon size
and unrolled depth, representing a combinatorially large *accepted language* (`N ×
non_head_count^levels`) with a linear-sized graph — "exactly what a continuation-class DAG is for."

**Which grammars.** Unverified which of the four names a `CompoundingRuleDef`; the derivation was
confirmed against the general mechanism and a synthetic stress fixture
(`recursive-endocentric-compounding`), not against a specifically-named one of the four language
grammars in this audit's own sources.

**What breaks without it.** Without the depth cap, a naively-recursive compound would unroll to
whatever the grammar's own recursion structure implies — unbounded for a genuinely recursive
compounding rule.

---

### 2.17 `[A]` Computed (not guessed) compounding depth cap

**Mechanism.** `compounding_max_depth(r) = 1 + max_apps(r) + Σ max_apps(ancestors)`
(`capability.rs:1442`), checked against `DEFAULT_COMPOUND_CHAIN_DEPTH_BUDGET = 200` (`emit.rs:286` per
this audit's direct read of the constant) before any lexc is written.

**Trigger.** `CompoundingRuleDef::multipleApplication` — directly detectable/computable from the
grammar model, no runtime information needed.

**What it bought.** For a synthetic stress fixture with `multipleApplication="9"`: `max_depth = 10`,
comfortably under 200 (`docs/fst-plan/conformance-fst-measurement.md` §5) — but that fixture's own
`words.yaml` "only actually exercises depths 0/1/(barely)2," so the depth-200 case is unverified by any
real corpus word, only by a synthetic unit test (a 60,000-`multipleApplication` grammar).

**Which grammars.** Same caveat as §2.16 — a general mechanism, exercised concretely only by a
synthetic fixture in the material read for this audit, not confirmed per-name against one of the four
grammars.

**What breaks without it.** SCC-style unbounded recursion for a genuinely recursive compounding
construct (the one cyclic construct this codebase's design catalogue names — resolved by this depth
bound rather than by SCC condensation, per `docs/fst-plan/grammar-optimization-techniques.md` C5).

---

### 2.18 `[A]` `ReduplicationPeeler`: never compiled into the FST at all

**Mechanism.** Four `O(word length)` string scans (prefix-copy, suffix-copy, separator+tail-copy,
separator+suffix-peel), a fresh port of the C# `ReduplicationProposer`, run at query time (not compile
time) and unioned into `propose`'s own candidate stream — the recursion target swapped from the C#
trie-based bare walker to the caller's foma proposer, since "reduplication peeling is
proposer-agnostic — it only needs a `fn(&str) -> Vec<Candidate>` to recurse residuals into"
(`peel.rs:1-12`).

**Trigger.** `classify_affix` routing a rule to `Role::Reduplication` — a static, per-rule label. This
is the one construct family in the whole survey provably **non-regular** by design (unbounded-copy
reduplication is not a regular relation), so it is architecturally excluded from the FST rather than
approximated inside it — `docs/fst-plan/conformance-fst-measurement.md` §4 calls this "the one
construction in this whole survey that is genuinely N-independent... it passes the report's own
'categorically simpler' test outright."

**What it bought.** `O(word length)` per query, independent of lexicon size N — the cheapest possible
mechanism for this construct family, and the only one that could be exact rather than approximated,
since a true `{ww}`-shaped copy is not finite-state.

**Which grammars.** Indonesian (`docs/fst-plan/foma-fst-plan.md`'s own D6/P2 sections, "reduplication
peel" — 7 Indonesian corpus words are reduplicated: `membagi-bagi`, `memijit-mijit`, etc., all recalled
correctly per the P3 timing table). Sena's own reduplicated corpus words (73 hyphenated lines per
`docs/fst-plan/corpus-word-list-hazards.md`) are a separate word-list-hygiene issue, not evidence about
the peeler itself. Amharic/Aweti reduplication use is not independently confirmed here.

**What breaks without it.** A true recall gap for any reduplicated word — not approximable by any
finite-state construction, so there is no fallback-inside-the-FST option; it must be a runtime peel or
nothing.

**A boundary this mechanism has, found by measurement/inspection, not by design**: a circumfix-*and*-
reduplication-combined shape is deliberately routed AWAY from the peel (which cannot recall a
wrap-both-sides shape — each of its four scans is one-sided) and into the expensive `O(roots ×
rules^depth)` enumeration path (§2.12) instead — "a genuinely uncharacterized construct... found here"
per `docs/fst-plan/conformance-fst-measurement.md` §4, Family A, `circumfix-reduplication-precedence`.
This is real cross-technique routing logic, not incidental.

---

### 2.19 `[A]` `classify_affix` role-classification precedence (and three staged bug fixes to it)

**Mechanism.** `classify_affix` is a pure, static, `O(|RHS|)`-per-allomorph label computation testing
leading/trailing-insert (→ `CircumfixPrefix`) BEFORE reduplication-shape BEFORE interior-action (→
`Infix`) — this ordering decides which of §2.12/§2.18/§2.16's mechanisms handles a given rule
(`docs/fst-plan/conformance-fst-measurement.md` §4, Family A).

**Trigger.** The shape of a rule's RHS action list — fully static and detectable, but the *correct
precedence order* among competing classifications required three separate, staged bug fixes found by
conformance fixtures, each pinning one specific misclassification: (1) `circumfix-non-first-allomorph-
selection` — a `CircumfixPrefix` allomorph declared at index ≥1 was unreachable because the admission
test only consulted allomorph 0 (a genuine recall gap, closed by scanning every allomorph); (2)
`circumfix-infix-interior-action-precedence` — a simultaneously-circumfixing-and-infixing RHS used to
misroute to the wrong builder, but "recall was never actually lost here" because both builders call the
identical real-engine resynthesis (an ownership fix, not a recall fix — verified empirically by
reverting and re-running); (3) `circumfix-reduplication-precedence` — the SAME misroute for a
circumfixing-and-reduplicating RHS WAS a real recall gap (the peel cannot recall a wrap-both-sides
shape), fixed by routing to the enumeration path instead.

**What it bought.** Closes real, conformance-fixture-pinned recall gaps; case (2) shows that not every
apparent bug in this area is actually a recall bug — an important methodological point (some
misclassifications only change which of two equally-correct mechanisms handles a case).

**Which grammars.** These three fixes are pinned by synthetic conformance fixtures, not the four named
language grammars directly, though the underlying constructs (circumfix, infix, reduplication) are
exactly what Amharic/Indonesian's own grammars exercise.

---

### 2.20 `[A/B]` The boundary-token cleanup bug and its precise fix (`reroute_null_shaped_affix_chains` + `finish_controllable_net`, ~94–425× regression discovered and fixed)

**Mechanism/history.** A DIFFERENT build path (`recipe_runtime::evaluate_plans` → `build::
build_controllable` → `build::finish_controllable_net`) composed a single, context-free
"delete every `Boundary`-kind char-def" regex onto the whole network after lexc emission. Sena's char
table declares THREE distinct `Boundary` kinds (an ordinary morph separator `+`; a genuine
**null/zero-morph marker family** `^0`/`*0`/`&0`/`∅`; another separator `.`) — blanket-deleting all
three erased exactly the adjacency information that used to make most continuation-class combinations
structurally unreachable, converting "required, uniquely-identifying transitions into free/epsilon-
like branches" (`docs/fst-plan/large-lexicon-proposal-explosion.md`).

**The precise root cause and fix, per direct read of `build.rs` (correcting/sharpening the diagnosis
doc's own framing).** `uflexc`'s prefix/suffix continuation lexicons are deliberately self-looping (to
allow arbitrary affix stacking) — harmless for an ordinary affix, which always consumes ≥1 real
character and so bounds recursion by query length. It is NOT harmless for an affix allomorph whose
ENTIRE underlying shape is composed only of `Boundary`-kind characters: **Sena's own compounding
allomorph `"^0+"` (7 occurrences, all identical)** is exactly this shape. Once the boundary-cleanup net
deletes every character of that allomorph's spelling, its lexc line degenerates to a zero-width,
epsilon-tagged entry sitting ON the self-loop — a free, unboundedly-repeatable insertion of that
morpheme's tag, taken any number of times without consuming any surface text; `apply_up` enumerates
every distinct accepting upper-tape string, so it multiplies out every repeat count up to its own
internal search bound (`build.rs:203-213`). The shipped fix, `reroute_null_shaped_affix_chains`,
detects any lexc line whose lower-tape text is entirely `Boundary`-kind characters and reroutes it off
the self-looping continuation onto a one-shot, non-reentrant successor — while duplicating every
*ordinary* line into a parallel `*NoNull` continuation so real affixes can still stack freely both
before and after the (at-most-once) null marker (`build.rs:189-287`).

**A documented two-iteration design history, not a one-shot fix.** A first version routed a null-shaped
line straight to `RootBare`/`#` (no further prefixes/suffixes allowed afterward at all) — "TOO narrow,"
caught by its own regression test failing `MultiplicityMismatch { word: "ps", expected: 3, actual: 2 }`
because a real prefix and the null prefix can legitimately combine in either order, and routing straight
to the bare-accept state silently dropped whichever order took the null prefix first (`build.rs:228-244`).

**A documented, NAMED, still-open scope gap (the same defect class recurring in a newer code path).** The
fix's own `match` recognizes only the two literal lexicon names (`PrefixChain`/`SuffixChain`) that
existed when it was written; `uflexc`'s later-added bounded-compound-loop introduced its OWN per-level
self-looping lexicons (`UCmpPfx0`, `UCmp2Pfx0`, ...) that recreate the identical hazard, invisible to
this name-based guard — "a name-based guard cannot defend a lexicon that did not exist when the guard
was written" (`build.rs:270-287`). The real fix for THOSE lexicons was moved to `uflexc`'s own
emission-time discipline (outside these three files, not verified directly in this audit); this
function is now "belt-and-braces for the compound levels rather than their only defence," pinned by a
dedicated regression test (`null_shaped_guard_scope_tests::reroute_is_a_no_op_on_the_compound_loop_lexicons`).

**Trigger.** A grammar whose char table declares a semantically load-bearing `Boundary` kind (a
null-morph marker, not merely a cosmetic separator) — this is the "b" root-cause the doc names
precisely, and it is statically visible in the char-def table's `kind="boundary"` declarations, but was
not caught until measured. Sena's own analysis names the specific morphological trigger: "words whose
morphology crosses a null-morph-marker boundary at a productive slot — Bantu nasal-class
prefixation... trigger it; words with no such juncture... do not."

**What it bought (measured, before/after the fix `9cb569f`/`build::reroute_null_shaped_affix_chains`,
per the doc's own superseded-banner update).** Direct A/B on 5 words: production (`emit::emit`, Path A)
= 127 total proposals; the buggy build path = **53,992** proposals — a **425×** blow-up, with `mbali`
alone at **516×** (104 → 53,720). Post-fix (per the doc's own banner): **575** proposals on the same
5-word slice — a **~94×** reduction from the pre-fix 53,992.

**Which grammars.** Sena exclusively (the null-morph-marker char-def family this bug targeted). The
doc's own recommendation (§"Recommendation") explicitly cites that the *mainline* `emit.rs` path
(`emit::emit`) never has this problem, because it never puts boundary tokens on the queryable surface
tape at all — "enumerate representation variants at emit/`uflexc` time... rather than emitting them
into the network and hoping a post-hoc compose-time deletion is semantically safe" — i.e. §2.8/§2.11's
own mainline strategy is cited as the reference-correct alternative design this buggy path should have
copied from the start.

**What breaks without it.** A ~425× proposal-count blow-up on ordinary, short, unambiguous words —
discovered because it made `pangloss recipe-optimize` on Sena look catastrophically slow, when the
underlying grammar ambiguity had not actually changed.

---

### 2.21 `[A]` Static, flag-free lexical partition for MPR/POS-gated PHONOLOGICAL subrules (`gate.rs`)

**Mechanism.** A `RewriteSubruleDef` carrying `requiredPartsOfSpeech`/`requiredMPRFeatures`/
`excludedMPRFeatures` used to be compiled as unconditional. The fix: scan every rule's subrules for a
nontrivial gate (`gate::find_gated_subrules`); for every lexical entry, compute the vector of booleans
"is this gated subrule applicable" by calling `pg_rules::rewrite::subrule_applicable` **directly** —
the same predicate the real engine's own trailing-prule cascade calls, widened from private to `pub`
for exactly this caller, "so the partition can never disagree with confirm about which entries are
gated" (`gate.rs:1-6`, `docs/fst-plan/p6-prototype-report.md` §7.2); partition all entries by that key;
compile ONE network per group with each group's inapplicable subrules' regex text simply never
rendered (arc omission, not a filter automaton); union the per-group networks — safe because the groups
are lexically disjoint by construction (an ordinary disjoint-language union, distinct from the union
hazard in §2.24).

**Trigger.** A `RewriteSubruleDef` with a nontrivial POS/MPR gate — directly, statically detectable
from the grammar model. The *whole point* of this technique's genesis, though, is that the OBVIOUS
encoding for this trigger (flag diacritics) does not work — see below.

**What it bought.** Recall-critical, not merely cost-saving: on Indonesian's real `prule5`
(`excludedMPRFeatures="mpr1"`), the un-gated (pre-existing) cascade run against an augmented grammar
"produces ZERO candidates for a word the real engine accepts — a genuine proposer recall loss, not
merely over-generation" (`docs/fst-plan/p6-prototype-report.md` §7.3, the `mentabur` row). The gated
network recovers it exactly. Amharic: `gate::find_gated_subrules` finds exactly `prule1`/`prule2`/
`prule3`, and `partition_entries` splits 76 entries into 3 groups (2/10/64) without crashing, with the
UNTOUCHED ungated compile path reproducing byte-identical numbers (82 states, 1,110,358 arcs) — proving
the gate change doesn't disturb the ungated path.

**Which grammars.** Indonesian (real, corpus-exercised MPR exclusion — though the real corpus
structurally cannot reach the critical juncture, so this had to be tested via 2 synthetic augmentation
entries, `tanam`/`tabur`, built to the same shape as two real corpus roots `tulis`/`pukul`). Amharic
(real POS gating, `prule1`/`prule2`/`prule3`, though end-to-end corpus recall for this is NOT gated —
Amharic's templated morphotactics are beyond this prototype's `uflexc` emitter's scope, so only the
compile-only path and a hand-authored equivalent fixture verify the mechanism). **This is Path `[B]`
(prototype)** for the *rule compiler* half, but the *gating decision itself* (`gate.rs`'s partition
logic) is reused by the mainline `--engine=foma`'s actual behavior in a different, cheaper way — see
next entry.

**What breaks without it (in Path B).** The `mentabur` recall loss shown above.

**What the MAINLINE (Path A) engine actually does instead, per direct measurement**: `docs/fst-plan/
conformance-fst-measurement.md` §9 Q4 ran `pangloss batch mpr-gated-exception/grammar.xml ... --engine=
foma` and found the shipped default engine gets the MPR-excluded word right too, but by
**over-generation, not gating**: `fst-health` showed "9 candidates proposed, 8 confirmed, 11.1%
rejection share" — propose offers the excluded candidate anyway; confirm alone rejects it. **Zero
propose-side MPR filtering exists in the mainline path at all** (grepped `emit.rs`/`preexpand.rs`/
`morphotactics.rs`: no hits outside the compounding block). So the *correctness* of MPR gating on real
`--engine=foma` traffic rests entirely on confirm, not on `gate.rs`'s partition — `gate.rs`'s partition
is real, tested, and used only via the recipe-optimizer's one recipe.

---

### 2.22 `[B]` Documented dead end: flag diacritics for phonological subrule gating (three independent toolkit defects)

**Mechanism attempted.** Set `@P.MPR1.1@` on an excluded root's lexc entry; test `@D.MPR1@`/
`@R.POS.<sym>@` in the gated subrule's own environment — the textbook two-level-morphology technique
for exactly this problem (`gate.rs:1-49`).

**Why it failed, in the vendored `foma = "=0.4.2"` crate, bisected empirically (three separate,
independently confirmed defects):**
1. **A flag literal inside a replace rule's own `||` context corrupts the compiled network.**
   `t -> 0 || a "@D.MPR1@" _` compiles cleanly but `apply_up`/`apply_down` return a NONDETERMINISTIC
   mix of "fired"/"didn't fire" for the same input, regardless of whether the flag was ever set — even
   the vacuous-pass case is wrong. A context that is JUST a flag literal additionally **crashed**
   (`STATUS_STACK_BUFFER_OVERRUN` inside `vendor/foma/src/minimize.rs`) on `apply_up`. `f0_viability.rs`
   F0.3 and `pk2_eliminate_flag_oracle.rs` only ever test flags OUTSIDE a `->` construct, so this gap
   was real and previously unexercised (`gate.rs:13-23`).
2. **`fsm_compose` does not treat flag symbols as epsilon-transparent by default**
   (`FomaOptions::default().flag_is_epsilon == false`, `foma-0.4.2/src/options.rs:83`) — a flag-bearing
   net composed with a flag-free one, with the flag never even set, returns EMPTY, not the vacuous-pass
   answer both nets give alone. `flag_is_epsilon = true` fixes this specific case but does NOT fix
   defect 1 (`gate.rs:24-33`).
3. **A Kleene-star "shadow the trigger char if flagged" workaround** (built to route around defect 1 by
   keeping flags out of any `->` construct) is itself order-fragile: a flag must be set strictly
   BEFORE the tape position it is tested at; a first draft appended the flag AFTER the gated segment
   (mirroring a different, unrelated module's lookahead convention) and silently gated nothing;
   prepending fixed that half, but the construction then gave wrong answers once composed with a real
   lexc net (right in isolation, wrong once real entries were substituted), root cause not fully
   isolated before the team called off the approach (`gate.rs:34-49`).

**Verdict.** "Three toolkit surprises deep on one technique was treated as the signal to stop, not keep
debugging blind" (`gate.rs`'s own doc). This was independently re-confirmed a second time, later, for a
DIFFERENT construct (`MprGroup::Overwrite`) by `docs/fst-plan/mpr-overwrite-encoding-research.md`'s own
fresh probes — including extending the finding to `fsm_intersect` specifically (which has "no
flag-awareness of any kind... it does not know flags exist as a category at all") and finding that
`flag_eliminate`'s theoretically-sound compile-time elimination is real but structurally inapplicable
exactly where this project would need it (at `->` replace rules, per defect 1). **Genuinely closed, not
merely unproven** — this is one of the few techniques in this catalogue with a hard, empirically-
demonstrated negative result, confirmed twice, independently, against two different constructs.

**Which grammars.** The obstruction is general (a vendored-toolkit defect, not grammar-specific), but
the one real usage across the three original reference grammars sits exactly at the shape defect 1
breaks (a `->` replace-rule context) — so this closes the door on the "obvious" fix for Indonesian's
`prule5`/Amharic's `prule1-3`.

---

### 2.23 `[B]` `RepresentationAliasMap`: multi-table shared-representation aliasing

**Mechanism.** For a multi-table grammar, renders a rule's own atom as the union of every table's token
for the same normalized spelling, folded into that rule's regex source text before one compile —
closing a real false-negative risk (table B's rule silently never firing on table-A-spelled material)
that a naive per-table-disjoint construction would miss (`docs/fst-plan/conformance-fst-measurement.md`
§8).

**Trigger.** A grammar declaring more than one `<CharacterDefinitionTable>` whose segment sets overlap
in normalized spelling — statically detectable (table count > 1, plus an overlap computation:
`O(table_count² × avg_table_size)`, "purely a function of the (small, fixed) combined character
inventory — never lexicon size N").

**What it bought.** Closes a documented reversal in the capability model's own risk assessment: "a
shared representation used to `Refuse` (treated as a false-positive risk), but tracing the actual
failure mode showed the real risk runs the other way (a false NEGATIVE)."

**Which grammars.** None of the three reference grammars this audit found evidence for (Indonesian,
Amharic, Aweti each declare exactly one `<CharacterDefinitionTable>` per `docs/fst-plan/
p6-prototype-report.md` §6 item 1). Sena's table count is unconfirmed here. This is genuinely untested
against any of the four named grammars — a Path-B mechanism built for a construct none of the four
currently exercises.

**What breaks without it.** A real correctness gap for a hypothetical multi-table grammar (false
negative: a rule that should fire on table-B-spelled input silently never does).

**Real mainline (Path A) multi-table correctness rides on something ELSE entirely**: `pg-rules`'s own
independent per-rule table resolution (`owning_table_for_prule`/`_metathesis_rule`/`_allomorph`,
`pg-rules/src/cache.rs`) plus an `emit.rs` fix to `collect_roots` (resolve each stratum's own table
fresh, not one fixed table argument) — real, used by both engines, but architecturally unrelated to
`RepresentationAliasMap`.

---

### 2.24 `[B]` Kaplan & Kay rewrite-rule cascade compiler (`replace.rs`): the "real" compiler underneath the prototype path

**Mechanism.** `RewriteRuleDef → foma xre regex → compiled Fsm`, via `fsm_compose`-folded stratum-order
composition (never `fsm_union`, at three nested levels: rule cascade, subrule fold, alpha-tuple fold —
`docs/fst-plan/conformance-fst-measurement.md` §3). A genuine Kaplan-Kay automaton compiler, not an
approximation bridge.

**Trigger.** Universal for this pipeline once a phonological rule exists — `compile_templated_
morphotactics` requires ≥1 rule (`rules_in_order` empty ⇒ `NoCompiledRules`, per `docs/fst-plan/
cascade-vs-enumeration-experiment.md`).

**What it bought.** Where recall holds (§2.28's caveat), the composed network is often *smaller* than
Path A's enumeration output: on `mpr-gated-exception` (a public conformance fixture), Path B's compiled
network is 25 states/32 arcs vs. Path A's 29/60 (`docs/fst-plan/cascade-vs-enumeration-experiment.md`).
On Amharic's 20-α-variable rules, the full 7-rule cascade compiles+composes in **2.14s** to an
82-state/1,110,358-arc net — "no crash, no OOM — a sharp contrast with `emit.rs`'s enumeration path"
for the same grammar (`docs/fst-plan/p6-prototype-report.md` §5.1). On Aweti's 18 phonological rules
(no morphotactics involved, rules only): **28.8ms**, 30 states/2,143 arcs — "answers the task's Aweti
question for the rule-compilation half of P6... no scale problem at all — the enumeration-based
emitter's OOM was never about the RULES" (§5.2). This is the strongest positive evidence in the whole
audit that a *different construction* (composition, not per-root enumeration) genuinely avoids §2.13's
blow-up class — but see §2.28 for why this is not (yet) a safe drop-in replacement.

**Which grammars.** Feasibility-proven at Indonesian scale (100% recall, `p6-prototype-report.md` §4),
Amharic scale (compile-only, no end-to-end corpus recall check — Amharic's templated morphotactics are
out of the minimal `uflexc` emitter's scope), and Aweti's rule-cascade-only scale (no morphotactics
involved at all in that probe).

**Two real toolkit findings on the way to this working (measurement-discovered, not designed for)**:
(1) the vendored xre grammar's comma operator accepts multiple environments for one shared rule, or
multiple bare rules with no context — but REJECTS two full clauses joined by comma, so the naive
"comma-join every branch" plan failed to compile at all; (2) combining tuple branches via `fsm_union`
compiles and runs but is **semantically wrong** — each per-tuple net is a complete replace transducer,
identity elsewhere, and unioning N of them reintroduces a spurious "did nothing" path at positions some
OTHER tuple's context should have owned — caught empirically (`apply_down` returned both the correct
and a spurious path), fixed by switching to sequential `fsm_compose` (sound because the tuples' contexts
are mutually exclusive by the joint-agreement filter's own construction). Measured before/after:
392,311 states/6,892,003 arcs (the union blow-up) → 38 states/401 arcs after the fix
(`p6-prototype-report.md` §2.2).

---

### 2.25 `[B]` PUA token alphabet: char-def identity, not literal spelling (`SegAlphabet`)

**Mechanism.** Every `CharDefId` in a grammar's surface table maps to one Private-Use-Area codepoint
(`SegAlphabet::token`, `PUA_BASE = 0xE000`); every lexc entry, rule regex, and query word is
built/encoded in that token space (`replace.rs:1-24`).

**Trigger.** Universal within Path B — this is the alphabet strategy for the WHOLE prototype pipeline,
not conditional on any specific grammar construct.

**What it bought.** Sidesteps, structurally rather than case-by-case, three footguns literal-string
lexc (Path A) has to work around one at a time: multi-representation segments need no cartesian product
(§2.8's mechanism becomes unnecessary — same char-def id, same token, automatically); multi-character
graphemes need no `Multichar_Symbols` bookkeeping between separately-compiled lexc and rule nets; the
morpheme-boundary `+` (a reserved xre Kleene-plus operator) never collides with a token. The cost: "the
composed network's own lower tape is not human-legible" — acceptable because the propose→confirm
contract only needs the upper tape's tag sequence.

**Which grammars.** Universal to Path B; not a per-grammar-triggered technique.

**A related bug found by this design, load-bearing and general**: the vendored `nfst-xre` lexer does
not reliably treat two adjacent PUA codepoints written back-to-back with no separator as two
independent atoms — silent mis-tokenization, no parse error, "just a rule that never fires." Fixed by
space-separating every rendered pattern-node piece unconditionally; this single fix "took Indonesian's
recall from 72/97 to 97/97" (`p6-prototype-report.md` §2.3) — a large, silent recall bug, found only by
building something non-trivial and bisecting the failure, not by reading the toolkit's documentation
(ASCII inputs tolerate bare concatenation fine; the gap is specific to non-ASCII/high-codepoint
symbols, exactly what this alphabet strategy is built from).

---

### 2.26 `[B]` Tuple-indexed α-variable resolution (generic over N variables)

**Mechanism.** `resolve_alpha_tuples` gathers every alpha-bound occurrence across a subrule's LHS/RHS/
environments (one occurrence may bind MULTIPLE variables — Amharic's CV-merger binds up to 20 on a
single node), enumerates the cross product of each occurrence's own candidate set, then filters to
combinations where every pair of same-`VarId` occurrences agrees at that variable's feature lane
(`replace.rs:33-43`).

**Trigger.** Any `AlphaVariable`, `polarity="plus"` occurrence — directly detectable (count of
alpha-bound pattern nodes). `polarity="minus"` ("disagree") is unimplemented — zero occurrences in any
reference grammar, so this gap is currently inert everywhere, but not detected/reported specially if
it ever did occur (the rule would be reported uncovered).

**What it bought (measured, the cleanest empirical validation of this whole cost model).** Indonesian's
`prule4` (1 variable, 2 occurrences): raw product 75 → 14 survivors. Amharic's `prule6`/`prule7`
CV-mergers (20 variables, up to 20 occurrences on one node): raw product **121,776** → **312**
survivors, matching reports/08's own predicted bound (nc15=59 × nc16=6 ⇒ ≤354) closely (`p6-prototype-
report.md` §5.1). "This is the prototype's cleanest empirical validation of the entire tuple-indexed
cost model the P6 architecture rests on: a naive per-variable expander (v^20, or even the raw
121,776-tuple product) would be the thing that actually explodes; the joint-agreement filter — the
SAME generic code that resolved Indonesian's one-variable case — collapses it to 312 without any
Amharic-specific logic."

**Which grammars.** Indonesian (`prule4`) and Amharic (`prule6`/`prule7`) are the two real, measured
cases; the mechanism was designed to generalize and was validated (not merely asserted) to do so across
a 20× scale jump in variable count with zero special-casing.

**Guard.** `DEFAULT_TUPLE_BUDGET = 5_000` (`compose_budget.rs:98`), checked BEFORE the per-tuple compile
loop; Amharic's real worst case (312) sits ~14× under it.

---

### 2.27 `[B]` RTL rewrite, metathesis, and Simultaneous-overlap: three independently-argued union/refusal safety cases

**Mechanism (RTL).** `compile_rtl_branch_net` builds the mirror rule (reversed LHS/RHS, swapped/
reversed environments), compiles it normally, `fsm_reverse`s the result (state renumbering only — tape
sides never swap), then `union_checked`s it with the plain net — argued safe because each branch is a
complete replace transducer with no spurious "elsewhere" escape (`docs/fst-plan/
conformance-fst-measurement.md` §3).

**Mechanism (metathesis).** Every candidate slot assignment rendered as one fully-literal branch,
unioned — the SAME "complete transducer, no spurious identity path" argument, not the sequential-
compose argument RTL/α-tuples use.

**Mechanism (Simultaneous-overlap).** `SimultaneousSubruleOverlapPredicate` intersects each subrule
pair's lowered `(left_language, focus_right_language)` automata (`crate::lower::spans_overlap`). If
provably disjoint, the rule compiles via the ORDINARY sequential-compose machinery, unchanged — no
separate "simultaneous" construction exists at all. If spans genuinely overlap, the rule is **refused
outright**, never compiled at any cost.

**Trigger.** `Dir::RightToLeft` (statically detectable, boolean); `MetathesisRuleDef` (statically
detectable); `RewriteMode::Simultaneous` with overlapping subrule spans — this last one requires an
actual automaton-intersection computation (`lower.rs::spans_overlap`), not a shallow static check, but
it IS a computable, deterministic predicate over the grammar alone (no runtime data needed).

**What it bought.** `Slot::Anchor` needs zero anchor-specific code for RTL — "position alone (leading
vs. trailing) carries the meaning, so reversal flips it automatically" (pinned by a fixture's own
negative controls). None of the three reference grammars exercise RTL, genuine metathesis, or
overlapping Simultaneous subrules at all (`replace.rs`'s own doc calls the RTL branch "DEAD for every
reference grammar's own rule").

**Which grammars.** None of Indonesian/Amharic/Aweti's 5+7+18 rules use any of these three
(`p6-prototype-report.md` §3's rule-semantics table, "Real gap, never triggered"). This is real, tested
Path-B machinery for constructs that are **entirely unexercised by the four named grammars** — the
strongest example in this catalogue of a technique built for future/unseen grammars rather than the
ones on hand.

**Determinism, corrected from a naive framing.** Three DIFFERENT, independently-argued safety cases for
the three places a union actually occurs in this family — the doc is explicit these should not be
conflated into one "unions are risky" story (RTL/metathesis: complete-transducer safety; `gate.rs`:
lexically-disjoint-language safety; Simultaneous-overlap: no union at all, either the ordinary cascade
or an outright refusal). `crate::unordered::build_deriv_chain` (Path A) is separately flagged as "the
one place in the entire survey... where a genuine per-level union of every candidate rule occurs with
NO determinism argument found anywhere in the source" — a live, unaddressed Mohri-1997 concern, not a
resolved one.

---

### 2.28 `[B]` Templated underlying-form emitter + rule-cascade composition (`emit_underlying_templated`, `compile_templated_morphotactics`): real code, genuinely mixed verdict

**Mechanism.** `emit.rs`'s `TextMode::UnderlyingTokens` variant emits plain underlying text in
`SegAlphabet` token space at every leaf site (no surface-probed junction-variant union, no
representation cartesian product — char-def identity already collapses that) while reusing every
STRUCTURAL function (`build_deriv_chain`, `build_slot_chain`) byte-identically with
`TextMode::SurfaceProbed` (`emit.rs:214-233`). Paired with `replace.rs`'s rule cascade
(`compile_and_compose_rules_recall_safe`), this is the ONE path in the whole `pg-foma` crate by which
`replace.rs` becomes reachable from a `pangloss` subcommand a user might run
(`pangloss recipe-optimize`'s `token-cascade-morphology` recipe) — per `docs/fst-plan/
conformance-fst-measurement.md` §6.

**Trigger.** Selected by the recipe optimizer's own applicability/scoring machinery, not by a grammar
construct detector a user would consult directly.

**What it bought — genuinely two different, apparently-conflicting measurements exist in this
codebase's own history, both real, on different grammars.**

- **Positive (Aweti-shaped).** Aweti's templated lexc alone: **23,661 states / 346,727 arcs**; the FULL
  composition (lexc .o. rules .o. boundary-cleanup), minimized: **35,846 states / 800,354 arcs, <3s**
  (`compose_budget.rs`'s own calibration comment, and `phase-b-compose-budget-design.md` §8) — no
  enumeration blow-up at all, a direct contrast to §2.13's disaster on the SAME grammar. A follow-on fix
  (dedicated-level-per-rule derivation chain, `docs/fst-plan/p6-deep-truncation-chain-report.md` §1)
  shrank this further to **14,806 states / 270,541 arcs**, fixing a real `PATHCOUNT_OVERFLOW`/
  `apply_up`-non-termination problem this construction had introduced (a single epsilon-yielding rule's
  tag was choosable up to 22×/48× along one path in the original chain design; the fix gives each rule
  its own dedicated level(s), capped at `MAX_DEDICATED_LEVELS_PER_RULE = 4`). Compose-based recall
  reported at **65/101** (later reconciled to **68/104** with a fuller denominator,
  `p6-deep-truncation-chain-report.md` §4), later still reported as **100/106** in
  `docs/fst-plan/synthetic-stress-grammar-plan.md`'s Phase C revised evidence (2026-07-28) — **this
  audit could not fully reconcile these two different recall figures for the same grammar across two
  different dated reports**; both are cited here rather than one being silently preferred, since the
  later number may reflect construct-coverage work done between the two dates, or the two measurements
  may be counting different things. Flagged explicitly as an open discrepancy for the report reader to
  resolve, not resolved by this audit.
- **Negative (a different, public conformance fixture, `templatic-root-modification`).**
  `docs/fst-plan/cascade-vs-enumeration-experiment.md` ran this SAME cascade path (via its already-
  public entry point, no new code) against three fixtures and found: on the one fixture exercising
  templatic/interdigitating "process" morphs (ablaut/`InsertSimpleContext`/`ModifyFromInput` — exactly
  Aweti's own relevant construct family), **the cascade path loses 6 of 25 words (24%) recall outright**
  — words both the oracle and the shipped enumeration engine confirm, the cascade confirms zero. Two
  distinct, source-verified causes: (1) two phonological rules (`prEpenthesis`/`prSimulFeeding`) are
  silently skipped by the rule compiler, with no fallback (unlike Path A's `junctions.rs` probe); (2)
  `InsertSimpleContext`/`ModifyFromInput` morphs are marked `allomorphs_skipped` with **no resynthesis
  mechanism at all** on this path — `emit_underlying_templated`'s own doc, read directly, states "no
  composite builder ever runs here," unlike Path A's `build_structural_composites` fallback (§2.12)
  which DOES catch this exact case for the mainline path.

**Reconciling the two results (a genuine open question, not resolved by this audit's own reading)**: it
is possible Aweti's own specific 18 phonological rules and specific templated morphotactics simply
never exercise the two failure modes the `templatic-root-modification` fixture hits (no
`InsertSimpleContext`/`ModifyFromInput` single-part ablaut allomorphs in Aweti's own rule set, or no
epenthesis/simultaneous-feeding rules of that exact shape) — this would make both results true
simultaneously, for different reasons, on different grammars. This audit did not verify Aweti's rule
set against those two specific triggers to confirm or refute that reconciliation; it is stated as the
most likely explanation, not as a verified fact.

**Which grammars.** Aweti (positive result, real gate, `tests/p6_aweti_gate.rs`, shipped at `dfb5025`).
The public `templatic-root-modification` fixture (negative result) is not one of the four named
grammars.

**What breaks without the chain-restriction fix specifically.** `apply_up` on `"ti"` did not complete
even 500 raw results in 45s before the fix (required an external kill); after, it completes 2,000,000
raw results in ~2.1s — though "an, ti" still don't surface their compose-verified-reachable oracle
analysis within that many raw results, "an `apply_up` search-ordering gap distinct from language
membership" — documented, not fixed, not hacked around.

---

### 2.29 `[B]` `ComposeBudget`: default-on state/arc/tuple/group/line caps on the composition path

**Mechanism.** `EnumerationBudget` (§2.15) guards ONLY the enumeration path; the composition path
(`replace.rs`, `gate.rs`, `uflexc.rs`) had **zero references to it and no `Result`-returning API at
all** before this module — everything returned bare `Option`/`Fsm`/panic. `ComposeBudget` introduces
checked wrappers (`compose_checked`/`union_checked`/`minimize_checked`) around every direct foma call
on this path, checking `Fsm::statecount`/`arccount` (free reads, `types.rs:223-224`) after each call, an
alpha-tuple survivor-count check BEFORE the expensive per-tuple compile loop (`replace.rs:506-513`), a
group-count check BEFORE any per-group compile work (`gate.rs:260-261`, "the single highest-leverage
check... gates all downstream V1/V4 work"), and an incremental line-count check during lexc emission so
a pathological grammar bails during the FIRST group rather than after building a multi-GB string
(`compose_budget.rs`, `phase-b-compose-budget-design.md`).

**Trigger.** Universal guard on the composition path once it exists at all — no per-grammar detection.
The specific default VALUES are calibrated against the four grammars' own real numbers (below).

**What it bought (exact constants, verified directly against the source in this audit).**
`DEFAULT_STATE_BUDGET = 2_000_000` and `DEFAULT_ARC_BUDGET = 20_000_000` — calibrated against Aweti's
real templated-path numbers (23,661 states/346,727 arcs lexc-only; 35,846/800,354 composed+minimized):
"~56x"/"~25x" headroom respectively (`compose_budget.rs:73-89`, read directly). `DEFAULT_TUPLE_BUDGET =
5_000` — Amharic's real worst case (312 survivors) sits ~14× under it. `DEFAULT_GROUP_BUDGET = 64` —
Indonesian needs exactly 2 groups, Amharic needs exactly 3; 64 covers up to 6 simultaneously-gated
subrules at full `2^k` combinatorial realization. **No graceful fallback by design** for a group-budget
breach: "merging/dropping groups is unsound (over/under-firing gated rules); the only correct response
is the typed error → fallback engine for that grammar" (`compose_budget.rs:100-111`, `phase-b-
compose-budget-design.md` §4 V6).

**A named, honestly-documented limitation, not glossed over.** "A between-step size check cannot catch
a blowup INSIDE one call: if a single compose/minimize call OOMs or spins, the check after it never
runs" — `fsm_compose` internally minimizes both operands (worst-case exponential determinize) BEFORE
returning, so the size caps only bound cost accumulating ACROSS calls, never inside one
(`compose_budget.rs:31-44`).

**Which grammars.** All four contribute a calibration number (Amharic: tuple budget; Aweti: state/arc
budget; Indonesian/Amharic: group budget), but the guard itself is universal, not per-grammar detected.

---

### 2.30 `[A]` "Outgoing-arc preparation": a one-time apply-time traversal optimization

**Mechanism.** A prepared-outgoing-arcs data structure for bounded `apply_up` traversal, replacing
whatever the previous per-lookup traversal did (`docs/fst-plan/
deep-truncation-chain-performance-follow-on.md`).

**Trigger.** Universal for the Aweti templated-path proposer once built; not conditional on a grammar
construct — a pure implementation-level performance fix applied once per compiled network.

**What it bought (measured, precise numbers).** One-time preparation cost: **5.364ms**. Bounded
traversal for the `parua`/`an`/`ti` probe words: **2.159ms → 0.889ms (2.43×)**, with "exact recorded
candidate/analysis identities, 100/106 recall, all 18 rules, and the 10,609-state/298,830-arc network
unchanged" — a pure speed win with an explicit equality proof, not a behavior change. Break-even is
named explicitly: "about 13 lookups" before the one-time preparation cost pays for itself.

**Which grammars.** Aweti (the templated/Path-B pipeline specifically; this is a performance
optimization on top of §2.28's mechanism, not a separate construction).

**What breaks without it.** Nothing breaks — this is a pure constant-factor win with a proof of
behavioral equivalence, not a capability fix.

**Discovered by measurement.** The doc's own ranked-experiments table names this as "Rank 1 — shipped,"
with five OTHER ranked candidate optimizations (semantic path canonicalization, targeted automaton-
intersection membership, earlier quotienting/determinization, confirm-group partitioning, incremental
decoding, content-addressed compiled-network cache) explicitly listed as **hypotheses, not predicted
wins**, each requiring its own red-test-first trigger before being attempted — this document is itself
a good primary source for "how this team decides whether an optimization is real" (a measured 20%
reduction on a named bounded workload, with exact-equality preserved, not reasoning from first
principles).

---

### 2.31 `[A]` `MprGroupOverwrite`'s capability predicate: a real code/documentation contradiction, and the unimplemented Path-B fix that would resolve it

**Mechanism (what's actually shipped).** `MprGroupOverwriteFailClosedPredicate::evaluate`, verbatim
from `docs/fst-plan/conformance-fst-measurement.md` §7:
```rust
fn evaluate(&self, profile: &CharacteristicsProfile, _plan_node: &PlanNodeKind) -> PredicateVerdict {
    profile.observations().iter().any(|obs| obs.kind == CharacteristicKind::MprGroupOverwrite)
        .then_some(PredicateVerdict::ConfirmOnly)
        .unwrap_or(PredicateVerdict::Admit)
}
```
**This never returns `Refuse`, under any input** — verified two independent ways: reading the function
body, and running `pangloss parse --engine=foma --no-enforce-capability` against two real fixtures
declaring a real, touched, multi-member `outputType="overwrite"` MPR group; both print `capability:
ConfirmOnly`, never `Refuse` (`conformance-fst-measurement.md` §9 Q1). This directly contradicts
roughly ten doc comments across six files (including this predicate's OWN name — "FailClosed" — and a
shipped unit test whose own doc comment says "must compose to `Refuse` (never `ConfirmOnly`, never
`Admit`)" immediately above an assertion that it equals `ConfirmOnly`).

**Trigger.** Presence of any `Overwrite`-output MPR group touched by any rule — statically detectable.
The predicate's ACTUAL behavior does not currently distinguish "touched, provably safe" from "touched,
genuinely unsafe" at all — it collapses everything to `ConfirmOnly`.

**What the never-shipped, fully-designed fix would do (`docs/fst-plan/
mpr-overwrite-encoding-research.md`, "Construction 2" — RESEARCH ONLY, not implemented anywhere).** A
reachability predicate (`Overwrite-drop-unreachable(G)`): monotone accumulation is provably EXACT (not
merely a safe over-approximation) for a group `G` if no two distinct, order-relevant touch points to
`G` ever assert genuinely different subsets. Measured against the (unnamed, per this repo's synthetic-
documentation rule) three reference grammars this research doc examined: "the only Overwrite group any
rule in any of the three reference grammars ever actually reads or writes is a singleton" and "every
multi-member Overwrite group in every reference grammar today is dead declaration — present... but
referenced by nothing" — so all three grammars' groups pass this reachability test TODAY, five of six
groups vacuously (nothing ever touches them). This would make the CURRENT `ConfirmOnly` verdict a
PROVEN one instead of an undocumented drift, at zero new FST cost (a characterizer-only reachability
pass, same complexity class as the already-shipped `compounding_max_depth`).

**Which grammars.** The three grammars this specific research doc examined (unnamed per the synthetic-
documentation convention, but consistent with the reference-grammar set) all pass the would-be fix's
predicate trivially. Whether this generalizes to Aweti/Sena is unconfirmed here.

**What breaks without the fix.** Nothing breaks at runtime — over-generation is always confirm-safe.
What is broken is TRUST: a user reading `pangloss fst-health`'s capability line for this construct is
told something the code no longer actually does, silently, since some point after the research doc was
written. `conformance-fst-measurement.md` ranks this as "the cheapest, highest-trust-value item" in its
whole gap list — a documentation/rename fix, or implementing the already-designed Construction 2, at
low cost either way.

---

### 2.32 `[B]` Recipe search / Plan-tree rewrites: mostly a constant-factor knob, not a growth-class change

**Mechanism.** A `RecipeFamily` (`recipe_registry.rs`) = an applicability predicate + a `Materializer`
applying one provably semantics-preserving rewrite to a baseline compiled `Plan`: reorder a gate's
partition groups, reorder a union's children, split a gate group into 2 or N sub-groups.

**Trigger.** Structural applicability against a specific `Plan` shape (e.g. `GatePermutation` needs a
gate with ≥2 groups; `complete-template` needs a permutable `Union` in the baseline plan at all — the
`recipe-template-generic` fixture's own `RECIPE_ELIMINATION.md` records this family as "structurally
inapplicable" to that grammar, an honestly-reported non-result).

**What it bought — a directly measured, not derived, finding, and the most recent commit at the time
several of these docs were written.** "Across 8 synthetic fixtures × 10 repetitions, all 7
rewrite-shaped recipes produced **bit-identical states/arcs/proposals/confirmation** to baseline
(assembly ends in `minimize_checked`, which canonicalizes away everything a group/union reordering
varies) — the ONLY metric that moved was build time, and only UPWARD (2.1×-5.2× baseline for partition
refinement). **Recipe search over 7 of 8 families changes a constant, never a growth class**" —
verbatim from `conformance-fst-measurement.md` §6, citing this repo's own `d1389eb` commit. This is the
single clearest documented instance in this whole audit of "minimization erases a whole axis of
plan-tree variation" — consistent with the user's own prior memory note ("Recipe axis is the compiler...
plan-shape recipes are erased by minimization... real axis = EmissionStrategy").

**The one genuinely different family — `token-cascade-morphology`** — is §2.28's mechanism (a real,
different compiler, not a Plan rewrite); this is the one recipe that actually changes which
construction builds the net, not merely how the same construction's intermediate steps are ordered.

**Which grammars.** The four `recipe-*-generic` fixtures are **synthetic promoted plan-shape fixtures**,
explicitly NOT the four language grammars (per this doc's own §0 disambiguation). Real production
evidence against these four synthetic fixtures is in `docs/fst-plan/
four-grammar-recipe-evidence-2026-07-28.md`, which separately found (repeating the same run 5 times)
that the "winner" flips between repetitions at sub-millisecond timing deltas on the one genuinely
non-trivial case (`recipe-gated-generic`) — motivating a later switch (per that doc's own banner) away
from wall-clock ranking toward deterministic HC-confirmation-step counters.

**What breaks without it.** Nothing breaks; this whole subsystem is a search-and-rank layer over an
already-sound compilation, not a capability bridge — its risk is picking an unstable "winner" on noisy
timing, not correctness.

---

### 2.33 `[A]` Content-addressed `Plan` node interning (`NodeId` dedup)

**Mechanism.** `plan.rs`'s `Plan::add_node` deduplicates identical plan subtrees by content address —
"measured once, stored once" (per `docs/fst-plan/grammar-optimization-techniques.md` E4, citing
`plan.rs:32-37`).

**Trigger.** Universal — any Plan construction benefits automatically; no per-grammar detection.

**What it bought.** Unmeasured as an isolated number in the material read for this audit; structurally,
this is what allows §2.32's recipe rewrites to observe "bit-identical" results across permutations (the
SAME compiled leaf is reused/recognized as identical regardless of which parent tree reached it).

**A documented near-miss, worth citing as a cautionary tale for any future memoization in this area.**
A cross-word `(MRuleId, Shape)` synthesize memo was tried during FST optimization and found **UNSOUND**
— not merely slow, a real correctness trap (per this repo's own prior design record, cited in
`grammar-optimization-techniques.md` E4). The generalizable lesson: memoizing across call boundaries in
this codebase needs an explicit soundness argument for what varies and what does not between calls —
plan-node content addressing works because `NodeId` genuinely captures everything the compiled artifact
depends on; the synthesize memo failed because it didn't.

---

### 2.34 `[A]` Differential oracle (`oracle.rs`): correctness-by-disagreement, not by a single ground truth

**Mechanism.** Builds ≥2 independently-derived over-approximations of the same grammar (via
`permute_gate_groups`, a second topology generator) and uses their DISAGREEMENT as a designed-in
correctness oracle, rather than trusting either one alone (`oracle.rs:1-6`, per
`grammar-optimization-techniques.md` G2).

**Trigger.** Universal — applicable to any grammar with a gate-partitionable structure.

**What it bought.** Unmeasured as a standalone number here; conceptually this is the same
diverse-redundancy argument as N-version programming, applied to compiler correctness — if two
capability-passing candidates disagree on a word's proposed set, one has a capability-envelope bug,
independent of which would otherwise win on cost.

### 2.35 `[B]` Ordering-multiplicity budget for `Unordered` strata, calibrated against Sena's real ceiling

**Mechanism.** Caps the loose-rule count of a `MorphRuleOrder::Unordered` stratum (whose combination
cascade admits up to `n!`, or under multi-application an `n^d`-shaped, number of rule orderings) —
a coarse but sound proxy: rule count is monotonically related to the true combinatorial danger even
though the module's own doc calls this "a conservative placeholder pending real-grammar measurement...
not a final number" (`compose_budget.rs:268-281`).

**Trigger.** Any stratum declared `Unordered` — statically detectable, boolean; the DANGER scales with
that stratum's own loose-rule count, also statically countable.

**What it bought (measured, and the calibration basis is named exactly).** `DEFAULT_ORDERING_
MULTIPLICITY_BUDGET = 100`, calibrated against "every `Unordered` stratum in this repo's own
reference/conformance corpus... and — the real ceiling — `samples/data/sena-hc.xml` (**25 loose rules**
in its own largest `Unordered` stratum)" — "100 leaves ~4× headroom above that measured ceiling"
(`compose_budget.rs:283-295`). Unlike most budgets in this catalogue, this one is explicitly framed as
a HARD CONSTRAINT DERIVED FROM A REAL GRAMMAR, not a generous multiplier chosen freely: "the HARD RULE
this crate follows is 'existing behavior unchanged': a default that regresses Sena... would violate
that regardless of how principled the calibration reasoning is otherwise."

**Which grammars.** Sena is named explicitly and repeatedly as "the real ceiling" for this budget — the
only one of the four grammars with a large `Unordered` stratum. Public conformance fixtures contribute
smaller data points (16, 11, 7, 6, 5 loose rules).

**What breaks without it.** `ComposeError::OrderingMultiplicityExceeded`'s own message: an `Unordered`
stratum beyond the cap is "honestly unsupported... never silently truncated" — a typed refusal, not a
wrong answer, for a hypothetical grammar whose `Unordered` stratum exceeds the calibrated bound.

---

### 2.36 `[B]` Apply-path and apply-candidate budgets: a SECOND, differently-calibrated pair of constants for the recipe-optimizer's own evaluation loop

**Mechanism.** Two deterministic, magnitude-only counters checked cooperatively inside the `apply_up`
decode loop itself (not at compile time): a cap on raw decoded-path count, and a cap on distinct
`(morphemes, root_index)` candidates accumulated. Explicitly framed as categorically different from
every compile-time budget above, because the apply path runs in-process, per word, against a
long-lived reused `ApplyHandle` — "a native thread cannot be safely hard-killed in Rust... the only
sound containment left is a deterministic, magnitude-only counter" (`compose_budget.rs:305-321`).
Ordinary `pangloss` traffic (`FomaProposer::propose`) is untouched (always `ApplyBudget::unbounded`,
which can never trip) — these two constants are resolved ONLY by the recipe-optimizer's own
`recipe_runtime::RuntimeBudget`.

**Trigger.** Not tied to a grammar construct at all — a runtime magnitude (how many raw `apply_up`
results a given word's plan-composed net produces) that can only be discovered by actually running the
query; no static count over the grammar predicts it.

**What it bought (measured against a synthetic pathological fixture, NOT one of the four named
grammars).** A grammar with `k` all-optional template slots, each firing a rule with
`multipleApplication = 1` (the DTD default), has a legitimate analysis count of `C(k_slots, k)` but a
plan-composed net can propose `k_slots^k` raw paths — "strictly more... it admits the SAME rule firing
repeatedly, which `multipleApplication = 1` forbids." Measured: **k=6 → 2,985,984 raw paths for 924
real analyses** (confirm still filters back down to exactly 924 — recall is not the problem,
magnitude is); **k=12 → ~8.9×10¹² raw paths**, large enough that `apply_up`'s eager enumeration
exhausts committed memory and aborts the process outright, "with no compile-time budget in this module
positioned to catch it" (`compose_budget.rs:374-407`). Both constants set to **1,000,000** — chosen to
sit between the largest real plan-composed net in the corpus (≤479 arcs) and the smallest pathological
case (3× over the cap at k=6).

**Which grammars.** None of the four by name — a synthetic stress fixture (a grammar with 12
all-optional template slots), explicitly distinct from Amharic/Aweti/Indonesian/Sena. This is one of
the few techniques in this catalogue whose entire calibration basis is a *deliberately constructed*
adversarial grammar rather than a real reference one — worth flagging for the "measured vs. reasoned"
question in §3.4: the trigger class was reasoned about in advance (over-permissive optional-slot
combinatorics), then a fixture was BUILT to confirm the reasoning, rather than being discovered as an
accident on a real corpus.

**What breaks without it.** Process death from memory exhaustion on one pathological word during
recipe-optimizer evaluation — the SAME failure class `EnumerationBudget` (§2.15) closes for the
eager-enumeration path's Aweti case, but here for the recipe-optimizer's own plan-composed nets on a
different, template-combinatorics trigger.

---

### 2.37 `[B]` Two prior, fixed bugs surfaced only by dedicated regression fixtures (RTL `Slot::Repeat` reversal; multi-table default-to-table-zero)

**Bug 1 — shallow RTL slot reversal.** `reversed_slots` (the function building an RTL rule's mirror
image, §2.27) originally used a shallow `.rev().cloned()` that failed to recurse into a
`Slot::Repeat`'s own `children` — producing a compiled RTL branch whose language was "provably NOT the
true reverse (the compiled net requires the WRONG, swapped environment content)." Fixed by recursing
into `Slot::Repeat`'s children during reversal; the pre-fix shape is deliberately kept in the test
suite as a named historical witness (`shallow_reversed_slots_pre_fix`), not merely deleted
(`replace.rs:858-896`, `2508-2779`). None of the four named grammars use RTL rewrite rules at all — this
bug was found and fixed entirely inside a Path-B construct with **zero real-grammar exposure**, purely
via a synthetic regression fixture built specifically to exercise `RewriteMode` × `Dir::RightToLeft` ×
`PatternNode::Quantifier` jointly.

**Bug 2 — hardcoded `char_tables[0]` default in rule-table resolution.** `owning_table`/
`owning_table_for_metathesis` previously (implicitly) risked falling back to `g.char_tables[0]` for a
multi-table grammar's rule; the fix walks `g.strata`'s own `prules` lists to resolve each rule's REAL
owning table. Verified via a synthetic two-table fixture deliberately built with different table
cardinalities on each side (2 vs. 3 segments), "so a wrong resolution produces a *provably wrong*
tuple count" (`replace.rs:2310-2499`, `owning_table_tests`) — i.e. the test is specifically designed so
the old bug's wrong answer and the new fix's right answer are numerically distinguishable, not merely
different code paths that happen to both compile. None of the four named grammars are multi-table
(confirmed independently for Indonesian/Amharic/Aweti by `docs/fst-plan/p6-prototype-report.md` §6 item
1; Sena's table count is unconfirmed in this audit) — this is a second Path-B-adjacent fix (though the
underlying mainline `pg-rules` table-resolution machinery it parallels, per §2.23, IS real and used by
both engines) validated purely by synthetic construction, ahead of any real multi-table grammar
existing in the corpus.

Both are included here specifically because they are clean examples of "found by building a dedicated
adversarial fixture, not by running a reference grammar" — a THIRD provenance category worth
distinguishing from "found by a reference-grammar corpus word" (§2.4, §2.8, §2.12) and "found by
running an already-public entry point against an already-public fixture" (§2.28's cascade-vs-
enumeration finding): here, the fixture itself did not exist until someone specifically built it to
probe a suspected gap, ahead of any grammar (real or synthetic-stress) actually needing the construct.

---

## 3. Answering the four cross-cutting questions

### 3.1 Universal vs. conditional

**Universal (always applied, no detection needed):**
- Lexc continuation-class morphotactics (§2.1)
- NFD normalization alignment (§2.9)
- `EnumerationBudget`'s fail-fast check on the enumeration path (§2.15) — the guard itself is
  unconditional; only whether it *trips* depends on the grammar
- `ComposeBudget`'s checked wrappers on the composition path (§2.29) — same distinction
- Bounded compound loop + computed depth cap, applied whenever any `CompoundingRuleDef` exists (§2.6,
  §2.17) — arguably conditional on the *presence* of compounding, but the mechanism itself (compute the
  bound, don't guess it) is applied the same way regardless of scale
- Content-addressed Plan interning (§2.33), differential oracle (§2.34) — infrastructure, not
  construct-triggered
- Outgoing-arc preparation (§2.30) — applied once per compiled network unconditionally

**Conditional (triggered by a specific grammar property):**
- Everything keyed to a specific `Role`/construct: reduplication peel (§2.18), rule-application
  pre-expansion for interdigitation/fusion (§2.12), junction-probing for real phonology (§2.10),
  MPR/POS static partition (§2.21), RTL/metathesis/Simultaneous-overlap compilation (§2.27),
  multi-table aliasing (§2.23), template grouping (§2.3), bare-root discharge (§2.7)
- The templated underlying-emitter/rule-cascade path (§2.28) — selected only when the recipe optimizer
  characterizes it as applicable and it wins, never a default

### 3.2 For each conditional technique: is the trigger automatically detectable from the grammar alone?

Most are **cleanly detectable** — a boolean or a count read directly off the grammar model, no runtime
information needed:
- Presence of any phonological rule (§2.10), any `<AffixTemplate>` with multiple templates sharing a
  category (§2.3), any `CompoundingRuleDef` (§2.6/§2.17), any multi-`<Representation>` char-def (§2.8),
  any `Role::Reduplication`-classified rule (§2.18), any gated `RewriteSubruleDef` (§2.21), `Dir::
  RightToLeft`/`MetathesisRuleDef` presence (§2.27), more than one `<CharacterDefinitionTable>` (§2.23),
  `entry.allomorphs.len() == 1 && is_bound` (§2.7).

A smaller but important set is **NOT cleanly detectable from a static count alone** — they require
either running the real synthesis engine, or a specific, non-obvious structural distinction:
- **The Aweti-shaped enumeration blow-up (§2.13/§2.15)**: `composite_scale_hint`'s pre-flight predictor
  — the obvious "count roots × rules" detector — did **not** predict this explosion in advance; Aweti
  "looked ordinary on all three signals." The disaster predictor that actually works had to be found by
  measurement: emitted-entry count *during* the run, not any static count beforehand. This is the
  single clearest example in this catalogue of a trigger that resists static detection.
- **Which specific (root, rule) pair actually needs interdigitation/fusion composite treatment
  (§2.12)**: the TRIGGER for running the expensive path is static (any `Infix` role, any phonological
  rule present), but the OUTCOME per pair requires actually invoking `pg_rules::morph::synthesize` +
  `probe_synthesize` — not statically knowable in advance which pairs will actually differ from a naive
  literal rendering.
- **Whether a phenomenon is inside `junctions.rs`'s locality boundary (§2.10)**: the refined finding
  from live measurement is that the real boundary is not "environment width ≤1 segment" but "does the
  phenomenon need to see material living in more than one morpheme's own text at once" — a genuinely
  cross-morpheme dependency is invisible to the probe; material fully inside one morpheme's text (even
  a wide one) is fine regardless of segment count. A naive width-counting detector gets this WRONG in
  both directions (would flag some safe wide-window cases and might miss a narrow-window cross-morpheme
  one).
- **The MPR `Overwrite` reachability predicate (§2.31)**: detectable in principle (a reachability BFS
  over the rule graph), but the actual VALUE that determines admissibility — whether any two reachable
  touch points assert different subsets — needed a from-scratch analysis of each of the reference
  grammars' actual declared groups to discover that all touched groups happen to be singletons or
  never-touched at all; this was NOT obvious from the group *declarations* alone (the loader's own
  default makes every undeclared-output-type group `Overwrite`, so "declares an `Overwrite` group" is a
  near-useless signal on its own — most such groups turn out to be dead declarations).
- **The boundary-token cleanup bug (§2.20)**: the trigger (a semantically load-bearing `Boundary` kind,
  as opposed to a cosmetic separator) is visible in the char-def table's declarations, but this was not
  caught by any static analysis — it took an A/B measurement against the SAME grammar through two
  DIFFERENT build paths to find.

### 3.3 Which techniques conflict, or where does order matter?

- **Compose vs. union for α-tuple branches (§2.24, §2.26)**: this is the sharpest ordering lesson in
  the whole catalogue. Folding N per-tuple complete replace transducers with `fsm_union` COMPILES,
  RUNS, and is semantically WRONG (reintroduces a spurious "did nothing" path); the correct combinator
  is sequential `fsm_compose`, and it is correct **only because** the tuples' contexts are mutually
  exclusive by the joint-agreement filter's own construction (§2.26) — a precondition that must hold for
  the fold's own correctness, not a free choice between two equivalent implementations.
- **The SAME union-vs-compose question resurfaces at least three more times, with THREE DIFFERENT
  answers, and the report source material is explicit these must not be conflated**: RTL/metathesis
  branches ARE safely unioned (§2.27) because each branch is a complete transducer with no shared input
  a sibling branch would wrongly own; `gate.rs`'s per-group union (§2.21) IS safe because the groups are
  lexically disjoint languages; the Simultaneous-overlap case (§2.27) never unions at all — it either
  reuses the ordinary sequential cascade unchanged, or refuses outright. Applying the WRONG one of these
  three safety arguments to a construction it doesn't actually satisfy is precisely the historical
  392,311-state incident (§2.24).
- **Flag diacritics vs. everything else that touches `->` replace rules (§2.22)**: this is a genuine,
  confirmed-twice conflict — a flag literal inside a `->` context corrupts the network regardless of
  what else is going on; this rules out flag diacritics as a component of ANY future technique that
  needs to gate inside a replace-rule context, not just the one construct (MPR/POS gating) it was first
  tried for. `flag_is_epsilon = true` must be set before ANY `fsm_compose` call where either side may
  carry flags outside a `->` context — a global toolkit setting, not a per-call choice, so it interacts
  with every other composition happening in the same process.
- **Boundary-token handling (§2.11 vs §2.20)**: two DIFFERENT parts of this codebase handle the exact
  same category of char-def (`Boundary`-kind) in two structurally different ways — the mainline emitter
  (§2.8/§2.11) never puts these tokens on the queryable tape at all (drops them, enumerates variants at
  emit time); a different build path (§2.20) put them on the tape and tried to clean them up post-hoc
  with one blanket context-free deletion rule, and that specific ordering (emit-with-tokens, THEN
  compose-away-unconditionally) is exactly what caused the 425× blow-up. The two approaches are not
  interchangeable variants of the same technique — one is provably safe by construction, the other
  requires either never doing it this way, or restricting the cleanup to be context-sensitive per
  occurrence (an unshipped fallback the doc names but does not implement).
- **Morphotactic pruning (§2.14) is necessary but not sufficient (§2.13)**: pruning strictly improves
  the SEARCH cost (verified subset-of-flat, no recall risk) but does nothing to the EMITTED OUTPUT size
  for a grammar whose real combinatorics are simply too large — a case where two techniques that sound
  like they should compose to "solved" instead leave a residual problem (§2.15's fail-fast refusal) that
  neither one addresses.

### 3.4 Which were discovered by measurement rather than reasoning?

This is a long list, and it is the single most important section for the owner's stated purpose — these
are exactly the techniques a from-first-principles generic detector is least likely to reproduce,
because their trigger or their fix was found only by actually running something and watching it fail or
succeed unexpectedly, not by reasoning about the grammar's declared structure:

1. **Derivation-layer depth = rule count** (§2.4) — found because `kubulukira` (a real Sena corpus
   word) failed under the C#-inherited fixed depth of 2.
2. **Outer (post-template) derivation layers** (§2.5) — found because Sena's `=mbo` clitic ordering
   didn't match the C# trie's own assumed structure; the C# trie itself had no construction for this at
   all, so there was nothing to "port" — this was discovered, not translated.
3. **Surface-variant cartesian product for multi-representation segments** (§2.8) — found as "13 of the
   first recall gate's 19 misses" on Sena.
4. **Rule-application pre-expansion's feature-segmentation bug** (§2.12) — using the loader's raw shape
   instead of a feature-re-segmented one silently zeroed recall to 0/76 roots for the target rules; only
   caught by actually running the fix and comparing.
5. **The Aweti disaster's real predictor is emitted-entry count, not probe count** (§2.15) — the
   intuitive metric (search-tree size) was tried first and found insufficient; the correct metric was
   found only after watching Aweti explode with a "moderate" 8.37M probe count but a catastrophic entry
   count.
6. **Morphotactic pruning's real-world payoff is smaller than static analysis predicted** (§2.14) — the
   static upper bound (up to 3.9×) over-predicted the realized 2.92× shrink, because dynamic filters
   were already doing more of the work than the static analysis accounted for.
7. **The α-tuple union-vs-compose semantic bug** (§2.24/§2.26) — "caught empirically, not by
   inspection": `apply_down` returning both a correct and a spurious path on a hand-built test case.
8. **The adjacent-PUA-codepoint tokenizer bug** (§2.25) — found only by building something non-trivial
   and bisecting a failure; cost Indonesian 25 percentage points of recall (72/97 → 97/97) silently,
   with no compile error at any point.
9. **The comma-join xre restriction** (§2.24) — found by attempting the "obvious" plan and having it
   fail to compile; not documented anywhere in the toolkit's own docs per this repo's own investigation.
10. **The MPR `Overwrite` reference-grammar shape** (§2.31) — "the only Overwrite group any rule in any
    of the three reference grammars ever actually reads or writes is a singleton" was found by grepping
    every actual usage, not by reasoning about what `Overwrite` groups typically look like.
11. **The boundary-token cleanup 425×/516× blow-up** (§2.20) — found by a direct A/B probe comparing two
    build paths on the identical five words; the root cause (three semantically different `Boundary`
    kinds treated identically) was visible in the grammar source the whole time but not caught until the
    measurement pointed at it.
12. **The templated-cascade's 24% recall loss on process morphs** (§2.28) — found by deliberately
    running an already-public, already-wired entry point against a fixture chosen specifically because
    it exercises the relevant construct family, and discovering a real, reproducible, deterministic
    recall regression the architecture had been hoped to fix, not merely leave unimproved.
13. **The dedicated-level-per-rule chain fix's real motivating number** (§2.28) — the 22×/48×
    choosability-per-path figure and the `PATHCOUNT_OVERFLOW` symptom were discovered by running
    `apply_up` and watching it fail to terminate, not predicted from the chain construction's design.
14. **Outgoing-arc preparation's 2.43× win and 13-lookup break-even** (§2.30) — explicitly the product
    of a measurement-gated experiment protocol (this doc's own methodology section demands a measured
    ≥20% reduction with an exact-equality proof before shipping ANY of six ranked candidates, and five
    of the six remain unshipped hypotheses for exactly this reason).
15. **Flag diacritics being closed, not merely unproven, for replace-rule contexts** (§2.22) — three
    independent toolkit defects, each found by writing a minimal throwaway probe and bisecting, not by
    reading the toolkit's own documentation (which does not name any of these three interactions).
16. **The recipe-optimizer's own headline finding — minimization erases plan-tree shape** (§2.32) — "the
    project's own most recent commit is this exact discovery," arrived at by running 8 fixtures × 10
    repetitions and finding bit-identical outputs, not by reasoning about what minimization should do to
    a reordered plan.

By contrast, the techniques that WERE substantially derivable by reasoning alone, before measurement
confirmed them, are the ones with a clean mathematical story from the outset: the tuple-indexed
α-variable bound (§2.26, `nc15 × nc16` was predicted before the Amharic run, and the measured 312
matched the prediction closely); the compounding depth formula (§2.17, a closed-form recurrence);
`ComposeBudget`'s calibration methodology itself (§2.29, "measure the largest real grammar, apply a
large safety multiplier" is a reasoned policy, even though the specific numbers it applies to came from
measurement). The dividing line in this codebase's own history is consistently: **constructions with a
clean automata-theoretic story (composition, subset construction, cross products with a provable filter)
tend to be predictable in advance; constructions that depend on emergent interaction between many rules,
many roots, and a specific vendored toolkit's undocumented edge behavior are not**, and the second
category is exactly where this team's own document trail shows repeated "found by measurement, not
predicted" language.

---

## 4. Summary table

| # | Technique | Path | Universal/Conditional | Grammar(s) that needed it | Measured win | Detectable trigger? |
|---|---|---|---|---|---|---|
| 2.1 | Lexc continuation-class morphotactics | A | Universal | All four | additive/linear, unmeasured as a number | n/a |
| 2.2 | Per-template slot chains (one tag/slot) | A | Conditional | Sena | 2.5M→8 candidates (`mbali`) | Yes (template/slot count) |
| 2.3 | Template grouping by shared FS | A | Conditional | Sena | 24→9 groups | Yes |
| 2.4 | Derivation depth = rule count | A | Conditional | Sena | fixes `kubulukira` miss | Partially — soundness relies on `multipleApplication=1` |
| 2.5 | Outer derivation layers | A | Conditional | Sena | fixes `=mbo` ordering | No — found only by measurement |
| 2.6/2.17 | Bounded compound loop + computed depth cap | A | Conditional | unconfirmed which of the four | linear, formula-derived | Yes |
| 2.7 | Bare-root compile-time discharge | A | Conditional | none in current corpus | proven no-op on real corpus | Yes |
| 2.8 | Surface-variant cartesian product | A | Conditional | Sena (also Indonesian) | fixed 13/19 recall misses | Yes |
| 2.9 | NFD normalization alignment | A | Universal | likely Amharic | unmeasured | n/a |
| 2.10 | Junction-aware `PhonologyProbe` | A | Conditional | Indonesian (also Amharic/Aweti) | 8-48x pipeline speedup (whole pipeline, not isolated) | Partially — locality boundary is subtle |
| 2.11 | Deletion-junction `{name}Stripped` partitions | A | Conditional | Indonesian | bundled into above | Yes (structural check) |
| 2.12 | Rule-application pre-expansion (interdigitation/fusion) | A | Conditional | Amharic | 2,930+51,023 entries, closes 32/32 P1c misses | Trigger yes, outcome no |
| 2.13 | Aweti enumeration blow-up (same mechanism) | A | Conditional | Aweti | crash: 2.83M entries, 34GB RSS | No — pre-flight signals looked ordinary |
| 2.14 | Morphotactic pruning | A | Conditional (guard universal on the path) | Amharic | 2.92x/6.9x shrink | Trigger yes, magnitude no |
| 2.15 | `EnumerationBudget` fail-fast | A | Universal (guard) | Aweti (calibration) | trips in ~2s vs 551s | n/a (guard) |
| 2.16/2.17 | Compounding license + chain | A | Conditional | unconfirmed | O(N×depth), linear | Yes |
| 2.18 | `ReduplicationPeeler` | A | Conditional | Indonesian | O(word length), N-independent | Yes |
| 2.19 | `classify_affix` precedence fixes | A | Conditional | pinned by synthetic fixtures | closes 3 staged bugs | Yes |
| 2.20 | Boundary-token cleanup fix | A | Conditional | Sena | 425x→~94x-reduced blow-up fixed | No — found by A/B measurement |
| 2.21 | Static MPR/POS partition (`gate.rs`) | B (mechanism); A (effect via over-gen instead) | Conditional | Indonesian, Amharic | closes real recall gap in B; A gets it right differently | Yes |
| 2.22 | Flag diacritics (dead end) | B (attempted) | Conditional | would-be Indonesian/Amharic | negative — 3 toolkit defects | Trigger yes, failure mode no |
| 2.23 | `RepresentationAliasMap` | B | Conditional | none of the four (untested) | n/a | Yes |
| 2.24 | Kaplan-Kay rewrite cascade compiler | B | Conditional | Indonesian/Amharic/Aweti (rules only) | 82 states/1.1M arcs Amharic in 2.14s; 28.8ms Aweti | Yes |
| 2.25 | PUA token alphabet | B | Universal (within Path B) | Indonesian (bug fix: 72/97→97/97) | large silent-bug fix | n/a |
| 2.26 | Tuple-indexed α-variable resolution | B | Conditional | Indonesian, Amharic | 121,776→312 survivors | Yes |
| 2.27 | RTL/metathesis/Simultaneous-overlap | B | Conditional | none of the four (unexercised) | n/a | Yes (RTL/metathesis); requires intersection (Simultaneous) |
| 2.28 | Templated underlying emitter + cascade | B | Conditional, recipe-selected | Aweti (mixed: positive + unresolved recall-figure discrepancy; negative on a different fixture) | 35,846/800,354→14,806/270,541 states/arcs; 2.43x apply speedup; also −24% recall on a different construct family | No |
| 2.29 | `ComposeBudget` | B | Universal (guard) | all four contribute calibration | 25-56x headroom over real numbers | n/a (guard) |
| 2.30 | Outgoing-arc preparation | A (Aweti templated path) | Universal | Aweti | 2.43x, 5.364ms one-time cost | n/a |
| 2.31 | `MprGroupOverwrite` predicate contradiction | A (shipped); B (would-be fix) | Conditional | 3 reference grammars, all pass vacuously/via singleton | n/a (a bug in the capability system's self-description) | Partially |
| 2.32 | Recipe search / Plan-tree rewrites | B (mostly) | Conditional | 4 synthetic fixtures, not the 4 language grammars | bit-identical output, 2.1-5.2x build-time cost | n/a |
| 2.33 | Content-addressed Plan interning | A | Universal | all | unmeasured directly | n/a |
| 2.34 | Differential oracle | A | Universal | all (infra) | unmeasured directly | n/a |

---

## 5. Technique-independence matrix: do these compose, or do they arrive in bundles?

**The question this section answers, precisely:** if the owner builds a set of named, independently
tunable "subrecipes" and lets an engineer combine them per language family, does that combinatorial
freedom buy anything real — or does every grammar in this repo's own history actually need one
fixed, coherent bundle, in which case a five-item menu (one recipe per grammar shape) is simpler and
loses nothing? This is answered from evidence already gathered above, not from new measurement.

### 5.1 The matrix

Only techniques with a grammar-specific trigger are included (universal techniques — the morphotactic
spine, NFD alignment, `EnumerationBudget`/`ComposeBudget`'s mere *existence* as opposed to which
constant a grammar happens to exercise, Plan interning, the differential oracle — apply everywhere and
say nothing about bundling, so they are excluded here and noted separately at the end). "✓(n)" gives a
scale parameter where one was measured, to show whether the SAME technique was exercised at different
magnitudes on different grammars (the strongest single signal for composability this audit has).

| Technique | § | Amharic | Aweti | Indonesian | Sena |
|---|---|:---:|:---:|:---:|:---:|
| Per-template slot chains (one tag/slot) | 2.2 | | | | ✓ |
| Template grouping by shared FS | 2.3 | | | | ✓(24→9) |
| Derivation depth = rule count | 2.4 | | | | ✓ |
| Outer (post-template) derivation layers | 2.5 | | | | ✓ |
| Bounded compound loop + computed depth cap | 2.6/2.17 | | | | ✓ (`"^0+"`, 7 occ.) |
| Surface-variant cartesian product | 2.8 | | | ✓(`char28`) | ✓(`char4`) |
| Junction-aware `PhonologyProbe` | 2.10 | ✓ | ✓ | ✓(primary) | — (true no-op) |
| Deletion-junction `{name}Stripped` partitions | 2.11 | | | ✓ | |
| Rule-application pre-expansion (interdigitation/fusion) | 2.12 | ✓(primary) | | | |
| Enumeration-path blow-up (same mechanism, pathological) | 2.13 | | ✓(crisis) | | |
| Morphotactic pruning (`MorphotacticIndex`) | 2.14 | ✓(measured A/B) | ✓(necessary-not-sufficient) | | |
| `EnumerationBudget` calibration | 2.15 | ✓(22,775 entries) | ✓(2.83M entries, primary) | | |
| `ReduplicationPeeler` | 2.18 | | | ✓ | (no redup rules) |
| `classify_affix` precedence fixes | 2.19 | (construct family) | | | |
| Boundary-token null-shaped-affix reroute | 2.20 | | | | ✓(`"^0+"`) |
| Static MPR/POS partition (`gate.rs`) | 2.21 | ✓(k=3) | | ✓(k=1) | |
| Flag-diacritic dead end (attempted, reverted) | 2.22 | ✓(would-be) | | ✓(would-be) | |
| Kaplan-Kay rewrite cascade compiler | 2.24 | ✓(compile-only) | ✓(rules-only) | ✓(100% recall) | |
| PUA-alphabet tokenizer bug found | 2.25 | | | ✓(72/97→97/97) | |
| Tuple-indexed α-variable resolution | 2.26 | ✓(v=20) | | ✓(v=1) | |
| Templated underlying emitter + cascade | 2.28 | | ✓(primary) | | |
| `ComposeBudget` tuple/state/arc/line/group calibration | 2.29 | ✓(tuple cap, group=3) | ✓(state/arc/line cap) | ✓(group=2) | |
| Outgoing-arc apply-time preparation | 2.30 | | ✓ | | |
| Ordering-multiplicity budget (`Unordered` strata) | 2.35 | | | | ✓(25 loose rules) |

Rows with a single ✓ mean "the only one of the four that exercises this trigger, in the sources read
for this audit" — **not** "the only grammar that could ever need it."

### 5.2 The read: mostly scattered/independent, with one real block-diagonal seam

**At the level of individual sub-techniques within the mainline path, the picture is scattered, not
bundled** — and the strongest evidence for that is not the empty cells above (which, with only four
grammars, are cheap to produce under either hypothesis) but the **rows with more than one non-empty
cell, at different scale parameters, requiring zero grammar-specific code changes to port**:

- **Static MPR/POS gating (§2.21)** runs unmodified at `k=1` gated subrule (Indonesian) and `k=3`
  (Amharic) — genuinely the same mechanism, same code, reused across two grammars that otherwise share
  almost nothing else in this matrix (Indonesian's row is junction-phonology-and-reduplication-shaped;
  Amharic's is interdigitation-and-alpha-variable-shaped).
- **Tuple-indexed α-variable resolution (§2.26)** runs unmodified at `v=1` (Indonesian's `prule4`) and
  `v=20` (Amharic's CV-merger) — the sources are explicit that this is "the SAME generic code path,"
  with "no Amharic-specific logic" needed to scale 20×. This is the single cleanest piece of evidence
  in the whole corpus that a sub-technique is a genuine, parameterizable subrecipe rather than a
  grammar-specific hack.
- **Morphotactic pruning + its `EnumerationBudget` guard (§2.14/§2.15)** fires for Amharic's
  interdigitation search AND Aweti's fusion search — two DIFFERENT triggering constructs
  (`Role::Infix` vs. fusion-class rules) feeding the SAME pruning/guard machinery. The guard doesn't
  care which construct produced the large search space; it only measures the space itself.
- **`ComposeBudget`'s five dimensions (§2.29)** are calibrated from FOUR different grammars along
  FOUR different axes simultaneously (Amharic → tuple cap and one group-cap data point; Indonesian →
  the other group-cap data point; Aweti → state/arc/line caps; Sena → the ordering-multiplicity cap,
  §2.35) — a single guard module that every grammar in the corpus contributes calibration evidence to,
  with no grammar needing all five dimensions and no dimension needing all four grammars.

**Conversely, several apparent "bundles" are not evidence of coupling — they are evidence of a rare
construct simply being exercised by only one grammar in a four-grammar sample.** Sena's five
morphotactic-scale techniques (§2.2–§2.5, §2.20) all key off the SAME underlying property (many
templates / deep derivation chains / null-morph boundary markers — Sena's own large, template-heavy,
phonology-free lexicon), but they are triggered by FOUR SEPARATE, independently-checkable grammar
predicates (template-category-sharing count; standalone-rule count exceeding 2; a post-template
standalone rule at all; a `Boundary`-kind char-def with a null-morph-marker role) — a fifth grammar
with, say, many templates but a fixed, shallow (≤2-rule) derivation structure and no null-morph
markers would want §2.2/§2.3 without §2.4/§2.5/§2.20 at all. These five are **siblings triggered by
related but distinct facts about one grammar's morphotactic shape**, not one atomic bundle — the
matrix cannot fully distinguish "loosely correlated siblings" from "one bundle" with N=4, but the
per-technique trigger conditions catalogued in §2 (each independently readable off the grammar model)
argue for the former.

**The one place the data genuinely does look block-diagonal, and it is the top-level choice, not a
sub-technique**: Aweti is the only grammar that could not be served by extending the mainline
enumeration-based construction (`emit.rs`/`preexpand.rs`/`junctions.rs`) at all — pruning (§2.14) made
its search bounded but "necessary but not sufficient" (its own words), and the actual fix was a
**wholesale substitution** of the whole-grammar compiler (§2.28's templated-cascade path). This is not
a subrecipe you add to Aweti's recipe; multiple mainline sub-techniques (junction-probing §2.10,
pre-expansion enumeration §2.12/§2.13) become simultaneously *irrelevant*, not merely inert, once that
substitution is made — the templated path emits underlying tokens and composes a real rule cascade
instead. This matches the recipe-optimizer's own independently-discovered finding (§2.32,
`EmissionStrategy`): "two whole-grammar compilers win two different languages," and 7 of 8 other
plan-shape "recipes" change nothing but a build-time constant. **The real menu-vs-composition line in
this codebase's own history sits exactly at that one axis** — which whole-grammar compiler
(`TunedSurfaceProbed` vs. `TemplatedUnderlyingTokens`) — not at the level of the ~25 finer-grained
techniques cataloged in §2, most of which compose freely within either compiler.

### 5.3 Per-technique composability call, where the evidence supports one

For every technique with more than one grammar in its matrix row (the only ones with direct evidence
either way), an explicit call on "independent, or requires/precludes another" and "would a fifth
grammar want it alone":

- **Static MPR/POS gating (§2.21) — independent, separable.** Requires nothing else in this catalogue
  (it is a pure static partition over the lexicon + a per-group compile, orthogonal to morphotactics,
  phonology-probing, and pruning alike). A fifth grammar with exactly one gated subrule and NO
  templates, NO interdigitation, and NO reduplication would want this alone — that is closer to
  Indonesian's own actual shape than to any constructed hypothetical.
- **Tuple-indexed α-variable resolution (§2.26) — independent, separable.** Lives entirely inside
  `replace.rs`'s rule compiler (Path B) or, for its Path-A analog... *(no Path-A analog exists — see
  the conflict note below)*. A fifth grammar with one heavily alpha-bound rule and nothing else exotic
  would want only this.
- **Morphotactic pruning + `EnumerationBudget` (§2.14/§2.15) — a matched PAIR, not separable from each
  other, but separable from everything else.** The guard is meaningless without the pruning it measures
  around, and the pruning without the guard reintroduced exactly the disaster it doesn't itself fully
  solve (Aweti). But this pair requires nothing about gating, alpha-variables, or which whole-grammar
  compiler is in use — any grammar whose `preexpand`/`struct_extend` search space is large wants both,
  regardless of what else it needs.
- **Junction-probing (§2.10) + deletion-stripped partitions (§2.11) — coupled, not separable from each
  other.** §2.11 is a direct refinement of one specific outcome §2.10 discovers (a root-adjacent
  deletion junction); it has no independent trigger of its own. But this coupled pair is independent of
  reduplication (§2.18), gating (§2.21), and interdigitation (§2.12) — Indonesian needs this pair AND
  §2.18 AND §2.21 simultaneously with no interaction between any two of the three, which is itself mild
  evidence FOR composability (three independently-triggered mechanisms coexisting on one grammar without
  conflict).
- **Reduplication peel (§2.18) — independent, separable, and architecturally CANNOT be replaced by
  anything else in this catalogue.** It sits outside the FST by mathematical necessity (§2.18's own
  entry: unbounded copying is provably non-regular). A fifth grammar could want reduplication peeling
  with zero phonology, zero templates, and zero gating — nothing else in this catalogue is a
  precondition or a substitute.
- **The whole-compiler choice (§2.24/§2.28, Path A vs. Path B) — the one genuine incompatibility.**
  Choosing the templated/cascade compiler for a grammar does not merely ADD a technique, it **removes
  the applicability of** junction-probing (§2.10) and pre-expansion enumeration (§2.12/§2.13) for that
  grammar's phonology/interdigitation, replacing them with the rule-compiler (§2.24) and the templated
  emitter (§2.28) — and, per the negative half of §2.28's own evidence (the `templatic-root-
  modification` fixture), the templated path is NOT yet a safe substitute wherever `emit.rs`'s
  `build_structural_composites` fallback (§2.12) was doing real recall-preserving work for ablaut/
  process morphs. **This means "compose subrecipe X into whichever compiler a grammar uses" is not
  always free** — some subrecipes (the composite-resynthesis fallback specifically) exist in only ONE
  of the two compilers today, so combining "Aweti's templated compiler" with "Amharic's process-morph
  handling" is not yet a supported combination — it would need that fallback mechanism PORTED to the
  templated path first, which is real, uncosted engineering work, not a configuration choice. This is
  the sharpest concrete instance of the coordinator's "complexity must be earned, not assumed" concern:
  a composable-subrecipe design has to budget for exactly this class of cross-compiler porting cost, or
  it will quietly promise combinations it cannot yet deliver.

### 5.4 Honest limits of this evidence

Four grammars is a genuinely thin sample for a claim about combinatorial richness — the strongest
possible counter-reading is that most rows in §5.1 have exactly one non-empty cell simply because each
grammar happens to be the only one exercising its own rare construct, which is equally consistent with
"these techniques are tightly bundled per language family" (if, hypothetically, every construct that
co-occurs in Sena's real grammar always co-occurs with the others in any real language) and with
"these techniques are independent and a fourth data point is not enough to see richer combinations
yet." This audit cannot distinguish those two readings from the matrix's empty cells alone. What DOES
distinguish them, on the evidence actually available, is the **small number of rows with two or more
non-empty cells** (§2.14/§2.15, §2.21, §2.26, §2.29) — each is a case where the SAME code, unmodified,
served two grammars whose OTHER rows share almost nothing, at genuinely different scale parameters.
That is real, if narrow, evidence for the composition hypothesis, not an artifact of small N — small N
would predict FEWER such multi-cell rows, not more, if the underlying truth were bundling. The honest
summary: **the sample is too small to prove rich composability, but the multi-cell rows that DO exist
are the kind of evidence a bundled-recipe world would not produce, and the one clear counter-example
(the whole-compiler choice) is a single, identifiable seam rather than a general pattern** — which
argues for a design with ONE small, fixed menu at the top (which compiler) and freely composable
subrecipes underneath it, rather than either a pure menu-of-five or a fully generic combinator.

---

## 6. Notes on sourcing and honesty gaps

- Several techniques (§2.6/2.16/2.17 compounding, §2.9 normalization) are confirmed as real mechanisms
  and calibrated against synthetic fixtures or general reasoning, but this audit could **not** confirm
  which of the four named grammars specifically exercises them — the source documents read for this
  audit describe the mechanism and its general calibration, not a per-grammar-name confirmation. Marked
  "unconfirmed"/"unverified" above rather than guessed.
- §2.28's two recall figures for Aweti (65/101 → 68/104 in one 2026-07-1x report; 100/106 in the
  2026-07-28 Phase C report) are reported as an **unreconciled discrepancy**, not resolved by this
  audit. The most likely explanation (construct-coverage work landing between the two dates) is stated
  as a hypothesis, not a fact.
- The `docs/fst-plan/` corpus contains several LEGACY documents describing the earlier, fully-sunset
  C# `hc-hybrid` prototype (`HERMITCRAB_FST_ADVISOR.md`, `LEVER_2.md`, and by their own banners several
  others) — explicitly marked superseded in their own headers and excluded from this catalogue's
  "implemented" claims; they are cited only where a current doc/module explicitly ports or references
  their reasoning (e.g. `emit.rs`'s own module doc naming `hc-hybrid/src/trie.rs` throughout).
- `docs/fst-plan/grammar-optimization-techniques.md` is itself explicitly "Research only. Nothing in
  this document is wired into any compile path" (its own header) — it was used in this audit only for
  its "In use" citations, each independently checked against the cited file/line, never for its own
  "Candidate"/"Dead end" recommendations, which belong to Report 2's territory (what we might do), not
  this report's (what we did).
