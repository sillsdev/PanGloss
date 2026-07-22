# Runtime Supplied Lexicon Design

Date: 2026-07-22

## Purpose

PanGloss shall let an application add ordinary stems to a loaded grammar without reloading the grammar or recompiling its foma network. Supplied stems are first-class lexical data with persistent identity, gloss metadata, grammatical class signatures, CRUD operations, native and WASM bindings, and explicit provenance in analyses.

This feature is a glorified spell-checker add-on. The official grammar remains the source for categories, rules, exceptional words, bound roots, irregular allomorphs, and other specialist lexical behavior. A supplied entry adds one ordinary free literal stem to one or more grammatical signatures already observed in the official lexicon.

PanGloss owns supplied-entry IDs and runtime behavior. It does not own user accounts or durable storage. A host exports one JSON blob, stores it, and imports it into a new grammar handle later.

## Compatibility principles

SIL.Machine/HermitCrab defines morphological and segmented-shape behavior. LibLCM defines the conventions for writing-system-aware strings, stable lexical identity, and lexical dates. `docs/research/liblcm-machine-lexical-normalization.md` records the source audit behind these decisions.

PanGloss normalizes lookup input to NFD through the grammar's character-definition table, uses longest-match segmentation, and indexes the resulting shape. It preserves the authored spelling as lexical metadata. PanGloss performs no unconditional lowercasing or generic case folding.

Supplied entries use LibLCM-style `dateCreated` and `dateModified` semantics. PanGloss sets both on add and updates `dateModified` on content or authority changes. Persistence uses LibLCM's UTC `yyyy-MM-dd HH:mm:ss.fff` representation.

## Architecture

Each loaded grammar handle owns four layers:

1. The immutable compiled `Grammar`.
2. The immutable compiled foma proposer, when available.
3. An immutable `ClassCatalog` computed once from the grammar.
4. A mutable, revisioned `SuppliedLexicon` and its small root overlay index.

The existing `RootAllomorphIndex` becomes a composite lexical lookup boundary. The immutable base trie continues to point into grammar tables. A separate overlay trie maps segmented root shapes to supplied-entry IDs and resolved class data. CRUD operations rebuild or incrementally update only the overlay trie. They never mutate `Grammar`, reload XML, or compile foma.

Analysis follows two sources and then a fallback:

1. The compiled foma path produces official grammar analyses.
2. The authoritative Morpher analysis cascade searches the supplied overlay trie at its normal lexical-lookup stage and produces supplied analyses.
3. PanGloss unions the results. It runs the unconstrained guess-root fallback only when both sources return no analysis.

The overlay path recognizes inflected and compound forms because morphology is unapplied before trie lookup. A future auxiliary overlay proposer may optimize this path without changing the API.

Each analysis carries one provenance variant: `grammar`, `supplied { entryId }`, `suppliedOverride { entryId, overriddenGrammarEntryId }`, or `guessed`.

## Class catalog and signatures

PanGloss derives the class catalog by scanning official lexical entries. For each ordinary free, non-partial stem, it extracts the complete grammatical signature:

- authored part-of-speech identity;
- complete lexical syntactic feature structure;
- complete MPR/inflection-class membership set.

The catalog deduplicates only exact signatures. It never merges a signature with a subset or superset. It excludes signatures observed only on bound or partial stems. It copies no exemplar spelling, gloss, allomorph restriction, environment, stem name, co-occurrence rule, family, property, or irregular form.

Signature identity uses authored XML IDs or GUIDs for parts of speech, features, feature values, and inflection classes. Display names and abbreviations do not participate in identity. PanGloss derives a deterministic `sig_` ID from a canonical sorted encoding of those authored identities. Renaming a category preserves identity; deleting it and creating a new category does not.

The live catalog stores resolved signatures for fast recall. The persistent supplied-lexicon document contains a mapping table only for signatures referenced by stored entries. Entries refer to compact signature IDs; the table retains authored IDs and readable current or last-known names. Active labels refresh from the current grammar during export. Inactive signatures retain their stored labels.

## Supplied entries and identity

A supplied entry contains:

- an immutable PanGloss-owned ID;
- exact authored stem spelling;
- one optional plain gloss, represented by an empty string when absent;
- one or more exact signature IDs;
- LibLCM-style creation and modification dates;
- authority and validation state.

IDs have the form `pgl_` followed by an unpadded 22-character base64url encoding of 128 cryptographically random bits. They contain no user, grammar, time, or lexical information. Tests inject deterministic random bytes.

The same 128 bits may later become a LibLCM GUID during promotion. PanGloss must use the shared, documented conversion and account for .NET `Guid` byte-order rules. Promotion itself is outside this feature.

Multiple entries may share one segmented root key. PanGloss preserves homographs and entries with different glosses or classes. Morphological results use the spelling encountered in analyzed text and return the stored spelling as lexical metadata.

The supplied lexicon has one optional `glossLanguage` tag. Blank glosses are always valid. A nonblank gloss requires the store's gloss language to be set first. This feature does not support multilingual gloss alternatives.

## Authority and promotion

PanGloss compares each `pgl_` identity with official grammar lexical identities. When the same 128-bit identity appears in the official grammar, the official entry wins by default. PanGloss retains the supplied record as `superseded`, reports a structured comparison, excludes it from overlay analysis, and uses the official entry.

An explicit `setEntryAuthority` operation may choose `suppliedOverride`. PanGloss then suppresses the matching official lexical entry and substitutes the complete supplied record. The override stores the official entry identity and an optional note. Removing the supplied record removes the override and restores official authority.

PanGloss performs no automatic partial merge or closest-class migration.

## Persistence and grammar updates

The grammar remains immutable during a handle's lifetime. A grammar update creates a new handle and imports the previously exported supplied-lexicon document.

The HermitCrab format carries `Language.Name` but no stable language or project GUID. The persistent document therefore records canonical `grammarName` as its lineage key and a compiled grammar fingerprint as its build identity.

- A grammar-name mismatch rejects the complete import.
- A matching name and fingerprint permits direct restoration.
- A matching name and changed fingerprint triggers complete reconciliation.

Import is replace-only and atomic. PanGloss validates the complete structure before changing live state. Duplicate entry IDs, conflicting signature definitions, unsupported schema versions, malformed IDs, or a grammar-name mismatch reject the import and preserve the prior overlay and revision.

After structural validation, PanGloss stores every entry even when it cannot activate it. Missing signatures, invalid shapes, and other grammar incompatibilities produce inactive entries with structured diagnostics. Entries detected in the official grammar become superseded. The import result reports exact match, compatible migration, inactive entries, and superseded entries.

The JSON document includes `schemaVersion`, `grammarName`, `sourceGrammarFingerprint`, optional `glossLanguage`, the referenced-signature mapping table, and entries. PanGloss rejects unsupported newer schema versions rather than dropping unknown semantics.

## CRUD, revisions, and search

The core API provides add, get, list, search, update, remove, clear, import, export, class-catalog access, classification, and explicit authority changes.

PanGloss generates an entry ID only after add validation succeeds. Update replaces stem, gloss, and the complete signature-ID set atomically; it never changes the ID. Authority changes use a separate operation. Remove is a hard delete. Deleting an override restores official authority. Clear hard-deletes every supplied record.

Every overlay state has an opaque revision. Successful mutations return a new revision and invalidate all cached analyses, including gloss-only changes. Failed and no-op mutations preserve the revision. Mutations accept an optional `expectedRevision`; mismatches return a conflict without changing state. A parse snapshots one revision and cannot mix old and new overlay state.

`get` returns a record in any state. `list` and `search` include active, inactive, superseded, and override records by default. Search is a simple deterministic in-memory scan over stem and gloss with optional exact POS, signature, and state filters. The management API needs no pagination or separate search index for the expected scale, including stores around 10,000 entries.

## Classification and diagnostic forms

The core accepts a stem plus optional known grammatical facts and returns a complete `ClassificationMatrix`. It filters the immutable class catalog by the known facts, synthesizes diagnostic forms through the real grammar, and records which signatures generate each distinct surface word. The matrix includes grammatical metadata and deterministic form IDs. The core retains no temporary session state.

Generation explores bounded multi-rule derivations in breadth-first order. It stops when every distinguishable signature pair has a separating form or a safety budget fires. The engine returns at most 128 diagnostic forms. It also enforces derivation, candidate, step, and time budgets. Any limit returns `exhaustive: false` with the precise truncation reason. PanGloss never calls truncated candidates grammatically equivalent.

The Rust `ClassificationGuide` consumes a matrix. It records `yes`, `no`, and `unknown` judgments, supports replacing or undoing an answer, returns remaining candidates, selects the next useful form adaptively, exposes all useful forms for one-screen UIs, and emits a final signature selection. `no` strictly eliminates every signature that predicts the word; `yes` eliminates signatures that do not; `unknown` adds no constraint.

The guide is advisory. A host may ignore it and use the matrix directly. The authoritative add operation accepts signature IDs, revalidates them against the current catalog, and creates the entry. Multiple surviving signatures are valid and remain distinct.

## Rust, native, and WASM APIs

Rust uses typed domain models throughout. Serde defines one canonical, versioned JSON representation at binding boundaries.

The C ABI keeps opaque grammar and classification-guide handles. Complex requests, responses, reconciliation reports, and errors cross the ABI as UTF-8 JSON through PanGloss-owned buffer conventions. The ABI exposes a function per meaningful operation rather than a generic command dispatcher.

WASM exposes the same serde models as JavaScript objects and exposes `ClassificationGuide` as a JavaScript class. C and WASM call the same Rust operations and must pass binding-conformance tests.

The delivery includes small Python and C# examples or smoke tests that call the C JSON ABI. Full Python and C# SDKs remain separate work.

All new failures use a shared structured error object with stable `code`, diagnostic `message`, and operation-specific `details`. Human-facing applications may pretty-print or replace messages.

## Concurrency

The immutable grammar, proposer, and class catalog remain shareable across threads. A lock protects the revisioned supplied store and overlay index. Reads snapshot an immutable overlay state. Mutations build and validate replacement state before taking the write path and publish the new state atomically.

The existing native safety rule still forbids freeing a grammar handle while another call uses it. The new mutation APIs remain safe alongside concurrent parses on a live handle.

## Removed legacy behavior

This change deletes the old `UserLexicon`/`class_key` model, XML exemplar cloning, augmented-XML grammar reload, foma recompilation in `applyUserLexicon`, and the legacy WASM compatibility API. PanGloss-demo will adopt the new schema and operations. No migration path is required because the legacy feature has no production users.

## Verification

Tests must cover class-signature extraction and stability, NFD and alternate-representation lookup, homographs, ordinary inflection and compounding through the overlay trie, official/supplied/override/guess provenance, strict classification judgments, the 128-form safety limit, CRUD and revision conflicts, cache invalidation, import atomicity, inactive reconciliation, promotion detection, override suppression, dates, ID generation and GUID conversion, structured errors, and C/WASM equivalence.

Differential fixtures shall compare an overlay entry with the same entry compiled into SIL.Machine/HermitCrab wherever both representations support identical behavior. Full workspace verification must include Rust tests, WASM builds/tests, native FFI tests, and Python/C# ABI smoke examples.

## Out of scope

This feature does not author new grammatical categories, model exceptional or bound stems, manage users, persist data, synchronize devices, merge independent stores, implement promotion, provide full Python/C# SDKs, build UI wording, or dynamically mutate a loaded grammar. Spelling-correction systems may use these APIs later, but suggestions remain outside the supplied-entry schema until a host submits a validated add operation.
