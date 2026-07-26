"""Metric unit tests with hand-computed expected values.

This is the test the task brief specifically asked for: one that "would catch a
silently-broken evaluation metric" via a tiny, by-hand-computable case. `ranked = ["b", "a",
"c"]` is fixed and every assertion below is arithmetic anyone can re-check on paper.
"""

import math

import pytest

from spellcheck_research.eval import metrics
from spellcheck_research.interchange import Sentence, Token
from spellcheck_research.models.base import SpellcheckModel

RANKED = ["b", "a", "c"]


def test_recall_at_k_hand_computed():
    assert metrics.recall_at_k(RANKED, "b", k=1) == 1.0  # b is rank 1
    assert metrics.recall_at_k(RANKED, "a", k=1) == 0.0  # a is rank 2, not in top-1
    assert metrics.recall_at_k(RANKED, "a", k=2) == 1.0
    assert metrics.recall_at_k(RANKED, "c", k=2) == 0.0  # c is rank 3
    assert metrics.recall_at_k(RANKED, "c", k=3) == 1.0
    assert metrics.recall_at_k(RANKED, "z", k=3) == 0.0  # not present at all


def test_reciprocal_rank_hand_computed():
    assert metrics.reciprocal_rank(RANKED, "b") == 1.0
    assert metrics.reciprocal_rank(RANKED, "a") == pytest.approx(0.5)
    assert metrics.reciprocal_rank(RANKED, "c") == pytest.approx(1.0 / 3.0)
    assert metrics.reciprocal_rank(RANKED, "z") == 0.0


def test_mrr_hand_computed():
    # Four instances with reciprocal ranks 0, 0.5, 1, 1 -> mean 0.625.
    assert metrics.mrr([0.0, 0.5, 1.0, 1.0]) == pytest.approx(0.625)
    assert metrics.mrr([]) == 0.0


class _ConstantModel(SpellcheckModel):
    """A model whose predictions/scores never depend on context -- makes every metric
    exactly hand-computable regardless of position in the sentence."""

    name = "constant-test-model"

    def __init__(self, ranked_predictions, scores):
        self._ranked = ranked_predictions
        self._scores = scores

    def fit(self, corpus):
        pass

    def score(self, candidate, context):
        return self._scores.get(candidate.surface, 0.0001)

    def predict_next(self, context, k):
        return self._ranked[:k]


def test_keystroke_savings_rate_hand_computed():
    # One three-letter word "cat"; the model always predicts "cat" top-1 regardless of
    # prefix, so it should be recognized after typing 1 character + 1 accept action.
    model = _ConstantModel(ranked_predictions=[("cat", 1.0)], scores={"cat": 1.0})
    sentence = Sentence(tokens=[Token(surface="cat")])
    rate, n = metrics.keystroke_savings_rate(model, [sentence], k=1)
    # len=3, keystrokes_used = min(3, 1+1) = 2, saved = 1 -> rate = 1/3.
    assert n == 1
    assert rate == pytest.approx(1.0 / 3.0)


def test_keystroke_savings_rate_no_prediction_means_zero_savings():
    model = _ConstantModel(ranked_predictions=[("xyz", 1.0)], scores={"xyz": 1.0})
    sentence = Sentence(tokens=[Token(surface="cat")])
    rate, n = metrics.keystroke_savings_rate(model, [sentence], k=1)
    assert n == 1
    assert rate == pytest.approx(0.0)


def test_perplexity_uniform_model_equals_vocab_size():
    # A model that scores every candidate identically implies a uniform distribution over
    # the vocabulary at every position, so perplexity must equal |vocab| exactly.
    vocab = ["a", "b", "c", "d"]
    model = _ConstantModel(ranked_predictions=[], scores={v: 1.0 for v in vocab})
    sentence = Sentence(tokens=[Token(surface="a"), Token(surface="c")])
    ppl = metrics.perplexity(model, [sentence], vocab)
    assert ppl == pytest.approx(4.0, rel=1e-6)


def test_perplexity_confident_correct_model_is_near_one():
    vocab = ["a", "b", "c"]
    scores = {"a": 1000.0, "b": 0.001, "c": 0.001}
    model = _ConstantModel(ranked_predictions=[], scores=scores)
    sentence = Sentence(tokens=[Token(surface="a"), Token(surface="a")])
    ppl = metrics.perplexity(model, [sentence], vocab)
    assert ppl < 1.01
