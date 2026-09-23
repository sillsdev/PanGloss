# Warning Architecture Implementation Plan

> **For agentic workers:** Execute this plan inline in the current worktree. Keep the three commits focused and verify each commit with a reverted-production red test before the green run.

**Goal:** Make grammar-health warnings trustworthy and driven by the grammar model's canonical partial-morpheme facts.

**Architecture:** `pg-grammar::grammar_health` owns validated diagnostic reports and format adapters. `Grammar::partial_morpheme_facts` owns the partial inventory and exposes typed entry/rule identities; warning construction consumes those identities without scanning `Grammar` again. A single FieldWorks navigation seam resolves only recognized GUID-backed sources and represents every other source as unavailable. Stats identity remains a stats concern and keeps its persisted key contract.

**Tech Stack:** Rust workspace, serde/serde_json, PowerShell-managed Cargo through `rust/tools/pg.ps1`, focused unit tests in `pg-grammar` and `pg-cli`.

---

### Task 1: Validated versioned report

**Files:**
- Modify: `rust/crates/pg-grammar/src/grammar_health.rs`
- Modify: `rust/crates/pg-cli/src/grammar_health.rs`
- Modify: `rust/crates/pg-cli/src/surface.rs`
- Test: the existing grammar-health unit tests in those modules

- [x] Add characterization tests for the versioned JSON default, direct structured decode validation, and renderer failure that names the finding code/index/field.
- [x] Run the focused tests red before changing production code.
- [x] Validate findings once before every renderer, keep no filtering path, and make `check_grammar_health` propagate canonical computation errors to the CLI.
- [x] Keep one `render_json` encoder for the versioned structured report; remove the bare-array adapter, flag, and compatibility DTO/test path.
- [x] Run `pwsh -NoProfile -File rust/tools/pg.ps1 -Mode check -Package <crate>` once each for `pg-grammar` and `pg-cli` (`-Package` takes one crate), then focused grammar and CLI tests; commit the first focused change with the required trailer.

### Task 2: Canonical partial identities and FieldWorks/log presentation

**Files:**
- Modify: `rust/crates/pg-grammar/src/model.rs`
- Modify: `rust/crates/pg-grammar/src/grammar_health.rs`
- Modify: `rust/crates/pg-grammar/src/grammar_health_presentation.rs`
- Modify: `rust/crates/pg-cli/src/grammar_health.rs`
- Test: model, grammar-health, presentation, and CLI fixture tests

- [x] Add typed partial-entry and partial-rule identities to `PartialMorphemeFacts`, and add a two-direction differential assertion that warning subjects/titles/IDs exactly match the canonical inventory with no missing or extra warnings.
- [x] Add a malformed-owner regression proving grammar-health returns the model error instead of silently admitting a report.
- [x] Add exact FieldWorks fixture assertions for lexical, affix, compound, table, and character-definition subjects; arbitrary XML keys must produce explicit unavailable navigation rather than an invented tool.
- [x] Replace duplicate partial scanners, source-ID selection, and link helpers with the canonical fact and one navigation owner seam. Render subtitle, location, and link/unavailable state in lossless log lines while preserving `--log-guids` as opt-in.
- [x] Strengthen the internal-title guard for forms such as `mrule#18`, add default/log/structured-output tests, run reverted-production red evidence, then commit the focused change with the required trailer.

### Task 3: Remove passive stats collateral and preserve persisted keys

**Files:**
- Modify: `rust/crates/pg-grammar/src/stats_identity.rs`
- Modify: `rust/crates/pg-cli/src/trace_render.rs`
- Test: stats identity and trace rendering tests

- [x] Add the persisted `root_index#ordinal` characterization test and run it red against the current derived key.
- [x] Remove passive warning-only resolution fields/types and trace fallback DTO/helpers that are not needed by the warning seam, while retaining shared human naming where it has real consumers.
- [x] Restore the trace golden output and the legacy root-index key; delete obsolete identity-resolution tests and helpers rather than retaining compatibility shims.
- [x] Run symbol searches proving removed paths are absent, run the focused green checks, commit the cleanup with the required trailer, and run the managed garbage-collection check.

### Task 4: Code-review Strong findings (Standards + Spec, 2026-09-22)

Each item is a design decision, not a suggestion. Red test first where behavior changes.

- [x] **Validate once, by type.** `check_grammar_health` returns a `GrammarHealthReport` whose only constructor validates; `render_json`/`render_log` take `&GrammarHealthReport` and never re-validate. Remove validation from `Serialize`. `GrammarHealthCheckFinding` no longer derives `Deserialize`; `from_json` is the single decode path, parses once, and constructs through the validating constructor. Validation errors get a typed `GrammarHealthReportError` (code, finding index, field), not `serde_json::Error`.
- [x] **No stored derived values.** Drop the stored `group_name`; serialize it derived from `code`. `ALL` gets a test that every variant appears exactly once.
- [x] **Navigation is a structural enum, built once.** Replace `FieldWorksLink { tool: String, url: Option, url_unavailable: Option }` with `Available { guid, tool, url }` / `Unavailable { reason }` (typed reason enum; missing project and missing GUID are distinct and both reported). Subjects are built complete with the project name passed into the checker; delete `prepare_report`, `complete_link`, and all post-construction mutation.
- [x] **Authoritative sources only.** A GUID comes only from a field that records a FieldWorks GUID (`source_msa_guid`, `source_infl_type_guid`, `form_guids`, or another field whose doc states FieldWorks provenance) — never from `xml_key`, `xml_id`, or `authored_id` by shape. A tool name is emitted only when verified against the FieldWorks checkout (`C:\Users\johnm\Documents\repos\FieldWorks`), with the FieldWorks source file cited in the test; every other subject is `Unavailable` with a specific reason. Add exact-assertion tests for each available kind.
- [x] **Lossless log.** The log line carries title, subtitle, location, and the full URL or unavailable reason; `--log-guids` stays opt-in. One subject formatter serves both log and location text; delete `where_text`/`location_text` duplication.
- [x] **Stats collateral fully reverted.** `stats_identity.rs` and `trace_render.rs` return to their `e7981595` behavior (labels, phon-rule label, trace output), except the `root_index#0` characterization test. Warning display naming lives in `grammar_health` (or the canonical partial facts), not `stats_identity`. `model.rs` does not import from `stats_identity`.
- [x] **One identity source for partial warnings.** `check_partial_morphemes` reads kind and `internal_id` from `PartialMorphemeFacts`, never re-walks `grammar.mrules`; one `MorphRuleDef` kind mapping.
- [x] **Comments.** Fix the literal tab at `grammar_health.rs` `title` doc and lost backticks; private `///` back to one line; remove stale claims (`IList<…>` "exactly", "Always exits 0", "source compatibility", "for Motif", "Reads partial directly"); restore the C# PORT note about Realizational rules; update the `main.rs` usage line with `--fw-project`/`--log-guids`.
- [x] **Out-of-range ids fail loudly** rather than rendering "unnamed …" (control-that-cannot-act).
- [x] Remove dead test assertions (`<= 45` after `<= 30`; redundant loops after sorted `assert_eq!`).
- [x] Symbol search proves gone: `prepare_report`, `complete_link`, `url_unavailable`, `group_name` field, `Deserialize` on finding, `source compatibility`, `IList`.

### Verification handoff

- [ ] Record exact red and green managed commands, commit ids, symbol-search evidence, and unresolved risks in `docs/superpowers/plans/2026-09-22-warning-architecture.result.md` (untracked; do not commit).
- [ ] Commit Tasks 1-4 as focused commits with the required trailer. `rustfmt: applied` reflow is committed, never reverted.
- [ ] Do not run the full suite, rebase, merge, push, or expand into FST structured-subject migration, importer warning transport, FST explanation validation, or unrelated stats redesign. The coordinator runs the second review, the rebase onto `main`, and the authoritative `-Mode test`.
