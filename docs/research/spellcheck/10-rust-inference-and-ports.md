# Rust inference stacks, WASM feasibility, and the port inventory for a tag-vocabulary mini-transformer reranker

Report 10 in the spell-checking research series. Scope: whether a small transformer used purely
as a **reranker** over morphological-analysis candidates (input = morpheme/tag/feature token
sequences, not characters or subwords) is buildable in Rust and deployable inside PanGloss's
bounded WASM inference envelope, using today's ecosystem — and, following the standing project
philosophy (`00-synthesis.md`, "Build philosophy"), precisely what must be ported if it isn't.
This report builds on `04-ngram-factored.md` (factored-LM/CG prior art) and goes materially
deeper than the "Rust/WASM feasibility" pass at the end of `05-gaps-and-transformers.md`,
verifying every claim made there against primary sources.

Design-only. No code, no spikes, no benchmarks run by this report — every number below is either
read from a primary source (crate docs, repo source, a benchmark instrument's own published
output) or is an arithmetic derivation shown in full and marked `[S]`.

**Read alongside**: `00-synthesis.md`, `04-ngram-factored.md`, `05-gaps-and-transformers.md`.

---

## Sources — fetched vs. not

**Fetched and read directly** (repo READMEs/source, crates.io API, docs.rs, GitHub API/issues,
WebAssembly proposal repos, HF Spaces benchmark discussion, project pages): `github.com/huggingface/candle`
(README + `candle-wasm-examples` listing), `github.com/tracel-ai/burn` (README + `burn-wgpu`),
`github.com/sonos/tract` (README), `github.com/pykeio/ort` (README + issues #195/#206/#260/#363/#450
via `gh api`), `crates.io` API JSON for `candle-core`, `burn`, `tract-onnx`, `ort`, `safetensors`,
`cg3`; `docs.rs/cg3`; `github.com/divvun/cg3-rs` (README, `Cargo.toml`, commit history) via `gh api`
and raw-content fetch; `github.com/divvun/divvunspell` `Cargo.toml`; GitHub repo-stats API
(`stargazers_count`/`open_issues_count`/`pushed_at`/contributor pagination) for candle, burn, tract,
ort, cg3-rs; `huggingface.co/openai-community/gpt2/raw/main/config.json`; the Xenova
`webgpu-embedding-benchmark` HF Space discussion (concrete WASM-vs-WebGPU latency table);
`radu-matei.com` WASM linear-memory article; `caniuse.com` WASM SIMD/threads support (via search
synthesis of the live table); `web.dev` COOP/COEP articles; `ort.pyke.io` backend docs (partial —
see below); `explainx.ai`/`github.com/soycaporal/ternlight` (a real, independently-verifiable npm
package + GitHub repo for a shipped Rust/WASM-SIMD sentence encoder — cross-checked against its own
npm listing, not taken from the blog alone); `arxiv.org/html/2505.06461v1` (on-device CPU-vs-GPU LLM
inference paper, HTML-rendered so fully extractable).

**Attempted, could not fetch (`[UNFETCHED]`)**: `crates.io/crates/*` HTML pages (JS-rendered SPA
shell only — worked around via the `crates.io/api/v1/crates/*` JSON endpoint instead, which is a
legitimate primary source, not a workaround-into-a-secondary-source); `ort.pyke.io/backends/web` and
`ort.pyke.io/setup/webassembly` (403 on direct fetch — status reconstructed instead from `pykeio/ort`
GitHub issues/PRs, which quote the same setup instructions verbatim in bug reports, and is treated
as `[A]`, not `[M]`, for anything not independently re-derivable from the issue text itself);
`tom-spink.com/papers/iiswc22leaps.pdf` (the "Leaps and Bounds" WASM bounds-checking paper — returned
raw PDF binary, not extractable; the specific overhead percentages below are therefore `[A]`, sourced
from a WebSearch synthesis of the paper's own indexed abstract/citations, not from text I read myself
— flagged explicitly at point of use); `sitepoint.com/webgpu-vs-webasm-transformers-js` (403 — the
"8–12ms on M2" figure attributed to this source elsewhere could not be independently confirmed and is
**dropped** from this report rather than cited unverified; the HF Space discussion numbers below are
used instead since they were fetched directly).

---

## 1. Rust inference stacks — current state

All four version/activity numbers below were read from the `crates.io` v1 API and the GitHub REST
API directly (`gh api repos/<org>/<repo>`), not from marketing copy, on 2026-07-24.

| Crate | License (SPDX) | Train or inference-only | wasm32 support — genuine evidence | Quantization | Version (crates.io) | Stars / open issues / contributors [M, `gh api`] | Last push [M] |
|---|---|---|---|---|---|---|---|
| **candle** (`candle-core`, HuggingFace) | `Apache-2.0 OR MIT` [M, repo LICENSE headers] | **Both.** README lists "Model training" as a first-class feature; autodiff + optimizers ship in-tree. | **Strong, concrete.** Dedicated `candle-wasm-examples/` and `candle-wasm-tests/` directories in the repo (not a doc claim) covering BERT, Whisper, T5, Phi, Llama2-c, YOLO, Segment-Anything, quantized Qwen3 — an encoder-only architecture (BERT) is in the list, which is the shape a reranker would use. Multiple live HF-Spaces browser demos linked from the README. Build instructions in-repo use `rustup target add wasm32-unknown-unknown` + `trunk serve`. [M] | GGUF loading + in-repo k-quant implementation (own `QMatMul`, no external quant library); quantized examples (Llama, Mistral, Qwen3 MoE) ship in-tree. [M/A — verified crate exists and is used in third-party projects (`Crane`), quant *format* support confirmed, exact bit-widths implemented not independently re-derived from source] | `0.11.0` (2026-06-26) [M] | 20,717 / 826 / 270 [M] | 2026-07-23 [M] |
| **burn** (Tracel AI) | `Apache-2.0 OR MIT` [M] | **Both.** Autodiff is a first-class backend decorator (`Autodiff<Backend>`), not bolted on. | **Real, but GPU-path-only.** `burn-wgpu` backend explicitly lists WASM under its WebGPU row of the platform-support matrix; browser MNIST/image-classification examples exist. A CPU-only wasm path (`burn-ndarray`) is also plausible but not the one the README foregrounds. No pure-CPU wasm SIMD example found equivalent to candle's. [M for wgpu row; no browser CPU-backend example independently found] | **Native, real, recent.** v0.19.0 (Oct 2025) release notes: "comprehensive quantization support (INT4/INT8)"; docs confirm per-tensor and per-block quantization down to 8/4/2-bit; **post-training quantization only — QAT is explicitly not yet supported.** [M] | `0.21.0` (2026-05-07) [M] | 15,634 / 293 / 296 [M] | 2026-07-24 [M] | 
| **tract** (Sonos) | `Apache-2.0 OR MIT` [M] | **Inference-only.** Self-described as "a neural-network inference engine"; no autodiff/training code in the project's own framing. | **Real, in-tree example, but example content unverified.** `examples/wasm-model-bench/` exists in the repo (contains `Cargo.toml` + `src/`, confirmed via `gh api repos/sonos/tract/contents/...` file listing), described in project docs as "running tract in the browser." I could not extract the example's own README/source text (404 on the specific path attempted) — the *existence* of the wasm example directory is `[M]`, its exact contents/benchmark numbers are `[UNFETCHED]`. | Not independently confirmed from primary source in this pass — flagged as unverified rather than asserted. | `0.23.4` (2026-07-08) [M] | 3,008 / 96 / 95 [M] | 2026-07-24 [M] |
| **ort** (`pykeio/ort`, ONNX Runtime Rust bindings) | `Apache-2.0 OR MIT` [M] | **Both** claimed (bindings expose ORT's own training APIs) — inference is the overwhelmingly dominant use case. | **Genuinely mixed — this is the most important nuance report 05 missed.** `ort` **does** support `wasm32-unknown-unknown` (confirmed three independent ways: (1) a user's own bug report quotes `#[cfg(target_arch = "wasm32")] ort::wasm::initialize();` as documented API; (2) PR #363 "WASM Emscripten example" and PR #450 "`ort-web`" are both **merged**; (3) a setup guide exists at `ort.pyke.io/setup/webassembly`, referenced directly by users). **But** `wasm32-wasi` support is explicitly **rejected** (issue #206, closed `not_planned`, with a real linker-failure log attached showing duplicate-symbol errors against `onnxruntime`'s bundled libc); a WebGPU-execution-provider-for-wasm32 feature request is also closed `not_planned` (issue #195); and a concrete `wasm-bindgen`/`trunk`-toolchain integration bug (issue #260, "Problem with ort and WASM," a `TextDecoder` panic) was closed `not_planned` — i.e. **known, reported, unresolved friction on exactly the toolchain PanGloss's `pg-wasm` crate already uses** (`wasm-bindgen` + `wasm32-unknown-unknown`). [M, all from GitHub issue/PR text read directly] | Exposed via underlying ONNX Runtime (which supports INT8/dynamic quantization at the ONNX-graph level) — not independently re-verified against `ort`'s own Rust-level API surface in this pass. | `2.0.0-rc.12` (2026-03-05) [M] — **notable maturity signal: `ort` has shipped only release-candidate versions of its 2.0 line since at least `rc.7` (Oct 2024); it has never cut a stable 2.0.0 in over a year and a half of `rc` tags** [M, from `gh api .../releases`] | 2,417 / 1 / 74 [M] | 2026-07-24 [M] |

**Other current, relevant projects found:**

- **`ratchet`** (Hugging Face) — MIT, `[M]`; explicitly "inference only"; web-first design with a live
  Whisper demo on HF Spaces and first-class Q8 quantization in its own JS API
  (`Quantization.Q8`); 767 stars, 1,256 commits, actively developed (28 open issues) but **no
  published version number** — treat as pre-1.0/experimental despite the real demo. `[M]`
- **`dfdx`** — pure-Rust autodiff/deep-learning crate. **Effectively inactive**: last GitHub update
  July 2024, no wasm claims found, no recent release activity — do not build on this. `[M]`
- **`tch-rs`** (libtorch/PyTorch C++ bindings) — requires the native `libtorch` shared library at
  link time; libtorch itself has no WASM target, so `tch-rs` structurally **cannot** target wasm32
  regardless of the Rust wrapper's own code. This rules it out for the WASM deployment mode
  entirely, though it remains a legitimate option for **offline training** on native hardware
  (see below). `[S], inferred from libtorch's own platform support — no wasm32 claim exists to refute]`
- **llama.cpp-adjacent Rust wrappers** (`llama-cpp-2`, `llama_cpp`, `mistral.rs`/`mistralrs`) —
  these wrap `llama.cpp`'s C++/GGML core via FFI. `llama.cpp` itself (via Emscripten, not
  `wasm32-unknown-unknown`) has shipped browser WASM demos (e.g. `whisper.cpp`'s WASM page, same
  GGML core) — but that is the **C++ build path**, not something the Rust FFI wrapper crates
  themselves are demonstrated to carry through to `wasm32-unknown-unknown`. No wasm32 example was
  found for any of the Rust llama.cpp wrapper crates specifically. Not a fit for PanGloss's
  `wasm32-unknown-unknown` target as a Rust dependency; the underlying GGML/llama.cpp C++ core would
  be an ESTABLISHED-C-TO-WRAP candidate only for the *native* deployment mode, not WASM. `[S]`

**Training-story note.** All of candle, burn, and `ort` claim training support; burn's is the most
architecturally deliberate (autodiff is a backend wrapper, not an afterthought) and is the one this
report would recommend for **offline, native, per-language model training** — i.e. training does not
need to happen in the WASM target at all, and nothing here requires it to. `dfdx` and `tch-rs` are
also viable offline-training options on native hardware if PyTorch-ecosystem familiarity or Python
interop matters more than staying in-repo pure-Rust; `tch-rs`'s wasm32 disqualification is irrelevant
to a training-only role.

**Confirms/corrects report 05.** Report 05's "Rust/WASM feasibility" pass asserted (a) candle has a
"WASM target" — **confirmed, strongly** `[M]`; (b) "`ort`... also has WASM support" — **confirmed but
materially incomplete**: `wasm32-unknown-unknown` yes, `wasm32-wasi` explicitly no, and real
toolchain-integration bugs are open/unresolved `[M]`, which report 05 did not know because it worked
from secondary summaries (`ort.pyke.io/backends`, `lib.rs/crates/ort-candle`) rather than the
issue tracker itself.

---

## 2. WASM reality check

### The literature-gap claim, re-examined

Report 05's claim: **no published WASM latency benchmark exists for sub-5M-parameter character
transformers.** This pass searched specifically for it (`"wasm" transformer inference latency ms
benchmark small model browser`, `onnxruntime-web benchmark latency ms tiny model wasm SIMD threads`,
`candle wasm benchmark`) and **confirms the gap for that exact framing** — no paper or benchmark
page was found that (a) states a parameter count under 5M, (b) states a character/tag-level
(non-subword) transformer, and (c) reports a wasm-specific millisecond figure, all three at once.

**But the gap is narrower than report 05 implied**, because directly adjacent numbers do exist and
should inform the estimate rather than be treated as a total void:

- **A real, shipped, WASM-SIMD, Rust-compiled encoder at closely comparable scale.** `ternlight`
  (`github.com/soycaporal/ternlight`, npm packages `@ternlight/base` and `@ternlight/mini`) is a
  sentence encoder distilled from `all-MiniLM-L6` using BitNet-b1.58-style ternary
  quantization-aware training, "custom Rust inference, compiled to WASM SIMD," with **no runtime
  model download** (weights bundled in the wasm binary). Self-reported, independently
  cross-checked against its own npm package listing (not just the blog write-up): `@ternlight/base`
  — 7 MB wire size, **~5 ms/embedding**; `@ternlight/mini` — 5 MB wire size, **~2.5 ms/embedding**.
  `[A]` — this is the project's own stated benchmark, not one I ran, but it is a real shipped
  artifact (verifiable npm package + GitHub repo), not a marketing estimate with nothing behind it.
  This is architecturally close to our target (Rust → wasm32 SIMD, encoder-only transformer,
  aggressively quantized) even though it operates on subword tokens rather than a tag vocabulary and
  is at a somewhat larger post-distillation scale than our 0.5–10M target — it is the single most
  relevant concrete number found anywhere in this research pass.
- **`Xenova/all-MiniLM-L6-v2` on the HF Spaces "WebGPU Embedding Benchmark" tool** (fetched directly
  from the discussion thread, not a secondary summary): unquantized fp32, sequence length 512,
  Chrome 122/Windows —

  | Batch | WASM (ms) | WebGPU (ms) |
  |---|---|---|
  | 1 | 507.2 | 45.7 |
  | 2 | 1059.8 | 137.6 |
  | 4 | 2089.7 | 204.9 |
  | 8 | 4172.9 | 356.7 |
  | 16 | 8225.3 | 367.4 |
  | 32 | 16416.9 | 579.1 |

  `[A]` — a specific benchmark tool's own reported output, read directly from the discussion thread,
  not re-run. This is the *same model family* as ternlight's base model (MiniLM-L6, 22M params) but
  **unquantized and at 100× the sequence length** (512 vs. ternlight's short-query use case) — the
  ~100× gap between 507ms here and ~5ms for ternlight is consistent with (quantization gain) ×
  (sequence-length gain), not a contradiction, and is itself informative: **sequence length and
  quantization dominate wasm transformer latency far more than raw parameter count does at this
  scale**, which matters directly for a reranker whose sequences are a handful of tag tokens, not
  512 subwords.
- **ONNX Runtime Web's own published number**: "using two threads with SIMD enabled can accelerate
  CPU inference by up to 3.4x compared to pure WebAssembly without those features." `[A]` — this
  figure is quoted across multiple secondary pages citing ORT's own documentation; I was not able to
  fetch `ort.pyke.io`/`onnxruntime.ai` performance pages directly (403 / not retrievable in this
  pass) to confirm the exact model/scale it was measured at, so **the scale this 3.4× figure applies
  to is unconfirmed** — report 05's citation of this number is corroborated as *existing*, but its
  applicability to sub-5M-param models specifically remains unverified either way.
- **No number exists anywhere found for a model in the exact 0.5–10M range with a tag-sized (few
  hundred entries) vocabulary.** This specific configuration is not benchmarked in public literature.
  Confirmed as a **genuine, first-class gap** — restated per the "be honest about negative results"
  rule: if PanGloss builds this, it will be producing the first public number at this exact point in
  the design space, not confirming or refuting an existing one.

### WASM SIMD and threading — browser support matrix

- **Fixed-width WASM SIMD** (128-bit): shipped in Chrome 91+, Firefox 89+, Safari 16.4+, Edge 91+,
  Opera 77+ `[A, via caniuse.com's live table, summarized through search since the interactive table
  itself isn't text-extractable]`. Safari's 16.4 floor (March 2023) is the practical constraint for
  "does SIMD reach effectively all users" — recent enough that most non-EOL devices support it, but
  not universal on old iOS.
- **Threads + `SharedArrayBuffer`**: supported in Chrome 74+, Firefox 79+, Safari 14.1+ (macOS) /
  14.5+ (iOS), Edge 79+ — "every major engine ships WASM threads on desktop and mobile, with global
  usage near 95%" `[A]`. **But** every browser gates `SharedArrayBuffer` behind cross-origin
  isolation: the page must serve `Cross-Origin-Opener-Policy: same-origin` **and**
  `Cross-Origin-Embedder-Policy: require-corp` `[M, web.dev — Google's own developer docs, read
  directly]`. This became mandatory (not optional) in Chrome 92+ specifically because of the 2018
  Spectre disclosure — `SharedArrayBuffer`'s high-resolution-timing side channel was disabled
  entirely until COOP/COEP could reintroduce it safely `[M]`. **Practical implication for
  PanGloss**: multithreaded WASM inference is only available if the *host page* embedding the
  PanGloss WASM module opts into cross-origin isolation — this is a deployment requirement on
  whoever integrates PanGloss into a browser app, not something PanGloss's own WASM binary controls,
  and it may not be satisfiable in every host context (embedded widgets inside third-party pages,
  browser extensions with restricted header control, etc.). A design that depends on WASM threads to
  hit its latency budget is taking on an integration-environment dependency, not just a compute one.

### Linear-memory cost model

- WASM linear memory is "a contiguous, mutable array of uninterpreted bytes" that grows in 64KiB
  page increments; every memory access is runtime-bounds-checked against the current allocation
  size `[M, radu-matei.com, read directly]`.
- **Bounds-checking overhead is real and workload-dependent, not negligible for matmul-heavy code**:
  a dedicated study ("Leaps and Bounds: Analyzing WebAssembly's Performance with a Focus on Bounds
  Checking," IISWC 2022) is reported (via search synthesis of its abstract/citations — **the PDF
  itself returned as unreadable binary in this pass, so treat the specific numbers as `[A]`, not
  `[M]`**) to measure bounds-checking overhead ranging from roughly **20% (Cholesky) to 220%
  (`gemm`)** depending on benchmark, with a worst case cited at up to 650%. **`gemm` — general
  matrix multiply — is precisely the operation that dominates transformer inference cost**, which
  means this is not an incidental finding for this report's purposes; it is close to the worst-case
  workload shape the paper tested. The same search also surfaced V8-specific SPEC-benchmark
  slowdown figures of roughly 1.55–1.76× vs. native `[A]`, consistent in direction (WASM meaningfully
  slower than native on compute-bound code, gap growing on non-x86 targets like Armv8/RISC-V) but
  measuring a different (general SPEC, not gemm-specific) workload.
- **No true SIMD gather/scatter**: WASM's fixed-width SIMD proposal does not include gather/scatter
  instructions; the follow-on "Relaxed SIMD" proposal (which does relax some semantics for
  performance, e.g. fused-multiply-add, swizzle, 4-way dot-product) is scoped explicitly around
  *local non-determinism* for existing operation categories (integer reinterpretation, float
  conversion edge cases, fma/precision) — gather/scatter specifically was not found described or
  planned anywhere in the WebAssembly/relaxed-simd repo's own overview document `[M, read directly
  from `github.com/WebAssembly/relaxed-simd`]`. This matters for attention/embedding-lookup code
  paths (index-driven gathers over an embedding table or KV cache) more than for the dense matmuls
  in the feed-forward/attention-projection layers, which is a further reason (beyond raw parameter
  count) that a **small, gather-light architecture is a better wasm fit than a naive port of a large
  transformer's data-access patterns** — though this is inference from the spec text, not a measured
  effect, and is marked `[S]`.

### WebGPU / WebNN — do they change the picture for tiny models?

- **WebGPU**: the HF Spaces benchmark above shows an **~11× speedup at batch 1** (507.2ms → 45.7ms)
  for the same MiniLM-scale model, growing to tens-of-×  at larger batch sizes — a real, directly
  measured number, not extrapolated `[M]`. Separately, Hugging Face's own `transformers.js` v3
  announcement claims "up to 100x faster than WASM" for WebGPU generally `[A — the blog post itself
  did not surface supporting per-model numbers when fetched directly, so treat the "100x" figure as
  asserted, not demonstrated, in the source actually read]`. **Caveat for tiny models specifically**:
  WebGPU's advantage comes overwhelmingly from parallelism across large matrix dimensions and
  batch/sequence size; the Xenova benchmark's own batch-size curve shows the WASM-to-WebGPU gap
  *shrinking* at very small effective problem sizes is plausible but not directly evidenced by data
  at batch<1 — there is no dedicated data point at the reranker's actual shape (batch of 5–50 short
  independent sequences, which could itself be batched to look like the benchmark's batch=8–32 rows,
  where WebGPU's advantage is largest, ~13–24× rather than ~11×). **`wgpu`** is the relevant Rust
  crate: mature and widely used for cross-platform graphics/compute (Vulkan/Metal/D3D12/GL natively,
  WebGL2/WebGPU on wasm), consumes WGSL/SPIR-V/GLSL, and is explicitly documented to hand WGSL
  straight through to the browser's own WebGPU implementation when compiled to wasm32 without the
  `webgl` feature `[M, docs.rs/wgpu + gfx-rs/wgpu README]`. The caveat found independently: "browser
  implementations of WebGPU are simply not stable yet" per a 2024-dated community source `[A]` —
  browser WebGPU has matured further since, per the maturing-but-still-partial WebNN picture below,
  but should not be assumed rock-solid across all target browsers without a fallback path.
- **WebNN**: reached **W3C Candidate Recommendation status in January 2026** `[A]`, but is "not ready
  for production" per its own compatibility docs — Chrome/Edge have experimental support,
  **Firefox and Safari have not adopted it** `[A]`. This rules WebNN out as a load-bearing dependency
  for a cross-browser PanGloss deployment today. Rust-side, an active third-party implementation
  effort (`rustnn`, aimed at bringing WebNN to Firefox) exists — v0.5.12 (May 2026), 89% operator
  coverage, CoreML/ONNX backends, 1,350+ WPT conformance tests passing `[A]` — genuinely interesting
  as a forward signal but not shippable infrastructure today: it targets *adding* WebNN to Firefox,
  it is not a stable API PanGloss could build against yet.
- **Net for a tiny reranker**: WebGPU is the more credible near-term accelerator of the two (real
  measured speedups, real mature Rust bindings via `wgpu`), but the WASM CPU path is the one with
  actual comparable-scale evidence (ternlight) and no cross-origin-isolation or browser-support
  caveat attached — **the CPU/SIMD wasm path, not a GPU path, is the safer primary target**, with
  WebGPU as an optional accelerator behind a capability check, not a requirement.

---

## 3. Parameter and latency budget

### The structural point: tag vocabulary collapses embedding+output-projection cost

**GPT-2 small, ground truth from primary source** (`huggingface.co/openai-community/gpt2/config.json`,
fetched directly): `vocab_size=50257`, `n_embd=768` (`d_model`), `n_layer=12`, `n_head=12`,
`n_positions=1024`. `[M]`

Using the standard transformer parameter-count decomposition (consistent with the commonly-cited
Kaplan-et-al-style approximation `N_non-embed ≈ 12 · n_layer · d_model²` for a 4×-expansion
feed-forward block with combined QKV + output projection — attention: `4·d_model²`
(`3·d_model²` QKV + `d_model²` out-proj); MLP: `8·d_model²` (`4·d_model²` up + `4·d_model²` down);
sum `12·d_model²` per layer) `[S, arithmetic below, structure of the formula corroborated by
independent secondary sources found during this pass]`:

- Transformer body (12 layers, d=768): `12 × 12 × 768² = 84,934,656` ≈ **85.0M**
- Token embedding (`vocab × d_model`): `50,257 × 768 = 38,597,376` ≈ **38.6M**
- Position embedding (`n_ctx × d_model`): `1,024 × 768 = 786,432` ≈ **0.79M**
- **Total (weights tied, GPT-2's actual configuration)**: 85.0 + 38.6 + 0.79 ≈ **124.3M**, matching
  the well-known "GPT-2 small = 124M" figure `[S, cross-checked against 124M being the universally
  cited figure for this exact config — consistency, not independent re-derivation, confirms the
  arithmetic]`.
- **Embedding share of total, tied weights**: 38.6M / 124.3M ≈ **31%** of all parameters are the
  single (shared) embedding/output matrix.
- **If untied** (separate output projection matrix, not shared with input embedding — some
  architectures do this): total ≈ 85.0 + 38.6 + 38.6 + 0.79 ≈ 163.0M; embedding+output share ≈
  77.2M / 163.0M ≈ **47%**.

**Same architecture body, vocab swapped to 300 (a plausible tag-vocabulary size), everything else
held fixed** (d=768, 12 layers, ctx=1024) `[S]`:

- Transformer body: unchanged, 85.0M
- Token embedding: `300 × 768 = 230,400` ≈ 0.23M
- Position embedding: unchanged, 0.79M
- **Total**: ≈ 86.0M; **embedding share: 230,400 / 85,951,488 ≈ 0.27%** — down from 31% to
  a quarter of one percent, for the identical transformer body.

**Now shrink the body too — plausible reranker configs at vocab=300, short context (64 tokens —
"a word's analysis plus a few words of context")** `[S, full arithmetic shown]`:

| Target scale | d_model | n_layer | Body params (`12·L·d²`) | Embed (tied, `300·d`) | Pos emb (`64·d`) | **Total params** | Embed share (tied) | Embed share (untied) |
|---|---|---|---|---|---|---|---|---|
| ~0.5M | 96 | 4 | 442,368 | 28,800 | 6,144 | **477,312** | 6.03% | 12.06%* |
| — | 64 | 4 | 196,608 | 19,200 | 4,096 | **219,904** | 8.73% | 15.75%* |
| — | 128 | 4 | 786,432 | 38,400 | 8,192 | **833,024** | 4.61% | 8.72%* |
| — | 256 | 4 | 3,145,728 | 76,800 | 16,384 | **3,238,912** | 2.37% | 4.63%* |
| ~2M | 192 | 5 | 2,211,840 | 57,600 | 12,288 | **2,281,728** | 2.52% | 4.98%* |
| ~10M | 320 | 8 | 9,830,400 | 96,000 | 20,480 | **9,946,880** | 0.97% | 1.92%* |

\*untied share computed as `2×embed / (total + embed)`.

**Reading the table**: at GPT-2's real vocabulary, embedding+output is 31–47% of parameters — the
single largest component in the model, larger than any individual transformer layer. At a
tag-sized vocabulary, embedding+output falls to low single digits or below 1% across every
plausible small-model config, **and never exceeds ~16% even in the smallest, most embedding-heavy
config tested (d=64) with untied weights** — the opposite regime from a normal small LM, where
shrinking `d_model` makes the now-fixed-size vocabulary table dominate *more*, not less. This is the
direct, quantified confirmation of the brief's structural claim: **a tag vocabulary doesn't just
shrink the model, it changes which component the parameter budget is spent on**, from
"mostly a giant lookup table" (GPT-2-style char/subword models) to "mostly transformer compute"
(this reranker). The practical consequence is that *quantizing the embedding table specifically*
(a common trick for subword models, since it's the single biggest tensor) buys almost nothing here
— the transformer body, not the vocabulary, is where size and compute both live for this design.

### CPU/WASM cost estimate

**FLOPs-per-forward-pass, standard approximation**: `2 · N · T` FLOPs, where `N` = (non-embedding)
parameter count and `T` = sequence length — "the factor of two comes from the multiply-accumulate
operation dominating matrix multiplication," per Kaplan et al. 2020's own framing, corroborated
across multiple sources found in this pass `[A — standard, widely-cited approximation, not
independently re-derived from Kaplan et al.'s original text in this pass]`. This formula undercounts
attention's own `O(L·d_model·T)` activation-activation cost, but for the reranker's short sequences
(T≈30–60 tokens, `d_model` in the 64–320 range from the table above) attention cost is a small
fraction of total FLOPs relative to the dense QKV/FFN projections, so the undercount is minor here
— explicitly the regime where Kaplan's own caveat about "short-context workloads" being fine to
approximate this way applies `[S]`.

Per-candidate forward-pass FLOPs at `T=50`:

| Model | N (params) | FLOPs/candidate (`2NT`) | FLOPs for 50 candidates |
|---|---|---|---|
| ~0.5M | 477,312 | 4.8×10⁷ (48 MFLOP) | 2.4×10⁹ (2.4 GFLOP) |
| ~2M | 2,281,728 | 2.3×10⁸ (230 MFLOP) | 1.1×10¹⁰ (11.4 GFLOP) |
| ~10M | 9,946,880 | 9.9×10⁸ (995 MFLOP) | 5.0×10¹⁰ (49.7 GFLOP) |

`[S, arithmetic; no independently-sourced achieved-GFLOP/s figure for a wasm32 SIMD small-matmul
workload was found in this pass to convert this into milliseconds responsibly — converting via an
assumed GFLOP/s figure would be estimating a number on top of an estimated number, which the sourcing
rules ask to avoid. Instead:]`

**Empirical calibration instead of FLOP/s extrapolation.** The one real, comparable, measured
data point is ternlight (§2): a **Rust, WASM-SIMD-compiled, ternary-quantized, MiniLM-distilled
encoder transformer** (larger than our 0.5–10M target pre-quantization, but the *shipped, quantized*
runtime scale is plausibly within a few × of our ~2–10M range) runs a full forward pass over a real
short text query in **~2.5–5 ms** in-browser `[A]`. Scaling that directly (not FLOP-derived, just
proportionally against the one real number available) to scoring 5–50 candidates sequentially,
unbatched:

| Candidates scored | At ~2.5ms/pass (mini-scale calibration) | At ~5ms/pass (base-scale calibration) |
|---|---|---|
| 5 | 12.5 ms | 25 ms |
| 20 | 50 ms | 100 ms |
| 50 | 125 ms | 250 ms |

`[S, direct scaling of an `[A]` empirical number — not a FLOP-derived estimate]`. This lands
**inside or at the edge of a typical interactive-UI latency budget (~100–300ms for a
"still feels instant" correction/rerank step)** across the whole 5–50 candidate range, *if* our
model's actual runtime cost per forward pass is in the same ballpark as ternlight's (plausible,
since our target param counts are smaller and our sequences are shorter than ternlight's typical
query text, but our token-embedding lookup + attention pattern is architecturally different enough
that this is a calibration, not a guarantee). Batching all candidates into one padded forward pass
(rather than 5–50 sequential calls) would very likely bring this down further, consistent with the
batch-size behavior seen in the Xenova WASM/WebGPU table (§2) where WASM throughput-per-item
improves with batching even though absolute latency grows — **no measured number exists for batched
short-sequence wasm inference at this exact scale either; flagged, not estimated further.**

**Bottom line for Q3**: the arithmetic is unambiguous and the empirical calibration is encouraging,
but **there is no substitute here for measuring the actual model once one exists** — every number in
this section down to the FLOPs table is real arithmetic on a real published GPT-2 config; every
number past that point (converting FLOPs or ternlight's number into "our model's" milliseconds) is
explicitly flagged as calibration/extrapolation, not measurement.

---

## 4. Quantization and small-model techniques

| Technique | Measured accuracy cost | Scale it was measured at | Rust-stack support (Q1 crates) |
|---|---|---|---|
| **INT8 post-training quantization** | "Accuracy statistically indistinguishable from FP32... the well-established result that 8-bit quantization preserves model quality with negligible accuracy cost" `[A]` | Broad LLM-scale literature (100M+ params); **no dedicated study found at <10M-param scale** — flagged explicitly per the sourcing rules, not assumed to transfer. | **Out-of-the-box** in `burn` (native INT8 PTQ, v0.19.0+, `[M]`) and `candle` (own k-quant GGUF-format implementation, `[M]`). `ort` exposes it via the underlying ONNX Runtime graph-level quantization tooling (not independently re-verified at the Rust-API level in this pass). |
| **INT4 quantization** | Mixed by architecture: "no to negligible accuracy degradation for encoder-only and encoder-decoder models, but... significant accuracy drop for decoder-only models" at one study scale; elsewhere "naive quantization to INT4 typically results in unacceptable accuracy degradation — perplexity increases of 10–50% or more" absent careful calibration `[A]`. Encoder-only is exactly this design's architecture, which is a mildly favorable signal. | LLM-scale (100M+) papers throughout; **no <10M-param study found.** | `burn` ships INT4 PTQ (`[M]`, v0.19.0+, alongside INT8/2-bit). `candle` supports GGUF's INT4-family k-quant formats (`Q4_0`/`Q4_K` etc.) via its own quantization code `[M]`. Neither is verified against a linguistically-tiny (<10M param) model in any source found — the *format* support is real and out-of-the-box; the *accuracy outcome at this scale* is unmeasured anywhere. |
| **Distillation** | The ternlight case (§2/§3) is itself a real, shipped distillation-to-ternary-weights example (`BitNet b1.58`-style, distilled from `all-MiniLM-L6`) `[A]`, but its accuracy retention numbers were not published in any source fetched — only its size/latency were. No distillation-accuracy-at-<10M-param number was found for a task resembling reranking. | N/A — no accuracy number found. | No dedicated distillation tooling found in candle/burn/tract/`ort`; this is a training-time technique (teacher-student loss during offline training), not an inference-runtime feature, so it lives in whichever offline training tool is chosen (burn/candle/`tch-rs`/Python), not in the WASM inference stack at all. |
| **Quantization-aware training (QAT)** | Not separately evidenced at any scale in sources found this pass. | — | **Not supported by `burn`** as of the version checked (`[M]`, explicitly noted as a gap in the same release notes that announced PTQ). Not found in candle either. This is the one clear **MUST-BUILD-OR-DO-WITHOUT** gap in the quantization story: if PTQ accuracy loss turns out to matter at <10M-param scale (unmeasured, per above), QAT is not an off-the-shelf Rust option today. |

**Honest summary for Q4**: quantization *format* support (INT8/INT4, GGUF k-quants) is genuinely
mature and out-of-the-box in both `burn` and `candle` — this is not a port target. What is
**genuinely unknown, not just unported**, is whether quantization holds up at PanGloss's actual
scale: essentially all published INT8/INT4 accuracy numbers are at 100M+ parameter scale, and this
report found zero dedicated small-model (<10M param) quantization-accuracy studies in any direction
(favorable or unfavorable) during this research pass. This should be treated as an open empirical
question to be answered by measuring PanGloss's own trained reranker, not inferred from LLM-scale
literature.

---

## 5. The port inventory

Licensing context established directly from this repo (not asserted): **PanGloss is MIT-licensed**
(`LICENSE`, read directly `[M]`); every `pg-*` Rust crate in `rust/crates/` inherits
`license.workspace = true` → MIT (`rust/Cargo.toml:29`, `[M]`). This matters concretely below.

| Component | Status | Notes / size estimate |
|---|---|---|
| **Transformer inference** (attention, layernorm, feedforward) | **EXISTS-IN-RUST** | `candle` and `burn` both implement full transformer inference primitives natively, with wasm32 evidence for each (§1). No port needed — this is an integration task (write the reranker's forward pass against one of these crates), not an engine-building task. |
| **Training loop / autodiff engine** (offline, per-language) | **EXISTS-IN-RUST** (native target; not required in WASM at all) | `burn`'s `Autodiff<Backend>` wrapper is the most architecturally clean fit; `candle` also supports training. Training happens offline on native hardware — the WASM target only needs to *load* the trained, quantized weights, never train. `tch-rs`/`dfdx` are native-only fallbacks if PyTorch-ecosystem interop is wanted for this offline step specifically; `dfdx`'s inactivity (§1) argues against relying on it. |
| **Tokenizer-equivalent for a tag vocabulary** | **TRIVIAL — not a port target; flagging explicitly per the brief's instruction not to invent complexity that isn't there.** | The "tokens" are already discrete grammar tags/morpheme IDs/feature-bundle IDs emitted by HermitCrab's own analysis — there is no raw-text segmentation problem (no BPE, no WordPiece, no subword-merge table) at all. The entire "tokenizer" is a closed, small (few-hundred-entry) enumeration → integer-ID lookup, which is a `HashMap`/small perfect-hash away, not a library dependency. This is the one component in this table that is cheaper than the brief even hypothesized. |
| **Quantization (post-training INT8/INT4)** | **EXISTS-IN-RUST** | Native, real support in `burn` (v0.19.0+) and `candle` (GGUF k-quant), per §1/§4. QAT specifically is a gap (§4) but PTQ is not a port target. |
| **Model serialization/format** | **EXISTS-IN-RUST for safetensors and GGUF-reading; EXISTS-IN-RUST (via `tract`/`ort`) for ONNX.** | `safetensors` (HuggingFace's own crate): mature, **4,346,201 all-time downloads**, **100% rustdoc coverage**, current version `0.7.0`, actively maintained by HF staff `[M]` — this is the strongest, most boring, most production-ready option and the natural default for PanGloss's own weight format. GGUF-reading crates exist but are fragmented and comparatively immature — `gguf-rs` (zero-copy, lazy tensor loading), `gguf-rs-lib` (async I/O + mmap feature flags, "significantly faster than Python" per its own claim, 4,087 downloads `[A]`), `woolly-gguf` (mmap-based, strongly typed) — **no single dominant, canonical GGUF-reading crate the way `safetensors` is canonical for its format**; picking one means picking among several small, young libraries, not a MUST-PORT job but a real evaluation task. ONNX loading is handled by `tract` (native format support, its whole purpose) and `ort` (via the underlying ONNX Runtime); both are EXISTS-IN-RUST for reading, not something to port. **Recommendation for PanGloss's own `.pgpack`-embedded format: `safetensors`, given its maturity gap over the GGUF options and the fact that `.pgpack` already needs a flat, mappable binary layout (§6) that safetensors' own zero-copy design provides natively.** |
| **Score calibration** (temperature scaling, Platt scaling) | **MUST-PORT — but trivially small.** | No dedicated Rust crate was found for either technique (`crates.io` search for "temperature scaling"/"Platt scaling" surfaced nothing on-topic; the one crate literally named `scaling` is an unrelated benchmarking tool). **This is a MUST-PORT finding, but the smallest one in this table by a wide margin**: temperature scaling is a single learned scalar `T` dividing logits before softmax — a few dozen lines of Rust plus a tiny offline calibration fit (e.g. gradient descent on held-out NLL, itself trivial in `burn`/`candle` since the forward pass already exists there); Platt scaling is a 2-parameter logistic regression, similarly small. **Size estimate `[S]`: 1–2 days**, almost entirely calibration-data-collection and validation time rather than engineering time. |
| **A factored/class-LM engine with backoff-graph search** | **MUST-PORT — confirmed, unchanged from report 04.** | Report 04 established SRILM's FLM module is the only reference implementation (20+ year old C++) and KenLM dropped factor support entirely; this pass found no new evidence of a maintained alternative in Rust or otherwise, and no LOC/effort estimate for such an engine exists anywhere in public literature (searched directly; nothing found). **Size estimate `[S]`, reasoned from scratch**: an ordinary word/tag-level Kneser-Ney n-gram engine (fixed backoff order, no factor graph) is itself already characterized by report 04 as "a real, multi-week engineering project, not a config flag." A *factored* LM adds: (a) a generalized-parallel-backoff graph search over which factor-combination to fall back to when a combination is unseen (the graph structure itself is a design choice requiring held-out tuning, not just an implementation task), (b) multi-factor count collection and smoothing per node in that graph, (c) a query engine that walks the graph rather than a single fixed chain. This is a strictly larger problem than plain KN backoff, not a variant of it. **Estimate: 4–8 person-weeks** for a correctness-first Rust implementation covering count collection, a fixed (not searched) backoff graph over 2–3 factors (e.g. POS, feature-bundle, lemma), and query/interpolation — **doubling to 8–16 weeks** if the backoff-graph-search-over-candidate-graphs step (choosing *which* graph, per Bilmes & Kirchhoff, rather than hand-fixing one) is included, since that step is itself a held-out-tuned search procedure requiring its own evaluation harness. `[S]` |
| **Constraint Grammar (CG-3) engine in Rust** | **PARTIAL EXISTS-IN-RUST — active, very recent, single-author, unreleased-stable, GPL-licensed. The most important finding in this report.** | See dedicated discussion immediately below the table. |

### The CG-3 finding, in full

Report 04/05 flagged this as an open question, not exhaustively searched. This pass did an
exhaustive search — `crates.io` full-text search UI (JS-rendered, worked around via the
`crates.io/api/v1/crates?q=...` JSON endpoint, a legitimate primary source), `lib.rs`, GitHub code
search, and direct web search for "constraint grammar rust," "cg3 rust," "vislcg3 rust bindings,"
"hfst rust." Results:

- **`hfst` Rust bindings: none found.** Clean negative result. `[M]`
- **VISL `cg3` (the C++ reference implementation, GPL-3.0-or-later) FFI bindings from Rust: none
  found.** Clean negative result. `[M]`
- **A from-scratch or transpiled Rust CG-3 engine: exists, and is very new.** The `crates.io`
  full-text API search for `cg3` returned exactly one on-topic result:
  **`cg3` — "VISL CG-3 (Constraint Grammar) engine — Rust port"**, `crates.io/crates/cg3`, repository
  `github.com/divvun/cg3-rs`. `[M, read directly from the crates.io API and the repo itself]`

**What it actually is, verified directly** (not taken from the crate description alone):

- **Author**: Brendan Molloy (`bbqsrc`), the crate's sole listed owner `[M]`. Independently
  corroborated as a genuine, long-standing Divvun infrastructure engineer — public information
  describes his prior Divvun work spanning CI/CD (Buildkite/Deno pipelines, 2025), production
  Sámi-language text-to-speech (2023–2025), and the "Divvun Runtime" supporting grammar checking and
  TTS (2023–2024) `[A, from search synthesis of his public resume/profile, not independently
  cross-verified against a primary Divvun announcement]`. This is not an anonymous or hobby effort —
  it is the organization that *owns* GramDivvun/`libdivvun` (the CG-3-based pipeline report 04/05
  already identified as PanGloss's closest architectural precedent) building a Rust CG-3 engine.
- **Status, per version history**: `v0.1.0` (2026-07-12) → `v0.1.1`/`v0.1.2` (2026-07-12) →
  **`v0.2.0` (2026-07-20)** `[M, crates.io API]`. **This crate's most recent release is four days
  before this report's research date (2026-07-24).** This is not a mature, battle-tested dependency
  — it is a very-fresh, very-active in-progress port.
- **Commit history** (`gh api`, read directly): first commit 2026-07-12, most recent 2026-07-20,
  roughly 30–60 commits in 9 days — "intense, sustained development," commit messages showing a
  structured "engine-decomp" refactoring campaign plus explicit "unsafe-zero"/"forbid unsafe_code"
  hardening work, not a one-shot code dump. `[M]`
- **Claimed completeness** (README, read directly, `[A]` — maintainer's own claim, not
  independently tested by this report per the design-only/no-spikes constraint): "fully compatible
  with CG-3 grammar source and byte-compatible with the current `.cg3b` binary ABI (rev 13898)";
  ships **six** of the upstream command-line binaries (`vislcg3`, `cg-comp`, `cg-proc`, `cg-conv`,
  `cg-relabel`, `cg-mwesplit`); implements all core rule types (SELECT/REMOVE/ADD/MAP/SUBSTITUTE);
  all eight upstream stream formats (CG, Apertium, Niceline, FST, plaintext, JSONL, binary, matxin);
  "successfully processes real linguistic data through Divvun's grammar checkers and Apertium
  machine translation systems"; includes "the upstream conformance test corpus." `docs.rs` separately
  reports "43.33% API coverage," which on inspection is a **rustdoc doc-comment coverage metric**
  (fraction of items with doc comments), **not** a functional-completeness metric — the README's own
  "Wave 2 of 4" framing describes an *idiomaticity* roadmap ("literal, bug-for-bug 1:1 translation"
  now, "idiomatic cleanups... deferred to Wave 4"), not a feature-completeness roadmap. Read together,
  the evidence points to "functionally complete, C++-shaped Rust code, not yet cleaned up" rather
  than "partial port" — but this is the maintainer's self-report, unverified by this report's own
  testing (explicitly out of scope — design-only, no spikes), and should be independently validated
  against the upstream conformance corpus before being relied on.
- **Code-quality signal**: `unsafe_code = "forbid"` enforced across library, binaries, and tests
  `[M, README]` — a meaningfully strong safety commitment for something claiming C++-parity
  behavior, and a positive signal for eventual wasm32 portability (no raw-pointer FFI surface to
  re-audit).
- **wasm32/WASM**: **explicitly out of scope by the maintainer's own stated design** — the README
  places "WebAssembly/Emscripten builds" alongside "native C API and language bindings" in an
  explicit out-of-scope list, contrasting with upstream C++ `cg3`, which does offer an Emscripten
  target `[M, read directly]`. **This does not necessarily mean the Rust code cannot compile to
  `wasm32-unknown-unknown`** — the crate is described as pure-Rust-by-default with only the optional
  SQLite-backed profiling feature requiring a C toolchain, which is architecturally wasm-friendly —
  but no wasm32 build, CI job, or example exists to confirm this either way. **Unverified, not
  refuted; flagged explicitly as the single biggest open question about this dependency for
  PanGloss's specific WASM deployment mode.**
- **Adoption/maturity signals**: 0 GitHub stars, 0 open issues, 73 total crates.io downloads at time
  of research `[M]` — consistent with a project that is days old, not evidence of a problem, but a
  reminder that **no independent party has yet exercised this code** beyond its own author and
  whatever internal Divvun testing produced the README's compatibility claims.
- **License — the load-bearing catch**: `GPL-3.0-or-later` `[M, crates.io API, matching upstream
  `cg3`'s own license]`. **PanGloss is MIT-licensed** (verified above). Depending on a
  GPL-3.0-or-later Rust crate as a **linked library dependency** inside a distributed PanGloss binary
  would very plausibly make the combined work a GPL-3.0 derivative for licensing purposes, which is
  a real constraint on a project that is otherwise entirely MIT — this is a licensing/legal
  determination this report is not qualified to make definitively (consult counsel before depending
  on it), but it is not a technicality to wave past. **Critically, this is the exact same constraint
  that already existed for the ESTABLISHED-C-TO-WRAP alternative** (FFI-wrapping the upstream C++
  `cg3`, also GPL-3.0-or-later) **— finding a Rust port does not change the licensing math at all**,
  it only changes the *engineering* math (no FFI/unsafe boundary to build and maintain). For the
  **native** deployment mode, "ship CG-3 as a separate GPL-licensed process/tool invoked via IPC or
  subprocess, keeping PanGloss's own code MIT" is a commonly-used pattern that may sidestep the
  "linked derivative work" question (again: not a determination this report can make) — but that
  pattern is **not available in the WASM deployment mode at all**: there is no separate-process
  model inside a browser sandbox, so a WASM-target CG-3 dependency would have to be linked directly
  into the same wasm module as everything else, which is precisely the scenario where the GPL
  linking question bites hardest. **This makes the licensing question, not the engineering question,
  the binding constraint on using `cg3-rs` (or upstream `cg3` via FFI) inside PanGloss's WASM
  Runtime specifically** — independent of whether `cg3-rs` itself ever gains a wasm32 target.
- **If a from-scratch MIT/Apache-licensed Rust CG-3 engine were built instead** (the
  build-philosophy-default answer if the license blocks `cg3-rs`): report 04 already established the
  formalism is well-documented (VISL's own CG-3 spec/tutorial, `edu.visl.dk/cg3_howto.pdf` and
  `edu.visl.dk/cg3/single/`, both cited there) and `cg3-rs`'s existence — even if unusable directly
  due to license — is now a **de facto reference implementation and test corpus to validate against**
  (its README states it ships "the upstream conformance test corpus," which is very likely itself
  redistributable independent of the GPL question since test *data* and GPL *code* are different
  licensing questions, though this too wants a real license check, not an assumption). **Size
  estimate `[S]`, informed by `cg3-rs`'s own scope (the clearest available proxy for how large this
  problem actually is, having just been built once)**: the rule engine + textual parser + core
  applicator (SELECT/REMOVE/ADD/MAP/SUBSTITUTE against a cohort/reading data model) is the load-bearing
  core and is comparable in scope to a mid-sized parser+interpreter project — **8–14 person-weeks**
  for a from-scratch MIT-licensed engine covering the core rule types and textual grammar format
  only (skipping the binary `.cg3b` ABI compatibility, the six CLI tools, and the eight stream-format
  converters `cg3-rs` also built, none of which PanGloss's reranker/detection use case strictly
  needs) — **rising toward `cg3-rs`'s own apparent ~2-person-week timeline for the fuller scope** if
  binary-format compatibility with existing Divvun/Giellatekno-authored `.cg3`/`.cg3b` grammars is
  wanted (which would let PanGloss potentially reuse or adapt existing GramDivvun-style rule sets
  rather than authoring CG rules from nothing — a real strategic advantage report 04 already flagged
  CG rule-writing as valuable for). **This is a materially smaller estimate than "MUST-PORT, nothing
  to go on" would have produced before this pass** — `cg3-rs`'s existence, even if its own license
  makes it undependable-on directly, is genuinely useful as a design/validation reference either way.

---

## 6. Model distribution

**`.pgpack` is explicitly data-only.** Read directly from this repo's own `CONTEXT.md` (not
asserted from memory): the PanGloss Language Pack (`.pgpack`) is defined as "a data-only runtime
plugin containing the proposing FST, matching Rust-HermitCrab runtime data, configured compact
diagnostic symbols, and package metadata. **It cannot contain WASM modules, native libraries,
scripts, or executable extensions.**" `[M, CONTEXT.md]`. This is a hard architectural constraint
already in force, not a new design decision this report is proposing: **a shipped reranker's
weights go into `.pgpack` as pure data (a safetensors blob or equivalent flat tensor layout);
the code that interprets those weights (the transformer forward pass) must already be compiled into
the PanGloss Runtime/WASM binary itself, not shipped inside the pack.** This maps cleanly onto the
existing FST-plus-runtime-data split `.pgpack` already uses for the analyzer.

**Format choice**: given §5's finding that `safetensors` is the most mature Rust option (100%
rustdoc coverage, 4.3M+ downloads, HF-maintained, zero-copy-by-design) versus GGUF's fragmented
small-crate ecosystem, **safetensors is the recommended weight format for the `.pgpack`-embedded
reranker blob** — it is also architecturally the right shape already: a flat header (JSON: dtype,
shape, byte offsets) followed by a raw tensor byte region, which is exactly the "zero-copy binary
blob" pattern report 04 already recommended for the n-gram tables (§7 of that report), giving
format continuity across the pack's different data components rather than introducing a second,
unrelated serialization scheme just for the reranker.

**Size, from §3's arithmetic, at INT8 (≈4× reduction from fp32, per the "routinely gives ~4x size
reduction with minimal quality loss" finding in §4, itself an LLM-scale-literature figure applied
here by extrapolation, `[S]`)**:

| Model | Params | fp32 size | INT8 estimate (`[S]`, ÷4) |
|---|---|---|---|
| ~0.5M | 477,312 | ≈1.9 MB | ≈0.48 MB |
| ~2M | 2,281,728 | ≈9.1 MB | ≈2.3 MB |
| ~10M | 9,946,880 | ≈39.8 MB | ≈9.9 MB |

All three land comfortably inside the size class of assets browsers already routinely fetch and
cache (ternlight's own shipped packages, §2, are 5–7 MB and are explicitly marketed as
browser-embeddable). Even the ~10M-param model at fp32 (≈40MB) is not disqualifying by itself, and
INT8 brings every tier here under 10MB.

**Memory-mapping: real on native, structurally unavailable on `wasm32-unknown-unknown`.**

- **Divvun's own precedent, verified directly from source rather than asserted**: `divvunspell`'s
  `Cargo.toml` (read directly, `[M]`) lists **both** `memmap2` (v0.9.4, workspace dependency) and
  `mmap-io` (v0.9.4, direct dependency) — confirming report 00's asserted-but-previously-unverified
  claim that Divvun memory-maps its FSTs on native targets. The same repository's own wasm-facing
  component (`support/accuracy-viewer`, a standalone Trunk-built wasm app) is called out in the repo
  structure as explicitly **excluded from the native workspace** — i.e. Divvun itself does not carry
  the mmap-based native code path into its own wasm build, corroborating that this is a known,
  already-navigated fork in their own architecture, not a hypothetical one. `[M]`
- **Why `wasm32-unknown-unknown` can't do this**: `mmap` is a POSIX/OS-level syscall (`[M, general
  fact]`); the `memmap2` crate — searched directly for wasm32 support — has **no
  `wasm32-unknown-unknown` support**, because the target itself has no filesystem/OS-mapping
  primitive to call into (`wasm32-unknown-unknown`'s own platform-support documentation states that
  "many pieces of functionality that require an operating system do not work and will return
  errors" `[M]`). This is a hard platform-capability gap, not a missing-crate-feature gap — no Rust
  crate can paper over the absence of the underlying syscall on this target.
- **What Emscripten/WASI offer, and why it's not the relevant path here**: Emscripten (its own
  runtime, distinct from `wasm32-unknown-unknown`) provides an `mmap`-*emulation* layer backed by
  its virtual filesystem, and WASI (`wasm32-wasi`/`wasm32-wasip1`/`wasip2`) provides real
  filesystem-adjacent capabilities through its capability-based I/O model — but PanGloss's `pg-wasm`
  crate already targets **`wasm32-unknown-unknown`** specifically (confirmed directly from
  `rust/crates/pg-wasm/Cargo.toml`'s `cfg(target_arch = "wasm32")` conditioning and its
  `wasm-bindgen`/`web-sys` dependency shape, which is the browser-via-`wasm-bindgen` path, not the
  WASI path) `[M]`. Switching targets to gain emulated mmap would be a much larger architectural
  change than the reranker feature justifies, and — consistent with `ort`'s own `wasm32-wasi`
  rejection (§1) — the Rust ML-inference ecosystem is not well set up for the WASI target regardless.
- **Concrete recommended loading strategy for WASM specifically, `[S]`**: load the safetensors blob
  as a plain byte buffer — `include_bytes!` at compile time if the weights are bundled directly into
  the wasm binary (ternlight's own approach, §2, "no runtime model download"), or fetched at runtime
  via the browser `fetch`/`ArrayBuffer` API and handed across the `wasm-bindgen` boundary as a
  `Vec<u8>`/`Uint8Array` if `.pgpack` loading is already streamed rather than compiled-in (matching
  however PanGloss already loads the rest of a `.pgpack`'s FST/runtime-data payload — this report did
  not re-derive that existing mechanism, and the reranker weights should simply reuse it rather than
  invent a second loading path). Either way, this is a **heap-resident `Vec`/`ArrayBuffer`, not a
  memory-mapped view** — there is no way around that on `wasm32-unknown-unknown`, but at the sizes in
  the table above (sub-1MB to ~10MB even unquantized-worst-case) this is a non-issue: a one-time
  linear-memory copy of a few megabytes is not a meaningful cost next to the multi-millisecond
  per-candidate forward-pass costs already estimated in §3, and `safetensors`' own zero-copy design
  means the *Rust-side* view into that buffer (tensor shape/dtype/offset metadata → slice into the
  byte buffer) still avoids a second copy even without OS-level mmap — the "zero-copy" property
  safetensors is designed around survives the move from mmap'd-file to heap-`Vec` just fine, since
  it was never actually dependent on mmap specifically, only on "don't deserialize/copy the tensor
  bytes themselves."

---

## HEADLINE

- **`[M]`** A serious, GPL-3.0-or-later Rust port of VISL CG-3 (`github.com/divvun/cg3-rs`, by a
  genuine long-standing Divvun engineer) exists as of four days before this report's research date
  — claiming full grammar-source and binary-ABI compatibility with upstream — but its license is
  very likely incompatible with linking into PanGloss's MIT-licensed WASM binary, which has no
  separate-process escape hatch the way native deployment does.
- **`[S]`** A tag vocabulary (≈300 entries) collapses embedding+output-projection from ~31–47% of
  parameters (GPT-2's real vocabulary, tied/untied) to under ~16% even in the smallest tested
  config and under 1% at the ~10M-param scale — the reranker's parameter and compute budget is spent
  almost entirely on transformer compute, not a lookup table, the opposite regime from a normal
  small LM.
- **`[A]`** No published WASM latency benchmark exists anywhere for a sub-5M-parameter,
  tag-vocabulary transformer specifically (confirmed, not just re-asserted) — but a real, shipped,
  Rust/WASM-SIMD-compiled, comparably-scaled encoder (`ternlight`) runs a full forward pass in
  ~2.5–5ms in-browser today, which is the closest available calibration point and lands a
  5–50-candidate reranking pass inside a plausible interactive-UI latency budget.

**VERDICT: yes-with-conditions.** A tag-vocabulary mini-transformer reranker fits a bounded WASM
envelope with today's Rust ecosystem — `candle` and `burn` both give genuine, evidenced wasm32
inference paths; the parameter arithmetic makes the model itself small (sub-10MB even
unquantized-worst-case at ~10M params, sub-2.5MB at ~2M params after INT8); `safetensors` gives a
mature, zero-copy-friendly format that survives the loss of mmap on `wasm32-unknown-unknown`
cleanly; and every purely-engineering component in the port inventory (tokenizer, calibration,
transformer inference itself) is either trivial or already exists. The conditions: (1) the model
must actually be trained and measured before any latency claim is treated as more than calibration
— nothing in this report is a substitute for that; (2) if CG-3 is wanted in the same WASM binary as
the reranker, the `cg3-rs` GPL question needs a real legal answer, not an assumption, before relying
on it, and a from-scratch MIT engine (now considerably easier to scope thanks to `cg3-rs` existing
as a reference) is the fallback; (3) any latency budget that assumes WASM threads or WebGPU needs a
CPU-SIMD-only fallback path, since both threading (COOP/COEP, host-page-dependent) and WebGPU
(browser-support-dependent) are conditions PanGloss's own WASM binary does not fully control.

**Single biggest technical risk**: not a missing crate or a missing browser API — it is that **no
one has measured `gemm`-heavy transformer inference under WASM's bounds-checking cost on the
specific short-sequence, small-`d_model`, small-batch shape this reranker actually has.** The one
directly relevant overhead number found (§2, `[A]`, unverified against the primary PDF) puts
bounds-checking overhead on `gemm`-like workloads as high as 220%, and this reranker's small matrix
dimensions (`d_model` 64–320) sit exactly in the regime where per-call fixed overhead (bounds checks,
lack of SIMD gather/scatter for embedding lookups, WASM function-call overhead relative to tiny
matmul sizes) is proportionally largest and least likely to be amortized the way it is in
large-matrix, large-batch benchmarks like the Xenova table in §2. This is the one number in the
entire report that is both load-bearing for the latency verdict and has zero direct measurement at
the relevant scale anywhere in public literature — it is the first thing to benchmark once a real
model exists, not the last.
