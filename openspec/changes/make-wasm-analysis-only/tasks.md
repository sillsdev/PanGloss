**Status note:** the `.pgpack` container + manifest (section 1) and the WASM *loading* side of section
3 are real and landed in `pg-pack`/`pg-wasm`. The part of this change's own name — removing the
compiler from WASM (3.2/3.3 and everything downstream in section 4) — is confirmed **not done**:
`pg-wasm/Cargo.toml` still depends on `pg-foma` directly and `pg-wasm/src/lib.rs` still constructs
`pg_foma::composite::FomaAnalyzer` at runtime (e.g. `PanGlossGrammar::new`/`apply_user_lexicon`).
Native artifact production (section 2) is only exercised over synthetic byte payloads, not real
compiled foma/HC artifacts, so it is not yet "native artifact production" in the sense this task means.

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
- [ ] 3.2 Replace grammar-XML construction and lexicon-triggered recompilation APIs with artifact/data
      loading operations
      (not done — `pg-wasm/src/lib.rs` still builds `pg_foma::composite::FomaAnalyzer` at
      `PanGlossGrammar::new`/`apply_user_lexicon` time)
- [ ] 3.3 Remove compiler construction code and dependencies from the WASM target
      (**not done** — confirmed: `pg-wasm/Cargo.toml` still depends on `pg-foma` directly; this is the
      change's own stated NOT-done item per `STAGING.md`)
- [x] 3.6 Expose a versioned native-C/WASM capability query; omit compiler exports from WASM and
      return typed `unsupported_capability` for operations absent from a build
      (`pg-wasm/src/pack.rs::is_unproven`/trust-status match; the "omit compiler exports from WASM"
      half is not true yet per 3.3)

## 4. Boundary verification

- [ ] 4.2 Add a build/export audit that fails if a compiler constructor or compile API reaches WASM
      (not done — and would currently fail, since `pg-foma`'s compiler IS reachable from WASM today)


## Descoped 2026-08-06

Sixteen open tasks were removed from this change. They were real, but they were not this change: native
package building (2.x), signing and licensing behaviour, isolated model handles, byte-level reader
agreement, and artifact-failure cases. Bundling them is why this sat at 10 of 29 rather than finishing
the one thing that blocks a release.

What remains is the release blocker and nothing else: take the compiler out of the browser, and make
it impossible to put back. Task 4.2 is the load-bearing one — an audit that fails if a compiler
constructor or compile API reaches the browser build means nobody has to remember this before
shipping, because the build refuses. Write it first: it fails immediately, which converts a thing
someone must remember into a thing someone must fix.

The descoped items are recorded in `docs/open-questions.md` under G12 rather than lost.
