# pg-realize

Turns a `pg-parse` word analysis into a natural-language gloss phrase -- *"in my houses"*, not
just *house-pl-poss.1s* -- as an additive, display-only layer on top of the frozen parity engine.
Design docs: `docs/natural-glosses-plan.md` (the architecture assessment -- especially sections
6-8) and `docs/natural-phrases-plan.md` (the milestone-by-milestone build plan this crate
implements, N0-N3 so far).

## The pipeline (N0 -> N2)

```
WordAnalysis            GlossBundle              GlossIr                 Realization
(pg-parse, frozen) --N0--> (tokens, root,   --N1--> (Concept, Num,  --N2--> (text, complete,
                            gloss/props)             Poss, CaseRole,         residue)
                                 |                    extras)                    |
                                 v                                               v
                         leipzig() string                                  "in my houses"
                         (always-available                                 (English, via
                          fallback)                                         TableRealizer)
```

- **N0** (`gloss_bundle`, `leipzig`, this crate's root module): resolves a `WordAnalysis`'s
  grammar-tier morpheme ordinals against `Grammar::morphemes` into a `GlossBundle`, and renders
  it as a Leipzig-style gloss string. This is the floor every later stage can fall back to --
  nothing downstream can ever produce *less* information than this.
- **N1** (`ir`, `map`): maps a `GlossBundle`'s raw, free-form gloss strings (grammar-author data,
  `pl`/`PL`/`poss.1s`/whatever a FLEx author typed) into `GlossIr`, a closed-enum typed
  intermediate representation (`Concept`, `Num`, `Poss`, `CaseRole`, plus an `extras` fallback
  bucket). The mapping source is, in priority order: a morpheme's own `realize` property, then a
  per-grammar sidecar TOML file (`map::RealizeMap`), then unmapped (-> `extras`). Total function,
  never fails.
- **N2** (`realize`, `table`): the `Realizer` trait (`fn realize(&self, ir: &GlossIr) ->
  Realization`) plus `TableRealizer`, the one implementation shipped so far -- a compile-time
  English construction table loaded from two embedded (`include_str!`) assets under
  `assets/eng/`: `templates.toml` (108 `(CaseRole, Poss, Num)` cells, e.g. `"Loc.P1Sg.Pl" = "in my
  {n:pl}"`) and `lexicon.toml` (irregular English plurals). `TableRealizer::new()` validates full
  108-cell coverage at load time -- a bad or incomplete asset fails the constructor (and CI, via
  `table::tests::assets_load_and_cover_all_108_cells`), never a runtime panic.
- **N3** (this milestone, `gf/`): the honest GF grammar sources that *define* those 108 cells, so
  `templates.toml` is no longer hand-authored truth but a mechanically regenerable artifact.

## The GF regeneration loop (N3)

`gf/` holds a small GF application grammar over the RGL (Grammatical Framework / Resource Grammar
Library), written as a functor so adding a language beyond English is a lexicon module plus a
~5-line functor instantiation, not a rewrite (`docs/natural-glosses-plan.md` section 8,
Architecture B: "GF as build tool" -- GF runs only at authoring time, nothing GF-shaped ships at
runtime):

- `Gloss.gf` -- abstract syntax: typed constructions mirroring `src/ir.rs`'s `GlossIr` exactly
  (one category per feature slot: `Case`, `Poss`, `GNum`; `GlossPhrase : Case -> Poss -> GNum ->
  NConcept -> Gl` combines all three plus the noun concept).
- `LexGloss.gf` / `LexGlossEng.gf` -- the per-language lexicon interface and its English instance
  (the placeholder noun `n_N` -- `mkN "house"` for English -- and the three case prepositions).
- `GlossFunctor.gf` -- the construction logic, written once against the real RGL API (`mkNP`,
  `mkCN`, `mkQuant`, `mkAdv`, `mkUtt`, `sgNum`/`plNum`, `a_Quant`, the RGL pronoun constants),
  never against an invented one -- see that file's header for exactly which opers were verified
  against the real `gf-rgl` source and why the functor's parameters are named `Grammar`/
  `Constructors`/`LexGloss` rather than `Syntax`/`SyntaxEng` (real, but build-generated API
  wrappers shipped with installed RGL distributions rather than checked-in gf-rgl sources).
- `GlossEng.gf` -- the ~2-line functor instantiation for English.
- `gen_templates.py` (stdlib-only Python) -- given a working `gf` install, compiles `GlossEng.gf`,
  enumerates all 108 `(Case, Poss, GNum)` trees applied to `n_N`, linearizes them via the GF
  shell, substitutes the placeholder word back out for `{n:sg}`/`{n:pl}` slots, and rewrites
  `assets/eng/templates.toml`.

**As of 2026-07-11 there was no `gf` install on the development machine that wrote these
sources**, so none of the `.gf` files had been compiled or run by hand -- only syntax-checked
(`gf/gen_templates.py` is plain-Python-syntax-checked; the `.gf` files were designed by reading
the real `gf-rgl` source directly, not by compiling against it). **As of 2026-07-13,
`.github/workflows/gf-ci.yml` closes that verification gap in CI:** it installs the official GF
3.12 Ubuntu package, sparse-checks-out the `gf-rgl` subtrees `GlossFunctor.gf` opens (`abstract`,
`api`, `common`, `english`, `prelude` at pinned tag `20260403`), and runs `gf --make GlossEng.gf`
on every push/PR that touches `gf/` -- so a broken construct in these sources now fails CI
instead of shipping silently wrong. That job compiles the grammar only; it does not run
`gen_templates.py` or commit its output, so `templates.toml` remains the committed,
hand-authored source of truth until someone actually runs the generator. The loop below is what
closes that second gap:

1. Edit the `.gf` sources in `gf/` (the construction logic, the lexicon, or both).
2. Run `python gen_templates.py --gf gf --out ../assets/eng/templates.toml` from `gf/` (with a
   local `gf` install, or by adapting `gf-ci.yml`'s install steps).
3. Commit the regenerated `templates.toml` alongside the `.gf` source change.
4. `table::tests::assets_load_and_cover_all_108_cells` (N2, already in the suite) guards the
   invariant the generator is responsible for maintaining -- full 108-cell coverage, exactly one
   `{n:sg}`/`{n:pl}` slot per cell -- so a bad regeneration fails `cargo test -p pg-realize`, not
   a silent drift.

`gf-ci.yml`'s first real run (2026-07-13) did turn up two genuine bugs, both fixed the same day
(see `gf/GlossEng.gf` and `gf/LexGlossEng.gf`'s own headers for the specifics): the `with (...)`
functor-instantiation clause needed one parenthesized `(Interface = Instance)` group per binding
rather than one comma-separated group, and `LexGlossEng.gf` needed `GrammarEng` opened alongside
`ParadigmsEng` for its `N`/`Prep` types to resolve. `GlossFunctor.gf`'s own three-way
`Grammar`/`Constructors`/`LexGloss` parameter combination -- the spot flagged as highest-risk --
compiled clean on the first try. The grammar links into a `.pgf` successfully as of that run.

## The Architecture-A upgrade path

`realize::Realizer` is the seam this was all built around: `TableRealizer` (Architecture B,
compile-time GF / runtime tables) is one implementation. A future Architecture A implementation
(an embedded PGF runtime -- `gf-core` or a `libpgf` binding -- linearizing directly against these
same `Gloss.gf`/`GlossEng.gf` sources at runtime instead of a pre-generated table) can slot in as
a second `Realizer` impl with **no change to any caller** -- `pg-cli`'s `--natural-gloss` wiring
programs against the trait, never against `TableRealizer` directly. See
`docs/natural-glosses-plan.md` section 8 for the full three-architecture comparison (B was chosen
ship-first specifically because it has zero runtime dependency and removes the one fatal risk
class -- an unproven Rust PGF runtime -- entirely; A stays a pure upgrade on the same grammar
source once/if the construction inventory outgrows a finite table).

## Testing

```sh
cargo test -p pg-realize
```

Unit tests live alongside each module; integration gates live under `tests/` (`n0_gloss_gate.rs`,
`n1_ir_gate.rs`, `n2_realize_gate.rs`) and exercise real `samples/data/*-hc.xml` grammars end to
end, including the flagship *"in my houses"* assertion and a corpus robustness sweep (all of
Indonesian, deterministically subsampled Amharic/Sena — see the test's own doc comment — with and
without a sidecar map: never panics, never empty output, parity signature unchanged).
