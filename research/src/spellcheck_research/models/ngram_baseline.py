"""A surface-wordform n-gram baseline with Stupid Backoff smoothing.

This is the "one working baseline model plugged in end-to-end" requirement: it is
deliberately the *weakest* model family PLAN.md names (D4's own corroborating evidence,
report 04, calls a plain surface-word trigram the textbook worst case for a morphologically
rich language -- "nearly every test trigram unseen"). It exists to prove the pipeline
(interchange format -> fit -> score/predict_next -> eval harness) end-to-end with a model
simple enough to hand-verify, not to be competitive. The class/factored n-gram D4 actually
specifies is future work built behind the same :class:`SpellcheckModel` interface.

Smoothing: Stupid Backoff (Brants et al. 2007), not Kneser-Ney
-----------------------------------------------------------------
The brief allows either. Stupid Backoff was chosen for this first drop because it needs no
discount-mass bookkeeping (no held-out discount estimation, no continuation counts) and is
easy to hand-verify on a tiny corpus -- exactly what "does not need to be fast or clever; it
needs to be correct" asks for. A modified-Kneser-Ney implementation is real, nontrivial
work (absolute discounting + continuation-count estimation) and is left as an explicit
follow-up in report 18 rather than rushed.

    S(w | context) = count(context, w) / count(context)                    if count > 0
                   = alpha^d * S(w | shorter context)                       otherwise
                   = unk_score                                             if even the
                                                                            unigram is 0

`d` is how many orders were backed off through, matching Brants et al.'s recursive
definition (not just a single flat discount at the top).
"""

from __future__ import annotations

from collections import Counter, defaultdict
from typing import Sequence

from spellcheck_research.interchange import Corpus, Token
from spellcheck_research.models.base import SpellcheckModel

BOS = "<s>"
EOS = "</s>"


class StupidBackoffNgram(SpellcheckModel):
    def __init__(self, order: int = 3, alpha: float = 0.4, unk_score: float = 1e-6):
        if order < 1:
            raise ValueError("order must be >= 1")
        self.order = order
        self.alpha = alpha
        self.unk_score = unk_score
        self.name = f"stupid-backoff-{order}gram-surface"

        # ngram_counts[n][context_tuple][word] = count; context_totals[n][context_tuple] = sum.
        # n == 1 always uses context_tuple == () (the unigram case).
        self.ngram_counts: dict[int, dict[tuple[str, ...], Counter]] = {
            n: defaultdict(Counter) for n in range(1, order + 1)
        }
        self.context_totals: dict[int, dict[tuple[str, ...], int]] = {
            n: defaultdict(int) for n in range(1, order + 1)
        }
        self.vocab: set[str] = set()
        self._fitted = False

    def fit(self, corpus: Corpus) -> None:
        for sentence in corpus:
            surfaces = [BOS] * (self.order - 1) + sentence.surfaces + [EOS]
            for i in range(self.order - 1, len(surfaces)):
                word = surfaces[i]
                for n in range(1, self.order + 1):
                    start = i - n + 1
                    if start < 0:
                        continue
                    context = tuple(surfaces[start:i])
                    self.ngram_counts[n][context][word] += 1
                    self.context_totals[n][context] += 1
                if word not in (BOS, EOS):
                    self.vocab.add(word)
            self.vocab.add(EOS)
        self._fitted = True

    def _raw_score(self, word: str, context: tuple[str, ...]) -> float:
        """Recursive Stupid Backoff score for one word given a right-truncated context
        (context[-1] is the token immediately before `word`)."""
        n = min(self.order, len(context) + 1)
        backoff_steps = 0
        order = n
        while order >= 1:
            ctx = context[len(context) - (order - 1) :] if order > 1 else ()
            total = self.context_totals[order].get(ctx, 0)
            if total > 0:
                count = self.ngram_counts[order][ctx].get(word, 0)
                if count > 0:
                    return (self.alpha**backoff_steps) * (count / total)
            order -= 1
            backoff_steps += 1
        return self.unk_score

    def score(self, candidate: Token, context: Sequence[Token]) -> float:
        """Returns the raw Stupid Backoff score (NOT a log-probability, and not
        normalized across the vocabulary -- see module docstring / Brants et al.: Stupid
        Backoff scores are explicitly not calibrated probabilities, only rank-comparable
        within one context). `eval.metrics.perplexity` normalizes explicitly rather than
        assuming this is already a probability."""
        ctx = tuple(t.surface for t in context[-(self.order - 1) :]) if self.order > 1 else ()
        return self._raw_score(candidate.surface, ctx)

    def predict_next(self, context: Sequence[Token], k: int) -> list[tuple[str, float]]:
        if not self._fitted or k <= 0:
            return []
        ctx = tuple(t.surface for t in context[-(self.order - 1) :]) if self.order > 1 else ()
        scored = [(w, self._raw_score(w, ctx)) for w in self.vocab]
        scored.sort(key=lambda kv: kv[1], reverse=True)
        return scored[:k]
