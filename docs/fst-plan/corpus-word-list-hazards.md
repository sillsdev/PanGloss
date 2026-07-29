# Corpus word-list hazards in `samples/data/`

Audit of the four private real-language word lists this repo's perf/recall tracks depend on
(`samples/data/{indonesian,sena,amharic,aweti}-words.txt`), triggered by a confirmed bug:
`amharic-words.txt` has CRLF endings and a 4-line English-gloss header (`break`, `pay`, `pfv`,
`want`) before any Ethiopic word appears. Slicing it with `head -3` silently yields three
unanalyzable gloss tokens (plus a trailing `\r`), which presents as a recall failure rather than
what it actually is: a bad slice of a fine word list.

This audit is about the *word lists*, not the grammars. No conclusions here reflect on FST
recall/compile work; they reflect on what happens if you slice these files naively.

All work here used `cargo` directly (not `rust/tools/pg.ps1`): PowerShell is broken in this
environment (`Microsoft.PowerShell.Management` fails to load), so the managed entry point could
not run. This is noted per the constraint in the task; it does not reflect a change to normal
policy.

## 1. File-level facts

Verified at the byte level (via a small Python script reading each file in binary mode and
splitting on `\n`, since several shell tools in this environment silently normalize `\r\n` on
read and produced misleading results before this was caught — see "Tooling trap" below).

| Corpus | Lines | Line ending | Blank lines | Leading/trailing whitespace | Duplicate lines | Non-word header |
|---|---|---|---|---|---|---|
| `indonesian-words.txt` | 121 | CRLF (all lines) | 0 | 0 | 0 | none |
| `sena-words.txt` | 7121 | CRLF (all lines) | 0 | 0 | 0 | none |
| `amharic-words.txt` | 673 | CRLF (all lines) | 0 | 0 | 0 | **lines 1-4**: `break`, `pay`, `pfv`, `want` |
| `aweti-words.txt` | 208 | CRLF (all lines) | 0 | 0 | 0 | none |

All four files are internally consistent CRLF throughout (confirmed with `xxd` at the start,
middle, and end of each file) — there is no mixed-line-ending file among the four. All four end
with a final `\r\n` (no missing trailing newline). No blank lines, no leading/trailing
space/tab whitespace on any line, no duplicate lines, in any of the four files.

**Tooling trap hit during this audit**: in this Git-Bash/MSYS environment, `awk` and (in a piped
`sed 's/\r$//' | cat -A` chain) apparently line-buffered output silently dropped `\r` bytes,
making `sed`/`awk`-based CR detection report 0 CRLF lines for files that `xxd` proves are 100%
CRLF. `grep -c $'\r'` and raw `xxd`/`od` byte inspection were the only reliable methods used here.
Anyone re-deriving these numbers should trust byte-level tools, not `awk`/`sed` text-mode line
counts, on this platform.

## 2. Non-word / implausible-word findings, per corpus

"Plausible" was decided against each grammar's own declared alphabet — the `SegmentDefinition`
and `BoundaryDefinition` `Representation` values in `*-hc.xml`'s `CharacterDefinitionTable`, or
`phonology.phonemes` / `phonology.boundaryMarkers` in `aweti.json` — not by guessing from Unicode
script blocks. A line is flagged if it contains a character absent from that inventory (which,
per `rust/crates/pg-parse/src/morpher.rs`, is exactly what makes the oracle throw
`InvalidShapeException` → batch status `SKIPPED`), or if it is not shape-invalid but is obviously
not a word (English gloss, interlinear-gloss break, metalinguistic label, stray symbol).

### indonesian-words.txt (121 lines, alphabet: `e p j t l c sy a h b i n u d ny ng - kh f k z m o
s r y w g ⁿ` + boundaries `+ ^0 .`)

- Line 100, `meⁿ`: every character (including `ⁿ`, U+207F) is in the segment table, so this is
  *alphabet-valid*, but it is the bare `meN-` prefix with no root attached — not a real surface
  word. Confirmed via `pangloss batch`: status `ok`, signature `-` (parses to nothing).
- Line 119, `write-CONTpijit`: contains uppercase `C`, `O`, `N`, `T`, `R`, `E`, `W`, `I` — none of
  which are in the (all-lowercase) segment table. This is gloss-template contamination: the
  English verb "write", a gloss abbreviation "CONT" (continuative), and the real Indonesian root
  "pijit" (which appears correctly elsewhere as `memijit-mijit`) got concatenated into one line.
  Confirmed: batch status `SKIPPED` (`InvalidShapeException`) — this is the same bug class as the
  amharic header, just one line deep in the file instead of at the top.
- No duplicate lines, no header section, no other characters outside the alphabet.

### sena-words.txt (7121 lines, alphabet: `y p i m pf o s a g v dh dj ps j u th ch bz yw bh h r
ng' l ph c ts f z kh bv e k w dz x d t sw b` + `' + ^0 .`; **no hyphen**)

86 lines contain a character outside the char table. Breakdown by offending character:

| Offending char | Count | Example lines |
|---|---|---|
| `-` (hyphen; not a segment or boundary at all) | 73 | 248 `ampfaka-mpfaca`, 1378 `ng'ono-ng'ono`, 5411 `ka-n'-khucha`, … |
| `á` (acute accent) | 9 | 279 `nikijibiyána`, 295 `tsákala`, 3992 `fátima`, 5853 `mabárigi`, 6325 `matálimba`, 6718 `báulu` |
| `2`, `3` (bare digits) | 2 | 1472 `neg-2psing`, 1474 `neg-3p`, 2598 `lox2` |
| `q` | 1 | 996 `require` |
| `í` | 1 | 1140 `subtítulo` |
| `ç` | 1 | 4824 `cabeça` |

The **73 hyphenated lines are the big one**: Sena reduplication (`nkhundu-nkhundu`,
`akulu-akulu`, `sawa-sawa`, …) is written with a hyphen in this word list, but the grammar's char
table defines no hyphen segment or boundary at all. Every one of these ~73 lines is therefore
**guaranteed** `SKIPPED` regardless of whether the grammar's reduplication rules are otherwise
correct — this is a word-list/char-table mismatch, not a morphology bug, and it is invisible
unless you check the char table (a straight recall run just reports these as failures).

A smaller set of lines is outright non-word contamination, distinct from the alphabet-violation
class above:
- `require` (line 996) — a bare English word.
- `neg-2psing` (1472), `neg-3p` (1474), `h-u-dadya` (1471), `h-a-da-dya` (1473) — these read as
  interlinear-gloss / morpheme-break debris (a person/number gloss tag and hyphen-segmented
  morpheme breakdowns), not surface words.
- `v-front` (2207), `det-suffix` (4707) — metalinguistic feature/rule labels.
- `lox2` (2598) — a stray alphanumeric token, likely a template/placeholder leak.

The remaining accented lines (`fátima`, `subtítulo`, `secondário`, `cabeça`, `mabárigi`,
`matálimba`, `báulu`) read as genuine Portuguese loanwords/proper nouns (plausible in a
Sena-Portuguese contact situation) whose accented letters simply were never added to the char
table — these are lower-priority than the contamination lines above, but still guaranteed
`SKIPPED`.

### amharic-words.txt (673 lines; alphabet is ~417 Ethiopic syllable/gemination segments plus a
handful of bare Latin `a e i o u` and Cherokee-block placeholder glyphs, per
`samples/data/amharic-hc.xml`)

- **Lines 1-4** (`break`, `pay`, `pfv`, `want`) are the confirmed motivating bug: an English-gloss
  header with no Ethiopic content. Real words start at line 5 (669 word lines total).
- Beyond the header, **17 word lines** use Ethiopic letter variants that are historically distinct
  graphemes but **absent from this grammar's `CharacterDefinitionTable`**: `ሕ`, `ሐ`, `ሥ`, `ሦ`, `ሃ`,
  `ዓ`, `ፀ`. Affected lines: 63-65 (`መጻሕፍት`, `መጽሐፉን`, `መጽሐፎቹን`), 71-75 (`ሥራውን`, `ሦሥቱን`, `ሦሥት`,
  `ሦስተኛ`, `ሦስት`), 91 (`ሰብረሃል`), 105 (`ሰብሮሃል`), 355 (`አሥረኛ`), 502 (`ውሃውን`), 512 (`ዓለሙ`), 601
  (`ይበላብሃል`), 604 (`ይነግሩሃል`), 615 (`ይነግርሃል`), 627 (`ገለፀ`). These are guaranteed unanalyzable for
  the same reason as the Sena hyphen case: a character the char table does not define, not a
  morphology bug.
- No duplicate lines, no whitespace hazards.

### aweti-words.txt (208 lines; alphabet from `phonology.phonemes` / `boundaryMarkers` in
`aweti.json` — includes accented/nasalized Latin letters, ejective-marked uppercase forms, and
`+`/`#` boundaries, but **no hyphen and no space**)

12 lines contain a character outside the alphabet:

- Hyphen-containing (SKIPPED, same class as Sena): lines 14 `itywytu-put`, 28 `waráju-puza`, 37
  `waraju-puza`, 40 `kaminuʼat-puza`, 71 `nãtsuat-put`, 120 `owatsa-puza`, 179 `awytyza-zan`, 180
  `ọwatsaᵀ -puᵀ -za`.
- Literal-space-containing (also SKIPPED — and these aren't single tokens at all): line 16
  `ị- tyᴾ`, line 70 `awytyza ʼytoto`, line 180 (also has a space, see above).
- Line 147, `µ`: a bare MICRO SIGN (U+00B5), no linguistic content — looks like a stray
  copy-paste artifact, not a word.
- Line 175, `wejmopáb`: ends in a bare `b`, which does not appear anywhere in this grammar's
  phoneme inventory (only uppercase ejective `P`/`T`/`K` and lowercase `p` exist) — likely a typo.

## 3. HC-oracle measurement: how many words actually get analyzed

Measured with the CLI's `batch` subcommand (`rust/target/release/pangloss.exe batch <grammar>
<words.txt> <out.tsv>`), **default engine** (HC oracle, `--engine=default` is implicit and never
capability-enforced). Columns are `index, word, <candidate/step count>, status, signature`; a
signature of `-` means no successful parse. `status` is `ok` (ran to completion — parse or no
parse), `SKIPPED` (`InvalidShapeException`: a character outside the char table), or `TIMEOUT`
(hit `--word-timeout-ms` before finishing).

**This is a sample, not a full-corpus run**, capped per the task at ~40 words. The exact slice for
every corpus is **the raw file's lines 1-40** (`sed -n '1,40p' samples/data/<name>-words.txt`) —
i.e. deliberately the same kind of naive `head`-style slice that caused the motivating bug, so the
numbers below show what that naive slice actually gets you today.

| Corpus | Sample | Command flags | SKIPPED | TIMEOUT | ok, no parse (`-`) | ok, real parse | Wall time |
|---|---|---|---|---|---|---|---|
| indonesian | lines 1-40 | defaults (`--threads=20`, no word-timeout needed) | 0 | 0 | 5 | 35 | ~1s |
| sena | lines 1-40 | defaults | 1 | 0 | 6 | 33 | a few seconds |
| amharic | lines 1-40 | `--word-timeout-ms=8000` | 4 | 7 | 19 | 10 | ~80s |
| aweti | lines 1-40 | `--word-timeout-ms=8000 --threads=1` | 5 | 14 | 8 | 13 | ~2 min |

Notes on each row:

- **indonesian**: sample is entirely within the "plausible root/derived-form" region of the file
  (the two hazard lines found in §2 are at lines 100 and 119, outside this 1-40 sample). Ran to
  completion instantly with no special flags. Separately confirmed the two out-of-sample hazard
  lines directly: `meⁿ` → `ok`/`-` (shape-valid, no parse); `write-CONTpijit` → `SKIPPED`
  (invalid shape) — exactly the two outcomes predicted in §2.
- **sena**: sample completed cleanly with default flags (`--threads=20`, no word-timeout). The one
  `SKIPPED` word in this slice is `n'nyumba` (line 4) — interesting because every individual
  character in it (including `'`) *is* in the alphabet; the shape rejection here is about how
  those characters combine, not a missing character, so it's a different (smaller) hazard class
  than the hyphen case in §2.
- **amharic**: **first attempt at this same 1-40 slice with no `--word-timeout-ms` timed out at
  180s (exit 124) without completing** — some individual words here take many seconds each to
  confirm (this corpus is known-slow; see `amharic-worst-words.txt`). Re-run with
  `--word-timeout-ms=8000` completed in full and is the row reported above. 7 of the 40 words hit
  that 8s timeout (lines 5, 12, 17, 29-32); those would eventually resolve given enough time, they
  are not shape-invalid, just slow.
- **aweti**: **first attempt at this slice with default flags (`--threads=20`, no word-timeout)
  ran for 6m35s, grew to over 30GB resident memory, and never completed** — it had to be killed
  (`Stop-Process`) to protect the shared machine, since other agents were working in the same
  checkout at the time. Bisection on this exact word list found the trigger: line 2, `tomoʼatu`,
  individually explores 7000+ steps and does not return in isolation either. Re-running the same
  40-word slice with `--word-timeout-ms=8000` **and** `--threads=1` completed in a controlled
  ~2 minutes; with the default `--threads=20` those per-word blowups run concurrently and multiply
  memory use, which is what produced the 30GB spike. 14 of the 40 words hit the timeout — a much
  higher proportion than any other corpus, consistent with this repo's memory of aweti's
  "apply_up explosion" (41 zero-width truncation mrules × 24-level derivation chain).

**What could not be measured**: a definitive "does it eventually finish" answer for the 7
amharic and 14 aweti words that hit `--word-timeout-ms=8000` in the runs above. They were not
run to unbounded completion (that risks exactly the multi-GB/multi-minute-per-word blowup that
had to be killed once already in this session) — reported here as "too slow to confirm within an
8s budget," not as "unanalyzable."

## 4. Deriving a valid word list reliably — what changed

Extended `rust/tools/corpus-manifest.json` with an optional `word_list` shape on any manifest file
entry whose `role` is `"corpus"`:

```json
"word_list": {
  "line_ending": "CRLF",           // "LF" | "CRLF" | "mixed"
  "skip_leading_lines": 4,          // non-word header lines to skip before "line 1 of the word list"
  "notes": "free text describing header contents and known non-word/alphabet-invalid lines"
}
```

This is populated for all four corpus files with the concrete facts from §§1-2 above (exact
header line count, hyphen/space-not-in-alphabet lines, the specific gloss-contamination lines,
and — for amharic/aweti — a pointer to the slow/pathological-word hazard from §3). It is
**descriptive metadata, not an enforced slicer**: nothing currently reads `word_list` to change
program behavior. The value is that the facts now live next to the file they describe in a
structured, validated place, instead of only in this point-in-time audit doc, which can drift.

Validation added to `rust/crates/pg-conformance-fixtures/src/corpus.rs`'s `validate_manifest`
(structural checks that hold with or without any corpus file present, consistent with the
existing design):
- `word_list.line_ending` must be one of `LF`/`CRLF`/`mixed`.
- `word_list.skip_leading_lines > 0` requires non-empty `notes` (a skip can never be silent/
  unexplained).
- `word_list` may only appear on a file whose `role` is `"corpus"`.

New tests (`word_list_metadata_is_validated`) cover all three rules plus the valid/positive case.
`cargo test -p pg-conformance-fixtures` passes (9 unit tests + 1 integration test), including the
pre-existing `the_committed_manifest_parses_and_validates` test against the now-extended real
manifest.

### What is *not* implemented (left as a follow-on)

- No code currently derives "the actual word list" (skips the header, drops known-bad lines)
  automatically from `word_list` metadata — e.g. a small helper in `pg-conformance-fixtures` that
  returns `(skip_leading_lines, line_ending)` for a logical corpus name, for any future tool that
  wants to slice a corpus file safely instead of re-discovering `skip_leading_lines` by hand. The
  manifest schema was deliberately built to make that helper easy to add later (the data it would
  need is already structured), but no caller needs it today, so it wasn't added speculatively.
- The `notes` field is deliberately free text rather than a structured list of bad line numbers —
  Sena alone has 86 flagged lines across 7 categories, and encoding all of them as data (rather
  than prose) felt like speculative structure for information nobody currently consumes
  programmatically. If a future consumer wants to filter these lines out at load time, that's the
  natural next step (a `known_bad_lines: [u32]` or similar), but should be added when there's an
  actual caller, not preemptively.

## Commands run (for reproducibility)

```
# File-level facts (line endings, blanks, dup, whitespace) -- Python, binary-mode read
python - <<'EOF'
... (splits each file on b"\n", checks trailing \r per line, strips \r before decoding UTF-8) ...
EOF

# Alphabet extraction from *-hc.xml (regex over SegmentDefinition/BoundaryDefinition/Representation)
# and from aweti.json (phonology.phonemes[].representations[].form, phonology.boundaryMarkers)

# HC-oracle sampling (exact slices used):
sed -n '1,40p' samples/data/indonesian-words.txt > indonesian-sample40.txt
sed -n '1,40p' samples/data/sena-words.txt        > sena-sample40.txt
sed -n '1,40p' samples/data/amharic-words.txt     > amharic-sample40.txt
sed -n '1,40p' samples/data/aweti-words.txt       > aweti-sample40.txt

rust/target/release/pangloss.exe batch samples/data/indonesian-hc.xml indonesian-sample40.txt indonesian-out.tsv
rust/target/release/pangloss.exe batch samples/data/sena-hc.xml       sena-sample40.txt        sena-out.tsv
rust/target/release/pangloss.exe batch samples/data/amharic-hc.xml    amharic-sample40.txt      amharic-out40.tsv --word-timeout-ms=8000
rust/target/release/pangloss.exe batch samples/data/aweti.json        aweti-sample40.txt        aweti-out40.tsv   --word-timeout-ms=8000 --threads=1
```

All intermediate slice/output files were written under this session's scratchpad directory, not
under the repo.
