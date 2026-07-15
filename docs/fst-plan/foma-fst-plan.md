# foma-fst-plan — Integrate foma into PanGloss, sunset the custom-spun FST

Date: 2026-07-15. Status: ACTIVE PLAN.
Basis: reports/01–09 (analysis + adversarial audits), plus three fresh recon passes
(workspace wiring map; foma upstream docs/source grilling, every claim URL-cited;
propose→verify contract spec, every claim file:line-cited). Key citations inline below.

## 0. Goal and non-goals

**Goal:** Replace the custom-spun FST proposer layer (`hc-hybrid`, 12,332 lines) with a
COTS foma network as the proposer, keep the Rust HermitCrab engine as the verifier
(propose→confirm), wire the result into CLI and `hc-wasm`, and delete `hc-hybrid`.

**Non-goals (explicitly out of scope for this plan):**
- Replacing `hc-fst` (2,848 lines). It is NOT the "custom-spun FST" in the product sense —
  it is the acceptor substrate *inside* the retained verify engine: every phonological rule
  match (`hc-rules/src/{rewrite,morph,metathesis,bridge}.rs`) and root-trie lookup
  (`hc-parse/src/root_trie.rs`) runs on it. It only dies if/when the FST-only endgame
  (verification gate, reports/08 §5) retires the whole engine.
- FST-only (no-verify) operation. That remains gated on reports/08 §5; this plan is the
  staging step that shares its compiler work (the emitter built here is the same artifact
  the FST-only endgame needs).
- The C# migration decision. Unaffected; the emitter logic is portable by design.

## 1. Architecture

```
HC XML ──hc-grammar::load──> Grammar
                                │
                    hc-foma::emit(Grammar)          [new crate]
                                │  foma source: lexc lexicon + tags (+ replace rules)
                    foma compile (at grammar load)   [COTS: `foma` crate]
                                │  Net (lexicon ∘ rules), applied UP at runtime
        word ──apply_up──> tag strings ──decode──> Candidates {morphemes, root_index}
                                │
              hc-foma::confirm (port of hc-hybrid replay.rs)
                                │  Morpher::parse_word_selected — pinned re-analysis
                                ▼
              genuine hc_parse::WordAnalysis ──> hc-realize (unchanged)
```

- **Soundness** comes from confirm (the real engine re-derives every emitted analysis);
  the FST only needs **recall** — every true analysis must appear among candidates.
  Over-generation is harmless (fails confirm silently, `replay.rs:118-192`).
- **Iron rule for the emitter: approximate only upward.** Where a construct is hard to
  compile exactly, weaken it into a superset (drop a context, make a rule optional,
  skip a constraint). Never approximate downward — under-generation is a silently lost
  analysis, not an error.
- Per-grammar tiering: a grammar whose foma path fails its parity gate falls back to the
  full engine search (`parse_word_opts`) — shipped behavior today, so fallback = status quo.

## 2. Verified foundations (grilled 2026-07-15)

**Runtime choice — `foma` crate (pure-Rust port), with C foma as oracle + fallback:**
- crates.io `foma` v0.1.1 (2026-07-12), github.com/divvun/foma-rs, Apache-2.0, by
  Divvun (bbqsrc); `mhulden` (foma's author) appears in the contributor list. Claims a
  1:1 port with a 545-test per-function behavioral spec. Pure Rust: lexc/regex parsing via
  sibling crates `nfst-lexc`/`nfst-xre`; gzip via `flate2` (no system zlib); adds a
  from-memory binary loader upstream C foma lacks (`crates/foma/src/io.rs`).
- Risks, verified: 0 stars, ~30 downloads, CI runs ubuntu-latest only — **no Windows or
  wasm32 CI**. Hence gate F0 below; nothing else in this plan starts until F0 passes.
- Upstream C foma (github.com/mhulden/foma @ 0facabc, 2026-03-11): Apache-2.0; CMake with
  real MSVC support for the *library* (readline is CLI-only, zlib required natively);
  official prebuilt Windows binaries exist (v0.10.0 release, MinGW64/MSYS2 — needs the
  bundled msys DLLs); full C API in `fomalib.h` drives lexc/regex/compose/apply without
  the CLI (`fsm_lexc_parse_string`, `fsm_parse_regex`, `apply_init`/`apply_up` with
  NULL-continuation enumeration); flags obeyed by default (`apply.c:283` `obey_flags=1`);
  binary load is filename-only (no buffer API) — one more reason the Rust port wins.
- Official Emscripten wasm build exists in-tree (CMake `if(EMSCRIPTEN)`, exports
  `_apply_up` etc.) — **fallback browser path only**; the primary path is the pure-Rust
  crate linked directly into `hc-wasm`, no second wasm module, no JS glue.

**The propose→verify contract (file:line-verified):**
- Candidate = `{ morphemes: Vec<MorphemeId>, root_index: i32 }`
  (`hc-hybrid/src/walk.rs:218-222`). **Allomorph IDs are NOT part of candidate identity**;
  confirm's owner map only resolves `MorphemeId → LexEntryId | MRuleId`
  (`hc-hybrid/src/replay.rs:82-98`).
- `confirm(g, owners, morpher, candidate, word)` (`replay.rs:118-192`) pins
  `Morpher::parse_word_selected` (`hc-parse/src/morpher.rs:237-246`) to the candidate's
  root(s)+rules; returns a genuine `hc_parse::WordAnalysis` or `None`. Morpher must be
  uncapped (`Morpher::new(g, usize::MAX)`, `replay.rs:106-110`).
- **Positional match trap:** `analyses_match` (`replay.rs:200-208`) is element-wise —
  morphemes in the wrong order or wrong `root_index` = silent loss. The engine's canonical
  order is `allomorphs_in_morph_order` = ascending surface-position order
  (`hc-parse/src/morpher.rs:725-746`); lexc's natural concatenation order matches it for
  prefix/root/suffix. Infix/interleave placement is a named verification point in P3.
- Output type: `hc_parse::WordAnalysis { morpheme_ids, root_morpheme_index, pos_id,
  guessed }` (`hc-parse/src/lib.rs:23-35`); glossing is downstream and unchanged
  (`hc-realize::gloss_bundle` needs only Grammar + WordAnalysis).
- Redup peel is proposer-agnostic: `ReduplicationProposer` (`hc-hybrid/src/proposers.rs:
  145-244`) only needs a `fn(&str) -> Vec<Candidate>` to recurse residuals into.
- Grammar enumeration APIs for the emitter: `Grammar` flat arenas
  (`hc-grammar/src/model.rs:1072-1103`), entries/allomorphs with concrete
  `shape: SegmentedText` (`model.rs:744-787`), affix `OutputAction::InsertSegments`
  concrete text (`model.rs:678-681`), templates/slots (`model.rs:720-737`),
  `allomorph_owners` reverse map (`model.rs:1062-1067`). Pre-probed surface variants come
  from `hc_rules::surface_probe::probe_synthesize` (used today by
  `hc-hybrid/src/surface.rs`, wired at `trie.rs:719-731`).
- **Not compilable as strings:** `OutputAction::Modify/InsertContext` (process morphs,
  ablaut/simulfix — `model.rs:682-685`). Zero uses in the three reference grammars
  (reports/06a); emitter must detect and route the grammar (or the specific rule) to the
  fallback tier rather than silently dropping.

## 3. Design decisions

- **D1 — Runtime = `foma` crate (pure Rust), pinned exact version.** Fallbacks if F0
  fails: (a) fix foma-rs upstream (Apache-2.0, active author, Divvun is a friendly org);
  (b) C foma subprocess `flookup` for native + official Emscripten module for browser
  (protocol verified: stdin line → `word TAB analysis` lines, `+?` on none; multi-net file
  = simulated composition, `-a` = priority union).
- **D2 — Tag alphabet.** Multichar symbols on the analysis tape only:
  `<R:nnnn>` (root morpheme, nnnn = `MorphemeId` index) and `<M:nnnn>` (non-root).
  Declared in lexc `Multichar_Symbols`. Decoder recovers `(Vec<MorphemeId>, root_index)`
  directly; multiple `<R:…>` in one path (compounds) split into one candidate per root,
  mirroring `walk.rs:230-255`.
- **D3 — Emitter strategy (v1): lexc + pre-probed surface variants; replace rules v2.**
  v1 emits pure lexc: root allomorph shapes and affix `InsertSegments` strings as concrete
  entries, with per-affix surface variants and deletion junctions enumerated by
  `probe_synthesize` — the exact trick `hc-hybrid` uses today, so recall is provably ≥
  hc-hybrid's. Allomorph environments/MPR/HeadFeatures are NOT encoded in v1
  (upward approximation; confirm prunes — that is the census-verified cheap direction,
  reports/09 Table 1). v2 (separate milestone, feeds the FST-only gate) emits real foma
  replace rules from prules and flags from constraints.
- **D4 — Multiplicity recovery.** The engine returns a multiset (Sena `mbali`: 8 today in
  Rust, 15 in C# — known engine divergence, orthogonal to this work). Candidates are
  deduped by `(morphemes, root_index)`; for each confirmed candidate, collect ALL matching
  analyses from the pinned `parse_word_selected` outcome (not just the first) to restore
  multiplicity. Distinct candidates yield disjoint matched sequences, so no cross-candidate
  double-count.
- **D5 — Compile at grammar load, from the HC XML.** No new shipped artifact: `emit` +
  foma-compile run inside grammar construction (grammars are 66–1,371 entries vs foma's
  38k-entries-in-1.2s precedent). Optional `.bin` cache later if load time says so.
- **D6 — Reduplication:** port the peel (`proposers.rs:145-244`) with the recursion target
  swapped to the foma proposer. `_eq()`/compile-replace deferred to v2.
- **D7 — Parity oracle = our own full engine** (`parse_word_opts`), because confirm IS our
  engine — the property being tested is exactly "the foma path loses nothing vs full
  search." The C# mbali divergence is tracked separately and does not gate this plan.
- **D8 — Sunset scope:** delete crate `hc-hybrid` + `hc-cli fst-stats` subcommand
  (`hc-cli/src/main.rs:92-157`) + its golden TSVs (`rust/parity-out/golden/fst-advisor/`)
  + its 18 gate-test files, after P3 gates pass. `replay.rs`, `token.rs` decode-order
  logic, and `proposers.rs` peel are ported into `hc-foma` first (with attribution
  comments). `hc-fst` stays (see non-goals). Docs in `docs/fst-plan/` get a status header
  pointing here.

## 4. Phases and gates

Each phase: sonnet subagent(s) implement; main session reviews every diff and runs the
gate personally before the next phase starts.

### P0 — Viability spike (gate F0) — *blocks everything*
New crate `rust/crates/hc-foma` with `foma` pinned; smoke tests:
1. Compile a toy lexc string (multichar tags, 2 continuation classes) via the crate's
   lexc entry point; `apply_up` returns expected tag strings (all paths).
2. Compile a regex replace rule; compose with the lexicon; apply up through composition.
3. Flag diacritics: a `@P.X.Y@`/`@R.X.Y@` pair gates paths correctly under apply-up
   (needed for v2; verify now while choosing the runtime).
4. `cargo test -p hc-foma` green on Windows (this machine).
5. `cargo check -p hc-foma --target wasm32-unknown-unknown` green.
6. Fidelity oracle: official C foma v0.10.0 Windows binary (GitHub release) compiles the
   same sources; `flookup` output set-equal to foma-rs `apply_up` on the toy inputs.
**Gate F0:** all six pass → proceed. Any fail → stop, report, decide fallback (D1) with John.

### P1 — Emitter + tag codec (gate F1)
`hc-foma::emit(Grammar) -> FomaSource` and `decode(tag_string) -> Vec<Candidate>`.
Order of attack (easiest recall first): **Sena** (0 prules, lexc-only, but 1,369 entries +
24 templates) → **Indonesian** (5 prules via pre-probed variants + junctions, 7 redup
words via peel) → **Amharic** (7 prules, 417 segments; if pre-probing explodes or stalls,
tier Amharic to fallback and record why — do not block the plan on it).
Unit tests per construct family; emitted source also compiled by C-foma oracle in a test
behind `--ignored` (uses the P0 binary).
**Gate F1:** all three grammars emit + compile (or are explicitly tiered out with a
reason); every lexc path decodes to a well-formed candidate; morph-order property spot-
checked against engine analyses for 20 hand-picked words incl. prefix+suffix combos.

### P2 — Propose→confirm composite (gate F2)
Port `replay.rs` (confirm, owners, analyses_match), the redup peel, and multiplicity
recovery (D4) into `hc-foma`. Public API:
`FomaAnalyzer::new(&Grammar) -> Result<Self>` and
`analyze_word(&self, word) -> ParseOutcome`-compatible output (same `structured` +
`analyses` shape as `parse_word_opts`, `morpher.rs:79-120`).
**Gate F2:** unit gates for: candidate that should fail confirm (over-gen pruned);
multiplicity on Sena `mbali` == full-engine Rust count (8, both orderings); redup words
round-trip; a `guess_root`-style miss returns empty rather than panicking.

### P3 — CLI wiring + full parity, conformance, and timing gates (gate F3) — *the go/no-go*
`hc-cli`: `--engine=foma` flag on `batch`/`parse` (default remains full engine).

**3a. Corpus parity.** Harness compares foma path vs `parse_word_opts` as multisets keyed
by `(morpheme_ids sequence, root_morpheme_index)`:
- Indonesian: all 121 corpus words — required 100%.
- Sena: sample-300 corpus — required 100% (any shortfall = enumerate + fix or tier out).
- Amharic: corpus words file — measured and reported; 100% required only if Amharic
  passed F1 (otherwise it ships on the fallback tier).

**3b. Conformance suite (machine submodule).** Run the C# conformance driver in adapter
mode against `hc-rs batch --engine=foma` (same path as `rust/tools/run-conformance.sh`,
which builds `hc-cli` release and drives `machine/src/SIL.Machine.Morphology.HermitCrab.
Conformance`). Required: the foma engine's pass/fail set is identical to the default
engine's — zero NEW divergences beyond `rust/tools/known-conformance-divergences.txt`.
Fixtures exercising constructs on a grammar's fallback tier run through the fallback and
must therefore be identical by construction; any diff there indicates a routing bug.
Wire an `--engine` pass-through into `run-conformance.sh` so both runs use one script.

**3c. Timing — "does foma make it faster?"** Same-machine, same-build (release) A/B on all
three grammars, full corpora, both engines, reported as a table in the P3 report and
copied into this plan when done:
- per-word p50 / p95 / max latency, and total corpus wall time, per grammar per engine;
- the named pathological cases individually: Sena's current ~10 s words, Indonesian's
  ~100 ms reduplication words — before/after;
- one-time costs, separated out: grammar load with emit+foma-compile vs load today
  (native), plus candidate-count and confirm-time distributions so we can see WHERE time
  goes if a case is slow;
- batch-mode throughput (words/sec) both engines, single-threaded, to remove rayon noise.
Targets: load < 2 s per grammar (soft); lookup+confirm p95 < 50 ms on Sena's current
10 s words; < 1 ms typical; total corpus wall time strictly faster than the full engine
on Sena and Indonesian. If foma is NOT faster somewhere, that is a reportable finding
with a profile, not something to bury — the sunset case rests partly on this number.

**Gate F3:** 3a parity 100% on non-tiered grammars; 3b zero new conformance divergences;
3c targets met (or explicit signed-off exceptions recorded here); no test regressions
workspace-wide (`cargo test`).

### P4 — hc-wasm integration (gate F4)
`PanGlossGrammar::new` builds `FomaAnalyzer` when the grammar is on the foma tier
(compile failure → automatic fallback to full engine, logged); `analyze_text` routes
through it. Confirm wasm32 build + a browser-side smoke run in PanGloss-demo (sibling
repo — coordinate, don't edit it here beyond what's needed to test).
**Gate F4:** wasm builds; demo analyzes Indonesian + Sena sample text with identical
results to native; bundle-size delta reported (budget: total < 10 MB).

### P5 — Sunset + docs (gate F5)
Per D8: delete `hc-hybrid`, `fst-stats`, fst-advisor goldens + gate tests; workspace
`Cargo.toml` cleanup; status headers on the seven `docs/fst-plan/*.md` legacy docs; this
plan updated to DONE with measured numbers.
**Gate F5:** `cargo test` + `cargo build --release` green workspace-wide;
`run-conformance.sh` re-run BOTH ways (default engine and `--engine=foma`) — default
unchanged, foma still zero new divergences post-sunset;
grep shows zero dangling `hc-hybrid` references; final timing table (3c) pasted into
this plan.

## 5. Commit strategy

The worktree branch (`worktree-fst-investigation`) has zero commits over main; all current
changes (incl. the unrelated `hc-lexicon`/realize work and `reports/`) are uncommitted.
Implementation commits are scoped: stage only `hc-foma`, `hc-cli`/`hc-wasm` touch-points,
and deletions — never sweep in the pre-existing uncommitted work. One commit per gate
passed (F0…F5), so each is independently revertable.

## 6. Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| foma-rs (3 days old) broken on Windows/wasm32 or API-incomplete | medium | F0 gate before anything else; D1 fallbacks (fix upstream / C foma flookup + Emscripten) |
| foma-rs semantic divergence from C foma | low-medium | C-foma oracle cross-check in F0 + F1 |
| Pre-probed variants miss a junction (recall loss) | low (same machinery as hc-hybrid today) | parity gates are exhaustive on corpora, keyed positionally |
| Tag order ≠ engine morph order on infix/edge cases | low | F1 spot-check + F3 positional-multiset parity would catch instantly |
| Amharic pre-probing explodes | medium | Explicit tier-out path; Amharic never blocks sunset |
| Multiplicity mismatch (free fluctuation) | low | D4 collects all matching analyses per candidate; mbali is a named F2 gate |
| Grammar-load compile too slow in browser | low (tiny lexicons) | measure at F3/F4; `.bin` cache via foma-rs memory loader if needed |
