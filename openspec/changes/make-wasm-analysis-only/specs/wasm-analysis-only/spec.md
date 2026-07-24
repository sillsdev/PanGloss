## ADDED Requirements

### Requirement: Native tooling is the sole compilation authority
Grammar and FST compilation SHALL occur only in native PanGloss tooling under the effective native
resource envelope. The WASM target SHALL neither link nor export compiler construction.

#### Scenario: Browser caller supplies grammar XML
- **WHEN** a browser caller attempts to load grammar source
- **THEN** the WASM interface rejects it and performs no compilation

### Requirement: Hosts discover exported capabilities
The C ABI and corresponding WASM bindings SHALL expose a versioned capability query. Capability
identifiers SHALL distinguish inference, Rust-HermitCrab diagnostics, FST compilation, and grammar
comparison. An operation absent from the current build SHALL return typed
`unsupported_capability`; WASM SHALL NOT export a dormant compiler operation.

#### Scenario: WASM host queries capabilities
- **WHEN** a WASM host queries its capability profile
- **THEN** inference and packaged Rust-HermitCrab analysis are reported as available while FST
  compilation and native reference validation are absent

### Requirement: WASM loads compatible analysis artifacts
WASM SHALL construct its analyzer only from one complete, self-contained, versioned analysis file
containing a precompiled FST proposer network and the matching runtime grammar data used by the Rust
HermitCrab port. A whole-package fingerprint SHALL bind both payloads. Compatibility SHALL be decided
by load-time runtime-feature containment (the pack manifest's required runtime-feature set is a
subset of the Runtime's provided set), not by an engine-compatibility-identifier equality check.

#### Scenario: Compatible artifact is loaded
- **WHEN** the artifact version, grammar fingerprint, required-runtime-feature set, proposer, and
  confirmation data all validate — with the required set contained in the Runtime's provided set
- **THEN** WASM enables bounded analysis without recompiling the network

#### Scenario: Both analysis stages run
- **WHEN** a compatible package is loaded and a word is analyzed
- **THEN** the precompiled FST proposes candidates and the Rust HermitCrab port consumes the matched
  runtime grammar payload to confirm and complete the analysis

#### Scenario: Caller explicitly requests HermitCrab-only diagnostics
- **WHEN** a compatible package is loaded and the caller selects the HermitCrab-only pipeline
- **THEN** WASM uses the packaged runtime grammar data without invoking the FST proposer, applies the
  shared budgets, and returns the selected pipeline plus detailed parse-failure diagnostics

#### Scenario: Artifact components do not match
- **WHEN** any version, fingerprint, payload, or confirmation-data validation fails
- **THEN** loading fails closed with a typed error and no partial analyzer remains usable

#### Scenario: Proposer and HermitCrab data come from different grammars
- **WHEN** either payload does not match the package fingerprint and grammar identity
- **THEN** the entire file is rejected and neither engine state becomes usable

### Requirement: Analysis packages use one bounded binary container
Container v1 SHALL contain fixed PanGloss magic bytes, a container version, a length-prefixed
canonical JSON pack manifest, a length-prefixed Rust HermitCrab runtime payload, a length-prefixed
foma binary payload in the existing foma encoding, and a SHA-256 digest covering the exact framed pack
manifest and payload bytes. The format SHALL specify one integer byte order. The pack manifest SHALL
carry a required-runtime-feature-set field (ADR 0004), an ADR 0005 capability-trust stamp, and an
FST-health admission/findings field reconciled with `add-fst-compilation-health-audit`'s schema.

The container SHALL be a data-only PanGloss Language Pack. It SHALL NOT contain an embedded WASM
module, native library, script, or dynamically executable extension. All executable behavior SHALL
come from the installed PanGloss Runtime.

#### Scenario: Declared section length is unsafe
- **WHEN** a section length overflows, exceeds its versioned limit, exceeds the total package limit,
  or extends beyond the available bytes
- **THEN** loading fails before allocating that section or constructing either engine

#### Scenario: Package contains trailing bytes
- **WHEN** bytes remain after the defined final digest
- **THEN** loading fails as a non-canonical package

#### Scenario: Package declares executable extension content
- **WHEN** a package contains or declares executable plugin content
- **THEN** loading fails before constructing either engine

### Requirement: Integrity is distinct from authorization
The SHA-256 package digest SHALL be described and used as an integrity check only. Licensing and
publisher authenticity metadata SHALL occupy a separately versioned pack manifest section and SHALL
NOT treat a public hash or a secret embedded in WASM as proof of authorization.

#### Scenario: Content is rehashed by an untrusted party
- **WHEN** modified content carries a newly computed valid SHA-256 digest but lacks a valid optional
  publisher signature
- **THEN** integrity may pass but the package is not reported as validly signed

### Requirement: License declarations do not enforce analysis permission
An analysis package MAY declare an open, commercial, or namespaced license classification and MAY be
unsigned. The declaration SHALL describe WASM deployment/provenance only and SHALL NOT enable,
disable, restrict, or authorize FieldWorks or WASM analysis.

#### Scenario: Unsigned commercial declaration is loaded
- **WHEN** a structurally valid package declares `commercial` and contains no publisher signature
- **THEN** the loader reports the declaration and `unsigned` status and permits analysis

#### Scenario: A declaration violates an expected licensing rule
- **WHEN** the host or a person considers the declaration missing, inconsistent, or unauthorized
- **THEN** PanGloss still permits analysis and exposes the metadata for that host or person to assess

### Requirement: Publisher signatures are optional offline provenance
Packages MAY carry an Ed25519 publisher signature over a domain-separated canonical representation
of the container version, pack manifest excluding its signature value, and both framed payloads. Signing
SHALL use an external private key; verification SHALL require no secret, entitlement, account, or
network service.

#### Scenario: Signature verifies
- **WHEN** an optional signature verifies against its declared public key or configured key ID
- **THEN** the loader reports `valid` and permits analysis

#### Scenario: Signature is invalid
- **WHEN** structural integrity passes but the optional publisher signature does not verify
- **THEN** the loader reports `invalid`, retains the declared metadata for inspection, and permits analysis

#### Scenario: Runtime is offline
- **WHEN** a signed or unsigned package is loaded without network access
- **THEN** license declaration reporting and all analysis behavior remain available without a license server

### Requirement: Stem inputs do not mutate the engine
Separately supplied stem data MAY participate in analysis through the declared analysis-data
boundary, but SHALL NOT cause WASM to construct, modify, or recompile the proposer engine.

#### Scenario: Stem data changes
- **WHEN** a caller supplies a different valid stem dataset
- **THEN** WASM applies it through the analysis-data interface without exposing or invoking a compiler

### Requirement: WASM application remains bounded
Artifact-based WASM analysis SHALL enforce deterministic per-word input, path, output, candidate,
and elapsed-work budgets.

#### Scenario: Artifact analysis exceeds a logical budget
- **WHEN** one analysis crosses an effective application budget
- **THEN** it terminates with a typed budget outcome without invoking any compiler or alternate strategy

### Requirement: Runtime model handles are isolated and immutable
Native and WASM Runtime APIs SHALL permit multiple concurrently loaded Language Pack handles. Each
analysis request SHALL explicitly select one handle and own independent scratch, budgets, trace, and
cancellation state. There SHALL be no mutable process-global active language. Native completed-model
handles SHALL be safe for concurrent analysis; an active compilation session remains single-owner
until it yields the immutable model.

#### Scenario: A multilingual host loads two packs
- **WHEN** requests analyze words concurrently against different pack handles
- **THEN** neither pack or request can mutate or select the other's model, budgets, or trace state
