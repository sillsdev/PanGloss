# CopyProcess subrecipe dossier

## Scope

CopyProcess owns explicit prefix, suffix, full-stem, and internal-span copying; bounded copies with
a proven maximum span; productive unbounded copying as a runtime `Peeled` operation; copy-span
identity; root-only versus affix-inclusive scope; nested chain depth; and multiplicity.

**Non-scope:** arbitrary copying advertised as one ordinary one-way FST, generic structural
insertion, template legality, phonological order, and boundary cleanup. `max_span = None` means
unbounded span is preserved and peeled, not silently truncated.

## Languages and families in mind

- **Anchor 1 — Tagalog. Family: Austronesian. Construct:** partial initial-CV and base-related reduplication exercise
  bounded prefix/internal-span copying and distinguish it from whole-stem copying.
- **Anchor 2 — Indonesian. Family: Austronesian. Construct:** productive full prosodic-word/root copying with
  prefixes excluded from the copied portion exercises unbounded/root-span peeling and interaction
  with ordered phonology.
- **Anchor 3 — Urama (Kiwai, Trans-New Guinea):** productive full reduplication with root-only
  copying when an enclitic or affix is present independently exercises the root-span contract.
- **Scale anchor — Yoruba (Niger-Congo):** full-stem and partial initial-consonant copying with
  epenthetic `i` is an independent family row supplied by the research, but its primary source was
  not independently rechecked here.

The confidence level is high for the Tagalog/Indonesian/Urama construct roles in the completed research. The
formal regularity boundary is supported by the cited formal papers, but no claim is made that every
enriched finite-state representation is impossible. The main source uncertainty is that the Yoruba
primary study was supplied by the research output but not independently rechecked in this task.

## Primary sources

- [Tagalog reduplication](https://brill.com/downloadpdf/journals/bki/106/2/article-p151_2.pdf)
  for multiple copy spans.
- [Indonesian phonology](https://people.ucsc.edu/~ddbrodki/PDFs/Brodkin_Indonesian.pdf) for
  full-copy interaction with ordered phonology.
- [A Grammar of Urama](https://openresearch-repository.anu.edu.au/bitstream/1885/111328/3/BrownEtAl-2016-UramaGrammar.pdf)
  for productive root copying with affix/enclitic exclusion.
- [Walther, Finite-State Reduplication](https://arxiv.org/abs/cs/0005025) and [Beesley &
  Karttunen, Finite-State Non-Concatenative Morphotactics](https://arxiv.org/abs/cs/0006044) for
  the distinction between ordinary one-way transduction and enriched/restricted treatments.
- Repository evidence: [`peel.rs`](../../../rust/crates/pg-foma/src/peel.rs) and [`f6_reduplication_peel_chain_depth.rs`](../../../rust/crates/pg-foma/tests/f6_reduplication_peel_chain_depth.rs).

## Grammar facts

`CopyKind` is explicit. A span proof is required before using `ExactFst`; otherwise the process is
`Peeled` and oracle-confirmed. Root identity, source allomorph, copied span, morpheme order, root
index, and multiplicity survive peeling. An inert reduplication hint alone does not create a
CopyProcess node.

**Invariants:** chain and input length are bounded operationally; a cap trip is non-certifying;
prefix copies precede the base in morpheme order; suffix copies follow it; and productive copying
is never presented as exhaustive FST coverage merely because finite examples were enumerated.

## Formal model and regularity

Bounded copying can be represented by a finite relation when `k`, the maximum span, is proven. A
generic bounded-span FST may require `O(|Σ|^k)` states. Productive total copying has a different
formal boundary; the supplied Walther result motivates a special peeled path, while Beesley and
Karttunen show that restricted enriched finite-state alternatives exist.

**Correctness obligations:** peeled-plus-confirmed multisets equal the oracle for certified fixtures;
the residual sent to the proposer is exact; copied span and root index are retained; and a timeout,
chain cap, or branch cap is not interpreted as a semantic negative.

**Failure modes:** false repeated-substring peel, wrong residual, reversed prefix/suffix morpheme
order, root-index loss, nested self-similar branching explosion, excess multiplicity, and treating
an inert hint as executable copying.

## Chosen architecture

1. Extract typed `CopyProcessSpec` with `CopyKind`, optional proven `max_span`, and chain cap.
2. Use `ExactFst` only for bounded spans with a proof; use the existing peeler for productive spans.
3. Recurse through residual proposals with explicit depth/input/branch budgets.
4. Confirm the full HC multiset before making a result selectable.

## Rejected architectures

- Ordinary one-way FST for arbitrary full copy: it overstates the regularity boundary.
- Finite enumeration presented as productive coverage: it is bounded evidence, not a proof.
- Every reduplication hint becomes a copy node: hints can be inert and do not prove executable
  copying.
- Unbounded peeling without resource bounds: self-similar inputs can branch explosively.
- Language-specific copy branches: copy span and disposition are the reusable abstraction.

## Interfaces and interactions

Morphotactics supplies the base and morpheme order. StructuralAllomorph may provide a bounded
captured action, but productive copying remains distinct. OrderedPhonology consumes copied material
only after the correct stratum and boundary state are preserved. The edge must carry copy-span,
root-index, identity, multiplicity, and `Peeled` disposition.

## Complexity and resource bounds

**Big-O variables:** `n` = token length, `k` = proven bounded span, `|Σ|` = alphabet size, `r` =
copy rules, `h` = candidate split points, `d` = nested depth, and `b` = base candidate count.

**Time:** bounded FST state construction can be `O(|Σ|^k)`. One peeler layer scans `O(n)` split
points with indexed token comparison; naive repeated comparison can approach `O(n^2)`. Candidate
work is approximately `O(r · b · h)`. Nested self-similar inputs can reach `O(n^d)` before caps.

**Space:** bounded state storage is `O(|Σ|^k)` plus `O(r)`. Nested candidate storage can approach
`O(n^d · b)` before caps. A cap bounds operational work but does not turn an exhausted run into an
exact negative.

## Conformance fixtures

### Exercise 1 — bounded Tagalog CV copy

Positive: a documented initial-CV reduplicant such as `basa → babasa`; expected multiset preserves
the bare root and reduplicated analysis as distinct identities with the copied span marked. Negatives
are full-stem copy, two-CV copy, and a wrong base position. This is a proposed bounded-span row,
not a current production claim.

### Exercise 2 — productive Urama full-root copy

Positive: `horo → horohoro`, and root-only copying when an enclitic is present. Expected disposition
is `Peeled` and the confirmed multiset retains the enclitic outside the copied span. Negatives are
copied enclitic, mismatched copy, and depth-cap exhaustion; exhaustion must be non-certifying.
The repository's [`f6_reduplication_peel_chain_depth.rs`](../../../rust/crates/pg-foma/tests/f6_reduplication_peel_chain_depth.rs)
is an implementation analogue: `kimbiakimbia` has exact set/multiplicity on the witnessed row,
while deep self-similar input is refused deterministically.

## Implementation status

The peeler scans prefix, suffix, separator, and separator-plus-suffix-peel structures and preserves
residuals/root indices. Its module documents exact single-layer evidence and open depth ≥2 and
higher-multiplicity proof obligations. Current status: strongest of the three research paths,
implementation still bounded by the existing confirmation and budget evidence.

## Known gaps and split triggers

No checked-in conformance fixture proves depth ≥2 oracle containment or multiplicity beyond the
witnessed single analysis. The second full-copy family (Urama) closes the research obligation at the
anchor level, not the implementation gate. A split/add is required for non-token identity,
arbitrary nonlocal alignment, or a runtime operation not expressible as span metadata plus peeling.

**Trigger matrix:** `fits` for bounded copy with a span proof; `refines` for a new span kind,
multiplicity witness, or budget dimension; `splits/adds` for nonlocal/non-token copying or a new
runtime operation.

## Research log

| Date | Evidence and direct link | Consequence |
|---|---|---|
| 2026-08-01 | [Urama grammar](https://openresearch-repository.anu.edu.au/bitstream/1885/111328/3/BrownEtAl-2016-UramaGrammar.pdf) and [Walther](https://arxiv.org/abs/cs/0005025) | Root-span productive copying and the ordinary-FST boundary are separate claims. |
| 2026-08-01 | [peel.rs](../../../rust/crates/pg-foma/src/peel.rs) and [chain-depth gate](../../../rust/crates/pg-foma/tests/f6_reduplication_peel_chain_depth.rs) | Single-layer exactness and deterministic cap refusal are repository evidence; deeper containment remains open. |

## Evidence decisions

| Date | Decision | Evidence | Architectural consequence / trigger |
|---|---|---|---|
| 2026-08-01 | fits | Partial and full copy spans recur across Austronesian and the independent Urama family. | Keep one typed CopyProcess with explicit span kind. |
| 2026-08-01 | refines | The peeler proves a narrow single-layer witness, not all depth/multiplicity cases. | Keep `Peeled`, budgets, and full multiset confirmation explicit. |
| 2026-08-01 | splits/adds | Copying needs non-token identity or arbitrary nonlocal alignment. | Add a separate runtime mechanism; do not broaden ordinary FST claims. |
