from spellcheck_research.synthetic.generator import generate_corpus
from spellcheck_research.synthetic.profiles import (
    HIGH_AMBIGUITY_MODERATE_RICHNESS,
    LOW_AMBIGUITY_ZERO_RICHNESS,
    CorpusProfile,
)


def test_generation_is_deterministic_given_seed():
    c1 = generate_corpus(HIGH_AMBIGUITY_MODERATE_RICHNESS, 30, seed=42)
    c2 = generate_corpus(HIGH_AMBIGUITY_MODERATE_RICHNESS, 30, seed=42)
    assert [s.surfaces for s in c1] == [s.surfaces for s in c2]
    # analyses content must also match, not just surfaces
    a1 = [[len(t.analyses) for t in s.tokens] for s in c1]
    a2 = [[len(t.analyses) for t in s.tokens] for s in c2]
    assert a1 == a2


def test_different_seeds_differ():
    c1 = generate_corpus(HIGH_AMBIGUITY_MODERATE_RICHNESS, 30, seed=1)
    c2 = generate_corpus(HIGH_AMBIGUITY_MODERATE_RICHNESS, 30, seed=2)
    assert [s.surfaces for s in c1] != [s.surfaces for s in c2]


def test_zero_richness_profile_never_attaches_features():
    corpus = generate_corpus(LOW_AMBIGUITY_ZERO_RICHNESS, 200, seed=7)
    n_analyses = 0
    for s in corpus:
        for t in s.tokens:
            for a in t.analyses:
                n_analyses += 1
                assert a.features == {}
    assert n_analyses > 0  # sanity: the test actually inspected something


def test_high_richness_profile_attaches_features_often():
    from spellcheck_research.synthetic.profiles import LOW_AMBIGUITY_HIGH_RICHNESS

    corpus = generate_corpus(LOW_AMBIGUITY_HIGH_RICHNESS, 300, seed=7)
    total = 0
    with_features = 0
    for s in corpus:
        for t in s.tokens:
            for a in t.analyses:
                total += 1
                if a.features:
                    with_features += 1
    rate = with_features / total
    # target is 0.85; generous tolerance since this is a stochastic count, not exact.
    assert 0.7 < rate < 0.95


def test_achieved_mean_ambiguity_is_close_to_target():
    profile = HIGH_AMBIGUITY_MODERATE_RICHNESS
    corpus = generate_corpus(profile, 800, seed=3)
    counts = [len(t.analyses) for s in corpus for t in s.tokens]
    mean = sum(counts) / len(counts)
    # Target 4.61; the generator calibrates the Poisson mean exactly in expectation, so a
    # wide sample should land close. Documented as an approximation, not exact -- see
    # generator.py's module docstring.
    assert abs(mean - profile.mean_analyses_per_token) < 0.6


def test_class_cardinality_knob_controls_vocab_size():
    small = CorpusProfile(
        name="tiny-test-only",
        description="test fixture, not a named research profile",
        n_open_classes=1,
        n_closed_classes=1,
        types_per_open_class=2,
        types_per_closed_class=1,
    )
    corpus = generate_corpus(small, 200, seed=1)
    distinct_surfaces = {t.surface for s in corpus for t in s.tokens}
    # 2 open stems * 4 paradigm forms + 1 closed stem * 4 paradigm forms = 12 possible
    # surfaces at most (fewer if some paradigm members collide, which is a documented
    # possibility -- see generator.py).
    assert len(distinct_surfaces) <= 12


def test_sentences_respect_length_profile_roughly():
    profile = HIGH_AMBIGUITY_MODERATE_RICHNESS
    corpus = generate_corpus(profile, 200, seed=5)
    lengths = [len(s.tokens) for s in corpus]
    assert all(length >= 1 for length in lengths)
    mean_len = sum(lengths) / len(lengths)
    assert abs(mean_len - profile.sentence_length_mean) < 2.0
