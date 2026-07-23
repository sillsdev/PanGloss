## 1. Pin the artifact contract

- [ ] 1.1 Implement container v1 with fixed magic, fixed integer byte order, length-prefixed canonical
      JSON manifest, Rust HermitCrab runtime payload, existing foma binary payload, and trailing
      SHA-256 digest over the exact framed manifest/payload bytes
- [ ] 1.2 Specify typed failures for unknown version, incompatible engine, mismatched grammar/data,
      corrupt network bytes, and absent confirmation data
- [ ] 1.3 Define versioned per-section and total byte limits; reject overflow, oversize declarations,
      truncation, and trailing bytes before payload allocation
- [ ] 1.4 Define optional license-declaration fields for license class/identifier/text or reference,
      publisher, and package identity; keep unknown namespaced declarations round-trippable
- [ ] 1.5 Define the optional Ed25519 block, domain-separated signed bytes, public-key/key-ID handling,
      and `unsigned`/`valid`/`invalid` reporting without authorization semantics
- [ ] 1.6 Reject executable extension sections or declarations; container v1 is a data-only
      PanGloss Language Pack whose behavior is supplied solely by PanGloss Runtime

## 2. Native artifact production

- [ ] 2.1 Expose the existing foma binary-memory writer through a native one-file package builder
- [ ] 2.2 Serialize the matching Rust HermitCrab runtime grammar payload into the same package
- [ ] 2.3 Produce packages only after supervised FST compilation and all size/resource checks succeed
- [ ] 2.4 Add deterministic round-trip, cross-payload mismatch, and fingerprint-mismatch tests
- [ ] 2.5 Add an offline native `sign package with private key` operation; never place a private key or
      shared secret in the package or WASM runtime

## 3. WASM analysis-only loading

- [ ] 3.1 Load and validate the complete one-file package before creating either engine state
- [ ] 3.2 Replace grammar-XML construction and lexicon-triggered recompilation APIs with artifact/data
      loading operations
- [ ] 3.3 Remove compiler construction code and dependencies from the WASM target
- [ ] 3.4 Integrate separately supplied stems only through the analysis-data boundary
- [ ] 3.5 Expose explicit combined and HermitCrab-only analysis selection from the same package;
      return named-pipeline parse diagnostics and never switch pipelines automatically
- [ ] 3.6 Expose a versioned native-C/WASM capability query; omit compiler exports from WASM and
      return typed `unsupported_capability` for operations absent from a build
- [ ] 3.7 Return isolated immutable model handles, require every request to select one, remove any
      global active-language state, and test native concurrent multi-pack analysis

## 4. Boundary verification

- [ ] 4.1 Prove native and WASM execute equivalent FST-propose plus Rust-HermitCrab-confirm/full-analysis
      results from the same package
- [ ] 4.2 Add a build/export audit that fails if a compiler constructor or compile API reaches WASM
- [ ] 4.3 Test malformed, stale, mismatched, oversized, and unsupported artifacts fail closed
- [ ] 4.4 Verify WASM analysis remains subject to per-word path/output/candidate/time budgets
- [ ] 4.5 Add golden byte fixtures proving native and WASM readers agree on canonical manifest bytes,
      integer byte order, section boundaries, digest coverage, and rejection behavior
- [ ] 4.6 Prove unsigned, validly signed, and invalidly signed packages all remain analyzable while
      exposing distinct signature status and the unchanged license declaration
- [ ] 4.7 Prove package loading and analysis perform no license-server or entitlement network request
- [ ] 4.8 Prove HermitCrab-only analysis uses packaged grammar data, shared budgets, and diagnostic
      outcomes without invoking the FST or any compiler
