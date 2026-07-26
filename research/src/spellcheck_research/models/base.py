"""The model plug-in interface.

Deliberately minimal -- this is research code for comparing model *families* (surface
n-gram, class/factored n-gram (D4), CRF-style reranker, bounded neural ablation (D5)), not
a production framework. Every model family PLAN.md names can be dropped in behind this one
class.

Contract
--------
- `fit(corpus)` trains (or re-trains from scratch) on a `Corpus` of held-in sentences.
  Implementations decide for themselves what statistics they need; the harness's only
  obligation is to never call `fit` with sentences that later appear in an evaluation
  split (see `eval.harness` for the split enforcement).
- `score(candidate, context)` returns a real number, higher = more probable/likely-correct.
  Not required to be a normalized log-probability *across models* (a perplexity comparison
  is only meaningful within one model that reports it honestly -- see `eval.metrics`), but
  within one model it must be monotonic in the model's own belief.
- `predict_next(context, k)` returns up to `k` `(surface, score)` pairs, best first, for
  what token is likely to follow `context`. This is what next-word-prediction and
  keystroke-savings evaluation call.
- `update(token)` is optional (default no-op) -- for models that adapt online (D7's
  personal-overlay shape). The harness never calls it during a `fit`/`evaluate` pass; it
  exists for a future online-adaptation experiment, not for the baseline in this drop.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Sequence

from spellcheck_research.interchange import Corpus, Token


class SpellcheckModel(ABC):
    """Abstract base class every model family under research implements."""

    #: A short, stable, human-readable name used in result tables/JSON. Subclasses should
    #: override this with something specific (e.g. "stupid-backoff-trigram-surface").
    name: str = "unnamed-model"

    @abstractmethod
    def fit(self, corpus: Corpus) -> None:
        """Train on `corpus`. May be called at most once per instance in this harness
        (re-fitting semantics, if any, are the subclass's own business)."""
        raise NotImplementedError

    @abstractmethod
    def score(self, candidate: Token, context: Sequence[Token]) -> float:
        """Score `candidate` (a token, itself possibly carrying multiple analyses) given
        the preceding `context` tokens (oldest first, i.e. `context[-1]` is the token
        immediately before `candidate`). Higher is better. `context` may be shorter than
        the model's nominal order (e.g. at sentence start) -- models must degrade
        gracefully (back off), never raise."""
        raise NotImplementedError

    @abstractmethod
    def predict_next(
        self, context: Sequence[Token], k: int
    ) -> list[tuple[str, float]]:
        """Return up to `k` `(surface_form, score)` predictions for the token following
        `context`, best (highest score) first. An empty list is a valid answer (the model
        has no basis to predict anything, e.g. a completely unseen context under a model
        with no generative fallback)."""
        raise NotImplementedError

    def update(self, token: Token) -> None:
        """Optional online-adaptation hook. No-op by default; a self-updating model
        overrides this to fold one observed token into its running statistics."""
        return None
