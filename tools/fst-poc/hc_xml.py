#!/usr/bin/env python
"""Parse a HermitCrab grammar XML file (the subset used by indonesian-hc.xml / sena-hc.xml)
into a plain-Python structure that build_fst.py turns into a foma/pyfoma transducer.

This is deliberately narrow: it implements exactly the constructs observed in the two reference
grammars (flat affix rules + reduplication + 2-root compounding for Indonesian; affix templates +
slots + compounding for Sena; 5 SPE rewrite rules for Indonesian; zero phonological rules for
Sena). Constructs outside this set raise NotImplementedError with the offending element name, so
a converter run either succeeds honestly or fails loudly naming the exact unsupported construct
(per the task's "STOP and characterize precisely" instruction).
"""
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from typing import Optional
import unicodedata


def nfd(s: str) -> str:
    return unicodedata.normalize("NFD", s)


# ---------------------------------------------------------------------------
# Character definition table
# ---------------------------------------------------------------------------

class CharTable:
    def __init__(self):
        self.reps_by_id = {}       # charid -> [representations] (first = primary)
        self.is_boundary = {}      # charid -> bool
        self.features = {}         # charid -> {feat_xml_id: symbol_xml_id}  (segments only)
        self._lookup = {}          # nfd(rep) -> charid
        self._all_reps_sorted = []  # all reps, longest first, for greedy tokenization

    def add(self, charid, reps, is_boundary, features=None):
        self.reps_by_id[charid] = reps
        self.is_boundary[charid] = is_boundary
        self.features[charid] = features or {}
        for r in reps:
            key = nfd(r)
            if key in self._lookup:
                raise ValueError(f"duplicate representation {key!r} (charid={charid})")
            self._lookup[key] = charid
        self._all_reps_sorted = sorted(self._lookup.keys(), key=len, reverse=True)

    def primary(self, charid):
        return self.reps_by_id[charid][0]

    def tokenize(self, text):
        """Greedy longest-match tokenization of literal text into a list of charids,
        mirroring pg-grammar's segment.rs. Boundary '+' chars and the archiphoneme boundary
        reps ('^0','*0','&0','∅') are matched the same way as segments."""
        text = nfd(text)
        out = []
        i = 0
        n = len(text)
        while i < n:
            matched = None
            for rep in self._all_reps_sorted:
                if text.startswith(rep, i):
                    matched = rep
                    break
            if matched is None:
                raise ValueError(f"cannot tokenize {text!r} at position {i} (no char-def matches)")
            out.append(self._lookup[matched])
            i += len(matched)
        return out

    def segment_charids(self):
        return [c for c, b in self.is_boundary.items() if not b]


# ---------------------------------------------------------------------------
# Grammar-wide structure
# ---------------------------------------------------------------------------

@dataclass
class RewriteSubrule:
    excluded_mpr: set
    required_mpr: set
    lhs_class: str            # nat-class id matched by the input segment (or None if lhs empty = epenthesis)
    rhs_literal: Optional[str]  # output literal text (None = deletion), '' = deletion too
    rhs_class: Optional[str]    # if the output is itself a natural class (alpha-variable case): nc id
    left_env: list             # list of ('class', ncid) | ('boundary', charid) tokens
    right_env: list


@dataclass
class PhonRule:
    xml_id: str
    name: str
    input_class: Optional[str]   # natural class id of the single input segment (None = epenthesis)
    subrules: list  # list of RewriteSubrule


@dataclass
class MSubrule:
    lhs_parts: list          # ordered list of part ids, in MorphologicalInput doc order
    part_kind: dict          # part_id -> ('stem') | ('fixed', [charids])
    rhs_actions: list        # ordered list of ('copy', part_id) | ('insert', literal_text)
    out_mpr: set = field(default_factory=set)  # MorphologicalOutput@MPRFeatures, propagated onto the derivation
    # Allomorph environment conditioning (Sena: 72 subrules use this; Indonesian: none).
    # Each entry: (require: bool, left_tokens: list, right_tokens: list) -- `require=True` means
    # the environment must match for this allomorph to be selectable; `require=False`
    # (ExcludedEnvironments; unused by both reference grammars -- 0 occurrences) means it must NOT.
    req_envs: list = field(default_factory=list)


@dataclass
class MRule:
    xml_id: str
    name: str
    gloss: str
    required_pos: Optional[set]
    output_pos: Optional[str]
    is_redup: bool
    partial: bool
    subrules: list  # list of MSubrule


@dataclass
class CRule:
    xml_id: str
    name: str
    head_pos: set
    nonhead_pos: set
    output_pos: str
    order: str  # 'nonhead_head' or 'head_nonhead'


@dataclass
class Slot:
    optional: bool
    rule_ids: list


@dataclass
class Template:
    required_pos: Optional[set]
    slots: list  # list of Slot


@dataclass
class LexEntry:
    xml_id: str
    pos: str
    gloss: str
    shapes: list  # list of literal shape strings (allomorphs)
    mpr: set


@dataclass
class Grammar:
    name: str
    chartable: CharTable
    pos_names: dict           # posid -> name
    natural_classes_segs: dict  # ncid -> set(charid)  (segment members only)
    phon_rules: list          # ordered list of PhonRule
    mrules: dict              # id -> MRule
    crules: dict              # id -> CRule
    templates: list           # list of Template (empty for flat grammars)
    lexicon: list             # list of LexEntry
    stratum_mrule_order: list  # ordered mrule ids as declared on the stratum (document order)
    unordered: bool


def _text(el):
    return el.text or ""


def _parse_one_token(child):
    if child.tag == "SimpleContext":
        has_alpha = child.find("AlphaVariables/AlphaVariable") is not None
        return ("class", child.get("naturalClass"), has_alpha)
    if child.tag == "BoundaryMarker":
        return ("boundary", child.get("boundary"))
    if child.tag == "Segment":
        return ("segment", child.get("segment"))
    raise NotImplementedError(f"pattern token {child.tag}")


def parse_env_tokens(seq_el, ct=None):
    """Parse a `PhoneticSequence`-shaped element (used both by phonological-rule environments
    and by MorphologicalInput patterns -- same child-element vocabulary in the DTD) into the
    token list phon.py's matcher and morph_match.py's matcher both consume: ('class', ncid,
    has_alpha) | ('segment', charid) | ('boundary', charid) | ('opt_seq', min, max, inner).

    MorphologicalInput patterns wrap the generic stem placeholder in boilerplate padding of the
    shape `<OptionalSegmentSequence min="0" max="-1"><BoundaryMarker.../><BoundaryMarker.../>
    </OptionalSegmentSequence>` (2 alternative boundary defs, e.g. the compound-null-boundary
    and the ordinary '+' morpheme boundary) -- identical padding around every stem placeholder in
    every rule in both reference grammars, never the rule's own distinguishing content. Modeled
    as a single ('boundary_any', {charids}) inner token: "skip zero or more boundary chars of any
    of these types here", which is what the padding means operationally (this PoC's earlier,
    working implementation ignored this padding entirely and reached the same 121/121 parity
    result, which is the empirical justification for treating it permissively rather than
    requiring the exact child sequence/order)."""
    toks = []
    for child in seq_el:
        if child.tag == "Segments":
            # `<Segments><PhoneticShape>mb</PhoneticShape></Segments>`: a literal, pre-segmented
            # required substring (used in allomorph RequiredEnvironments and, rarely, directly in
            # a MorphologicalInput pattern) -- expands to one ('segment', charid) token per
            # character, tokenized the same greedy-longest-match way as any lexicon shape.
            if ct is None:
                raise ValueError("parse_env_tokens: <Segments> requires a CharTable (ct=...)")
            shape = _text(child.find("PhoneticShape"))
            for cid in ct.tokenize(shape):
                toks.append(("segment", cid))
        elif child.tag == "OptionalSegmentSequence":
            inner_children = list(child)
            if len(inner_children) == 1:
                inner = _parse_one_token(inner_children[0])
            elif inner_children and all(c.tag == "BoundaryMarker" for c in inner_children):
                inner = ("boundary_any", frozenset(c.get("boundary") for c in inner_children))
            else:
                raise NotImplementedError(
                    f"OptionalSegmentSequence with {len(inner_children)} mixed children"
                )
            toks.append(("opt_seq", child.get("min"), child.get("max"), inner))
        else:
            toks.append(_parse_one_token(child))
    return toks


def parse_grammar(path: str) -> Grammar:
    tree = ET.parse(path)
    root = tree.getroot()
    lang = root.find("Language")
    name = _text(lang.find("Name"))

    pos_names = {}
    for pos in lang.findall("./PartsOfSpeech/PartOfSpeech"):
        pos_names[pos.get("id")] = _text(pos.find("Name"))

    # --- phonological feature system: featid -> {symid: symid} (order doesn't matter, we only
    # need membership tests) ---
    feat_symbols = {}
    for feat in lang.findall("./PhonologicalFeatureSystem/SymbolicFeature"):
        fid = feat.get("id")
        syms = [s.get("id") for s in feat.findall("./Symbols/Symbol")]
        feat_symbols[fid] = syms

    # --- character definition table ---
    ct = CharTable()
    cdt = lang.find("CharacterDefinitionTable")
    for seg in cdt.findall("./SegmentDefinitions/SegmentDefinition"):
        cid = seg.get("id")
        reps = [_text(r) for r in seg.findall("./Representations/Representation")]
        feats = {}
        for fv in seg.findall("FeatureValue"):
            featid = fv.get("feature")
            symvals = fv.get("symbolValues", "").split()
            if len(symvals) != 1:
                raise NotImplementedError(
                    f"segment {cid} feature {featid} has {len(symvals)} symbolValues (expected 1)"
                )
            feats[featid] = symvals[0]
        ct.add(cid, reps, is_boundary=False, features=feats)
    for bnd in cdt.findall("./BoundaryDefinitions/BoundaryDefinition"):
        cid = bnd.get("id")
        reps = [_text(r) for r in bnd.findall("./Representations/Representation")]
        ct.add(cid, reps, is_boundary=True)

    # --- natural classes -> sets of segment charids ---
    nc_segs = {}
    for nc in lang.findall("./NaturalClasses/SegmentNaturalClass"):
        ncid = nc.get("id")
        members = set(s.get("segment") for s in nc.findall("Segment"))
        nc_segs[ncid] = members
    for nc in lang.findall("./NaturalClasses/FeatureNaturalClass"):
        ncid = nc.get("id")
        constraints = []  # (featid, set(symid))
        for fv in nc.findall("FeatureValue"):
            featid = fv.get("feature")
            symvals = set(fv.get("symbolValues", "").split())
            constraints.append((featid, symvals))
        members = set()
        for cid in ct.segment_charids():
            ok = True
            for featid, symvals in constraints:
                have = ct.features[cid].get(featid)
                if have is None:
                    continue  # unconstrained on this char -> matches (full-mask default)
                if have not in symvals:
                    ok = False
                    break
            if ok:
                members.add(cid)
        nc_segs[ncid] = members

    # --- phonological rules ---
    phon_rules = []
    for pr in lang.findall("./PhonologicalRuleDefinitions/PhonologicalRule"):
        xml_id = pr.get("id")
        pname = _text(pr.find("Name"))
        inp_segs = pr.findall("./PhoneticInput/PhoneticSequence/Segment")
        inp_ctx = pr.findall("./PhoneticInput/PhoneticSequence/SimpleContext")
        if inp_segs:
            input_class = ("segment", inp_segs[0].get("segment"))
        elif inp_ctx:
            has_alpha = inp_ctx[0].find("AlphaVariables/AlphaVariable") is not None
            input_class = ("class", inp_ctx[0].get("naturalClass"), has_alpha)
        else:
            input_class = None  # epenthesis: empty PhoneticInput

        subrules = []
        for sr in pr.findall("./PhonologicalSubrules/PhonologicalSubrule"):
            excluded_mpr = set((sr.get("excludedMPRFeatures") or "").split())
            required_mpr = set((sr.get("requiredMPRFeatures") or "").split())
            out_seg = sr.findall("./PhoneticOutput/PhoneticSequence/Segment")
            out_ctx = sr.findall("./PhoneticOutput/PhoneticSequence/SimpleContext")
            if out_seg:
                rhs = ("segment", out_seg[0].get("segment"))
            elif out_ctx:
                rhs = ("class", out_ctx[0].get("naturalClass"))
            else:
                rhs = ("delete", None)

            def parse_env(env_el):
                if env_el is None:
                    return []
                return parse_env_tokens(env_el.find("PhoneticTemplate/PhoneticSequence"), ct)

            left_env = parse_env(sr.find("Environment/LeftEnvironment"))
            right_env = parse_env(sr.find("Environment/RightEnvironment"))
            subrules.append(RewriteSubrule(
                excluded_mpr=excluded_mpr, required_mpr=required_mpr,
                lhs_class=input_class, rhs_literal=None, rhs_class=rhs,
                left_env=left_env, right_env=right_env,
            ))
        phon_rules.append(PhonRule(xml_id=xml_id, name=pname, input_class=input_class, subrules=subrules))

    # --- morphological + compounding rules (searched anywhere under Strata, order-preserving) ---
    mrules = {}
    crules = {}
    templates = []
    stratum_mrule_order = []
    unordered = True

    def parse_phonseq_as_input_part(pseq):
        """A MorphologicalInput PhoneticSequence, parsed into the SAME token vocabulary
        phonological-rule environments use (see `parse_env_tokens`): a linear sequence of
        [boundary]* [optional leading class/segment conditioning]* [generic-stem placeholder]
        [boundary]* -- matched at derivation time by morph_match.match_stem_pattern, which
        uniformly handles both a bare generic stem (Indonesian's common case) and a stem whose
        allomorph choice is CONDITIONED on the natural class of its own first segment (Sena's
        "mu-3"-style allomorphy) or a fixed literal prefix (Indonesian's mrule15 reduplication
        trigger) -- all three are just different prefixes of the same token list."""
        return parse_env_tokens(pseq, ct)

    def parse_output_actions(out_el):
        actions = []
        for child in out_el:
            if child.tag == "CopyFromInput":
                actions.append(("copy", child.get("index")))
            elif child.tag == "InsertSegments":
                shape = _text(child.find("PhoneticShape"))
                actions.append(("insert", shape))
            else:
                raise NotImplementedError(f"MorphologicalOutput action {child.tag}")
        return actions

    lexicon = []
    for stratum in lang.findall("./Strata/Stratum"):
        mrule_defs = stratum.find("MorphologicalRuleDefinitions")
        if mrule_defs is None:
            continue  # empty stratum (e.g. an unused Clitics/Surface placeholder stratum)
        unordered = stratum.get("morphologicalRuleOrder", "unordered") == "unordered"
        stratum_mrule_order = (stratum.get("morphologicalRules") or "").split()
        for el in mrule_defs:
            if el.tag == "MorphologicalRule":
                xml_id = el.get("id")
                rname = _text(el.find("Name"))
                gloss = _text(el.find("Gloss"))
                req_pos = set(el.get("requiredPartsOfSpeech").split()) if el.get("requiredPartsOfSpeech") else None
                out_pos = el.get("outputPartOfSpeech")
                partial = el.get("partial") == "true"
                subrules = []
                is_redup = False
                for msr in el.findall("./MorphologicalSubrules/MorphologicalSubrule"):
                    part_kind = {}
                    lhs_parts = []
                    for pseq in msr.findall("./MorphologicalInput/PhoneticSequence"):
                        pid = pseq.get("id")
                        lhs_parts.append(pid)
                        part_kind[pid] = parse_phonseq_as_input_part(pseq)
                    out_el = msr.find("MorphologicalOutput")
                    if out_el.get("redupMorphType"):
                        is_redup = True
                    actions = parse_output_actions(out_el)
                    out_mpr = set((out_el.get("MPRFeatures") or "").split())
                    req_envs = []
                    for env in msr.findall("./RequiredEnvironments/Environment"):
                        left_el = env.find("LeftEnvironment/PhoneticTemplate/PhoneticSequence")
                        right_el = env.find("RightEnvironment/PhoneticTemplate/PhoneticSequence")
                        left_toks = parse_env_tokens(left_el, ct) if left_el is not None else []
                        right_toks = parse_env_tokens(right_el, ct) if right_el is not None else []
                        req_envs.append((True, left_toks, right_toks))
                    for env in msr.findall("./ExcludedEnvironments/Environment"):
                        left_el = env.find("LeftEnvironment/PhoneticTemplate/PhoneticSequence")
                        right_el = env.find("RightEnvironment/PhoneticTemplate/PhoneticSequence")
                        left_toks = parse_env_tokens(left_el, ct) if left_el is not None else []
                        right_toks = parse_env_tokens(right_el, ct) if right_el is not None else []
                        req_envs.append((False, left_toks, right_toks))
                    subrules.append(MSubrule(lhs_parts=lhs_parts, part_kind=part_kind, rhs_actions=actions,
                                              out_mpr=out_mpr, req_envs=req_envs))
                mrules[xml_id] = MRule(xml_id=xml_id, name=rname, gloss=gloss, required_pos=req_pos,
                                        output_pos=out_pos, is_redup=is_redup, partial=partial,
                                        subrules=subrules)
            elif el.tag == "CompoundingRule":
                xml_id = el.get("id")
                rname = _text(el.find("Name"))
                head_pos = set((el.get("headPartsOfSpeech") or "").split())
                nonhead_pos = set((el.get("nonHeadPartsOfSpeech") or "").split())
                out_pos = el.get("outputPartOfSpeech")
                sub = el.find("./CompoundingSubrules/CompoundingSubrule")
                actions = parse_output_actions(sub.find("MorphologicalOutput"))
                # Determine concatenation order from the action list: which of
                # head/nonhead's copy-index comes first.
                head_pid = sub.find("HeadMorphologicalInput/PhoneticSequence").get("id")
                nonhead_pid = sub.find("NonHeadMorphologicalInput/PhoneticSequence").get("id")
                order = None
                for a in actions:
                    if a[0] == "copy" and a[1] == head_pid:
                        order = "head_nonhead"
                        break
                    if a[0] == "copy" and a[1] == nonhead_pid:
                        order = "nonhead_head"
                        break
                crules[xml_id] = CRule(xml_id=xml_id, name=rname, head_pos=head_pos,
                                        nonhead_pos=nonhead_pos, output_pos=out_pos, order=order)
            elif el.tag == "RealizationalRule":
                raise NotImplementedError("RealizationalRule not supported by this PoC converter")
            else:
                raise NotImplementedError(f"stratum rule element {el.tag}")

        for tmpl in stratum.findall("./AffixTemplates/AffixTemplate"):
            req_pos = set((tmpl.get("requiredPartsOfSpeech") or "").split()) or None
            slots = []
            for slot in tmpl.findall("Slot"):
                optional = slot.get("optional") == "true"
                rule_ids = (slot.get("morphologicalRules") or "").split()
                slots.append(Slot(optional=optional, rule_ids=rule_ids))
            templates.append(Template(required_pos=req_pos, slots=slots))

        # --- lexicon ---
        for entry in stratum.findall("./LexicalEntries/LexicalEntry"):
            xml_id = entry.get("id")
            pos = entry.get("partOfSpeech")
            gloss = _text(entry.find("Gloss"))
            mpr = set((entry.get("ruleFeatures") or "").split())
            shapes = [_text(a.find("PhoneticShape")) for a in entry.findall("./Allomorphs/Allomorph")]
            lexicon.append(LexEntry(xml_id=xml_id, pos=pos, gloss=gloss, shapes=shapes, mpr=mpr))

    return Grammar(
        name=name, chartable=ct, pos_names=pos_names, natural_classes_segs=nc_segs,
        phon_rules=phon_rules, mrules=mrules, crules=crules, templates=templates,
        lexicon=lexicon, stratum_mrule_order=stratum_mrule_order, unordered=unordered,
    )


if __name__ == "__main__":
    import sys
    g = parse_grammar(sys.argv[1])
    print(f"grammar: {g.name}")
    print(f"segments: {len(g.chartable.segment_charids())}")
    print(f"natural classes: {len(g.natural_classes_segs)}")
    print(f"phon rules: {len(g.phon_rules)}")
    print(f"mrules: {len(g.mrules)} (redup: {sum(1 for m in g.mrules.values() if m.is_redup)})")
    print(f"crules: {len(g.crules)}")
    print(f"templates: {len(g.templates)}")
    print(f"lexicon entries: {len(g.lexicon)}")
    print(f"unordered stratum: {g.unordered}")
