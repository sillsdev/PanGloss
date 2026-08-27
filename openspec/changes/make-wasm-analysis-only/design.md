## Decisions

Execution order and exclusive ownership are governed by `openspec/changes/STAGING.md`.

- Native PanGloss tooling is the only compilation authority.
- The deployment artifact is exactly one self-contained file. Reuse foma's tested binary-memory
  representation inside a PanGloss envelope rather than inventing another network encoding.
- The product-facing artifact is a PanGloss Language Pack and is strictly data-only. Container v1
  has no section for WASM modules, native libraries, scripts, or dynamically executable extensions;
  engine behavior changes require a PanGloss Runtime release.
- The envelope (pack manifest) identifies its schema version, grammar fingerprint, precompiled
  proposer payload, and matched runtime grammar payload for the Rust HermitCrab port. Compatibility is
  **not** an engine-compatibility-identifier equality check; per
  `docs/adr/0004-runtime-feature-compatibility.md` the pack manifest stamps the **required
  runtime-feature set** it was built against (payload-format version, runtime operations its
  execution needs, foma-feature level, HC-port semantic version, extensions), the Runtime declares its
  **provided** set, and the pack loads iff `required ⊆ provided`. The provided set is append-only, so
  an old pack keeps loading on every newer Runtime without a version-equality bump.
- One package fingerprint covers the envelope and both payloads. The loader never accepts a proposer
  and HermitCrab grammar state from separate packages.
- Container v1 is, in order: fixed PanGloss magic bytes; an unsigned integer container version; a
  length-prefixed canonical UTF-8 JSON manifest; a length-prefixed Rust HermitCrab runtime payload;
  a length-prefixed existing foma binary payload; and a 32-byte SHA-256 digest over the exact manifest
  and payload framing bytes. Multi-byte integers use one specified byte order in the format module.
- The loader checks magic, version, integer overflow, each section limit, and total package limit
  before allocating or hashing payload storage. Trailing or truncated bytes fail closed.
- The pack manifest carries package/grammar identity, payload format versions, the required-
  runtime-feature set (ADR 0004), an FST-health admission/findings field reconciled with
  `add-fst-compilation-health-audit`'s finding schema and severity bands, creation metadata, and a
  versioned licensing/authenticity section. ("Pack
  manifest" — the per-`.pgpack` blob — is distinct from the source-controlled capability registry of
  ADR 0001; bare unqualified "manifest" is avoided throughout this design.) SHA-256 supplies structural
  integrity, not license authorization.
- Licensing is declaration and provenance only. The manifest may declare `open`, `commercial`, or a
  namespaced license class plus license identifier/text/reference and publisher metadata.
- A package may be unsigned or optionally signed with Ed25519. The signature covers a domain-separated
  canonical representation of the container version, manifest with an empty signature field, and
  both framed payloads. The signature block carries the algorithm, public key or key identifier, and
  signature bytes; signing tooling accepts the private key outside the package.
- The loader reports `unsigned`, `valid`, or `invalid` signature state. All three states may analyze;
  signature state and declarations are visible to the host and diagnostics. No entitlement, secret,
  network lookup, license server, feature restriction, or analysis refusal is part of this design.
- Licensing metadata applies to WASM package deployment/provenance. It does not mediate FieldWorks or
  other native analysis.
- WASM validates the complete envelope before constructing an analyzer. Unknown versions, a required-
  runtime-feature not in the Runtime's provided set, mismatched fingerprints, malformed payloads, and
  missing confirmation data fail closed.
- WASM exposes package loading and analysis only. FST compiler crates, FST constructor functions,
  grammar-XML-to-FST compilation, and lexicon-triggered FST recompilation are absent from its target
  dependency graph. Loading the packaged runtime grammar for the Rust HermitCrab engine does not
  construct or modify the FST.
- Separately developed stem support may replace or augment stem data through the agreed analysis-data
  boundary; it cannot mutate or rebuild the engine.

## Dependencies

The loader relies on the existing `fsm_read_binary_mem` behavior. Native production artifact output
depends on `harden-foma-resource-safety`. Work touching `pg-wasm` is serialized with the in-flight
stem-input work.

This change is reworked to `docs/adr/0004-runtime-feature-compatibility.md`'s load-time compatibility
model. The pack manifest's FST-health admission/findings field is reconciled with
`add-fst-compilation-health-audit`'s finding schema, stable codes, and severity bands rather than
defining its own; that change is the schema owner, this change is a consumer/carrier.
