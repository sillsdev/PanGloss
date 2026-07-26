# Keyman lexical-model API — the ground truth D8 needed

Report 12 in the spell-checking research series. Scope: `PLAN.md` § D8 (decided 2026-07-25) named
Keyman as PanGloss's first integration target without ever reading the Keyman lexical-model API
itself — only its docs landing pages. Five things were flagged as depending on that reading: the
emit target, D9's tier-1 generative fit, D6's word-breaker consumer, D10's latency contract, and
whether a Rust/WASM module can load inside Keyman's model worker on a low-end Android device. This
report reads the actual interface — TypeScript type definitions, the compiler source, the worker
runtime, the correction-search engine — and answers all five.

Design-only. No code, no spikes. Every claim is graded: `[M]` = read directly from a primary
source (quoted or paraphrased from the actual file), `[A]` = abstract/snippet-level only, `[S]` =
my own synthesis or engineering judgment, shown as such.

**Read alongside**: `PLAN.md` § D8 (emit target), § D9 (tiered candidate supply), § D10 (latency
policy), § D6 (tokenization); `11-latency-policy.md` (adopted p90 single-stream, flagged
provisional pending this report); `10-rust-inference-and-ports.md` (WASM feasibility, format and
evidence-grading convention followed here); `03-keyboard-keyman.md` (keyboard-geometry mining —
does not overlap this report, which is about the plugin contract, not keyboard layouts);
`openspec/changes/make-wasm-analysis-only/proposal.md` and `rust/crates/pg-wasm/`;
`openspec/changes/import-writing-system-data/proposal.md` (D6's data source, the word breaker's
consumer).

---

## Sources — fetched vs. not

**Fetched and read directly**, via `gh api` against `raw.githubusercontent.com`-equivalent content
endpoints on `keymanapp/keyman` (not docs pages, the actual source), all on 2026-07-25: the type
contract `common/web/types/src/lexical-model-types.ts` (the `LexicalModel`, `Context`,
`Capabilities`, `Suggestion`, `LexiconTraversal`, `WordBreakingFunction` interfaces, in full); the
worker runtime `web/src/engine/predictive-text/worker-thread/src/main/index.ts` (`LMLayerWorker`,
model loading state machine), `model-compositor.ts` (`ModelCompositor.predict`,
`predictionAutoSelect`), `predict-helpers.ts` (`correctAndEnumerate`, `predictFromCorrections`,
`shouldStopSearchingEarly`), `correction/execution-timer.ts` and `correction/distance-modeler.ts`
(the `ExecutionTimer`/`SearchSpace` timing machinery, including the `33`ms constant), `models/
dummy-model.ts`; the compiler `developer/src/kmc-model/src/lexical-model.ts` (`LexicalModelSource`,
the `'trie-1.0'|'fst-foma-1.0'|'custom-1.0'` format union) and `lexical-model-compiler.ts` (the
`generateLexicalModelCode` switch that proves what each format actually does, in full); the
reference implementation `web/src/engine/predictive-text/templates/src/trie-model.ts` (`TrieModel`,
`Traversal`, `Trie`); the real `custom-1.0` test fixture
`developer/src/kmc-model/test/fixtures/example.qaa.custom/{example.qaa.custom.model.ts,
ExampleCustomModel.ts}`; the word breaker `web/src/engine/predictive-text/wordbreakers/{README.md,
src/main/default/index.ts}`; `web/src/engine/src/interfaces/prediction/languageProcessor.interface.ts`
and `web/src/engine/src/main/headless/languageProcessor.ts` (`LanguageProcessor`, the
`mayPredict`/`mayCorrect`/`mayAutoCorrect` triad, the `Capabilities` defaults including
`maxLeftContextCodePoints: 64`); `web/README.md` (the dependency-graph diagram proving Android/iOS
embed the identical worker code via a `webview` build target); `web/docs/internal/
keystroke-processing.md` and `web/src/engine/predictive-text/worker-main/docs/
worker-communication-protocol.md` (cadence: "Keyman should ask for an asynchronous prediction on
most key presses" `[M]`); `android/KMEA/app/src/main/assets/android-host.js` (confirms a
`window.jsInterface` WebView-bridge pattern, i.e. Android hosts the web engine rather than
reimplementing it); GitHub PR `keymanapp/keyman#15778` (`gh api repos/keymanapp/keyman/pulls/15778`,
full body) and its issue-search confirmation; the SIL community forum thread
`community.software.sil.org/t/dictionary-for-laz/11258/8` (via WebFetch); the Keyman blog post
`blog.keyman.com/2026/03/creating-an-advanced-custom-lexical-model-with-keyman/` (via WebFetch, with
a follow-up fetch specifically requesting verbatim quotes); `help.keyman.com/developer/
current-version/guides/lexical-models/intro/index` (via WebFetch, verbatim quotes) and
`.../advanced/index.md`, `.../advanced/model-definition-file.md` (via `gh api` against the docs
source in-repo, not the rendered page); `help.keyman.com/products/android/current-version/about/
system-requirements` (via WebFetch, verbatim: Android 5.0 minimum, Chrome/WebView **53.0** minimum)
and the iOS equivalent (12.1 minimum, via WebSearch synthesis, not independently re-fetched — see
below).

**Fetched but discrepant, flagged rather than silently reconciled**: a `WebSearch` synthesis
separately reported the Android minimum as Chrome **37.0** where a direct `WebFetch` of the live
system-requirements page returned **53.0**. The direct fetch is treated as authoritative (`[M]`);
the `37.0` figure is not used as a citable number, only noted as a discrepancy, because I could not
determine whether it reflects an older doc revision cached by the search index or a
misattribution.

**Rejected outright, not cited even as `[A]`**: one `WebSearch` synthesis, in response to a query
combining "custom-1.0" with "WebAssembly," produced the sentence "these are Keyman lexical models
that compile to WebAssembly format" with no supporting source. This directly contradicts the
compiler source read in full (`lexical-model-compiler.ts`'s `generateLexicalModelCode`, which emits
plain ES3 JavaScript, never WASM, for every implemented format) and is dropped entirely — the same
"don't cite what you can't verify against a primary source" discipline reports 10 and 11 applied to
similarly unverifiable search-engine claims.

**Not attempted**: driving Keyman Core's native C API directly (report 03 already established it
is undocumented for this purpose); the Delphi/Pascal desktop-side lexical model tooling
(`developer/src/tike/...`, `developer/src/common/delphi/lexicalmodels/...`) — out of scope, since
D8's target is mobile/web, not Windows desktop Keyman Developer's own UI; the `.kvks`/OSK/touch
layout formats (report 03's territory, correctly not duplicated here); load-testing or actually
running a `custom-1.0` model with an embedded WASM payload — this report is design-only per its
mandate, and the finding that this is *architecturally possible* is established from source reading
alone, not from a spike.

---

## Answer to the load-bearing question first: can a custom model be more than a wordlist?

**Yes — and Keyman's own compiler already reserves a name for something even closer to PanGloss's
shape than what it currently ships.**

### The interface itself has no wordlist shape at all

`common/web/types/src/lexical-model-types.ts` `[M]`, read in full, defines `LexicalModel` as a
plain TypeScript interface:

```typescript
export interface LexicalModel {
  configure(capabilities: Capabilities): Configuration;
  readonly languageUsesCasing?: boolean;
  applyCasing?(form: CasingForm, text: string): string
  toKey?(text: string): string;
  predict(transform: Transform, context: Context): Distribution<Suggestion>;
  readonly punctuation?: LexicalModelPunctuation;
  wordbreaker?: WordBreakingFunction;
  traverseFromRoot?(): LexiconTraversal;
}
```

The only *required* members are `configure()` and `predict()`. `predict()`'s contract is "given a
keystroke transform and the surrounding context, return a probability distribution over
suggestions" — nothing in the type system constrains what happens inside that function. There is no
`lexicon: TextWithProbability[]` field, no trie-shaped storage requirement, nothing that
presupposes a wordlist. `traverseFromRoot()` (returning a `LexiconTraversal`, described below) is
explicitly **optional** — the interface documents it as something models "*may* provide... to
enhance prediction and correction results" `[M]`, not something they must.

### The compiler declares three formats; the wordlist path is only one of them

`developer/src/kmc-model/src/lexical-model.ts` `[M]`:

```typescript
export interface LexicalModelDeclaration {
  readonly format: 'trie-1.0'|'fst-foma-1.0'|'custom-1.0',
  ...
}
```

`developer/src/kmc-model/src/lexical-model-compiler.ts`'s `generateLexicalModelCode` `[M]`, read in
full, is the ground truth for what each format actually compiles to — not a docs paraphrase:

| Format | What the compiler does with it |
|---|---|
| `trie-1.0` | Builds a `TrieModel` (word-list-and-frequency trie) from TSV sources — the documented tutorial path, and the only one `help.keyman.com`'s public guides describe in depth. |
| `custom-1.0` | Concatenates the model's own TypeScript/JavaScript `sources` files (transpiled per-file via `ts.transpileModule`, ES3 target) and emits `LMLayerWorker.loadModel(new ${rootClass}());` — **instantiates an arbitrary class and hands it directly to the worker.** |
| `fst-foma-1.0` | `throw new ModelCompilerError(ModelCompilerMessages.Error_UnimplementedModelFormat(...))` — **declared in the type union, reserved as a name, never implemented.** |

The `custom-1.0` path is real and working, confirmed against an actual shipped test fixture,
`developer/src/kmc-model/test/fixtures/example.qaa.custom/ExampleCustomModel.ts` `[M]`:

```typescript
export class ExampleCustomModel implements LexicalModelTypes.LexicalModel {
  configure(capabilities): LexicalModelTypes.Configuration { ... }
  predict(transform, context): LexicalModelTypes.Distribution<LexicalModelTypes.Suggestion> {
    if (transform.deleteLeft == 0 && context.left.endsWith('te') && transform.insert == 'h') {
      return [ /* hand-computed suggestions */ ];
    }
    return [];
  }
}
```

`predict()` here is ordinary, arbitrary TypeScript — an `if` statement over the transform and
context, not a lookup. There is nothing wordlist-shaped about it at all.

### Keyman's own team names PanGloss's exact use case as the reason `custom-1.0` exists

A Keyman blog post from four months before this report, `blog.keyman.com/2026/03/creating-an-
advanced-custom-lexical-model-with-keyman/` `[M]`, verbatim (re-fetched specifically to get exact
quotes, not paraphrase):

> "The standard lexical models supported in Keyman are currently wordlist-based. This works well
> for many languages, but for polysynthetic languages or those with complex morphologies, it is not
> practical to list all possible word forms."
>
> "For these languages, it makes sense to embed grammar knowledge in to the lexical model and
> reduce the wordlist dramatically."
>
> "The `ExampleCustomModel` class must implement at least `configure` and `predict` functions, and
> can also optionally implement as much of the `LexicalModel` interface... as makes sense for your
> use case."

This is not PanGloss's own framing read back into Keyman's docs — it is Keyman's team independently
naming "polysynthetic languages" and "complex morphologies" as *the reason the escape hatch exists*,
months before this report was commissioned, with no knowledge of PanGloss. Corroborated by a real
deployment conversation: `community.software.sil.org/t/dictionary-for-laz/11258/8` `[A]`, a SIL
forum thread where a developer proposes exactly "leverage existing verb conjugation logic" for Laz
(a Kartvelian language with complex verb morphology), and a Keyman team member (Marc) responds with
implementation guidance and the caution "I don't want to underestimate the complexity of
implementing this – conceptually the idea is straightforward but there are many complex edges."
Feasible and endorsed; not trivial. Both things are true at once, and the report should not
flatten that.

### The real ceiling, stated precisely

The contract is: **a class satisfying (at minimum) `configure()`+`predict()`, doing whatever
computation it wants inside `predict()`, synchronously, per call.** That "synchronously" is a real
constraint, not a hand-wave — see § Execution environment below. Subject to that, there is no
wordlist/trie requirement anywhere in the interface, the compiler, or a real shipped example.
**D9's tier-1 generative approach — producing unseen inflected forms from the grammar — fits inside
`predict()` without needing anything from Keyman that doesn't already exist.** D8 does not need
revisiting on this axis.

### Two qualifications that temper, but do not reverse, that conclusion

1. **`fst-foma-1.0` is a name, not a feature.** Keyman's own architects anticipated an FST-based
   model format — literally named after the same `foma` toolchain PanGloss's proposer uses — and
   reserved it in the type system, then never built it. This is worth surfacing to Keyman directly
   as a partnership conversation (parallel to the Divvun conversation D8 already keeps open, but
   pointed at Keyman instead): PanGloss could be the reason that format finally gets specified,
   rather than something PanGloss builds against unilaterally. It is not something to wait for or
   depend on — `custom-1.0` already does the job.
2. **`custom-1.0` is young, and Keyman's own team says so.** PR `keymanapp/keyman#15778` `[M]`,
   read in full, merged **2026-03-24** — four months before this report — fixing a `globalThis`
   reference bug in the compiled boilerplate for custom models. Its own description: *"The
   boilerplate code for custom lexical models has never really been tested."* This is not
   disqualifying (the fix landed, the fixture now has a unit test, the mechanism works as read),
   but it means PanGloss would be among the first serious real-world loads on this path, and should
   budget for finding Keyman-side bugs rather than assuming a mature, battle-tested surface.

---

## The execution environment

### It is a Web Worker, and the same one on every platform

The `LMLayer` (`@keymanapp/lexical-model-layer`, `worker-main/`) and its worker
(`@keymanapp/lm-worker`, `worker-thread/`) run inside a dedicated Web Worker — `web/README.md`'s own
architecture diagram `[M]` labels this subgraph literally `"PredText: WebWorker + its interface"`.

Critically, **this is not web-only infrastructure that mobile reimplements** — `web/README.md` `[M]`
documents the build output tree:

```text
build/app/browser/release  Fully-compiled KeymanWeb modules for release
build/app/webview/release  Fully-compiled KMEA/KMEI modules for inclusion in mobile app builds
```

and its dependency graph shows `WebView["/web/src/app/webview"] --> CommonEngine`, where
`CommonEngine` sits directly above the same `PredText` (LMLayer+worker) subgraph the browser build
uses. **KMEA (Keyman for Android) and KMEI (Keyman for iPhone/iPad) embed the identical KeymanWeb
JS engine, including its Worker-hosted predictive-text layer, inside a platform WebView** — they do
not reimplement prediction natively. This is corroborated structurally by
`android/KMEA/app/src/main/assets/android-host.js` `[M]`, which begins with a `window.jsInterface`
WebView-bridge pattern (the standard Android `addJavascriptInterface` idiom), and by the parallel
existence of `ios/engine/KMEI/KeymanEngine/resources/Keyman.bundle/Contents/Resources/ios-host.js`.
Android's own developer docs independently use the term "LMLayer" for the mobile API surface
(`android/docs/engine/KMManager/deregisterLexicalModel.md` `[M]`: "deregisters the specified
lexical model from the LMLayer"), which would be an odd word choice if Android had its own,
differently-named prediction engine.

**Consequence: whatever runs inside a `predict()` call on desktop web runs, unmodified, inside the
mobile apps too**, subject only to the WebView's own JS/WASM engine version (see § Platform-version
risk, below) — there is no separate mobile contract to design against.

### A WASM module can load there, but not the way `wasm-bindgen`'s default boilerplate assumes

Two structural facts, both read directly from the model-loading code path
(`worker-thread/src/main/index.ts`'s `LMLayerWorker`, `[M]`, read in full):

1. **Model construction is synchronous.** A model is loaded either via `importScripts(url)`
   (blocking) or by `eval`-ing supplied code through
   `new Function('LMLayerWorker', 'models', 'correction', 'wordBreakers', code)`. Both execute the
   model's top-level code — including, for `custom-1.0`, the compiler-emitted
   `LMLayerWorker.loadModel(new ${rootClass}());` call — in one synchronous turn. Nothing in this
   path `await`s anything before transitioning to the `'ready'` state and casting the `ready`
   message back to the host.
2. **`predict()` is called synchronously, not awaited**, even though the outer
   `ModelCompositor.predict()` is itself `async` (for its own correction-search loop —
   see § Latency). `predict-helpers.ts`'s `predictFromCorrections` `[M]`: `let predictions =
   lexicalModel.predict(correction.sample, context);` — no `await`, and the result is used
   immediately as a plain array.

**Implication, `[S]` (my own engineering read, not documented by Keyman):** a WASM-backed
`custom-1.0` model cannot use `wasm-bindgen`'s default async `init()` boilerplate (`--target web`)
inside the constructor, because there is no point in the loading sequence where the LMLayer waits
for a promise before calling `predict()`. The two realistic paths are: (a) instantiate the WASM
module **synchronously** — `wasm-bindgen` has shipped a synchronous `initSync()` entry point
precisely for embeddings like this one, fed either bytes bundled directly into the compiled
`.model.js` (as a base64 literal) or fetched via a synchronous `XMLHttpRequest` inside the Worker
(permitted for workers, unlike the main thread); or (b) begin async instantiation eagerly and have
`predict()` degrade gracefully (e.g. return `[]`, or fall back to a cheap non-WASM tier) until the
module reports ready. Path (a) is simpler and matches what `pg-wasm`'s existing wasm-bindgen
surface (`rust/crates/pg-wasm/src/lib.rs`) already looks like — every exported method there
(`analyzeWord`, `analyzeText`, etc.) is already a synchronous `wasm-bindgen` call with no `Promise`
in its signature, so the *calling convention* PanGloss already has is exactly the one this
environment needs; only the one-time module-instantiation step would need the synchronous variant.
No WASM module size limit was found anywhere searched (the `.keyboard_info` schema, the packaging
docs, the compiler source) — a genuine negative result, not an oversight.

### Platform-version risk: a real, previously-unstated finding

Keyman for Android's documented minimum, from a direct fetch of the live system-requirements page
`[M]`: *"Keyman for Android will run on Android phones and tablets that have a minimum version of
Android 5.0 (Lollipop)... Keyman for Android requires a minimum version 53.0 of Google Chrome."*
Chrome shipped WebAssembly support in v57 (March 2017); **Keyman's documented Android floor (Chrome
53) predates that by several months.** iOS's documented minimum (12.1, per `[A]` WebSearch
synthesis, not independently re-fetched) postdates Safari/WKWebView's WASM support (iOS 11, 2017)
comfortably, so iOS carries no equivalent risk.

This does not mean WASM cannot ship — Android's WebView component auto-updates independently of the
OS version on any device with active Play Store access, so the overwhelming majority of real Android
5.0+ devices in the field run a WebView far newer than Chrome 53 `[S]`, the same practical caveat
Google's own WebView-update documentation makes generally. But a device frozen at the literal
documented floor (no Play Store access, unmaintained) genuinely could not run a WASM-backed model,
and Keyman's docs do not distinguish this. This is a concrete, new input to D10's still-open
"name the reference low-end device" item — report 11 found no literature method for choosing one;
this report adds that whatever device is chosen should be checked for actual (not nominal) WebView
version, and the certification story should say explicitly whether "Android 5.0 minimum" devices
with a stale WebView are in or out of scope for the WASM-backed tier.

---

## The latency contract

### Keyman does impose a number — but it binds a narrower thing than "the whole `predict()` call"

`correction/distance-modeler.ts` `[M]`: `static readonly DEFAULT_ALLOTTED_CORRECTION_TIME_INTERVAL
= 33; // in milliseconds.` This constructs an `ExecutionTimer(SEARCH_TIMEOUT, SEARCH_TIMEOUT * 1.5)`
in `ModelCompositor.predict()` — a **33ms soft budget, 49.5ms hard ceiling**, per prediction request.
The search this bounds is a genuine best-first anytime enumeration: `predict-helpers.ts`'s
`correctAndEnumerate` `[M]` runs `for await (let match of searchSpace.getBestMatches(timer)) { ...
if (shouldStopSearchingEarly(...)) break; }`, yielding to the event loop periodically
(`STANDARD_TIME_BETWEEN_DEFERS = 5`ms) and checking `timer.elapsed` to decide whether to keep
enumerating. This is independently, natively an anytime/interruptible search in exactly the
technical sense report 11 found for PanGloss's own tier design — except this one is Keyman's, not
PanGloss's, and it already exists in production.

**The precise scope, established by reading what the timer actually wraps** (this is the finding
that matters most for D10, and it is easy to get wrong by skimming): the 33/49.5ms budget governs
**Keyman's own enumeration over a model's `LexiconTraversal`** — the fat-finger/typo-tolerant
correction search that only runs when a model implements the optional `traverseFromRoot()` method.
It does **not** individually time-box each call to `lexicalModel.predict()`. Concretely:

- If a model does **not** implement `traverseFromRoot()`, `ModelCompositor`'s constructor never
  builds a `ContextTracker`, and `correctAndEnumerate`'s `if (!contextTracker)` branch calls
  `predictFromCorrections` **once**, directly, with **no use of the timer at all** — it is
  constructed but never consulted in this branch. A slow `predict()` call here has **no
  host-enforced cutoff of any kind.**
- If a model **does** implement `traverseFromRoot()`, the `for await` loop drives Keyman's own
  trie-walk (`children()`/`entries()` on the model's `LexiconTraversal`) under the 33/49.5ms
  ceiling, and calls `predictFromCorrections` (which in turn calls the model's `predict()`) once
  per surviving correction candidate the walk discovers. The **enumeration** is bounded; each
  individual `predict()` call invoked along the way is still, itself, uninstrumented.

**Consequence for D9/D10, stated plainly: the host-enforced timeout is opt-in, and it is opt-in
through a specific interface member, not a blanket contract.** A `custom-1.0` model that does all of
its tier-1/tier-2 generative work inside a single `predict()` call, without implementing
`traverseFromRoot()`, has no host-side recovery if that call runs long — it would stall the Worker
(and therefore the keyboard's whole predictive-text pipeline) for that keystroke, with nothing in
Keyman's own code stopping it. This is a materially different, and more concerning, finding than
"Keyman doesn't specify a budget" (report 11's literature-search framing) — **Keyman does specify a
budget, but only for callers who opt into a specific extension point.**

### The constructive reading: `traverseFromRoot()` is a good structural fit for the tier design

`LexiconTraversal` (`lexical-model-types.ts` `[M]`) is a lazy, generator-based interface —
`children(): Generator<{char, traversal}>`, `child(char): LexiconTraversal|undefined`, `entries:
TextWithProbability[]`, `p: number` (max weight under this node) — designed for exactly the
"explore promising branches first, stop when the budget runs out" access pattern D9's tiers already
describe. **Recommendation, `[S]`:** route PanGloss's tiered candidate generator through
`traverseFromRoot()` (wrapping the FST-driven generation as a lazy trie-shaped walk) rather than
computing everything inside one `predict()` call. Doing so is what makes Keyman's own 33ms/49.5ms
ceiling actually apply to PanGloss's tier-1/tier-2 work — it is the concrete mechanism by which
D9's anytime design gets a **host-enforced**, not merely self-imposed, backstop. This is an
implementation recommendation for D9 to absorb, not a re-opening of the tier design itself.

### Cadence: every keystroke, confirmed, no debounce found

`web/src/engine/predictive-text/worker-main/docs/worker-communication-protocol.md` `[M]`: "Keyman
should ask for an asynchronous prediction on **most key presses**." `web/docs/internal/
keystroke-processing.md` `[M]` independently describes `InputProcessor` as "the connection point
for generating prediction requests" for "**all** keystroke processing variants." A targeted search
of the `LanguageProcessor` and prediction-context source for `debounce`/`throttle` found nothing —
a genuine negative result, not an omission. This directly closes D10's "does tier 1 run on every
keystroke or only on a thin cache?" open question **at the host boundary**: Keyman calls `predict()`
on (near-)every keystroke, gated only by an on/off flag (`mayPredict`), never a rate limit. Any
throttling of expensive internal tiers must happen inside the model — invisible to, and not imposed
by, Keyman.

### One more concrete, previously-unknown number: the context window is 64 code points

`web/src/engine/src/main/headless/languageProcessor.ts` `[M]`:

```typescript
// Establishes KMW's platform 'capabilities', which limit the range of context a LMLayer
// model may expect.
const capabilities: Capabilities = {
  maxLeftContextCodePoints: 64,
  maxRightContextCodePoints: supportsRightDeletions ? 0 : 64
}
```

This is the default across the browser/webview hosts (a model's `configure()` can request less, but
not more). **D4's inter-word class trigram needs the classes of roughly two preceding words** —
64 code points is generous for most orthographies but is a real, previously unquantified ceiling
worth carrying into calibration, especially for languages with long average word length (the
polysynthetic case D9 exists for). Also note `maxRightContextCodePoints` defaults to **0** unless
the host explicitly supports right-deletions — most Keyman host platforms give a model *no* text to
the right of the cursor at all by default. This is a real constraint on any future correction pass
that would want lookahead context, though it does not affect D9/D10 as currently scoped (which are
both forward/left-context designs).

### What this means for report 11's p90-single-stream adoption

Report 11 adopted p90 single-stream "by explicit analogy to MLPerf Mobile," explicitly flagged as
"a convention we are choosing to align with, not... proven optimal." Keyman's contract does **not**
hand PanGloss a stricter or different percentile methodology — it hands PanGloss something
narrower and more concrete: a **raw millisecond ceiling on one specific, opt-in code path** (the
`traverseFromRoot()`-driven correction search). These are not competing answers to the same
question; they answer different questions. **Recommendation:** keep the p90-single-stream framing
as the discipline for measuring and calibrating PanGloss's own tiers end-to-end (this is still
entirely PanGloss's own measurement to make, per D10), and *additionally* adopt Keyman's
33ms/49.5ms figures as a hard design target specifically for whatever portion of the tiered
generator is exposed through `traverseFromRoot()` — because that portion, and only that portion, is
a value Keyman will actually enforce rather than one PanGloss merely aspires to.

---

## The word breaker

### The interface is exactly as narrow as it needs to be, and D6's data fits it directly

`WordBreakingFunction` (`lexical-model-types.ts` `[M]`): `(phrase: string): Span[]` — a pure
function from a full string to an ordered, non-overlapping, phrase-covering array of `Span {start,
end, length, text}`. Nothing about Unicode UAX #29, nothing about a particular segmentation
algorithm — any function with that signature is legal, whether supplied as `model.wordbreaker`
directly on a `custom-1.0` class or as `LexicalModelSource.wordBreaker` in the declarative
`trie-1.0` path.

The bundled default (`@keymanapp/models-wordbreakers`, `wordbreakers/src/main/default/index.ts`
`[M]`) implements UAX #29's default word-boundary rules over a Unicode `Word_Break` property table,
but exposes exactly the extension points a per-writing-system word-forming set needs, without
requiring a full rewrite:

```typescript
export interface DefaultWordBreakerOptions {
  rules?: WordbreakerRule[];              // custom rules layered after WB1-WB4
  propertyMapping?(char: string): string; // reassign a character's word-break property
  customProperties?: string[];            // define new properties for use with custom rules
}
```

`openspec/changes/import-writing-system-data/proposal.md` `[M]` defines exactly what D6 will
supply: "word-forming vs. non-word-forming character classification (from `exemplarCharacters`,
generalized)" per writing system, sourced from a FieldWorks project's own `.ldml` sidecar files.
This maps onto Keyman's contract two ways, both real and supported:

1. **Lightweight**: a `propertyMapping` function that maps "in the word-forming set" to an existing
   UAX #29 category (e.g. `ALetter`) and everything else to `Other`, riding on Keyman's existing
   rule machinery for free.
2. **Wholesale**: a custom `WordBreakingFunction` that walks the string using the word-forming set
   directly, bypassing UAX #29 entirely — closer to what a grammar-derived, per-writing-system
   word-forming set actually models (orthographic word-hood for *this* language, not general-purpose
   Unicode text segmentation).

**No shape mismatch was found.** This is a small, well-scoped adapter task once
`import-writing-system-data` lands — the consumer contract is already settled by Keyman, and it was
settled before this report, not by this report; what this report adds is the confirmation that it
fits without any redesign on either side.

---

## Autocorrect vs. prediction: one entry point, three independent host policy dials

**One entry point**, confirmed at the type level and the runtime level. `LexicalModel.predict()` is
the sole method that produces suggestions. `Suggestion.tag` (`undefined` for a plain prediction,
`'keep'`, `'correction'`, `'emoji'`, `'revert'`) is metadata *on individual members* of the single
`Distribution<Suggestion>` `predict()` returns — never a separate call, separate message, or
separate model method.

"Autocorrect" is a **post-processing step downstream of that one call**, inside `ModelCompositor`
(i.e. still in the Worker, still downstream of whatever model is loaded) — `predict-helpers.ts`'s
`predictionAutoSelect` `[M]`, read in full: it marks at most one suggestion per `predict()` call
with `autoAccept = true`, gated by `AUTOSELECT_PROPORTION_THRESHOLD = 0.66` (the best correction
candidate's probability mass must be ≥66% of same-tier alternatives' summed mass), plus explicit
guards — never auto-correct away from a valid "keep" match, never auto-correct from an empty-root
correction, a lone remaining candidate auto-accepts unconditionally.

Layered on top of *that*, the host exposes **three independent boolean policy dials**
(`LanguageProcessor`, `[M]`, read in full): `mayPredict`, `mayCorrect` (gates whether the
fat-finger/typo-tolerant `alternates` distribution is even generated and passed into the correction
search — "If corrections are not enabled, bypass the correction search... just do a direct lookup"
`[M]`), and `mayAutoCorrect` (defaults to `false`, gates whether the host actually *applies* an
`autoAccept`-flagged suggestion automatically versus merely displaying it in the suggestion strip
like every other candidate).

**This precisely validates John's framing, more exactly than the framing itself states it**: it is
not merely that prediction and correction are "the same engine looking at different parts of the
document" — they are, structurally, **the same single call, the same single output list**, split
into presentation modes by (a) a per-suggestion tag computed by the model or the compositor, and
(b) three host-level toggles that decide which parts of that one output get surfaced automatically
versus shown as choices. There is no separate Keyman API surface for "autocorrect" at all.

---

## What this changes in D6, D8, D9 and D10

### Keyman's contract settles this

- **D6** — the word-breaker consumer shape is settled: `(phrase: string) => Span[]`, with a
  documented, real extension point (`propertyMapping`, or a wholesale custom function) that D6's
  per-writing-system word-forming set slots into without redesign on either side. What remains open
  is unchanged and is D6's own, not Keyman's: which of the two integration shapes to use, and
  actually building the LDML-exemplar extraction pipeline (`import-writing-system-data`) that
  supplies the data in the first place.
- **D8** — confirmed, not merely left standing. The single biggest latent risk to D8 — that
  Keyman might turn out to be wordlist-only, which would have invalidated the whole pivot away from
  Divvun — does not obtain. `custom-1.0` permits arbitrary code, is real (not aspirational), and
  Keyman's own team names PanGloss's exact use case (grammar-generated forms for morphologically
  complex languages) as its reason for existing. Two things D8 should absorb as stated risks rather
  than blockers: `custom-1.0`'s documented immaturity (untested boilerplate until a March 2026 fix),
  and `fst-foma-1.0` as a live, un-taken opportunity for a Keyman partnership conversation — a
  reserved name, not a built feature, and not something to depend on.
- **D9** — the tiered/anytime design is unchanged, but Keyman's `LexiconTraversal` interface gives
  it a concrete, host-enforced backstop it did not have before: implementing `traverseFromRoot()`
  over the tiered generator (rather than doing all generation inside one `predict()` call) is what
  makes Keyman's own correction-search timer bound PanGloss's tier-1/tier-2 wall-clock cost. This is
  a new implementation recommendation for D9, not a re-litigation of D9 itself.
- **D10 — cadence.** Settled: predict() runs on (near-)every keystroke, no debounce, confirmed by
  the protocol doc's own words and by an empty search for throttling logic. D10's "does tier 1 run
  on every keystroke" open question is closed at the host boundary.

### Still ours to decide

- **D10 — the latency budget itself.** Keyman does not universally override report 11's p90
  adoption, but it does hand PanGloss a concrete, host-*enforced* number (33ms soft / 49.5ms hard)
  for the specific code path that opts into `traverseFromRoot()`. Whether PanGloss's tiers route
  through that path (gaining the enforcement, but constrained to Keyman's search-loop shape) or stay
  inside a plain `predict()` call (unconstrained by Keyman, but with zero host-side recovery from a
  slow implementation) is a real design choice this report surfaces but does not make. The p90
  framing remains the only available discipline for whatever, if anything, ends up outside
  `traverseFromRoot()`.
- **D10 — the reference device.** Sharpened, not settled: Android's documented minimum WebView
  (Chrome 53, per direct fetch — one inconsistent secondary source said 37) predates WebAssembly
  support; iOS's does not. Real devices mostly auto-update past this floor, but a genuinely frozen
  device at the documented minimum could not run a WASM-backed model. Whatever reference device(s)
  D10 eventually names should be checked against actual, not nominal, WebView version, and the
  certification story should state explicitly whether stale-WebView Android 5.0 devices are in or
  out of scope.
- **D9/D10 — the context window.** New, concrete number: 64 code points of left context by default,
  0 code points of right context on most host platforms. Worth carrying into calibration for
  languages with long average word length, though it does not force any redesign of D9 or D10 as
  currently written.
- **The WASM instantiation strategy.** Established as *architecturally possible* (a Worker has no
  documented WASM size limit, and both mobile platforms' WebViews are new enough), but the specific
  mechanism — synchronous `initSync()`-style instantiation feeding bytes bundled into the compiled
  `.model.js` or fetched via synchronous XHR — is an engineering decision for whoever builds the
  `.model.ts` wrapper around `pg-wasm`, not something Keyman's docs specify. `pg-wasm`'s existing
  all-synchronous call surface (`analyzeWord`, `analyzeText`, etc., `rust/crates/pg-wasm/src/
  lib.rs`) is already the right shape for `predict()`'s synchronous-call requirement; only the
  one-time module-load step needs attention.

### What could not be verified

The iOS minimum-version figure (12.1) rests on a `WebSearch` synthesis, not an independently
re-fetched primary page — low risk (it is consistent with well-established public knowledge about
WKWebView's WASM support timeline, and does not drive any conclusion in this report on its own), but
flagged per this series' convention rather than silently upgraded to `[M]`. The Android
Chrome-minimum discrepancy (53 vs. 37 across two fetch methods) was not resolved to a single
authoritative historical figure — the live page's 53.0 is used, but this report cannot rule out that
older installed Keyman versions in the field were built against a genuinely lower floor. No attempt
was made to actually build or run a `custom-1.0` model with an embedded WASM payload — the
conclusion that this works is a read of the loading code path, not a demonstrated spike, consistent
with this report's design-only mandate but worth stating as the boundary of what "yes, it works" means
here.
