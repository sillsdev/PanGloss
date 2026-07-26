"""The Rust-analyzer <-> Python-research interchange format.

Line-delimited JSON (JSONL), one JSON object per **token**, carrying the surface form,
explicit sentence/document boundary markers, and the **full list of analyses** the
analyzer produced for that token (never a single "best" analysis).

Why "full list of analyses" is load-bearing, not a nice-to-have
-----------------------------------------------------------------
`docs/research/spellcheck/PLAN.md` D4 marginalizes over the analysis lattice rather than
disambiguating first ("the n-gram scores over the analysis lattice ... rather than
requiring a hard disambiguation pass first"). A format that stores one analysis per token
would force disambiguation at export time, silently collapsing exactly the ambiguity D4 is
designed to sum over — see report 13's measured ambiguity (Sena: mean 4.61 analyses/word,
p90 9). So every :class:`Token` carries a (possibly empty, possibly length-1, normally
several-long) list of :class:`Analysis` records, and nothing in this module ever picks one.

Why per-token JSON lines rather than per-sentence
--------------------------------------------------
A per-sentence record (nested token list) is also reasonable, but per-token wins for this
project: it streams (constant memory over an arbitrarily large corpus, matching the Rust
side's own streaming `.fwdata` reader design -- see `pg-fwdata/src/xml.rs`'s doc comment),
it diffs and greps cleanly line-by-line, and boundary information (`doc_id`, `sentence_id`,
`position`, `is_sentence_start`/`is_sentence_end`) is cheap to carry per line and is all a
consumer needs to reconstruct sentence grouping without a nested structure. The in-memory
API (:func:`read_jsonl` / :class:`Sentence`) still groups tokens into sentences immediately
on load, so nothing downstream has to think in the flat/line-delimited representation.

Every field here is a direct, minimally-lossy projection of `WordAnalysis`
(`rust/crates/pg-parse/src/lib.rs`), per D1's load-bearing-factor criterion:

    WordAnalysis field      -> Analysis field
    pos_id                  -> pos            (resolved to the grammar's own POS symbol name)
    syn_fs (minus POS)      -> features       (flat feature-name -> value-name string map)
    morpheme_ids            -> morphemes      (ordered morpheme identities/labels)
    root_morpheme_index     -> stem           (the morpheme at that index, if known)
    guessed                 -> guessed
    (duplicate-count / provenance evidence) -> score  (a relative weight, NOT a probability;
                                                        see Analysis.score docstring)

See `research/docs/interchange-format.md` for the full schema and a worked example.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable, Iterator

FORMAT_NAME = "pangloss-spellcheck-research-jsonl"
FORMAT_VERSION = 1


@dataclass
class Analysis:
    """One candidate morphological analysis of a token.

    Mirrors `WordAnalysis` (`rust/crates/pg-parse/src/lib.rs:25-44`) field-for-field, per
    D1's load-bearing-factor criterion -- nothing here is semantic (no gloss, no sense, no
    domain); everything here is a deterministic function of the parse.
    """

    pos: str
    """The POS symbol name (e.g. "V", "N", "Vaux"). Never a numeric id -- ids are only
    stable within one compiled grammar, and this format must survive being read by code
    that never loaded that grammar."""

    features: dict[str, str] = field(default_factory=dict)
    """The `syn_fs` feature bundle, flattened to feature-name -> symbolic-value-name pairs,
    with POS itself excluded (POS has its own field above). Nested/complex feature
    structures (HermitCrab's `head`/`foot`) are flattened with a "/"-joined dotted key,
    e.g. `{"head/tense": "past"}`. Empty for an analysis with no features beyond POS --
    report 13 measured this is the *normal* case for some grammars (Indonesian: 0% of
    analyses carry anything beyond POS), so an empty dict must never be treated as missing
    data."""

    morphemes: list[str] = field(default_factory=list)
    """The ordered morpheme-identity sequence (`morpheme_ids`), as stable string labels
    (gloss IDs or morpheme names -- never a grammar-local integer). This is what the
    intra-word n-gram term (D4) is estimated over."""

    stem: str = ""
    """The root/stem morpheme's label, i.e. `morphemes[root_morpheme_index]` in the source
    representation. Duplicated out of `morphemes` for convenience since "which morpheme is
    the root" is itself load-bearing (D1: "prefix/suffix partition, affix counts on each
    side")."""

    score: float = 1.0
    """A relative weight for this analysis, NOT a calibrated probability. Multiple
    analyses of one token are weighted evidence for D4's lattice marginalization, but the
    weights are not required to sum to 1 -- a consumer normalizes if it needs a
    distribution. Default 1.0 (uniform) when the source has no better evidence (e.g. no
    corpus-frequency or duplicate-count signal available)."""

    guessed: bool = False
    """Mirrors `WordAnalysis.guessed` -- true iff this analysis came from an unknown-root
    guess branch rather than the shipped lexicon. A found asset per D1: a guessed analysis
    is not evidence of correctness and must never be scored as if it were a lexicon-backed
    one."""

    def feature_key(self) -> str:
        """A deterministic string key for this analysis' (pos, features) pair -- the
        thing D4's backoff rungs 2/3 group by. Sorted so key equality is content equality
        regardless of dict insertion order."""
        parts = ",".join(f"{k}={v}" for k, v in sorted(self.features.items()))
        return f"{self.pos}|{parts}"

    def to_dict(self) -> dict:
        return {
            "pos": self.pos,
            "features": dict(self.features),
            "morphemes": list(self.morphemes),
            "stem": self.stem,
            "score": self.score,
            "guessed": self.guessed,
        }

    @staticmethod
    def from_dict(d: dict) -> "Analysis":
        return Analysis(
            pos=d["pos"],
            features=dict(d.get("features", {})),
            morphemes=list(d.get("morphemes", [])),
            stem=d.get("stem", ""),
            score=float(d.get("score", 1.0)),
            guessed=bool(d.get("guessed", False)),
        )


@dataclass
class Token:
    """One token occurrence in running text, with its full analysis lattice attached.

    An empty `analyses` list is a **valid, meaningful state**: it means the analyzer
    produced zero confirmed analyses for this surface form (report 13's `zero_analyses`
    bucket) -- it is not the same as "not yet analyzed" and must not be silently dropped by
    a consumer that only checks `len(analyses) == 1`.
    """

    surface: str
    doc_id: str = "doc0"
    sentence_id: int = 0
    position: int = 0
    is_punct: bool = False
    is_sentence_start: bool = False
    is_sentence_end: bool = False
    analyses: list[Analysis] = field(default_factory=list)
    gold_analysis_index: int | None = None
    """Index into `analyses` of a human-verified analysis, if one exists (e.g. a FLEx
    `WfiGloss`-linked segment occurrence). `None` is the overwhelmingly common case for real
    interlinear text -- see report 18's Part 1 finding that gold per-token analysis linkage
    is rare (a fraction of a percent of tokens in the real `.fwdata` corpora inspected) even
    when running text itself is abundant. Never required for training (D15: raw text plus
    the analyzer is sufficient), only for evaluation."""

    @property
    def is_ambiguous(self) -> bool:
        return len(self.analyses) > 1

    @property
    def is_oov(self) -> bool:
        """No confirmed analysis at all -- the coverage-gap case report 13 found dominates
        real corpora (Sena 49% coverage, Amharic 24%, Aweti 49%, Indonesian 85%)."""
        return len(self.analyses) == 0

    def to_dict(self) -> dict:
        return {
            "record_type": "token",
            "doc_id": self.doc_id,
            "sentence_id": self.sentence_id,
            "position": self.position,
            "surface": self.surface,
            "is_punct": self.is_punct,
            "is_sentence_start": self.is_sentence_start,
            "is_sentence_end": self.is_sentence_end,
            "analyses": [a.to_dict() for a in self.analyses],
            "gold_analysis_index": self.gold_analysis_index,
        }

    @staticmethod
    def from_dict(d: dict) -> "Token":
        return Token(
            surface=d["surface"],
            doc_id=d.get("doc_id", "doc0"),
            sentence_id=int(d.get("sentence_id", 0)),
            position=int(d.get("position", 0)),
            is_punct=bool(d.get("is_punct", False)),
            is_sentence_start=bool(d.get("is_sentence_start", False)),
            is_sentence_end=bool(d.get("is_sentence_end", False)),
            analyses=[Analysis.from_dict(a) for a in d.get("analyses", [])],
            gold_analysis_index=d.get("gold_analysis_index"),
        )


@dataclass
class Sentence:
    """A contiguous run of tokens between a sentence start and end marker.

    This is the unit held-out splitting must respect (never split a sentence across
    train/dev/test) and the unit inter-word (cross-word) models condition over.
    """

    tokens: list[Token]
    doc_id: str = "doc0"
    sentence_id: int = 0
    free_translation: str | None = None
    """An optional free-translation string carried alongside the sentence (mirrors FLEx
    `Segment.FreeTranslation`, confirmed present in the real corpora inspected for report
    18's Part 1). Not consumed by any model here -- carried through for future use."""

    @property
    def surfaces(self) -> list[str]:
        return [t.surface for t in self.tokens]


Corpus = list[Sentence]


def write_jsonl(path: str | Path, corpus: Corpus, *, meta: dict | None = None) -> None:
    """Write a corpus to the interchange JSONL format.

    Emits one leading `record_type: "meta"` line (format name/version plus any caller
    metadata such as source/profile/language-shape), then one `record_type: "token"` line
    per token, in document/sentence/position order.
    """
    path = Path(path)
    with path.open("w", encoding="utf-8") as f:
        header = {
            "record_type": "meta",
            "format": FORMAT_NAME,
            "version": FORMAT_VERSION,
        }
        if meta:
            header.update(meta)
        f.write(json.dumps(header, ensure_ascii=False) + "\n")
        for sentence in corpus:
            n = len(sentence.tokens)
            for i, tok in enumerate(sentence.tokens):
                tok.doc_id = sentence.doc_id
                tok.sentence_id = sentence.sentence_id
                tok.position = i
                tok.is_sentence_start = i == 0
                tok.is_sentence_end = i == n - 1
                d = tok.to_dict()
                # Sentence-level metadata is carried on every token record of that
                # sentence (redundant across a sentence's tokens, but keeps every line
                # self-describing and avoids inventing a second record kind).
                d["free_translation"] = sentence.free_translation
                f.write(json.dumps(d, ensure_ascii=False) + "\n")


def iter_token_dicts(path: str | Path) -> Iterator[dict]:
    """Stream raw token records (skipping the meta line) without materializing sentences --
    for callers that only need a flat token stream (e.g. perplexity over raw text)."""
    path = Path(path)
    with path.open("r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            d = json.loads(line)
            if d.get("record_type") == "token":
                yield d


def read_jsonl(path: str | Path) -> tuple[Corpus, dict]:
    """Read a corpus from the interchange JSONL format, grouping tokens back into
    sentences by contiguous `(doc_id, sentence_id)` runs. Returns `(corpus, meta)`.
    """
    path = Path(path)
    meta: dict = {}
    corpus: Corpus = []
    current_tokens: list[Token] = []
    current_key: tuple[str, int] | None = None
    current_translation: str | None = None

    def flush():
        nonlocal current_tokens, current_key, current_translation
        if current_tokens:
            corpus.append(
                Sentence(
                    tokens=current_tokens,
                    doc_id=current_key[0],
                    sentence_id=current_key[1],
                    free_translation=current_translation,
                )
            )
        current_tokens = []
        current_key = None
        current_translation = None

    with path.open("r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            d = json.loads(line)
            if d.get("record_type") == "meta":
                meta = d
                continue
            tok = Token.from_dict(d)
            key = (tok.doc_id, tok.sentence_id)
            if current_key is not None and key != current_key:
                flush()
            current_key = key
            current_translation = d.get("free_translation")
            current_tokens.append(tok)
    flush()
    return corpus, meta


def corpus_from_surface_sentences(
    sentences: Iterable[list[str]], *, doc_id: str = "doc0"
) -> Corpus:
    """Convenience constructor for surface-only sentences (no analyses) -- mainly useful in
    tests and for adapting a plain tokenized text file with no analyzer available."""
    corpus: Corpus = []
    for sid, surfaces in enumerate(sentences):
        tokens = [Token(surface=s) for s in surfaces]
        corpus.append(Sentence(tokens=tokens, doc_id=doc_id, sentence_id=sid))
    return corpus
