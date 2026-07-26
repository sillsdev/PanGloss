# GNU Aspell — system profile

Profiled against the fixed comparison rubric for the PanGloss spell-checking research
(`docs/research/spellcheck/00-synthesis.md` and sibling reports 01–06). Cross-references
sibling reports rather than repeating them; phonetic-hashing background (Soundex/Metaphone
mechanics, Editex, PanPhon/ALINE, the general information-loss argument against hash-bucket
phonetic matching) is already covered in `02-phonological-distance.md` §1 and is only
restated here where Aspell's specific implementation adds a new fact.

**Labeling convention** (matches 01/02): **measured** = read directly from a primary source
(fetched and quoted/paraphrased faithfully); **asserted** = a primary source states it
without derivation/benchmark; **secondary-summary** = primary text could not be extracted,
relying on an abstract/search summary; **synthesis** = my own conclusion combining sourced
facts, flagged as such. **Aspell-specific environment note**: `aspell.net` itself could not
be fetched directly in this environment — every direct `WebFetch` against `aspell.net`
failed with a TLS certificate mismatch (`Host: aspell.net is not in the cert's altnames:
DNS:*.sf.net, DNS:sf.net...`), i.e. the site's current TLS termination presents a
SourceForge certificate, not one valid for `aspell.net`. This is a **new, environment-level
unfetchable-source finding**, distinct from the PDF-extraction failures reported in 01/02.
`web.archive.org` was also blocked outright by the fetch tool. All aspell.net manual pages
below were retrieved through a text-extraction proxy (`r.jina.ai`) as a workaround; treat
manual-page content as measured (the proxy returns the page's own text) but be aware the
GNU Aspell manual has changed page names across versions (0.50 → 0.60.8.2) — several current
links referenced by cross-links within the manual itself now 404 (`Notes-on-the-Algorithms-Used.html`,
`Notes-on-8-bit-Characters.html`, `Format-of-the-Main-Word-List-File.html`, `Notes-on-Dictionaries.html`);
these are flagged individually below where they bear on a field. Source code facts are read
directly from `github.com/GNUAspell/aspell` (the canonical upstream mirror, confirmed via
aspell.net's own resources page — measured).

---

## ARCH

**measured, synthesized from multiple primary pages.** One-line: *near-miss edit-distance
generation (Ispell-derived) merged with a modified, tabularized Metaphone-family phonetic
code ("soundslike"), scored by a weighted average of weighted edit distance in both the
raw-word space and the soundslike space, over a word list compressed with prefix/suffix
affix rules.* Aspell's own manual states the merge directly: "the magic behind my spell
checker comes from merging Lawrence Philips['] excellent Metaphone algorithm and Ispell's
near miss strategy" (`aspell.net/metaphone/`, `aspell.net/man-html/Aspell-Suggestion-Strategy.html`).
This is a two-signal weighted-cascade design, not the single unified weighted-FST composition
report 02 recommends for PanGloss (§6 there) — Aspell computes near-miss edits and
soundslike-edits as two separate distance computations and *then* averages the two
resulting scores, rather than composing one substitution-cost function used by one search.

## LEXICON

**measured.** A flat, per-language compiled word list — not a generative morphology. The
compiled dictionary (`aspell.net/dev-html/Part-1-...Compiled-Dictionary-Format.html`,
GNU Aspell Developer's Manual, primary source) is a hybrid on-disk structure: a "clean form"
(lowercased, de-accented) hash table for lookup, soundslike-keyed groups linked by 32-bit
byte offsets (not pointers, specifically so the file can be `mmap`ed and shared read-only
across concurrently running Aspell processes), and a sequential data block enabling
iteration without going through the hash table. Word-list entries are `word[/affixflags]`
pairs (`Working-With-Affix-Info-in-Word-Lists.html`); affix compression (below) expands
each entry into the finite set of surface forms its flags license — the underlying model is
still "the dictionary is a big list of legal strings," structurally the same "acceptance =
membership" model that `01-lexical-distance.md` §4.1 contrasts with PanGloss's
propose→confirm *parse* acceptance model. There is no analyzer, no root/affix
morphosyntactic feature output, and no notion of an ungenerated wordform being "grammatical
but unseen" the way PanGloss's FST-propose+HC-confirm pipeline treats it.

## MORPHOLOGY

**measured — affix compression is a compression technique over an enumerated word list,
not generative morphology; answers the prompt's expected "no."** `munch-list`
(`Working-With-Affix-Info-in-Word-Lists.html`) works *backward* from an already-complete
list of surface wordforms to find "a minimal (or close to it) set of roots and affixes that
will match the same list of words" — i.e. it is a dictionary-size optimization discovered
from existing forms, not a rule system that derives new, previously-unlisted forms. The
language data file's `affix` field (`The-Language-Data-File.html`) only configures how
aggressively affix-flagged entries get *expanded back out* at dictionary-compile/lookup
time (`affix-compress`, `partially-expand`) — there is no productive concatenative or
templatic rule application beyond what `munch-list` already baked into the flags at compile
time, and no non-concatenative (root-and-pattern, reduplication, infixation, vowel harmony)
capability of any kind is mentioned anywhere in the manual. Confirms the prompt's
expectation directly: **no unbounded inflection without enumeration.** Every legal wordform
Aspell can recognize was, in some form, present in the training word list that `munch-list`
compressed; nothing is synthesized at check-time that wasn't already implicit in that list.
This is the single sharpest contrast with PanGloss's reason for existing (per
`00-synthesis.md`'s framing of the original plan's core problem: "the lexicon is a grammar
... not a dictionary file").

## ERRORMODEL

**measured, primary source `Aspell-Suggestion-Strategy.html` (via proxy) + `metaphone` page.**
Five-step pipeline, quoted/paraphrased directly from the manual:

1. Convert the misspelled word to its soundslike (its Metaphone code, for English).
2. Find dictionary words whose *soundslike* is within 1–2 edit distance of the query's
   soundslike — at distance 1 by trying all single-edit variants, at distance 2 by scanning
   the dictionary with early-termination scoring (i.e. a bounded, not exhaustive, scan).
3. Separately pull in curated real-word-error pairs from a **replacement dictionary**
   (`repl-table`/`.prepl` file, format `misspelled_word correction`, one pair per line;
   ships pairs like `teh → the`) — a **static, hand-curated confusion list**, not a learned
   or contextual real-word-error detector (see DETECTION below for why this is a narrower
   claim than it sounds).
4. **Score/rank**: each candidate gets "the weighed average of the weighed edit distance of
   the word to the misspelled word and [of] the soundslike equivalent" — i.e. two separate
   weighted-edit-distance numbers (one over raw letters, one over soundslike codes) are
   each computed, then averaged. "Weighted edit distance" itself just means the near-miss
   operations (insert space/hyphen, transpose two adjacent letters, substitute one letter,
   delete a letter, add a letter) carry different costs rather than a uniform cost of 1 each
   — Aspell's own manual is explicit that the soundslike itself "is a rough approximation of
   how the word sounds. It is not the phoneme of the word by any means" (i.e. Aspell's own
   documentation disclaims phonetic precision, consistent with report 02's general
   information-loss argument against hash-style phonetic matching).
5. Deduplicate and substitute.

**How this compares to report 02's central finding**: this is exactly the
"cascade with a late fixed-formula combination" pattern report 02 flags as inferior to one
unified weighted composition — Aspell doesn't have Hunspell's harder gating problem (score
incomparability across *sequential stages*, per report 02 §6's Hunspell/Norvig discussion),
because it computes both distances for the same candidate set and averages rather than
falling back; but it is still two independently-computed distance metrics combined by a
fixed averaging formula, not a single substitution-cost function integrated into one search
the way `divvunspell`/HFST compose an error-transducer with a lexicon transducer (02 §6,
01 §3.2). The **modified, tabularized Metaphone** Aspell actually ships (`aspell.net/metaphone/`,
primary/measured) is described by its own author-maintained page as reaching "a 1% better
score" when swapped for a Double-Metaphone variant on some test data, with the explicit
caveat "for some words the new Metaphone algorithm lead to worse results" — an
**author-reported, not independently benchmarked** number, same caveat class report 02 §1
already applies to Metaphone 3's ~99%-accuracy claim.

## DETECTION

**measured + synthesis.** Aspell's primary function is **correction** (suggestion ranking
for a token already known to be absent from the dictionary); *non-word* detection is
trivial and free (a word not in the compiled word list, after `store-as`
lowering/de-accenting, is flagged). **Real-word error handling is much narrower than it
sounds**: the only "real-word error" mechanism found in primary sources is the static
replacement dictionary (`teh → the`, a fixed hand-curated confusion table, ERRORMODEL step
3) — this is a **known-typo lookup table**, not detection of a real-word error via sentence
context (e.g. Aspell has no mechanism to flag "their" as wrong in "they went to *their*
house is nice" — that would require agreement/context checking it does not have). No
grammar-checking, no agreement checking, no real-word error detection driven by context was
found in any manual page fetched. This confirms report `00-synthesis.md`'s framing that
"detection is the harder half" (finding #2, citing Constraint Grammar as the missing layer)
— Aspell supplies zero of that layer; it is a pure non-word corrector plus a hand-authored
confusion list.

## CONTEXT

**measured — confirms the prompt's expectation of "none."** No sentence-context modeling,
no n-grams (word, morpheme, or otherwise), and no statistical language model of any kind
appear anywhere across the Introduction, Suggestion-Strategy, Options, or "How Aspell Works"
(older 0.50 manual) pages fetched. The 0.50-era "How Aspell Works" chapter — fetched
directly as a primary source specifically to check for this — describes the same five-step
near-miss+soundslike pipeline with **no mention of context, n-grams, or statistical language
models** at all; ranking is purely per-candidate-word scoring against the single misspelled
token in isolation. This is architecturally consistent with report 04's finding that a
context-free / n-gram-free design is a defensible baseline for very low-resource settings
(04's own conclusion: word n-grams are the textbook worst case for morphologically rich
languages at small corpus sizes) but also means Aspell has zero mechanism for the
free-real-word-confusion-set idea report 04 flags as falling out of PanGloss's own analyzer
"for free" — Aspell has no analyzer to fall out of.

## SEMANTICS_POS

**measured — confirms the prompt's expectation of "no."** The main word list entry format
is `word[/affixflags]` only (`Working-With-Affix-Info-in-Word-Lists.html`, measured); the
language data file's documented fields (`charset`, `special`, `soundslike`,
`invisible-soundslike`, `repl-table`, `keyboard`, `sug-split-char`, `affix`, `store-as`,
`norm-required`, `normalize` — full list read directly from `The-Language-Data-File.html`)
contain no field for part-of-speech, semantic domain, or any other lexical-semantic
annotation. No POS tag, feature bundle, or semantic-domain concept appears in any fetched
manual page, source-directory listing, or search result. This is a hard structural
ceiling relative to PanGloss's LibLCM/FLEx-sourced data model (POS + inflectional features +
semantic domains, per the task framing) — Aspell's data model has literally no slot to put
that information even if it were available.

## DATA_REQ

**measured (file format/fields) + synthesis (quantified effort).** Per
`The-Language-Data-File.html` (primary, fetched) and `Adding-Support-For-Other-Languages.html`
(primary, fetched — itself thin: "You basically need to create the language data file, and
compile a new word list... Adding a language to Aspell is fairly straightforward," with no
elaborated complexity estimate given in-manual), the minimum artifact set for a new language
is:

1. **`lang.dat`** — a small config file (mandatory: `name`, `charset`; optional:
   `data-encoding`, `special` non-letter characters with begin/middle/end position flags,
   `soundslike`, `invisible-soundslike`, `repl-table`, `keyboard`, `sug-split-char`, `affix`
   compression settings, `store-as`, `norm-required`, `normalize`) — on the order of a dozen
   key-value lines; this part is genuinely small.
2. **A word list** to compile into the main dictionary (`aspell --lang=lang create master
   ./base < wordlist`, `Creating-an-Individual-Word-List.html`, measured) — size is
   open-ended and is the dominant cost; for a hyper-minority language with no prior
   digitized wordlist this is the same "a wordlist doesn't exist for these languages"
   problem `00-synthesis.md` opens with, i.e. the actual bottleneck is identical to
   PanGloss's own founding problem, not solved by Aspell's tooling.
3. **Optionally, an affix file** (prefix/suffix flag rules) to shrink the compiled word list
   — genuinely optional; skipping it just means a larger flat word list with no flags.
4. **Optionally, a `name_phonet.dat` soundslike table** (`soundslike` field naming convention
   confirmed via `phonet.cpp`, measured from source, plus corroborating secondary web
   summary of the same filename convention) — this **is** the "hand-authored
   phonetic-equivalence table" the prompt asks about. Quantifying its size: reading
   `phonet.cpp` directly (primary source, measured) shows each rule line is a
   pattern→replacement pair with optional modifiers — `(...)` character-class groups, `-`
   for backtracking, `<` for "replace and continue," `^`/`$` for word-boundary anchors, and
   a 0–9 priority digit (used for ordering/precedence among competing rules) — i.e. a small
   **hand-written rewrite-rule DSL**, structurally similar in spirit to a phonological
   rewrite-rule file (superficially closer to PanGloss's own `pg-rules` rewrite-cascade
   *shape* than to a flat lookup table), authored per-language by a human who knows that
   language's pronunciation-to-spelling correspondences. **I could not find a published
   count of how many rule lines a real-language `_phonet.dat` file contains** (e.g. English's)
   — this is a genuine, flagged gap; the source code shows the rule *grammar*, not a
   real file's size. Absent that number, the honest statement is: it is a nontrivial,
   linguistically-informed authoring task (on the order of the effort of writing a small
   phonological ruleset by hand), not a trivial lookup table, and — critically for
   MINORITY_VERDICT below — the `simple`/`none` fallback modes exist precisely because this
   artifact is optional and commonly skipped for lower-priority languages.
5. **Optionally, `standard.kbd`/`dvorak.kbd`/`split.kbd`-style keyboard files** for
   typo-adjacency weighting (confirmed present in `github.com/GNUAspell/aspell/tree/master/data`,
   measured directory listing) — same "hardcoded QWERTY/AZERTY-shaped grid" critique report
   03 already levels at Hunspell's `KEY` directive (03 §"Keyboard/Keyman") applies here
   nearly verbatim; Aspell ships the identical class of artifact, not a Keyman-derived one.

**Bottom line, synthesis**: only #1 (config) and #2 (word list) are mandatory; #3–#5 are
each independently optional fallback-having features (`affix-compress` off, `soundslike =
simple` or `none`, no keyboard file at all are all documented, graceful degradation paths —
this is Aspell's own designed answer to "what if this language-specific artifact doesn't
exist yet," and it is honest about the degradation, unlike a silent wrong-default). For a
hyper-minority language the realistic minimum viable deployment is **word list + `lang.dat`
only**, accepting `soundslike=none` (near-miss edit distance alone, no phonetic layer) and no
affix compression (flat list) — which is exactly a bare non-word checker, with none of
Aspell's headline "superior suggestions" advantage engaged.

## PERSONALIZATION

**measured.** Two related, from-source-confirmed mechanisms, both simple flat files, no
learning/adaptation model beyond accumulation:

- **Personal word list** (`.aspell.<lang>.pws`) — header line `personal_ws-1.1 lang num
  [encoding]` (num is a word-count *hint*, not required to be accurate) followed by one word
  per line (`Format-of-the-Personal-and-Replacement-Dictionaries.html`, measured). Aspell's
  multi-process handling is explicitly documented as "intelligent" about concurrent personal
  dictionary writes from multiple simultaneously-running Aspell processes (`aspell.net`
  front-page summary, measured) — i.e. the concurrency-safety concern is acknowledged and
  handled, but the data model itself is a flat append-only word list, not a confusion model.
- **Personal replacement dictionary** (`.aspell.<lang>.prepl`) — header `personal_repl-1.1
  lang num [encoding]` (num unused, "should always be 0"), then `misspelled_word correction`
  pairs, one per line (same page, measured) — this is the per-user analogue of the
  system-wide static replacement table (ERRORMODEL step 3): Aspell can accumulate a user's
  own recurring typo→correction pairs, but this is direct pair storage, not a learned
  weighted confusion matrix. Aspell's own manual phrase for this capability, quoted from the
  front page: it "can learn from user's misspellings" — a fair characterization of "append
  known corrections to a personal list," not gradient/statistical learning.

Direct comparison to report 06: 06 already flags Aspell's personal-word-list mechanism by
title/existence only (§"Personalization & privacy-preserving aggregation", citing
`Creating-an-Individual-Word-List.html` as unfetched at the time, "aspell.net was
unreachable by direct fetch in report 03 too"). **This report supersedes that flag**: the
page (and its sibling replacement-dictionary format page) were fetched successfully here via
the `r.jina.ai` proxy workaround; the exact two file formats above are the answer 06 left
open. Compared to the personal-overlay design `00-synthesis.md` §A sketches for PanGloss
(personal wordlist + personal *confusion model* + personal cache/adaptation LM as three
distinct sub-models composed with `λ`-interpolation), Aspell only ever implements the first
of the three, and implements it as a flat file rather than a revisioned overlay structure —
there is no per-user weighted confusion model, no adaptation LM, nothing resembling
`pg-parse::SuppliedRootOverlay`'s revision tracking (01 §5.4).

## INTEGRATION

**measured, with an important currency correction.** Aspell ships (a) a standalone CLI
(`aspell`), (b) a C library other programs link against directly ("an actual library that
other programs can link to instead of having to use it through a pipe" — `Introduction.html`,
measured, framed explicitly as an improvement over Ispell's pipe-only interface), and (c) an
Ispell-compatible pipe/protocol mode for drop-in replacement of Ispell-speaking clients. Its
own front page states Emacs prefers Aspell over Hunspell over Ispell when no spell checker
is manually configured (search-summary-level confirmation, not independently primary-source
verified beyond the search snippet — flagged). **Current-state correction to the task
prompt's framing** ("older LibreOffice, email clients"): search results converge that
**modern LibreOffice, OpenOffice, Firefox, and Thunderbird now default to Hunspell**, not
Aspell — Aspell integration with LibreOffice today is documented as requiring extra
configuration/plugin work rather than being a first-class default (per a NixOS/nixpkgs issue
report, secondary evidence, not a primary Aspell or LibreOffice statement). This matches the
prompt's own "older" qualifier: Aspell was the OpenOffice.org-era default before Hunspell
supplanted it; treat "Aspell integrates with LibreOffice" as **historically true, not
currently the default path**. The **Enchant** abstraction layer (not independently
fetched/verified in this pass — flagged, mentioned only via search snippets) is the more
durable current integration point: it lets a single API target Aspell, Hunspell, or other
backends interchangeably, meaning most "Aspell integration" in modern software is actually
"Enchant integration with Aspell as one pluggable backend," not direct linking.

## LICENSE

**measured, primary source `aspell.net/man-html/Copying.html` (via proxy).** The Aspell
*library* is **GNU Lesser General Public License (LGPL), version 2.1 or (at the licensee's
option) any later version** — confirms the prompt's expectation exactly. The manual/
documentation text itself is separately licensed under the **GNU Free Documentation License,
version 1.1 or later** (a different, non-code license, worth noting so it isn't conflated
with the library's LGPL terms). The page additionally notes some library components carry "a
weaker license," all stated to remain LGPL-compatible — I could not identify, in this pass,
exactly which components or what "weaker" means precisely (flagged as a minor
not-fully-verified detail; does not change the headline LGPL-2.1+ answer). LGPL is
copyleft-at-the-library-level but permits linking from non-GPL/proprietary programs (unlike
GPL) provided the library itself remains replaceable/relinkable — relevant to PanGloss
insofar as *wrapping* Aspell (rather than porting its algorithm) would be LGPL-compatible
with an MIT-licensed host (`rust/Cargo.toml:29`, per 01 §2.2's citation of PanGloss's own
license) in the same way `divvunspell`'s Apache-2.0/MIT dual license already is (01 §3.2) —
though per the repo's stated build philosophy (00-synthesis.md, "Build philosophy" section:
wrap only an "established, easily-usable C library," otherwise port), Aspell's tightly
coupled C++ architecture (mmap'd machine-dependent binary dictionary format, per LEXICON)
makes it a weaker "trivially wrappable" candidate than a header-only or small well-isolated
library would be.

## FOOTPRINT

**measured (structural facts) + explicit gap (no numeric memory/latency figures found).**
The compiled dictionary is designed for a small runtime footprint *shared across processes*:
32-bit byte offsets instead of pointers specifically so the compiled file can be `mmap`ed
read-only and shared between concurrently running Aspell processes rather than each process
holding its own private copy (`Part-1-Compiled-Dictionary-Format.html`, primary, measured).
Dictionary "preferred size" is a configurable two-digit code from `10` (tiny) to `90`
(insane), default `60` (med-large) (`The-Options.html`/search-corroborated, measured) — i.e.
Aspell dictionaries are explicitly built at a chosen size/quality tradeoff point, not a fixed
one-size artifact. **I could not find a single documented, measured resident-memory (RSS) or
on-disk-size figure in MB for any real compiled dictionary** (e.g. `en` at size 60) in this
pass — this is a genuine gap, flagged rather than estimated. **WASM feasibility, synthesis
only, no direct evidence found**: no existing Aspell-to-WebAssembly port was found by direct
search (contrast: Hunspell has *two* independent, actively maintained WASM ports —
`hunspell-wasm` and `hunspell-asm`/`kwonoj/hunspell-asm` — and SymSpell has a Rust-native WASM
port, `spellchecker-wasm`; Aspell has **zero** comparable community WASM effort found).
Aspell's `mmap`-based, machine/endian-dependent compiled dictionary format is a structural
complication for a WASM target specifically because browser WASM sandboxes have no real
`mmap`-backed shared-file-across-processes primitive the way native Aspell exploits — a
port would need to either emulate that layer or fall back to loading the whole dictionary
into linear memory per instance, forfeiting the multi-process memory-sharing benefit
entirely. This is a reasoned inference from the documented design, not a benchmarked
finding — flag it as synthesis, not measured.

## RUST_C

**measured.** Aspell itself is C/C++ (confirmed via its own GitHub source tree:
`lib/`, `lib5/`, `modules/speller/default/*.cpp`, autotools build). No native Rust
reimplementation or `bindgen`-style direct FFI binding to `libaspell` was found. The one
Rust crate found that names Aspell (`ispell`, on crates.io/`lib.rs`) works by **shelling out
to the `aspell`/`ispell`/`hunspell` command-line executable and parsing its pipe protocol**,
not by linking `libaspell` directly — i.e. it is a process-spawning wrapper, not an
in-process binding, and would be unusable inside a WASM/bounded deployment target the way
PanGloss's own FOOTPRINT requirements need. By contrast, Hunspell has a direct Rust FFI
binding (`hunspell-rs`) and a from-scratch pure-Rust reimplementation compatible with its
dictionary format (`zspell`) — Aspell has neither. Per the repo's stated build philosophy
(00-synthesis.md: port rather than settle for a weaker off-the-shelf tool when no
established easily-usable library exists), Aspell's Rust-ecosystem story is the weakest of
the systems likely to appear in this comparison table: not established-C-to-wrap (no clean
FFI binding exists to wrap) and not exists-in-Rust (no port exists) — it would require
either accepting the process-spawn `ispell`-crate approach (incompatible with WASM) or
writing a from-scratch `libaspell` FFI binding as a prerequisite to using it at all.

## MINORITY_VERDICT

**synthesis, grounded in all fields above.** Aspell is a poor architectural fit for a
hyper-minority language target, for reasons that are structural, not merely
resourcing-related:

1. **The lexicon model is exactly backwards for PanGloss's actual problem.** Per LEXICON/
   MORPHOLOGY, Aspell needs a complete enumerated wordform list before it can do anything —
   `munch-list` compresses an existing list, it never generates new legal forms. For a
   language whose entire reason for having PanGloss is that "a wordlist doesn't exist" and
   morphology must generate the lexicon (00-synthesis.md's framing of the original plan's
   core flaw), Aspell offers no leverage at all on the actual bottleneck; it can only be
   *fed* whatever wordlist some other process (ideally PanGloss's own generative grammar)
   produces. This mirrors, almost exactly, 01 §4.1's finding about the plan's Phase 1
   SymSpell-style delete-table: "acceptance = dictionary membership," not "acceptance = a
   parse."
2. **The phonetic table is explicitly English-pronunciation-shaped, and this is not a
   trivial swap.** Per ERRORMODEL/DATA_REQ, the "soundslike" mechanism *is* built on a
   Metaphone-family algorithm — Metaphone was designed, per its own inventor's stated goal
   and per report 02 §1, specifically for English consonant/vowel pronunciation patterns
   (silent letters, `ph→f`, `gh` sequences, digraph handling tuned to English orthographic
   depth). Aspell's manual is candid that soundslike is "a rough approximation," and that
   this specific mismatch is exactly why Aspell ships `soundslike=simple` and
   `soundslike=none` fallbacks — i.e. the software's own designers anticipated that a
   full Metaphone-quality phonetic table is a per-language authoring burden most languages
   won't get, and built graceful degradation for that case. **What breaks concretely for a
   hyper-minority language**: (a) there is no off-the-shelf Metaphone variant for most
   target languages — someone has to author a `_phonet.dat` rule table by hand, in the
   bespoke rewrite-DSL `phonet.cpp` implements, requiring real phonological/orthographic
   expertise in that specific language; (b) per SIL's own orthography-design literature
   (already cited in 02 §4 — Simons' "Principles of Multidialectal Orthography Design"),
   newly designed orthographies for previously unwritten languages are deliberately shallow/
   phonemic, meaning the graphemic surface form is usually already close to the
   phonological one — the corollary is that Aspell's phonetic layer, designed to bridge a
   *deep*-orthography gap (English-style), is solving a problem many of PanGloss's target
   orthographies don't structurally have, while offering no help with the problem they *do*
   have (morphophonological alternation producing whole different suffix strings, per 01
   §1.4); (c) crucially, Metaphone-style consonant-mapping rules assume a segment inventory
   and phonotactic shape broadly like English's — a language with contrastive tone,
   click consonants, ejectives/implosives, vowel harmony, or a segment inventory Metaphone's
   rule categories don't anticipate has no natural slot in that rule DSL at all; the author
   would be reverse-engineering Metaphone's category assumptions onto a phonology they were
   never designed to model, rather than authoring from a clean feature system the way report
   02's `unif_closure`/`feature_lanes` proposal does.
3. **No morphology-aware candidate generation at any layer** — per MORPHOLOGY/SEMANTICS_POS,
   there is no way to tell Aspell "this exact string is an unattested-but-grammatical
   inflected form," which is the single most important fact PanGloss's own propose→confirm
   pipeline can supply that Aspell structurally cannot use even if it were handed to it (no
   field, no hook, no concept of a grammatical-but-unseen wordform).
4. **No WASM precedent and a `mmap`-shaped runtime design work against a bounded deployment
   target** (FOOTPRINT) — every comparable system this comparison will likely also profile
   (Hunspell, `divvunspell`/HFST) either has an active WASM port or is architecturally
   composed of a Rust-native weighted-transducer library designed for embedding; Aspell has
   neither, and its C/C++ Rust-ecosystem story (RUST_C) is the weakest of the group.

**Where Aspell would still be a legitimate reference, not a deployment target**: its
weighted-average scoring formula (ERRORMODEL) and its honest, explicit fallback ladder for
missing per-language artifacts (`soundslike=none`/`simple`, no affix file, no keyboard file
— DATA_REQ) are a clean worked example of "graceful degradation when a language-specific
resource doesn't exist yet," a design stance PanGloss's own per-grammar go/no-bar work
(per user memory: dead-end-census, per-grammar strategy selection) already independently
converges on.

## HEADLINE

**synthesis, for the comparison table.**

**Strengths** (vs. grammar-derived phonological distance, report 02):
1. **Mature, battle-tested, genuinely fast weighted-average scoring** over two independently
   useful signals (edit distance + phonetic proximity) — decades of real-world tuning behind
   a simple, well-understood formula; not novel, but not fragile either.
2. **Explicit, designed-in graceful degradation** when a per-language artifact (phonetic
   table, affix file, keyboard file) doesn't exist yet — `none`/`simple` fallback modes are a
   named, documented feature, not a silent worse-default; a genuinely useful precedent for
   PanGloss's own per-grammar resource-availability story.
3. **LGPL-2.1+, decades-stable, minimal external dependencies** — a legally and
   operationally low-risk reference implementation to study, even where it won't be adopted.

**Weaknesses** (vs. grammar-derived phonological distance, report 02):
1. **The phonetic layer is a hand-authored, English-pronunciation-shaped hash/rule table**
   (Metaphone-family), exactly the kind of "lossy, untunable, English-specific" artifact
   report 02 argues PanGloss should not build (02 §"Consequences for PanGloss" point 1) —
   Aspell is a direct real-world instance of the pattern report 02 is warning against, not a
   counter-example to it.
2. **The lexicon model requires a complete enumerated wordlist and cannot generate legal
   unattested forms** — structurally incompatible with PanGloss's founding premise that the
   lexicon *is* a grammar (00-synthesis.md), and offers zero leverage on the actual
   data-scarcity problem for a hyper-minority language.
3. **No morphological awareness, no context/n-grams, no POS/semantic data, no WASM
   precedent, no clean Rust binding** — every one of these is either a hard "no" (by design,
   confirming the prompt's own expectations) or an unaddressed gap (WASM, Rust), leaving
   Aspell as a useful historical/architectural reference point but not a candidate
   foundation to build or wrap for PanGloss's actual deployment target.

---

## Sources consulted (primary unless noted)

- `aspell.net/` front page, `aspell.net/metaphone/`, `aspell.net/man-html/{Introduction,
  Aspell-Suggestion-Strategy,Customizing-Aspell,Implementation-Notes,Adding-Support-For-Other-Languages,
  Copying,Working-With-Dictionaries,Creating-an-Individual-Word-List,
  Format-of-the-Personal-and-Replacement-Dictionaries,Working-With-Affix-Info-in-Word-Lists,
  The-Language-Data-File,The-Options}.html` — all fetched via `r.jina.ai` proxy due to the
  TLS-certificate mismatch on direct fetch (flagged above); treat content as measured
  (proxy preserves page text) but page-existence/naming as version-sensitive.
- `aspell.net/0.50-doc/man-html/8_How.html` (older manual, "How Aspell Works" chapter) —
  fetched via proxy, used specifically to double-check the context/n-gram question.
- `aspell.net/dev-html/Part-1-...Compiled-Dictionary-Format.html` (Aspell Developer's
  Manual) — fetched via proxy.
- `github.com/GNUAspell/aspell` source tree: top-level layout, `data/` directory listing,
  `modules/speller/default/` file listing, and `phonet.cpp` content — read directly.
- crates.io/lib.rs search results for Rust bindings (`ispell`, `hunspell-rs`, `zspell`); web
  search for Aspell WASM ports (none found) and for LibreOffice/Emacs integration currency.

## Unfetchable / unverified — explicit list

- Every direct HTTPS fetch to `aspell.net` (TLS cert mismatch — environment-level, not a
  content problem; worked around via proxy, see above).
- `web.archive.org` — blocked outright by the fetch tool in this environment.
- `Notes-on-the-Algorithms-Used.html`, `Notes-on-8-bit-Characters.html`,
  `Format-of-the-Main-Word-List-File.html`, `Notes-on-Dictionaries.html` — 404 on the current
  (0.60.8.2) manual; likely renamed/restructured since the 0.50-era manual (whose equivalent
  chapters were partially recovered instead, e.g. `8_How.html`).
- Exact rule count / file size of any real language's `_phonet.dat` file (e.g. English's) —
  only the rule *syntax* was confirmed from `phonet.cpp`, not a real file's size.
- Numeric RSS/on-disk-size figures for any compiled dictionary at any of the documented
  `10`–`90` size codes.
- The precise identity of the "weaker license" components mentioned in passing on the
  Copying page.
- Independent primary confirmation of the Emacs-prefers-Aspell and
  Enchant-as-current-integration-layer claims (search-snippet level only).
