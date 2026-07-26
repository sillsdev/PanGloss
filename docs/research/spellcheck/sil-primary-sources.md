# SIL primary-source follow-up (the two docs report 05 flagged)

Follow-up on report 05's highest-value manual reads. Status 2026-07-24.

## Access notes

- **`ICU_and_writing_systems.pdf`** (Ken Zook, FieldWorks, 2019-10-31) — **retrieved.**
  `sil.org` blocks non-browser fetches (403), but the doc is mirrored at
  `https://downloads.languagetechnology.org/fieldworks/Documentation/ICU_and_writing_systems.pdf`
  (also `software.sil.org/fieldworks/.../ICU-and-writing-systems.pdf`). Text extracted
  with `pdftotext -layout`. Findings below.
- **`tone_and_unicode_issues.pdf`** ("Best practice when using non-alphabetic
  characters in orthographies", SIL, 2018-05-04) — **retrieved** (user downloaded it
  manually to the repo root after sil.org 403'd every automated client; text extracted
  with `pdftotext -layout`). Findings below. Adjacent primary source UTN #19
  (unicode.org/notes/tn19) was fetched but is deliberately high-level.

## Findings from ICU_and_writing_systems.pdf (measured — read from the PDF text)

Two findings here directly corroborate and *ground* open questions from reports 01/02/05
with concrete LibLCM behavior, not just literature:

1. **Normalization is per-writing-system tailored data that LibLCM already ships — NOT a
   universal safe preprocessing step.** FieldWorks overrides ICU's *canonical combining
   classes* for Hebrew because "standard normalization on Biblical Hebrew results in
   incorrect reordering of diacritics." They set custom combining-class values (some code
   points U+0599–U+05C7), tested by normalizing the entire Hebrew OT and confirming no
   spurious reordering (the only 2 reordered words traced to errors in the source text).
   The doc explicitly warns overriding Unicode definitions is a last resort. **Implication
   for the speller:** confirms report 05's "no safe bolt-on normalization" with a real
   example — and shows the fix lives as *per-writing-system combining-class config* the
   platform already carries. A PanGloss speller must consume the writing system's
   normalization/combining-class configuration, not apply stock ICU NFC/NFD. This is the
   worse-than-NFD case report 05 predicted, documented in the wild.

2. **The orthographic edit unit already exists as per-writing-system data.** FieldWorks
   writing systems define **multigraphs as single units** via ICU collation tailoring
   rules (worked example: `&n<ng` makes the digraph "ng" a unit sorting after "n"; the doc
   lists `ng` / `'ng'` / `ng` syntaxes), plus **custom characters and SIL
   Corporate PUA** code points (with custom Unicode properties). **Implication:** report
   01's requirement that edit distance operate over orthographic units (not bytes/scalars)
   is satisfiable by *sourcing the unit inventory from the writing system's ICU collation
   tailoring + custom-character definitions* — LibLCM already has this; the speller should
   not invent a unit model or use byte/scalar edits. Connects to report 02's use of
   `CharDefTable` and report 01's `representations`/`representations_nfd` split.

3. **Collation tailoring is also the ranking-order source.** Per-locale ICU `coll` rules
   define language-specific sort order; useful when ordering candidate corrections, and it
   ignores diacritics at the primary level unless the rest of the word matches — a
   built-in near-match notion worth noting.

## Findings from tone_and_unicode_issues.pdf (measured — read from the PDF text)

This doc is more spell-check-relevant than expected: it's fundamentally about
TOKENIZATION / word-boundary behavior, driven by Unicode character properties.

4. **The "word-forming" character property is the gate on whether spell-checking works
   at all.** The doc explicitly lists "spell checking" (alongside word searches,
   wordlists, collating/sorting, text selection) among the functions that BREAK when an
   orthography encodes a tone/grammatical marker as a non-word-forming punctuation char:
   the word-breaker splits the word and the speller never sees it as one token. "This is
   not a flaw in the software, but a feature." **Implication:** PanGloss tokenization
   must respect the writing system's declared word-forming character set (Unicode
   property classes: LETTER / COMBINING / MODIFIER LETTER are word-forming; most
   punctuation is not), NOT a generic word-breaker. This is the concrete mechanism behind
   report 05's word-boundary error class and the missing-tokenizer gap in the plan's API.
5. **Orthographies legitimately need BOTH word-forming and non-word-forming markers**
   (e.g. a plural/grammatical-tone marker that is part of the word must be word-forming;
   a phrase-level marker must not be) → the speller can't assume a fixed word-forming set;
   it's per-writing-system, and includes word-forming punctuation "look-alikes"
   (U+A78A MODIFIER LETTER, U+0347 COMBINING EQUALS SIGN BELOW, etc.).
6. **Concrete FLEx/LibLCM tokenization behavior:** FLEx now treats the apostrophe as a
   *default word-forming* character (older versions inserted a word boundary before even
   word-internal apostrophes, breaking glottal-stop/ejective orthographies). The speller
   inherits this; it's a real per-version behavior, not a hypothetical.
7. **New input-error classes surfaced (feed the error model):**
   - *Autocorrect/smart-quote mangling* — a straight apostrophe `'` is silently rewritten
     by Word (English → curly `’`; German → low-open `‚…`` high). A high-prevalence,
     locale-dependent confusable set the speller should normalize/repair. Ties to report
     03 (the host/keyboard environment silently rewrites characters).
   - *Tone homoglyphs* — SIL got grammatical-tone characters into Unicode U+A700 range
     that look *identical* to ASCII `=` and `:` but are different code points. A concrete
     homoglyph error class (report 05 flagged homoglyphs abstractly; here's a real,
     SIL-specific pair users will confuse).

## Net effect on the synthesis

- Strengthens the "unified weighted model over orthographic units, sourced from grammar/
  writing-system data" direction (reports 01+02).
- Upgrades the normalization open question from "literature says be careful" to
  "LibLCM already models per-WS combining classes; consume that config".
- Adds a TOKENIZATION requirement (was implicit): the speller/word-breaker must be
  driven by the writing system's word-forming character set, not a generic breaker —
  and tokenization is now a named first-class component, not a detail of the API.
- Adds two concrete error classes to design the error model against: autocorrect
  apostrophe/quote mangling, and U+A700-range tone homoglyphs of `=`/`:`.
