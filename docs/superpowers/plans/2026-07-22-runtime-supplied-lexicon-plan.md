# Runtime Supplied Lexicon Implementation Plan

Date: 2026-07-22
Design: `docs/superpowers/specs/2026-07-22-runtime-supplied-lexicon-design.md`

## Delivery strategy

Implement on branch `feature/runtime-supplied-lexicon` in an isolated worktree based on design commit `fe4b741`. Preserve every unrelated dirty or untracked file in the main checkout. Use test-driven development for every behavior change and commit at each phase boundary.

The dependency direction remains acyclic:

```text
pg-grammar -> pg-rules -> pg-parse
     |                       |
     +-----------------------+-> pg-lexicon -> pg-ffi / pg-wasm
```

`pg-parse` owns low-level overlay root references, lookup traits, provenance, and immutable overlay tries. `pg-lexicon` owns catalog construction, persistent supplied entries, CRUD, reconciliation, runtime snapshots, classification, and guides. Bindings own handles and orchestration.

## Phase 1: preserve authored grammar identities

Modify `pg-grammar` model, XML loader, snapshot compiler, and tests so every runtime-indexed MPR retains its authored XML ID alongside its name. Confirm POS, syntactic-feature, feature-value, and lexical-entry authored IDs survive both XML loading and snapshot compilation. Keep hot-path numeric IDs and existing names for compatibility.

Write failing tests first for duplicate display names with distinct IDs, rename stability, declaration-order stability, and lexical GUID extraction. Add canonical accessors needed by signature construction.

Commit: `grammar: preserve authored class identities`

## Phase 2: exact class catalog

Replace the legacy POS/MPR-name `ClassCandidate` model with typed `ClassSignature`, `SignatureId`, `ResolvedSignature`, and `ClassCatalog` models in `pg-lexicon`.

Build signatures only from official entries that expose at least one ordinary literal, free, unrestricted root and are not partial. Include authored POS identity, the complete lexical syntactic feature structure, and the complete MPR membership set. Deduplicate exact canonical signatures only. Derive deterministic `sig_` IDs from a documented canonical encoding and cryptographic digest; never use Rust's process-dependent hash.

Write failing tests for exact deduplication, subset/superset separation, excluded bound/partial/pattern entries, rename stability, authored-ID changes, XML reorder stability, and a golden canonical encoding.

Commit: `lexicon: derive exact class signatures`

## Phase 3: supplied-entry store

Replace `UserLexEntry` and `UserLexicon` with typed entry, revision, authority, validation, date, ID, request, response, and structured-error models. Implement injected ID and clock sources. Generate `pgl_` plus 22 unpadded base64url characters from 128 random bits. Use the shared .NET GUID conversion and golden cross-language vectors.

Implement add, get, list, simple scan search, atomic full replacement update, hard remove, clear, gloss-language changes, and explicit authority changes. Require a store gloss language before accepting a nonblank gloss. Preserve LibLCM `dateCreated`/`dateModified` semantics and format. Implement optional optimistic `expectedRevision` checks and no-op detection.

Write failing tests for ID format and nondeterminism injection, GUID vectors, dates, gloss invariants, homographs, atomic validation, ID/date preservation, revision changes, conflicts, and no-op behavior.

Commit: `lexicon: add revisioned supplied-entry store`

## Phase 4: parser overlay and provenance

Add low-level immutable overlay types to `pg-parse` without introducing a dependency on `pg-lexicon`. Generalize the root lookup boundary to return either grammar roots or supplied-root payloads. Preserve grammar numeric IDs for official roots and carry supplied identity, lexical spelling, gloss, resolved syntactic features, MPR set, stratum, and authority explicitly for overlay roots.

Search the composite base-plus-overlay index in ordinary lexical lookup and the non-head root filter used by compounding. Generalize the guessed-root side-channel and every validity/rendering/generation resolution site that assumes grammar-backed root IDs. Add `AnalysisProvenance` to `WordAnalysis` and internal words. Ensure supplied identity participates in deduplication so homographs survive.

Write failing tests for NFD equivalence, alternate representations, homographs, ordinary inflection, compound heads and non-heads, provenance variants, override suppression, override removal, and a differential fixture comparing an overlay entry with the same entry compiled into Machine/HermitCrab.

Commit: `parse: support supplied-root overlays`

## Phase 5: persistence, reconciliation, and runtime snapshots

Build `SuppliedLexiconRuntime` around an immutable `Arc` snapshot published under a short write lock. Parse operations clone one snapshot before work and never hold the store lock during analysis. Mutations validate and build replacement entries and overlay tries before atomic publication.

Define the versioned export document with grammar name, source fingerprint, optional gloss language, referenced-signature mapping table, entries, authority, validation, and dates. Compute a deterministic build fingerprint from a canonical grammar-source representation; harmless source-only changes may cause revalidation but never data loss. Import remains replace-only and atomic.

Reconcile matching-name/different-fingerprint imports entry by entry. Retain incompatible entries inactive. Detect official lexical identities that share the supplied entry's 128 bits and mark them superseded. Refresh active labels, retain inactive labels, and support explicit complete supplied overrides.

Write failing tests for round trips, schema rejection, grammar-name hard stops, changed fingerprints, duplicates, conflicting mappings, missing signatures, invalid shapes, promotion detection, override authority, label handling, atomic failure, and concurrent snapshot consistency.

Commit: `lexicon: add atomic persistence and reconciliation`

## Phase 6: classification matrix and guide

Replace exemplar-based and single-rule paradigm generation with a typed `ClassificationMatrix`. Filter the catalog from known facts, generate real multi-rule forms breadth-first, aggregate identical surface words, and record predicting signatures and grammatical metadata. Stop when every distinguishable pair has a separator or a derivation, candidate, step, time, or 128-form safety budget fires. Report precise truncation and never infer equivalence from truncated work.

Implement grammar-independent Rust `ClassificationGuide` state over a matrix: yes/no/unknown, replace answer, undo, remaining signatures, adaptive next form, all useful forms, and final selection. Keep the core operation stateless and revalidate selections during add.

Write failing tests for known facts, multi-rule discovery, aggregation, strict judgments, undo/replacement, adaptive selection, determinism, every budget, the 128 ceiling, and selection revalidation.

Commit: `lexicon: add classification matrix and guide`

## Phase 7: shared analysis orchestration

Create one orchestration path used by native and WASM bindings:

1. confirm official foma proposals;
2. run overlay-aware Morpher analysis with unconstrained guessing disabled;
3. suppress official entries selected by explicit overrides;
4. union and provenance-aware deduplicate official and supplied analyses;
5. run the ordinary guesser only on a total union miss.

Do not inject supplied roots into grammar-only foma owner tables. Overlay edits must not compile or replace the official foma network. Include the overlay revision in cached results and invalidate all stale caller cache entries, including after gloss-only edits. Remove unconditional lowercasing from lexical identity paths.

Write foma union, total-miss guess, override, cache-revision, spelling, and no-recompile regressions.

Commit: `analysis: union official and supplied lexical paths`

## Phase 8: native C JSON API

Refactor the native grammar handle to own immutable grammar/proposer/catalog state plus the revisioned supplied runtime. Avoid a permanently stored Morpher tied to a stale overlay; construct a snapshot-aware Morpher per call or cache only overlay-independent parser data.

Add a shared length-delimited UTF-8 JSON call helper with panic containment, empty output on failure, structured error envelopes, and existing PanGloss-owned buffer freeing. Expose meaningful functions for catalog, CRUD, import/export, authority, classification, and guide operations. Keep or deliberately version existing binary parse APIs; add a JSON analysis surface for provenance rather than silently changing old layouts. Bump the ABI version.

Write null, UTF-8, malformed JSON, panic, buffer ownership, every-operation, guide-lifetime, concurrency, provenance, and existing ABI regression tests.

Commit: `ffi: expose supplied lexicon JSON APIs`

## Phase 9: WASM API parity

Make `PanGlossGrammar` own the same catalog and supplied runtime and delegate to the same typed Rust operations as C. Expose corresponding camelCase methods and a WASM `ClassificationGuide` wrapper. Serialize maps as JSON-compatible objects.

Delete `candidateClasses`, `disambiguatingForms`, `applyUserLexicon`, XML augmentation, grammar reload, and foma recompilation. Add overlay revision to analysis cache input/output and ignore stale cache entries. Preserve authored token text.

Write normalized JSON conformance fixtures shared with native tests, structured-error parity, provenance, cache, foma union, no-lowercasing, and no-recompile tests. Run the wasm32 check and Node smoke.

Commit: `wasm: expose native-equivalent supplied lexicon APIs`

## Phase 10: remove legacy code and add host smokes

Delete XML augmentation and the old model/tests. Run an `rg` removal gate for `UserLexicon`, `UserLexEntry`, `class_key`, `augment_xml`, `applyUserLexicon`, `candidateClasses`, and `disambiguatingForms` in production code.

Add small Python and C# native-ABI examples that load a grammar, inspect the catalog, add a supplied entry, parse an inflected form with supplied provenance, export, remove, import, parse again, exercise a revision conflict, and free every handle/buffer.

Commit: `test: certify supplied lexicon binding parity`

## Review and verification gates

After each phase, run its focused tests and review the commit for spec compliance before proceeding. After all phases, Luna performs a full spec review and Terra performs a code-quality and integration review. Fix every finding, re-run both reviews, and commit the fixes.

Final verification from `rust/`:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p pg-grammar --release
cargo test -p pg-parse --release
cargo test -p pg-lexicon --release
cargo test -p pg-foma --release
cargo test -p pg-ffi --release
cargo test -p pg-wasm --release
cargo test --workspace --release
cargo check -p pg-wasm --target wasm32-unknown-unknown
wasm-pack build crates/pg-wasm --target nodejs --dev --out-dir pkg
node tools/f4-wasm-smoke.js
cargo build -p pg-ffi --release
```

Then run the new Python and C# native ABI smokes against the release library. Verify the plan requirement-by-requirement against code, tests, binding outputs, and commit history before marking the work complete.

## Plan changes discovered during archaeology

The approved design did not expose three implementation prerequisites. This plan adds them without changing product behavior:

1. Preserve authored MPR IDs before computing stable signatures; the current grammar model stores only MPR display names.
2. Put overlay primitives in `pg-parse` and management in `pg-lexicon` to avoid a dependency cycle.
3. Query the overlay from compounding's non-head root filter as well as ordinary lexical lookup.

The plan also makes the native binary parse ABI explicitly compatible or versioned while adding the required JSON/provenance surface. It does not silently change the existing binary layout.
