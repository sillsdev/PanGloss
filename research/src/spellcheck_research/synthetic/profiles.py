"""Named synthetic-corpus profiles, described by statistical shape.

Standing repo rule (see `.gitignore` and `MEMORY.md`'s "Synthetic conformance only" entry):
committed fixtures are synthetic, and are never named after or modeled to resemble a
specific real language. Every profile here is named by its measurable statistical shape
(ambiguity level x feature richness), not by a language.

The shape *targets* -- not the data -- are drawn from `docs/research/spellcheck/
13-first-measurements.md`, which measured analysis-ambiguity and `syn_fs` population across
four real grammars and found they cluster into distinct combinations of "how ambiguous is
an average token" and "how often does an analysis carry a feature beyond bare POS". This
module reproduces those *combinations of numbers* as controllable generator knobs so a model
can be stress-tested against the shape of a language family's morphology without touching
any real language's data. No profile below is a stand-in for, or reproduction of, any single
real grammar -- the four presets are the four corners of the (ambiguity, richness) space
report 13 happened to observe occupied, nothing more specific than that.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class CorpusProfile:
    """Controllable knobs for :func:`spellcheck_research.synthetic.generator.generate_corpus`.

    Every field is a generation *target*: the generator calibrates simple parametric
    distributions (Poisson/Zipf/Dirichlet) to approximate the named target, documented
    per-field below. These are approximations, not exact reproductions -- see
    `research/docs/interchange-format.md` and the generator's own docstring for the exact
    calibration used, and `tests/test_synthetic_generator.py` for the tolerance a caller can
    expect.
    """

    name: str
    description: str

    n_open_classes: int = 4
    """Open (unbounded-vocabulary) word classes, e.g. noun-like / verb-like slots."""
    n_closed_classes: int = 3
    """Closed (small, fixed-vocabulary) word classes, e.g. determiner-like slots."""

    class_transition_concentration: float = 1.5
    """Dirichlet concentration for each class's row of the class-transition matrix. Lower
    = more peaked/informative transitions (more signal for an inter-word class n-gram to
    find); higher = closer to uniform (harder for D4's cross-word term)."""

    types_per_open_class: int = 60
    """Distinct word *stems* generated per open class."""
    types_per_closed_class: int = 6
    """Distinct word *stems* generated per closed class -- deliberately small, matching
    real closed classes (determiners, conjunctions, ...)."""
    vocab_zipf_s: float = 1.3
    """Zipf exponent for a class's stem-frequency distribution -- the Zipf-skew knob."""

    mean_analyses_per_token: float = 1.0
    """Target mean number of analyses attached per token -- the ambiguity-level knob."""
    p90_analyses_per_token: int = 1
    """Documented target for the 90th percentile of analyses/token. Informational: the
    generator's single-parameter Poisson-shifted sampler is calibrated off the mean only
    (see generator docstring); this field records the report-13-shaped target for
    documentation/comparison, and `tests/test_synthetic_generator.py` checks the *achieved*
    p90 lands within a documented tolerance, not that this field drives generation
    directly."""

    feature_richness: float = 0.0
    """Probability that a given analysis carries at least one feature beyond bare POS --
    the feature-richness knob. 0.0 reproduces report 13's clean zero-richness case
    (Indonesian-shaped: 0.00% of analyses carried anything beyond POS)."""
    n_features_when_rich: int = 2
    """How many feature key/value pairs a "rich" analysis carries."""

    mean_morphemes_per_word: float = 2.2
    """Target mean morpheme-sequence length per analysis -- the morphological-richness
    knob feeding the intra-word term (D4)."""

    sentence_length_mean: float = 8.0
    """Target mean tokens per sentence."""

    seed: int = 0
    """Default RNG seed if the caller doesn't pass one to `generate_corpus`."""


HIGH_AMBIGUITY_MODERATE_RICHNESS = CorpusProfile(
    name="high_ambiguity_moderate_richness",
    description=(
        "Wide analysis lattices (mean ~4.6, p90 ~9 analyses/token) with moderate, "
        "POS-concentrated feature richness (~30-45% of analyses carry a feature beyond "
        "bare POS). The widest-lattice corner of report 13's measured space."
    ),
    mean_analyses_per_token=4.61,
    p90_analyses_per_token=9,
    feature_richness=0.35,
    class_transition_concentration=1.2,
)

LOW_AMBIGUITY_ZERO_RICHNESS = CorpusProfile(
    name="low_ambiguity_zero_richness",
    description=(
        "Near-flat analysis lattices (mean ~1.0, p90 1) and ZERO feature richness -- "
        "every confirmed analysis carries bare POS and nothing else. The D1 "
        "'total collapse' corner: D4's backoff ladder degenerates to POS alone here, on "
        "purpose, so a model comparison can see how each family degrades in this regime."
    ),
    mean_analyses_per_token=1.03,
    p90_analyses_per_token=1,
    feature_richness=0.0,
    class_transition_concentration=2.5,
)

LOW_AMBIGUITY_HIGH_RICHNESS = CorpusProfile(
    name="low_ambiguity_high_richness",
    description=(
        "Near-flat analysis lattices (mean ~1.1, p90 2) but HIGH feature richness "
        "(~85% of analyses carry a feature beyond POS, concentrated in one dominant open "
        "class). Coverage is thin and lattices are narrow, but every surviving analysis is "
        "feature-rich."
    ),
    mean_analyses_per_token=1.12,
    p90_analyses_per_token=2,
    feature_richness=0.85,
    class_transition_concentration=1.8,
    mean_morphemes_per_word=3.0,
)

MODERATE_AMBIGUITY_MIXED_RICHNESS = CorpusProfile(
    name="moderate_ambiguity_mixed_richness",
    description=(
        "Moderate ambiguity (mean ~1.5, p90 2) with richness sharply split by class -- "
        "some open classes carry features on ~100% of analyses, others on 0%. Also the "
        "one shape in report 13's space where a bitset-style low-cardinality feature "
        "(D1's 'mpr') actually fires on a nontrivial fraction of analyses, so this profile "
        "sets `feature_richness` high enough that at least one closed-set low-cardinality "
        "feature is exercised."
    ),
    mean_analyses_per_token=1.47,
    p90_analyses_per_token=2,
    feature_richness=0.45,
    class_transition_concentration=1.5,
)

ALL_PROFILES: dict[str, CorpusProfile] = {
    p.name: p
    for p in (
        HIGH_AMBIGUITY_MODERATE_RICHNESS,
        LOW_AMBIGUITY_ZERO_RICHNESS,
        LOW_AMBIGUITY_HIGH_RICHNESS,
        MODERATE_AMBIGUITY_MIXED_RICHNESS,
    )
}
