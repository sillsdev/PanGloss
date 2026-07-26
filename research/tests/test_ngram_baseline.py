"""Hand-verified Stupid Backoff arithmetic on a tiny corpus.

Corpus: one sentence, surfaces = ["a", "b", "a", "b"], bigram model (order=2, alpha=0.4).

Padded internally: ["<s>", "a", "b", "a", "b", "</s>"]. Bigram counts:
    (<s> -> a): 1        context "<s>" total 1
    (a -> b):   2         context "a"   total 2
    (b -> a):   1         context "b"   total 2
    (b -> </s>): 1
Unigram counts over target words {a, b, a, b, </s>}: a=2, b=2, </s>=1, total=5.

All expected numbers below are derived from exactly those counts, shown in each
assertion's comment.
"""

import pytest

from spellcheck_research.interchange import Sentence, Token
from spellcheck_research.models.ngram_baseline import StupidBackoffNgram


def _fit_tiny_model():
    model = StupidBackoffNgram(order=2, alpha=0.4)
    sentence = Sentence(tokens=[Token(surface=s) for s in ["a", "b", "a", "b"]])
    model.fit([sentence])
    return model


def test_seen_bigram_scores_as_plain_conditional_probability():
    model = _fit_tiny_model()
    ctx = [Token(surface="a")]
    # count(a->b)=2, context "a" total=2 -> P(b|a) = 2/2 = 1.0, no backoff.
    assert model.score(Token(surface="b"), ctx) == pytest.approx(1.0)


def test_another_seen_bigram():
    model = _fit_tiny_model()
    ctx = [Token(surface="b")]
    # count(b->a)=1, context "b" total=2 -> P(a|b) = 1/2 = 0.5.
    assert model.score(Token(surface="a"), ctx) == pytest.approx(0.5)
    # count(b-></s>)=1 out of the same total 2 -> also 0.5.
    assert model.score(Token(surface="</s>"), ctx) == pytest.approx(0.5)


def test_unseen_bigram_backs_off_to_discounted_unigram():
    model = _fit_tiny_model()
    ctx = [Token(surface="a")]
    # "a" never follows "a" (only "b" does), so back off one order:
    # alpha^1 * unigram(a)/total = 0.4 * (2/5) = 0.16.
    assert model.score(Token(surface="a"), ctx) == pytest.approx(0.4 * (2 / 5))


def test_completely_unknown_word_gets_unk_floor():
    model = _fit_tiny_model()
    ctx = [Token(surface="a")]
    assert model.score(Token(surface="never-seen"), ctx) == pytest.approx(model.unk_score)


def test_predict_next_ranks_by_score_descending():
    model = _fit_tiny_model()
    ctx = [Token(surface="a")]
    predictions = model.predict_next(ctx, k=3)
    words = [w for w, _ in predictions]
    assert words[0] == "b"  # score 1.0, strictly highest
    scores = dict(predictions)
    assert scores["b"] == pytest.approx(1.0)
    assert scores["a"] == pytest.approx(0.4 * (2 / 5))
    assert scores["</s>"] == pytest.approx(0.4 * (1 / 5))
    # descending order enforced
    assert scores["b"] > scores["a"] > scores["</s>"]
