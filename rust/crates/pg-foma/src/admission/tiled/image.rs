//! The tiled language as one acceptor: edge pieces, a root and edit zones pushed through the phonological cascade.

use super::classify::{Item, Side, Window};
use super::pieces::Pieces;
use super::roots::{trie_net, Roots};
use super::{Limits, TileCounts};
use crate::admission::build;
use crate::admission::meter::{self, size_sum, Meter};
use crate::admission::{AdmissionRefusal, Composition, Size};
use crate::cascade_parts::RulePart;
use foma::constructions::fsm_count;
use foma::options::FomaOptions;
use foma::structures::{fsm_empty_set, fsm_empty_string};
use foma::types::Fsm;
use pg_grammar::model::Grammar;
use std::collections::{BTreeMap, BTreeSet};

/// States plus arcs of unioned parts that are minimized together; a quarter of the size that draws a warning.
const UNION_CHUNK: usize = 250_000;

pub(super) fn any_text(meter: &Meter, opts: &FomaOptions, tokens: &[char]) -> Fsm {
    build::symbols_union(meter, opts, tokens.iter().map(|c| c.to_string()))
        .map(|net| meter.star(opts, net))
        .unwrap_or_else(fsm_empty_string)
}

/// The star of the pieces of one side: the literals, or any text when some side may hold any text.
fn pieces_star(meter: &Meter, opts: &FomaOptions, pieces: &[Side], alphabet: &[char]) -> Fsm {
    if pieces.contains(&Side::Any) {
        return any_text(meter, opts, alphabet);
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
    meter.star(opts, trie_net(meter, opts, &words))
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
        self.meter.check(0)?;
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
        self.meter.check(size_sum(&left) + size_sum(&right))?;
        let mut raw = self.meter.compose(self.opts, left, right);
        fsm_count(&mut raw);
        let before = meter::size(&raw);
        self.see(&format!("{name} before projecting"), before)?;
        self.meter.check(size_sum(&raw))?;
        let mut lower = self.meter.lower(raw);
        fsm_count(&mut lower);
        self.see(&format!("{name} lower side"), meter::size(&lower))?;
        self.meter.check(size_sum(&lower))?;
        let net = self.meter.minimize(self.opts, lower);
        let after = meter::size(&net);
        self.see(&format!("{name} minimized"), after)?;
        self.record(name, before, after, started);
        Ok(net)
    }

    /// Union of nets, minimized each time the pending parts pass `UNION_CHUNK`, so the sum of many parts is never held at once.
    fn union_min(&mut self, name: &str, parts: Vec<Fsm>) -> Result<Fsm, AdmissionRefusal> {
        let mut parts = parts.into_iter();
        let mut net = parts.next().unwrap_or_else(fsm_empty_set);
        let (mut pending, mut open) = (size_sum(&net), false);
        for part in parts {
            pending += size_sum(&part);
            net = self.meter.union(self.opts, net, part);
            open = true;
            if pending > UNION_CHUNK {
                net = self.minimize(name, net)?;
                (pending, open) = (size_sum(&net), false);
            }
        }
        match open {
            true => self.minimize(name, net),
            false => Ok(net),
        }
    }

    fn minimize(&mut self, name: &str, net: Fsm) -> Result<Fsm, AdmissionRefusal> {
        let started = std::time::Instant::now();
        let mut raw = net;
        fsm_count(&mut raw);
        let before = meter::size(&raw);
        self.see(&format!("{name} before minimizing"), before)?;
        self.meter.check(size_sum(&raw))?;
        let net = self.meter.minimize(self.opts, raw);
        let after = meter::size(&net);
        self.see(&format!("{name} minimized"), after)?;
        self.record(name, before, after, started);
        Ok(net)
    }
}

/// Segments of a length range with boundaries anywhere between them.
fn window(meter: &Meter, opts: &FomaOptions, tokens: &Tokens, (lo, hi): Window) -> Fsm {
    let unit = meter.concat(opts, tokens.boundaries.clone(), tokens.segment.clone());
    let mut net = fsm_empty_string();
    for _ in 0..lo {
        net = meter.concat(opts, net, unit.clone());
    }
    match hi {
        None => {
            let rest = meter.star(opts, unit);
            net = meter.concat(opts, net, rest);
        }
        Some(hi) => {
            for _ in lo..hi {
                let one = meter.optional(opts, unit.clone());
                net = meter.concat(opts, net, one);
            }
        }
    }
    meter.concat(opts, net, tokens.boundaries.clone())
}

fn item_net(meter: &Meter, opts: &FomaOptions, tokens: &Tokens, item: &Item) -> Fsm {
    match item {
        Item::Segments(lo, hi) => window(meter, opts, tokens, (*lo, *hi)),
        Item::Lit(texts, ..) => {
            let words: BTreeSet<String> = texts.iter().filter(|t| !t.is_empty()).cloned().collect();
            let net = trie_net(meter, opts, &words);
            match texts.iter().any(String::is_empty) {
                true => meter.union(opts, net, fsm_empty_string()),
                false => net,
            }
        }
    }
}

fn items_net(meter: &Meter, opts: &FomaOptions, tokens: &Tokens, items: &[Item]) -> Fsm {
    items.iter().fold(fsm_empty_string(), |net, item| {
        meter.concat(opts, net, item_net(meter, opts, tokens, item))
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
    let cut_window = window(&run.meter, run.opts, tokens, (0, Some(limit)));
    let cut = match front {
        true => run.meter.quotient_left(run.opts, cut_window, net),
        false => run.meter.quotient_right(run.opts, net, cut_window),
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
    let win = window(&run.meter, opts, tokens, key);
    let cut = match front {
        true => run.meter.quotient_left(opts, win, zone.clone()),
        false => run.meter.quotient_right(opts, zone.clone(), win),
    };
    let with_empty = run.meter.union(opts, cut, fsm_empty_string());
    let net = run.minimize(name, with_empty)?;
    kept.insert(key, net.clone());
    Ok(net)
}

fn same(meter: &Meter, opts: &FomaOptions, a: &Fsm, b: &Fsm) -> bool {
    meter::size(a) == meter::size(b) && meter.equivalent(opts, a, b)
}

/// The union of the parts, each minimized and skipped when equivalent to one already kept.
fn distinct_union(run: &mut Run, name: &str, parts: Vec<Fsm>) -> Result<Fsm, AdmissionRefusal> {
    let mut kept: Vec<Fsm> = Vec::new();
    for part in parts {
        let part = run.minimize(name, part)?;
        if !kept.iter().any(|p| same(&run.meter, run.opts, p, &part)) {
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
        let pre = items_net(&run.meter, opts, tokens, &edit.pre);
        lefts.push(run.meter.concat(opts, pre, dropped));
        let back = ("zone drop back", rc, edit.right, false);
        let dropped = cut_zone(run, tokens, &mut kept_right, back)?;
        let post = items_net(&run.meter, opts, tokens, &edit.post);
        rights.push(run.meter.concat(opts, dropped, post));
    }
    let lefts = distinct_union(run, "left part", lefts)?;
    let rights = distinct_union(run, "right part", rights)?;
    Ok((
        run.meter.concat(opts, left.clone(), lefts),
        run.meter.concat(opts, rights, right.clone()),
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
        let stable = same(&run.meter, run.opts, &next_left, &lc)
            && same(&run.meter, run.opts, &next_right, &rc);
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
    let anything = window(&run.meter, run.opts, tokens, (0, None));
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
    let left = pieces_star(&run.meter, opts, &pieces.inner_left, tokens_all);
    let right = pieces_star(&run.meter, opts, &pieces.inner_right, tokens_all);
    let front = (pieces.chop_left, true);
    let roots = chop(run, tokens, "root chop front", roots_all, front)?;
    let back = (pieces.chop_right, false);
    let roots = chop(run, tokens, "root chop back", roots, back)?;
    let (lc, rc) = edit_zones(run, pieces, tokens, (left, right), counts)?;
    let with_roots = run.meter.concat(opts, lc, roots);
    let mut core = run.meter.concat(opts, with_roots, rc);
    if pieces.repeat_inner {
        core = run.meter.plus(opts, core);
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
        let identity = build::identity_cascade(&run.meter, opts, &g.char_tables[0]);
        net = run.project("cascade", net, identity)?;
    }
    for part in rules {
        net = run.project(&part.rule, net, part.net)?;
    }
    if let Some(spelling) = &spelling {
        net = run.project("spelling", net, spelling.clone())?;
    }
    net = run.project("orthography", net, spelled.clone())?;
    let outer_left = pieces_star(&run.meter, opts, &pieces.outer_left, tokens_all);
    let outer_right = pieces_star(&run.meter, opts, &pieces.outer_right, tokens_all);
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
        let with_left = run.meter.concat(opts, l, whole);
        whole = run.meter.concat(opts, with_left, r);
        whole = run.minimize("padded", whole)?;
    }
    if let Some(deep) = roots.deep {
        let with_left = run.meter.concat(opts, outer_left, deep);
        let mut bare = run.meter.concat(opts, with_left, outer_right);
        bare = run.minimize("deep core", bare)?;
        bare = through(run, "deep", bare)?;
        let both = run.meter.union(opts, whole, bare);
        whole = run.minimize("whole", both)?;
    }
    if pieces.repeat_outer {
        let again = run.meter.plus(opts, whole);
        whole = run.minimize("repeated", again)?;
    }
    Ok(whole)
}
