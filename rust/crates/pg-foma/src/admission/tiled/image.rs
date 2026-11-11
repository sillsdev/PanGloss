//! The tiled language as one acceptor: edge pieces, a root and edit zones pushed through the phonological cascade.

use super::classify::{Item, Side, Window};
use super::pieces::Pieces;
use super::roots::{trie_net, Roots};
use super::{Limits, TileCounts};
use crate::admission::build::{self, Meter};
use crate::admission::{AdmissionRefusal, Composition, Size};
use crate::cascade_parts::RulePart;
use foma::constructions::{
    fsm_compose, fsm_concat, fsm_count, fsm_equivalent, fsm_kleene_plus, fsm_kleene_star,
    fsm_optionality, fsm_quotient_left, fsm_quotient_right, fsm_union,
};
use foma::extract::fsm_lower;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::structures::{fsm_empty_set, fsm_empty_string};
use foma::types::Fsm;
use pg_grammar::model::Grammar;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn any_text(opts: &FomaOptions, tokens: &[char]) -> Fsm {
    build::symbols_union(opts, tokens.iter().map(|c| c.to_string()))
        .map(|net| fsm_kleene_star(opts, net))
        .unwrap_or_else(fsm_empty_string)
}

/// The star of the pieces of one side: the literals, or any text when some side may hold any text.
fn pieces_star(opts: &FomaOptions, pieces: &[Side], alphabet: &[char]) -> Fsm {
    if pieces.contains(&Side::Any) {
        return any_text(opts, alphabet);
    }
    let words: BTreeSet<String> = pieces
        .iter()
        .flat_map(|p| match p {
            Side::Lit(texts) => texts.clone(),
            Side::Any => Vec::new(),
        })
        .filter(|text| !text.is_empty())
        .collect();
    if words.is_empty() {
        return fsm_empty_string();
    }
    fsm_kleene_star(opts, trie_net(opts, &words))
}

/// One segment, and any run of boundaries.
pub(super) struct Tokens {
    pub(super) segment: Fsm,
    pub(super) boundaries: Fsm,
}

/// The cascade after the stem: the spelling relation when there is one, the rules in order, and the orthography.
pub(super) type Cascade = (Option<Fsm>, Vec<RulePart>, Fsm);

/// Composition and minimizing under the size bounds, recording what was built.
pub(super) struct Run<'a> {
    pub(super) opts: &'a FomaOptions,
    pub(super) meter: Meter,
    pub(super) limits: Limits,
    pub(super) largest: usize,
    pub(super) steps: Vec<Composition>,
}

impl Run<'_> {
    fn see(&mut self, name: &str, size: Size) -> Result<(), AdmissionRefusal> {
        let total = size.states + size.arcs;
        self.largest = self.largest.max(total);
        self.meter.observe(&format!("tiled {name}"), size, &[])?;
        if total > self.limits.stop {
            return Err(AdmissionRefusal::Containment {
                structure: format!("tiled {name}"),
                size: total,
            });
        }
        Ok(())
    }

    fn record(&mut self, name: &str, before: Size, after: Size, started: std::time::Instant) {
        self.steps.push(Composition {
            structure: name.to_owned(),
            before,
            after,
            ms: started.elapsed().as_millis() as u64,
        });
    }

    /// Composes the acceptor with a relation and keeps the lower side, minimized.
    fn project(&mut self, name: &str, left: Fsm, right: Fsm) -> Result<Fsm, AdmissionRefusal> {
        let started = std::time::Instant::now();
        let mut raw = fsm_compose(self.opts, left, right);
        fsm_count(&mut raw);
        let before = build::size(&raw);
        self.see(&format!("{name} before projecting"), before)?;
        let mut lower = fsm_lower(raw);
        fsm_count(&mut lower);
        self.see(&format!("{name} lower side"), build::size(&lower))?;
        let net = fsm_minimize(self.opts, lower);
        let after = build::size(&net);
        self.see(&format!("{name} minimized"), after)?;
        self.record(name, before, after, started);
        Ok(net)
    }

    /// Union of nets, minimized after each addition so no intermediate holds the sum of the parts.
    fn union_min(&mut self, name: &str, parts: Vec<Fsm>) -> Result<Fsm, AdmissionRefusal> {
        let mut parts = parts.into_iter();
        let mut net = parts.next().unwrap_or_else(fsm_empty_set);
        for part in parts {
            net = self.minimize(name, fsm_union(self.opts, net, part))?;
        }
        Ok(net)
    }

    fn minimize(&mut self, name: &str, net: Fsm) -> Result<Fsm, AdmissionRefusal> {
        let started = std::time::Instant::now();
        let mut raw = net;
        fsm_count(&mut raw);
        let before = build::size(&raw);
        self.see(&format!("{name} before minimizing"), before)?;
        let net = fsm_minimize(self.opts, raw);
        let after = build::size(&net);
        self.see(&format!("{name} minimized"), after)?;
        self.record(name, before, after, started);
        Ok(net)
    }
}

/// Segments of a length range with boundaries anywhere between them.
fn window(opts: &FomaOptions, tokens: &Tokens, (lo, hi): Window) -> Fsm {
    let unit = fsm_concat(opts, tokens.boundaries.clone(), tokens.segment.clone());
    let mut net = fsm_empty_string();
    for _ in 0..lo {
        net = fsm_concat(opts, net, unit.clone());
    }
    match hi {
        None => net = fsm_concat(opts, net, fsm_kleene_star(opts, unit)),
        Some(hi) => {
            for _ in lo..hi {
                net = fsm_concat(opts, net, fsm_optionality(opts, unit.clone()));
            }
        }
    }
    fsm_concat(opts, net, tokens.boundaries.clone())
}

fn item_net(opts: &FomaOptions, tokens: &Tokens, item: &Item) -> Fsm {
    match item {
        Item::Segments(lo, hi) => window(opts, tokens, (*lo, *hi)),
        Item::Lit(texts, ..) => {
            let words: BTreeSet<String> = texts.iter().filter(|t| !t.is_empty()).cloned().collect();
            let net = trie_net(opts, &words);
            match texts.iter().any(String::is_empty) {
                true => fsm_union(opts, net, fsm_empty_string()),
                false => net,
            }
        }
    }
}

fn items_net(opts: &FomaOptions, tokens: &Tokens, items: &[Item]) -> Fsm {
    items.iter().fold(fsm_empty_string(), |net, item| {
        fsm_concat(opts, net, item_net(opts, tokens, item))
    })
}

/// The net with up to `limit` segments cut from its front or back.
fn chop(
    run: &mut Run,
    tokens: &Tokens,
    name: &str,
    net: Fsm,
    (limit, front): (usize, bool),
) -> Result<Fsm, AdmissionRefusal> {
    if limit == 0 {
        return Ok(net);
    }
    let cut_window = window(run.opts, tokens, (0, Some(limit)));
    let cut = match front {
        true => fsm_quotient_left(run.opts, cut_window, net),
        false => fsm_quotient_right(run.opts, net, cut_window),
    };
    run.minimize(name, cut)
}

/// The zone with a window cut from its edge or left whole, built once per distinct window.
fn cut_zone(
    run: &mut Run,
    tokens: &Tokens,
    kept: &mut BTreeMap<Window, Fsm>,
    (name, zone, key, front): (&str, &Fsm, Window, bool),
) -> Result<Fsm, AdmissionRefusal> {
    if let Some(net) = kept.get(&key) {
        return Ok(net.clone());
    }
    let opts = run.opts;
    let win = window(opts, tokens, key);
    let cut = match front {
        true => fsm_quotient_left(opts, win, zone.clone()),
        false => fsm_quotient_right(opts, zone.clone(), win),
    };
    let net = run.minimize(name, fsm_union(opts, cut, fsm_empty_string()))?;
    kept.insert(key, net.clone());
    Ok(net)
}

fn same(opts: &FomaOptions, a: &Fsm, b: &Fsm) -> bool {
    build::size(a) == build::size(b) && fsm_equivalent(opts, a.clone(), b.clone())
}

/// The union of the parts, each minimized and skipped when equivalent to one already kept.
fn distinct_union(run: &mut Run, name: &str, parts: Vec<Fsm>) -> Result<Fsm, AdmissionRefusal> {
    let mut kept: Vec<Fsm> = Vec::new();
    for part in parts {
        let part = run.minimize(name, part)?;
        if !kept.iter().any(|p| same(run.opts, p, &part)) {
            kept.push(part);
        }
    }
    run.union_min(&format!("{name} union"), kept)
}

/// One round of the zones around the stem: each edit applied to every member of the zone, plus the zone itself and the zone cut at the other edge.
fn zone_round(
    run: &mut Run,
    pieces: &Pieces,
    tokens: &Tokens,
    (left, right): (&Fsm, &Fsm),
    (lc, rc): (&Fsm, &Fsm),
) -> Result<(Fsm, Fsm), AdmissionRefusal> {
    let opts = run.opts;
    let (wl, wr) = (pieces.chop_left, pieces.chop_right);
    let mut lefts = vec![
        lc.clone(),
        chop(run, tokens, "zone back", lc.clone(), (wr, false))?,
    ];
    let mut rights = vec![
        rc.clone(),
        chop(run, tokens, "zone front", rc.clone(), (wl, true))?,
    ];
    let (mut kept_left, mut kept_right) = (BTreeMap::new(), BTreeMap::new());
    for edit in &pieces.windowed {
        let front = ("zone drop front", lc, edit.left, true);
        let dropped = cut_zone(run, tokens, &mut kept_left, front)?;
        let pre = items_net(opts, tokens, &edit.pre);
        lefts.push(fsm_concat(opts, pre, dropped));
        let back = ("zone drop back", rc, edit.right, false);
        let dropped = cut_zone(run, tokens, &mut kept_right, back)?;
        let post = items_net(opts, tokens, &edit.post);
        rights.push(fsm_concat(opts, dropped, post));
    }
    let lefts = distinct_union(run, "left part", lefts)?;
    let rights = distinct_union(run, "right part", rights)?;
    Ok((
        fsm_concat(opts, left.clone(), lefts),
        fsm_concat(opts, rights, right.clone()),
    ))
}

/// The zones before and after the stem, iterated to a fixed point; any text when the rounds do not settle in time.
fn edit_zones(
    run: &mut Run,
    pieces: &Pieces,
    tokens: &Tokens,
    (left, right): (Fsm, Fsm),
    counts: &mut TileCounts,
) -> Result<(Fsm, Fsm), AdmissionRefusal> {
    let wanted = match pieces.windowed.is_empty() {
        true => 0,
        false => pieces.edit_apps,
    };
    let (mut lc, mut rc) = (left.clone(), right.clone());
    let mut converged = wanted == 0;
    for round in 0..wanted.min(run.limits.rounds) {
        let (next_left, next_right) = zone_round(run, pieces, tokens, (&left, &right), (&lc, &rc))?;
        let next_left = run.minimize(&format!("left zone {round}"), next_left)?;
        let next_right = run.minimize(&format!("right zone {round}"), next_right)?;
        let stable = same(run.opts, &next_left, &lc) && same(run.opts, &next_right, &rc);
        (lc, rc) = (next_left, next_right);
        counts.edit_rounds = round + 1;
        if stable {
            converged = true;
            break;
        }
    }
    converged |= counts.edit_rounds >= wanted;
    counts.edit_converged = converged;
    counts.edit_widened = !converged;
    if converged {
        return Ok((lc, rc));
    }
    let anything = window(run.opts, tokens, (0, None));
    Ok((anything.clone(), anything))
}

/// The inner language: zones around a root, repeated when a rule copies whole stems.
fn core_language(
    run: &mut Run,
    pieces: &Pieces,
    roots_all: Fsm,
    (tokens_all, tokens): (&[char], &Tokens),
    counts: &mut TileCounts,
) -> Result<Fsm, AdmissionRefusal> {
    let opts = run.opts;
    let left = pieces_star(opts, &pieces.inner_left, tokens_all);
    let right = pieces_star(opts, &pieces.inner_right, tokens_all);
    let front = (pieces.chop_left, true);
    let roots = chop(run, tokens, "root chop front", roots_all, front)?;
    let back = (pieces.chop_right, false);
    let roots = chop(run, tokens, "root chop back", roots, back)?;
    let (lc, rc) = edit_zones(run, pieces, tokens, (left, right), counts)?;
    let mut core = fsm_concat(opts, fsm_concat(opts, lc, roots), rc);
    if pieces.repeat_inner {
        core = fsm_kleene_plus(opts, core);
    }
    run.minimize("core", core)
}

/// The whole tiled language on the surface side.
pub(super) fn tile(
    run: &mut Run,
    pieces: &Pieces,
    roots: Roots,
    (spelling, rules, spelled): Cascade,
    (tokens_all, tokens): (&[char], &Tokens),
    (g, counts): (&Grammar, &mut TileCounts),
) -> Result<Fsm, AdmissionRefusal> {
    let opts = run.opts;
    let core = core_language(run, pieces, roots.all.clone(), (tokens_all, tokens), counts)?;
    let mut net = core;
    if spelling.is_none() {
        let identity = build::identity_cascade(opts, &g.char_tables[0]);
        net = run.project("cascade", net, identity)?;
    }
    for part in rules {
        net = run.project(&part.rule, net, part.net)?;
    }
    if let Some(spelling) = &spelling {
        net = run.project("spelling", net, spelling.clone())?;
    }
    net = run.project("orthography", net, spelled.clone())?;
    let outer_left = pieces_star(opts, &pieces.outer_left, tokens_all);
    let outer_right = pieces_star(opts, &pieces.outer_right, tokens_all);
    let outer_any = !pieces.outer_left.is_empty() || !pieces.outer_right.is_empty();
    // Outer pieces reach the surface through the spelling only.
    let through = |run: &mut Run, name: &str, side: Fsm| -> Result<Fsm, AdmissionRefusal> {
        let mut net = side;
        if let Some(spelling) = &spelling {
            net = run.project(&format!("{name} spelling"), net, spelling.clone())?;
        }
        run.project(&format!("{name} orthography"), net, spelled.clone())
    };
    let mut whole = net;
    if outer_any {
        let l = through(run, "outer left", outer_left.clone())?;
        let r = through(run, "outer right", outer_right.clone())?;
        whole = fsm_concat(opts, fsm_concat(opts, l, whole), r);
        whole = run.minimize("padded", whole)?;
    }
    if let Some(deep) = roots.deep {
        let mut bare = fsm_concat(opts, fsm_concat(opts, outer_left, deep), outer_right);
        bare = run.minimize("deep core", bare)?;
        bare = through(run, "deep", bare)?;
        whole = run.minimize("whole", fsm_union(opts, whole, bare))?;
    }
    if pieces.repeat_outer {
        whole = run.minimize("repeated", fsm_kleene_plus(opts, whole))?;
    }
    Ok(whole)
}
