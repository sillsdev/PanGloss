## 1. Rust core: `pangloss diagnose` skeleton + report model

- [ ] 1.1 Add a `diagnose` subcommand to `pg-cli` that accepts a grammar path (reusing `load_grammar`'s `.xml`/`.json`/`.fwdata` dispatch) and a `words.txt`, and `--project`/word-list/output-dir args
- [ ] 1.2 Define the `report.json` model (`schema_version`, grammar metadata, `word_count`, per-word rows, compile profile, summary) as serde types in a new `pg-cli` diagnostics module
- [ ] 1.3 Emit `report.json` for a grammar with grammar metadata + word count only (end-to-end write path), plus a golden-ish unit test on a tiny in-memory grammar

## 2. Per-word timing distribution

- [ ] 2.1 Run the word list through the default `Morpher` and the `--engine=foma` propose→confirm path, capturing wall-clock per word (reuse `batch`'s threading + `--word-timeout-ms`)
- [ ] 2.2 Compute p50/p95/p99/worst/mean per engine; segregate timed-out words into their own count, excluded from percentiles
- [ ] 2.3 Unit-test the percentile aggregation (including the timed-out-segregation rule) with fixed inputs

## 3. Per-mechanism compile profile + state-explosion curve

- [ ] 3.1 Add an optional `CompileProfile` capture sink; thread it (as `Option`, `None` in production) through `emit` and `replace::compile_and_compose_rules`
- [ ] 3.2 Record per phonological rule: own-net compile time + states/arcs; record composed-net states/arcs after each fold step (the state-explosion curve); record per-template lexc line counts and α-tuple survivor counts (reuse `TupleReport`/emit counters)
- [ ] 3.3 Derive category counts (rewrite-rule subtypes, templates, partition groups, compounding, strata, char-def tables) from the loaded `Grammar` model, aligned with the construct matrix
- [ ] 3.4 Populate the `compile_profile` block in `report.json`; verify the non-diagnostic compile path is byte-identical with the sink `None` (run the Indonesian/Amharic/Aweti gates)

## 4. Gloss dump + deep debug

- [ ] 4.1 Emit `glosses.tsv` — one row per input word with its gloss(es), no-analysis words explicitly marked
- [ ] 4.2 Add `--debug`: per-word (foma path) proposed-candidate count, confirmed count, and dead-end signal, written to `debug.jsonl`
- [ ] 4.3 Test gloss dump completeness (every input word present) and that `--debug` is off by default

## 5. PowerShell runner + `incoming/` convention

- [ ] 5.1 Add `scripts/diagnose.ps1` with `<lang>` / `-All` / `-Project <path>` and `-Full` / `-Debug`, invoking the Rust core and rendering `report.md` from `report.json`
- [ ] 5.2 Establish `incoming/<lang>/{grammar.*,words.txt}`; add `incoming/` to `.gitignore` with a committed `incoming/README.md` documenting the convention
- [ ] 5.3 Render `report.md`: per-language timing table, compile-profile table with the state-explosion curve, and (when present) the parity summary
- [ ] 5.4 Fail loudly (non-zero, naming the directory) when a selected language lacks a grammar or `words.txt`

## 6. C# reference harness + parity (`--full`)

- [ ] 6.1 Add a stats/timing subcommand to `machine/src/SIL.Machine.Morphology.HermitCrab.Tool` that runs C# HermitCrab over the same `words.txt` and emits per-word gloss + timing in the shared TSV protocol + a JSON sidecar
- [ ] 6.2 Wire `-Full` in `diagnose.ps1` to invoke it via `dotnet run`; align outputs by word and compute word→gloss parity (gloss-set agreement) + Rust-vs-C# comparative timing into `report.json`/`report.md`
- [ ] 6.3 `-Full` fails clearly when `dotnet` is unavailable; without `-Full` the Rust-only report is unaffected

## 7. First-class integration: CI guard + skill

- [ ] 7.1 Commit a tiny synthetic fixture grammar (via `pg-grammar-gen`) + word list under a non-gitignored fixtures path
- [ ] 7.2 Add a CI job that runs `diagnose` on that fixture and asserts the run succeeds and `report.json` is well-formed (no corpora, no `--full`)
- [ ] 7.3 Author `.claude/skills/grammar-diagnostic/SKILL.md` documenting when to run the pipeline, how to read the compile profile / dead-end signals, and the hand-off to `dead-end-census`

## 8. Verification

- [ ] 8.1 Run `diagnose --all --full --debug` over the four languages; confirm the report reproduces the p50/p95/worst/mean + word-count + compile-time table and Rust-vs-C# parity
- [ ] 8.2 Confirm the existing byte-identity/recall gates (Indonesian 97/97, Amharic parity, Aweti) remain green with the instrumentation present
- [ ] 8.3 `openspec validate add-grammar-diagnostics --strict` passes
