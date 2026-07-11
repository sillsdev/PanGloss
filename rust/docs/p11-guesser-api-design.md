# P11 — Guesser API (`guessRoot`/`LexicalGuess`) port: design

Status: **design only** (plan `rust-optimizations-phase2.md` §P11, [FABLE-PLAN then SONNET]).
Decided in scope 2026-07-10 (Open scope decisions #1: "PORT IT"). No engine code changes accompany
this doc. Target implementer: a Sonnet-tier agent working mechanically from §5-§7.

Oracle: `.worktrees/parse-opt/src/SIL.Machine.Morphology.HermitCrab/Morpher.cs` (the live C#
engine; guess subsystem identical to master — the whole cluster is HISTORY-MATRIX rows 12, 20, 24,
25, 26, 35, 39, 40, all `[guesser, non-goal]` until now). Literal test oracle:
`AnalyzeWord_CanGuess_ReturnsCorrectAnalysis` + `TestMatchNodesWithPattern`
(`.worktrees/parse-opt/tests/SIL.Machine.Morphology.HermitCrab.Tests/MorpherTests.cs:242-274,
349+`).

---

## 1. What the C# does, read end-to-end

### 1.1 Where lexical patterns come from (grammar model + loader)

There is **no dedicated XML construct**. A lexical pattern is an ordinary
`<LexicalEntry>/<Allomorphs>/<Allomorph>/<PhoneticShape>` whose text uses the pattern language,
because `LoadRootAllomorph` is the **only** `new Segments(table, str, allowPattern: true)` call
site (`XmlLanguageLoader.cs:501`). The pattern language (`CharacterDefinitionTable.GetShapeNodes`,
cs:108-219, consulted only where literal longest-match fails):

- `[Class]` — a natural-class node (FS = the class's FS, no `StrRep` for `FeatureNaturalClass`;
  member-`StrRep`-union for `SegmentNaturalClass`);
- `([Class])` — same node marked `Annotation.Optional` (only a single class inside parens is
  legal; `([C][V])` is ill-formed);
- `[Class]*` — same node marked Optional **and** iterative (`SetIterative(true)`, stored in
  `Annotation.Data`; `HermitCrabExtensions.cs:162-173`). Kleene `+` is impossible ('+' is a
  boundary marker).

`RootAllomorph`'s constructor derives the classification (`RootAllomorph.cs:16-29`):
**IsPattern = any node is iterative OR (optional AND not a boundary)**. Note carefully: a bare
`[Class]` node (mandatory, non-iterative) does **not** make a pattern — such an allomorph is a
normal root, trie-indexed, already handled by the Rust wave-4 `CdSet` trie edges.

`Morpher`'s constructor partitions (`Morpher.cs:74-85`): `IsPattern` allomorphs are diverted into
one flat `_lexicalPatterns` list — **across all strata** — and are **NOT added to the trie**.
Everything else about them still loads normally (`LoadRootAllomorph` gives them environments,
stem name, properties; the post-strata pass gives them co-occurrence rules; their owning entry has
syn FS, MPR features, morpheme co-occurrence rules, `IsPartial`, family).

### 1.2 `ParseWord`'s guess branch (`Morpher.cs:235-307`)

```
analyses  = _analysisRule.Apply(input).ToList()          // unchanged
origAnalyses = guessRoot ? analyses : null               // the RAW analysis candidates
syntheses = Synthesize(word, analyses).ToList()          // unchanged (normal lexicon path)
if (guessRoot && syntheses.Count == 0):                  // guess ONLY on a total miss
    matches = []
    foreach analysisWord in origAnalyses:
        foreach synthesisWord in LexicalGuess(analysisWord).Distinct():
            foreach alternative in synthesisWord.ExpandAlternatives():
                foreach validWord in _synthesisRule.Apply(alternative).Where(IsWordValid):
                    if IsMatch(word, validWord): matches.Add(validWord)
    matches.Sort((x, y) => y.Morphs.Count().CompareTo(x.Morphs.Count()))   // DESC by morph count
    return matches
return syntheses
```

Details that matter for parity:

- **`origAnalyses`** is just the materialized analysis list reused (no re-analysis, no copy —
  the comment at cs:279 says `Synthesize` doesn't mutate it). Rust's `results` map is the same
  set.
- **Ordering is observable.** The descending-`Morphs.Count()` sort is exactly why the C# unit
  test sees `[*gag ed_suffix]` (2 morphs) at `analyses2[0]` for "gagd" rather than the bare
  1-morph `[*gagd]` guess that is *also* produced (a `[Any]*` pattern matches the un-unapplied
  analysis candidate too). `List.Sort` is unstable; tie order is unspecified. Batch signatures
  re-sort lexically, so this only matters for `AnalyzeWord` enumeration order.
- **No dedup of `matches`** — unlike the normal path's `HashSet`/`Distinct` — duplicates from
  different analysis words survive into the output (and `result_signature` deliberately keeps
  duplicates, `hc-parse/src/lib.rs:33-40`).
- **`.Distinct()` on `LexicalGuess(...)`** uses default equality. `Word` does not override
  `Object.Equals` (that is what `FreezableEqualityComparer<Word>` exists for, and it is *not*
  passed here), and every yielded guess is a freshly constructed clone — so this `Distinct()` is
  a de-facto no-op; the real dedup is `LexicalGuess`'s per-pattern `shapeSet` (§1.3). Implementer:
  treat it as a no-op; flag in review if an oracle fixture ever contradicts this.
- **Phase-5 lexical gating** (`lexicalGatingActive = ... && !guessRoot`, cs:246-252) is a
  parse-opt-only, default-off optimization with **no Rust counterpart** — nothing to port; the
  comment just documents why guessing (which synthesizes from patterns, not the real lexicon)
  must never run under a real-lexicon reachability gate. The M6 memo (`AnalysisScope`) is
  orthogonal: it memoizes analysis, which completes before the guess branch runs; the guess
  branch itself re-runs only synthesis. No memo change needed.
- **Trace gating**: `LexicalGuess` fires `_traceManager.LexicalLookup(input.Stratum, input)` once
  per analysis word and `SynthesizeWord` per fabricated word — trace-only, no behavioral effect
  when not tracing. Tracing is out of scope for the port (rust-conversion.md §7), so this whole
  dimension drops out.
- **`MaxUnapplications`** (added by the guesser origin commit `3b4f8441`) is already ported
  (`AnalyzerConfig.max_unapplications`, default 0 = unlimited, matching `Morpher.cs:109` /
  `AnalysisStratumRule.cs:181`). Not P11 work.

### 1.3 `LexicalGuess` (`Morpher.cs:522-590`) + `MatchNodesWithPattern` (cs:597-647)

Per analysis word, per lexical pattern:

1. `table = input.Stratum.CharacterDefinitionTable` — the **analysis word's** stratum table is
   used for matching/rendering, even though patterns from all strata are tried.
2. `MatchNodesWithPattern(inputNodes, patternNodes)` — a direct recursive matcher (C# comment:
   deliberately NOT `Pattern<Shape,ShapeNode>`/Matcher, "the Matcher doesn't preserve the
   unifications of the nodes"). Semantics, ported literally:
   - `inputNodes` = **all** of the analysis shape's nodes (segments AND boundaries; boundaries
     only drop out later, at rendering). The match must consume the **entire** input — this is a
     whole-shape match, not a substring search (`pattern.Count == p` accepts only when
     `nodes.Count == n`).
   - Input-side `Optional`/iterative flags are **ignored** — every input node must be consumed.
     Only pattern-side flags have skip/repeat semantics.
   - `pattern[p].Optional && !obligatory` → branch: skip pattern item. `obligatory=true` is
     passed only by the iterative-reuse branch, suppressing skip-of-`p` immediately after
     consuming at `p` via iteration (derivation dedup — skip-after-iterate would duplicate the
     consume-then-advance path).
   - Consume: `UnifyShapeNodes(nodes[n], pattern[p])` — full-FS unification; on success the
     matched node is the input node itself if unification added nothing, else a fresh node with
     the unified FS (class features narrow underspecified input nodes; kind/`Type` mismatch —
     boundary vs segment — fails unification). Then branch to `(n+1, p, obligatory=true)` if
     `pattern[p]` is iterative, and always to `(n+1, p+1, false)`.
   - Result: a list of matched-node lists (multiple paths through `([Seg])([Seg])` etc.).
3. Render each match: `match.ToString(table, false)` (`HermitCrabExtensions.cs:317-335`): skip
   boundary and deleted nodes; per node append `GetMatchingStrReps(node).FirstOrDefault()` — the
   **first** table char-def (table iteration order; insertion order in practice) whose full FS
   unifies with the node, first representation. (`GetShapeStrings`' cross-product alternative
   exists but is deliberately unused — "spurious ambiguities", cs:536-538.)
4. Dedup **per pattern** by rendered string (`shapeSet` — HISTORY row 12 `73a8d852` moved it
   inside the pattern loop: two *different* patterns producing the same string must each yield a
   guess — cross-pattern homographs are real).
5. Fabricate the root: `new RootAllomorph(new Segments(table, shapeString)) { Guessed = true }` —
   a **concrete** re-segmentation of the rendered string — copying from the pattern allomorph:
   `AllomorphCoOccurrenceRules`, `Environments`, `Properties`, `StemName`, `IsBound`.
6. Fabricate its `LexEntry`: `Id = Gloss = shapeString`; initialized from the input word
   (`IsPartial = input.SyntacticFeatureStruct.IsEmpty`, `SyntacticFeatureStruct = input's`,
   `Stratum = input.Stratum`) — then, whenever the pattern has an owning morpheme (**always**, for
   any loader-built or test-built grammar), overwritten from the pattern's entry:
   `MorphemeCoOccurrenceRules`, `Properties`, `Stratum`, `MprFeatures`, `SyntacticFeatureStruct`,
   `IsPartial`. Net effect: the fabricated entry ≡ the pattern's owning entry, except `Id`/`Gloss`
   (= the guessed string) and the allomorph list (exactly one: the fabricated root).
7. `newWord = input.Clone(); newWord.RootAllomorph = root` — the `RootAllomorph` setter
   (`Word.cs:148-169`) replaces the shape with the fabricated root's shape and pulls
   stratum/syn-FS/MPR/`IsPartial` from the fabricated entry. The clone drops `Alternatives` but
   records `Source = input` — so `ExpandAlternatives()` at the call site recovers merged
   equivalent analyses, exactly as `SynthesizeAnalysis` does for real lexical lookups.

The fabricated word then flows through the **ordinary** synthesis cascade — same code path as a
real root, no special-casing downstream.

### 1.4 The `Guessed` flag

`Allomorph.Guessed` (`Allomorph.cs:78`) is set in exactly one place (`Morpher.cs:546`) and **read
nowhere in the library** — it has zero effect on parse results, `WordAnalysis`, or the batch
signature. (The `*` in the test's `[*gag]` is `WordAnalysis.ToString`'s root-*index* marker,
`WordAnalysis.cs:63-69`, not a guessed marker.) Its consumer is FieldWorks, via the API surface.
Consequence for the port: `guessed` must be *representable and queryable* (FFI/structured output)
but must not perturb any existing output byte.

### 1.5 The literal test oracle

`AnalyzeWord_CanGuess_ReturnsCorrectAnalysis` (`MorpherTests.cs:242-274`): standard test base +
`ed_suffix` (`V`, RHS `+d` via Table3), a `NaturalClass "Any"` with an **empty** FS added to the
Morphophonemic table, and `AddEntry("pattern", new FeatureStruct(), Morphophonemic, "[Any]*")`
(so the pattern entry has empty syn FS ⇒ `IsPartial = true`). Asserts:

| call | expected |
|---|---|
| `AnalyzeWord("gag")` (no guess) | empty |
| `AnalyzeWord("gagd")` (no guess) | empty |
| `AnalyzeWord("gag", true)[0]` | `[*gag]` |
| `AnalyzeWord("gagd", true)[0]` | `[*gag ed_suffix]` (the 2-morph guess sorts before the also-produced 1-morph `[*gagd]`) |

`TestMatchNodesWithPattern` (`MorpherTests.cs:349+`) unit-tests the matcher directly (optional /
iterative / unification cases) — port its cases as Rust unit tests on the new matcher.

---

## 2. Current Rust state

What already exists (do not rebuild):

- **Pattern segmentation is fully ported** (finding N3): `hc-grammar/src/segment.rs::
  segment_with_patterns` — `[Class]`, `([Class])`, `[Class]*`, error cases — and
  `load.rs::load_root_allomorph` (line 1854) already routes every root-allomorph
  `<PhoneticShape>` through it. Pattern nodes come out as `NO_CHAR_DEF` segments carrying the
  class's member `CdSet` and class lanes, with `NodeFlags::OPTIONAL`/`ITERATIVE`
  (`hc-shape/src/lib.rs:156-172, 402-404`).
- **`RootAllomorphDef`** (`hc-grammar/src/model.rs:697-711`) already holds everything
  `LexicalGuess` copies: `is_bound`, `environments`, `co_occurrence`, `properties`, `stem_name`.
- **The trie already has the class-edge machinery** (`hc-parse/src/root_trie.rs` wave-4
  `CdSet` edges) for *non-pattern* class-bearing roots like `b[Vowel]t` — that stays.

The one live **divergence**: `RootAllomorphTrie::build` (`root_trie.rs:113-138`) indexes ALL root
allomorphs, with an explicit doc note that the `IsPattern` branch was out of scope. Since stored
`OPTIONAL`/`ITERATIVE` flags are deliberately not modeled on trie edges, a `[Any]*` entry today
becomes a single mandatory unrestricted edge — i.e. **a lexical pattern currently matches any
one-segment word in normal (guess-off) lexical lookup**, where C# would never surface it at all.
Any pattern-bearing FieldWorks grammar hits this. P11 step 1 fixes it as a side effect of the
faithful partition.

Plumbing facts that shape the design: `hc_rules::word::Word.root_allomorph` is
`Option<AllomorphId>`; morphs are `MorphRecord { allomorph, morpheme, order, .. }`; every
downstream consumer resolves ids through the immutable `Grammar` (`allomorph_owners`, `entries`,
`morphemes`) — `validity.rs::allomorphs_valid_impl:419`, `morpher.rs::morpheme_join:394`,
`structured_analysis:414`. A guessed root exists in no grammar table, so id resolution is the
crux (§4.4).

---

## 3. P10 `StrRep`-identity lane: how it interacts (answer: it doesn't, by construction)

The P10 lane (`hc-rules/src/bridge.rs:138-178`) is an **FST-compile-time** device: an extra
synthetic lane holding a char-def membership bitset, opt-in per compile site, exact only for
≤64-def tables, with the hard pairing rule that id-lane FSTs may only receive id-lane inputs.

The guess matcher must **not** go through `hc-fst` at all — C# itself refuses the Matcher here
because it "doesn't preserve the unifications of the nodes" (`Morpher.cs:138-140`), and the port
should follow: a direct recursive matcher over `Shape` nodes (§4.3). On that path the identity
dimension P10 restored for FSTs is already first-class and needs no lane:

- node identity = `char_def` (the `StrRep` analog, per `root_trie.rs`'s module doc);
- class membership = the pattern node's stored `CdSet` (`Shape::node_cd_set`), which is
  arbitrary-width (`CdBits` = `SmallVec<[u64;1]>`, `hc-shape/src/lib.rs:53`) — so guess matching
  is **exact even for >64-def tables (Amharic)**, unlike the id-lane's ≤64 bound. No wholesale
  disable, no over-approximation arm;
- phonological refinement = `flat_unifiable` on node lanes.

This is exactly `root_trie.rs::edge_matches`' predicate (concrete → char-def equality +
lane-unify; pattern edge → `CdSet` membership + lane-unify; `NO_CHAR_DEF` query → wildcard the
identity gate, lanes still gate) — reuse that shape of logic, plus §1.3's kind check (boundary vs
segment) which the trie never needed (it filters to segments; the guess matcher must not, §1.3
step 2).

One implementation-time check, not a design risk: analysis-output shapes never *store* the id
lane (it is added transiently on FST inputs by `morph::segs_of`), but mirror
`root_trie.rs::shape_search_segments`' width check (`shape.feat_width() == phon_features.len()`)
when reading node lanes, so a differently-widthed shape falls back to table lanes.

P5 note: the P5 design (over-extended `StrRep` model — char-def FSs carry `StrRep` only in
zero-phon-feature grammars) applies to the *rendering* side here too. `GetMatchingStrReps` in C#
unifies full FSs, so which char-defs "match" a node depends on whether the grammar's char-defs
carry `StrRep`. The port's `surface.rs::matching_str_reps` (node `cd_set` + lanes) is the settled
analog — the guess renderer must reuse it (§4.3), inheriting whatever P5 lands there rather than
inventing a third identity model.

---

## 4. Design

### 4.1 API shape (Rust surface)

- `Morpher::parse_word(&self, word)` — unchanged, byte-identical (guess off). It becomes a thin
  wrapper over the new entry point.
- **New:** `Morpher::parse_word_opts(&self, word: &str, opts: &ParseOptions) -> ParseOutcome`
  with `pub struct ParseOptions { pub guess_root: bool }` (non-exhaustive; future per-call knobs
  land here instead of more method variants). This mirrors C#'s per-call `guessRoot` parameter
  rather than a Morpher-construction flag — FieldWorks toggles it per word.
- `ParseOutcome` gains `pub guessed: bool` (true iff the returned analyses came from the guess
  branch — C#'s signal is "the caller passed true and got results with `RootAllomorph.Guessed`").
  Per-analysis granularity is unnecessary: the branch is all-or-nothing (`syntheses.Count == 0`
  gate), so one flag describes every returned analysis.
- `WordAnalysis` (`hc-parse/src/lib.rs:22-26`) gains `pub guessed: bool` (the per-analysis mirror
  FieldWorks will want via FFI; always equal to the outcome flag today, but the wire format
  shouldn't bake that coupling in).
- `hc_parse_batch` / `BatchWordOutcome`: thread `ParseOptions` through (one options value for the
  whole batch run).
- `hc-cli`: `batch` gains `--guess` (off by default; default path byte-identical). No change to
  the TSV columns or signature format — see §4.5.
- `hc-ffi`: `hc_parse_word`/`hc_parse_batch` gain a `guess_root: i32` parameter (0/1) and the
  wire encoding of each analysis gains the `guessed` flag ⇒ **`hc_abi_version` bump**. (The
  managed facade consuming this is the same out-of-scope §4.1-C# work already noted for M8.)

### 4.2 Grammar model + loader (`hc-grammar`)

Almost nothing: the XML surface and segmentation already exist (§2). Add:

- `RootAllomorphDef.is_pattern: bool`, computed at load in `load_root_allomorph` by the exact C#
  ctor rule (`RootAllomorph.cs:16-29`): any interior node with `flags.is_iterative()`, or
  (`flags.is_optional()` and `kind != NodeKind::Boundary`). A stored field (not a method) to
  mirror C#'s compute-once and keep the Morpher-build partition allocation-free.
- Unit tests in `load.rs`/`segment.rs`: `[Any]*` ⇒ pattern; `([V])` ⇒ pattern; `b[Vowel]t` ⇒ NOT
  a pattern; `pit` ⇒ not; boundary-optional-only shapes (`+` is Optional after segmentation) ⇒
  NOT a pattern (the `!= Boundary` guard — this is why every ordinary `pi+t` root doesn't
  classify as a pattern).

No lint change: a pattern entry is valid loadable grammar (UNPORTED-SILENT stays at zero by
*porting*, not linting).

### 4.3 Where the guesser lives (`hc-parse/src/guess.rs`, new module)

`hc-parse` is the right crate: the guesser is Morpher-tier orchestration (like
`lexical_lookup`), needs `Grammar` + `Shape` + tables + `surface::matching_str_reps`, and nothing
in `hc-rules` may depend back on it. Contents:

- `match_nodes_with_pattern(input: &[GuessNode], pattern: &[GuessNode]) -> Vec<Vec<GuessNode>>` —
  the literal recursive port of `Morpher.MatchNodesWithPattern` (§1.3 step 2), where `GuessNode`
  is a small resolved view `(kind, char_def, lanes, cd_set, optional, iterative, deleted)` built
  from a `Shape`'s interior nodes (anchors excluded — they are not `ShapeNode`s in C#).
  Unification of an input node with a pattern node = kind equality + identity gate (concrete
  char-def equality / `CdSet` membership / `NO_CHAR_DEF` wildcard, as `edge_matches`) + lane
  unification, producing the unified node (input node narrowed by pattern lanes ∩, and by the
  pattern's `cd_set` when the input is underspecified) — the narrowing matters for rendering.
- `render_match(table, &[GuessNode]) -> String` — §1.3 step 3: skip boundaries/deleted, per node
  the first rep of the first table char-def whose kind matches, whose id is in the node's
  effective cd-set, and whose lanes unify — i.e. exactly `surface.rs::matching_str_reps`'
  predicate; refactor that function's core to be callable on a node-view rather than duplicating
  it (it currently takes `(shape, i)`).
- `lexical_guess(morpher, analysis_word) -> Vec<Word>` — §1.3 steps 1-7. Fabrication of the
  guessed `Word` mirrors `morpher.rs::set_root_allomorph`: shape = `segment_with_features` of the
  rendered string against the **pattern entry's stratum table**; stratum / `syn_fs` / `mpr` /
  `is_partial` from the pattern's owning entry (§1.3 step 6's "net effect" — the port always
  takes the `Morpheme != null` branch since every Rust pattern has an owner via
  `allomorph_owners`; record that simplification in the module doc with the cs:564-579 citation);
  clone semantics = `clone_without_alternatives` + `source = Some(Rc::new(aw.clone()))`, same as
  `lexical_lookup` (§1.3 step 7).

`Morpher` gains `lexical_patterns: Vec<(AllomorphId, LexEntryId)>` (flat, all strata, document
order — C#'s single list), built in `Morpher::new` by the same partition that now *excludes*
those allomorphs from `RootAllomorphIndex::build` (pass the predicate down or prefilter; either
way `root_trie.rs:113-138`'s doc note is replaced by the real branch). The guess branch itself is
~15 lines in `parse_word_opts` after step 4 (§1.2's pseudocode), collecting into a `Vec` (no
dedup) and sorting descending by `w.morphs.len()` (C# `Morphs.Count()`), stable sort (Rust
`sort_by` is stable; C# is unstable — tie order is unobservable after `result_signature` sorting,
and for FFI consumers stability is the safer superset).

### 4.4 Representing the guessed root (the crux)

The guessed allomorph/entry/morpheme exist in no `Grammar` table, and `Grammar` is immutable and
shared across threads — no appending. Design: **sentinel ids + a per-word payload**.

- `hc-rules/src/word.rs`: `pub struct GuessedRoot { pub pattern_allo: AllomorphId, pub
  pattern_entry: LexEntryId, pub text: String }` and `Word.guessed_root:
  Option<Rc<GuessedRoot>>` (`Rc` — words are cloned heavily; the payload is immutable once
  fabricated; `Word` is already `!Send` per-parse). `text` doubles as the fabricated `Id`/
  `Gloss`/`MorphemeId` string (C# sets all three to `shapeString`).
- Sentinels: `AllomorphId::GUESSED = AllomorphId(u32::MAX)`, `MorphemeId::GUESSED =
  MorphemeId(u32::MAX)` — used in `Word.root_allomorph` and the root `MorphRecord`. Constants on
  the id types in `hc-grammar/src/model.rs`, so every match site names them.
- Resolution sites, each keyed by "identity = sentinel; **content** = delegated to the pattern":
  1. `validity.rs::allomorphs_valid_impl` (line 419's `allomorph_owners` index — the one place
     that would panic today): on the sentinel, fetch `w.guessed_root` and check against the
     **pattern allomorph's def** (`entries[pattern_entry].allomorphs[i]` via `pattern_allo`):
     `is_bound` (copied verbatim in C#), stem-name gates (`stem_name` copied; the
     "excluded-stem-name of sibling allomorphs" loop iterates the **fabricated** entry's
     allomorphs in C# — exactly one, itself — so it is a no-op: do NOT iterate the pattern's real
     siblings), environments (identical objects to the pattern's in C#, so reuse
     `cache.allomorph(pattern_allo).envs` — no per-guess compilation), allomorph co-occurrence
     rules (the pattern's rule list evaluated with the guessed root as primary; "other" references
     to real allomorph ids compare against the word's other morphs as usual, and the sentinel
     correctly never equals any real id — mirroring C#'s fabricated-object-≠-pattern-object
     semantics), and morpheme co-occurrence rules (same delegation at the morpheme level).
  2. `morpher.rs::morpheme_join` / `structured_analysis`: sentinel morpheme →
     `guessed_root.text` for the join (C# `Morpheme.Id = shapeString` is what `BatchCommand`
     prints); `morpheme_ids` carries `u32::MAX` + `WordAnalysis.guessed = true`.
  3. `morpher.rs::allomorphs_in_morph_order`: no change (operates on ids; the sentinel dedups
     correctly since a word has at most one guessed root).
  4. Nothing else: synthesis rules, surface rendering, `is_match`, `expand_alternatives` are all
     shape-/trail-driven and never resolve the root allomorph id.

  Audit obligation for the implementer: grep `hc-rules`/`hc-parse` for `allomorph_owners\[` and
  `\.allomorph\b.*\.0 as usize` and disposition every site against the list above (known extra
  sites: `morph.rs`'s blocking/`ChooseInflectionalStem` seeds — unreachable for a guessed root,
  which is never a lexicon entry; `generate_words` — takes real `LexEntryId`s only; document
  both as unreachable rather than handling them).

### 4.5 Output/signature impact

None, deliberately. The batch TSV signature for a guessed parse is what C#'s `BuildSignature`
would print: morpheme join includes the guessed string (via §4.4-2), surface via the ordinary
renderer. `guessed` travels only in `ParseOutcome`/`WordAnalysis`/FFI — no new TSV column, no
marker in the signature, so every existing golden stays byte-valid and guess-on TSVs diff cleanly
against a guess-on oracle.

---

## 5. Ordered implementation plan (landable chunks)

Each lands green on the full workspace suite + Indonesian 121/121 + the standard corpus probes;
none changes default-path output except chunk 2 (which *fixes* a divergence).

1. **`hc-grammar`: `is_pattern`** — field, loader compute, unit tests (§4.2). Inert.
2. **`hc-parse`: the partition** — exclude `is_pattern` allomorphs from `RootAllomorphTrie::
   build`; add `Morpher.lexical_patterns`; update `root_trie.rs`'s module doc. Gate test: a
   grammar with a `[Any]*` entry must return `-` (not a bogus root) for a one-segment word with
   guess off — red against today's engine, green after. This is the latent-divergence fix and is
   correct independently of the rest of P11.
3. **`hc-rules`: guessed-root plumbing** — `GuessedRoot`, `Word.guessed_root`, sentinels,
   `validity.rs` delegation (§4.4-1), unit tests constructing guessed words by hand. Inert (no
   producer yet).
4. **`hc-parse/src/guess.rs`: the matcher** — `match_nodes_with_pattern` + `render_match` +
   the ported `TestMatchNodesWithPattern` cases + iterative/optional/unification unit tests.
   Inert.
5. **Wire-up** — `lexical_guess`, `ParseOptions`, `parse_word_opts`, the guess branch + sort,
   `morpheme_join`/`structured_analysis` sentinel handling, `ParseOutcome.guessed`. Rust port of
   `AnalyzeWord_CanGuess_ReturnsCorrectAnalysis` lands here against a committed XML fixture
   (remove the "not ported until decided" note at `csharp_port_morpher.rs:21-24`).
6. **Surfaces + conformance** — CLI `--guess`, FFI params + abi bump, fixtures (§6), gate tests
   replaying them.

Estimated total: M (2 is S and standalone; 3-5 are the bulk; nothing touches the frozen `hc-fst`).

---

## 6. Conformance fixtures (to build during chunk 6, oracle-verified)

**Oracle caveat first**: the C# CLI (`hc.dll`) has **no guess flag** — `BatchCommand` hardcodes
`ParseWord(word, out _)` (guess off; `BatchCommand.cs:209`), and no Tool command mentions
guessing. Same situation as the Generation API. Options, in preference order: (a) add a
`--guess` option to `BatchCommand` in the `parse-opt` worktree (a 5-line change: option flag +
`ParseWord(word, out _, _guess)`), rebuild `hc.dll`, and record the Tool patch + commit in each
fixture README's "Generating command" section (the strrep-identity README is the format model);
(b) a throwaway NUnit/console harness calling `Morpher.AnalyzeWord(word, true)`. (a) keeps the
established `script.txt` replay protocol; it needs John's OK since `parse-opt` doubles as the
shared oracle — **ask before patching** (open question #2). The Rust `--guess` flag has no
upstream C# CLI equivalent either way; note that in the CLI help text.

Fixture set, under `rust/conformance/guesser/` (each: `grammar.xml` + `words.txt` +
`expected.tsv` + `script.txt` + README, replayed by a `hc-parse/tests/guesser_gate.rs`):

1. **`canguess-basic`** — XML transcription of the C# unit test (`[Any]*` pattern with empty
   class FS + `-d` V suffix + a real V root as control). Words: `gag`, `gagd`, a real-root word
   (guess must NOT fire when normal parse succeeds), and both run guess-off (all `-` except the
   real root) and guess-on. Pins: the zero-result gate, the multi-morph guess, the 1-vs-2-morph
   coexistence in one signature.
2. **`pattern-shapes`** — patterns `([Seg])([Seg])` (HISTORY row 25: multiple FST-paths-to-same-
   string dedup), `[C][V]*` (mixed concrete-class + star), and a class-restricted `[Vowel]*`
   (pins `CdSet` membership: consonant-bearing words must not guess). Zero-phon-feature grammar
   (Sena-style) so identity is the only discriminator — the P10 lesson as a guesser fixture.
3. **`carryover`** — patterns carrying `StemName`, a `RequiredEnvironment`, `isBound="true"`,
   properties, and an allomorph co-occurrence rule; words chosen so each copied constraint
   rejects at least one otherwise-valid guess (bound root alone; environment mismatch; stem-name
   mismatch). Pins §4.4-1's delegation.
4. **`homograph-multipattern`** — two distinct patterns both matching the same word (same
   rendered string) ⇒ two guesses survive (row 12's per-pattern `shapeSet`); plus one pattern
   producing the same string via two paths ⇒ one guess.
5. **`strata`** — pattern entry in the deep stratum of a 2-stratum grammar; pins the fabricated
   entry taking the *pattern's* stratum and the guessed root flowing up the full synthesis
   pipeline.
6. **`feature-grammar`** — a phon-feature-bearing grammar (Indonesian-style table) with a
   `[Vowel]*`-ish pattern; pins lane unification + rendering when `StrRep` is absent from
   char-def FSs (the P5 regime).

Corpus regression: guess-off full runs on Indonesian/Sena/Amharic before/after chunk 6 must be
byte-identical (the flag default path). No guess-on corpus gate initially — no C# golden exists;
generate one with the patched Tool if (a) is approved.

---

## 7. Open questions

1. **`Distinct()` semantics** (§1.2): treated as a no-op by design; verify once against the live
   oracle with fixture 4 (a duplicate-producing case) before freezing `expected.tsv`.
2. **Patching the oracle Tool** with `--guess` for fixture generation (§6) — John's call, since
   `.worktrees/parse-opt` is the shared oracle. Fallback (b) works without touching it.
3. **FFI wire format for `guessed`** — bool-per-analysis chosen here (§4.1); confirm against
   what the eventual C# facade/FieldWorks actually reads (`Allomorph.Guessed` is per-allomorph,
   but only the root can be guessed, so per-analysis is lossless today).
4. **Guess-on ordering over FFI**: descending morph count is ported (observable in `AnalyzeWord`
   order); ties are C#-unstable. If a fixture ever exposes tie order, decide then (current call:
   Rust stable sort, documented as a strengthening).
