# Divvun/GiellaLT architecture and toolchain — research report 1/6

Scope: the actual GiellaLT build pipeline, which FST backends it really uses, why
`divvun/foma-rs` exists, the `divvun/registry` contract, what joining Divvun requires, and
implications for PanGloss. All claims marked **VERIFIED** (read directly, cited) or
**INFERRED** (reasoned from verified facts, cited to the facts it rests on). Unknowns are
stated as such, not guessed.

Sources cloned shallow (`git clone --depth 1`) into
`C:/Users/johnm/AppData/Local/Temp/claude/C--Users-johnm-Documents-repos-LCAtom/1b5e24e2-aeac-4668-b883-e199cfb811d9/scratchpad/divvun/a1/`:
`foma-rs/`, `registry/`, `giella-core/`, `lang-sme/`. Everything under that path is cited as
`scratch/<repo>/<path>:<line>` below (repo root = that directory). GitHub API (`gh api`,
public, unauthenticated) used for commit/release history that a shallow clone can't show.

---

## 1. The actual GiellaLT pipeline, stage by stage

**VERIFIED**, from `scratch/lang-sme` (a real, current `lang-*` repo, North Sámi) and
`scratch/giella-core` (the shared build machinery):

1. **Source**: hand-authored `lexc` (lexicon), `twolc` or `xfst`-style rewrite rules
   (phonology), and constraint-grammar (`.cg3`) disambiguation rules, laid out under
   `src/fst/{morphology,orthography,phonetics,syllabification,tagsets}/` and `src/cg3/`
   (`scratch/lang-sme/src/Makefile.am` directory list; confirmed by `find` over the tree).
2. **Lexicon compilation**: `.lexc` → `.hfst` via `hfst-lexc`, or → `.foma` via the `foma`
   interpreter's `read lexc` command — both rules exist side by side
   (`scratch/giella-core/am-shared/lexc-include.am:22-45`, quoted in §2 below).
3. **Phonology/rewrite rules**: `hfst-twolc` (two-level rules) or `hfst-xfst`/`foma`
   (Xerox-regex replace rules), composed onto the lexicon via `hfst-compose`/
   `hfst-compose-intersect` (`gt_PROG_HFST` wires all of these as `AC_PATH_PROG` checks,
   `scratch/lang-sme/m4/hfst.m4:53-92`, including `HFST_COMPOSE`, `HFST_COMPOSE_INTERSECT`,
   `HFST_TWOLC`, `HFST_XFST`, `HFST_REGEXP2FST`).
4. **Format conversion / packaging**: `hfst-fst2fst` converts the composed network between
   backend formats (openfst-tropical/log, sfst, foma) and to the optimized-lookup (`.hfstol`)
   runtime format used for fast analysis (`scratch/lang-sme/m4/hfst.m4:60`, `:72`;
   `hfst-format-include.am` below).
5. **Disambiguation**: `vislcg3` (compiled `.cg3b` grammars via `cg-comp`, applied via
   `vislcg3`/`cg-proc`) reduces the FST's raw analyses to the linguistically preferred reading
   (`scratch/lang-sme/m4/giella-macros.m4:452-461`, `AC_PATH_PROG([VISLCG3]…)`,
   `AC_PATH_PROG([VISLCG3_COMP], [cg-comp]…)`).
6. **Tokenization / pattern matching**: `hfst-tokenize` / `hfst-pmatch` compiled from
   `pmatch` sources for text segmentation ahead of morphological analysis (`HFST_TOKENISE`,
   `scratch/lang-sme/m4/hfst.m4:89`; `hfst-pmatch2fst`, `:75`).
7. **Spelling**: the analyser network (with generator/acceptor projections) is packaged as a
   `.zhfst`/`.bhfst` archive consumed at runtime by `hfst-ospell` (desktop/legacy) or
   `divvunspell` (Divvun's own Rust engine) — confirmed by the registry's own description of
   `divvunspell`: "Spell checking engine and library for ZHFST/BHFST spellers"
   (`scratch/registry/README.md:54`).
8. **Grammar checking**: `vislcg3`-disambiguated + further CG-tagged output feeds a grammar
   checker consumed by `divvun-gramcheck-web`/`divvun-runtime`
   (`scratch/registry/README.md:34,50`).
9. **Build orchestration**: GNU **autotools** (`autogen.sh` → `configure` → `make` →
   `make install`), language-independent logic factored into `giella-core`'s
   `am-shared/*.am` fragments (`00-DO-NOT-EDIT-THIS-DIRECTORY-readme.txt`, confirmed present
   at `scratch/giella-core/am-shared/`) plus per-language `configure.ac`/`Makefile.am` in each
   `lang-*` repo. `./configure --help` in a language repo enumerates
   `--enable-dicts`/`--disable-generators`/`--without-xfst`/`--with-backend-format=FORMAT`
   etc. (`giellalt.github.io/infra/infraremake/NewInfraTechnicalOverview.html`, **VERIFIED**
   via WebFetch, and `--with-backend-format` independently confirmed by reading
   `scratch/lang-sme/m4/hfst.m4:136-140` directly, quoted §2).
10. **CI/CD and distribution**: **not** per-repo CI config — `lang-*`/`keyboard-*`/`dict-*`
    repos are auto-connected to **Buildkite** by a `sync-github` service in the
    `divvun-actions` repo (TypeScript/Deno); builds show at builds.giellalt.org; artefacts are
    published to **Páhkat** repositories and pulled by Divvun Manager / mobile keyboard apps
    (`scratch/registry/CI_CD.md:1-20`, quoted in full above — this is the entire file).

Everything in 1–9 is **VERIFIED** by reading actual `lang-sme` and `giella-core` build
files, not by reading docs about them.

---

## 2. Which FST backends does GiellaLT actually support? (foma vs HFST vs Xerox)

**Foma is real, wired, and buildable — but it is explicitly the *fallback of last resort* in
the GiellaLT autotools logic, not a co-equal first-class backend, and it cannot compile the
majority of real grammars because most use `twolc` phonology, which foma cannot read.**

Quoted evidence, all **VERIFIED**:

- `scratch/lang-sme/m4/giella-macros.m4:413-421` (`gt_PROG_FOMA`):
  ```
  # If Xerox tools and Hfst are not found, assume we want Foma:
  AS_IF([test x$gt_prog_xfst = xno -a x$gt_prog_hfst = xno ...],
        [with_foma=yes
         fallback_to_foma="INFO: Neither Xfst nor Hfst were found, falling back to using Foma"])
  ```
- `scratch/lang-sme/m4/giella-macros.m4:436-443` — a **hard configure error**, not a warning,
  if foma is selected but the grammar's phonology is `twolc`:
  ```
  AS_IF([test x$gt_prog_foma = xyes -a "x$(grep 'GT_PHONOLOGY_MAIN' .../src/fst/Makefile*.am | grep 'twolc')" != "x"],
        [gt_MSG_ERROR([You only have Foma, or you requested to use Foma, but
  your main phonology file is a twolc file, which Foma can not compile. ...])])
  ```
- Separately, HFST itself is a **multi-backend framework** and "foma" is also one of HFST's
  four *internal automaton storage formats* (`sfst`, `foma`, `openfst-tropical`,
  `openfst-log`), selected by `--with-backend-format` and consumed by every `HFST_*` tool via
  `$HFST_FORMAT`:
  - `scratch/lang-sme/m4/hfst.m4:136-145`:
    ```
    AC_ARG_WITH([backend-format],
      [AS_HELP_STRING([--with-backend-format=FORMAT],
        [enable the hfst backend format specified (one of: sfst, foma, openfst-tropical, openfst-log) @<:@default=$DEFAULT_HFST_BACKEND@:>@])], ...)
    AM_CONDITIONAL([WITH_FOMA], [test "$with_backend" == foma])
    ```
  - `scratch/giella-core/am-shared/hfst-format-include.am:20-23`:
    ```
    if WITH_FOMA
    HFST_FORMAT= --format=foma
    HFST_OLFORMAT= --format=optimized-lookup-unweighted
    endif # WITH_FOMA
    ```
  - North Sámi's own `configure.ac` picks exactly this: `DEFAULT_HFST=[yes]`,
    `DEFAULT_FOMA=[no]`, but `DEFAULT_HFST_BACKEND=[foma]`
    (`scratch/lang-sme/configure.ac:167-171`) — i.e. lang-sme's *default* build uses **HFST
    tools** (`hfst-lexc`, `hfst-twolc`, `hfst-compose`, …) with **foma's automaton
    representation as HFST's internal storage backend**, NOT the standalone `foma`
    interpreter. This is a completely different sense of "foma" than "compile my grammar with
    the foma binary" — it means "HFST, using foma's C library under the hood for the
    unweighted algebra," chosen because Sámi languages are unweighted (no OpenFST tropical
    weights needed) and foma's C engine is faster/lighter than SFST for that case.
  - A **separate, genuinely-standalone-foma path also exists**, for producing a `.foma`
    speller acceptor artifact: `scratch/giella-core/am-shared/lexc-include.am:32-38`
    (verbatim):
    ```
    ####### Foma build rules: #######
    .generated/%.foma: .generated/%.lexc $(GENDIR)
        $(AM_V_FOMA)"$(FOMA)" $(VERBOSITY) \
                -e "set lexc-align ON" \
                -e "read lexc $<" \
                -e "save stack $@ " \
                -s
    ```
    and `scratch/giella-core/am-shared/tools-spellcheckers-fstbased-desktop-foma-dir-include.am:33-57`,
    which either builds a `.foma` acceptor by composing HFST-built filter transducers and
    then converting with `hfst-fst2fst -f foma`, or — per the include name
    (`...-foma-dir-include.am` vs the sibling `...-hfst-dir-include.am`) — has a genuinely
    separate all-foma path for spellers, gated by `CAN_FOMA_SPELLER`.
- **Conclusion (VERIFIED, converging from both angles): foma is a real, tested, CI-exercised
  backend in the GiellaLT build system** — both as an HFST storage format and as a standalone
  lexc/xfst compiler for spellers — **but it is scoped to unweighted, twolc-free grammars,
  is the explicit fallback when Xerox/HFST tools are absent, and is not the default toolchain
  choice for any language repo inspected here.** Whether other `lang-*` repos default to
  `DEFAULT_FOMA=yes` is **unknown** — only `lang-sme` was inspected; the m4 macros make clear
  this is a per-language `configure.ac` choice, not a global GiellaLT policy.
- xfst (true Xerox tools) is a third, independent option, disableable via `--without-xfst`
  (WebFetch of `giellalt.github.io/infra/infraremake/NewInfraTechnicalOverview.html`,
  **VERIFIED** by the fetch, not independently cross-checked in m4 source — the m4 files read
  here confirm `gt_prog_xfst` is checked but the macro body itself was not opened).

---

## 3. Why does `divvun/foma-rs` exist? Is Divvun "moving to foma-rust"?

**This is bigger than a single crate.** As of the last commit visible (`gh api
repos/divvun/foma-rs/commits`, 2026-07-19T21:36:48Z; repo `created_at` 2026-07-12T18:02:48Z —
**VERIFIED** via `gh api repos/divvun/foma-rs -q '.created_at,.pushed_at'`), Divvun/bbqsrc
(GitHub user `bbqsrc` = Brendan Molloy, Divvun's lead engineer — VERIFIED, crates.io publisher
identity below) started an aggressive, simultaneous, multi-repo initiative to **port the
entire native C/C++ toolchain to pure Rust**, all created within one week of each other:

| Repo | Ports | Created | Last push seen |
|---|---|---|---|
| `divvun/foma-rs` | C **foma** (Mans Hulden) → Rust `foma` crate | 2026-07-12 | 2026-07-19 |
| `divvun/hfst-rs` | C++ **HFST** (`libhfst`) → Rust `hfst` crate | 2026-07-12 | 2026-07-19 |
| `divvun/cg3-rs` | C++ **VISL CG-3** → Rust `cg3` crate | 2026-07-12 | 2026-07-20 |
| `divvun/divvunspell` | (pre-existing, Rust already) `divvun-fst` lib | — | — |

(dates **VERIFIED** via `gh api repos/divvun/{foma-rs,hfst-rs,cg3-rs} -q '.created_at,.pushed_at'`)

**The relationship is: `foma-rs` is a sibling dependency *of* `hfst-rs`, not a parallel
standalone replacement runtime.** `hfst-rs`'s own README states this explicitly
(`gh api repos/divvun/hfst-rs/contents/README.md`, **VERIFIED**, quoted):

> "A HFST transducer is backend-neutral; the port implements the same facade over: ...
> **foma / unweighted** — the native `foma` crate (a sibling Rust port), on by default via
> the `foma` feature; the fast path for unweighted algebra and `.foma` I/O. ... Out of scope
> by design: ... the SFST and native-C++ OpenFST back-ends (replaced by `rustfst` and the
> native `foma` crate)."

So in the new pure-Rust stack, **HFST remains the front-end/format layer** (its own
`nfst-lexc`/`nfst-twolc`/`nfst-xfst`/`nfst-pmatch`/`nfst-xre` parser crates, from
`necessary-nu/nfst`, feed HFST's compilers) and **foma-rs is plugged in underneath it as the
unweighted-algebra backend** — precisely mirroring the classic C-era relationship
(`--with-backend-format=foma` inside HFST, §2 above), just reimplemented in Rust instead of
linking C foma via FFI. `divvun-runtime` (the pipeline orchestrator Divvun actually ships;
"Modular language processing pipeline system for grammar checkers and text-to-speech",
`scratch/registry/README.md:50`) depends on the **`hfst`** crate (git dependency,
`divvun/hfst-rs`) and **`cg3`** crate, feature-gated as `mod-hfst`/`mod-cg3`
(`gh api repos/divvun/divvun-runtime/contents/Cargo.toml`, **VERIFIED**, quoted):
```
# Native pure-Rust HFST port (replaces the old C++ FFI wrapper hfst-rs).
hfst = { git = "https://github.com/divvun/hfst-rs" }
# Native pure-Rust VISL CG-3 port (replaces the old C++ FFI wrapper).
cg3 = { git = "https://github.com/divvun/cg3-rs" }
divvun-fst = { git = "https://github.com/divvun/divvunspell" }
```
There is **no direct `foma = {...}` dependency line in `divvun-runtime`'s `Cargo.toml`** —
foma-rs reaches the runtime only transitively, through `hfst-rs`'s default `foma` feature.

**Git history confirms this is a from-scratch, days-old effort, not years of gradual
migration**, and also shows *how* the port was done: `gh api repos/divvun/foma-rs/commits`
(**VERIFIED**, 258 commits total) shows a bimodal history — a handful of commits from 2017,
2020, 2021 (authors `mhulden`, `Sjur N Moshagen`, `Ambient Lighter`, `Yonghee Kim` — i.e. the
literal history of **upstream `mhulden/foma`**, carried over into this repo, presumably by
branching/importing it as the port's starting point so the C reference stayed alongside the
new Rust modules) followed by **205 commits from `Brendan Molloy` between 2026-07-13 and
2026-07-19** doing the actual port. Commit message pattern (`polish:`, `revisit:`, `spec:`,
`done:` prefixes referencing a work-breakdown structure — e.g. "mark w10-dead-cruft done",
"re-verify and bump 207 stale annotation version pins") matches the README's stated
methodology: literal 1:1 port → behavioral spec (`docs/spec/port/`, 545 tests) → idiomatization
(`scratch/foma-rs/README.md:19-33`, **VERIFIED**). A `plan/main.styx` work-tracking file in the
repo (**VERIFIED**, read directly) confirms a structured, wave-based plan (`wave-1-markup`:
"author sem rules for all 947 C functions", broken into per-file passes) — this is a
methodical spec-then-port effort, not an experiment.

**Release cadence**: crates.io shows the `foma` crate went from `0.1.0` (2026-07-12, yanked
minutes later) through `0.4.2` (2026-07-19) in one week — 6 published versions in 7 days, all
published by `bbqsrc` (**VERIFIED** via `curl https://crates.io/api/v1/crates/foma`, full
JSON captured). **No commits or releases are visible after 2026-07-19** in either the git log
or crates.io version history fetched here — i.e., as of this research (conversation "today" =
2026-07-30), **the project went quiet for at least 11 days** after an intense one-week burst.
Whether it resumed after that is **unknown** — this agent's data cutoff is the live query
above; no evidence either way beyond it. One open issue exists,
`Multichar symbol names containing a literal 0 digit are dropped from sigma` (#2, opened
2026-07-26, **VERIFIED** via `gh api repos/divvun/foma-rs/issues`) — filed *after* the last
visible commit, unaddressed as of the query.

**crates.io ownership** (**VERIFIED**, `curl .../owners`): `foma`, `nfst-lexc` (and by
extension presumably `nfst-xre`, not separately queried but same publishing pattern) are all
solely owned by `bbqsrc` (Brendan Molloy). `necessary-nu/nfst`
(`gh api repos/necessary-nu/nfst`, **VERIFIED**) is a separate GitHub org holding the
HFST/Xerox-grammar parser crates (`nfst-syntax`, `nfst-xre`, `nfst-lexc`, and per its own
README also `nfst-twolc`/`nfst-xfst`/`nfst-pmatch` used by `hfst-rs`) — description: "Rust
parsers, ASTs, and pretty-printers for the finite-state grammar languages of the HFST / Xerox
toolchain." `necessary-nu` appears to be Brendan Molloy's personal/secondary org (his crates.io
account email domain is `necessary.nu`, matching the `plan/main.styx` commit author
`brendan@necessary.nu`, **VERIFIED**) rather than an independent third party — **INFERRED**
from the matching name and commit metadata, not confirmed by an explicit "same person"
statement anywhere.

**Verdict on assumption (a) — "Divvun is or soon will be moving to foma-rust":**
**Half true, materially reframed.** Divvun (specifically bbqsrc) is executing a from-scratch,
~1-week-old (as of the plan doc's 2026-07-15/16 date; ~2.5 weeks old as of this report's date)
initiative to reimplement its **entire native toolchain** — HFST, foma, and VISL CG-3 — as
pure Rust, wired into `divvun-runtime`. **foma-rs is real and does exist**, but it is not
itself "the runtime Divvun is moving to" — it's the unweighted-FST engine living *inside* the
new pure-Rust HFST port, exactly the role classic C foma already played inside HFST via
`--with-backend-format=foma`. Divvun's actual runtime dependency is on `hfst-rs` (+`cg3-rs`
+`divvunspell`), not on the bare `foma` crate. **None of this new Rust stack appears in the
production `lang-sme` build** inspected here — `lang-sme`'s `configure.ac`/m4 macros still
shell out to installed **C** `hfst-*`/`foma`/`vislcg3` binaries (§1–2 above), with no
reference to `hfst-rs`/`foma-rs`/`cg3-rs` found anywhere in that repo. So: real, active,
fast-moving, but **still an R&D effort not yet wired into the language-model build pipeline**,
and even once it is, its role for foma-rs specifically is "backend inside HFST," not
"replacement of the whole toolchain's format/frontend."

---

## 4. The `divvun/registry` — what does it actually register?

**VERIFIED** by reading the whole repo (`scratch/registry/`, 2 files: `README.md`,
`CI_CD.md` — the repo is small, no manifest schema or artifact format lives here):

- It is **not** a language/manifest registry. It is a **catalog of Divvun's own software
  repositories** (~30 repos) with columns: short description, language(s), target platform,
  license, open-issue count (`scratch/registry/README.md:11-15`). Three sections: "Product
  Software" (end-user apps: Divvun Manager, keyboards, grammar-checker web plugins, spell
  checker webeditor, sátni.org dictionary site…), "Support Software" (libraries/tools:
  `divvunspell`, `divvun-runtime`, `kbdgen`, `pahkat`, `gut`, `CorpusTools`, `morph-test`,
  `GiellaLTGramTools`/`GiellaLTLexTools`…), "Build and Release Infrastructure"
  (`divvun-actions`, `buildkite-overview`, `zulip-buildkite-bot`, `rsigncode`).
- It **explicitly says** language/keyboard/dictionary resources are catalogued *elsewhere*:
  "The GiellaLT linguistic resources are not listed here — see the automatically generated
  overviews of keyboard layouts, language models and shared resources instead"
  (`scratch/registry/README.md:6-9`, links to `giellalt.github.io/{KeyboardLayouts,
  LanguageModels,SharedResources}.html` — those pages were **not** fetched in this pass; their
  exact format is **unknown** from this research).
- **The only "manifest" artifact found in this research is Páhkat's `manifest.toml`** — a
  **package-metadata** file (product IDs, localized names/descriptions per platform, Windows
  MSI product codes, per-tool version numbers), **not** a language-model or FST-capability
  contract. Read directly from `scratch/lang-sme/manifest.toml` (**VERIFIED**, quoted in
  relevant part):
  ```toml
  [package.speller]
  name = "North Sami"
  version = "4.5.2"
  [package.grammar]
  name = "North Sami"
  version = "1.3.3"
  [package.tts-textproc]
  name = "North Sami"
  version = "1.0.1"
  [package.hyphenator]
  name = "North Sami"
  version = "0.1.0"
  ```
  This is populated from `configure.ac`'s `AC_SUBST` variables (`SPELLERVERSION`,
  `GRAMCHECKVERSION`, etc. — `scratch/lang-sme/configure.ac:65,74,77,80`) via
  `manifest.toml.in` → `manifest.toml` autotools substitution, per the "Edit `manifest.toml.in`
  with proper UUID product ID" step in the new-language checklist (WebFetch of
  `giellalt.github.io/infra/HowToAddANewLanguage.html`, **VERIFIED** by the fetch).
- **What runtimes consume it**: Páhkat (`divvun/pahkat`, "Reference implementation of the
  Páhkat package management standard") is the package manager; Divvun Manager
  (macOS/Windows) and the mobile keyboard apps are the clients that install/update packages
  described by these manifests (`scratch/registry/README.md:30-31,37`, `CI_CD.md:19-20`:
  "Built artefacts are published to Páhkat repositories and distributed to end users via
  Divvun Manager and the mobile keyboard apps").
- **Is there a documented contract we could target?** Only at the packaging layer (Páhkat
  manifest = product metadata for installers), **not** at the linguistic/FST layer. There is
  no "registry" document specifying required FST symbol conventions, tag alphabets, or a
  formal I/O contract a third-party analyzer must satisfy to be "Divvun-compatible" — **this
  research found no such document**; it may exist in `giellalt.github.io`'s deeper pages
  (`LanguageModels.html`, `infra/` tree) not fetched here — flagged as an open question in §7.

---

## 5. What would "a new language joins Divvun/GiellaLT" require in practice?

**VERIFIED** via WebFetch of `giellalt.github.io/infra/HowToAddANewLanguage.html`
(content synthesized by the fetch tool from the live page, not independently re-verified
against a second source — flagged INFERRED-adjacent where noted):

1. **Tooling**: the `gut` CLI (`divvun/gut`, "A Git(Hub) multirepo maintenance tool", Rust —
   independently confirmed to exist in `scratch/registry/README.md:59`). Requires GitHub admin
   access to the `giellalt` org.
2. **Naming**: repo = `lang-XXX` where `XXX` is the **ISO 639-3** three-letter code
   (`gt_PROG_...` macros reuse this as `GLANG`, confirmed live in
   `scratch/lang-sme/configure.ac:45-56`: `GLANG=sme`, `GLANG2=se`,
   `GLANGUAGE="North Sami"`). Keyboard repos are `keyboard-XXX`.
3. **Scaffolding**: `gut template generate -t template-lang-und -d lang-XXX`, prompting for
   ISO codes, English name, license (e.g. `LGPLv3` — lang-sme itself is GPL-3.0-or-later per
   its own file headers, `scratch/lang-sme/configure.ac:6-16`, so license choice is
   per-language, not fixed), repo name.
4. **Repo creation**: `chmod a+x autogen.sh`, commit, then `gut create repo -d . -o giellalt
   -r lang-XXX -p`; `git pull -u origin main`; set description/website; add a Zulip webhook;
   add topic tags (maturity/family/location).
5. **GitHub Pages**: branch `main` + `/docs` initially, switched to `gh_pages` root after
   first build (for the auto-generated docs site).
6. **CI/CD**: per §1/§4 above, **zero manual CI config** — Buildkite auto-connects via
   `divvun-actions`'s `sync-github` service once the repo exists under `giellalt`
   (`scratch/registry/CI_CD.md:15-17`, VERIFIED).
7. **Packaging metadata**: edit `manifest.toml.in` with a real product UUID; run
   `./autogen.sh && ./configure`; for keyboards, add a UUID to
   `XXX.kbdgen/targets/win.yaml`.
8. **Distribution access**: verify Páhkat write access/team membership; follow the Páhkat
   index instructions; request DevOps restart the `divvun-web` droplet (this last step
   strongly suggests a **manual, human-in-the-loop DevOps step still gates first
   publication** — not fully self-service).
9. **Local dev loop**: `cd lang-LANGCODE && ./autogen.sh && ./configure && make && make check`.
10. **Governance**: the requirement for GitHub **admin** access to the `giellalt` org to create
    the repo, plus the manual Páhkat-team-and-DevOps steps, indicates a **gatekept, not
    self-service, onboarding** — someone with org-admin rights must sponsor a new language.
    No explicit written review/approval policy (e.g. "language committee sign-off") was found
    in the pages fetched; this may exist elsewhere in `giellalt.github.io` — **unknown**.

---

## 6. Implications for PanGloss

### What we could plug into today

- **The lexc format itself is a real, portable target.** `scratch/giella-core`'s build rules
  (`lexc-include.am:22-45`, `hfst-format-include.am`, quoted §1–2) show `.lexc` is compiled by
  **either** `hfst-lexc` **or** foma's own `read lexc`. PanGloss's `pg-foma::emit` already
  emits standard `lexc` (`LEXICON`, `Multichar_Symbols` — VERIFIED,
  `rust/crates/pg-foma/src/emit.rs:1170-1216,3278-3338` locally), which is the same source
  language GiellaLT's own foma path consumes. In principle a PanGloss-emitted `.lexc` file
  is at least *syntactically* the same kind of artifact GiellaLT's `foma`/`hfst-lexc` compile
  — see caveats below.
- **`hfst-ospell`/`divvunspell` consume `.zhfst`/`.bhfst` archives**, which are ultimately
  compiled FST + affix data, format-agnostic at the storage layer once built. If PanGloss ever
  needed to interoperate at the *artifact* level (not the source level), the target format is
  well-documented by HFST/Divvun tooling, not something we'd have to invent.
- **`vislcg3`** is a mature, standalone, well-specified disambiguation stage that operates on
  cohorts of FST analyses — orthogonal to whether the FST itself came from foma, HFST, or
  PanGloss's proposer. If PanGloss ever wanted post-hoc disambiguation among multiple
  confirmed analyses, `vislcg3`/`cg3-rs` is a plausible, independent, off-the-shelf component.

### What is structurally incompatible

- **PanGloss's foma network is a proposer, not a standalone analyzer, by design** (per
  `docs/fst-plan/foma-fst-plan.md:60-70`, VERIFIED, this repo): it deliberately over-generates
  and depends on the Rust HermitCrab engine's `confirm()` step to prune. GiellaLT's/Divvun's
  toolchain has **no equivalent confirm/prune stage** — their FSTs (foma or HFST) are the
  *entire* analyzer; recall AND precision both come from the compiled network alone. Shipping
  a PanGloss-emitted `.lexc`/`.foma` file into a Divvun pipeline as-is would produce a speller/
  analyzer with the over-generation baked in and un-pruned — **it would not behave correctly
  standalone**. Running PanGloss "on Divvun's infrastructure" would require porting the
  confirm step too, which is PanGloss/HermitCrab-specific and has no analog in GiellaLT's
  toolchain.
- **v1 emission (current mainline, D3) has no replace rules** — it's lexc + pre-probed
  surface-variant enumeration (`docs/fst-plan/foma-fst-plan.md:136-144`, VERIFIED). The P6
  follow-on that *does* emit real foma replace-calculus rules found **three separate
  correctness issues in the vendored `foma-rs` fork's flag-diacritic handling inside replace
  rules** (`docs/fst-plan/foma-fst-plan.md:494-507`, VERIFIED, this repo) and had to route
  around them with a bespoke flag-free static partition scheme instead of the standard
  xfst/foma flag-diacritic idiom. That is direct evidence that our compiled output, where it
  goes beyond bare lexc, is currently tuned to specific (and per this evidence, partly
  nonstandard/buggy) behavior of **`foma = "=0.4.2"`** as vendored
  (`rust/crates/pg-foma/Cargo.toml:12-24`, VERIFIED, pinned exact "so a version bump is always
  deliberate," with an open unresolved tag-dropping bug noted in the same comment block) —
  not necessarily behavior a standard C foma, HFST's foma backend, or a future foma-rs release
  would reproduce identically. "Runs anywhere, including potentially on Divvun" is therefore
  **not proven** for anything past the v1 lexc-only subset, and even v1's tag alphabet
  (`<R:nnnn>`/`<M:nnnn>` multichar symbols keyed to PanGloss's internal `MorphemeId`,
  `docs/fst-plan/foma-fst-plan.md:132-135`, VERIFIED) is PanGloss-specific and would need a
  translation layer to mean anything to GiellaLT/Divvun tooling, which expects its own tagset
  conventions (POS/feature tags meaningful to CG3 grammars and the shipped analyzer's known
  consumers).
- **GiellaLT's own configure logic treats foma as second-class**: it cannot compile `twolc`
  phonology at all (hard error, §2 above) and is only auto-selected when Xerox and HFST tools
  are both absent. Most real `lang-*` phonologies likely use `twolc` or HFST's xfst dialect
  (only `lang-sme` was inspected here; whether this generalizes across the ~100+ GiellaLT
  language repos is **unknown**). This means "foma is a viable common target across Divvun
  languages" cannot be assumed without checking more `lang-*` repos.
- **Divvun's new pure-Rust stack (hfst-rs/foma-rs/cg3-rs) is not in the production build** at
  all yet (§3) — there is nothing "live" to integrate with today even if source-level
  compatibility were solved.

### Verdict on the two stated assumptions

- **(a) "Divvun is or soon will be moving to foma-rust"**: **Reframe, don't accept as-is.**
  Divvun is mid-flight on a broader from-scratch pure-Rust reimplementation of its *entire*
  native toolchain (HFST, foma, VISL CG-3), started ~2026-07-12, with 205+ commits in one
  week then apparently quiet for 11+ days as of this report. `foma-rs` exists and is real, but
  it is architecturally a **backend living inside the new `hfst-rs`**, exactly mirroring
  classic C foma's role inside classic C HFST — not a wholesale format switch away from HFST.
  None of it is wired into the actual `lang-*` language-build pipeline yet.
- **(b) "We produce a standard foma grammar that can run anywhere, including potentially on
  Divvun"**: **Only partly true, and only for the v1 lexc-only path.** The lexc *syntax* we
  emit is standard and would parse in GiellaLT's foma/HFST-lexc tooling. But (i) our compiled
  network is a proposer that requires PanGloss's own confirm step to be a correct analyzer —
  it is not a drop-in replacement for a GiellaLT analyzer as shipped; (ii) our tag alphabet is
  PanGloss-internal, not GiellaLT's tagset convention; (iii) the P6 replace-rule path already
  had to work around vendored-foma-rs-specific bugs, meaning our emitted artifact set, beyond
  bare lexc, is not proven portable to a different foma implementation or to HFST's own
  foma-format backend without re-verification.

---

## 7. Top 3 open questions this research could not answer

1. **Does foma-first (`DEFAULT_FOMA=yes`, no HFST/Xerox) generalize beyond `lang-sme`?**
   Only one `lang-*` repo was inspected. Whether most GiellaLT languages use `twolc` (foma-
   incompatible) or xfst-only phonology (foma-compatible) determines how broadly "foma is a
   viable common Divvun target" holds. Not answered here — would need sampling several more
   `lang-*` repos' `configure.ac`/`src/fst/Makefile.am` `GT_PHONOLOGY_MAIN` settings.
2. **What exactly does `giellalt.github.io/LanguageModels.html` / `SharedResources.html`
   register**, and is there a machine-readable manifest schema for language models
   specifically (as opposed to Páhkat's product-packaging `manifest.toml`)? The registry
   repo explicitly defers to these pages (`scratch/registry/README.md:6-9`) but they were not
   fetched in this pass — this is the most direct path to "a documented contract we could
   target" and remains open.
3. **Did the `hfst-rs`/`foma-rs`/`cg3-rs` pure-Rust initiative resume after 2026-07-19/20, and
   is there any stated intent (blog post, issue, PR discussion) to eventually route the
   `lang-*` production build through it instead of the C/C++ toolchain?** No commit, release,
   or discussion after that date was found via the GitHub API queries run here; this may
   simply reflect the query time window (this report's "today" is 2026-07-30) rather than the
   project being abandoned, and a targeted search of Divvun's Zulip/blog (not queried — no
   access) or a later GitHub check would resolve it.

---

## Appendix: raw evidence trail (for re-verification)

- `gh api repos/divvun/foma-rs -q '.description,.homepage,.owner.login,.created_at,.pushed_at'`
  → owner `divvun`, created `2026-07-12T18:02:48Z`, pushed `2026-07-19T21:37:06Z`.
- `gh api repos/divvun/foma-rs/commits --paginate` → 258 commits; month histogram:
  `2017-04:1, 2020-02:2, 2021-05:36, 2022-06:2, 2023-09:2, 2023-11:1, 2024-05:1, 2025-02:2,
  2026-02:6, 2026-07:205`.
- `curl https://crates.io/api/v1/crates/foma` → versions `0.1.0`(yanked)…`0.4.2`, all
  published by `bbqsrc`, `0.4.2` crate_size 342714 bytes, 31890 lines Rust across 52 files.
  `curl .../owners` for `foma` and `nfst-lexc` → sole owner `bbqsrc` both.
- `gh api repos/divvun/hfst-rs/contents/README.md` and `.../Cargo.toml`-equivalent facts
  (via `gh api repos/divvun/hfst-rs -q '.created_at,.pushed_at'` →
  `2026-07-12T19:00:26Z` / `2026-07-19T22:03:51Z`).
- `gh api repos/divvun/cg3-rs -q '.created_at,.pushed_at'` → `2026-07-12T17:51:58Z` /
  `2026-07-20T01:37:35Z`.
- `gh api repos/divvun/divvun-runtime/contents/Cargo.toml --jq '.content' | base64 -d` → full
  file read, `[workspace.dependencies]` and `[dependencies]`/`[features]` sections quoted
  above in full for the FST-relevant lines.
- `gh api repos/necessary-nu/nfst -q '.description'` and its README (`gh api
  repos/necessary-nu/nfst/contents/README.md` implied by the description fetch).
- Local repo reads: `docs/fst-plan/foma-fst-plan.md` (full file, this repo's own plan doc —
  not part of Divvun, but load-bearing for the "what do we emit / what's the architecture"
  half of this report) and `rust/crates/pg-foma/src/emit.rs` (partial — file exceeds the
  256KB single-read limit; read via `Grep` for structural markers and doc-comments, offsets
  1170-1216/3278-3338/4023-4200 range) and `rust/crates/pg-foma/Cargo.toml` (full file).
