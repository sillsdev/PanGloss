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
- [ ] 1.6 Reject executable extension sections or declarations; container v1 is a data-only
      PanGloss Language Pack whose behavior is supplied solely by PanGloss Runtime
      (not verified — no extension-rejection check found)

## 2. Native artifact production

- [ ] 2.1 Expose the existing foma binary-memory writer through a native one-file package builder
      (not done — `write_pack` takes arbitrary `&[u8]` payloads; no producer function found that
      builds a real foma-compiled binary from a `Grammar`)
- [ ] 2.2 Serialize the matching Rust HermitCrab runtime grammar payload into the same package
      (not done — same gap, no real HC runtime payload producer found)
- [ ] 2.3 Produce packages only after supervised FST compilation and all size/resource checks succeed
      (not done — no such gate exists on the package-write path)
- [ ] 2.4 Add deterministic round-trip, cross-payload mismatch, and fingerprint-mismatch tests
      (partial — round-trip tests exist but only over synthetic byte arrays
      (`SYNTHETIC_RUNTIME_PAYLOAD`/`SYNTHETIC_FOMA_PAYLOAD`), not real compiled artifacts)
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
- [ ] 3.4 Integrate separately supplied stems only through the analysis-data boundary (not verified — blocked on 3.2/3.3)
- [ ] 3.5 Expose explicit combined and HermitCrab-only analysis selection from the same package;
      return named-pipeline parse diagnostics and never switch pipelines automatically (not done)
- [x] 3.6 Expose a versioned native-C/WASM capability query; omit compiler exports from WASM and
      return typed `unsupported_capability` for operations absent from a build
      (`pg-wasm/src/pack.rs::is_unproven`/trust-status match; the "omit compiler exports from WASM"
      half is not true yet per 3.3)
- [ ] 3.7 Return isolated immutable model handles, require every request to select one, remove any
      global active-language state, and test native concurrent multi-pack analysis (not verified)

## 4. Boundary verification

- [ ] 4.1 Prove native and WASM execute equivalent FST-propose plus Rust-HermitCrab-confirm/full-analysis
      results from the same package (not done — depends on 3.2/3.3)
- [ ] 4.2 Add a build/export audit that fails if a compiler constructor or compile API reaches WASM
      (not done — and would currently fail, since `pg-foma`'s compiler IS reachable from WASM today)
- [ ] 4.3 Test malformed, stale, mismatched, oversized, and unsupported artifacts fail closed
      (partial — `pg-pack`-level malformed/oversized/version tests exist; WASM-specific artifact
      rejection not separately confirmed)
- [ ] 4.4 Verify WASM analysis remains subject to per-word path/output/candidate/time budgets (not verified)
- [ ] 4.5 Add golden byte fixtures proving native and WASM readers agree on canonical pack-manifest
      bytes, integer byte order, section boundaries, digest coverage, and rejection behavior (not verified)
- [ ] 4.9 Prove an older Runtime loads a pack whose required-runtime-feature set it fully provides,
      and refuses (with a typed incompatibility, not a crash) a pack requiring a feature it lacks
      (the `required ⊆ provided` containment check exists (`pg-pack/src/compat.rs`) but this specific
      round-trip proof was not separately confirmed)
- [ ] 4.6 Prove unsigned, validly signed, and invalidly signed packages all remain analyzable while
      exposing distinct signature status and the unchanged license declaration (not verified end to end)
- [ ] 4.7 Prove package loading and analysis perform no license-server or entitlement network request (not verified; no networking code found, but not proven)
- [ ] 4.8 Prove HermitCrab-only analysis uses packaged grammar data, shared budgets, and diagnostic
      outcomes without invoking the FST or any compiler (not done — no HermitCrab-only WASM pipeline exists yet)
