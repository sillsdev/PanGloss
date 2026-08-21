# Transient PanGloss Project Cache Design

**Status:** Approved design; pending implementation planning.

## Objective

Motif must be able to hand PanGloss a FieldWorks project state, release the live
FieldWorks project, and ask PanGloss to compile and assess that same state a few
minutes later. Capturing the state should finish as quickly as practical.

PanGloss will create an opaque, immutable, short-lived project cache from either
a `.fwdata` project or a live LibLCM `LcmCache`. The cache contains everything
PanGloss needs to rebuild its engines and analyze captured texts later. It is an
internal cache, not a durable interchange format or archival representation.

FLExText ingestion and persistent compiled-engine caching are outside this
design.

## Ownership boundary

PanGloss owns:

- extracting and normalizing the required FieldWorks data;
- the cache's internal layout and compatibility fingerprint;
- atomic publication and integrity checks;
- rebuilding selected backends from the cache; and
- executing Motif's later analysis request and producing PanGloss reports.

Motif owns:

- the destination supplied for cache creation;
- the cache's lifetime and deletion;
- the later job command;
- selection of cached texts or occurrences to analyze;
- the current test expectations;
- backend and execution options; and
- requested report destinations.

This boundary lets Motif revise its selection or expectations and rerun a job
without reopening FieldWorks or recreating the project cache. Any change to the
captured grammar or source texts requires a new cache.

## Cache contents

The cache stores PanGloss-owned normalized data, including:

- the complete grammar state needed by the PanGloss compiler;
- relevant project metadata and writing-system data;
- stable FieldWorks identities needed for analysis identity and provenance;
- captured texts, occurrences, and approved analyses;
- source and capture options; and
- an integrity manifest with content digests and the PanGloss internal-format
  fingerprint.

The cache does not store a compiled Foma network, a constructed Rust HC engine,
or other backend runtime state. The later job builds its requested backends from
the normalized cache. This keeps capture fast and keeps the cache independent of
backend choices made by Motif later.

The internal files and DTOs are private implementation details. Motif treats the
cache directory as an opaque handle and does not read or modify its contents.

For capture purposes, the required data closure is precise even though its DTO
layout is private: it is the existing PanGloss compiler input plus every object
and value transitively referenced by a captured text occurrence or its approved
analysis, including the writing systems and stable identities used by those
values. Data used only by FieldWorks UI or editing workflows is excluded. The
implementation plan must map this semantic closure to the current PanGloss
project model and text/assessment DTOs; changing that private mapping does not
change Motif's contract.

## Text capture and later selection

Cache creation accepts an optional allowlist of stable FieldWorks text GUIDs.

- If the allowlist is omitted, PanGloss captures every relevant text.
- If it is present, PanGloss copies only the named texts and the information
  required to interpret them.
- An explicitly empty allowlist creates a grammar-only cache.

A selected text includes all of its paragraphs, segments, token occurrences,
and approved analyses. Duplicate GUIDs in the request are harmless and are
normalized to one selection. Any unknown GUID makes capture fail with all
unknown GUIDs listed. “Every relevant text” means every project text represented
by FieldWorks' text repositories that contains or can contain analyzable
occurrences; non-text UI state and media payloads are not captured.

This allowlist is only a capture-size optimization. It does not define the
assessment corpus and does not prune unrelated grammar state. Motif's later job
request selects the cached texts or occurrences to analyze and supplies the
expectations to apply.

If a job names a text or occurrence that is absent from the cache, PanGloss
rejects the complete request before compilation or analysis. The diagnostic
lists all missing identifiers. PanGloss never silently skips missing inputs or
produces a partial coverage report.

## Capture from a live `LcmCache`

The managed integration exposes an asynchronous capture operation conceptually
equivalent to:

```csharp
Task<CaptureResult> CaptureAsync(
    LcmCache source,
    string destination,
    CaptureOptions options,
    CancellationToken cancellationToken);
```

LibLCM synchronizes the whole object model with a reader/writer lock. PanGloss
runs extraction on a worker thread inside `WorkerThreadReadHelper`, obtained
through `IWorkerThreadReadHandler`. The read scope waits for an active write
unit of work to finish and prevents a new write unit of work while PanGloss
traverses the required object graph.

Within that scope PanGloss must:

- materialize lazy repository enumerations before their backing collections can
  change;
- traverse every required LibLCM property;
- copy values into immutable PanGloss-owned DTOs; and
- retain no LibLCM objects, interfaces, or lazy enumerables.

PanGloss releases the LibLCM read lock before serialization, hashing, and file
I/O. The caller does not need to stop editing globally, but must invoke capture
after the current UI unit of work and must not block the UI thread waiting
synchronously for the worker. A short pause in new edits during extraction is
an accepted consistency cost.

The PanGloss managed facade creates and owns the worker task and read scope. The
caller must keep the `LcmCache` alive until `CaptureAsync` completes and must not
dispose it concurrently. Cancellation may stop waiting for the read scope or
stop extraction at safe checkpoints. The immutable DTO boundary is reached
inside the read scope: when that scope ends, no returned object may retain a
LibLCM reference or lazy enumeration.

## Capture from `.fwdata`

The CLI accepts a `.fwdata` project and produces the same normalized cache as the
managed `LcmCache` path. The two paths share the normalization and validation
pipeline after their source adapters.

The intended command shape is:

```text
pangloss cache create PROJECT.fwdata DESTINATION [--text-guid GUID ...]
```

The managed API is required for a live `LcmCache`; a native CLI process cannot
receive an in-process managed object.

## Validation and failure policy

Capture validates only what is necessary to publish a complete, internally
self-consistent cache. It does not compile a backend, run capability-envelope
checks, or reject ordinary grammar defects. Those defects are captured and
reported by the later build.

Capture fails when, for example:

- a requested source text does not exist;
- required referenced data cannot be resolved;
- the `LcmCache` becomes unusable or is disposed;
- normalized data violates the private cache invariants;
- serialization, hashing, or publication fails; or
- cancellation is requested before publication.

An unresolved reference is a capture failure only when it belongs to the
required data closure defined above. A malformed or unsupported grammar value
that PanGloss can faithfully represent in its compiler input is captured and
left for the later build to diagnose.

Failure diagnostics distinguish source-integrity, destination, cancellation,
and I/O failures from grammar diagnostics deferred to build time.

## Atomic publication

The destination's parent directory must already exist. The destination itself
must not exist. An existing file or directory is a hard error; PanGloss never
overwrites or merges a cache.

PanGloss writes to a uniquely named temporary sibling of the destination so the
temporary data and destination reside on the same filesystem. It serializes and
hashes every payload, writes the final manifest, verifies the complete cache,
and then atomically renames the temporary sibling to the destination.

The final rename is also the exclusive destination reservation. If concurrent
creators target the same destination, exactly one rename can succeed and every
loser reports that the destination already exists. A cache becomes valid only
when the complete manifest and payload have been published at the requested
destination. Cancellation observed before the rename fails capture; once the
rename succeeds, capture succeeds even if cancellation is requested
simultaneously.

On an ordinary failure or cancellation, PanGloss removes its unpublished
temporary data when possible. A crash may leave an unmistakably temporary
sibling, but never a valid destination. Only a successfully published directory
is usable as a cache.

Temporary siblings use a PanGloss-specific name plus a unique nonce. PanGloss
does not automatically remove stale siblings from other capture attempts;
Motif may remove them only when no capture is active. Atomic visibility is
required, but power-loss durability beyond the guarantees of the destination
filesystem is outside this design.

## Compatibility

The cache has no public schema version, migration path, or backwards-
compatibility promise. Its manifest records a PanGloss internal-format
fingerprint. A consumer whose expected fingerprint differs rejects the cache
before reading its payload and reports that the temporary cache belongs to an
incompatible PanGloss format and must be recreated.

The fingerprint is a safety guard, not a public interchange contract. PanGloss
is the only writer and reader. It is derived from the private normalized-data
contract and its serializer/deserializer expectations, not from project content
or the selected runtime backend. Equivalent builds may share a fingerprint;
incompatible builds must not.

## Later job execution

The later job consumes:

- the immutable cache destination;
- Motif's selected text or occurrence identifiers;
- Motif's current expectations;
- backend, budget, and execution options; and
- output locations.

PanGloss first verifies the manifest, fingerprint, and requested identifiers. It
then reconstructs its normalized project model, builds the requested backends,
analyzes the selected inputs, applies the supplied expectations, and writes the
normal PanGloss report and log artifacts.

The cache is read-only and supports multiple concurrent jobs. Each job owns its
compiled runtime state and output paths; no job mutates the shared cache.
Output destinations must be distinct and must not equal, contain, or reside
inside the cache destination; PanGloss rejects overlapping paths before work
begins. Motif must not delete the cache until capture has completed and every
consuming job has ended; deletion racing with a job is caller misuse and may
fail that job.

## Testing

Implementation verification must cover:

1. equivalent normalized output from `.fwdata` and a live `LcmCache`;
2. capture of all texts, a GUID allowlist, and an explicitly empty allowlist;
3. preservation of stable identities, approved analyses, and writing systems;
4. consistent worker-thread capture while FieldWorks writes occur before and
   after the LibLCM read scope;
5. release of the LibLCM lock before serialization and file I/O;
6. rejection of incomplete source state without converting grammar defects into
   capture failures;
7. atomic success, cancellation, I/O failure, and crash-shaped incomplete data;
8. hard failure for an existing destination;
9. rejection of a mismatched internal-format fingerprint;
10. hard failure listing every requested but uncached text or occurrence;
11. rerunning revised Motif expectations against the same immutable cache; and
12. concurrent read-only jobs with independent runtime state and outputs.

## Operational exclusions

This first delivery does not provide cache migration, encryption, access-control
management, relocation across filesystems, automatic expiry or garbage
collection, recovery of unpublished temporary siblings, power-loss durability,
or protection against Motif deleting an in-use cache. Normal operating-system
file permissions still apply.

## Rejected alternatives

### FLExText as the cache boundary

FLExText is an established external text format, but it is unnecessary when
PanGloss receives the `.fwdata` project or the live `LcmCache` directly. It is
not part of this first delivery.

### A public, versioned PanGloss interchange format

Motif does not need to understand or preserve the cache. Publishing its schema
would create compatibility and migration obligations without serving the
short-lived workflow.

### Compiled backend artifacts

Capturing compiled engines would lengthen the foreground operation, bind the
cache to backend choices, and add runtime compatibility concerns. Rebuilding
from normalized state during the later job is the simpler contract while normal
builds remain acceptably fast.

### Saving partial source state

A later job cannot distinguish a deliberately absent object from a failed
capture. PanGloss therefore fails cache creation when it cannot produce a
complete snapshot of the requested scope.
