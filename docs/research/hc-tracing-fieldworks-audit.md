# HermitCrab tracing and FieldWorks “why did this not parse?” audit

Date: 2026-07-22

## Conclusion

PanGloss should reuse HermitCrab's established trace semantics rather than invent a new diagnostic model. Machine defines a tree of analysis and synthesis events; FieldWorks supplies the production-grade explanation adapter, enriching those events with LibLCM identities and failure details and presenting the tree as an expandable path browser. The Rust port already mirrors most of Machine's tree and failure-reason model.

The narrow improvement PanGloss needs is resource containment around that same model: bounded trace collection, explicit trace-completeness metadata, and a machine-readable projection with stable grammar/source identities. Truncating a trace must not change or invalidate an otherwise complete analysis result. PanGloss should not replace the tree with sampling by default or invent causal explanations.

## What users do today

FieldWorks' **Try a Word** dialog lets a user request a trace and optionally select the morphemes/MSAs that the search should consider. It dispatches the word, trace flag, and selected identifiers asynchronously (`../FieldWorks/Src/LexText/ParserUI/TryAWordDlg.cs:434-455`). `HCParser` turns that selection into `LexEntrySelector` and `RuleSelector` predicates, enables the Morpher trace manager, parses, and embeds the returned trace under the word's XML result (`../FieldWorks/Src/LexText/ParserCore/HCParser.cs:178-218`). This is an important precedent: the established answer to an overwhelming trace is a caller-requested, linguistically meaningful restricted rerun, not lossy interpretation of an arbitrary sample.

The FieldWorks help text explains the diagnostic workflow as following colored paths through reverse phonology, root/stem matching, affix removal, compounding, lexical lookup, forward synthesis, forward phonology, and final surface-form comparison. Green paths reach a successful parse; red paths do not; failure reasons appear at the ends of paths (`../FieldWorks/Src/Transforms/Presentation/FormatHCTrace.xsl:1410-1458`). The rendered HTML has expandable path controls and links grammar objects back into Language Explorer (`FormatHCTrace.xsl:755-876`, `1410-1497`).

Thus “why didn't this parse?” is answered by showing where candidate paths survived or died and the locally known reason—not by producing one inferred root cause.

## Machine's trace model

Machine's generic trace is an ordered, bidirectional tree. Each node records:

- a `TraceType` (word analysis/synthesis, stratum input/output, template input/output, morphological or phonological rule analysis/synthesis, lexical lookup, blocking, success, or failure);
- an optional rule/source object;
- an optional subrule index;
- optional input and output `Word` values;
- an optional `FailureReason`;
- ordered child nodes.

These fields and event kinds are defined in `../machine/src/SIL.Machine.Morphology.HermitCrab/Trace.cs:8-155`. `TraceManager` appends events under `Word.CurrentTrace`; a successfully applied morphological rule moves that word's trace cursor to the new rule node so subsequent work nests under it (`../machine/src/SIL.Machine.Morphology.HermitCrab/TraceManager.cs:68-78`, `218-227`). This cursor behavior is central to reconstructing search paths.

Machine exposes 24 non-sentinel failure reasons, including environments, allomorph/morpheme co-occurrence, surface mismatch, patterns, syntactic features, MPR features, stem names, partial parses, bound roots, template-order restrictions, and application-count exhaustion (`../machine/src/SIL.Machine.Morphology.HermitCrab/ITraceManager.cs:3-29`). Call sites supply a free-form `failureObj` alongside the enum when richer explanation is possible.

Tracing is opt-in. Ordinary parsing checks `ITraceManager.IsTracing` before creating the root (`../machine/src/SIL.Machine.Morphology.HermitCrab/Morpher.cs:95-124`). Tracing is not merely passive logging: Machine disables equivalent-analysis merging while tracing so distinct exploratory paths remain visible (`../machine/src/SIL.Machine.Morphology.HermitCrab/AnalysisStratumRule.cs:104-124`).

## FieldWorks' production adapter

FieldWorks implements `ITraceManager` as `FwXmlTraceManager`, building XML directly rather than first building Machine's generic `Trace` objects (`../FieldWorks/Src/LexText/ParserCore/FwXmlTraceManager.cs:20-39`). It retains the same broad event/path structure but deliberately omits some generic events: stratum begin/end and unsuccessful analysis-rule unapplication are no-ops (`FwXmlTraceManager.cs:41-47`, `91-93`), and blocking is also omitted (`FwXmlTraceManager.cs:329-331`). Therefore Machine's generic tree and FieldWorks' user-facing XML are related projections, not byte- or node-for-node equivalents.

The adapter's major value is its semantic enrichment. It translates generic failures into specific XML explanations, for example:

- phonological-rule category or MPR-feature mismatches (`FwXmlTraceManager.cs:175-215`);
- affix POS/inflection-feature, stem-name, required/excluded inflection-type, environment/pattern, maximum-application, and template-order failures (`FwXmlTraceManager.cs:244-326`);
- final co-occurrence, environment, surface mismatch, inflection-feature, stem-name, bound-root, disjunctive-allomorph, and partial-parse failures (`FwXmlTraceManager.cs:339-421`).

It resolves morphemes through LibLCM MSA and inflection-type identifiers and creates FieldWorks-specific morpheme elements (`FwXmlTraceManager.cs:473-485`). Rule elements use an MSA-backed numeric ID for morphemes and zero otherwise (`FwXmlTraceManager.cs:488-501`). Those IDs are useful for the live UI, but HVO-like process/project identifiers are not suitable as durable PanGloss interchange identities; PanGloss' separate analysis-identity work should continue to use retained GUID/source keys.

`HCTrace` transforms the returned XML through `FormatHCTrace.xsl` into a temporary HTML result page (`../FieldWorks/Src/LexText/ParserUI/HCTrace.cs:15-37`). The XML is therefore already a serializable diagnostic artifact, but it is a FieldWorks presentation contract, not a canonical cross-engine protocol.

## Filtering, laziness, and resource behavior

The parse call returns `IEnumerable<Word>`, but the core search materializes important intermediate collections; the trace itself is retained as an in-memory tree/XML document until rendering. FieldWorks invokes the parse on its parser worker and returns the completed `XDocument`; the UI only polls for asynchronous completion (`../FieldWorks/Src/LexText/ParserCore/ParserWorker.cs:103-119`; `../FieldWorks/Src/LexText/ParserUI/TryAWordDlg.cs:549-579`). The HTML display can hide/collapse details, but that is presentation after collection, not lazy trace generation (`FormatHCTrace.xsl:1495-1498`).

Machine includes `MaxUnapplications`, specifically documented as a way to debug words whose excessive unapplications can take 30 minutes; zero means the default is not limited (`../machine/src/SIL.Machine.Morphology.HermitCrab/Morpher.cs:56-79`; `AnalysisStratumRule.cs:142-145`). FieldWorks' shown parser setup does not assign that property (`../FieldWorks/Src/LexText/ParserCore/HCParser.cs:144-176`). Neither Machine's `TraceManager` nor FieldWorks' `FwXmlTraceManager` has a trace-node or trace-byte cap, truncation status, or sampling strategy.

Tracing can generate more work than ordinary parsing because equivalent analyses are deliberately not merged. Consequently, diagnostics can amplify the pathology they are intended to explain. This is the concrete reason PanGloss needs trace-specific containment in addition to analysis budgets.

## What the Rust port already mirrors

Rust's `pg-rules::trace` explicitly ports Machine's 19 real trace types and 24 real failure reasons by name, replaces polymorphic C# sources with a closed `TraceSource` enum, and models the trace cursor as an arena handle (`rust/crates/pg-rules/src/trace.rs:36-145`). Its `TraceSink`/`NoopSink` design checks tracing before cloning words or computing failure details, preserving the zero-cost-when-off intent (`trace.rs:10-28`, `162-188`). `TreeTraceSink` stores the same ordered tree shape in an arena and reproduces Machine's cursor semantics (`trace.rs:546-600`).

The Rust CLI already renders both human-readable text and nested JSON. Its JSON is intended for structural comparison of trace type, source/rule identity, and failure reason rather than whitespace (`rust/crates/pg-cli/src/trace_render.rs:1-10`, `138-177`). Tests and source comments document live C# comparisons and deliberate fixes to match C# exploration behavior (`trace_render.rs:248-318`).

Known differences are already documented in the Rust implementation: it drops C#'s free-form `failureObj`, although FieldWorks demonstrates that this object is important for high-quality explanations; it snapshots complete `Word` objects rather than a compact diagnostic projection (`rust/crates/pg-rules/src/trace.rs:18-28`). Rust's `TreeTraceSink` is also unbounded: it appends nodes to a `Vec` with no event/byte limit (`trace.rs:550-600`).

## Reuse and narrow improvements

### Reuse unchanged

1. Treat Machine's event tree, ordering/cursor semantics, and `FailureReason` vocabulary as the reference domain model.
2. Preserve the FieldWorks principle of enriching a generic failure reason with the concrete competing feature, environment, allomorph, rule, or constraint when the engine knows it.
3. Preserve complete paths under normal diagnostic limits; do not replace them with inferred causal claims.
4. Support a caller-selected morpheme/rule restriction for an explicit diagnostic rerun, analogous to FieldWorks' `LexEntrySelector`/`RuleSelector` flow.
5. Keep human presentation outside the core; expose structured trace data that FieldWorks, AI tools, or other applications can render.

### Improve narrowly

1. Add a bounded trace sink using the shared runtime budget policy. Bound at least node count and serialized diagnostic bytes. The bound must be high enough for ordinary Try-a-Word use and caller-adjustable only within the shared absolute ceiling.
2. When the trace bound is reached, stop collecting further trace events but allow semantic analysis to continue under its independent analysis budget. Mark the trace `truncated`, report the exact limit and omitted-event count when knowable, and never label the semantic result incomplete merely because its explanation is incomplete.
3. Prefer deterministic prefix retention over sampling for “why did this fail?”: parent/child continuity and the first observed dead ends are intelligible, whereas detached sampled nodes are not. If later evidence justifies representative sampling, add it as a separate explicit mode, not as the default.
4. Add typed, structured failure detail to Rust where FieldWorks already proves its diagnostic value. Do this incrementally from actual C#/FieldWorks cases; do not recreate LibLCM's renderer or add speculative fields.
5. Emit stable source identities alongside names. Use grammar XML keys or retained LibLCM GUIDs for machine interchange; keep runtime ordinals/HVOs only as local debugging annotations.
6. Version the machine trace schema. The existing nested JSON is a useful starting point, but authoritative delta tooling needs explicit schema version, trace completeness, effective trace limits, selected filters, and engine/package context.

## Direct answer to the planning question

Yes, trace collection should be separately bounded and may truncate without invalidating a complete semantic result—but this should be described as a containment improvement to the battle-tested Machine/FieldWorks trace workflow, not a new breadcrumb system. The default retained structure should remain a coherent ordered trace tree. The most valuable additional feature is FieldWorks-style caller-selected restriction followed by an explicit rerun; arbitrary path sampling should not be the primary diagnostic interface.
