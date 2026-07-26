from spellcheck_research.synthetic.generator import generate_corpus
from spellcheck_research.synthetic.profiles import (
    CorpusProfile,
    HIGH_AMBIGUITY_MODERATE_RICHNESS,
    LOW_AMBIGUITY_HIGH_RICHNESS,
    LOW_AMBIGUITY_ZERO_RICHNESS,
    MODERATE_AMBIGUITY_MIXED_RICHNESS,
    ALL_PROFILES,
)

__all__ = [
    "generate_corpus",
    "CorpusProfile",
    "HIGH_AMBIGUITY_MODERATE_RICHNESS",
    "LOW_AMBIGUITY_HIGH_RICHNESS",
    "LOW_AMBIGUITY_ZERO_RICHNESS",
    "MODERATE_AMBIGUITY_MIXED_RICHNESS",
    "ALL_PROFILES",
]
