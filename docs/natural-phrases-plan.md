# Natural Phrases — Implementation Plan (Architecture B, ship-first)

**Status:** approved for build 2026-07-11, on branch `natural-phrases` (worktree
`.worktrees/natural-phrases`). Design rationale: `docs/natural-glosses-plan.md` (esp. §6–§8).
This plan implements **Architecture B** (compile-time GF, runtime tables) scoped to a
demoable end-to-end pipeline, with the grammar source and trait boundaries laid so
Architecture A (embedded PGF runtime) stays a pure upgrade.

## Non-negotiable constraints

1. **Parity is untouchable.** `result_signature`, `ParseOutcome.analyses`, the batch TSV
   protocol, and everything `hc-ffi` encodes stay byte-identical. All new output is additive
   and behind new flags. The full existing test suite must stay green.
2. **No changes to `hc-hybrid`** (F9 is in flight on `main`). Core crates (`hc-parse`,
   `hc-grammar`, `hc-rules`, …) may gain at most small `pub` read-only accessors, and only if
   actually needed (currently believed unnecessary: `Grammar::morphemes`,
   `MorphemeInfo::{gloss, properties}`, `ParseOutcome::structured`, `WordAnalysis` are
   already `pub`).
3. **Pure Rust, no `build.rs`, no new external deps** beyond what the workspace already pins
   — with one allowance: a tiny TOML parser is needed for mapping/table files. Prefer
   `toml` (widely used) added to `[workspace.dependencies]`; if avoiding even that, use a
   restricted hand-rolled reader — decide in N1, document the choice.
4. **Graceful degradation everywhere.** No input — unmapped gloss, missing lexicon entry,
   guessed root, weird bundle — may ever panic or produce empty output. The floor is always
   the Leipzig gloss line from N0.
5. Match repo idiom: doc comments citing plan sections, gate-style integration tests,
   `cargo fmt` limited to files this work owns.

## Repo facts the implementer needs (verified 2026-07-11)

- Parse API: `hc_parse::Morpher::new(&grammar, usize::MAX)`, `.parse_word(word) ->
  ParseOutcome`. `ParseOutcome.structured: Vec<WordAnalysis>` mirrors `.analyses` index-for-
  index (`morpher.rs:77-96`).
- `WordAnalysis { morpheme_ids: Vec<u32>, root_morpheme_index: i32, pos_id: Option<u32>,
  guessed: bool }` (`hc-parse/src/lib.rs:23`). `morpheme_ids` are **grammar-tier ordinals**
  — dense indices into `Grammar::morphemes: Vec<MorphemeInfo>` (`model.rs:1068`) — except
  `u32::MAX` = guessed root sentinel (`MorphemeId::GUESSED`).
- `MorphemeInfo { gloss: Option<String>, properties: Vec<(String,String)>, … }`
  (`model.rs:475-487`).
- CLI: `hc-cli/src/main.rs`, hand-rolled arg parsing (no clap), subcommands `parse` /
  `batch` / `fst-*`; `run_parse` at `main.rs:164` is the wiring point.
- Sample grammars (`samples/data/`): **amharic** (nominal demo: glosses `pl`, `poss.1s` …
  `poss.3p`, adpositions `at`/`from`/`to`, `abl`, verb features), **indonesian** (English
  lexical glosses `read`/`sell`/`teach`…, affix glosses `Caus`/`APPL`/`AV`/`NMLZR`/`RECIP`
  — mostly Leipzig-fallback territory), **sena** (Portuguese glosses + noun-class digits —
  pure fallback; good robustness corpus).
- The GF compiler is **not installed** on this machine: N3's generator must be runnable-
  later, with hand-authored tables as the committed source of truth for now.

## Milestones

### N0 — `hc-realize` crate + gloss bundle layer

New workspace crate `rust/crates/hc-realize` (member + `[workspace.dependencies]` entry),
depending on `hc-grammar` + `hc-parse` only.

- `pub struct GlossBundle { pub root: GlossToken, pub tokens: Vec<GlossToken>, pub pos: Option<String>, pub guessed: bool }`
  where `GlossToken { pub gloss: Option<String>, pub properties: Vec<(String,String)>, pub is_root: bool }`
  and `tokens` preserves morpheme order (root included at its position).
- `pub fn gloss_bundle(grammar: &Grammar, wa: &WordAnalysis) -> GlossBundle` — resolves
  ordinals via `Grammar::morphemes`, maps the `u32::MAX` sentinel to a root token with
  `gloss: None` (callers render it from the surface word).
- Leipzig rendering: `pub fn leipzig(bundle: &GlossBundle, surface_word: &str) -> String`
  → `house-pl-poss.1s` style (root gloss or `*{surface_word}*` when guessed; missing gloss
  → the morpheme's `morph_id`/`xml_key` in brackets).
- CLI: `hc-rs parse <grammar> <word> --gloss` prints one extra tab column per analysis line
  (or per-analysis lines — match existing output style; parity line unchanged).
- Tests: unit tests on synthetic bundles + an integration gate parsing real amharic +
  indonesian + sena words, asserting exact Leipzig strings, and asserting the parity
  signature is unchanged with/without the flag.

### N1 — IR + per-grammar mapping

In `hc-realize`:

- `pub struct GlossIr { pub concept: Concept, pub num: Num, pub poss: Poss, pub case: CaseRole, pub extras: Vec<String> }`
  with `Concept::{Lex(String), Guessed(String)}`, `Num::{Unspec,Sg,Pl}`,
  `Poss::{None,P1Sg,P2Sg,P3Sg,P1Pl,P2Pl,P3Pl}` (+ gender-marked variants as plain data,
  e.g. `P3SgF` — mirror what amharic actually distinguishes: `poss.2f`, `poss.3m`, …),
  `CaseRole::{None,Loc,Abl,All}`. `extras` collects mapped-but-unrealized and unmapped
  tokens for fallback display. Keep it a struct of closed enums — **no** stringly features.
- Mapping sources, priority order: morpheme property `realize = "<Feature>:<Value>"` →
  sidecar TOML → unmapped (goes to `extras`).
- Sidecar format `samples/data/<grammar>-realize.toml`:
  `[features] "pl" = "Num:Pl"`, `"poss.1s" = "Poss:P1Sg"`, `"at" = "Case:Loc"`,
  `"from" = "Case:Abl"`, `"to" = "Case:All"`, … Write the real file for **amharic**;
  indonesian gets a minimal one (roots pass through; `Caus` etc. stay extras); sena none
  (pure fallback path must still work).
- `pub fn to_ir(bundle: &GlossBundle, map: &RealizeMap, surface: &str) -> GlossIr` — total
  function, never fails.
- Tests: mapping-priority unit tests; amharic integration test producing expected IRs.

### N2 — Table realizer + CLI wiring (the demo)

- `pub trait Realizer { fn realize(&self, ir: &GlossIr) -> Realization; }` with
  `Realization { pub text: String, pub complete: bool, pub residue: Vec<String> }`
  (`complete=false` ⇒ caller appends/uses Leipzig fallback; `residue` = unrealized extras).
- `TableRealizer` loading two embedded (via `include_str!`) English assets under
  `rust/crates/hc-realize/assets/eng/`:
  - `templates.toml`: key = `(case, poss, num)` construction cell, value = template with
    `{n}` slots, e.g. `Loc.P1Sg.Pl = "in my {n:pl}"`, `None.None.Sg = "a {n:sg}"`,
    `Abl.P3Sg.Pl = "from his/her {n:pl}"`. Enumerate the **full** 4×8×3 space (~96 cells;
    many share strings — TOML can stay explicit, it is generated later by N3).
  - `lexicon.toml`: irregular plural exceptions (`house` is regular; include the standard
    irregulars: man/men, child/children, foot/feet, …) + an English regular pluralizer in
    Rust (`-s/-es/-ies` rules) as the default. Multi-word glosses (`treat.someone`,
    `soar skyward`) and `Guessed` concepts: substitute verbatim (dots → spaces), pluralize
    the final token only when regular, else mark `complete=false`.
- Verb/POS guard: N2 realizes **nominal** IRs only; a bundle whose extras look verbal or
  whose construction cell is missing → straight fallback. No verb templates in scope.
- CLI: `hc-rs parse <grammar> <word> --natural-gloss=eng [--realize-map=<path>]` — prints
  per analysis: parity signature line (unchanged) + `gloss:` line (N0) + `eng:` line. Map
  path defaults to `<grammar-path stem>-realize.toml` next to the grammar if present.
- Tests:
  - Gate test: amharic demo words end-to-end — a possessed-plural-locative noun must render
    like *"in my houses"* (pick real words from `amharic-words.txt` and pin exact expected
    strings after inspecting actual parses; the test documents the full chain).
  - Robustness gate: for **every** word in all three `samples/data/*-words.txt`, with and
    without sidecars: never panics, never empty output, parity signature unchanged.
    (This is the plan's property test, run as a plain exhaustive loop — the corpora are
    small.)

### N3 — GF sources + regeneration path (reproducibility, not runtime)

- `assets/gf/` at repo root (or `rust/crates/hc-realize/gf/` — implementer picks, document
  it): `Gloss.gf` (abstract: the typed construction inventory matching N1's IR exactly),
  `GlossFunctor.gf` (concrete written once over the `Syntax` interface), `GlossEng.gf`
  (functor instantiation), `LexGloss.gf`/`LexGlossEng.gf` (placeholder lexeme `n_N`).
  Must be honest GF (real RGL API — `mkNP`, `mkQuant`, `mkAdv`, `mkPrep`, `sgNum`/`plNum`);
  it cannot be compiled in this environment, so mark it clearly as "not yet CI-verified".
- `tools/gen_templates.py` (or a `cargo run -p hc-realize --bin gen-templates` guard-railed
  stub): given a working `gf` install, enumerates the construction space, linearizes with
  the placeholder lexeme, and rewrites `templates.toml`. Documented in the crate README;
  a test asserts `templates.toml` parses and covers the full construction space (the
  invariant the generator would guarantee).
- Doc updates: `docs/natural-glosses-plan.md` gets a short "shipped: N0–N3, see
  natural-phrases-plan.md" status note; crate-level `//!` docs explain the A-upgrade path
  (swap `TableRealizer` for a PGF-backed `Realizer` impl).

### N4 — Finishing

`cargo fmt` (own files only), `cargo clippy -p hc-realize -p hc-cli`, full workspace
`cargo test`, update this plan's status line with shipped scope + known gaps. Commit
history: one commit per milestone, messages `N0: …` style, co-authored-by trailer per repo
convention.

## Agent execution notes

- One sonnet agent per milestone, sequential (N0 → N1 → N2 → N3 → N4), each starting from
  the previous milestone's committed state in the worktree; orchestrator reviews the diff
  between milestones.
- Each agent: read this plan + `docs/natural-glosses-plan.md` §6–§8 first; run
  `cargo test -p hc-realize` plus touched-crate tests before declaring done; full-suite run
  happens at N2 and N4.
- Anything discovered that contradicts this plan (e.g. amharic parses don't produce the
  expected bundles): stop, record the discrepancy in the milestone commit message and this
  file, and adjust the demo target rather than forcing the pinned strings.
