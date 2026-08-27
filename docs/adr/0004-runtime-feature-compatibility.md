# Load-time compatibility: append-only runtime features, required-set containment

## Decision

Whether a Runtime may load a pack is decided by **runtime features, not grammar
constructs.** The Runtime is a stable interpreter (foma FST executor + Rust-HermitCrab
confirm) plus scaffolding; grammar constructs are lowered away by the compiler into generic
FST + runtime data. A pack stamps the **required runtime-feature set** it was built against
(payload-format version, the runtime *operations* its execution needs, foma-feature level,
HC-port semantic version, extensions); the Runtime declares the **provided** set; the pack
loads iff `required ⊆ provided`.

The provided set is **append-only**: it only ever grows, and an existing behavior is never
altered in place. A genuine non-additive change is a rare, **carefully staged major-version
event** at which grammars recompile — not a silent per-pack refusal and not a behavior swap
under an unchanged version.

## Why

Users download packs (at app startup, from a distribution site); old and new packs, built
against different runtime-capability versions, must coexist. There may be thousands of packs;
forcing every one to upgrade on each engine release is unacceptable. Append-only containment
makes "old packs run forever" arithmetic rather than a promise.

## Key consequences

- **Two directions, both defined.** Backward (old pack on newer engine): `required ⊆
  provided` always holds under append-only → **old packs run unchanged forever.** Forward
  (new pack on older engine): runs *iff* it requires no feature the old engine lacks;
  otherwise a typed, allowed incompatibility — "upgrade PanGloss to run this grammar." Most
  new packs use only stable features and run on older engines; only those using a new runtime
  feature require the upgrade.
- **Both anticipated change types are additive, so neither breaks old packs.** New usability/
  spell-check features (frequent) append to `provided`; old packs don't require them. A new
  foma/HC capability or a result-preserving speedup (rare) is likewise additive or
  transparent. Old packs keep requiring exactly what they always did.
- **The only pack-breaking change is altering an existing behavior in place — forbidden
  within a major line.** If a behavior genuinely must change (a foma/HC semantic correction —
  expected ~never), it is a staged **major-version** bump with coordinated grammar recompile.
  Old (previous-major) packs are never silently run under new-major semantics.
- **Recompile is otherwise always a producer opt-in**, taken to pick up **compiler**
  improvements (smaller/faster FSTs, a newly-supported construct), never a load-time
  requirement forced on an old pack.
- **Distinct from ADR 0001.** Compile-time capability (can we faithfully *compile* this
  grammar? — about constructs, hard-fail, produces a pack) is separate from load-time
  compatibility (can this Runtime *run* this pack? — about runtime features, containment
  check). A pack's required-feature set is a small compiler byproduct: only constructs needing
  a runtime operation (e.g. reduplication → the query-time peel op) contribute, plus format
  version and extensions. Most constructs are fully lowered and impose no runtime requirement.
- **Manifest schema is thereby determined:** payload-format version + required
  runtime-feature/semantic-version set + FST-health admission/findings + identity/
  provenance. This fills the previously-missing WASM-manifest health-admission field. (There
  is no separate "certification" consumer of this provenance: correctness is proven by the
  in-repo conformance integration tests against committed ground truth — see ADR 0001 — not by
  a terminal stage that consumes prior reports. The manifest's identity/provenance serves pack
  identity and load-time compatibility, not a certification gate.)
