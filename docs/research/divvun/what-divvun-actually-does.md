# What Divvun / GiellaLT actually does

The verified research record. If you want the argument rather than the evidence, read
[why-not-just-use-divvun.md](why-not-just-use-divvun.md) instead.

## Provenance — read this before citing anything here

This consolidates a research pass run on **2026-07-30** in which parallel agents read real
`lang-*` and `giella-core` sources from fresh clones, and a synthesizing session re-verified every
load-bearing claim directly against those sources before recording it. Seventeen raw reports were
produced; this document replaces them. They remain in git history at commit `e0dd20f` if the
underlying working notes are ever needed.

Three things to know about the status of what follows:

- **The GiellaLT clones no longer exist on this machine.** Claims about their sources are inherited
  from the 2026-07-30 verification and have not been re-checked since. Line numbers were accurate
  on that date against the then-current `main` of each repo.
- **Claims about PanGloss's own source were re-verified on 2026-07-31** against this worktree's
  base commit `ce58d83` and are current.
- Where the 2026-07-30 pass could not verify something, it is marked **[unverified]** here rather
  than smoothed over. Where a later report corrected an earlier one, only the corrected version
  appears; the superseded claim is noted where it is likely to resurface.

---

## 1. The pipeline

From `lang-sme` (North Sámi, their deepest grammar) and `giella-core` (the shared build machinery):

1. **Source.** Hand-authored `lexc` (lexicon), `twolc` *or* `xfst`-style rewrite rules (phonology),
   and `.cg3` constraint-grammar rules, under `src/fst/{morphology,orthography,phonetics,…}/` and
   `src/cg3/`.
2. **Lexicon compilation.** `.lexc` → `.hfst` via `hfst-lexc`, or → `.foma` via foma's
   `read lexc`. Both rules exist side by side (`giella-core/am-shared/lexc-include.am:22-45`).
3. **Phonology.** `hfst-twolc` (two-level) or `hfst-xfst`/`foma` (replace rules), composed onto the
   lexicon via `hfst-compose` / `hfst-compose-intersect`.
4. **Format conversion.** `hfst-fst2fst` converts between backend formats and to the
   optimized-lookup `.hfstol` runtime format.
5. **Disambiguation.** `vislcg3` on compiled `.cg3b` grammars, reducing the FST's raw analyses to
   the contextually preferred reading.
6. **Tokenization.** `hfst-tokenize` / `hfst-pmatch`.
7. **Spelling.** The analyzer network plus generator/acceptor projections, packaged as
   `.zhfst`/`.bhfst` and consumed by `hfst-ospell` or `divvunspell`.
8. **Build orchestration.** GNU autotools, with language-independent logic factored into
   `giella-core/am-shared/*.am` and per-language `configure.ac` / `Makefile.am`.
9. **CI/CD.** Not per-repo config — `lang-*` repos are auto-connected to Buildkite by a
   `sync-github` service in `divvun-actions`; artifacts publish to Páhkat and are pulled by Divvun
   Manager and the mobile keyboard apps.

Stages 1–9 were verified by reading the build files themselves, not documentation about them.

---

## 2. Five mechanisms, one per constraint shape

North Sámi's division of labor:

| Layer | Mechanism | Measured | Job |
|---|---|---|---|
| Phonology | `twolc` two-level rules, parallel-**intersected** | 112 named rules; 65 `Where…matched`; **0 flag diacritics** | consonant gradation, umlaut |
| Trigger transport | Deletable diacritics `X1-X9`/`Q1-Q9`/`W1-W9` mapped `:0`, plus archiphoneme `º` at the gradation site | ~30 symbols; trigger sets `WeG`, `Dummy`, `NonMPDummy` (`phonology.twolc:72,110-116`) | carry a grade trigger from the *affix* to the *stem* |
| Morphotactic legality | Flag diacritics | 1,118 in `src/`: 672 `@U.`, 214 `@P.`, 94 `@C.`, 79 `@R.`, 59 `@D.` | derivation order, compounding legality |
| Post-lexical pruning | Ordered `.o.` cascade over tag strings | ~20 `.regex` filters in `src/fst/filters/` | remove illegal tag strings |
| Disambiguation | `vislcg3`, strictly downstream | `disambiguator.cg3:1` pipe: `preprocess \| hfst-optimised-lookup …hfstol \| lookup2cg` | sentence-context reading selection |

**The design insight is the division of labor, not any single trick.** Each mechanism is matched to
a different shape of bound: concatenative morphotactics bounds additively (lexc), phonology bounds
via intersection/composition (twolc), ordering constraints bound monotonically (flags), tag-string
legality bounds regularly (filter cascade), and genuine ambiguity is deferred to context (CG).

An independent report reasoning only from finite-state theory, having read no Sámi source, reached
the identical "one mechanism per bound-shape" conclusion. That convergence is the strongest single
result of the investigation.

### Grammar-internal pruning happens inside the FST

The existence proof is `src/fst/filters/` — `block-illegal_compound-strings.regex`,
`remove-illegal-derivation-strings-flagbased.regex`, `convert_to_flags-CmpNP-tags.regex`. The
flag-based one is the exemplar: an inverse-replace rule that *inserts* `@D.`/`@P.` flag pairs
around each derivation tag, so "derivations must ascend Der1→Der5, no double passives" becomes flag
unification at apply time rather than an enumerated list of legal derivation sequences.

Constraint Grammar is strictly downstream of this and is contextual disambiguation only. The FST
has already decided well-formedness before CG sees anything.

---

## 3. Premises corrected

Each of these was believed before the research and is false or over-scoped.

### 3.1 Divvun does not run standalone foma for real languages

`lang-sme/configure.ac:167-171`:

```
AC_SUBST([DEFAULT_FOMA],         [no])
AC_SUBST([DEFAULT_HFST],         [yes])
AC_SUBST([DEFAULT_HFST_BACKEND], [foma])
```

Standalone foma is off. HFST is on, with foma's C library as HFST's *internal storage backend* —
`HfstDataTypes.h:51` defines `FOMA_TYPE, /**< A foma transducer, unweighted. */`. Different things;
the shared word makes the conflation easy.

Further, foma cannot compile their phonology. `giella-core/am-shared/` has `.foma` build rules for
`%.lexc`, `.regex`, and `.xfscript`, but **no `.twolc → .foma` rule**. `twolc-include.am:42-57`
holds a commented-out foma path with the reason stated verbatim:

```
######## Foma build rules (based on Hfst): #######
### Commented out for now, they interfere with proper Foma builds, and do not
### work for the Hfst + Foma combo.
```

The disabled code split twolc rules into `RULE_PARTS`, intersected them pairwise, dumped to AT&T
text and read that into foma. They attempted the bridge and abandoned it.

**Why this does not block us:** we emit replace-calculus rules, which is the `.xfscript` route, and
that route *is* foma-compilable in their own build. The twolc gap constrains our ability to
*consume* GiellaLT grammars, not to *produce* ours.

**[unverified]** `gt_PROG_FOMA`'s macro body — whether foma is genuinely the fallback of last
resort. Invoked at `lang-sme/configure.ac:181` but the body was absent from the shallow
`giella-core` clone. `DEFAULT_FOMA=no` is the solid form of the same point.

### 3.2 `foma-rs` is a young, one-person effort, and a backend component

Every upstream commit dated 2026-07-16/17, all by one author (Brendan Molloy), nothing in the 13
days that followed. It is a sibling crate plugged in *underneath* `hfst-rs` as its unweighted
backend; `divvun-runtime` depends on `hfst-rs`/`cg3-rs` rather than on `foma` directly, and none of
the Rust stack appears in the production `lang-sme` build.

"Divvun is moving to foma-rust" is better stated as: *one engineer began porting the native
toolchain to Rust two and a half weeks ago, and we adopted one crate before the effort had a track
record.* Not a reason to reverse course — we already ship on it — but a dependency risk to hold
consciously.

### 3.3 Flag diacritics do not carry the long-range phonology

This was the pivotal open question, and the answer is no. `phonology.twolc` contains **zero** flag
diacritics. The 1,118 flags do morphotactic and derivational bookkeeping only. Gradation runs on
the trigger-diacritic mechanism: deletable symbols on the affix, mapped to zero on the surface,
read by two-level rules on the stem.

**This is the most promising finding for PanGloss.** That mechanism is functionally what HC's MPR
features are — a morphophonological trigger transported across intervening material as a symbol on
the tape. See [ideas-worth-borrowing.md](ideas-worth-borrowing.md).

### 3.4 The two-level/cascade seam is real but declinable

Originally recorded as *the* blocker deciding whether HC could be retired. That was over-weighted.
PanGloss never needs to emit or consume `twolc`, so the hard translation direction — cascade →
two-level, which requires a linguist to reformulate every rule and abandon abstract intermediate
segments — is a job we can decline.

The decisive evidence is GiellaLT's own portfolio:

- **`lang-kal` (Kalaallisut) writes all its phonology as a replace-rule cascade.** 7 `.xfscript`
  files, `phonology.xfscript` = 512 lines, and **zero `.twolc` files anywhere in the repo**. It
  ships a released grammar bundle and a packaged speller. The hardest polysynthetic language in
  their portfolio uses our formalism.
- **The build asymmetry is exact.** `giella-core/am-shared/xfscript-include.am:28-29` has a live,
  uncommented `.xfscript → .foma` rule invoking `$(FOMA) -l $< -e "save stack $@" -s`, against
  `twolc-include.am:42-57` commented out as interfering with foma builds.
- **~30 `lang-*` repos contain `phonology.xfscript`** (org-wide code search, not verified
  repo-by-repo).
- **No live phonology requiring true two-level was found.** The one mutual-conditioning candidate —
  diphthong simplification needing gradation-grade context, `phonology.twolc:1204-1291` — was
  attempted as a genuine two-level construct three times and abandoned each time by GiellaLT's own
  developers in favor of the trigger-diacritic device, which transfers to the cascade unchanged.

Every named `twolc` facility either has a direct same-cost replace-calculus idiom, is a pure
notational convention with no formal content (`Sets`, archiphonemes, boundary symbols), or requires
enumeration (alpha-variables) — and that last cost is one PanGloss's compiler already pays for HC's
own alpha-variables, not a new tax.

### 3.5 The flag/replace-rule defect in `foma-rs` is real but narrowly scoped

`rust/crates/pg-foma/src/gate.rs:1-20` (re-verified 2026-07-31) concludes that `->` and flags "do
not mix safely in this port". That is **correct in observation, over-scoped in conclusion.**

The mechanism, verified from source in both languages: the replace calculus is entirely
**flag-blind** — `grep -ci flag` returns **0** in both `foma-rs/crates/foma/src/rewrite.rs` and
upstream `foma/foma/rewrite.c` — while `apply` treats any flag-shaped symbol as **zero-width
unconditionally** (`foma/apply.c:1084`: *"For flags, we consume 0 symbols of the input string,
naturally"*).

The collision occurs only when a flag occupies a **matched** role — inside a `||` context, compiled
through a `NotContain` construction — where the builder treats it as a real symbol and apply treats
it as consuming nothing. A flag merely **inserted** by a rule and read at plain apply time never
enters that construction.

That explains why Divvun's 1,118 production flags do not contradict the matched-role finding, but
it does not prove that the unmodified `<-` relation is safe for PanGloss's production direction.
For `A <- B`, parsing keeps A on the upper tape and B on the lower tape; `apply_down` consumes A
and emits B. The minimal exact-shaped projection in
`rust/crates/pg-foma/tests/flag_replace_scope.rs` accepts ascending and rejects descending there.
The original relation's `apply_up` consumes B and emits A; its zero-width upper-only flags fail
open, with the exact descending output set {`+Der2+Der1`}. Applying `fsm_invert` produces the
inverse relation `B <- A`; its `apply_up` consumes A and emits B, and the exact
ascending/descending output sets plus the `apply_set_obey_flags(false)` causality control pass.
The precise safe/unsafe line is therefore both flag role and application direction, not merely
which side of the arrow the source text names.

All three of `gate.rs`'s original findings are **inherited from upstream C, not port regressions** —
For finding 2, the pinned foma-rs 0.4.2 source's `foma-0.4.2/src/mem.rs` explicitly says the C `g_*` option
globals moved to `crate::options::FomaOptions`; the actual default is
`flag_is_epsilon: false` at `foma-0.4.2/src/options.rs:83`, and `fsm_compose` consumes that option at
`foma-0.4.2/src/constructions/products.rs:214`. An earlier uncommitted `flag_twosided` observation is
not auditable because its exact construction, managed command, memory cap/units, peak
source/measurement, and failure phase are not committed; it was not rerun and supplies no
evidence here.

---

## 4. Accuracy: what is measured and what is not

Summarized here; the full argument with numbers is in
[why-not-just-use-divvun.md §4](why-not-just-use-divvun.md).

- `giella-core/scripts/run-morph-tester.sh.in:146` passes `--ignore-extra-analyses`
  **unconditionally, org-wide**, disabling the check that would catch over-generation in the
  analysis direction.
- The negative-assertion syntax (`~`-prefixed forms) appears in **0 of 1,572** YAML test files.
- `lang-sme/docs/docu-sme-testplan.md:138-153` states it plainly: *"When we test whether words are
  let through or not, we do not test whether the parser actually gives correct analyses."*
- The **generation** direction *is* exact-set tested and the flag does not relax it. Their strict
  bar is on the speller-facing direction.
- Their maturity classification names precision for spell checkers and grammar checkers at every
  tier, and **never** for the morphological analyzer at any tier.

**Verdict.** For the analyzer — the artifact that would replace `confirm` — this is tolerance by
design, documented as such, not a soundness proof. Their FST-only deployment is evidence that it
ships, not that it is sound. PanGloss's `confirm` is a strictly stronger contract than anything in
their pipeline.

### What they gave up, and how they remediate

Four sourced cases of "we wanted X, could not express it cleanly, did Y instead" in `lang-sme`
alone: the structural G1/G2/G3 gradation-class abstraction (attempted, documented as flawed three
times, abandoned for 44 enumerated special cases); a twolc downcasing rule (abandoned for compile
time, replaced by flags); the `šž` gradation alternation (pulled for over-generation, replaced by
hand-listed lexemes); and over-generation itself, named as an accepted design property with
downstream composed filters as the intended remedy.

The remediation workflow, when over-generation does hurt:

1. **Hand-tag the offending reading** with an `Err/*` tag so downstream consumers can route around
   it. `Err/Orth` alone appears **1,890 times** in the North Sámi lexc sources.
2. **Comment out the over-generating rule**, with an inline note. Nine instances in
   `adjectives.lexc`, each annotated "we overgenerate".
3. **Compose a restrictive filter on top of a deliberately loose base** — different analyzer
   variants (normative / dict / descriptive) deliberately carry different over-generation
   tolerances for different consumers.
4. **Corpus-scale regression diffing with a human in the loop** —
   `check_analysis_regressions.sh.in` diffs pipeline output against a committed goldstandard in a
   graphical difftool. This would surface new over-generation, but depends on a person accepting or
   rejecting each diff; it is not a pass/fail CI gate.
5. **Crowdsourced bug-mining** — `lang-sme#563`/`#447` are literally a native speaker reading
   corpus sentences through the grammar checker and reporting every spurious compound, one comment
   per sentence, over months.

Worth noting for contrast: their answer to "what about words not in the lexicon" is a **separate,
cheaper, explicitly approximate guesser FST** (`src/fst/guesser.xfscript`), not the real grammar run
permissively.

---

## 5. Filter tractability — what a cheap FST-side check can and cannot do

This section came from asking whether FST filters could replace `confirm` entirely. **We have since
settled on keeping HC**, so read it as a map of where a cheap proposer-side tightening is available
and where it provably is not — not as a replacement programme.

The criterion used: a filter is categorically simpler only if its size is independent of *N*
(lexicon entries), it determinizes without exponential blowup, and staging does not blow up either.

**The enabling fact, verified in our own source.**
`rust/crates/pg-featstruct/src/ops.rs:106-123` (re-verified 2026-07-31) shows `is_unifiable` is a
merge-walk checking each shared feature independently, with no cross-feature interaction at all.
That is the mathematical license for checking feature dimensions separately and intersecting them —
**n·k, not kⁿ** — which is what decided whether the unification-gate families were tractable.

**Proven simpler, with constructions:**

| Family | Route |
|---|---|
| `MorphemeCoOccurrence` | Direct — and the tags it needs are already on the tape |
| `BoundRoot` | Cheaper than a filter entirely: a compile-time topological fact. Just omit the bare-root arc. Zero new states |
| `StemName` rule-level gate | Static partition, \|F\|=0 |
| The four feature-gate families (`Mpr`, `HeadFeatures`, `ObligatoryFeatures`, `CompoundingFs`) | One shared run-flag / trigger / deferred-tail schema |

**Proven not simpler, and these do not look soluble:**

- Co-occurrence in `Anywhere` mode is a tight **2^k** Myhill-Nerode bound — achieved, not merely
  feared.
- The non-reachability-provable MPR `Overwrite` case is **4^k**.

**The calibration case, which is the uncomfortable one.** `Environment` is the only family we ever
actually built, and `rust/crates/pg-foma/src/precision.rs:188-198` (re-verified 2026-07-31) records
its growth as `entries × coverable_constraints`. It is not a filter that came out too big — it is
inline flag text baked into the proposer's own lexc entries, so there is no separate filter object
at all. **We never tested the criterion; we built something else.** That bounds how much weight the
other verdicts carry: 7 of the 9 families called dischargeable are predictions from a schema
shipped in only 2 cases.

**Two theory corrections worth keeping.** Mohri 1997's twins property guarantees determinization
*terminates*, not that it is polynomial — the original criterion asked for the wrong thing.
Polynomial is only guaranteed by never invoking subset construction at all: compile-time
predicates, or composing already-deterministic pieces. And a conservation law applies: a filter can
only reject on information present on the tape, so encoding derivation facts for the filter
*enlarges the proposer*. Both stages cannot be simplified at once.

**Governance gap found along the way:** `StemName` and `FreeFluctuation` have no `CharacteristicKind`
entry in `rust/crates/pg-foma/src/capability.rs` at all — a blind spot in the capability lattice,
which otherwise records a per-construct `Proven` / `ConfigPredicate` / `ConfirmOnly` / `FailClosed`
disposition (`capability.rs:83-102`, re-verified 2026-07-31).

---

## 6. Scale, for the record

- Finnish `lang-fin` is **1,156,174 lines of lexc** and ships — roughly 10x the FLEx-scale target of
  10⁴–10⁵ entries. Tractability at scale is settled.
- The bottleneck is the compose-against-rules step and **alphabet size**, not lexicon size, which
  matches what PanGloss measured on Amharic's 417-segment inventory.
- Their build strategy: whole-lexicon single `hfst-lexc` compile, then
  `determinize → minimize → hfst-compose-intersect → minimize` against the rules transducer, rather
  than free composition. This suggests our `Compose` primitive wants a restricted/intersecting mode.
- Greenlandic inverts the profile: its affix file is **127,966 lines** against ~47K of stems
  combined. In polysynthesis the derivational machinery dwarfs the lexicon.

---

## 7. Open questions

- **[unverified]** `gt_PROG_FOMA`'s macro body — is foma truly the fallback of last resort?
- **[unverified]** Whether any GiellaLT language has productive reduplication, and how it is
  encoded.
- **[unverified]** Whether `hfst-twolc`'s `Sets` / rule-variable expansion suffers the same alphabet
  blow-up PanGloss measured on Amharic.
- **[unverified]** The `HAVE_FOMA` / `--with-foma` status of the HFST build Divvun's production
  pipeline actually uses. If it is built `--without-foma`, the `FOMA_TYPE` conversion path is
  unavailable there regardless of what a from-source HFST build supports.
- No FST state/arc counts, compile times, or memory figures are published anywhere — only lexicon
  sizes and release-artifact bytes. **Our own measured numbers may be the better baseline.**
- Governance and licensing: UiT copyright, org-admin-gated repo creation, `gut` CLI, Buildkite CI
  auto-wiring; and no documented policy anywhere in GiellaLT for incorporating FieldWorks/FLEx-derived
  lexical data. That needs a human decision, not engineering.
