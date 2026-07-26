"""End-to-end pipeline tests: interchange -> synthetic generation -> fit -> evaluate.

Also covers the two hard-enforced guardrails in `eval.harness`:
- held-out splitting never breaks a sentence in half,
- evaluating a model against data that overlaps its training sentences raises, rather than
  silently producing an optimistic number.
"""

import pytest

from spellcheck_research.cli import run as cli_run
from spellcheck_research.eval.harness import evaluate, split_corpus
from spellcheck_research.models.ngram_baseline import StupidBackoffNgram
from spellcheck_research.synthetic.generator import generate_corpus
from spellcheck_research.synthetic.profiles import ALL_PROFILES, HIGH_AMBIGUITY_MODERATE_RICHNESS


def test_split_corpus_never_splits_a_sentence_and_covers_everything():
    corpus = generate_corpus(HIGH_AMBIGUITY_MODERATE_RICHNESS, 100, seed=9)
    train, dev, test = split_corpus(corpus, train_frac=0.7, dev_frac=0.15, test_frac=0.15, seed=1)

    all_ids = set()
    for part in (train, dev, test):
        for s in part:
            key = (s.doc_id, s.sentence_id)
            assert key not in all_ids, "a sentence appeared in more than one split"
            all_ids.add(key)

    original_ids = {(s.doc_id, s.sentence_id) for s in corpus}
    assert all_ids == original_ids
    assert len(train) + len(dev) + len(test) == len(corpus)


def test_split_corpus_rejects_bad_fractions():
    corpus = generate_corpus(HIGH_AMBIGUITY_MODERATE_RICHNESS, 10, seed=1)
    with pytest.raises(ValueError):
        split_corpus(corpus, train_frac=0.5, dev_frac=0.5, test_frac=0.5)


def test_evaluate_refuses_to_run_on_data_the_model_was_fit_on():
    corpus = generate_corpus(HIGH_AMBIGUITY_MODERATE_RICHNESS, 20, seed=2)
    train, _dev, _test = split_corpus(corpus, seed=1)
    model = StupidBackoffNgram()
    model.fit(train)

    # Deliberately evaluate against (part of) the train set itself.
    with pytest.raises(ValueError, match="fit on"):
        evaluate(model, train, train[:2])


def test_full_pipeline_produces_sane_metric_ranges():
    corpus = generate_corpus(HIGH_AMBIGUITY_MODERATE_RICHNESS, 300, seed=4)
    train, _dev, test = split_corpus(corpus, seed=4)
    model = StupidBackoffNgram(order=3)
    model.fit(train)

    result = evaluate(model, train, test, k=5, source_label="test-profile")

    assert result.n_test_sentences == len(test)
    assert result.n_test_tokens > 0
    assert 0.0 <= result.oov_rate <= 1.0
    assert 0.0 <= result.next_word_recall_at_k <= 1.0
    assert 0.0 <= result.next_word_mrr <= 1.0
    assert 0.0 <= result.correction_recall_at_k <= 1.0
    assert 0.0 <= result.correction_mrr <= 1.0
    assert -1e-9 <= result.keystroke_savings_rate <= 1.0
    assert result.perplexity > 0.0

    # The correct word is always injected into the correction candidate set (see
    # `harness._correction_candidates`), so recall@k for correction with k >= 1 should
    # never be a hard zero across a few hundred tokens -- a regression here would signal
    # the candidate set stopped including the true word.
    assert result.correction_recall_at_k > 0.0


def test_cli_run_end_to_end(tmp_path):
    exit_code = cli_run(
        profile_names=list(ALL_PROFILES)[:1],
        n_sentences=100,
        k=3,
        seed=1,
        out_dir=tmp_path,
        dump_corpus=True,
    )
    assert exit_code == 0
    assert (tmp_path / "results.json").exists()
    profile_name = list(ALL_PROFILES)[0]
    assert (tmp_path / f"{profile_name}.jsonl").exists()
