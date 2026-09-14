# Does `Word::alternatives` expand to ~N distinct analyses, or collapse to a handful?

Research note, 2026-09-14. Branch `research/alternatives-yield` off `fix/alternatives-shared`
(`e1d30ac7`, itself on main `4c870938`), which already carries `alloc-trace`, `HC_WORD_STATS`, the
`estimate_word_bytes` alternatives fix, and `Word::alternatives: Vec<Rc<Word>>`. Follows
`docs/research/word-memory-trace.md`, which found `alternatives` is 81-94% of live-frontier bytes
but never checked what re-expanding it (`Word::expand_alternatives`, `pg-rules/src/word.rs`) at
synthesis actually yields.

## 1. Instrumentation

`HC_ALT_YIELD=1` (env-gated, off by default; `pg_parse::alt_yield`, mirroring `pg_rules::word_stats`),
hooked at both `for alt in syn_word.expand_alternatives()` call sites in
`pg-parse/src/morpher.rs::parse_word_core_selected` (the normal and guess-branch loops):

- **`canonical_alt_total`/`canonical_alt_max`**: `aw.alternatives.len()` for each canonical `aw` in
  `results.values()` — the actual per-stratum-merge canonical, not the lexical-lookup clone
  `syn_word` that calls `expand_alternatives()` (`Word::clone_without_alternatives` clears
  `syn_word.alternatives` and points `syn_word.source` at `aw`, so `expand_alternatives` recovers
  `aw`'s alternatives by walking the `source` spine — confirmed by reading
  `Morpher::lexical_lookup_filtered`, morpher.rs:643-645).
- **`expanded_total`**: the summed length of every `Word::expand_alternatives()` return value.
- **`distinct_identities`**: every synthesized candidate that passes `is_word_valid_traced` and
  `is_match_traced` (the same gate feeding the `matches: HashMap<WordKey, Word>` the parse output is
  built from) is projected via `pg_parse::identity::AnalysisIdentity::project(&structured_analysis(w,
  guessed), grammar)` and inserted into a `BTreeSet<AnalysisIdentity>`. **This is the exact type,
  equality, and collection `pg-parse/tests/memo_parity_gate.rs::identity_set` uses** to dedupe
  `on_outcome.structured`/`off_outcome.structured` before comparing memo on/off (confirmed by reading
  that test directly) — not an invented identity. `AnalysisIdentity` is `{ morphemes:
  Vec<Option<String>>, root_index: i32, category: Option<String> }` with derived structural
  `PartialEq`/`Eq`/`Ord`.
- **`dropped_as_duplicate`**: `expanded_total - distinct_identities`.

`pg-cli` prints one `ALTYIELD` stderr line per word under `HC_ALT_YIELD=1`.

## 2. Part 1 measurement

Release build, `--threads 1`, `-RunMemoryGB 6`, memo on, single word (or two sharing a step-cap) per
run. Aweti rows use `samples/data/aweti.fwdata`; the Sena row uses `samples/data/sena.fwdata`.

| word | cap | status | canonical_alt_total | canonical_alt_max | expanded_total | **distinct** | dropped | collapse % |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| oteʼikateʼika | 200k | CAP | 29,524 | 839 | 3,122 | **0** | 3,122 | 100% |
| oteʼikateʼika | 1M | CAP | 219,652 | 1,919 | 17,993 | **0** | 17,993 | 100% |
| Ajkululape | 200k | CAP | 75,169 | 1,079 | 11,235 | **1** | 11,234 | 99.99% |
| kukudziwisani (Sena, control) | 1M | completes | 455 | 11 | 370 | **6** | 364 | 98.4% |

`canonical_alt_max` matches `HC_WORD_STATS`'s independently-computed `alt_len_max` exactly at every
row (839 / 1,919 / 1,079), cross-validating the instrumentation against `docs/research/word-memory-trace.md`
§2's own numbers for the same words/caps.

**Answer: collapse, overwhelmingly, not expansion.** The two pathological words hit the step cap
before completing at all, so the correct reading of their `0`/`1` rows is "this canonical's whole
`alternatives` subtree produced zero (resp. one) usable analysis, not that N alternatives collapsed to
N distinct ones" — the search never finished, so most of `expanded_total`'s candidates never even
reach `is_word_valid_traced`/`is_match_traced`. **The Sena control is the clean, cap-independent
data point**: it completes normally (never hits the cap) and still collapses 370 expanded candidates
into 6 distinct identities — a 98.4% collapse rate with no confound from an incomplete search.

Also visible: `expanded_total` is a small fraction of `canonical_alt_total` (3,122 of 29,524 at
200k) — most canonicals in `results.values()` never reach `expand_alternatives()` at all, because
`lexical_lookup_filtered` finds no matching root for them (`record_no_root` fires first) and their
stashed alternatives are simply never expanded, let alone deduped.

## 3. Part 2: which branch, and why

The measurement selects **pruning at the push sites**, not delta-encoding: alternatives collapse
to 0-6% of `expanded_total`, so the merge is overwhelmingly hoarding candidates that produce nothing
new (or nothing distinct). Delta-encoding a full-clone alternative down to its differing fields would
still be storing a delta *per candidate*, most of which never contribute a distinct analysis — pruning
is the correct response to the measured shape of the problem, not a smaller encoding of the same
count.

### What was actually pruned, and why it is conservative

`pg-rules/src/stratum.rs`'s two `words[idx].alternatives.push(Rc::new(w))` sites (`:1555`/`:1561` on
the pre-fix line numbers). The unsafe-but-tempting approach would be to dedupe on
`pg_parse::identity::AnalysisIdentity` directly — but that identity requires a `Grammar` and the full
lexical-lookup/synthesis pipeline, which live in `pg-parse`, and `pg-rules` cannot depend on
`pg-parse` (the same constraint `Morpher::lexical_lookup_filtered`'s doc on `NonHeadRootFilter`
already documents). So the push-time test uses `WordKey` (`Word::dedup_key`) instead — the identity
`pg-rules` itself already computes and uses for its own dedup — plus two fields
`Word::expand_alternatives`/`is_word_valid_traced` read off a stored alternative that `WordKey`
excludes: `syn_fs` (feeds `pos_id` and the obligatory-feature validity check) and `obligatory` itself.
`WordKey` deliberately excludes `syn_fs` (`stratum.rs`'s own comment: "a distinct state key can still
collide here"), and reading `expand_alternatives`'s fallback branch (`out.push(self.clone())`,
`word.rs`:577) confirms it carries the WHOLE alternative forward, `syn_fs` included, whenever the
alternative's `source` didn't itself fan out further upstream — so `syn_fs` is not safe to drop from
the comparison.

Two alternatives (or an alternative and its canonical's own frozen state, captured *before*
`generalize_syn_fs` widens it) equal on `(WordKey, syn_fs, obligatory)` are proven, not guessed, to
replay to a byte-identical `expand_alternatives` output: they share the exact same `source` (set once
per `analyze()` call), and every other field `expand_alternatives`/`is_word_valid_traced`/
`is_match_traced`/`structured_analysis` read off an alternative is either covered by `WordKey`, is one
of these two extra fields, or (`mpr`, `flags.is_partial`, `unapplied_rule_counts`) is not read by any
of them at all (confirmed by reading each function). Implemented as `dedup_alternative` +
`AltKey = (WordKey, FeatureStruct, Vec<FeatId>)`, with a per-canonical `HashSet<AltKey>` seeded
lazily from the canonical's own pre-widen state. Unit-tested (`stratum::dedup_alternative_tests`):
an exact duplicate of the canonical is pruned, a same-`WordKey`-different-`syn_fs` alternative is
kept, and a second exact duplicate of an already-kept alternative is pruned.

This is deliberately narrow: it only removes alternatives that are *provably* redundant against
already-retained state, not ones that would only turn out redundant after full synthesis (branch 1's
common case — different `mrule_apps` order, same eventual `AnalysisStateKey` — is NOT caught by this,
since two different unapplication orders generally have different `WordKey`s). §4 reports how much of
the measured collapse this conservative fix actually reaches.

## 4. Before/after ALLOC peak

"Before" rows are `docs/research/word-memory-trace.md` §1's own measurements on this same commit
(`e1d30ac7`), before the push-time pruning; "after" rows are fresh runs with the fix applied, same
harness (debug build, `--threads 1`, `-RunMemoryGB 6`, `--features alloc-trace`, `HC_ALLOC_STATS=1`).

| word | cap | ALLOC peak before | ALLOC peak after | change | alt_len_max before → after |
|---|---:|---:|---:|---:|---:|
| oteʼikateʼika | 200k | 229.6 MB | 188.8 MB | -40.8 MB (-17.8%) | 839 → 479 |
| oteʼikateʼika | 1M | 1337.2 MB | 1055.2 MB | -282.0 MB (-21.1%) | 1919 → 1199 |
| Ajkululape | 200k | 381.2 MB | 349.1 MB | -32.1 MB (-8.4%) | 1079 → 599 |
| kukudziwisani (Sena, control) | 1M | 37.7 MB | 39.5 MB | +1.8 MB (noise) | 11 → 11 (unchanged) |

The Sena control's `alt_len_max` is byte-for-byte unchanged: the fix found zero literal duplicates to
remove among its 455 stored alternatives (its own `ALTYIELD` line is identical before/after), which is
exactly the conservative behavior intended — the 1.8 MB "after" delta there is run-to-run noise (grammar
load jitter, allocator fragmentation), not a real regression, since nothing was pruned.

**`oteʼikateʼika`@200k does NOT fall under 100 MB.** After the fix it is 188.8 MB, 88.8 MB over. The
new top term is still `alternatives`: 31.58 MB of the 36.72 MB post-fix live-peak total (86.0%,
statistically the same share as the 86.0% measured before the fix in
`docs/research/word-memory-trace.md` §2) — the conservative prune reduced the absolute size of the
`alternatives` subtree (fewer literal-duplicate entries survive) without changing which field
dominates, because it only removes provably-redundant entries, not the much larger set that only
turns out redundant after full synthesis (§5).

## 5. What this does not establish

- The conservative `WordKey`+`syn_fs`+`obligatory` prune only catches literal-duplicate alternatives
  (identical rule-unapplication trail, or an exact repeat of one). It does not catch the far larger
  class the Sena control's 98.4% collapse implies: structurally *different* candidates (different
  `mrule_apps` order/history) that only turn out to render the same final analysis after synthesis.
  Catching that would need the identity from (3) applied cheaply at push time, which is not possible
  without `pg-rules` depending on `pg-parse`'s grammar-aware synthesis pipeline.
- `expanded_total`'s two capped rows (`distinct=0`/`1`) conflate "never reached a valid+matching
  candidate because the cap fired first" with "reached one but it duplicated another" — the Sena
  control is the only row in this table free of that confound.
