#!/usr/bin/env python
"""Build the Indonesian FST by ENUMERATING every concrete derivation (finite: 66 roots x 13
rules, unordered stratum, each rule usable at most once per path -- HC's default max_apps=1),
running each one through the REAL phonological rule cascade (phon.py -- the SAME rewrite-rule
interpreter for every rule, so reduplication is not special-cased at the phonology layer at
all), then building the final SURFACE<->TAGS transducer as a union of the resulting literal
(surface, tag-sequence) pairs.

This directly tests the reduplication interaction the user asked about: a redup rule's RHS
action list references the same stem part twice ('CopyFromInput' x2), which this generic
interpreter naturally expands into two literal copies of whatever concrete stem token sequence
it was called with -- INCLUDING a stem that itself already carries a prefix (e.g. mrule7 "-Cont"
copying an already meN-derived "meⁿ+tulis" stem for "menulis-nulis"-shaped words), because
enumeration works on the ALREADY-DERIVED token sequence, not the bare root. Each copy is then
carried through the SAME 5-rule phonological cascade as any ordinary word, so assimilation /
deletion is computed mechanically per occurrence, not hand-derived.

This is the standard xfst/foma "compile-replace" workaround (Beesley & Karttunen 2000): finite
because the lexicon is finite, at a cost that is measured, not assumed.
"""
import sys
import time
from dataclasses import dataclass, field

from hc_xml import Grammar, parse_grammar, MRule
from phon import apply_phon_rules
from morph_match import match_stem_pattern


@dataclass(frozen=True)
class Deriv:
    tags: tuple           # ordered tuple of xml ids introduced (root first, then each rule)
    tokens: tuple         # intermediate token list (char-ids), WITH boundary markers
    pos: str
    used: frozenset       # mrule xml ids already applied along this path
    mpr: frozenset


def _is_bare_stem(pattern):
    """True iff `pattern` is exactly the generic-stem placeholder with no leading constraint at
    all (the common shape: [boundary]* [Any] [boundary]*, no class/segment/fixed conditioning
    before it) -- i.e. the WHOLE current stem binds to this part, vs. a fixed-literal or
    natural-class-conditioned pattern where only the matched PREFIX should bind (mrule15's "meN"
    reduplication trigger; Sena's conditioned allomorphy)."""
    for tok in pattern:
        if tok[0] == "opt_seq" and tok[3][0] == "class" and tok[3][1] == "nc1":
            return True
        if tok[0] == "opt_seq" and tok[3][0] in ("boundary", "boundary_any"):
            continue
        return False
    return False


def _match_lhs(subrule, cur_tokens, ct, nc_segs):
    """Return dict part_id -> tuple(charids) if subrule's MorphologicalInput pattern(s) match
    cur_tokens, else None. Each lhs part is matched via the shared morph_match matcher in
    sequence (handles a bare generic stem, a fixed literal prefix like mrule15's reduplication
    trigger -- TWO parts: a fixed 'meN' pattern then a generic stem -- and a natural-class-
    conditioned stem like Sena's allomorph selection, all uniformly)."""
    bindings = {}
    remaining = cur_tokens
    terminal_seen = False
    for pid in subrule.lhs_parts:
        if terminal_seen:
            raise NotImplementedError("MorphologicalInput part after a greedy generic-stem placeholder")
        pattern = subrule.part_kind[pid]
        if _is_bare_stem(pattern):
            # The generic placeholder greedily binds everything left -- no further matching
            # needed (and no further parts are expected to follow it).
            bindings[pid] = remaining
            terminal_seen = True
            continue
        rest = match_stem_pattern(pattern, remaining, ct, nc_segs)
        if rest is None:
            return None
        bindings[pid] = remaining[:len(remaining) - len(rest)]
        remaining = rest
    if not terminal_seen and remaining:
        return None  # leftover tokens the LHS pattern never accounted for
    return bindings


def _apply_subrule(subrule, bindings, ct):
    out = []
    for action in subrule.rhs_actions:
        if action[0] == "insert":
            out.extend(ct.tokenize(action[1]))
        elif action[0] == "copy":
            out.extend(bindings[action[1]])
    return tuple(out)


def enumerate_derivations(g: Grammar, max_roots=None, verbose=True):
    ct = g.chartable
    lexicon = g.lexicon if max_roots is None else g.lexicon[:max_roots]

    applicable_by_pos = {}
    for mid, m in g.mrules.items():
        if m.required_pos is None:
            continue
        for p in m.required_pos:
            applicable_by_pos.setdefault(p, []).append(mid)

    all_derivs = []  # every derivation ever produced (roots + every rule-wrapped extension)
    frontier = []
    # Memoize on (root xml-id, frozenset of applied rule ids): the 'unordered' stratum lets any
    # applicable subset of rules fire in any order, but commuting affixes (independent prefixes/
    # suffixes) reach the SAME final token sequence regardless of the order they were applied in
    # -- without this key, the search re-derives every permutation of every subset (k! instead of
    # 2^k), which measurably exploded to 1.17M derivations / 126s for just 66 roots (recorded in
    # the report as the un-memoized baseline). Rules whose LHS pattern requires a specific prior
    # rule (e.g. mrule15's fixed 'meN' prefix match) are unaffected: an order that doesn't produce
    # the required prefix simply fails `_match_lhs` and never reaches the memo check.
    seen = set()
    for e in lexicon:
        for shape in e.shapes:
            toks = tuple(ct.tokenize(shape))
            d = Deriv(tags=(e.xml_id,), tokens=toks, pos=e.pos, used=frozenset(), mpr=frozenset(e.mpr))
            key = (e.xml_id, d.used)
            if key in seen:
                continue
            seen.add(key)
            all_derivs.append(d)
            frontier.append(d)

    while frontier:
        new_frontier = []
        for d in frontier:
            for mid in applicable_by_pos.get(d.pos, []):
                if mid in d.used:
                    continue
                m = g.mrules[mid]
                for subrule in m.subrules:
                    bindings = _match_lhs(subrule, d.tokens, ct, g.natural_classes_segs)
                    if bindings is None:
                        continue
                    new_tokens = _apply_subrule(subrule, bindings, ct)
                    new_mpr = d.mpr | subrule.out_mpr
                    new_used = d.used | frozenset([mid])
                    root_id = d.tags[0]
                    # Keyed on the resulting tokens too (not just root+rule-set): most
                    # permutations of a commuting rule-set converge to the identical token
                    # sequence and are safely collapsed, but an order-sensitive interaction (a
                    # rule whose LHS only matches after a specific prior rule) that produces a
                    # genuinely different result is kept as a distinct derivation.
                    key = (root_id, new_used, new_tokens)
                    if key in seen:
                        continue
                    seen.add(key)
                    nd = Deriv(
                        tags=d.tags + (mid,),
                        tokens=new_tokens,
                        pos=m.output_pos or d.pos,
                        used=new_used,
                        mpr=new_mpr,
                    )
                    all_derivs.append(nd)
                    new_frontier.append(nd)
        frontier = new_frontier

    if verbose:
        print(f"enumerated {len(all_derivs)} pre-compound derivations", file=sys.stderr)

    # 2-root compounding (bounded: compose two already-closed derivations, do not further wrap
    # rules onto the compound -- matches the engine's documented 2-root bound, HYBRID_FST_
    # FEASIBILITY.md §8.5).
    by_pos = {}
    for d in all_derivs:
        by_pos.setdefault(d.pos, []).append(d)

    compound_derivs = []
    for cid, c in g.crules.items():
        boundary_tok = ct.tokenize("+")
        for hpos in c.head_pos:
            for npos in c.nonhead_pos:
                heads = by_pos.get(hpos, [])
                nonheads = by_pos.get(npos, [])
                for h in heads:
                    for nh in nonheads:
                        if c.order == "nonhead_head":
                            toks = nh.tokens + tuple(boundary_tok) + h.tokens
                            tags = (cid,) + nh.tags + h.tags
                        else:
                            toks = h.tokens + tuple(boundary_tok) + nh.tokens
                            tags = (cid,) + h.tags + nh.tags
                        compound_derivs.append(Deriv(tags=tags, tokens=toks, pos=c.output_pos,
                                                      used=frozenset(), mpr=frozenset()))
    if verbose:
        print(f"generated {len(compound_derivs)} 2-root compound derivations", file=sys.stderr)

    return all_derivs + compound_derivs


BOUNDARY_TEXT_CHARS = None  # populated by surface_form() on first call, per-grammar


def surface_form(tokens, g: Grammar, boundary_ids):
    """Run the phonological cascade (synthesis direction) then strip every boundary char --
    real HC surface words never contain literal '+'/null-boundary characters."""
    ct = g.chartable
    mpr = frozenset()  # mpr already baked into which subrules fired; phon rules re-check per Deriv's own mpr at call site
    raise RuntimeError("use derive_and_realize instead")


def derive_and_realize(g: Grammar, max_roots=None, verbose=True):
    """Full pipeline: enumerate -> apply phonology -> strip boundaries -> return list of
    (surface_str, tags_tuple)."""
    ct = g.chartable
    boundary_ids = set(cid for cid, b in ct.is_boundary.items() if b)
    derivs = enumerate_derivations(g, max_roots=max_roots, verbose=verbose)
    results = []
    t0 = time.time()
    for d in derivs:
        realized = apply_phon_rules(list(d.tokens), g.phon_rules, ct, g.natural_classes_segs, d.mpr)
        surface_tokens = [c for c in realized if c not in boundary_ids]
        surface = "".join(ct.primary(c) for c in surface_tokens)
        results.append((surface, d.tags))
    if verbose:
        print(f"realized {len(results)} derivations through phonology in {time.time()-t0:.2f}s", file=sys.stderr)
    return results


if __name__ == "__main__":
    g = parse_grammar(sys.argv[1])
    results = derive_and_realize(g, verbose=True)
    surfaces = {}
    for surf, tags in results:
        surfaces.setdefault(surf, []).append(tags)
    print(f"distinct surface forms: {len(surfaces)}")
    for probe in ["menulis", "menulis-nulis", "mengamat-amati", "membagi-bagi", "menyewa-nyewa",
                  "mengayuh-ngayuh", "meminta-minta", "memijit-mijit"]:
        if probe in surfaces:
            print(f"  {probe}: {len(surfaces[probe])} analyses -> {surfaces[probe]}")
        else:
            print(f"  {probe}: NOT GENERATED")
