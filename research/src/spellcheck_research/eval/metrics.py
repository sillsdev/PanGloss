"""Evaluation metrics: recall@k / accuracy@k, MRR, keystroke savings, perplexity.

Each function documents its own OOV handling explicitly rather than silently skipping or
substituting -- the classic trap named in the task brief. None of these functions filter
the input themselves; `eval.harness.evaluate` is where a corpus-level OOV *rate* is
computed and reported alongside the metrics, so a metric average is never quietly computed
over only the in-vocabulary subset without that fact being visible in the result.
"""

from __future__ import annotations

import math
from typing import Sequence

from spellcheck_research.interchange import Sentence, Token
from spellcheck_research.models.base import SpellcheckModel


def recall_at_k(ranked: Sequence[str], relevant: str, k: int) -> float:
    """1.0 if `relevant` appears in the top `k` of `ranked` (already best-first), else 0.0.

    Note: with exactly one relevant item per instance (the case everywhere in this
    harness -- next-word prediction has exactly one true next word; correction has exactly
    one true correction), "recall@k" and "accuracy@k" are the identical computation. Both
    names are used in the task brief; this one function is both. A metric with multiple
    relevant items per instance would need a different formula (recall = found/total
    relevant) -- not needed here and not implemented, to avoid a false precision claim.
    """
    return 1.0 if relevant in ranked[:k] else 0.0


def reciprocal_rank(ranked: Sequence[str], relevant: str) -> float:
    """1/rank (1-indexed) of `relevant` in `ranked`, or 0.0 if absent (the standard MRR
    convention -- an item that never appears contributes zero, not an undefined value)."""
    for i, item in enumerate(ranked):
        if item == relevant:
            return 1.0 / (i + 1)
    return 0.0


def mrr(per_instance_ranks: Sequence[float]) -> float:
    """Mean of already-computed `reciprocal_rank` values. Returns 0.0 for an empty input
    (documented rather than raising or returning NaN) since an evaluation harness may
    legitimately hand this zero instances (e.g. a test split with no eligible tokens)."""
    if not per_instance_ranks:
        return 0.0
    return sum(per_instance_ranks) / len(per_instance_ranks)


def keystroke_savings_rate(
    model: SpellcheckModel, sentences: Sequence[Sentence], k: int = 5
) -> tuple[float, int]:
    """Fraction of keystrokes saved by accepting the model's top-`k` suggestion as soon as
    it appears while typing a word left-to-right, averaged over every non-punctuation token
    in `sentences`.

    Definition (standard word-completion KSR): for a word of length L, simulate typing
    prefixes of length 1..L. At each prefix length p, ask `model.predict_next(context, k)`
    -- filtered to candidates starting with that prefix -- whether the true word is
    present. The first p where it is found lets the user accept after typing p characters
    plus one action to accept, so keystrokes-used = p + 1 (capped at L, since accepting
    never costs more than just typing the rest). If the true word never appears in any
    prefix's top-k, keystrokes-used = L (no savings). Savings for that word is
    `L - keystrokes_used`. The returned rate is `total_saved / total_L` over all
    (non-punctuation) tokens considered.

    Returns `(rate, n_tokens_considered)` -- the count is returned alongside the rate so a
    caller can see how many tokens the average is actually over (another OOV-adjacent
    trap: a rate over zero or very few tokens is not a stable estimate).

    OOV policy: a token whose surface never appears in the model's predicted candidates at
    any prefix length contributes zero savings (not excluded from the denominator) --
    "no savings" is the honest outcome for an unpredictable word, not a reason to drop it
    from the average.
    """
    total_len = 0
    total_saved = 0
    n_considered = 0

    for sentence in sentences:
        context: list[Token] = []
        for tok in sentence.tokens:
            if tok.is_punct:
                context.append(tok)
                continue
            word = tok.surface
            L = len(word)
            if L == 0:
                context.append(tok)
                continue
            n_considered += 1
            total_len += L

            keystrokes_used = L
            for p in range(1, L + 1):
                prefix = word[:p]
                predictions = model.predict_next(context, max(k * 5, k))
                candidates = [w for w, _ in predictions if w.startswith(prefix)][:k]
                if word in candidates:
                    keystrokes_used = min(L, p + 1)
                    break
            total_saved += L - keystrokes_used
            context.append(tok)

    rate = (total_saved / total_len) if total_len > 0 else 0.0
    return rate, n_considered


def perplexity(
    model: SpellcheckModel,
    sentences: Sequence[Sentence],
    vocab: Sequence[str],
) -> float:
    """Perplexity of `model` over the token stream in `sentences`.

    `model.score(candidate, context)` is not assumed to already be a normalized
    probability (several model families, e.g. Stupid Backoff, deliberately are not -- see
    `models.ngram_baseline`'s docstring). So this function normalizes explicitly at every
    position: it computes `model.score` for every word in `vocab` given the same context,
    and takes `P(true word) = score(true) / sum(score(v) for v in vocab)`. This is
    O(|vocab|) per token, which is fine for the small synthetic/research-scale vocabularies
    this harness targets and deliberately wrong to assume for a production-scale
    vocabulary -- stated here rather than silently degrading.

    A token whose true surface is not itself in `vocab` (OOV against the evaluation
    vocabulary) is skipped from the perplexity computation -- OOV rate must be reported
    separately (see `eval.harness.evaluate`), never folded silently into a perplexity
    number that a reader would otherwise assume covers every token.
    """
    vocab_list = list(vocab)
    log_probs: list[float] = []
    for sentence in sentences:
        context: list[Token] = []
        for tok in sentence.tokens:
            if tok.surface in vocab_list:
                scores = [model.score(Token(surface=v), context) for v in vocab_list]
                z = sum(scores)
                true_score = model.score(tok, context)
                p = (true_score / z) if z > 0 else 0.0
                p = max(p, 1e-12)  # floor to avoid log(0) on a genuinely zero-mass event
                log_probs.append(math.log(p))
            context.append(tok)

    if not log_probs:
        return float("inf")
    avg_neg_log_prob = -sum(log_probs) / len(log_probs)
    return math.exp(avg_neg_log_prob)
