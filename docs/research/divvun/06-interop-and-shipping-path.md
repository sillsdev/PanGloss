# 06 — Interop and shipping path: could a PanGloss grammar ship as a Divvun language?

Agent 6 of 6. Scope: the concrete engineering path (and blockers) for shipping a PanGloss-generated
FST through the Divvun/GiellaLT toolchain. No code changed. Sources: local repo reads in
`C:/Users/johnm/Documents/repos/PanGloss/.claude/worktrees/divvun-research/`, shallow clones under
`C:/Users/johnm/AppData/Local/Temp/claude/C--Users-johnm-Documents-repos-LCAtom/1b5e24e2-aeac-4668-b883-e199cfb811d9/scratchpad/divvun/a6/{hfst,divvunspell,pahkat,giella-core,lang-sme,giellalt-site,divvun-manager-macos}`,
plus `C:/Users/johnm/Documents/repos/foma-rs` (already local, not cloned). Claims are marked
VERIFIED (read directly), INFERRED (reasoned from adjacent evidence), or **unknown**.

## Executive summary

Two findings dominate everything else in this report:

1. **The single biggest blocker is architectural, not format-level.** GiellaLT/Divvun ships the
   compiled FST *as the entire analyzer* — no runtime verifier sits behind it. PanGloss's foma net
   is deliberately over-generating by design (`docs/fst-plan/foma-fst-plan.md:34-40`: "propose+prune
   is the permanent shape," "FST-only (no-verify) operation is off the table") and is only correct
   in combination with the live Rust HermitCrab `confirm` step. Handing Divvun "just the FST" would
   ship a transducer that accepts strings the real grammar rejects — the opposite of what Divvun's
   own quality bar assumes. See §2 and the blocker list.
2. **Formats are not the blocker they might appear to be — text-level lexc is the real seam, not
   binary transducer bytes.** foma's own binary format, HFST's `FOMA_TYPE` backend, and HFST's
   own `hfst-lexc` compiler all exist specifically to make this interoperable (§1, §4). The tag
   *convention* (§3) and the all-or-nothing GiellaLT build/test pipeline (§4) are bigger practical
   obstacles than any byte format.

Bottom line: **source-level interop (emit lexc/twolc-shaped source, let GiellaLT's real toolchain
compile and test it) is the only seam anyone has actually built support for.** Artifact-level
interop (hand over a compiled binary FST and have it "just work" inside a `lang-XXX` build) has no
supported path anywhere in the tooling we inspected.

---

## 1. Binary format reality check

### 1.1 foma's native binary format (VERIFIED)

foma's "binary" file is not a black box and not AT&T format — it is a **gzip-compressed, line-oriented
plaintext dump** with a fixed section grammar. Read directly from `foma-rs`'s port of foma's own
`io.c`:

- Header line literal `##foma-net 1.0##` (`C:/Users/johnm/Documents/repos/foma-rs/crates/foma/src/io.rs:938`).
- `##props##` section: one line of 13 whitespace-separated fields — arity, arccount, statecount,
  linecount, finalcount, pathcount, is_deterministic/is_pruned/is_minimized/is_epsilon_free/
  is_loop_free (as `Tern`), an `extras` bitfield (`is_completed` 2 bits + `arcs_sorted_in` 2 bits +
  `arcs_sorted_out` 2 bits), and the network name (`io.rs:947-995`, `Props::from_extras`/`to_extras`
  at `io.rs:1206-1229`).
- `##sigma##` section: one `"<number> <symbol>"` line per alphabet symbol (`io.rs:1014-1054`,
  writer at `io.rs:1266-1271`).
- `##states##` section: rows of 2, 3, 4, or 5 whitespace-separated ints depending on which fields
  repeat from the previous row (`in`/`out`/`target`/`final`, with `state_no` only present on 4- and
  5-field rows) — `io.rs:1056-1129`, `explode_line` at `io.rs:886-909`.
- Optional `##cmatrix##` (confusion matrix, for spelling-correction weight lookup) then `##end##`
  (`io.rs:1131-1163`).
- The whole thing is wrapped in gzip by the *writer* (`fsm_write_binary_file`/`fsm_write_binary`,
  `io.rs:667-691`, via `flate2::write::GzEncoder`) — this is a convention, not a magic-number
  requirement: `fsm_read_binary_mem` sniffs the `1f 8b` gzip magic and falls back to reading the
  bytes raw if absent (`io.rs:755-772`). A **separate** reader, `read_att` (`io.rs:347-409`), handles
  the unrelated tab-separated AT&T text format (`hfst-fst2fst`'s other common interchange format) —
  foma's native binary and AT&T text are two different things; don't conflate them.
- `foma-rs`'s in-memory API (`fsm_read_binary_mem_prefix`, `io.rs:779-804`) additionally supports
  reading one gzip member off the front of a multi-image stream and reporting bytes consumed — built,
  per its own comment, specifically to cope with "HFST's per-transducer `[header][gzip-image]`
  framing," i.e. `foma-rs`'s author was already thinking about HFST interop.

**`divvun/foma-rs`'s own quirk relevant here (VERIFIED, self-contained bug, not a format
incompatibility):** `foma::lexcread::lexc_add_mc` (the `Multichar_Symbols` *declaration* path) and
`lexc_string_to_tokens` (the entry-text *tokenizer*) disagree about how an escaped literal `0` digit
inside a multichar symbol name decodes, so a `Multichar_Symbols` name containing a literal `0`
silently decomposes into single-character arcs in the compiled network
(`PanGloss/rust/crates/pg-foma/src/tags.rs:21-67`, module doc point 3 — filed upstream as
`divvun/foma-rs#2`, per `tags.rs:276-277`; that same module doc states **"the original C foma reader
does not have it"**). This is purely a `foma-rs` implementation defect in the Rust port, not a
property of the foma binary format itself — a network compiled by real upstream C foma or by
`hfst-lexc` targeting the `foma` output format would not need PanGloss's `ZERO_GLYPH` workaround at
all. It is a portability *non-issue* in the outbound direction (our workaround produces valid,
if cosmetically odd, symbol names any foma-family tool can read); it would matter only if PanGloss
ever needed to read binaries built by other tools that assume the bug's absence — it doesn't.

### 1.2 HFST's `FOMA_TYPE` backend — the pivotal fact (VERIFIED)

HFST genuinely links foma's own C library as one of its pluggable `ImplementationType` backends —
this is not merely a converter, and not a shim:

- `ImplementationType` enum: `SFST_TYPE, TROPICAL_OPENFST_TYPE, LOG_OPENFST_TYPE, FOMA_TYPE, XFSM_TYPE,
  HFST_OL_TYPE, HFST_OLW_TYPE, HFST2_TYPE, UNSPECIFIED_TYPE, ERROR_TYPE`
  (`hfst/libhfst/src/HfstDataTypes.h:46-62`).
- `FomaTransducer` (`hfst/libhfst/src/implementations/FomaTransducer.{h,cc}`) wraps foma's own opaque
  `struct fsm` (`FomaTransducer.h:19`) and calls straight through to foma's C API — `fsm_compose`,
  `fsm_cross_product`, `fsm_merge_sigma`, `fsm_construct_*`, `fsm_determinize`, `fsm_minimize`, etc.
  (`FomaTransducer.cc:233-296`).
- Linkage is **external, not vendored**: `PKG_CHECK_MODULES(FOMA, [libfoma >= 0.10.0])`
  (`hfst/configure.ac:711`), and this is **entirely conditional on `--with-foma`** (default yes) —
  `configure.ac:102-108`. At the C++ level everything is `#if HAVE_FOMA` (`FomaTransducer.cc:16`,
  `HfstTransducer.cc:5377-5412`); a downstream package built `--without-foma` throws
  `ImplementationTypeNotAvailableException` for any foma-format file.
- **Gzip is NOT auto-decompressed.** `HfstInputStream::guess_fst_type` detects the gzip magic
  (`0x1f 0x8b 0x08`) and *throws* `FileIsInGZFormatException` rather than piping through zlib
  (`hfst/libhfst/src/HfstInputStream.cc:589-604`); `hfst-fst2fst` catches this and tells the user to
  gunzip first (`hfst/tools/src/hfst-fst2fst.cc:312-318`). Since real foma tooling — and `foma-rs`
  (`io.rs:667-691` above) — gzip-wraps by convention, **the caller must gunzip before handing a
  `.fsm` to HFST.**
- HFST's own reader hand-parses the exact same `##foma-net 1.0##`/`##props##`/`##sigma##`/
  `##states##`/`##end##` grammar described in §1.1 (`FomaTransducer.cc:886` onward), confirming the
  format is genuinely shared, not merely similar.
- **Weight loss is real but symmetric-with-none-to-lose**: foma's `struct fsm` has no weight field at
  all, so foma→HFST conversion hard-codes weight `0` on every transition/final state
  (`ConvertFomaTransducer.cc:178-190`), and HFST→foma conversion **silently drops any real HFST
  weights** — it copies finality but never reads the weight map (`ConvertFomaTransducer.cc:296-305`).
  This only matters if a weighted HFST transducer is pushed down to `FOMA_TYPE`; going the other way
  (unweighted foma → HFST) loses nothing that existed.
- **A direction/orientation gotcha, independent of format correctness**: `hfst-fst2fst` warns that
  converting a native foma transducer may need inversion for `hfst-lookup` to behave as expected —
  "hfst-flookup works as foma's flookup" (`hfst-fst2fst.cc:210-222`). Worth testing explicitly in any
  pilot, not assuming upper/lower tape orientation carries over for free.
- `hfst-lexc` (HFST's own lexc compiler, `hfst/libhfst/src/parsers/lexc-parser.yy`,
  `hfst/tools/src/hfst-lexc-compiler.cc`) reads standard/Xerox-style lexc and can itself *emit*
  `FOMA_TYPE` output (`hfst-lexc --format=foma`, per `man/hfst-lexc.1:94-95`), with HFST-only
  additive extensions layered on top: an embedded `# "weight: N"` gloss-field convention for weighted
  targets, `--xerox-composition`, flag-diacritic hyperminimization flags (`--withFlags`,
  `--minimizeFlags`, `--renameFlags`), and extra `-W*` warnings. None of PanGloss's emitted lexc uses
  any of these HFST-only extensions (§2), so this is a non-issue for us either way.

### 1.3 divvunspell's own binary readers (VERIFIED — and this is where "no foma path" becomes concrete)

`divvunspell` — the actual runtime that end-user spellers/analyzers embed — does **not** link
libhfst and does **not** speak foma's format at all:

- A repo-wide `grep -ri foma` across the divvunspell checkout returns **zero matches** anywhere
  (source, `Cargo.toml`, README, tests) — a clean, verified negative result.
- Instead, `divvunspell` has its own **from-scratch reimplementation** of the HFST optimized-lookup
  binary layout: `TransducerHeader::parse` (`divvunspell/src/transducer/hfst/header.rs:28-65`) reads
  a 5-byte skip (assumed `"HFST\0"` — **the magic bytes are never actually checked**, `header.rs:32`),
  a `u16` header length, then little-endian `u16 input_symbols`, `u16 symbols`, `u32
  trans_index_table`, `u32 trans_target_table`, `u32 states`, `u32 transitions`, then 9 `u32` boolean
  property flags (`header.rs:41-53`). The alphabet section is a run of NUL-terminated UTF-8 strings
  (`divvunspell/src/transducer/hfst/alphabet.rs:113-153`), followed by fixed-size index-table records
  (6 bytes) and transition-table records (12 bytes) (`divvunspell/src/constants.rs:3-4`), with a
  `0x8000_0000` sentinel separating index-table addressing from transition-table addressing.
- A second, simpler format, **THFST**, is also read (`divvunspell/src/transducer/thfst/mod.rs`): no
  magic/header at all, a JSON-serialized alphabet plus two flat `#[repr(C)]` 8-byte record arrays
  (`IndexTableRecord`/`TransitionTableRecord`, `thfst/mod.rs:37-56`). This is the format `.bhfst` box
  containers use, and it is deliberately the easier target for a from-scratch producer.
- `.zhfst` (VERIFIED, `divvunspell/src/archive/zip.rs`) is a plain zip containing an `index.xml`
  (parsed by `serde-xml-rs` into a `SpellerMetadata` struct — `divvunspell/src/archive/meta.rs:8-192`,
  schema `<hfstspeller><info>…</info><acceptor type="..." id="...">…</acceptor><errmodel
  id="...">…</errmodel></hfstspeller>`) plus two members whose *names* are read out of the XML's
  `acceptor`/`errmodel` `id` fields (conventionally `acceptor.default.hfst`/`errmodel.default.hfst`,
  `zip.rs:97-130`). Both extracted members are parsed **hardcoded** as the HFST format above
  (`zip.rs:133-146`, type alias `HfstZipSpeller = HfstSpeller<HfstTransducer, HfstTransducer>`) — no
  generic transducer-type dispatch inside the `.zhfst` path.
- `.bhfst` (a `box-format` archive, `divvunspell/src/archive/boxf.rs`) *is* generic over the
  `Transducer`/`TransducerLoader` traits (`divvunspell/src/transducer/mod.rs:108-146`), whose own doc
  comment explicitly invites third-party implementations ("Implementors can provide custom
  transducer formats beyond the built-in HFST and THFST formats"). This is the one place in the
  entire pipeline where "bring your own binary format" is an actually-supported extension point —
  but it still requires writing Rust code that implements those traits; it is not a drop-in
  byte-format acceptance.
- divvunspell's speller is a genuine **two-transducer architecture**: a lexicon/acceptor transducer
  plus a separate error-model/mutator transducer, walked in lockstep with a symbol-numbering
  translator reconciling their independent alphabets (`divvunspell/src/speller/mod.rs:1-9,869-951`,
  `divvunspell/src/speller/worker.rs`). Plain acceptance/analysis bypasses the mutator entirely
  (`speller/mod.rs:729,767`).
- License: library code (the `divvun-fst`/`divvunspell` crates) is dual MIT/Apache-2.0; the CLI
  tools are GPL-3.0 (`divvunspell/README.md:309-318`, `Cargo.toml:7`).
- `thfst-tools` (a sibling crate) converts *existing* HFST-format files to THFST/BHFST
  (`hfst-to-thfst`, `zhfst-to-bhfst`) — divvunspell/thfst-tools has **no code anywhere that
  authors a `.zhfst` zip or an `index.xml` from raw components**; building the canonical `.zhfst` in
  the first place happens upstream in the GiellaLT build (§4), outside this checkout.

### 1.4 Format-conversion graph

```
foma-rs .fsm (gzip, "##foma-net 1.0##" text dump)
   │  [gunzip — required, HFST does not auto-decompress]
   ▼
plain "##foma-net 1.0##" text
   │  [HFST FomaTransducer::read_net — REQUIRES HFST built --with-foma / HAVE_FOMA]
   ▼
HfstBasicTransducer (generic HFST in-memory form)
   │  lossless for structure/alphabet/flag-diacritics; all weights become 0 (none existed)
   ├──► hfst-fst2fst -f openfst-tropical / openfst-log / sfst / xfsm     (weighted targets: weight=0 everywhere)
   ├──► hfst-fst2fst -f optimized-lookup-unweighted  ──►  .hfstol  ──►  READABLE by divvunspell's
   │                                                                    own from-scratch HFST-format
   │                                                                    parser (header.rs/alphabet.rs)
   └──► hfst-fst2fst -f foma  (round-trip back, lossless minus weights — none to lose)

HFST (any weighted backend) ──► FOMA_TYPE:  weights SILENTLY DROPPED (ConvertFomaTransducer.cc:296-305)

foma-rs .fsm  ──✗ NO PATH ──►  divvunspell directly (zero foma-awareness in divvunspell, verified)

.hfstol / HFST format  ──► thfst-tools hfst-to-thfst ──►  THFST  ──►  divvunspell (native, simpler)
THFST / arbitrary format ──► implement Transducer+TransducerLoader traits ──► .bhfst (supported extension point)

lexc TEXT SOURCE (ours, standard-shaped) ──► hfst-lexc --format={foma|hfst-ol|...}  (no binary round-trip needed at all)
```

**The practical implication**: the shortest, least-lossy real path is **not** "compile with
`foma-rs`, then convert the binary" — it's **"emit the same lexc/twolc-shaped text source we already
produce, and let `hfst-lexc`/`hfst-twolc`/`hfst-xfst` compile it directly to whatever HFST backend
Divvun's own toolchain wants,"** sidestepping the whole binary-conversion chain (and the `HAVE_FOMA`
conditional) entirely. This reframes the interop question from "can foma-rs's bytes survive
conversion" to "is our emitted lexc text actually standard lexc" — answered in §2.

---

## 2. What we emit — is it portable lexc?

Read `PanGloss/rust/crates/pg-foma/src/emit.rs` (5,646 lines; module doc `emit.rs:1-120`),
`pg-foma/src/tags.rs` (400 lines, full read), and `pg-foma/src/replace.rs` header (2,846 lines,
prototype only).

**Structurally, yes — standard lexc shape.** The emitter writes conventional `Multichar_Symbols`
(`emit.rs:3303,4389`), `LEXICON <Name>` sections (`emit.rs:1171,3460,4488`), and `END;`-style
lexicon chaining — this is the same skeleton `hfst-lexc`/upstream C foma/Xerox lexc all expect
(confirmed independently by agent 3's reading of `lang-sme/src/fst/morphology/root.lexc`, which has
the identical `Multichar_Symbols` / `LEXICON Root` shape).

**Semantically, the upper (analysis) tape is NOT a linguistic tag string at all — it is internal
bookkeeping.** This is the most important finding in this section:

- Every lexc entry's upper side is a multichar symbol `<R:nnnn>` (root morpheme) or `<M:nnnn>`
  (non-root morpheme), where `nnnn` is the raw `MorphemeId` index — `tags.rs:1-136`,
  `emit.rs`'s "Tag tape convention" section: *"Every lexc entry's UPPER side is the morpheme's tag
  SYMBOL ONLY (never literal underlying text)."* There is no POS letter, no feature name, no `+Sg`/
  `+Nom`-style content anywhere on the tape — decoding one of these tags (`tags::decode_path`) yields
  only `(is_root: bool, MorphemeId)` pairs, not linguistic categories.
  This encoding exists purely to support PanGloss's own propose→confirm handshake (`decode_path` →
  `Candidate` → re-verified by the live HermitCrab engine, `docs/fst-plan/foma-fst-plan.md:60-68`'s
  architecture diagram) — it was never designed to be a human- or CG-legible analysis string.
- A `ZERO_GLYPH` (`'z'`) substitutes for every literal ASCII `0` digit in a tag numeral
  (`tags.rs:58-86`) — a workaround for the `foma-rs`-specific `Multichar_Symbols` decomposition bug
  described in §1.1. This makes the already-opaque tags additionally illegible even as raw numbers
  (`<R:zzz1>` instead of `<R:0001>`), though it is not itself a portability blocker (§1.1).
- The lower (surface) tape, in the **mainline** emit path, is literal orthographic text with
  structural boundary characters (`+`, `^0`-family, `.`) stripped (`emit.rs`'s "Surface spelling"
  section) — this part IS human-legible and portable.
- The **P6 prototype** (`replace.rs`, explicitly "NOT wired into the mainline emit/analyzer path,"
  `replace.rs:1-30`) takes a different, more radical approach for its *rule-compiled* alternative:
  every character-definition is mapped to **one Private-Use-Area Unicode codepoint**, and both lexc
  entries and rule regexes are built in that PUA token space, specifically to route around foma's
  ASCII-reserved xre operators (`+` is foma's Kleene-plus!) and multi-representation segments. Its own
  doc admits the cost: *"the composed network's own lower tape is not human-legible orthography."*
  This prototype is not shipping today, but the plan explicitly frames HC-rewrite-rule-to-foma-regex
  compilation as **mainline follow-on work (P6)**, not a rejected idea
  (`docs/fst-plan/foma-fst-plan.md:34-40`: "compiling the HC rewrite rules into foma's replace
  calculus... is now mainline follow-on work"). **If that path becomes the production emitter, the
  lower tape would stop being real orthography** — a forward-looking blocker for any downstream
  consumer (CG taggers, hyphenators, generation-mode YAML tests, anything expecting literal
  wordforms) that isn't PanGloss's own analyzer. Flagged for whoever picks up P6.
- Today's **mainline** emitted network is lexicon + morphotactics + enumerated junction variants only
  (P1 stage 1/2) — general HC rewrite-rule composition via foma's replace calculus is prototype-only.
  This means the mainline foma net is not, and is not intended to be, a complete standalone analyzer:
  it is explicitly an over-generating proposer that depends on live `confirm` (§4's architecture
  point, restated here because it is a property of *what we emit*, not just how it's used).

**Anything that would not compile under upstream C foma / `hfst-lexc` / Xerox lexc?** Nothing found
in the mainline path that is a genuine incompatibility (as opposed to a stylistic oddity): the
ZERO_GLYPH-mangled tag numerals and Multichar_Symbols runs for NFD-decomposed diacritics
(`emit.rs:1008-1027`, e.g. `"e\u{301}"` declared as its own multichar symbol to avoid an `é` NFD run
compiling as two separate codepoint arcs) are both **standard, legal lexc constructs** — just
generated for reasons specific to our own pipeline, not divergences from the lexc language itself.
No grep for a `F1_QUIRK_AUDIT.md`-style "known divergences" list turned up anything about lexc/xfst
portability specifically — that document (`docs/fst-plan/F1_QUIRK_AUDIT.md`) is explicitly marked
LEGACY and covers the *sunset* `hc-hybrid` custom-FST prototype's fidelity to the C# HermitCrab
engine, not foma/lexc portability; it is not relevant to this question (checked and ruled out,
`F1_QUIRK_AUDIT.md:1-8`).

**Verdict for §2**: the lexc *skeleton* is portable. The tag *content* is not a linguistic tagset at
all today (§3 covers what would be needed) — this is a bigger gap than any syntax incompatibility.

---

## 3. Tag/analysis-string convention mismatch

GiellaLT convention (VERIFIED, agent 3's reading of `lang-sme`): tags are declared in each
language's own `root.lexc` `Multichar_Symbols` block (e.g. `+N`, `+A`, `+V`, `+Pron`, `+Sg`, `+Nom` —
`lang-sme/src/fst/morphology/root.lexc:49-61`), producing analysis strings like `bietna+N+Sg+Nom`
(POS-first, `+`-delimited, ordered by convention). This convention is **not enforced by
`giella-core`** — no shared multichar-symbols file or validator exists there; it is documented per
language family in hand-written prose (`lang-sme/docs/docu-sme-grammartags.md`, and a cross-language
convention note `lang-sme/docs/docu-mini-smi-grammartags.md` meant to help sibling Sámi languages
converge by social convention, not by a checked schema). YAML tests key on this exact string shape
(`gt-desc-yamls/*.yaml`: `bietna+N+Sg+Nom: bietna`), so the tagset is load-bearing for the whole test
suite, not cosmetic.

PanGloss's emitted upper tape (§2) carries none of this: it is `<R:nnnn>`/`<M:nnnn>` morpheme-ID
references, not POS/feature tags, not `+`-delimited, not human-legible, and lemma vs. stem is not
distinguished on the tape at all (the lower tape carries literal surface spelling, stripped of
boundary characters, but no separate lemma citation form). There is also no analogue of GiellaLT's
`@`-flag-diacritic leakage concern in the mainline path, because mainline doesn't emit MPR-gating
flag diacritics onto the tape yet (`replace.rs`'s own "What this module does NOT attempt" list:
"MPR gating (`required_mpr`/`excluded_mpr` on a subrule) — flag-diacritic emission is P6 mainline
work... not attempted in this slice").

**Is a mechanical mapping feasible?** Partially, and only as new work, not as a trivial adapter:

- The underlying HC grammar model DOES carry richer-than-`MorphemeId` information: `pg-grammar`'s
  morpheme model has a `gloss: Option<String>` field (`pg-grammar/src/model.rs:507`), and a whole
  downstream crate, `pg-realize` (`ir.rs`, `map.rs`, `realize.rs`, `signature.rs`, `table.rs`), exists
  to turn analyses into human-facing glosses/realizations. So the *data* needed to build a real
  tagset (per-morpheme category, per-rule feature contribution) is not absent from PanGloss — it
  simply isn't the thing baked onto the foma tape today (INFERRED: `pg-realize`'s existence and
  `gloss` field strongly suggest the mapping data exists somewhere in the pipeline; this report did
  not do a deep read of `pg-realize` to confirm it already produces GiellaLT-shaped strings —
  treat "gloss data exists" as VERIFIED and "it's readily convertible to a GiellaLT tagset" as
  INFERRED, not confirmed).
- A mechanical *encoding* layer (MorphemeId → `+TAG` string, looked up from a table built once per
  grammar) is straightforward engineering **once someone has decided the tagset** — i.e. decided
  tag names (`+Sg` vs `+Sing` vs `+SG`), decided ordering conventions, decided how compounds/
  sub-lexicon-derived tags are named, and decided how much of GiellaLT's existing shared convention
  (if any exists for the target language family) to adopt vs. invent fresh.
- That tagset decision is explicitly a **human/linguist decision** in GiellaLT's own tradition (the
  existence of hand-written `docu-*-grammartags.md` files, and the complete absence of an
  auto-derived or centrally-enforced tag schema in `giella-core`, is direct evidence of this — VERIFIED
  by agent 3, §6 of their findings, and not contradicted anywhere else). A generator can propose a
  mechanical, internally-consistent tagset from PanGloss's own feature-structure model; it cannot
  discover the *conventional* GiellaLT tag names or ordering a human community expects for a
  brand-new language without that community's own linguist making choices — same as any
  from-scratch GiellaLT language today.

**Verdict for §3**: the FST's own tape today has essentially zero overlap with GiellaLT's tag
convention. Building the mapping is credible engineering work (the source data for it exists
elsewhere in PanGloss), gated by a genuinely human, per-language tagset-design step that GiellaLT
itself treats as manual, not automatable.

---

## 4. The GiellaLT repo/build contract

(Full detail from agent 3's research, summarized and cited here.)

- **Autotools, not CMake, not a Python build orchestrator** (VERIFIED): `lang-sme/configure.ac`,
  `lang-sme/Makefile.am`, `autogen.sh`; depends on a sibling `../giella-core` checkout, version-gated
  (`lang-sme/m4/giella-macros.m4:92-114`, minimum giella-core 1.11.0). Flow:
  `./autogen.sh && ./configure && make && make check && make install`.
- **Source layout**: `src/fst/morphology/{root.lexc, stems/*.lexc, affixes/*.lexc, clitics.lexc,
  compounding.lexc}`, `src/fst/morphology/phonology*.twolc`, `src/fst/tagsets/sme.regex`,
  `src/fst/filters/*.regex`, `src/fst/orthography/*.regex`.
- **Genuinely compiles from source — no drop-in-binary seam exists anywhere** (VERIFIED): pattern
  rules in `giella-core/am-shared/lexc-include.am:23-29` literally invoke `hfst-lexc ... -o $@ $<`
  (and a parallel foma-targeted rule via `foma -e "read lexc $<" -e "save stack $@"`);
  `twolc-include.am:34-40` invokes `hfst-twolc`; then `lang-sme/src/fst/Makefile.am` chains dozens of
  further `hfst-xfst`/`foma` compose steps against the filter/orthography regex files (e.g. the
  `analyser-gt-norm` target composing 10 filters plus flag-diacritic handling in one shell-piped
  script, `Makefile.am:331-366`). A repo-wide grep for "prebuilt"/"import"/"drop-in" across both
  `.am`/`.ac`/`.m4` trees returned zero hits. The one "copy" rule that exists
  (`giella-core/am-shared/dot-generated-dir.am:14-18`) only relocates giella-core's *own* just-built
  artifact, not an externally supplied one.
- **Testing**: YAML files (`test/gt-desc-yamls/*.ana.yaml`/`*.gen.yaml`) keyed by
  `lemma+TAGS: surfaceform` (§3's tagset, load-bearing here), consumed by
  `giella-core/scripts/run-yaml-testcases.sh.in` → an external `morph-test`/`morph-test2` tool
  (enforced present via `AC_PATH_PROGS`, `giella-macros.m4:181-187`), wired into `make check` via
  `giella-core/am-shared/devtest-include.am`.
- **`--enable-*` flags** (VERIFIED, `giella-macros.m4` `gt_ENABLE_TARGETS`, ~lines 549-990):
  `--enable-analysers`, `--enable-generators`, `--enable-spellers`, `--enable-grammarchecker`
  (needs `vislcg3`), `--enable-syntax`, `--enable-apertium`, `--enable-tts`, `--enable-fst-hyphenator`,
  `--enable-all-tools`, plus tuning flags (`--enable-hyperminimisation`, `--enable-twostep-intersect`).
- **License**: GPLv3 for both `giella-core` and `lang-sme` (`LICENSE` files, headers citing
  "Copyright © 2000-2025 The University of Tromsø & the Norwegian Sámi Parliament," with an
  alternate-licensing contact — see §6).

**Verdict for §4 (answers the brief's explicit question): source-level interop is the only seam
that exists.** There is no artifact-level path — the build machinery insists on compiling lexc/twolc
itself via `hfst-lexc`/`hfst-twolc`/`hfst-xfst`/`foma` invoked from its own Makefiles, and `make
check` depends on the exact tag convention (§3) baked into those compiled artifacts. Substituting a
foreign FST would mean either reverse-engineering every intermediate Makefile target (fragile,
unsupported, breaks YAML tests that assume GiellaLT tag strings) or doing the only thing the system
actually supports: emitting our own lexc/twolc-shaped source into a real `lang-XXX` skeleton and
letting GiellaLT's toolchain compile and test it end-to-end.

---

## 5. `divvun/registry` + downstream runtimes

(Full detail from agent 4's research.)

- There is **no single structured "registry" file** — the catalog is the `giellalt` GitHub org itself
  plus GitHub *topics* (`maturity-prod`, `langfam-uralic`, `geo-nordic`, etc.), scraped live by the
  docs site's build script (`giellalt-site/fetch_github_repos.rb`) and rendered into tables. A private
  counterpart, `divvun/private-registry`, exists for unlisted languages (unfetchable in this pass —
  **unknown** contents). Distribution proper goes through a separate Pahkat index repo
  (`divvun/pahkat.uit.no-index`, referenced but not fetchable — **unknown** exact structure).
- **Pahkat** (`github.com/divvun/pahkat`, explicitly "alpha software... should not be used in
  production" per its own docs) is the package manager: a `manifest.toml` per language repo declares
  multiple independent packages (`[package.speller]`, `[package.grammar]`, `[package.tts-textproc]`,
  `[package.hyphenator]`), each with SPDX-licensed releases, per-platform installer metadata, and
  code-signing fields.
- **Downstream consumer requirements beyond a bare analyzer FST** (VERIFIED per-product):
  - *Keyboards*: do NOT need the analyzer at all — pure layout definitions built by `kbdgen` from
    YAML; no dependency on the FST/speller found.
  - *divvunspell*: needs the acceptor **and** a separate error-model (mutator) FST reconciled through
    a shared/translatable alphabet, an `index.xml` metadata file, and (for tuning) an optional JSON
    accuracy-config — see §1.3.
  - *Grammar checkers*: need **human-authored Constraint Grammar (`.cg3`) rule files** — a
    disambiguator, semantic-roles grammar, and a language-specific `grammarchecker.cg3` — run through
    `vislcg3`, layered on top of the analyzer's `.hfstol` output (`lang-sme/src/cg3/disambiguator.cg3`,
    `lang-sme/tools/grammarcheckers/*.cg3`). This is entirely separate linguistic work, not derivable
    from the FST.
  - *Hyphenation* is its own independently-versioned FST-based Pahkat package
    (`syllabification/hyphenation.xfscript`), built and toggled separately from both analyzer and
    grammar checker.
  - *Machine translation* (Apertium): the analyzer FST is reused (converted to Apertium's `.att`
    format, `--enable-apertium`), but Apertium transfer rules/bidix are a wholly separate,
    Apertium-side artifact not produced by GiellaLT.
- **The delta between "we have an analyzer FST" and "this is a shippable Divvun language" is large**:
  minimally, an error-model FST + `index.xml` (speller), a hand-written CG disambiguation grammar
  (basic quality-of-life for any consumer beyond raw lookup), a Pahkat `manifest.toml` + signed
  installers per platform, and org-level registry/topic placement. None of these exist yet for a
  PanGloss grammar and none are generatable from the FST alone.

---

## 6. Governance and licensing

(Full detail from agent 4's research.)

- **Per-repo license, not centrally fixed, but GPL-family by strong convention**: `lang-sme`,
  `giella-core`, and (checked directly) `lang-smj` are all GPLv3; the site states resources are
  "available under various open source licenses, mostly GPL or MIT"; `divvunspell`'s own library is
  dual MIT/Apache-2.0 while its CLI tools are GPL-3.0 — license mixing exists even within one Divvun
  repo. A new `lang-XXX` scaffold (`gut template generate`) prompts for `__LICENSE__` at creation time
  — it's a per-repo choice, not enforced by tooling.
- **No CLA / formal copyright-assignment process found** in either clone or via search. What exists
  instead: an `AUTHORS` file convention (a placeholder `__FIXME__` in `lang-sme`'s own copy) plus a
  README line — *"The authors named in the AUTHORS file are available to grant other licencing
  choices"* — and per-file headers on the core linguistic tooling (`.cg3`, `.xfscript`, `.twolc`
  files) explicitly claiming **"Copyright © 2000-2026 UiT The Arctic University of Norway"** with
  *"Other licensing options are available upon request, please contact giellatekno@uit.no or
  feedback@divvun.no."* This is direct evidence that **UiT institutionally holds and administers
  copyright** over the core tooling and mediates alternate licensing — a human/organizational
  channel, not something discoverable or settleable purely from the public repos.
- **Org gating, independent of code license**: creating a new language repo requires GitHub org-admin
  rights and the `gut` CLI ("You need to be at least admin"); reaching "Production" maturity (and
  hence the Divvun Manager front page) requires meeting criteria including "a proper license" and
  working CI/CD, adjudicated by org maintainers. So even a fully open-licensed, technically complete
  PanGloss-generated language repo would need **UiT/Divvun maintainer sign-off** to be listed and
  distributed under the Divvun/GiellaLT name and channel — this is not self-service.
- **FieldWorks/FLEx-derived lexical data — no precedent, no documented policy (unknown, genuine
  gap)**: no GiellaLT/Divvun contribution doc addresses ingesting SIL-licensed lexical data at all.
  The closest adjacent mechanism is a documented `git subtree`-based convention for absorbing *any*
  external FST/lexical source under a mandatory `ext-<source>` directory prefix
  (`giellalt-site/infra/NewLanguageExtSource.md`, used e.g. for Apertium morphology repos) — this
  shows a provenance-*tagging* convention exists, but says nothing about license-compatibility
  vetting, and nothing ties it to SIL/FieldWorks specifically. **This needs a direct human decision**
  (almost certainly routed through the same UiT/Divvun contacts above) before any FLEx-sourced
  lexicon is contributed anywhere in this ecosystem — do not treat this as solved by the `ext-`
  convention alone.

**Everything in this section that is a human decision, not an engineering one**: (a) which license a
new PanGloss-derived language repo uses; (b) whether/how it gets UiT/Divvun org sign-off to live
under the `giellalt`/`divvun` names and distribution channels; (c) whether FLEx/SIL-sourced lexical
data can legally go into a GiellaLT repo at all, and under what terms.

---

## 7. Staged interop plan

### Smallest experiment (proves or kills the idea in days, not weeks)

**Language**: pick the PanGloss reference grammar with the *simplest* phonology already in the repo
(Sena, per `emit.rs`'s module doc — "Stage 1 (Sena, 0 phonological rules)") so the mainline emitter's
enumerated-junction-variant path is exercised with no dependency on the P6 replace-rule prototype.

**Exact steps**:
1. Run PanGloss's existing `pg-foma` emit path for Sena to produce the lexc source it already
   generates today (no new code — this is the existing mainline output, per `emit.rs`'s own doc).
2. Take that **lexc text**, not any compiled `.fsm` binary, and feed it to a real, unmodified
   `hfst-lexc` binary (built against upstream libfoma/HFST, obtained from the `hfst` project, not
   `foma-rs`) with `--format=foma` and separately `--format=optimized-lookup-unweighted`. This tests
   §2's central claim (lexc skeleton is portable) directly, without touching the binary-conversion
   chain in §1.4 at all.
3. Diff: (a) whether `hfst-lexc` accepts the source without error (proves/disproves lexc syntax
   portability, §2); (b) `hfst-summarize`/`hfst-fst2fst -f foma` round-trip the result and compare
   `apply_up` output on a handful of Sena test words against `foma-rs`'s own `apply_up` on the same
   lexc, compiled by `foma-rs` (proves/disproves that the two toolchains agree on what the lexc
   *means*, independent of §1's `ZERO_GLYPH`/decomposition footgun, which `hfst-lexc` should not
   exhibit at all per §1.1).
4. Success criterion: `hfst-lexc` compiles the emitted Sena lexc with zero errors, and `apply_up`
   over a sample word list yields the identical `<R:nnnn>`/`<M:nnnn>` tag sequences (module the
   `ZERO_GLYPH` cosmetic substitution) that `foma-rs` produces on the same source. This is a
   half-day-to-few-day experiment requiring no new PanGloss code — only a build of upstream
   `hfst-lexc` (or `foma`) and a shell script, matching the brief's "do not run any build" constraint
   for *this* research pass (a follow-on implementation task, not part of this report).

### Stage 2 — real GiellaLT tag emission + a toy `lang-XXX` skeleton

Design a genuinely new emitter mode (or a post-processing pass over the existing `MorphemeId`-tagged
lexc) that replaces `<R:nnnn>`/`<M:nnnn>` with real `+POS+Feature` tags, sourced from `pg-grammar`'s
existing `gloss`/feature-structure data (§3). Scaffold a minimal `lang-und`-style repo (per
`giellalt-site/infra/HowToAddANewLanguage.md`'s `gut template generate` flow) with `giella-core` as a
sibling checkout, drop the tagged lexc in as `root.lexc`/`stems/*.lexc`, and run `./autogen.sh &&
./configure --enable-analysers && make && make check` for real, including hand-writing a handful of
YAML test cases in GiellaLT's own `lemma+TAGS: surface` format (§4). This is the point at which §2's
"is our lexc *actually* standard" claim gets tested against the real build/test harness, not just a
bare `hfst-lexc` invocation.

### Stage 3 — confront the propose/confirm architecture mismatch head-on

Decide, deliberately, how to resolve §2's biggest finding (PanGloss's foma net is a deliberately
over-generating proposer, not a standalone analyzer) before attempting to ship anything under the
Divvun name for real: either (a) restrict to a closed, enumerable vocabulary and compile the
*confirmed* (word, analysis) pairs into an exact lexicon-only transducer (feasible, but abandons
open-class generativity — the entire point of an FST analyzer); or (b) accept that a PanGloss-shipped
"Divvun language" is, honestly, an over-generating acceptor with a documented false-positive rate,
and disclose that difference explicitly rather than presenting it as equivalent to a hand-compiled
GiellaLT analyzer. This is a product/scope decision, not an engineering task, and should happen
before Stage 2's toy repo is treated as a real precedent for a production language.

---

## 8. Blocker list

- **[technical-hard]** PanGloss's foma net is architecturally a proposer that depends on a live
  external verifier (HermitCrab `confirm`); Divvun ships the FST as the complete analyzer with no
  runtime verifier. Shipping "just the FST" produces unconfirmed false-positive analyses. Resolving
  this without abandoning open-vocabulary generativity is a genuine unsolved design problem (§2, §7
  stage 3), not a matter of more engineering effort on the current architecture.
- **[technical-hard]** If HC rewrite-rule compilation via foma's replace calculus (`replace.rs`'s
  Private-Use-Area token scheme) becomes the mainline emitter (per the plan's own stated direction),
  the lower/surface tape stops being legible orthography — breaking every downstream consumer
  (CG, hyphenation, generation tests) that needs literal wordforms, not just PanGloss's own analyzer
  (§2).
- **[technical-cost]** The emitted upper tape carries zero linguistic tag content today
  (`<R:nnnn>`/`<M:nnnn>` only); building a real GiellaLT-shaped tag emitter is credible but net-new
  work, requiring both an encoding layer and (see next item) a human tagset design (§3).
- **[social-or-licensing]** GiellaLT's own tradition treats the analysis tagset (names, ordering,
  feature inventory) as a per-language human/linguist decision, not something giella-core validates
  or a generator can derive unassisted — a real design bottleneck for "any language," since each new
  language needs its own linguist-authored tag convention regardless of who builds the FST (§3, §6).
- **[technical-cost]** GiellaLT's build has no artifact-level seam — every path we checked compiles
  lexc/twolc from source via its own Makefiles; slotting in a compiled binary would mean reverse
  engineering and bypassing dozens of intermediate Makefile targets, unsupported and untested by the
  ecosystem's own tooling (§4).
- **[technical-cost]** Full shippability requires artifacts PanGloss does not produce at all today:
  a separate error-model FST reconciled to the acceptor's alphabet, an `index.xml`, and (for anything
  beyond bare lookup) hand-written Constraint Grammar disambiguation/grammar-check rules and a
  separate hyphenation FST (§5).
- **[social-or-licensing]** Shipping under the `giellalt`/`divvun` name and distribution channel
  requires UiT/Divvun org-maintainer sign-off (repo creation needs org-admin rights; "Production"
  maturity is adjudicated by maintainers) — independent of whatever license the code itself carries
  (§6).
- **[social-or-licensing]** No documented policy exists anywhere in GiellaLT for incorporating
  FieldWorks/FLEx-derived lexical data, which may carry SIL's own license terms; this is a genuine
  gap requiring a direct human/legal decision, not resolvable by reading the existing docs (§5, §6).
- **[unknown]** `HAVE_FOMA`/`--with-foma` status of whatever HFST build Divvun's own production
  pipeline actually uses — if it's built `--without-foma`, the `FOMA_TYPE` conversion path in §1.4 is
  unavailable there regardless of what's true of a from-source HFST build (§1.2). Not verified either
  way in this pass.
- **[unknown]** Contents/requirements of `divvun/private-registry` and `divvun/pahkat.uit.no-index` —
  both referenced but not fetchable in this research pass (§5).
- **[unknown]** Whether foma-rs's `apply_up`/`apply_down` orientation matches HFST's `hfst-lookup`
  convention out of the box, or needs the inversion `hfst-fst2fst` warns about (§1.2) — flagged as a
  concrete thing to test in Stage 1, not assumed either way.

---

## Appendix: pinned versions referenced

- `foma` crate: `foma = "=0.4.2"` (`PanGloss/rust/crates/pg-foma/Cargo.toml:23`); fork tracked at
  `johnml1135/foma-rs` per the shared brief (not independently re-verified in this pass — the
  `divvun/foma-rs` upstream defects cited in §1.1/§2 are filed against the upstream repo per
  `tags.rs:276-277`).
- HFST checkout: `AC_INIT([hfst],[3.17.1],...)` (`hfst/configure.ac:34`).
- divvunspell checkout: commit `e768dc6d`, 2026-06-24, `origin` = `https://github.com/divvun/divvunspell.git`.
