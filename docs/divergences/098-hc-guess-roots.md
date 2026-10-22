# 098: FieldWorks `<HC><GuessRoots>` was never read; PanGloss guessed only on `--guess`

Kind: behavioural.
Status: fixed-in-rust. A FieldWorks project now guesses roots as FieldWorks does: on by default, off when `<HC><GuessRoots>` is false. A HermitCrab XML grammar still never guesses by default, as the C# `hc` tool does not. Rust-only gap, so no Machine issue.

## C# site

FieldWorks `Src/LexText/ParserCore/HCParser.cs` (FieldWorks `620cd9903`): the constructor sets `m_guessRoots = true` (cs:62); `LoadParser` replaces it with `(bool) guessRootsElem` when `<HC><GuessRoots>` is present (cs:165, 173-174; a malformed value throws and loads no parser); `ParseWord` and `ParseToXml` call `Morpher.ParseWord(word, out _, m_guessRoots)` (cs:99, 238). The parameters dialog writes `GuessRoots` `true` whenever it fills in defaults (`Src/LexText/ParserUI/ParserParametersDlg.cs::LoadParserData`), so most projects whose parameters were ever edited carry an explicit `true`.

Machine `aab237b2` `Morpher.ParseWord` (cs:184-241, the guess branch at cs:217) guesses only when `guessRoot` is true and the ordinary synthesis found nothing; `LexicalGuess` (cs:475) iterates `_lexicalPatterns`, so a grammar without lexical-pattern root allomorphs gets no guesses. A guess's root allomorph copies the pattern allomorph's `Properties`, and its fabricated entry copies the pattern morpheme's `Properties` (cs:502, 524), so FieldWorks `HCParser` reports a guessed morph with the pattern's MoForm and MSA and stores the matched text as `MorphInfo.GuessedString` (cs:460); `ParseFiler.cs:286-290` writes that string over the bundle's form.

Machine's own hosts never guess unless asked. At the pinned submodule head `72dbd4f9`, the `hc` tool's `ParseCommand.cs:30`, `TestCommand.cs:37` and `SignatureFormat.cs:22` (batch) call `ParseWord(word)` or `ParseWord(word, out trace)`, which pass `guessRoot = false` (`aab237b2` cs:170-182). The conformance harness `Runner.cs:208` sets `guessRoot` per word, only when an expected parse is marked `guess: true`; `pg-conformance-fixtures` omits such words from adapter replays (`PROTOCOL.md` section 3).

## Rust site

- `pg-fwdata/src/parser_params.rs` (`hc_parser_bool`) imports `GuessRoots` into `ParserParameters::guess_roots` (`pg-snapshot`, default `true`, omitted from JSON when `true`, so an older snapshot reads `true`); a value other than `true`/`false`/`1`/`0` fails the import with `INVALID_PARSER_PARAMETER`, naming `HC.GuessRoots` and the value.
- `pg-grammar/src/compile/mod.rs` copies it to `Grammar::guess_roots`; `pg-grammar/src/load.rs` sets `false` for `<HermitCrabInput>` XML.
- `pg_parse::Morpher::new` starts `Morpher::guess_roots` from the grammar; `with_guess_roots` overrides it; `project_parse_options` hands it to callers as `ParseOptions::guess_root`. `Morpher::parse_word` still never guesses, as C# `ParseWord(word)` does not, and the FFI's per-call `guess_root` argument is unchanged.
- `Morpher::parse_word_core` enters the guess branch only when `guess_root` is set, the ordinary path found nothing, and `lexical_patterns` is non-empty. `ParseOutcome::guessed` is now true only when the guesser found the analyses; before, it was set whenever the branch ran. `pg-lexicon`'s `UnifiedAnalysis::guessed`, which the `pg-ffi` JSON API, `hc_parse_word_opts`/`hc_parse_batch_opts` and `pg-wasm` report, follows it, so those callers now see `guessed: false` for a guess that found nothing.
- `WordAnalysis::guessed_pattern` carries the pattern allomorph and morpheme, and `parse_morph::project_parse_analysis` projects a guessed morph as FieldWorks does: the pattern's MoForm and MSA (and inflection type), plus `guessedString`. Before, a guessed analysis was always `unavailable` (`UnsupportedRuntimeRoot`).

## CLI precedence and output contract

`pangloss batch|parse|serve`: `--guess` or `--guess=true` forces guessing on, `--guess=false` forces it off; without the flag the project's value applies. A malformed `--guess=` value is refused. The stats cache and manifest record the effective value.

The output layout depends only on the explicit flag, never on the project setting, so a reader that never passes `--guess` sees the layout it always saw:

- `batch` writes the sixth `guessed` column only with `--guess`/`--guess=true`; the 5-column default that the Machine conformance driver and other adapters read is unchanged for every project. Guessed analyses are identified in the `--analyses` sidecar by their morphs' `guessedString`.
- `parse` prints its `guessed:` line only with `--guess`/`--guess=true`; otherwise a guessed result gets a note on stderr.
- `serve` results carry a new optional `guessed` boolean beside `result` in both modes; the plain-mode `result` string has the `guessed:` line only when the server runs with `--guess`. `traceDetails` documents already carry `result.guessed` and each morph's `guessedString`.

No `--describe` interface version changes: the batch default layout and the serve wire format are unchanged, and the serve addition is an optional field, which `serve-protocol.md` says does not bump the protocol.

## Can it change a parse?

Only for a FieldWorks project that both guesses (the default) and has a lexical-pattern root allomorph, and only for a word the lexicon cannot analyse: such a word now gets guessed analyses where it had none. A project without a lexical pattern parses, counts work, and dispatches batch exactly as before: the guess branch does not run, and `batch --threads N` keeps the `hc_parse_batch` scheduler. The local before/after measurements below compare two PanGloss builds; they do not measure agreement with `hc.dll`.

## Differences from C# that remain

- Trace only: with guessing on and no lexical pattern, C# still enters its guess loop and emits a `LexicalLookup` trace node per analysis word, each finding nothing; Rust skips the branch, so those empty lookups are absent from a trace. Parses, signatures and work counts are unaffected; the gate is what keeps work counts identical for pattern-less projects.
- A guess fabricated from several analysis words with the same shape is reported once per analysis word, as C# does (no dedup in its guess branch); FieldWorks' `ParseFiler` merges equal bundles when it files them.

## Parity evidence

Recovered 2026-10-22 from the local run artifacts below. These are PanGloss-before/PanGloss-after comparisons on the five local reference `.fwdata` projects, not FieldWorks oracle observations. The before executable was built from the `f4b72959` Rust tree; the candidate was built from `5e61ed862f155c7a434068532ce0f89444617a22`. The SHA-256 hashes are `D5C57DD9952EBF9A65E5D5807E1075B5312FE8622C959E5221395A96CC8CF4A4` (before) and `407D5E02F235130D4658538161B48E0A9DFF775390797401077293CE2AA4B785` (candidate).

| Project | Rows in both builds | Matching `ok` parses | Matching `SKIPPED` rows, excluded from parse evidence | `CAP` rows, not comparable | Missing rows (before / after) | `ok` status or identity differences |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Indonesian | 70 / 70 | 70 | 0 | 0 | 0 / 0 | 0 |
| Aweti | 206 / 206 | 194 | 12 | 0 | 0 / 0 | 0 |
| Amharic | 673 / 673 | 669 | 4 | 0 | 0 / 0 | 0 |
| Sena | 6,146 / 6,146 | 5,493 | 653 | 0 | 0 / 0 | 0 |
| Mbugwe | 1,645 / 1,645 | 1,569 | 2 | 74 | 0 / 0 | 0 |

`parse_compare.py` reported no missing rows in either direction and no status or identity differences. Its `IDENTICAL` counts include rows where both outputs say `SKIPPED` with signature `-`; those are listed separately above and are not completed parse comparisons. `CAP` rows are also excluded because their signatures are partial and unconfirmed. The completed comparisons therefore cover 70 Indonesian, 194 Aweti, 669 Amharic, 5,493 Sena, and 1,569 Mbugwe `ok` rows; their `ok` statuses and serialized analysis-signature fields matched byte-for-byte. This does not assert equality of elapsed time or work counters. The separately documented Sena and Amharic worst-word lists are not present as local `*-worst-words.txt` files, so these results do not cover those pinned lists. No C# FieldWorks observations were recovered.

The runs used four threads and no wall-clock timeout. Mbugwe used `--step-cap=2000000`, Sena used `--step-cap=50000000`, and the other three used the batch default step cap. The artifacts are under `C:\Users\johnm\AppData\Local\Temp\claude\H--repos-PanGloss\f875e9b7-3bf2-433c-9327-eb5d64f7ac2c\scratchpad\parity` (`<project>-before.tsv`, `<project>-after.tsv`, logs, after-run analysis JSONL, and `<project>-compare.txt`). The recorded runner and comparison command templates were:

```powershell
./rust/tools/pg.ps1 -Mode run -Exe <before-or-candidate-exe> -- batch <project.fwdata> <project-words.txt> <output.tsv> --threads 4 [--step-cap=<limit>] [--analyses <output.jsonl>]
python rust/tools/parse_compare.py <after.tsv> <before.tsv> --show 50 --multiset
```

The exact inputs were `H:\repos\PanGloss\samples\data\{indonesian,aweti,amharic,mbugwe,sena}.fwdata` and their matching `*-words.txt` lists. The separate worst-word lists were absent from that data directory.

## Coverage

Each test below was rerun with its layer reverted and failed:

- `pg-fwdata` `guess_roots_is_read_from_the_hc_element_and_defaults_to_true`, `malformed_guess_roots_is_fatal_and_names_the_field_and_value` (import reverted to a constant `true`).
- `pg-snapshot` `a_snapshot_without_merge_analyses_reads_the_fieldworks_default_true` checks legacy JSON defaults for both `merge_analyses` and `guess_roots`; reverting the `guess_roots` serde default to `false` fails its GuessRoots assertion.
- `pg-grammar` `guess_roots_parser_parameter_reaches_the_grammar_and_defaults_to_true` (compile reverted to a constant).
- `pg-parse/tests/guesser_gate.rs`: `the_grammar_guess_roots_setting_is_the_morpher_default_and_parse_word_never_guesses` (`Morpher::new` reverted to `false`), `guessed_is_false_when_the_guess_branch_finds_nothing` (`guessed` reverted to "the branch ran"), `guessing_without_a_lexical_pattern_changes_nothing` (pattern gate removed: work counts differ).
- `pg-cli/src/tests.rs`: `a_project_guesses_roots_by_default_without_changing_the_batch_tsv_layout` (both `--threads` paths; fails with the TSV column keyed on the effective value, and with the guessed-morph projection reverted) and `an_explicit_guess_flag_overrides_the_project_and_alone_adds_the_guessed_column` (fails with the CLI override ignored, the import reverted, the compile reverted, or `guessed` reverted). Both use the synthetic `.fwdata` fixture with a `t[V]*d` lexical-pattern root.
- `pg-cli/tests/serve_protocol.rs` `guessed_words_match_cold_in_both_modes` (the serve `guessed` field forced false).

No conformance fixture carries the setting: the draft `morpher:` channel (`pg-conformance-fixtures`) refuses a `GuessRoots` key, because the harness chooses `guessRoot` per word from `guess: true` parses rather than from a host setting. The guessed identities in these tests are HC-Rust's own guesser output (entries 019 and 027), not observed from `hc.dll` on a FieldWorks project.
