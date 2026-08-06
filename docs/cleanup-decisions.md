# Cleanup decisions — eight calls that unlock the rest

Every item below was **declined by an agent under a conservative stop rule** and verified centrally.
They are grouped by the axis they share, so one answer releases a batch rather than a file.

Ordered by what they unblock. D1 and D2 together account for most of the remaining work.

---

## D1 — Is the public API surface ours to change?

**The question:** may we delete exported items with no callers, and change public signatures, without
a deprecation path?

Five separate items are blocked on this one answer, all declined for the same reason ("that is an API
call, not dead-code removal"):

| Item | What it costs to keep |
|---|---|
| `pg-parse::morpher::synthesize_guessed_stem` — `pub fn`, **zero callers repo-wide** (verified: its definition and one doc mention) | ~45 lines plus the file's largest comment block, and a second subtly-different guessed-stem path a future caller could pick by mistake |
| `EvidenceProvenance::Behavioral` — unproducible; all 12 predicates return `Structural` | a wire variant nothing can emit |
| `ParseOutcome` — 8 public fields, 3 marked "diagnostic only, not part of any C# contract", 3 booleans encoding mutually-informative outcome kinds | the width that forced `empty_outcome` to exist |
| `Morpher`'s four independent `with_*` knobs vs the synthesis side's bundled `SynthesisBudget` | an asymmetry whose only defence is a frozen test's struct literal |
| `analyze_stratum_scoped_filtered`'s non-optional `cache` param | blocks collapsing four entry points into one delegation chain |

**Recommendation: yes, treat the surface as ours.** No external consumer exists, and the repo's own
standing position is that breaking changes are acceptable in research code. The counter-argument worth
naming: `synthesize_guessed_stem` may be a deliberately parked add-to-dictionary seam — if so, say so
and it stays.

## D2 — Do we collapse the cached/uncached/traced matrix?

**The question:** pg-rules writes the same logic out once per (cached × traced) combination. Do we
parameterise the spine, accepting that tests will exercise a different path than they do today?

- Six rule-application pairs in `morph.rs` (`synth_affix`, `synth_realizational`, `synth_compound`,
  `ana_affix`, `ana_realizational`, `ana_compound`, each with a `_cached` twin) — **~400 near-verbatim
  lines**, differing only in where the LHS FST comes from, traced vs untraced, and
  `mpr_group_ok` vs `mpr_gate_reason`.
- The same 5-arm `(Kind, mode)` dispatch written four times in `rewrite.rs` — **~200 lines**.
- Three traced analysis shells sharing a ~45-line skeleton — **~135 lines**.
- `stratum::memo_apply_rules` vs `run_template_batch`: identical memoise / replay / in-flight-guard /
  store protocol over two tables.

**This is the single largest structural item in the codebase — roughly 700 lines of duplication.**

**Recommendation: yes, but not yet — do it after the subrecipe build-out, not before.** The
parameterisation has to name exactly the axes the subrecipe work is about to move (LHS source,
gate function), and doing it now means choosing them twice. The concrete risk of waiting is stated in
`synth_compound_cached`'s own doc: a gate-order divergence between the two halves can drift silently.
If that worries you more than the rework, invert this.

## D3 — Where does per-call state live?

**The question:** introduce a per-call context struct, or keep threading parameters?

`parse_word_core_selected` does five separable jobs in ~180 lines (guards, trace-root minting, the
analysis cascade, the match loop, the guess branch, result projection). Extracting any of them
mechanically needs 7–9-parameter helpers; doing it properly needs a `ParseRun<'a>` holding `trace`,
`root`, `budget`, `lex_entry_filter`, `rule_filter` and the candidate counter. The same shape blocks
unifying `lexical_lookup_filtered`'s supplied-root branch.

**Recommendation: yes, one context struct, built once inside the single core.** This does **not**
threaten the "one body parameterised by a trace sink" property — the traced and untraced paths still
cannot drift, because there is still one body.

## D4 — `TableId(0)` is hardcoded in two functions. Fix now or file?

**Not a cleanup question — a live defect.** `rewrite::synthesize_with_mpr` (`:862`) and
`rewrite::analyze` (`:1296`) do `let table_id = TableId(0);` while **seven sibling call sites** resolve
`owning_table_for_prule(g, pid)`. On a multi-table grammar those two are table-blind, contradicting the
rule `morph.rs:44` states outright: *"Table zero is never an implicit default."*

Fixing it means giving both functions a `PRuleId` — a public signature change, hence D1.

**Recommendation: fix now, with a multi-table regression test.** Multi-table grammars are exactly what
the capability layer refuses on today, so this is likely masked rather than harmless.

## D5 — Two semantics questions only you can answer

Neither is a refactor; both are domain calls that a cleanup pass correctly refused.

1. **Should the runtime-non-head path permute word order?** `generate_words_from_analysis` unions over
   `interleavings`; `generate_analysis_with_runtime_non_heads` replays one derivation. Otherwise the
   two are the same ~35-line skeleton. Answering makes the asymmetry an explicit argument instead of an
   implicit fork.
2. **Do `ana_narrow_general`, `syn_feature` and `syn_narrow` get the fix `ana_feature` already got?**
   They carry the same latent failure, documented in `width_matches`. The fix (per-row group capture)
   is known. It is a behaviour change.

## D6 — How strong should the evidence gates be?

Three items, one axis — how much verification we demand of ourselves:

- **Generalise the strict citation check.** The test deleted with `FailClosed` graded zero rows, but it
  uniquely asserted that a cited identifier lives *in one of its own cited files* and is preceded by
  `#[test]`. The survivors only check the name exists somewhere. **This is the check that would have
  caught this session's three dead citations automatically.**
- **Schema discipline.** `COVERAGE_CLI_SCHEMA_VERSION` is still `1` after JSON fields were removed.
- **An unreviewed fixture swap.** Five rationales described an `Overwrite` MprGroup while the
  `include_str!` pointed at `simultaneous-subrule-genuine-overlap`. The prose is corrected; the swap
  itself was never reviewed.

**Recommendation: do the first, it is cheap and high-value. Bump the schema. Review the swap.**

## D7 — `procgov` adopts the `sccache` daemon and wedges a build slot

**Infrastructure, and it recurs.** Two build slots were found held with zero `cargo`/`rustc` alive:
`pg.ps1` waits on `procgov`, which waits on the `sccache` **server** it adopted via `-r`. The daemon
never exits, so neither does procgov, so the slot mutex is never released. `gc` cannot reap it — the
parents are alive. Cleared by hand this session.

`-r` is documented as recursing onto everything cargo spawns, and with `RUSTC_WRAPPER=sccache` the
daemon is precisely what gets adopted. **Recommendation: exclude the sccache server from the job
object, and have `gc` recognise a procgov holder with no compiler children.**

## D8 — Placement and ledger hygiene

Low stakes, cheap, but they decay if unowned:

- `LoweringAdapter` now has its own module. It is 1:1 with `EmissionStrategy` in `enumerate.rs`, whose
  `LoweredCandidate` carries it — fold them, or keep the seam legible? Cheap to change now.
- `digest_projection` went with the cut; `recipe_runtime::framed`/`grammar_identity` re-implements the
  same length-prefixed framing rule and used to cite it as the canonical statement. One named home?
- `tasks.md` 7.5, 7.13 and the reference-count table still describe the deleted subsystem as shipped.
