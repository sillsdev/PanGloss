**Status note:** the `.pgpack` container + manifest (section 1) and the WASM *loading* side of section
3 are real and landed in `pg-pack`/`pg-wasm`. The runtime `FomaAnalyzer`/FST compilation path is
gone. `PanGlossGrammar::new` still accepts grammar XML and initializes
`pg_lexicon::SuppliedLexiconRuntime`; `pg-wasm` still depends directly on `pg-foma` for runtime and
health data. The remaining 3.2/3.3/4.2 work is the artifact/data-only boundary, compiler-only
dependency cleanup, and a build/export reachability audit. Native artifact production (section 2) is
only exercised over synthetic byte payloads, not real compiled foma/HC artifacts, so it is not yet
"native artifact production" in the sense this task means.

## 1. Pin the artifact contract

- [x] 1.1 Implement container v1 with fixed magic, fixed integer byte order, length-prefixed canonical
      JSON pack manifest, Rust HermitCrab runtime payload, existing foma binary payload, and trailing
      SHA-256 digest over the exact framed pack-manifest/payload bytes
      (`pg-pack/src/format.rs::write_pack`/`read_pack`, versioned `VersionLimits`)
- [x] 1.1e Add the pack manifest's required-runtime-feature-set field (ADR 0004: payload-format
      version, required runtime operations, foma-feature level, HC-port semantic version, extensions)
      and implement the loader's `required ⊆ provided` containment check in place of any engine-
      compatibility-identifier equality check
      (`pg-pack/src/compat.rs`: `RequiredRuntimeFeatures`/`ProvidedRuntimeFeatures`; consumed by
      `pg-wasm/src/pack.rs`)
- [x] 1.1f Add the pack manifest's ADR 0005 capability-trust stamp (proven, or overridden/unproven
      plus override record) and reconcile its FST-health admission/findings field with
      `add-fst-compilation-health-audit`'s schema rather than defining a parallel one
      (`pg-pack/src/trust.rs`: `CapabilityTrust`/`CapabilityOverrideRecord`;
      `pg-wasm/src/pack.rs::is_unproven`. Note: `add-fst-compilation-health-audit`'s own admission
      wiring is not itself done yet — see that change's tasks.md — so "reconcile" is aspirational
      until that side exists too)
- [x] 1.2 Specify typed failures for unknown version, incompatible engine, mismatched grammar/data,
      corrupt network bytes, and absent confirmation data
      (`PackLoadError`/`PgPackError` enums)
- [x] 1.3 Define versioned per-section and total byte limits; reject overflow, oversize declarations,
      truncation, and trailing bytes before payload allocation
      (`format.rs::check_section_limit` against `VersionLimits`)
- [x] 1.4 Define optional license-declaration fields for license class/identifier/text or reference,
      publisher, and package identity; keep unknown namespaced declarations round-trippable
      (`pg-pack/src/license.rs`: `LicenseClass`/`LicenseDeclaration`)
- [x] 1.5 Define the optional Ed25519 block, domain-separated signed bytes, public-key/key-ID handling,
      and `unsigned`/`valid`/`invalid` reporting without authorization semantics
      (`pg-pack/src/signature.rs`, exercised by `format.rs` round-trip tests)

## 2. Native artifact production

- [x] 2.5 Add an offline native `sign package with private key` operation; never place a private key or
      shared secret in the package or WASM runtime
      (`pg-pack/src/signature.rs` sign/verify; no private key material in the package format)

## 3. WASM analysis-only loading

- [x] 3.1 Load and validate the complete one-file package before creating either engine state
      (`pg-wasm/src/pack.rs::load_pack`)
- [ ] 3.2 Replace grammar-XML construction with artifact/data loading operations
      (**open** — `PanGlossGrammar::new` still accepts grammar XML and initializes
      `pg_lexicon::SuppliedLexiconRuntime`; runtime `FomaAnalyzer`/FST compilation is already gone)
- [ ] 3.3 Remove compiler-only construction/dependency surface from the WASM target
      (**open** — `pg-wasm/Cargo.toml` still depends directly on `pg-foma` for runtime/health data;
      no compiler-reachability audit has established the final boundary)
- [x] 3.6 Expose a versioned native-C/WASM capability query; omit compiler exports from WASM and
      return typed `unsupported_capability` for operations absent from a build
      (`pg-wasm/src/pack.rs::is_unproven`/trust-status match; no compiler export is currently exposed,
      while dependency cleanup remains open in 3.3)

## 4. Boundary verification

- [ ] 4.2 Add a build/export audit that fails if a compiler constructor or compile API reaches WASM
      (**open** — runtime `FomaAnalyzer`/FST compilation is gone, but no audit proves that compiler
      reachability remains absent)


## Descoped 2026-08-06

Sixteen open tasks were removed from this change. They were real, but they were not this change: native
package building (2.x), signing and licensing behaviour, isolated model handles, byte-level reader
agreement, and artifact-failure cases. Bundling them is why this sat at 10 of 29 rather than finishing
the one thing that blocks a release.

What remains is the release blocker and nothing else: complete artifact/data-only loading, separate any
compiler-only dependency surface from the browser runtime, and make compiler reachability impossible
to reintroduce. Task 4.2 is the load-bearing one — an audit that fails if a compiler constructor or
compile API reaches the browser build means nobody has to remember this before shipping, because the
build refuses. Write it first: it fails immediately, which converts a thing someone must remember into
a thing someone must fix.

The descoped items are recorded in `docs/open-questions.md` under G12 rather than lost.
