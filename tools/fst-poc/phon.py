"""Direct simulation of HC's SPE-style phonological rewrite-rule cascade over a token list
(list of char-ids), operating exactly on the natural-class segment sets `hc_xml.py` already
expanded from the grammar's feature system. This is the *synthesis* direction (apply rules
forward, in stratum order) -- used at PoC-compile time to turn each enumerated concrete
underlying derivation into its actual surface form, mechanically, through the real rule
cascade (no hand-derivation of the assimilation/deletion interaction).
"""


def _match_tok(tokendef, charid, ct, nc_segs):
    kind = tokendef[0]
    if kind == "class":
        return charid in nc_segs.get(tokendef[1], set())
    if kind == "segment":
        return charid == tokendef[1]
    if kind == "boundary":
        return charid == tokendef[1]
    if kind == "boundary_any":
        return charid in tokendef[1]
    raise NotImplementedError(f"env token kind {kind}")


def _has_explicit_boundary(env):
    return any(t[0] == "boundary" for t in env)


def _strip_boundaries(tokens, ct):
    return [c for c in tokens if not ct.is_boundary.get(c, False)]


def _match_suffix(prefix_tokens, env, ct, nc_segs):
    """Does `prefix_tokens` (list of charids) END WITH a sequence matching `env` (a list of
    tokendefs, some of kind 'opt_seq')? Returns True/False. Small backtracking search (env is
    short in every rule we have).

    Boundary transparency: HermitCrab's environment matching treats a morpheme-boundary marker
    as invisible to a natural-class/segment constraint UNLESS the environment explicitly names a
    `BoundaryMarker` -- confirmed empirically via the Rust engine's own --trace output on
    "menulis-nulis" (prule3 "Nasalization in reduplication" fires across the reduplication's '+'
    boundary even though its environment never mentions one; see the report). We replicate this
    by matching against a boundary-STRIPPED view of the tokens whenever the environment itself
    contains no explicit boundary token; an environment that DOES name a boundary matches the
    real (unstripped) token stream literally, as before.
    """
    if not _has_explicit_boundary(env):
        prefix_tokens = _strip_boundaries(prefix_tokens, ct)

    def rec(pi, ei):
        if ei < 0:
            return True
        tok = env[ei]
        if tok[0] == "opt_seq":
            _, mn, mx, inner = tok
            mn = int(mn)
            mx = None if mx in (None, "-1") else int(mx)
            # try consuming k repetitions of inner, k from max down to min (greedy) or just
            # brute-force small k since these spans are short in practice.
            k = 0
            tries = []
            p = pi
            reps = 0
            while True:
                tries.append((p, reps))
                if mx is not None and reps >= mx:
                    break
                if p - 1 < 0 or not _match_tok(inner, prefix_tokens[p - 1], ct, nc_segs):
                    break
                p -= 1
                reps += 1
                if reps > 64:
                    break
            for (p2, reps2) in reversed(tries):
                if reps2 >= mn and rec(p2, ei - 1):
                    return True
            return False
        else:
            if pi - 1 < 0:
                return False
            if not _match_tok(tok, prefix_tokens[pi - 1], ct, nc_segs):
                return False
            return rec(pi - 1, ei - 1)
    return rec(len(prefix_tokens), len(env) - 1)


def _match_prefix(suffix_tokens, env, ct, nc_segs):
    """Does `suffix_tokens` (list of charids) START WITH a sequence matching `env`? Same
    boundary-transparency rule as `_match_suffix` (see its docstring)."""
    if not _has_explicit_boundary(env):
        suffix_tokens = _strip_boundaries(suffix_tokens, ct)

    def rec(pi, ei):
        if ei >= len(env):
            return True
        tok = env[ei]
        if tok[0] == "opt_seq":
            _, mn, mx, inner = tok
            mn = int(mn)
            mx = None if mx in (None, "-1") else int(mx)
            p = pi
            reps = 0
            tries = [(p, reps)]
            while True:
                if mx is not None and reps >= mx:
                    break
                if p >= len(suffix_tokens) or not _match_tok(inner, suffix_tokens[p], ct, nc_segs):
                    break
                p += 1
                reps += 1
                tries.append((p, reps))
                if reps > 64:
                    break
            for (p2, reps2) in tries:
                if reps2 >= mn and rec(p2, ei + 1):
                    return True
            return False
        else:
            if pi >= len(suffix_tokens):
                return False
            if not _match_tok(tok, suffix_tokens[pi], ct, nc_segs):
                return False
            return rec(pi + 1, ei + 1)
    return rec(0, 0)


def apply_phon_rules(tokens, phon_rules, ct, nc_segs, mpr_set):
    """Apply the full ordered cascade of PhonRule objects to a token list (list of charids),
    synthesis direction. Each rule: simultaneous, one left-to-right pass (matches HC's default
    non-iterative subrule mode for these two reference grammars -- both are 'Simultaneous' rules
    with no self-feeding within the same rule, verified against the XML `rewriteRuleMode`
    absence -> DTD default). Returns the new token list."""
    for rule in phon_rules:
        if rule.input_class is None:
            continue  # epenthesis rules: none in the reference grammars; skip if ever seen
        new_tokens = []
        i = 0
        n = len(tokens)
        while i < n:
            cid = tokens[i]
            input_matches = _match_tok(rule.input_class, cid, ct, nc_segs)
            fired = False
            if input_matches:
                for sub in rule.subrules:
                    if sub.excluded_mpr & mpr_set:
                        continue
                    if sub.required_mpr and not (sub.required_mpr & mpr_set):
                        continue
                    if not _match_suffix(tokens[:i], sub.left_env, ct, nc_segs):
                        continue
                    if not _match_prefix(tokens[i + 1:], sub.right_env, ct, nc_segs):
                        continue
                    # matched -- compute replacement
                    if sub.rhs_class[0] == "delete":
                        pass  # emit nothing
                    elif sub.rhs_class[0] == "segment":
                        new_tokens.append(sub.rhs_class[1])
                    elif sub.rhs_class[0] == "class":
                        # alpha-variable output: resolve to the specific charid that shares the
                        # SAME value for the rule's variable feature as the *matched* environment
                        # segment (e.g. nasal place assimilation). We approximate by intersecting
                        # the input segment's own natural class membership options: pick the
                        # member of rhs_class whose place feature matches the right-environment
                        # trigger consonant (found by re-scanning the right env's matched
                        # boundary+class token for the concrete charid).
                        new_tokens.append(_resolve_alpha(rule, sub, tokens, i, ct, nc_segs))
                    fired = True
                    break
            if not fired:
                new_tokens.append(cid)
            i += 1
        tokens = new_tokens
    return tokens


ALPHA_FEATURE_XML_ID = "feat271"  # 'OrthPlace' -- the only phonological feature any
# AlphaVariable ties in either reference grammar (Indonesian's two alpha rules: nasal place
# assimilation, and its reduplication counterpart). Not derived generically from the rule's own
# VariableFeature table only because doing so cleanly needs threading one more field through
# hc_xml.py's PhonRule/RewriteSubrule for a fact that is, empirically, constant across every
# alpha rule in both reference grammars -- flagged here, not hidden, as a scoped simplification.


def _scan_for_alpha_trigger(tokens, start, direction, alpha_ncid, ct, nc_segs, max_scan=32):
    """Scan tokens from `start` in `direction` ('back'|'fwd'), skipping boundary chars
    transparently (matching the same boundary-transparency rule as the environment matcher),
    and return the first char-id that is a member of `alpha_ncid`. `None` if not found within
    `max_scan` non-boundary chars."""
    step = -1 if direction == "back" else 1
    j = start
    seen = 0
    while 0 <= j < len(tokens) and seen < max_scan:
        c = tokens[j]
        if not ct.is_boundary.get(c, False):
            seen += 1
            if c in nc_segs.get(alpha_ncid, set()):
                return c
        j += step
    return None


def _find_alpha_env_token(env):
    for tok in env:
        if tok[0] == "class" and len(tok) > 2 and tok[2]:
            return tok[1]
        if tok[0] == "opt_seq" and tok[3][0] == "class" and len(tok[3]) > 2 and tok[3][2]:
            return tok[3][1]
    return None


def _resolve_alpha(rule, sub, tokens, i, ct, nc_segs):
    """Alpha-variable resolution: two genuinely different SPE patterns share this rule shape --
    (a) 'nasal assimilation' (prule4): the INPUT is a bare placeholder segment with no place of
    its own, so the alpha value must come from a REMOTE trigger named in an environment (here,
    the following consonant it assimilates to); (b) 'nasalization in reduplication' (prule3): the
    INPUT is itself a natural class carrying the SAME alpha variable as the output (the classic
    SPE `[a place] -> [+nas, a place]` idiom, "keep your own place, just add nasality") -- here
    the alpha value is the INPUT SEGMENT's OWN place feature, and looking at any environment
    token would be wrong (confirmed empirically: doing so for prule3 first produced "menulis-
    nyulis" instead of the engine's "menulis-nulis" -- see the report). We disambiguate by
    checking whether the rule's own `input_class` declares the alpha annotation."""
    members = nc_segs.get(sub.rhs_class[1], set())
    trigger_cid = None
    if len(rule.input_class) > 2 and rule.input_class[2]:
        trigger_cid = tokens[i]
    else:
        right_ncid = _find_alpha_env_token(sub.right_env)
        if right_ncid is not None:
            trigger_cid = _scan_for_alpha_trigger(tokens, i + 1, "fwd", right_ncid, ct, nc_segs)
        if trigger_cid is None:
            left_ncid = _find_alpha_env_token(sub.left_env)
            if left_ncid is not None:
                trigger_cid = _scan_for_alpha_trigger(tokens, i - 1, "back", left_ncid, ct, nc_segs)
    if trigger_cid is None:
        return sorted(members)[0]
    trigger_place = ct.features[trigger_cid].get(ALPHA_FEATURE_XML_ID)
    if trigger_place is None:
        return sorted(members)[0]
    for m in members:
        if ct.features[m].get(ALPHA_FEATURE_XML_ID) == trigger_place:
            return m
    return sorted(members)[0]
