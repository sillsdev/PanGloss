//! Natural-phrases N1 (`docs/natural-phrases-plan.md` N1): the typed intermediate
//! representation a bundle's tokens get mapped into, plus `to_ir`, the function that builds
//! one. Closed enums, not stringly features (`docs/natural-glosses-plan.md` §6.3's proposed
//! record made concrete for the Rust side).
//!
//! Additive on top of N0's `crate::GlossBundle`: `to_ir` only reads a bundle and a
//! `crate::map::RealizeMap`, never touches parity output, and — like `crate::gloss_bundle`
//! and `crate::leipzig` — is a total function: no input produces a panic or a fatal error,
//! only more material in `GlossIr::extras`.
#![forbid(unsafe_code)]

use crate::map::{FeatureAssignment, RealizeMap};
use crate::{GlossBundle, GlossToken};

/// The word's headword concept: either a real lexical citation form (a root's `<Gloss>`), or,
/// for a guessed root (`GlossBundle::guessed` with no lexicon entry to draw a citation form
/// from), the surface word itself standing in for the concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Concept {
    /// The root morpheme's gloss (its citation form), e.g. `"house"`.
    Lex(String),
    /// No lexical gloss available (a guess-branch analysis with no lexicon entry); the surface word itself, e.g. `Guessed("mbal")`.
    Guessed(String),
}

/// Grammatical number, as amharic's `pl` gloss (and any other grammar's plural-marking
/// morpheme) resolves to. `Unspec` is the default: no morpheme in the bundle assigned `Num`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Num {
    Unspec,
    Sg,
    Pl,
}

impl Num {
    /// All variants, in declaration order — used by `crate::table::TableRealizer`'s 108-cell
    /// (`docs/natural-phrases-plan.md` N2) full-coverage validation.
    pub(crate) const ALL: [Num; 3] = [Num::Unspec, Num::Sg, Num::Pl];

    pub(crate) fn parse(value: &str) -> Option<Num> {
        match value {
            "Unspec" => Some(Num::Unspec),
            "Sg" => Some(Num::Sg),
            "Pl" => Some(Num::Pl),
            _ => None,
        }
    }
}

/// Possessor person/number/gender, derived directly from amharic-hc.xml's own possessive-gloss
/// inventory (`samples/data/amharic-hc.xml`, `<Gloss>poss.*</Gloss>`, verified by grepping every
/// `<Gloss>` in the file): `poss.1s`, `poss.1p`, `poss.2m`, `poss.2f`, `poss.2p`, `poss.3m`,
/// `poss.3f`, `poss.3p` — eight distinctions, no bare `poss.2`/`poss.3` and no `poss.1f`/
/// `poss.1m` (1st person doesn't gender-distinguish in this grammar's inventory; 2nd/3rd person
/// singular do, 2nd/3rd person plural don't). Correspondence:
///
/// | amharic gloss | `Poss` variant |
/// |---|---|
/// | `poss.1s`      | `P1Sg`  |
/// | `poss.1p`      | `P1Pl`  |
/// | `poss.2m`      | `P2SgM` |
/// | `poss.2f`      | `P2SgF` |
/// | `poss.2p`      | `P2Pl`  |
/// | `poss.3m`      | `P3SgM` |
/// | `poss.3f`      | `P3SgF` |
/// | `poss.3p`      | `P3Pl`  |
///
/// `None` is the default: no morpheme in the bundle assigned `Poss` (an unpossessed noun).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Poss {
    None,
    P1Sg,
    P1Pl,
    P2SgM,
    P2SgF,
    P2Pl,
    P3SgM,
    P3SgF,
    P3Pl,
}

impl Poss {
    /// All variants, in declaration order — used by `crate::table::TableRealizer`'s 108-cell
    /// (`docs/natural-phrases-plan.md` N2) full-coverage validation.
    pub(crate) const ALL: [Poss; 9] = [
        Poss::None,
        Poss::P1Sg,
        Poss::P1Pl,
        Poss::P2SgM,
        Poss::P2SgF,
        Poss::P2Pl,
        Poss::P3SgM,
        Poss::P3SgF,
        Poss::P3Pl,
    ];

    pub(crate) fn parse(value: &str) -> Option<Poss> {
        match value {
            "None" => Some(Poss::None),
            "P1Sg" => Some(Poss::P1Sg),
            "P1Pl" => Some(Poss::P1Pl),
            "P2SgM" => Some(Poss::P2SgM),
            "P2SgF" => Some(Poss::P2SgF),
            "P2Pl" => Some(Poss::P2Pl),
            "P3SgM" => Some(Poss::P3SgM),
            "P3SgF" => Some(Poss::P3SgF),
            "P3Pl" => Some(Poss::P3Pl),
            _ => None,
        }
    }
}

/// Case/adposition role. amharic's nominal adposition-like suffixes: `at` (locative), `from`
/// (ablative; `abl` is a second rule realizing the same ablative meaning — see
/// `samples/data/amharic-realize.toml`'s comment), `to` (allative). `None` is the default: no
/// morpheme in the bundle assigned `Case`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaseRole {
    None,
    Loc,
    Abl,
    All,
}

impl CaseRole {
    /// All variants, in declaration order — used by `crate::table::TableRealizer`'s 108-cell
    /// (`docs/natural-phrases-plan.md` N2) full-coverage validation.
    pub(crate) const ALL: [CaseRole; 4] =
        [CaseRole::None, CaseRole::Loc, CaseRole::Abl, CaseRole::All];

    pub(crate) fn parse(value: &str) -> Option<CaseRole> {
        match value {
            "None" => Some(CaseRole::None),
            "Loc" => Some(CaseRole::Loc),
            "Abl" => Some(CaseRole::Abl),
            "All" => Some(CaseRole::All),
            _ => None,
        }
    }
}

/// The typed intermediate representation `to_ir` builds from a `GlossBundle`: a closed-enum
/// concept + feature slots, plus `extras` for everything the mapping chain couldn't place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlossIr {
    pub concept: Concept,
    pub num: Num,
    pub poss: Poss,
    pub case: CaseRole,
    /// Display strings (a token's gloss, or `"[?]"` when it has none) for tokens that were
    /// mapped to nothing (no property, no sidecar entry) or that assigned a feature slot a
    /// prior token already claimed (the conflict rule: first assignment wins, every later
    /// conflicting one lands here verbatim instead of being silently dropped). In original
    /// token order.
    pub extras: Vec<String>,
}

/// Build a `GlossIr` from a `GlossBundle`, a `RealizeMap` sidecar (or
/// `RealizeMap::empty`), and the analysis's surface word. Total: never panics, never fails.
///
/// Root resolution: `bundle.root_index` (defensively re-validated against `bundle.tokens.len()`
/// here, same defensive posture as `crate::gloss_bundle` itself) names the root token, which
/// becomes `concept` and does **not** also flow through the feature-mapping chain below. A
/// guessed root (`bundle.guessed`) becomes `Concept::Guessed(surface_word)`; an unglossed real
/// root becomes `Concept::Lex("[?]")` (matching N0's `leipzig` rendering rule); a glossed real
/// root becomes `Concept::Lex(gloss)`. When there is no valid root index (defensive only — the
/// real engine always sets one), `concept` is `Concept::Lex("[?]")` and **every** token,
/// including what would have been the root, flows through the feature-mapping chain.
///
/// Every non-root token resolves through this priority chain (`docs/natural-phrases-plan.md`
/// N1's mapping priority order):
///
/// 1. A `realize` morpheme property (`"Feature:Value"` syntax, same grammar as the sidecar
///    file). A parse error here (grammar-author data, not curated) degrades to unmapped —
///    falls through to step 2, not straight to `extras`.
/// 2. The sidecar map, keyed by the token's gloss string (no gloss -> no lookup, falls through).
/// 3. Unmapped: the token's display string goes to `extras`.
///
/// Conflict rule: if a resolved assignment targets a feature slot (`num`/`poss`/`case`) a prior
/// token already filled, the later token's display string goes to `extras` instead — never
/// silently lost, never overwriting the first assignment. `Ignore` assignments drop the token
/// entirely (not even `extras`).
pub fn to_ir(bundle: &GlossBundle, map: &RealizeMap, surface_word: &str) -> GlossIr {
    let root_idx = bundle.root_index.filter(|&i| i < bundle.tokens.len());

    let concept = match root_idx.and_then(|i| bundle.tokens.get(i)) {
        Some(root_token) if bundle.guessed => {
            let _ = root_token; // guessed roots never carry a usable gloss; surface word wins.
            Concept::Guessed(surface_word.to_string())
        }
        Some(root_token) => match &root_token.gloss {
            Some(g) => Concept::Lex(g.clone()),
            None => Concept::Lex("[?]".to_string()),
        },
        None => Concept::Lex("[?]".to_string()),
    };

    let mut num: Option<Num> = None;
    let mut poss: Option<Poss> = None;
    let mut case: Option<CaseRole> = None;
    let mut extras: Vec<String> = Vec::new();

    for (i, token) in bundle.tokens.iter().enumerate() {
        if root_idx == Some(i) {
            continue;
        }
        match resolve_assignment(token, map) {
            Some(FeatureAssignment::Ignore) => {}
            Some(FeatureAssignment::Num(n)) => assign_or_extra(&mut num, n, token, &mut extras),
            Some(FeatureAssignment::Poss(p)) => assign_or_extra(&mut poss, p, token, &mut extras),
            Some(FeatureAssignment::Case(c)) => assign_or_extra(&mut case, c, token, &mut extras),
            None => extras.push(display_token(token)),
        }
    }

    GlossIr {
        concept,
        num: num.unwrap_or(Num::Unspec),
        poss: poss.unwrap_or(Poss::None),
        case: case.unwrap_or(CaseRole::None),
        extras,
    }
}

/// Priority chain steps 1-2: `realize` property, then sidecar-by-gloss; `None` means unmapped, so the caller pushes to `extras`.
fn resolve_assignment(token: &GlossToken, map: &RealizeMap) -> Option<FeatureAssignment> {
    if let Some((_, raw)) = token.properties.iter().find(|(k, _)| k == "realize") {
        if let Ok(assignment) = crate::map::parse_feature_assignment(raw) {
            return Some(assignment);
        }
        // Parse error in grammar-author data: degrade to unmapped by this source, fall through to the sidecar.
    }
    let gloss = token.gloss.as_deref()?;
    map.lookup(gloss)
}

/// First writer to a feature slot wins; a later conflicting token's display string goes to `extras` instead of being dropped or silently overwriting.
fn assign_or_extra<T>(
    slot: &mut Option<T>,
    value: T,
    token: &GlossToken,
    extras: &mut Vec<String>,
) {
    if slot.is_none() {
        *slot = Some(value);
    } else {
        extras.push(display_token(token));
    }
}

/// A token's fallback display string, the same rule `leipzig` uses for a non-root token: its gloss, or `"[?]"` when it has none.
fn display_token(token: &GlossToken) -> String {
    token.gloss.clone().unwrap_or_else(|| "[?]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GlossToken;

    fn tok(gloss: Option<&str>, is_root: bool) -> GlossToken {
        GlossToken {
            gloss: gloss.map(str::to_string),
            properties: Vec::new(),
            is_root,
        }
    }

    fn tok_with_property(gloss: Option<&str>, prop_value: &str) -> GlossToken {
        GlossToken {
            gloss: gloss.map(str::to_string),
            properties: vec![("realize".to_string(), prop_value.to_string())],
            is_root: false,
        }
    }

    fn bundle(tokens: Vec<GlossToken>, root_index: Option<usize>, guessed: bool) -> GlossBundle {
        GlossBundle {
            tokens,
            root_index,
            pos_id: None,
            guessed,
        }
    }

    fn sidecar() -> RealizeMap {
        RealizeMap::parse(
            "[features]\n\"pl\" = \"Num:Pl\"\n\"poss.1s\" = \"Poss:P1Sg\"\n\"at\" = \"Case:Loc\"\n",
        )
        .expect("valid sidecar")
    }

    #[test]
    fn property_takes_priority_over_sidecar() {
        // gloss "pl" is in the sidecar as Num:Pl, but the property says Num:Sg -- property wins.
        let t = GlossToken {
            gloss: Some("pl".to_string()),
            properties: vec![("realize".to_string(), "Num:Sg".to_string())],
            is_root: false,
        };
        let b = bundle(vec![tok(Some("house"), true), t], Some(0), false);
        let ir = to_ir(&b, &sidecar(), "house");
        assert_eq!(ir.num, Num::Sg);
    }

    #[test]
    fn malformed_property_degrades_to_sidecar_lookup() {
        let t = GlossToken {
            gloss: Some("pl".to_string()),
            properties: vec![("realize".to_string(), "not valid".to_string())],
            is_root: false,
        };
        let b = bundle(vec![tok(Some("house"), true), t], Some(0), false);
        let ir = to_ir(&b, &sidecar(), "house");
        assert_eq!(
            ir.num,
            Num::Pl,
            "bad property should fall through to sidecar"
        );
    }

    #[test]
    fn conflict_first_wins_rest_to_extras() {
        let t1 = tok_with_property(Some("pl"), "Num:Pl");
        let t2 = tok_with_property(Some("dup"), "Num:Sg");
        let b = bundle(vec![tok(Some("house"), true), t1, t2], Some(0), false);
        let ir = to_ir(&b, &RealizeMap::empty(), "house");
        assert_eq!(ir.num, Num::Pl, "first assignment wins");
        assert_eq!(ir.extras, vec!["dup".to_string()]);
    }

    #[test]
    fn ignore_drops_token_from_extras() {
        let t = tok_with_property(Some("epenthetic"), "Ignore");
        let b = bundle(vec![tok(Some("house"), true), t], Some(0), false);
        let ir = to_ir(&b, &RealizeMap::empty(), "house");
        assert!(ir.extras.is_empty());
    }

    #[test]
    fn unmapped_token_goes_to_extras_verbatim() {
        let t = tok(Some("obj"), false);
        let b = bundle(vec![tok(Some("house"), true), t], Some(0), false);
        let ir = to_ir(&b, &RealizeMap::empty(), "house");
        assert_eq!(ir.extras, vec!["obj".to_string()]);
    }

    #[test]
    fn unmapped_ungossed_token_goes_to_extras_as_bracket_question() {
        let t = tok(None, false);
        let b = bundle(vec![tok(Some("house"), true), t], Some(0), false);
        let ir = to_ir(&b, &RealizeMap::empty(), "house");
        assert_eq!(ir.extras, vec!["[?]".to_string()]);
    }

    #[test]
    fn glossed_root_becomes_lex_concept() {
        let b = bundle(vec![tok(Some("house"), true)], Some(0), false);
        let ir = to_ir(&b, &RealizeMap::empty(), "house");
        assert_eq!(ir.concept, Concept::Lex("house".to_string()));
    }

    #[test]
    fn unglossed_real_root_becomes_bracket_question_lex_concept() {
        let b = bundle(vec![tok(None, true)], Some(0), false);
        let ir = to_ir(&b, &RealizeMap::empty(), "mystery");
        assert_eq!(ir.concept, Concept::Lex("[?]".to_string()));
    }

    #[test]
    fn guessed_root_becomes_guessed_concept_with_surface_word() {
        let b = bundle(vec![tok(None, true)], Some(0), true);
        let ir = to_ir(&b, &RealizeMap::empty(), "gag");
        assert_eq!(ir.concept, Concept::Guessed("gag".to_string()));
    }

    #[test]
    fn no_root_index_yields_bracket_question_and_all_tokens_through_chain() {
        let t = tok_with_property(Some("pl"), "Num:Pl");
        let b = bundle(vec![tok(Some("house"), false), t], None, false);
        let ir = to_ir(&b, &RealizeMap::empty(), "house");
        assert_eq!(ir.concept, Concept::Lex("[?]".to_string()));
        // "house" itself, with no root to consume it, is unmapped -> extras.
        assert_eq!(ir.extras, vec!["house".to_string()]);
        assert_eq!(ir.num, Num::Pl);
    }

    #[test]
    fn empty_bundle_never_panics() {
        let b = bundle(vec![], None, false);
        let ir = to_ir(&b, &RealizeMap::empty(), "");
        assert_eq!(ir.concept, Concept::Lex("[?]".to_string()));
        assert_eq!(ir.num, Num::Unspec);
        assert_eq!(ir.poss, Poss::None);
        assert_eq!(ir.case, CaseRole::None);
        assert!(ir.extras.is_empty());
    }

    #[test]
    fn full_priority_and_multiple_features_together() {
        // root "house" + Num:Pl (property) + Poss:P1Sg (sidecar) + Case:Loc (sidecar) + one unmapped extra token ("obj").
        let root = tok(Some("house"), true);
        let pl = tok_with_property(Some("pl-ish"), "Num:Pl");
        let poss = tok(Some("poss.1s"), false);
        let case = tok(Some("at"), false);
        let extra = tok(Some("obj"), false);
        let b = bundle(vec![root, pl, poss, case, extra], Some(0), false);
        let ir = to_ir(&b, &sidecar(), "house");
        assert_eq!(ir.concept, Concept::Lex("house".to_string()));
        assert_eq!(ir.num, Num::Pl);
        assert_eq!(ir.poss, Poss::P1Sg);
        assert_eq!(ir.case, CaseRole::Loc);
        assert_eq!(ir.extras, vec!["obj".to_string()]);
    }
}
