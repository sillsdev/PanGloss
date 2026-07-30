# North Sámi (`lang-sme`) morphophonology reverse-engineering

Research agent 2 of 6, PanGloss / Divvun-GiellaLT investigation.

**Sources cloned** (shallow, `--depth 1`) into
`C:/Users/johnm/AppData/Local/Temp/claude/C--Users-johnm-Documents-repos-LCAtom/1b5e24e2-aeac-4668-b883-e199cfb811d9/scratchpad/divvun/a2/`:

- `lang-sme/` — https://github.com/giellalt/lang-sme (cloned 2026-07-30; HEAD of `main` at clone time, no pinned commit hash captured — treat line numbers as valid for that snapshot).
- `giella-core/` — https://github.com/giellalt/giella-core (shared Automake build-rule fragments, `am-shared/*.am`).
- `giella-shared/` — https://github.com/giellalt/giella-shared (cloned but turned out to contain none of the files referenced by `lang-sme`'s Makefiles; all the generic build logic actually lives in `giella-core`, not `giella-shared`. Noted as a finding, not a dead end I hid.)

All claims below are marked **VERIFIED** (I read the exact text at the cited `path:line`) or **INFERRED** (my reading of how pieces fit together, not literally stated anywhere). Where I looked and found nothing, I say **unknown / not found in repo** rather than estimate.

---

## 1. Repo layout and division of labor

VERIFIED, from `lang-sme/src/fst/` (`ls -la` at that path) and `lang-sme/src/fst/morphology/`:

| Path | Job |
|---|---|
| `src/fst/morphology/root.lexc` | Multichar-symbol declarations (alphabet, all POS/inflection/derivation tags), the `LEXICON Root` entry point, and the shared `ENDLEX`…`ENDLEX5` tail lexica that every word passes through on the way out (compounding legality, spelling-relax, space-compound, `+Err/Orth` serialization). 1179 lines. |
| `src/fst/morphology/stems/*.lexc` | One file per open/closed word class holding the actual lexical stems: `nouns.lexc` (6.55 MB!), `verbs.lexc` (622 KB), `adjectives.lexc` (641 KB), `adverbs.lexc`, `numerals.lexc`, `sme-propernouns.lexc` (939 KB), plus small closed classes (`adpositions`, `conjunctions`, `interjections`, `particles`, `pronouns`, `subjunctions`, punctuation). Each stem entry names its continuation lexicon (an inflection-class / paradigm identifier) explicitly. |
| `src/fst/morphology/affixes/*.lexc` | The paradigms themselves — one file per POS (`nouns.lexc` 167 KB, `verbs.lexc` 105 KB, `adjectives.lexc` 99 KB, `numerals.lexc` 182 KB, `pronouns.lexc`, `propernouns.lexc` 70 KB, `possessive-suffixes.lexc`, `abbreviations.lexc`, `acronyms.lexc`, `symbols.lexc`). This is where the huge sublexicon fan-out lives (see §2) and where the gradation-trigger diacritics are attached to suffixes. |
| `src/fst/morphology/clitics.lexc`, `compounding.lexc` | Clitic attachment (focus particles `+Foc/…`, question particle) and the compounding machinery (the `R`/`Rreal`/`RrealAfterCmpNFlags`/`MiddleNouns` lexica, all flag-diacritic-heavy — see §4). |
| `src/fst/morphology/phonology.twolc` (1781 lines) and the near-duplicate `src/fst/phonology-L2.twolc` (1763 lines) | The two-level morphophonological rule set: gradation, diphthong simplification, metaphony, vowel rising/shortening, word-final neutralization. `phonology.twolc` is the L1 (native-speaker) analyser rules; `phonology-L2.twolc` is a variant used for the error-tolerant L2/learner analyser (same rule inventory, tuned to also accept some non-standard forms). `phonology.bergslan.twolc` is a third, dialect-specific twolc variant. |
| `src/fst/morphology/generated_files/` | Copies of shared cross-Sámi (`smi-`) lexc fragments (propernouns, abbreviations, acronyms) pulled in from a shared-common package at build time — empty in this checkout except a README (build-generated, gitignored). |
| `src/fst/morphology/incoming/` | Raw wordlists awaiting integration into `stems/` (multi-MB `.txt` files: `missing2018-08-16.txt` 5.4 MB, `missingSIKOR060121.txt` 2.2 MB) — the unprocessed backlog, useful evidence of how much lexical curation this grammar represents. |
| `src/fst/morphology/oldstuff/` | Legacy paradigm-generation Perl/shell scripts (`gen-paradigms.pl`, `make-dictindex`) from a pre-lexc infrastructure, kept for reference. |
| `src/fst/filters/*.regex` | Post-lexical **cascaded** xfst replace rules operating on the *tag* string, not phonology: `reorder-tags.sme.regex` (6.2 KB), `insert-default-compounding-tags.regex`, `split-CmpN-tags.regex`, `remove-illegal-derivation-strings(-flagbased).regex`, `block-illegal_compound-strings.regex`, etc. These run **after** lexicon∘phonology composition (see §5). |
| `src/fst/phonetics/`, `src/fst/syllabification/` | Separate `xfscript`s for text→IPA/TTS transcription (`txt2ipa.xfscript`, `text2tts-sme.xfscript`, both >60 KB) and hyphenation (`hyphenation.xfscript`, 9.3 KB) — not part of the analyser, downstream consumers of it. |
| `src/fst/tagsets/`, `src/fst/tags.yaml` | Tag documentation/export, not compiled into the FST. |
| `src/fst/guesser.xfscript` | A **separate**, structurally-defined guesser transducer for unknown words (see §7 — not integrated into the main analyser by default). |

The naming convention **`phonology` = twolc (parallel constraints)** vs **`filters/*.regex` = xfst replace-rule cascade (ordered composition)** is real and load-bearing — see §5.

---

## 2. Morphotactics: continuation-lexicon design and scale

VERIFIED counts (`grep -c '^LEXICON'` per file, this checkout):

| File | `LEXICON` stanzas |
|---|---|
| `affixes/propernouns.lexc` | 415 |
| `affixes/nouns.lexc` | 285 |
| `affixes/verbs.lexc` | 270 |
| `affixes/numerals.lexc` | 123 |
| `affixes/pronouns.lexc` | 125 |
| `affixes/adjectives.lexc` | 191 |
| `affixes/abbreviations.lexc` | 65 |
| `affixes/acronyms.lexc` | 24 |
| `affixes/possessive-suffixes.lexc` | 21 |
| `affixes/symbols.lexc` | 5 |
| **affixes/ subtotal** | **1561** |
| `stems/numerals.lexc` | 87 |
| `stems/adverbs.lexc` | 33 |
| `stems/nouns.lexc` | 19 |
| `stems/adjectives.lexc` | 10 |
| `stems/verbs.lexc` | 18 |
| `stems/pronouns.lexc`, `adpositions.lexc`, etc. (9 small files) | 43 |
| **stems/ subtotal** | **213** |
| `root.lexc` | 9 |
| `clitics.lexc` | 15 |
| `compounding.lexc` | 13 |
| **Whole-module total** | **≈1811 `LEXICON` stanzas** |

Approximate entry counts, VERIFIED by counting lines that look like lexc entries (`grep -cE '^\S.*\+N.*:'` etc.): `stems/nouns.lexc` has **≈100,494** lines containing a colon (upper bound on entry count, includes comments-with-colons; a stricter `+N` match gives ≈95,391), `stems/verbs.lexc` has ≈599 `+V` entries. These are the "curated word evidence" the grammar is built from — this is a multi-decade, hand-curated lexicographic resource, not a toy grammar.

**How the inflection-class explosion is kept under control**: rather than one flat inflection paradigm per POS, GiellaLT names each phonologically/morphotactically distinct stem shape as its own sublexicon and gives it a mnemonic name after a representative word. E.g. from `root.lexc:1090-1123` (VERIFIED):

```
LEXICON GOAHTI-A !!≈ * **@CODE@** divided into a-i-u due to errortag-branch
LEXICON GOAHTI-I  !!≈ * **@CODE@** divided into a-i-u due to errortag-branch
LEXICON GOAHTI-U  !!≈ * **@CODE@** divided into a-i-u due to errortag-branch
LEXICON GOAHTI !!≈ * **@CODE@** Bisyll. V-Nouns. Short nom-compound-forms goahte-,long/short gen
LEXICON MOARSI !!≈ * **@CODE@** Bisyll. V-Nouns. Short nom-compound-forms goahte-,long/short gen, optional diph simpl
LEXICON GOAHTILONG !!≈ * **@CODE@** Long nom-compound-forms, long gen
LEXICON ALBMI !!≈ * **@CODE@** Bisyll. V-Nouns. Short nom-compound-forms, long gen.
LEXICON AIGI-I !!≈ * **@CODE@** Bisyll. V-Nouns. Short nom-compound-forms, short gen.
LEXICON STAHTA !!≈ * **@CODE@** Bisyll. Non-Gradating a-Nouns; i-Illative
...
```

(and dozens more: `IIJA`, `ESSEIJA`, `KAIJA`, `IIVA`, `PROFIILA`, `STRUKTUR`, `KULTUR`, `KANTUVRA`, `MAŠIIDNA`, `MÁŠEN`, `BENSIN`, `ADRENALIN`, `TELEFON`, `AKTION`, `NATION`, `KANON`, `SOSIAL`, `ARENA`, `BANDY`, `MEDIA`, `OBOE` — mostly loanword stem shapes). Each named sublexicon is a **paradigm class**: a stem shape (syllable count, final segment, whether it undergoes gradation, whether it has diphthong simplification, whether compound/genitive/nominative forms are long or short) picks its class once at the lexc entry (`stems/nouns.lexc:70,74`, VERIFIED: `láhki+N+Err/Orth-a-á+Sem/Feat:lahºki LAHKI ;`), and the class then owns exactly the right continuation chain into `affixes/nouns.lexc`. This is continuation-lexicon-as-paradigm-dispatch — the sublexicon name *is* the inflection class.

Explosion containment mechanisms, all VERIFIED in use:
1. **Shared continuation chains** — many stem classes funnel through the same terminal case/number sublexica (`K`, `NPx3V`, `Px3V`) once the stem-shape-specific weak/strong-grade handling is done, so the paradigm tables themselves aren't duplicated per class, only the *entry point* into them.
2. **Multichar-symbol tags as class discriminants inside the tag string**, not just as lexc routing — `+G3` ("Grade 2-3 for homonymies with grade 1-2"), `+G7` ("Grade 3, no consonant gradation") and `+Gram/3syll` ("trisyllabic verbs") are ordinary output tags (`root.lexc:167-168,183`, VERIFIED) that also correlate with which sublexicon routed there — i.e., the same distinction is encoded twice (once as lexc routing, once as a surfaced tag) so the analyser output is self-documenting about which morphophonological class a form belongs to.
3. **`+Use/…` filter tags** for optional/dialectal/registered subsets — e.g. `+Use/Circ`, `+Use/NG`, `+Use/GC`, `+Use/SpellNoSugg` (seen throughout `compounding.lexc` and `root.lexc:1119`) act as markers later stripped or filtered by the `filters/*.regex` cascade (§5), letting one shared lexc graph serve multiple downstream products (spellchecker vs. analyser vs. Apertium MT lexicon) without duplicating the graph.
4. Two entirely **separate morphotactic mechanisms coexist** for long-distance dependency, cleanly divided by kind: **flag diacritics** (`@…@`) for morphotactic/compounding/orthography legality (compile-time, no output symbol — see §4.1), and **twolc diacritic symbols** (`X1…, Y1…, Q1…, W1…`) for phonological triggering (surface-deletable tape symbols consumed by twolc rules — see §4.2). This split is itself a scale-control device: it keeps the two kinds of "action at a distance" from being encoded in the same currency, so lexc compilation doesn't have to reason about phonological alternations, and twolc doesn't have to reason about morphotactic legality.

---

## 3. Consonant gradation, in detail

**VERIFIED finding: gradation is a mix of (a) lexical archiphoneme marking on the stem, (b) trigger-diacritic transmission from the suffix, and (c) `twolc` two-level rules — never done purely in `xfst` cascade rules and never done purely by listing every grade-pair as separate lexemes.**

### 3a. The stem side: an archiphoneme convention

`phonology.twolc:66-68` (VERIFIED):
```
 º:0  !! `º` is for CnsGrad of the `lg:lgg` and `lºl:ll` type
 ¤:0  !! `¤:0` prevents ConsGrad in certain words
```
Every gradating stem is written in the lexicon in its **strong-grade lexical form with an explicit `º` archiphoneme marking the gradation site**. E.g. `stems/nouns.lexc:70,74` (VERIFIED):
```
láhki+N+Err/Orth-a-á+Sem/Feat:lahºki LAHKI ;
láhki+N+Sem/Feat:láhºki LAHKI ;
```
and dozens of examples with compound first-parts, e.g. `stems/nouns.lexc:78-84`:
```
12%0%0-lohku+N+CmpN/SgN+CmpN/SgG+CmpN/PlG+CmpNP/First+Sem/Time:12%0%0-lohºku ALBMI ;
1978-láhka+N+CmpNP/First+Sem/Rule:1978-láhºka GOAHTI-A ;
```
`º` is deleted to `0` on the surface unconditionally as far as the alphabet declaration says (`º:0`), but the *phonology.twolc* rules use `º` as **left context** to identify the segment(s) eligible to weaken, and the actual segmental change (deletion, degemination, cluster simplification) is what the rule performs — `º` itself never surfaces.

Additionally there is a whole family of **numbered morphophoneme letters that are archiphonemes for alternation classes**, `phonology.twolc:44-54` (VERIFIED):
```
u6:u
e7:e h7:h i7:i o7:o u7:u æ7:æ
g8:g h8:h m8:m n8:n
  ! the x8 ones are consonants that alternate in stem-final positions.
b9:b d9:d e9:e g9:g h9:h j9:j k9:k m9:m n9:n o9:o p9:p r9:r s9:s t9:t z9:z æ9:æ š9:š ž9:ž
! The x9 consonants  never alternate.
```
i.e. the *same surface letter* is spelled differently in the lexicon depending on whether it participates in an alternation (`g8` = alternating stem-final `g`) or explicitly does not (`g9` = non-alternating `g`) — this is a textbook archiphoneme/underspecification device: the lexical form itself carries the alternation-class information as a diacritic subscript on the letter, and `docu-sme-twol.md:35-42` spells out the linguistic motivation explicitly (VERIFIED):
> "Thus, the word 'viehkat' has the stem viehkag, whereas the word 'Rollag' has the stem Rollag9. This is to distinguish betweed the alternating *-g* in 'viehkag' … and the non-alternating *-g9* in 'Rollag9' (which always is realised as *g*)."

### 3b. The suffix side: trigger diacritics

Suffix realizations in `affixes/nouns.lexc` attach the *empty-string-except-a-diacritic* pattern, e.g. `affixes/nouns.lexc:184-190` (VERIFIED):
```
+Sg+Gen:%>X4 K ;
+Sg+Acc:%>X4 K ;
+Sg+Loc:%>X4s K ;
+Pl+Nom:%>X4t K ;
```
`%>` is the inflectional-suffix morpheme-boundary symbol; `X4` is a pure trigger — it has no surface realization (declared `X4:0` in the alphabet, `phonology.twolc:72-75`) and exists solely to be visible as *right context* to the twolc rules that need to know "a weak-grade-inducing suffix follows".

### 3c. The rule side: `twolc` biconditional rules keyed on the trigger set `WeG`

`phonology.twolc:110` defines the trigger set (VERIFIED):
```
WeG = X4 X5 X6 X8 X9 Q4 Q5 Q6 Q7 Q8 W1 W4 W5 W7 %^DISIMP ;
```
and the gradation rules are literally conditioned on "followed (at some distance, through morpheme-boundary/consonant material) by a member of `WeG`". A representative sample, all VERIFIED at `phonology.twolc`:

```
"Gradation: h Loss"                                    ! johka : joga (h:0)      (line 514)
  h:0 <=> _ º: Cy: ( %>: ) ( »: ( »: »: )) Vow ( StemCns:) ( %>: ) ( »: ( »: »: )) (:StemCns) WeG: ;
            where Cy in (p t k c č) ;

"Gradation: Prenasal Stops"                            ! sápmi : sámi (p:0)      (line 529)
  Cx:0 <=> _ Cy Vow ( StemCns:) ( %>: ) ( »: ( »: »: )) (:StemCns) WeG: ;
            where Cx in (p t k)
                  Cy in (m n ŋ)
            matched ;

"Gradation: Double Consonant"                          ! káffe:káfes             (line 552)
  Cx:0 <=> Vow: _ Cy ( %>: ) ( »: ( »: »: )) Vow ( StemCns:) ( %>: ) ( »: ( »: »: )) (:StemCns) WeG: ;
            where Cx in (đ f l m n ŋ r s š ŧ v)
                  Cy in (đ f l m n ŋ r s š ŧ v)
            matched ;

"Gradation: Cluster ŋ + Non-sonorant"                  ! seaŋºga:seaŋgga         (line 611)
  º:Cx <=> Vow: ŋ _ Cz Vow ( StemCns:) ( %>: ) ( »: ( »: »: )) (:StemCns) WeG: ;
            where Cx in ( g k )
                  Cz in ( g k )
            matched ;
```
The rule count: **44 named `"Gradation: …"` rules** out of **112 total named `twolc` rules** in `phonology.twolc` (VERIFIED, `grep -c`). Grade-cluster coverage runs from simple single-consonant loss (h-loss, prenasal-stop loss) through geminate simplification, preaspirated geminates, jodded double consonants, and a long tail of specific three/four-consonant clusters (`ft:ftt`, `bn:bnn`, cluster-`m`+non-sonorant, cluster-`n`+non-sonorant with two disjoint `where` sets for different consonant subsets, `ihm`/`vhl`, `lbm`/`jdn`/`vdn`, `ldnj`/`vdnj`, `rbm`/`rdn`/`rgŋ`, `rdnj`, `ist`/`vsk`, `ršt`/`ršk`/`mšk`, `kc`/`ks` ×2). Each cluster type gets its **own hand-written rule**, not a generalized "grade" transform — see §7 for why a general definition was attempted and abandoned.

### 3d. `+G3`/`+G7` as surfaced grade-class tags

`root.lexc:167-168` (VERIFIED):
```
 +G3      !!≈ * **@CODE@** Grade 2-3 for homonymies with grade 1-2, +N+G3
 +G7      !!≈ * **@CODE@** Grade 3, no consonant gradation, +N+G7
```
This is the lexicographer-facing escape hatch: for stems where grade assignment is genuinely ambiguous or lexically idiosyncratic, the analysis output itself carries a grade-disambiguating tag rather than relying purely on the twolc machinery to resolve it structurally.

**How the trigger is transmitted stem→affix boundary→rule, summarized**: the affix continuation lexicon chosen by the stem (e.g. `K`, `NPx3V`) determines *which* diacritic (if any) rides along with a given case/number/person suffix; the twolc rule then looks past the morpheme boundary (`%>:`), past intervening non-alternating consonant material (`StemCns:`), to find that diacritic in the `WeG` set. This is functionally a **feature-passing mechanism implemented as literal tape symbols**, not true unification — the "feature" (weak-grade-required) is a fixed, enumerable, finite set of about 30 distinct symbols (X1–X9, Q1–Q9, Y1–Y9, W1–W9), each hand-assigned per suffix/rule combination, documented exhaustively in `docs/docu-sme-twol.md:63-264` (a full concordance of which rules and which lexicon entries reference each symbol — VERIFIED, this doc literally exists to keep the 30-symbol space tractable for maintainers).

---

## 4. Long-distance / interweaving phenomena — the heart of the report

Two structurally different mechanisms are used for two structurally different classes of long-distance dependency, and the repo is disciplined about which is used where.

### 4.1 Flag diacritics (`@U.…@`, `@P.…@`, `@N.…@`, `@R.…@`, `@D.…@`, `@C.…@`) — for morphotactic/legality filtering

`docs/docu-sme-flag-diacritics.md:15-40` gives the canonical semantics used throughout the grammar (VERIFIED, quoting in full):
> - **U or Unification flags, `@U.feature.value@`:** accepted iff the two flags in the derivation string agree.
> - **P or Positive (Re)Setting, `@P.feature.value@`:** sets/resets the feature.
> - **N or Negative (Re)Setting, `@N.feature.value@`:** sets to the negation.
> - **R or Require Test, `@R.feature.value@`:** succeeds iff feature is currently set to value, else the path is blocked.
> - **D or Disallow Test, `@D.feature.value@`:** succeeds if feature is neutral or incompatible with value.
> - **C or Clear Feature, `@C.feature@`:** resets feature to neutral.

Cost/mechanism: compiled away entirely by `hfst-lexc`/`foma`'s flag-diacritic handling (or left in and interpreted at lookup time if `--withFlags`/hyperminimization is *not* requested — see the `WANT_HYPERMINIMISATION` option in `giella-core/am-shared/src-morphology-dir-include.am:26-29`, VERIFIED). No output symbol; purely a single-path compile/runtime filter over an otherwise-generated string. Real, distinct flag names counted (VERIFIED, `grep -oE` across all `*.lexc`, frequency-sorted):

```
95  @U.Cap.Obl@              (proper-noun capitalization required)
35  @R.Px.add@                (possessive-suffix "add" test)
32  @C.NeedNoun@               (compounding: 2nd part must resolve to N)
31  @C.NeedsVowRed@            (possessive-suffix vowel reduction gating)
27  @U.NeedsVowRed.ON@
19  @U.Cap.Opt@               (proper-noun capitalization optional, for derived adjectives)
13  @U.NeedsVowRed.OFF@
 5  @U.CmpNone.FALSE@ / @D.CmpNone.TRUE@ / @D.CmpLast.TRUE@   (compounding position legality)
 4  @C.SpellRlx@
 3  @U.CmpHyph.{TRUE,FALSE}@ / @R.SpellRlx.ON@ / @R.ErrOrth.ON@ / @D.NeedNoun.ON@ / @D.ErrOrth.ON@ / @D.CmpOnly.FALSE@ / @D.CmpHyph.TRUE@ / @C.SpaceCmp@ / @C.CmpHyph@
```
plus a `@U.number.{one..ten,zero}@` family used purely for numeral-internal agreement.

Concrete uses, all VERIFIED:

**(i) Compound legality** — `root.lexc:1148-1177` (the `ENDLEX`→`ENDLEX5` tail every word passes through):
```
LEXICON ENDLEX
   @D.CmpOnly.FALSE@@D.CmpPref.TRUE@@D.NeedNoun.ON@ ENDLEX2 ;
!! The `@D.CmpOnly.FALSE@` flag diacritic is used to disallow words tagged
!! with +CmpNP/Only to end here.
!! The `@D.NeedNoun.ON@` flag diacritic is used to block illegal compounds.
```
and `compounding.lexc:33-53` (the `R`/`RAlmostReal`/`Rreal`/`RrealAfterCmpNFlags` chain), which routes N+N, N+(V→N via nominalizing derivation), and N+(A→N) compounds legally while blocking raw N+V and N+A via `@P.NeedNoun.ON@ … @U.NeedsVowRed.ON@` pairs set at the derivational-suffix side and checked/cleared at the compound-final tail. `docs/xerox-discussion.md:326-400` preserves an actual 2000s-era e-mail exchange between the North Sámi developer and a Xerox flag-diacritics expert working through exactly this design (the "forward-looking feature requirement" pattern, named `NeedNom`/`NeedNoun` by convention) — a first-hand account of the design being invented, not retrofitted.

**(ii) Downcasing of proper-noun-derived adjectives** (`Oslo` → `oslolaš`) — `docu-sme-flag-diacritics.md:95-123`. Notable: **this was originally attempted as a `twolc` rule** ("exchanged all initial uppercase letters with an initial lowercase one if the stem was followed by the right kind of derivational suffix … still found at the end of the twol-sme.txt file, where it is commented out"), and explicitly **abandoned for a flag-diacritic solution because compile time was too long** (§7 covers this as a "gave up" case).

Compounding-side vowel-reduction gating for possessive suffixes on trisyllabic-vs-bisyllabic stems is handled the same way: `@U.NeedsVowRed.ON@`/`@U.NeedsVowRed.OFF@`/`@C.NeedsVowRed@` (31+27+13 hits) rather than any twolc syllable-counting rule.

### 4.2 twolc diacritic-trigger symbols — for phonological triggering (see §3 for the full mechanism)

Cost/mechanism, contrasted with flag diacritics: these **are** ordinary tape symbols in the lexical-side alphabet, mapped to `0` on the surface (`X1:0 … W9:0`, `phonology.twolc:72-75`), so they cost one extra symbol-pair per trigger occurrence in the lexc-compiled lexicon transducer and are visible as literal context to the parallel `twolc` rule set — they are not removed until the two-level rules have all applied. `docs/docu-sme-twol.md` documents each of the ~30 symbols exhaustively with which rules reference it and which lexicon entries emit it (quoted in §3d).

### 4.3 `Where … matched` — twolc's alpha-variable mechanism

VERIFIED, 65 occurrences of the `matched` keyword in `phonology.twolc`. Canonical form, e.g. `phonology.twolc:326-330`:
```
"Word Final Consonant Neutralization 1"  !smirezit : smires, Troandin-bisma
  Cx:Cy <=> Vow: (( CntrCns:) ( %>: ) ( »: ( »: »: )) Dummy:+) _ (º:0 k:0) ( %>: ) ( »: ( »: »: )) ( :0 - Y5: - Y6: )( :0 - Y5: - Y6: ) ( ∑ ) ( ∑ ) [ Hyph | ( »: ( »: »: )) ( ∑ ) # ]  ;
            where Cx in (z m h p g b d)
                  Cy in (s n t t t t t)
            matched ;
```
This is a genuine positional alpha-variable: `Cx` and `Cy` range in lockstep over two parallel lists (`z→s, m→n, h→t, p→t, g→t, b→t, d→t`), i.e. one rule template stands in for 7 concrete correspondence rules simultaneously, and `matched` forces index-alignment rather than the cross-product `where…in` would otherwise give. This is the closest twolc analogue to HermitCrab's alpha-variables over feature values — except here the variable ranges over *phoneme identity*, and correspondence is pairwise-by-position in a literal list rather than by a shared feature-structure variable.

### 4.4 Archiphonemes / underspecified symbols

Covered in §3a-3b: `º` (gradation site marker), `¤` (gradation-blocker / word-final-weakening-blocker, dual-purpose per `docu-sme-twol.md:44-46`: "It prevents consonant gradation, and it prevents the word-final vowel weakening i>e, u>o"), and the `e7/i7/o7/u7`, `g8/h8/m8/n8`, `b9/d9/g9/…` alternation-class letter families.

### 4.5 Boundary/trigger symbols in rule contexts

VERIFIED, `phonology.twolc:80-87`:
```
! Morpheme boundaries:
 «  ! Derivational prefix
 »  ! Derivational suffix
 %< ! Inflectional prefx
 %> ! Inflectional suffix
 #  ! Word boundary for both lexicalised and dynamic compounds
 %^ ! (exceptional) soft hyphenation point
 ∑ ! Used in front of # for dynamic compounds.
```
Six distinct boundary types are visible to rule contexts (not collapsed to a single "#"), letting rules distinguish "before an inflectional suffix" from "before a derivational suffix" from "at a real word edge" from "at a dynamic-compound edge" — this is precisely the apparatus that lets the gradation/diphthong-simplification rules skip *through* morpheme boundaries (`( %>: ) ( »: ( »: »: ))` appears in almost every rule shown above) while still stopping at a genuine word edge (`[ Hyph | # ]`). The `∑` symbol specifically marks *dynamic* (as opposed to lexicalized) compound junctures and is produced by `compounding.lexc`'s flag-diacritic-laden `RrealAfterCmpNFlags` lexicon (§4.1) — i.e., the compounding module (flag-diacritic side) hands a literal tape symbol to the phonology module (twolc side) at exactly the seam between the two mechanisms.

### 4.6 Syllable-count conditioning — resolved lexically, not computed by the FST

VERIFIED: `+Gram/3syll` (`root.lexc:183`, `!! ## Other tags … +Gram/3syll !!≈ * **@CODE@**: trisyllabic verbs`) appears as a routing/output tag throughout `affixes/verbs.lexc` (e.g. lines 1517, 1522, 1569, 1583, 1602, 1618, 1622, 1627…, all VERIFIED), always paired with a *different* continuation lexicon than the bisyllabic counterpart (`MUITALStem`, `ALISTStem`, `MUITTASJStem`, `BEAGASJStem`, `HURAIStem`, `MUITALINCH`, `OAHPAHITStem`, `NUOSKITStem`, `DeverbalNounsMUITTASJTV`). **Syllable count is never counted by a running rule** — it is a static property of the stem, known when the lexicographer writes the lexc entry, and is dispatched purely by which sublexicon the stem's entry points to. This sidesteps the general problem of "count syllables at an arbitrary distance" entirely: the lexc graph doesn't need to look back across material to know syllable count, because the classification happened once, off-line, by a human, at entry-authoring time.

### Summary table for §4

| Mechanism | What it does | Cost | Where used | Nearest formal analogue |
|---|---|---|---|---|
| Flag diacritics `@U/P/N/R/D/C@` | Single-path compile/runtime filter, no output symbol | Compiled away (or interpreted per-lookup without `--withFlags`) | Compounding legality, capitalization, orthography-variant gating, possessive vowel-reduction gating | Unification of atomic feature values |
| twolc trigger diacritics (`X1…W9`) | Literal deletable tape symbols visible to parallel rules | One symbol pair per occurrence in the lexc transducer, until twolc rules apply and delete it | All gradation, diphthong simplification, metaphony, vowel shortening | Archiphoneme / feature-passing without unification |
| `Where…matched` | Positional alpha-variable pairing two/three parallel phoneme lists | One rule stands for N correspondence pairs | Word-final neutralization, metaphony, general vowel-alternation rules | HermitCrab alpha-variables (see §8) |
| Archiphoneme letters (`º ¤ e7 g8 b9…`) | Lexical pre-classification of alternation behavior, baked into the spelling of the morpheme | Zero runtime cost — resolved by which symbol the lexicographer typed | Gradation site marking, non-alternation marking | HC underspecified feature values in a segment |
| Six-way boundary symbols (`« » %< %> # ∑`) | Let rules distinguish morpheme-boundary kind without look-around cost | One symbol per boundary occurrence | All gradation/diphthong rules' contexts | Stratum/domain boundary marking in SPE-style rules |
| Syllable-count tags (`+Gram/3syll`) | Static lexical pre-classification, no counting at runtime | Zero — resolved at lexc-authoring time | Trisyllabic vs. bisyllabic verb paradigms | Lexical diacritic / arbitrary stem-class feature in HC |

---

## 5. Rule ordering and composition

**VERIFIED, from `giella-core/am-shared/twolc-include.am:34-40`, `lexc-include.am`, `lookup-include.am`, `src-morphology-dir-include.am:180-260`, and `src-fst-dir-include.am:255-322`.** The build is a **hybrid**, with a clean seam:

1. **Lexicon**: all `.lexc` sources are concatenated (`.generated/lexicon.lexc`, `src-morphology-dir-include.am:134-136`) and compiled once with `hfst-lexc` (or, on the `CAN_FOMA` path, read directly by the standalone `foma` binary via `read lexc`) into a single acceptor `lexicon.hfst`/`lexicon.foma`.
2. **Phonology**: `phonology.twolc` is compiled *once* with `hfst-twolc --resolve` (`twolc-include.am:26-40`, VERIFIED — `HFSTTWOLFLAGS=--resolve`) into a single rule transducer. `--resolve` is the twolc engine's standard behavior of **intersecting all parallel two-level constraints together** and resolving conflicts — this is genuinely the Koskenniemi two-level model (parallel constraints simultaneously true), not a cascade, for the phonology layer.
3. **Composition seam**: the lexicon and the (already-internally-intersected) rule transducer are then **composed** — `src-fst-dir-include.am:279-288` (VERIFIED):
   ```
   .generated/generator-raw-gt-desc.tmp1.hfst: morphology/.generated/lexicon.hfst \
                        morphology/.generated/phonology.compose.hfst $(GENDIR)
       $(HFST_DETERMINIZE) ... $< \
       | $(HFST_MINIMIZE) ... \
       | $(HFST_COMPOSE_INTERSECT) ... -2 morphology/.generated/phonology.compose.hfst \
       | $(HFST_MINIMIZE) ... -o $@
   ```
   Note this uses **`hfst-compose-intersect`**, not a plain compose — i.e. even the lexicon∘phonology seam is done with an intersecting composition (relevant background: `hfst-compose-intersect` is optimized for composing a large lexicon with a rule transducer without blowing up intermediate states, by lazily intersecting rather than materializing the full compose first).
4. **Cascade**: everything downstream of that raw analyser — tag reordering (`filters/reorder-tags.sme.regex`), semantic/subpos tag reordering, compound-tag splitting (`split-CmpN-tags.regex`, `split-CmpNP-tags.regex`), illegal-derivation-string removal, homonymy/variant/dialect tag stripping for the TTS-oriented generator variant — is applied as an explicitly **ordered `.o.` composition chain** of independent replace-rule transducers, e.g. `src-fst-dir-include.am:311-322` (VERIFIED):
   ```
   $(PRINTF) "read regex \
         @\"filters/reorder-tags.$(GTLANG).$*\" \
     .o. @\"filters/reorder-subpos-tags.$*\" \
     .o. @\"filters/reorder-semantic-tags.$*\" \
     .o. @\"$<\" \
     ;\n save stack $@\n quit\n" | $(XFST_TOOL)
   ```
   and the TTS-normalization generator target in `src/fst/Makefile.am:136-150` chains **eleven** successive filter compositions (`remove-derivation-position-tags`, `remove-semantic-tags`, `remove-homonymy-tags`, `remove-variant-tags`, `remove-dialect-tags`, `remove-norm-comp-tags`, `remove-usage-tags`, `remove-ABBR-strings`, `remove-ACR-strings`, `remove-derivation-strings`, `remove-error-strings`, `remove-PUNCT-strings`, all VERIFIED at that line range) — a genuinely long ordered cascade, explicitly analogous to what motivates HC's ordered strata, but operating entirely on the **tag string**, never on phonological segments. So:

**The seam is exact**: *all phonological rewriting happens inside one intersected `twolc` rule set, composed exactly once with the lexicon. Everything after that point is tag-string surgery via an ordered cascade of independent replace-rule transducers.* This design sidesteps the two-level-vs-cascade ordering-paradox problem (the classic reason cascades need strata) by construction: the phonological rules never have to be sequenced relative to each other because two-level rules are declarative constraints, not procedural rewrites, and the *only* thing that could create an ordering paradox — the tag-string cascade — is kept deliberately free of any phonological content, so there is nothing for the filters to accidentally feed back into the phonology.

**Two build backends exist side by side** (`CAN_HFST` and `CAN_FOMA` in `src/fst/Makefile.am:47-93`, both VERIFIED) and a *third* axis (`WITH_FOMA` in `giella-core/configure.ac:20-23`, VERIFIED) controls whether the **HFST tools themselves** emit foma's native binary format (`HFST_FORMAT=--format=foma`) as their internal transducer backend — this is different from the standalone `$(FOMA)` binary path. **This distinction directly matters for the PanGloss question of running on Divvun infra**: the lexicon side has a genuine, live, standalone-foma build path (`.generated/%.foma: .generated/%.lexc … "$(FOMA)" -e "read lexc $<"`, `lexc-include.am:32-38`, VERIFIED). The **phonology-to-standalone-foma path is not** — the only code that would build `phonology.compose.foma` from raw HFST rule output by splitting, intersecting, and reading via AT&T format into standalone foma is **entirely commented out** in `giella-core/am-shared/twolc-include.am:42-57` (VERIFIED, quoted in full):
```
######## Foma build rules (based on Hfst): #######
### Commented out for now, they interfere with proper Foma builds, and do not
### work for the Hfst + Foma combo.
#%-phon.foma: %-phon.hfst
#	$(AM_V_HSPLIT)$(HFST_SPLIT) -p RULE_PARTS $<
#	$(AM_V_at)cp RULE_PARTS1.hfst $@.hfst
#	$(AM_V_INTRSCT)for f in $$(ls RULE_PARTS*); do \
#		  $(HFST_INTERSECT) -1 $$f -2 $@.hfst \
#		| $(HFST_MINIMIZE) > $@.tmp.hfst && cp $@.tmp.hfst $@.hfst; done
#	$(AM_V_FST2TXT)$(HFST_FST2TXT) --do-not-print-weights -i $@.hfst > $@.att
#	$(AM_V_FOMA)$(PRINTF) "\
#		read att $@.att\n\
#		save stack $@\n\
#		quit\n" \
#		| $(FOMA) $(VERBOSITY)
#	$(AM_V_at)rm -f RULE_PARTS*.hfst $@.tmp.hfst $@.att
```
**INFERRED**: the practical implication is that lang-sme's *full* analyser (lexicon+gradation+everything) as actually shipped is built through HFST tooling (optionally with foma as HFST's internal backend format via `WITH_FOMA`), not through the standalone `foma`/`divvun/foma-rs`-compatible toolchain reading raw `.twolc` source directly. If PanGloss wants to run a lang-sme-style grammar purely on `divvun/foma-rs`, the twolc rule compilation step (`hfst-twolc --resolve`) has no drop-in foma-native equivalent in this repo's own build infrastructure — it would need to be replicated (there does exist third-party `foma`-ecosystem tooling for two-level rules historically, but it is not what this repo's Makefiles invoke). This is exactly the boundary the shared brief asked us to find.

---

## 6. Scale and build facts

What the repo and its docs *actually* state (I did not run any build, per instructions):

- **Lexc scale** (VERIFIED byte sizes and stanza/entry counts): `stems/nouns.lexc` 6.55 MB / 19 sublexica / ~95K `+N` entries; `stems/verbs.lexc` 622 KB / 18 sublexica / ~599 `+V` entries; `stems/adjectives.lexc` 641 KB / 10 sublexica; `stems/sme-propernouns.lexc` 939 KB; `affixes/` totals 1561 `LEXICON` stanzas across 10 files; whole morphology module ≈1811 `LEXICON` stanzas (§2).
- **twolc scale** (VERIFIED): `phonology.twolc` 1781 lines, 112 named rules, 44 of them gradation rules; ~30 distinct diacritic-trigger symbols (`X1-X9,Y1-Y9,Q1-Q9,W1-W9`) each individually documented in `docs/docu-sme-twol.md`.
- **Backlog scale** (VERIFIED): `morphology/incoming/missing2018-08-16.txt` is 5.4 MB and `missingSIKOR060121.txt` is 2.2 MB of attested-but-not-yet-lexicalized word forms, i.e. active curation backlog several times the size of some entire stem files.
- **Compiled-transducer state/arc counts for the actual production analyser: unknown / not found in this repo.** The only state/arc counts present anywhere in the checkout are from a small *teaching example* in `docs/docu-sme-flag-diacritics.md:142-153` (Xerox-book demo transducer, not lang-sme itself): "defined UC: 568 bytes. 2 states, 26 arcs, 26 paths" / "5.2 Kb. 6 states, 356 arcs, Circular" / lexicon "1.5 Kb. 32 states, 35 arcs, 12 paths" / composed result "1.8 Kb. 38 states, 45 arcs, 18 paths." These are illustrative toy numbers from a 2000s design discussion, not the shipped North Sámi analyser — I am flagging this explicitly rather than extrapolating a production estimate.
- **CI / build-time facts**: `.github/workflows/` in `lang-sme` contains only `docs.yml` (publishes documentation via a shared `giellalt/.github` reusable workflow) and `zulip.yml` (chat notification on push). **No CI job that builds and reports the FST, and no recorded build-time numbers, were found in this repo.** (INFERRED: actual FST builds likely happen in `giellalt`'s separate infra/CI repos, e.g. `infra-scripts`/nightly-build systems, which were out of scope for this clone — noting as a gap rather than guessing at numbers.)
- **In-source regression testing convention** (VERIFIED, `phonology.twolc:240-278`): every rule carries inline positive (`!!€`) and negative (`!!$`) test word pairs immediately below it, e.g. `phonology.twolc:557-558`:
  ```
  !!€ káffeX4s
  !!€ ká0fe0s
  ```
  extracted by "a final script" (per the in-file comment) into separate test files and checked automatically — i.e. per-rule unit tests co-located with the rule, a documented methodology, not merely inferred from scattered examples.

---

## 7. What they explicitly gave up

Four concrete, sourced cases of "we wanted X, couldn't express it cleanly, and did Y instead":

**(a) Structural definition of gradation-grade classes (G1/G2/G3) abandoned in favor of per-cluster hand-written rules.** `phonology.twolc:140-165` (VERIFIED, quoting the developers' own running commentary):
```
G1  = Cns | h [l|r] | n j ;
RealCns    = Cns - º:0 ;
all2Cns    = Cns Cns ;
real2CnsG3 =  b: m | d: n | g: ŋ | b: b: | d: d: | g: g: | z: z: | ž: ž: ; ! This could be the problem
2CnsG2     = all2Cns - real2CnsG3 ;
3CnsG2     = Cns RealCns Cns ; !!!!!!!!!!!!!!                        ! đºb:đbb, đºg:đgg, ...
fake8CnsG3 = Cns: º: Cns ;                                          ! đºb:đ0b
3CnsG3     = h [ c c | č č | p p | k k | t t ] | [ d d | l l ] j  ;

!G2  = 2CnsG2 | 3CnsG2 ;
! 2CnsG2 is well-defined, 3CnsG2 is not. :-(

!G3  = fake8CnsG3 | real2CnsG3 | 3CnsG3 ;
! => All 3 parts of the G3 definition are flawed

!G12 = G1 | 2CnsG2 ; ! testing G2 parts  ! This we do while waiting for 3CnsG2.
```
And, in the same file, the developers explicitly debate three strategies for a diphthong-simplification sub-problem and document why they rejected two of them (`phonology.twolc:1204-1240`, VERIFIED):
> "1. lexical strategy: … con: gives a lexical solution to a basically morphophonological problem" … "2. morphophonological strategy, the smj way. … con: defining G3 is a mess, at best" … "We have optional dipht. simpl. only in G3-nouns and no cons grad. Going for 2:"
followed immediately by more commented-out attempted rules with their own failure annotations. **The shipped grammar never has a working general `G3` definition** — every one of the 44 gradation rules and every diphthong-simplification rule instead names its own explicit consonant/cluster list in a `where … in (…)` clause. The abstraction was attempted, documented as flawed three separate times, and abandoned; the working system is 44+ enumerated special cases, each individually regression-tested (§6).

**(b) A `twolc` rule for proper-noun-derived-adjective downcasing was written, then abandoned for compile-time cost, in favor of flag diacritics** — `docu-sme-flag-diacritics.md:107-123` (VERIFIED, quoted in full in §4.1). Notably the flag-diacritic replacement was **itself unresolved at the time of that doc** ("This is fixed, isn't it? … so far, we don't have a working solution"), and the doc preserves a genuinely confused xfst debugging session (`flex scanner jammed`) trying to get a textbook flag-diacritics example working at all. (The tags `@U.Cap.Obl@`/`@U.Cap.Opt@` are now live in `root.lexc`, 95+19 hits — so this was eventually resolved, but the repo preserves the record of the abandoned first approach and the rocky second one.)

**(c) A gradation alternation (`šž`) commented out because it caused bad overgeneration, replaced by lexical listing.** `phonology.twolc:568-577` (VERIFIED):
```
!"Gradation: šž"  !Commented out because it causes wordforms like *muitalepmožis. Instead we now have !this in lexicon:
!nju0nuš:njunnuš MALIS ;
!nju0nuš:njunnuž MALIS ; !SUB
! Cx:Cy <=> Vow: _ X2: Vow ( StemCns:) (:StemCns) ;	! njunnoša:njunnožis
```
i.e. a genuine morphophonological rule was pulled because it overgenerated, and the correct forms for the specific lexemes affected were instead hand-listed directly in the lexicon (`MALIS` sublexicon) — the reverse move from (b): here the FST-general solution was abandoned *for* a lexical one, exactly the opposite direction. Also `phonology.twolc:561-565` documents an ad hoc rule added purely to cover one substandard spelling variant (`Ruotta:Ruotas`), annotated "Note: I added a t rule, due to substandard Ruotta:Ruotas" — a rule justified by a single attested irregular form, not a productive pattern.

**(d) Overgeneration is an explicitly named, accepted design property, with the intended remedy being downstream composed filters — the same "overgenerate then prune" philosophy PanGloss itself uses.** `docs/xerox-discussion.md:326-369` preserves a real design discussion (VERIFIED, quoting the relevant exchange):
> "At the moment, I let my recursive lexicon R accept continuation both to N, V and A, thus I also allow the illicit compounds stem+N & stem+A / stem+N & stem+V … but as you can see, it is allowed by the 8at the moment9 overgenerating parser." … "KRB: you could, of course, produce the overgenerating lexicon and then remove the overgeneration by composing filters on the top. They would have to be carefully written to allow multiple-part legal compounds, but it would almost certainly be possible to match and filter out illegal compounds that way. But let's assume that you want to use flag diacritics."
The team chose flag diacritics over compose-a-filter-afterward for this particular problem, but the alternative — **overgenerate, then compose a pruning filter** — was explicitly considered as a *valid, standard* technique and is exactly what the `filters/*.regex` cascade (§5) does do for other problems (tag legality, not compound legality). A related, less-hedged admission is preserved in the (now-superseded, kept "for nostalgic reasons" per its own header) `docs/docu-sme-bugs.md:70-74`: "The parser gives bealkálas from bealkit, which is correct, but it overgenerates to joavdálas for joavdit, where the correct form should be jovdelas." — a named, un-fixed overgeneration bug in the historical record.

**Also relevant but not a "give-up": the guesser is a structurally separate, hand-built transducer, not the analyser run permissively.** `src/fst/guesser.xfscript` (VERIFIED, quoted in §1/here) defines an unknown-word guesser from scratch (`define PossWord (s) cons^{0,2} [vowel|dipth] (i) [cons|cons2|cons3|cons^3] vowel cons^{0,1};`) with its own phonotactic approximation of legal Sámi syllable/cluster shapes, substituted in for a placeholder root (`^GUESSNOUNROOT`) in a saved lexicon (`g-sme.save`). This is worth noting for the PanGloss comparison: GiellaLT's answer to "what do you do with words the lexicon doesn't have" is a **separate, cheaper, explicitly approximate FST**, not "run the real grammar more permissively."

---

## 8. Mapping to HermitCrab constructs

| Divvun/GiellaLT mechanism | Nearest HC construct | Mapping quality |
|---|---|---|
| Flag diacritics (`@U/P/N/R/D/C@`) for compounding/orthography legality | MPR (morpheme property realization) gating features, or plain feature-structure agreement between morphemes in a rule | **Lossy but tractable both directions.** HC's feature-structure unification is strictly more expressive (structured values, not just atomic flags) than flag diacritics' atomic-value set/test/clear semantics — translating HC→flag-diacritics means flattening a feature structure into a finite enumeration of named atomic flags (doable, but loses generality if the HC grammar uses genuinely structured features). Translating flag-diacritics→HC is close to lossless: each `@op.feature.value@` maps onto a boolean/enum MPR feature check. |
| twolc diacritic-trigger symbols (`X1…W9`) for gradation/diphthong/metaphony triggering | Nothing directly analogous — closest is an **MPR feature carried as an actual segment-adjacent marker**, or, more accurately, a **rewrite-rule trigger encoded via an alpha-variable on a stratum-local feature** | **Hard, and the hard direction is HC→FST, not the reverse.** HC expresses "this suffix requires weak grade on the stem" as ordinary morphophonological rule feeding/ordering across strata plus feature agreement at rule-application time — it does not need a literal deletable tape symbol because its engine re-evaluates structure at each stratum. Compiling an HC rule of this kind *into* an FST naturally reproduces exactly this twolc-diacritic pattern (insert a trigger symbol at the suffix, write a rule keyed on "trigger symbol somewhere to the right, across skippable material", delete the trigger on the way out) — which is reassuring: it means the mechanism PanGloss would need to synthesize when lowering an HC rule to foma is *the same one GiellaLT hand-built*, not something novel. The reverse (reading a twolc-diacritic grammar back into HC form) is comparatively easy: each trigger symbol is just sugar for an HC-side ordered rule + feature check. |
| Archiphonemes (`º ¤ e7/i7/o7/u7 g8/h8/m8/n8 b9/d9/g9…`) | HC's underspecified feature values on a segment in a lexeme's underlying representation | **Close to lossless in both directions.** This is precisely what underspecification is for in a feature-based framework: `g8` = "a `g` unspecified for [alternates]" is just a segment whose relevant feature has a cover value that phonological rules resolve later; HC's feature-structure segments do the same thing natively and arguably more legibly (a real feature name/value rather than a numeral suffix convention memorized from a doc file). |
| `Where … matched` positional alpha-variables | HC's alpha-variables over feature values in a rewrite rule | **Near-lossless, and this is the strongest structural match in the whole grammar.** twolc's `matched` keyword literally *is* the two-level-formalism's implementation of the same idea HC calls an alpha-variable: one rule template, several feature/segment correspondences varying in lockstep. The chief difference is domain — twolc's variable ranges over enumerated phoneme lists (`Cx in (z m h p g b d)`), while HC's alpha-variables typically range over feature values (voicing, place) that then determine segment realization compositionally. Where the HC grammar's alternations are stated over phonemes directly (as North Sámi gradation effectively is), the translation is close to mechanical. |
| Six-way boundary symbols (`« » %< %> # ∑`) distinguishing prefix/suffix/inflectional/derivational/word/compound edges | HC's stratum boundaries plus its distinction between derivational and inflectional affixation position classes | **Reasonably close, lossy in detail.** HC keeps this information implicitly, in *which stratum* a rule belongs to and *which slot* an affix occupies, rather than as an explicit symbol visible to rule contexts across strata. Translating HC→FST would need to *reify* stratum/slot identity as one of these literal boundary symbols so that a single composed rule set (no more strata after composition) can still make the distinctions HC's ordered strata used to make for free. This reification is exactly the "how do we avoid the ordering-paradox problem cascades have" question from §5 — and GiellaLT's answer (six boundary symbols + one intersected rule set) is a concrete existence proof that it is possible, at least for this grammar's phenomena. |
| Syllable-count conditioning via static lexc routing (`+Gram/3syll` → distinct sublexicon), not FST-side counting | HC stem-class / lexical property tagging (a feature on the root entry, checked by rule context or by MPR gating) | **Essentially lossless — and this is good news for PanGloss.** Neither system tries to count syllables at match time; both push the classification to lexicon-authoring time and let morphotactic routing (HC: stem-class feature + rule applicability; GiellaLT: which sublexicon a stem's lexc entry points into) do the work. This is one mechanism where "compile HC to foma" requires no new invention at all — a stem-class feature in HC maps directly onto "route this stem to sublexicon X vs Y" in lexc, with zero loss. |
| Two-level (`twolc`, parallel/intersected) for phonology vs. ordered `.o.` cascade (`filters/*.regex`) for tag-string post-processing | HC's single ordered-stratum rewrite-rule model, applied uniformly to both phonology and (via templates/blocks) morphotactic bookkeeping | **Partial mismatch, favorable for PanGloss's architecture.** HC does not distinguish "phonological rule" from "tag bookkeeping rule" formally — both are ordered rewrite rules in a stratum. GiellaLT's clean separation (declarative intersected rules for phonology; ordered cascade only for tag-string surgery, never phonology) is *narrower* than what HC allows but is exactly the separation PanGloss's own architecture already assumes (foma handles the FST-level proposer, HC re-confirms/prunes) — meaning HC→foma lowering for the *phonological* component can, in principle, target the two-level/intersected style rather than needing a full ordered-stratum cascade compiled into foma's `replace` rules, IF the HC grammar's ordering is genuinely only used to prevent phonological rules from feeding each other pathologically (the classic reason for strata) rather than for true feeding/bleeding relationships. Where HC's strata *do* encode true feeding order (rule A's output is rule B's structural trigger), that ordering is real information that a flat intersected twolc rule set cannot represent, and the honest translation there is the ordered `.o.` cascade instead — i.e. **which HC strata map to "twolc-style intersection" vs. "cascade" cannot be decided in general; it depends on whether the specific strata in the source HC grammar have true rule-feeding relationships, which must be checked per grammar, not assumed.** |

**Overall honesty check on translation direction difficulty**: Lowering HermitCrab (feature-structure, alpha-variable, ordered-stratum) grammars into GiellaLT's toolset (flag diacritics + twolc diacritic symbols + intersected two-level rules + ordered tag-cascade) looks **mechanically constructible for every mechanism this report found**, because in each case GiellaLT's device is a *strictly more concrete, more circumscribed* instance of the more general HC device (finite flag alphabet vs. feature structures; enumerated alpha-variable lists vs. general alpha-variables; static lexical stem-class tags vs. counting). The one genuine risk is exactly the two-level-vs-cascade seam (§5, last row): if an HC grammar's stratum ordering encodes true rule feeding (not just paradox-avoidance), a naive lowering into a single intersected `twolc`-style rule set will silently produce a different — likely overgenerating — language, and the fix (an explicit ordered cascade, foma `replace` rules composed in sequence) is exactly what GiellaLT itself falls back to for its own non-phonological ordering needs. Going the *other* direction — reading a shipped GiellaLT-style FST back into an HC-equivalent form — is comparatively harder for the diacritic-trigger family (§4.2) specifically, because the ~30 hand-assigned trigger symbols carry no explicit semantic label (unlike HC feature names): reverse-engineering "what feature is `Q7` actually standing for" requires exactly the kind of manual cross-referencing `docs/docu-sme-twol.md` had to be written to support, i.e. it is recoverable, but only with real linguistic analysis effort, not by mechanical inspection of the FST alone.
