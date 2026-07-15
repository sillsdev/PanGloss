"""Shared MorphologicalInput pattern matcher, used by both derive_indonesian.py (whose only
LHS shapes are 'generic stem' and one fixed-literal-then-stem rule) and derive_sena.py (whose
LHS shapes additionally condition the allomorph on the natural class of the stem's FIRST
segment, e.g. Sena's "mu-3" rule choosing 'mw-' before a back vowel vs 'm-' elsewhere -- a
genuine phonologically-conditioned-allomorph-selection pattern, not a fixed literal).

A MorphologicalInput `PhoneticSequence` is parsed (hc_xml.parse_env_tokens) into the SAME
token vocabulary phon.py's rewrite-rule environments use: ('class', ncid, has_alpha) |
('segment', charid) | ('boundary', charid) | ('opt_seq', min, max, inner). Every reference-
grammar rule's pattern is: [optional boundaries] [optional leading class/segment constraints]
[the generic-stem placeholder: OptionalSegmentSequence wrapping the "Any" class, ncid='nc1' in
both grammars] [optional trailing boundaries] -- i.e. a linear prefix-constraint-then-generic-
stem shape, matched left to right against the actual current derivation's token sequence.
"""
from phon import _match_tok


def match_stem_pattern(tokens, cur, ct, nc_segs, any_ncid="nc1"):
    """Match `tokens` (a parsed PhoneticSequence pattern) against `cur` (the current derivation's
    token tuple). Returns the REMAINING stem tokens (a tuple) bound to the generic-stem
    placeholder if the pattern matches, else None."""
    pos = 0
    i = 0
    n = len(tokens)
    while i < n:
        tok = tokens[i]
        if tok[0] == "opt_seq":
            _, mn, mx, inner = tok
            mn = int(mn)
            mx = None if mx in (None, "-1") else int(mx)
            if inner[0] == "class" and inner[1] == any_ncid and mn <= 1:
                # The generic stem placeholder -- bind everything remaining in `cur` here.
                # Every following pattern token must be a trailing optional-boundary wrapper
                # (the only shape observed in both reference grammars); anything else would mean
                # a pattern shape this PoC's matcher does not yet generalize to.
                for t2 in tokens[i + 1:]:
                    if not (t2[0] == "opt_seq" and t2[3][0] in ("boundary", "boundary_any")):
                        raise NotImplementedError(
                            f"unsupported pattern token after the generic-stem placeholder: {t2}"
                        )
                return tuple(cur[pos:])
            reps = 0
            while (mx is None or reps < mx) and pos < len(cur) and _match_tok(inner, cur[pos], ct, nc_segs):
                pos += 1
                reps += 1
            if reps < mn:
                return None
        else:
            if pos >= len(cur) or not _match_tok(tok, cur[pos], ct, nc_segs):
                return None
            pos += 1
        i += 1
    return tuple(cur[pos:])
