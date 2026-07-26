"""Round-trip tests for the interchange format -- the load-bearing design piece.

Specifically guards against the one failure mode the task brief calls out explicitly: a
format that silently collapses a token's multiple analyses down to one. Every test corpus
here includes at least one genuinely ambiguous token (>1 analysis) and asserts the full
list survives a write/read round trip untouched.
"""

from spellcheck_research.interchange import (
    Analysis,
    Sentence,
    Token,
    read_jsonl,
    write_jsonl,
)


def _sample_corpus():
    ambiguous = Token(
        surface="kanta",
        analyses=[
            Analysis(pos="V", features={"tense": "past"}, morphemes=["kant", "a"], stem="kant", score=1.0),
            Analysis(pos="N", features={}, morphemes=["kanta"], stem="kanta", score=0.3, guessed=True),
        ],
    )
    unambiguous = Token(surface="the", analyses=[Analysis(pos="DET")])
    oov = Token(surface="zzqx", analyses=[])  # zero confirmed analyses -- a real, valid state
    s1 = Sentence(tokens=[unambiguous, ambiguous], doc_id="d0", sentence_id=0, free_translation="hello")
    s2 = Sentence(tokens=[oov], doc_id="d0", sentence_id=1)
    return [s1, s2]


def test_round_trip_preserves_full_analysis_lattice(tmp_path):
    corpus = _sample_corpus()
    path = tmp_path / "corpus.jsonl"
    write_jsonl(path, corpus, meta={"source": "unit-test"})

    read_back, meta = read_jsonl(path)

    assert meta["format"]
    assert meta["source"] == "unit-test"

    assert len(read_back) == 2
    s1, s2 = read_back

    # The ambiguous token must keep BOTH analyses, in order, with every field intact.
    ambiguous_tok = s1.tokens[1]
    assert ambiguous_tok.surface == "kanta"
    assert len(ambiguous_tok.analyses) == 2
    assert ambiguous_tok.analyses[0].pos == "V"
    assert ambiguous_tok.analyses[0].features == {"tense": "past"}
    assert ambiguous_tok.analyses[0].morphemes == ["kant", "a"]
    assert ambiguous_tok.analyses[1].pos == "N"
    assert ambiguous_tok.analyses[1].guessed is True
    assert ambiguous_tok.is_ambiguous is True

    # Zero-analysis token stays zero-analysis (not silently promoted or dropped).
    oov_tok = s2.tokens[0]
    assert oov_tok.surface == "zzqx"
    assert oov_tok.analyses == []
    assert oov_tok.is_oov is True

    # Sentence boundary markers recomputed correctly on write.
    assert s1.tokens[0].is_sentence_start is True
    assert s1.tokens[0].is_sentence_end is False
    assert s1.tokens[1].is_sentence_end is True
    assert s2.tokens[0].is_sentence_start is True
    assert s2.tokens[0].is_sentence_end is True

    # Document/sentence grouping preserved.
    assert s1.doc_id == "d0" and s1.sentence_id == 0
    assert s2.doc_id == "d0" and s2.sentence_id == 1
    assert s1.free_translation == "hello"


def test_analysis_feature_key_is_order_independent():
    a = Analysis(pos="V", features={"tense": "past", "num": "sg"})
    b = Analysis(pos="V", features={"num": "sg", "tense": "past"})
    assert a.feature_key() == b.feature_key()
