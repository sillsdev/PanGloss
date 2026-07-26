"""Synthetic token-sequence generator.

Produces a :class:`~spellcheck_research.interchange.Corpus` with **zero real-language
data** -- every surface form, morpheme, and feature value is a generated placeholder code
(e.g. `"st03012p2fx3_1"`), never a real word or real morpheme from any language. This
satisfies the standing repo rule that committed fixtures are synthetic (see `.gitignore`
and the "Synthetic conformance only" convention this project already follows for the Rust
conformance suite) and lets the harness run end-to-end with nothing gitignored/local.

What is modeled, and how each profile knob maps to a generation mechanism
--------------------------------------------------------------------------
- **Class-cardinality profile** (`n_open_classes`, `n_closed_classes`,
  `types_per_open_class`, `types_per_closed_class`): open classes get many stems, closed
  classes get few -- matching real closed-class behavior (determiners, conjunctions, ...).
- **Zipf skew** (`vocab_zipf_s`): each class's stem list is sampled with Zipfian weights
  over its own rank order, so a handful of stems dominate frequency within every class.
- **Morphological richness** (`mean_morphemes_per_word`): each generated wordform is a
  stem plus a Poisson-distributed number of affixes drawn from a small per-class affix
  pool, so affixes (unlike whole wordforms) recur across many different stems -- the
  property report 04/D4 depend on ("morphemes recur even when wordforms do not").
- **Ambiguity rate** (`mean_analyses_per_token`): the number of analyses attached to a
  token is `1 + Poisson(mean - 1)`. This calibrates the *mean* exactly (in expectation);
  the resulting p90 is a consequence of the Poisson shape, not independently controlled --
  see `tests/test_synthetic_generator.py` for the achieved-vs-target tolerance actually
  verified. Extra ("distractor") analyses are drawn from a possibly-different class with
  geometrically decaying relative score, modeling genuine cross-class ambiguity.
- **Feature richness** (`feature_richness`, `n_features_when_rich`): each analysis
  independently carries a small feature bundle with this probability, else none -- so
  `feature_richness=0.0` reproduces a clean zero-`syn_fs` corpus exactly (every analysis
  has POS and nothing else).
- **Cross-word structure**: a Markov chain over classes (transition matrix rows drawn from
  a Dirichlet with concentration `class_transition_concentration`) generates the class
  sequence for each sentence, so the inter-word class n-gram (D4) has genuine, tunable
  signal to find -- lower concentration means more peaked (more predictable) transitions.

Documented simplification: a token's *distractor* analyses reuse a different class's
morpheme/stem data while the token keeps one fixed surface string. Real cross-word
ambiguity can indeed put unrelated morpheme decompositions under one surface form (e.g.
"flies" as a plural noun vs. a verb), but this generator does not attempt to make a
distractor's morphemes spell the shared surface -- it only models the *lattice structure*
(multiple analyses, differing POS/features/morphemes, per token), which is all any model
or metric in this harness inspects. Stated here rather than hidden.
"""

from __future__ import annotations

import numpy as np

from spellcheck_research.interchange import Analysis, Corpus, Sentence, Token
from spellcheck_research.synthetic.profiles import CorpusProfile

PARADIGM_SIZE = 4
"""Fixed number of distinct inflected surface forms generated per stem. Not exposed as a
profile knob -- `mean_morphemes_per_word` already controls affix count per form, and
adding a second knob for paradigm size was judged not worth the complexity for this first
drop."""

AFFIX_POOL_SIZE = 5
"""Distinct affix codes generated per class, reused across all of that class's stems."""

FEATURE_POOL: list[tuple[str, list[str]]] = [
    ("feat0", ["v0", "v1", "v2"]),
    ("feat1", ["v0", "v1"]),
    ("feat2", ["v0", "v1", "v2", "v3"]),
]
"""Small synthetic feature-name/value pool used when an analysis is chosen to be
"feature-rich" (see `CorpusProfile.feature_richness`)."""


def _class_names(profile: CorpusProfile) -> list[str]:
    return [f"OPEN{i}" for i in range(profile.n_open_classes)] + [
        f"CLOSED{i}" for i in range(profile.n_closed_classes)
    ]


def _zipf_weights(n: int, s: float) -> np.ndarray:
    ranks = np.arange(1, n + 1, dtype=float)
    w = 1.0 / np.power(ranks, s)
    return w / w.sum()


def _build_vocab(
    rng: np.random.Generator, profile: CorpusProfile
) -> tuple[list[list[tuple[str, list[str], str]]], list[np.ndarray]]:
    """Returns (per-class wordform list, per-class Zipf sampling weights).

    Each wordform is `(surface, morphemes, stem)`.
    """
    n_open = profile.n_open_classes
    n_closed = profile.n_closed_classes
    per_class_vocab: list[list[tuple[str, list[str], str]]] = []
    per_class_weights: list[np.ndarray] = []

    for class_idx in range(n_open + n_closed):
        is_open = class_idx < n_open
        n_stems = profile.types_per_open_class if is_open else profile.types_per_closed_class
        affix_pool = [f"fx{class_idx}_{k}" for k in range(AFFIX_POOL_SIZE)]

        class_vocab: list[tuple[str, list[str], str]] = []
        for stem_idx in range(max(n_stems, 1)):
            stem_code = f"st{class_idx}_{stem_idx:03d}"
            for p in range(PARADIGM_SIZE):
                n_affixes = max(
                    0, int(rng.poisson(max(profile.mean_morphemes_per_word - 1.0, 0.0)))
                )
                chosen = (
                    list(rng.choice(affix_pool, size=n_affixes, replace=True))
                    if n_affixes > 0
                    else []
                )
                surface = f"{stem_code}p{p}" + "".join(chosen)
                morphemes = [stem_code] + chosen
                class_vocab.append((surface, morphemes, stem_code))
        per_class_vocab.append(class_vocab)
        per_class_weights.append(_zipf_weights(len(class_vocab), profile.vocab_zipf_s))

    return per_class_vocab, per_class_weights


def _maybe_features(rng: np.random.Generator, profile: CorpusProfile) -> dict[str, str]:
    if rng.random() >= profile.feature_richness:
        return {}
    n = min(profile.n_features_when_rich, len(FEATURE_POOL))
    idxs = rng.choice(len(FEATURE_POOL), size=n, replace=False)
    feats: dict[str, str] = {}
    for idx in idxs:
        name, values = FEATURE_POOL[int(idx)]
        feats[name] = values[int(rng.integers(0, len(values)))]
    return feats


def _sample_ambiguity_count(rng: np.random.Generator, mean: float) -> int:
    lam = max(mean - 1.0, 0.0)
    return 1 + int(rng.poisson(lam))


def generate_corpus(
    profile: CorpusProfile,
    n_sentences: int,
    *,
    seed: int | None = None,
    n_docs: int = 1,
) -> Corpus:
    """Generate a synthetic :class:`Corpus` of `n_sentences` sentences shaped by `profile`.

    Deterministic given `seed` (falls back to `profile.seed`). Sentences are distributed
    as evenly as possible across `n_docs` documents (useful for testing doc-aware held-out
    splitting); with the default `n_docs=1` everything is one document.
    """
    rng = np.random.default_rng(seed if seed is not None else profile.seed)

    class_names = _class_names(profile)
    n_classes = len(class_names)
    vocab, weights = _build_vocab(rng, profile)

    concentration = np.full(n_classes, profile.class_transition_concentration)
    transition = np.stack([rng.dirichlet(concentration) for _ in range(n_classes)])
    initial = rng.dirichlet(concentration)

    corpus: Corpus = []
    sentences_per_doc = [n_sentences // n_docs] * n_docs
    for i in range(n_sentences % n_docs):
        sentences_per_doc[i] += 1

    global_sentence_id = 0
    for doc_idx, n_doc_sentences in enumerate(sentences_per_doc):
        doc_id = f"doc{doc_idx}"
        for _ in range(n_doc_sentences):
            length = max(1, int(rng.poisson(max(profile.sentence_length_mean - 1.0, 0.0))) + 1)
            tokens: list[Token] = []
            prev_class: int | None = None
            for pos in range(length):
                probs = initial if prev_class is None else transition[prev_class]
                class_idx = int(rng.choice(n_classes, p=probs))
                prev_class = class_idx

                word_idx = int(rng.choice(len(vocab[class_idx]), p=weights[class_idx]))
                surface, morphemes, stem = vocab[class_idx][word_idx]

                true_analysis = Analysis(
                    pos=class_names[class_idx],
                    features=_maybe_features(rng, profile),
                    morphemes=list(morphemes),
                    stem=stem,
                    score=1.0,
                )
                n_analyses = _sample_ambiguity_count(rng, profile.mean_analyses_per_token)
                analyses = [true_analysis]
                for extra in range(n_analyses - 1):
                    d_class_idx = int(rng.integers(0, n_classes))
                    d_word_idx = int(rng.choice(len(vocab[d_class_idx]), p=weights[d_class_idx]))
                    _, d_morphemes, d_stem = vocab[d_class_idx][d_word_idx]
                    analyses.append(
                        Analysis(
                            pos=class_names[d_class_idx],
                            features=_maybe_features(rng, profile),
                            morphemes=list(d_morphemes),
                            stem=d_stem,
                            score=0.5 ** (extra + 1),
                        )
                    )

                tokens.append(
                    Token(
                        surface=surface,
                        doc_id=doc_id,
                        sentence_id=global_sentence_id,
                        position=pos,
                        is_sentence_start=(pos == 0),
                        is_sentence_end=(pos == length - 1),
                        analyses=analyses,
                    )
                )

            corpus.append(Sentence(tokens=tokens, doc_id=doc_id, sentence_id=global_sentence_id))
            global_sentence_id += 1

    return corpus
