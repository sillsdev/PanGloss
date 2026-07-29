# Structured analysis identity is a self-contained value

A structured analysis identity is an ordered list of stable source keys plus a root index and a
category key, carried in the artifact as values. It is never a reference resolved against a compiled
model, and never contains compiler-assigned dense ordinals.

## Why

The primary use of grammar comparison is telling a linguist what changed after they edited a
grammar. Deleting or renaming a morpheme is one of the most ordinary edits there is. If identity
were a reference into a live model, every analysis mentioning a deleted morpheme would fail to
resolve, and the tool would return "cannot compare" precisely when the grammar changed most — the
one case it exists to explain. As a value, the same situation is trivial: baseline holds the
identity, candidate does not, so it is `removed`.

This also makes reports durable. An assessment artifact stays fully interpretable years later, when
neither grammar still compiles and no model can be loaded to resolve anything against.

## Considered Options

Resolving identity against the compiled model at comparison time was the obvious alternative, and it
is what the existing code shape invites: `pg_parse::WordAnalysis` carries `morpheme_ids: Vec<u32>`
and `pos_id: Option<u32>`, which are dense compiler-assigned indices, and its derived `Eq` compares
them directly. That equality is fine for same-model work and is load-bearing for `pg_lexicon`'s
`push_unique` deduplication, so it stays — but it must not be mistaken for analysis identity.
`define-grammar-coverage-contract` already forbids dense ordinals as cross-engine identity keys for
the same underlying reason.

## Consequences

Every identity must be projected to stable source keys before it enters an artifact. Those keys
exist today: `MorphemeInfo.xml_key` holds the MSA GUID on the LibLCM path and the `id` attribute on
HC XML, and part-of-speech symbols carry stable ids. Variant entries and null affixes use
synthesized composite keys, which are stable so long as their source GUIDs are.

Reports therefore carry key strings rather than integers. Interning them into a top-level table
recovers most of the size, and digests are computed over the expanded form so table ordering cannot
affect them.

A key that is missing from one side is ordinary evidence. A key that collides *within one model*
remains an integrity error.
