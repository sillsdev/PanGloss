# 098: FieldWorks `<HC><GuessRoots>` is not read; PanGloss guesses only on `--guess`

Kind: behavioural.
Status: open, not implemented. FieldWorks guesses roots by default; PanGloss does not, and has no project-driven switch. Left unwired because the faithful default would change every FieldWorks project's output, which needs a decision rather than an import.

## C# site

FieldWorks `Src/LexText/ParserCore/HCParser.cs` (FieldWorks `620cd9903`): the constructor sets `m_guessRoots = true` (cs:62); `LoadParser` replaces it with `(bool) guessRootsElem` when `<HC><GuessRoots>` is present (cs:165, 173-174; a malformed value throws and loads no parser); `ParseWord` and `ParseToXml` call `Morpher.ParseWord(word, out _, m_guessRoots)` (cs:99, 238). The parameters dialog writes `GuessRoots` `true` whenever it fills in defaults (`Src/LexText/ParserUI/ParserParametersDlg.cs::LoadParserData`), so most projects whose parameters were ever edited carry an explicit `true`. Machine `aab237b2` `Morpher.ParseWord` (cs:193-218) guesses only when the ordinary synthesis found nothing and the grammar has lexical-pattern root allomorphs. FieldWorks marks a guessed morph by storing the matched text as its form (`MorphInfo.GuessedString`, cs:460; `ParseFiler.cs:286-290`).

## Rust site

`pg_parse::ParseOptions::guess_root` and `pangloss batch|parse|serve --guess` (default off); `ParseOutcome::guessed`; `batch` adds a sixth `guessed` column only when `--guess` is passed.

## What it would take

1. Import `GuessRoots` with the strict boolean rule (as `MakePartialsFinal`, entry 101) and default `true` when absent, carried on `Grammar`.
2. Decide the default question this entry exists to raise: honouring FieldWorks means `--guess` behaviour for every FieldWorks project unless a new `--guess=false` says otherwise, and the batch TSV gaining its sixth column by default for those projects. Projects without lexical-pattern roots would get no new analyses, but `ParseOutcome::guessed` is currently set whenever the guess branch runs, even when it finds nothing, so that would need to become "found by guessing" first.
3. Confirm that guessed analyses are reported the way FieldWorks stores them (a guessed form, not a confirmed entry) in `--analyses` rows.
4. CLI precedence as for entry 101: an explicit `--guess`/`--guess=false` overrides the project.

## Coverage

None for the project setting. The guesser itself is covered by `pg-parse/tests/guesser_gate.rs` and entries 019 and 027.
