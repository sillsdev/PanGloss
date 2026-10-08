# Grammar facts are derived parser evidence

**Status:** accepted

## Context

A language owner can inspect which grammar statements PanGloss retained and which it omitted,
without running words through the parser. PanGloss publishes those facts as a rebuildable file;
tools such as Motif combine them with human evidence and decide what to recommend.

Parsimony clients need stable joins from source GUIDs to authored grammar facts and to PanGloss's
conversion decisions. The mutable parser statistics cache has a different lifetime and purpose.
Facts also need to remain interpretable when compilation refuses or a source section is not yet
exported.

## Decision

PanGloss owns a separate `pg-facts` crate and publishes versioned SQLite artifacts from exact
Snapshot bytes plus an explicit, closed Baseline/Dry-Run context. Artifacts carry distinct exact
source, semantic grammar, model, context and producer identities. The writer stores authored rows
separately from typed inventory and load-decision rows. Decisions stay with the importer/compiler
that made them; the facts crate serializes those decisions and does not derive refusal reasons from
diagnostic prose. A v7 artifact may also contain exactly one immutable stats run identified by its
closed cache digest and versioned manifest. That run is tied to the same Snapshot, model, compiler
build and production compile options as the facts. The stats cache remains the mutable history and
collection format; facts preserve one validated run with explicit word completion, counter support,
and source GUID bridges.

Publication uses a same-directory temporary database, one transaction, integrity checks and
no-clobber atomic publication. A schema change requires a new schema version and rebuild from the
original inputs. There are no migrations and the writer never replaces an existing artifact.
Structured compiler refusal can still produce authored and diagnostic facts while marking the
effective section unavailable.

## Consequences

- `pg-stats` remains the mutable home for run collection and history; a facts file contains either
  no run or one manifest-bound frozen projection, never a live cache attachment.
- Every consumer checks section availability and load dispositions before interpreting absence as
  an empty grammar or a successful load.
- Facts do not contain linguistic judgements, recommendations, parse analyses, or Proposal data.
- Static facts remain readable without parser measurements. Run-specific stats are optional, tied
  to the exact facts identities, and carry explicit word completion, support, and source mappings.
- Unimplemented sections remain explicitly unavailable; a schema version change requires rebuilding
  from the exact original Snapshot, context, cache, and manifest inputs.
