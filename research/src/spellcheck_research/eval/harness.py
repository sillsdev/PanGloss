"""The evaluation harness: held-out splitting, OOV accounting, and the metric suite.

Guards against three classic traps, each enforced in code rather than left as a convention:

1. **Held-out splitting respects sentence boundaries.** :func:`split_corpus` shuffles and
   partitions whole :class:`~spellcheck_research.interchange.Sentence` objects, never
   individual tokens -- a sentence can never straddle train/dev/test.
2. **A model is never evaluated on data it was fit on.** :func:`evaluate` takes the exact
   `train` sentences that were passed to `model.fit(...)` and raises `ValueError` if any
   `(doc_id, sentence_id)` appears in both `train` and `test` before computing a single
   metric.
3. **An explicit, stated OOV policy.** Every metric that can encounter an out-of-vocabulary
   surface form documents exactly what it does (see `eval.metrics`), and `evaluate` reports
   `oov_rate` as its own field alongside every other metric -- so a reader can never mistake
   "the model handled everything perfectly" for "the model was never tested on anything
   hard."
"""

from __future__ import annotations

import json
import random
import string
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Sequence

from spellcheck_research.interchange import Corpus, Sentence, Token
from spellcheck_research.eval import metrics
from spellcheck_research.models.base import SpellcheckModel

_CORRUPT_ALPHABET = string.ascii_lowercase + "0123456789_"


def split_corpus(
    corpus: Corpus,
    *,
    train_frac: float = 0.8,
    dev_frac: float = 0.1,
    test_frac: float = 0.1,
    seed: int = 0,
) -> tuple[Corpus, Corpus, Corpus]:
    """Partition `corpus` into (train, dev, test) by shuffling and slicing whole sentences.

    Never splits inside a sentence -- the unit of shuffling is
    :class:`~spellcheck_research.interchange.Sentence`, so cross-word context is always
    evaluated with the same context it would have at inference time.
    """
    total = train_frac + dev_frac + test_frac
    if abs(total - 1.0) > 1e-6:
        raise ValueError(f"train/dev/test fractions must sum to 1.0, got {total}")

    idx = list(range(len(corpus)))
    random.Random(seed).shuffle(idx)
    n = len(idx)
    n_train = round(n * train_frac)
    n_dev = round(n * dev_frac)

    train_idx = idx[:n_train]
    dev_idx = idx[n_train : n_train + n_dev]
    test_idx = idx[n_train + n_dev :]
    return (
        [corpus[i] for i in train_idx],
        [corpus[i] for i in dev_idx],
        [corpus[i] for i in test_idx],
    )


def _sentence_ids(sentences: Sequence[Sentence]) -> set[tuple[str, int]]:
    return {(s.doc_id, s.sentence_id) for s in sentences}


def _assert_disjoint(train: Sequence[Sentence], test: Sequence[Sentence]) -> None:
    overlap = _sentence_ids(train) & _sentence_ids(test)
    if overlap:
        raise ValueError(
            f"{len(overlap)} sentence(s) appear in both train and test "
            f"(e.g. {next(iter(overlap))}) -- refusing to evaluate a model on data it was "
            "fit on."
        )


def _corrupt(word: str, rng: random.Random) -> str:
    """One random single-character edit (delete/substitute/insert/transpose) -- a cheap,
    stated stand-in for a real error corpus (none exists for a young orthography; see
    `00-synthesis.md`'s "Evaluation from nothing" open question). Good enough to exercise
    the correction metrics end-to-end; not a substitute for real or synthetic
    grammar-informed error generation (D5's job, out of scope here)."""
    if not word:
        return word
    op = rng.choice(["delete", "substitute", "insert", "transpose"])
    i = rng.randrange(len(word))
    if op == "delete" and len(word) > 1:
        return word[:i] + word[i + 1 :]
    if op == "substitute":
        c = rng.choice(_CORRUPT_ALPHABET)
        return word[:i] + c + word[i + 1 :]
    if op == "insert":
        c = rng.choice(_CORRUPT_ALPHABET)
        return word[:i] + c + word[i:]
    if op == "transpose" and len(word) > 1:
        j = min(i + 1, len(word) - 1)
        chars = list(word)
        chars[i], chars[j] = chars[j], chars[i]
        return "".join(chars)
    return word


def _correction_candidates(
    true_word: str, vocab: Sequence[str], rng: random.Random, n_distractors: int = 6
) -> list[str]:
    """Build a correction candidate set: the true word, a couple of corrupted variants of
    it, and `n_distractors` other random vocabulary items. The true word is always present
    -- this harness measures *ranking* quality given a candidate set, not candidate-set
    *recall* (whether the true word survives generation at all is a distinct, separately
    named open item -- report 13 § "What I could not measure" item 2 -- and is not
    implemented here)."""
    candidates = {true_word, _corrupt(true_word, rng), _corrupt(true_word, rng)}
    pool = [w for w in vocab if w != true_word]
    if pool:
        sample_n = min(n_distractors, len(pool))
        candidates.update(rng.sample(pool, sample_n))
    return list(candidates)


@dataclass
class EvaluationResult:
    model_name: str
    source_label: str
    k: int
    n_test_sentences: int
    n_test_tokens: int
    oov_rate: float
    next_word_recall_at_k: float
    next_word_mrr: float
    correction_recall_at_k: float
    correction_mrr: float
    keystroke_savings_rate: float
    keystroke_tokens_considered: int
    perplexity: float
    extra: dict = field(default_factory=dict)

    def to_dict(self) -> dict:
        return asdict(self)


def evaluate(
    model: SpellcheckModel,
    train: Sequence[Sentence],
    test: Sequence[Sentence],
    *,
    k: int = 5,
    source_label: str = "",
    seed: int = 0,
) -> EvaluationResult:
    """Evaluate a fitted `model` on `test`, given the exact `train` it was fit on (used
    only to build the training vocabulary for OOV accounting and correction-candidate
    distractors -- never re-fit here).

    `model.fit(train)` must already have been called by the caller; this function does not
    call it, so the same fitted model can be evaluated against several test slices without
    re-fitting.
    """
    _assert_disjoint(train, test)
    rng = random.Random(seed)

    train_vocab: set[str] = set()
    for s in train:
        for t in s.tokens:
            train_vocab.add(t.surface)
    vocab_list = sorted(train_vocab)

    n_tokens = 0
    n_oov = 0
    next_word_recalls: list[float] = []
    next_word_rrs: list[float] = []
    correction_recalls: list[float] = []
    correction_rrs: list[float] = []

    for sentence in test:
        context: list[Token] = []
        for tok in sentence.tokens:
            n_tokens += 1
            if tok.surface not in train_vocab:
                n_oov += 1

            # --- Next-word prediction: predict the token itself given prior context. ---
            predictions = model.predict_next(context, k)
            ranked_words = [w for w, _ in predictions]
            next_word_recalls.append(metrics.recall_at_k(ranked_words, tok.surface, k))
            next_word_rrs.append(metrics.reciprocal_rank(ranked_words, tok.surface))

            # --- Correction: rank a small candidate set against a corrupted observation. ---
            candidates = _correction_candidates(tok.surface, vocab_list, rng)
            scored = sorted(
                candidates,
                key=lambda w: model.score(Token(surface=w), context),
                reverse=True,
            )
            correction_recalls.append(metrics.recall_at_k(scored, tok.surface, k))
            correction_rrs.append(metrics.reciprocal_rank(scored, tok.surface))

            context.append(tok)

    ksr, ksr_n = metrics.keystroke_savings_rate(model, test, k=k)
    ppl = metrics.perplexity(model, test, vocab_list)

    return EvaluationResult(
        model_name=getattr(model, "name", model.__class__.__name__),
        source_label=source_label,
        k=k,
        n_test_sentences=len(test),
        n_test_tokens=n_tokens,
        oov_rate=(n_oov / n_tokens) if n_tokens else 0.0,
        next_word_recall_at_k=metrics.mrr(next_word_recalls),  # mean of 0/1 = recall rate
        next_word_mrr=metrics.mrr(next_word_rrs),
        correction_recall_at_k=metrics.mrr(correction_recalls),
        correction_mrr=metrics.mrr(correction_rrs),
        keystroke_savings_rate=ksr,
        keystroke_tokens_considered=ksr_n,
        perplexity=ppl,
    )


def format_table(results: Sequence[EvaluationResult]) -> str:
    """A simple aligned plain-text table -- no extra dependency for something this small."""
    headers = [
        "model",
        "source",
        "k",
        "n_tok",
        "oov%",
        "nw_recall@k",
        "nw_mrr",
        "corr_recall@k",
        "corr_mrr",
        "ksr",
        "ppl",
    ]
    rows = []
    for r in results:
        rows.append(
            [
                r.model_name,
                r.source_label,
                str(r.k),
                str(r.n_test_tokens),
                f"{100 * r.oov_rate:.1f}",
                f"{r.next_word_recall_at_k:.3f}",
                f"{r.next_word_mrr:.3f}",
                f"{r.correction_recall_at_k:.3f}",
                f"{r.correction_mrr:.3f}",
                f"{r.keystroke_savings_rate:.3f}",
                f"{r.perplexity:.2f}",
            ]
        )
    widths = [max(len(h), *(len(row[i]) for row in rows)) if rows else len(h) for i, h in enumerate(headers)]
    lines = [" | ".join(h.ljust(w) for h, w in zip(headers, widths))]
    lines.append("-+-".join("-" * w for w in widths))
    for row in rows:
        lines.append(" | ".join(c.ljust(w) for c, w in zip(row, widths)))
    return "\n".join(lines)


def write_results_json(path: str | Path, results: Sequence[EvaluationResult]) -> None:
    path = Path(path)
    path.write_text(
        json.dumps([r.to_dict() for r in results], indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
