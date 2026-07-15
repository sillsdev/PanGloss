#!/usr/bin/env python
"""Build Sena's FST by enumerating derivations PER AFFIX TEMPLATE (a template is a fixed,
POS-gated sequence of slots; each slot is a closed choice among a handful of alternative mrules,
optionally skippable) -- much more tightly bounded than Indonesian's free "any subset of N rules
in any order" stratum, since a template fixes both the SLOT COUNT and, per slot, the exact
alternative set. No phonological rules exist in this grammar (confirmed: 0 `<PhonologicalRule>`
elements), so there is no rewrite cascade to run -- "realizing" a derivation is just stripping
boundary-marker characters from its token sequence.

Sena's one HC construct Indonesian doesn't exercise: allomorph selection conditioned on the
natural class of the stem's OWN first segment (e.g. "mu-3"'s mw-/m-/(default) allomorphs). This
is handled by the SAME `morph_match.match_stem_pattern` matcher Indonesian's mrule15 reduplication
trigger uses (a fixed/conditioned prefix followed by a generic-stem placeholder) -- no
special-casing needed, confirming the matcher generalizes across both grammars' allomorphy
shapes.
"""
import sys
import time
from dataclasses import dataclass

from hc_xml import Grammar, parse_grammar
from morph_match import match_stem_pattern
from phon import _match_suffix


@dataclass(frozen=True)
class SDeriv:
    tags: tuple
    tokens: tuple
    pos: str


def _match_lhs(subrule, cur_tokens, ct, nc_segs):
    bindings = {}
    remaining = cur_tokens
    terminal_seen = False
    for pid in subrule.lhs_parts:
        if terminal_seen:
            raise NotImplementedError("MorphologicalInput part after a greedy generic-stem placeholder")
        pattern = subrule.part_kind[pid]
        if _is_bare_stem(pattern):
            bindings[pid] = remaining
            terminal_seen = True
            continue
        rest = match_stem_pattern(pattern, remaining, ct, nc_segs)
        if rest is None:
            return None
        bindings[pid] = remaining[:len(remaining) - len(rest)]
        remaining = rest
    if not terminal_seen and remaining:
        return None
    return bindings


def _is_bare_stem(pattern):
    for tok in pattern:
        if tok[0] == "opt_seq" and tok[3][0] == "class" and tok[3][1] == "nc1":
            return True
        if tok[0] == "opt_seq" and tok[3][0] in ("boundary", "boundary_any"):
            continue
        return False
    return False


def _apply_subrule(subrule, bindings, ct):
    out = []
    for action in subrule.rhs_actions:
        if action[0] == "insert":
            out.extend(ct.tokenize(action[1]))
        elif action[0] == "copy":
            out.extend(bindings[action[1]])
    return tuple(out)


def _env_ok(subrule, cur_tokens, ct, nc_segs):
    """Allomorph environment gating (Sena: 72 subrules use RequiredEnvironments; 0 use
    ExcludedEnvironments in either reference grammar). Checks the LEFT environment against the
    stem's own trailing content (e.g. "/mb_" = stem must end in 'mb') via the SAME boundary-
    transparent suffix matcher phon.py's rewrite-rule cascade uses. RIGHT environment (10 of the
    72) is NOT enforced -- at LHS-matching time nothing has been attached to the right of this
    affix yet, so there is no concrete content to test it against; treated as always-satisfied.
    This is a scoped, named gap (see the report), not a silent one: it can only ever cause
    OVER-generation (an allomorph fires when the real engine's right-context check would have
    blocked it), never under-generation, and its measured impact is in the parity comparison."""
    for require, left_toks, right_toks in subrule.req_envs:
        left_ok = (not left_toks) or _match_suffix(cur_tokens, left_toks, ct, nc_segs)
        ok = left_ok  # right_toks deliberately unchecked (see docstring)
        if require and not ok:
            return False
        if not require and ok:
            return False
    return True


def _apply_rule(mid, g, d, verbose_errors=None):
    """Try every subrule (allomorph) of mrule `mid` against derivation `d`; return list of new
    SDeriv for every subrule whose LHS pattern AND environment conditions match (usually exactly
    one, the allomorph whose conditioning fits this stem)."""
    m = g.mrules[mid]
    ct = g.chartable
    out = []
    for subrule in m.subrules:
        bindings = _match_lhs(subrule, d.tokens, ct, g.natural_classes_segs)
        if bindings is None:
            continue
        if not _env_ok(subrule, d.tokens, ct, g.natural_classes_segs):
            continue
        new_tokens = _apply_subrule(subrule, bindings, ct)
        out.append(SDeriv(tags=d.tags + (mid,), tokens=new_tokens, pos=m.output_pos or d.pos))
    return out


def enumerate_templated(g: Grammar, max_roots=None, only_roots=None, verbose=True):
    ct = g.chartable
    if only_roots is not None:
        lexicon = [e for e in g.lexicon if e.xml_id in only_roots]
    else:
        lexicon = g.lexicon if max_roots is None else g.lexicon[:max_roots]
    by_pos = {}
    for e in lexicon:
        by_pos.setdefault(e.pos, []).append(e)

    all_results = []  # (surface_tokens_with_boundary, tags) -- every derivation is a candidate word
    t0 = time.time()
    bare_roots = []
    for e in lexicon:
        for shape in e.shapes:
            toks = tuple(ct.tokenize(shape))
            d = SDeriv(tags=(e.xml_id,), tokens=toks, pos=e.pos)
            bare_roots.append(d)
            all_results.append(d)

    for ti, tmpl in enumerate(g.templates):
        pos_set = tmpl.required_pos or set(by_pos.keys())
        roots_here = [d for d in bare_roots if d.pos in pos_set]
        if not roots_here:
            continue
        frontier = roots_here
        for slot in tmpl.slots:
            new_frontier = []
            for d in frontier:
                options = []
                if slot.optional:
                    options.append(d)
                for rid in slot.rule_ids:
                    if rid not in g.mrules:
                        continue
                    options.extend(_apply_rule(rid, g, d))
                if not options and not slot.optional:
                    continue  # dead end: mandatory slot, nothing matched -- drop this branch
                new_frontier.extend(options)
            frontier = new_frontier
            if not frontier:
                break
        all_results.extend(frontier)
        if verbose and (ti + 1) % 5 == 0:
            print(f"  template {ti+1}/{len(g.templates)}: cumulative {len(all_results)} derivations "
                  f"({time.time()-t0:.1f}s)", file=sys.stderr)

    if verbose:
        print(f"templated enumeration: {len(all_results)} derivations in {time.time()-t0:.1f}s "
              f"({len(lexicon)} roots)", file=sys.stderr)

    # 2-root compounding: composing against the FULL 11M+-derivation `all_results` set exploded
    # to 47.8M candidate pairs and ~20+ GB RSS in an earlier run of this script (killed before it
    # could threaten the host) -- Sena's compounding rules gate on the SAME pos-classes several
    # heavily-templated verb categories also output (e.g. mrule7/8's headPartsOfSpeech list
    # overlaps the verb template's output pos set), so every one of the 11M inflected verb forms
    # became a compounding candidate. Scoped down to compounding over BARE ROOTS only (not
    # fully-inflected template outputs) -- a deliberate, reported limitation: this PoC's compound
    # coverage is restricted to root+root compounds, matching the construction actually shown in
    # the sample grammar (pronoun/copula-class compounds), not root+fully-inflected-verb compounds
    # (which the grammar's own DTD permits but which no corpus word in the 300-word sample needs --
    # verified against the oracle: see the report).
    by_pos2 = {}
    for d in bare_roots:
        by_pos2.setdefault(d.pos, []).append(d)
    compound_results = []
    for cid, c in g.crules.items():
        boundary_tok = tuple(ct.tokenize("+"))
        for hpos in c.head_pos:
            for npos in c.nonhead_pos:
                heads = by_pos2.get(hpos, [])
                nonheads = by_pos2.get(npos, [])
                for h in heads:
                    for nh in nonheads:
                        if c.order == "nonhead_head":
                            toks = nh.tokens + boundary_tok + h.tokens
                            tags = (cid,) + nh.tags + h.tags
                        else:
                            toks = h.tokens + boundary_tok + nh.tokens
                            tags = (cid,) + h.tags + nh.tags
                        compound_results.append(SDeriv(tags=tags, tokens=toks, pos=c.output_pos))
    if verbose:
        print(f"generated {len(compound_results)} 2-root compound derivations", file=sys.stderr)

    return all_results + compound_results


def realize_sena(g: Grammar, max_roots=None, only_roots=None, verbose=True):
    """No phonology to run -- just strip boundary chars. Returns list of (surface, tags)."""
    ct = g.chartable
    boundary_ids = set(cid for cid, b in ct.is_boundary.items() if b)
    derivs = enumerate_templated(g, max_roots=max_roots, only_roots=only_roots, verbose=verbose)
    results = []
    for d in derivs:
        surface = "".join(ct.primary(c) for c in d.tokens if c not in boundary_ids)
        results.append((surface, d.tags))
    return results


if __name__ == "__main__":
    max_roots = int(sys.argv[2]) if len(sys.argv) > 2 else None
    g = parse_grammar(sys.argv[1])
    results = realize_sena(g, max_roots=max_roots, verbose=True)
    surfaces = {}
    for surf, tags in results:
        surfaces.setdefault(surf, []).append(tags)
    print(f"distinct surface forms: {len(surfaces)}")
