//! WS2 tests over a three-layer stack `Lex | Fold | Err` with an exhaustive oracle.

use std::collections::{BTreeMap, BTreeSet};

use super::spell_read::{Label, SpellModel};
use super::speller::*;
use super::*;

const UNIVERSE: u32 = 8;
const MARK: u32 = 5;
const CHARS: [char; 6] = ['a', 'b', 'c', 'A', 'B', '\u{301}'];

fn fold(t: u32) -> Option<u32> {
    match t {
        0..=2 => Some(t),
        3 => Some(0),
        4 => Some(1),
        _ => None,
    }
}

// ---------------------------------------------------------------- the stack

#[derive(Clone, Debug)]
struct Entry {
    toks: Vec<u32>,
    tags: Vec<Tag>,
}

#[derive(Clone, Copy, Debug)]
struct Weights {
    sub: u32,
    del: u32,
    ins: u32,
}

impl Weights {
    const UNIT: Weights = Weights {
        sub: 1000,
        del: 1000,
        ins: 1000,
    };
}

fn lex_layer(entries: &[Entry], mark_loop: bool) -> NfaLayer {
    let mut meter = BuildMeter::default_limits();
    let mut b = NfaBuilder::new("lex", UNIVERSE, &mut meter);
    let start = b.add_state(false).unwrap();
    for e in entries {
        let mut at = start;
        for (i, &t) in e.toks.iter().enumerate() {
            let last = i + 1 == e.toks.len();
            let to = b.add_state(last).unwrap();
            let tags: &[Tag] = if i == 0 { &e.tags } else { &[] };
            b.add_arc(at, Input::Eps, Output::Tok(t), to, tags, 0, 0)
                .unwrap();
            if last && mark_loop {
                b.add_arc(to, Input::Eps, Output::Tok(MARK), to, &[], 0, 0)
                    .unwrap();
            }
            at = to;
        }
    }
    b.finish()
}

fn fold_layer() -> NfaLayer {
    let mut meter = BuildMeter::default_limits();
    let mut b = NfaBuilder::new("fold", UNIVERSE, &mut meter);
    let s = b.add_state(true).unwrap();
    for t in 0..=MARK {
        let out = fold(t).map_or(Output::Eps, Output::Tok);
        b.add_arc(s, Input::Tok(t), out, s, &[], 0, 0).unwrap();
    }
    b.finish()
}

fn err_layer(w: Weights) -> NfaLayer {
    let mut meter = BuildMeter::default_limits();
    let mut b = NfaBuilder::new("err", UNIVERSE, &mut meter);
    let s = b.add_state(true).unwrap();
    for x in 0..3 {
        b.add_arc(s, Input::Tok(x), Output::Tok(x), s, &[], 0, 0)
            .unwrap();
        b.add_arc(s, Input::Tok(x), Output::Eps, s, &[], w.del, 1)
            .unwrap();
        b.add_arc(s, Input::Eps, Output::Tok(x), s, &[], w.ins, 1)
            .unwrap();
        for y in 0..3 {
            if x != y {
                b.add_arc(s, Input::Tok(x), Output::Tok(y), s, &[], w.sub, 1)
                    .unwrap();
            }
        }
    }
    b.finish()
}

struct Model {
    typed_exact: Vec<u32>,
    stack_loop: bool,
}

impl SpellModel for Model {
    fn observe(&self, mv: &MoveView) -> Label {
        let written = mv.out_of(0);
        let folded = mv.out_of(1);
        let case = match (written, folded) {
            (Some(w), Some(_)) => {
                let matched = mv.edits == 0 && mv.to_pos == mv.from_pos + 1;
                if matched {
                    self.typed_exact.get(mv.from_pos).is_none_or(|&t| t != w)
                } else {
                    w >= 3
                }
            }
            _ => false,
        };
        Label {
            written,
            folded,
            case,
        }
    }
    fn written_char(&self, t: u32) -> char {
        CHARS[t as usize]
    }
    fn written_token(&self, c: char) -> Option<u32> {
        CHARS.iter().position(|&x| x == c).map(|p| p as u32)
    }
    fn is_mark(&self, t: u32) -> bool {
        t == MARK
    }
    fn reference_stacks(&self, key: &[u32]) -> Vec<Vec<u32>> {
        vec![Vec::new(); key.len()]
    }
    fn stack_loop(&self) -> bool {
        self.stack_loop
    }
}

struct Stack {
    layers: Vec<NfaLayer>,
}

impl Stack {
    fn new(entries: &[Entry], w: Weights, mark_loop: bool) -> Self {
        Stack {
            layers: vec![lex_layer(entries, mark_loop), fold_layer(), err_layer(w)],
        }
    }

    fn run(
        &self,
        word: &[u32],
        exact: &[u32],
        opts: &SpellOptions,
        edit: impl FnOnce(&mut SpellLimits),
        confirm: Option<&mut dyn Confirmer>,
    ) -> SpellOutcome {
        let refs: Vec<&dyn Layer> = self.layers.iter().map(|l| l as &dyn Layer).collect();
        let model = Model {
            typed_exact: exact.to_vec(),
            stack_loop: false,
        };
        let mut limits = SpellLimits::for_request(opts.max_edits, 1000);
        limits.prepass = PrepassPlan {
            cuts: vec![2],
            budget: PrepassBudget::default(),
        };
        edit(&mut limits);
        let s = Speller {
            layers: &refs,
            model: &model,
            limits,
        };
        s.propose(word, opts, confirm)
    }
}

// ---------------------------------------------------------------- the oracle

#[derive(Clone, Debug, PartialEq, Eq)]
struct OVariant {
    surface: String,
    weight: u64,
    edits: u8,
    stack: u32,
    case: u32,
    roots: BTreeSet<u32>,
    rules: BTreeSet<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OGroup {
    key: Vec<u32>,
    weight: u64,
    edits: u8,
    variants: Vec<OVariant>,
    more: bool,
}

/// Least `(weight, edits, case)` of an alignment of the folded string with at most `e_max` edits.
fn align(
    s: &[u32],
    word: &[u32],
    exact: &[u32],
    w: Weights,
    e_max: u32,
) -> Option<(u64, u32, u32)> {
    let units: Vec<u32> = s.iter().copied().filter(|&t| t != MARK).collect();
    let n = units.len();
    let m = word.len();
    // dp[i][j][e] = least (weight, case)
    let mut dp = vec![vec![vec![None::<(u64, u32)>; e_max as usize + 1]; m + 1]; n + 1];
    dp[0][0][0] = Some((0, 0));
    let upd = |slot: &mut Option<(u64, u32)>, v: (u64, u32)| {
        if slot.is_none_or(|b| v < b) {
            *slot = Some(v);
        }
    };
    for i in 0..=n {
        for j in 0..=m {
            for e in 0..=e_max as usize {
                let Some((cw, cc)) = dp[i][j][e] else {
                    continue;
                };
                let upper = |t: u32| (t >= 3) as u32;
                if i < n && j < m && fold(units[i]) == Some(word[j]) {
                    let case = (units[i] != exact[j]) as u32;
                    upd(&mut dp[i + 1][j + 1][e], (cw, cc + case));
                }
                if e < e_max as usize {
                    if i < n && j < m && fold(units[i]) != Some(word[j]) {
                        upd(
                            &mut dp[i + 1][j + 1][e + 1],
                            (cw + w.sub as u64, cc + upper(units[i])),
                        );
                    }
                    if i < n {
                        upd(
                            &mut dp[i + 1][j][e + 1],
                            (cw + w.del as u64, cc + upper(units[i])),
                        );
                    }
                    if j < m {
                        upd(&mut dp[i][j + 1][e + 1], (cw + w.ins as u64, cc));
                    }
                }
            }
        }
    }
    (0..=e_max as usize)
        .filter_map(|e| dp[n][m][e].map(|(w, c)| (w, e as u32, c)))
        .min()
}

fn oracle(
    entries: &[Entry],
    word: &[u32],
    exact: &[u32],
    w: Weights,
    e_max: u32,
    theta: Option<u32>,
    caps: (usize, usize),
) -> Vec<OGroup> {
    let mut by_string: BTreeMap<Vec<u32>, OVariant> = BTreeMap::new();
    for e in entries {
        let Some((wt, ed, cs)) = align(&e.toks, word, exact, w, e_max) else {
            continue;
        };
        if theta.is_some_and(|t| wt > t as u64) {
            continue;
        }
        let v = by_string.entry(e.toks.clone()).or_insert_with(|| OVariant {
            surface: e.toks.iter().map(|&t| CHARS[t as usize]).collect(),
            weight: wt,
            edits: ed as u8,
            stack: e.toks.iter().filter(|&&t| t == MARK).count() as u32,
            case: cs,
            roots: BTreeSet::new(),
            rules: BTreeSet::new(),
        });
        for t in &e.tags {
            match t.kind().unwrap() {
                TagKind::Root(i) => {
                    v.roots.insert(i);
                }
                TagKind::Rule(i) => {
                    v.rules.insert(i);
                }
                TagKind::NonHead(_) => {}
            }
        }
    }
    let mut by_key: BTreeMap<Vec<u32>, Vec<(Vec<u32>, OVariant)>> = BTreeMap::new();
    for (toks, v) in by_string {
        let key: Vec<u32> = toks.iter().filter_map(|&t| fold(t)).collect();
        by_key.entry(key).or_default().push((toks, v));
    }
    let mut groups: Vec<OGroup> = by_key
        .into_iter()
        .map(|(key, mut vs)| {
            vs.sort_by(|a, b| {
                let k = |v: &OVariant, t: &Vec<u32>| {
                    (
                        v.weight,
                        v.stack,
                        v.case,
                        t.len(),
                        v.surface.chars().collect::<Vec<_>>(),
                    )
                };
                k(&a.1, &a.0).cmp(&k(&b.1, &b.0))
            });
            let weight = vs.iter().map(|(_, v)| (v.weight, v.edits)).min().unwrap();
            // stacks: the marks of each unit
            let stacks = |t: &[u32]| -> Vec<Vec<u32>> {
                let mut out: Vec<Vec<u32>> = Vec::new();
                for &x in t {
                    if x == MARK {
                        out.last_mut().unwrap().push(x);
                    } else {
                        out.push(Vec::new());
                    }
                }
                out
            };
            let mut seen: Vec<(Vec<Vec<u32>>, usize)> = Vec::new();
            let mut listed = Vec::new();
            let mut more = false;
            for (t, v) in &vs {
                let st = stacks(t);
                match seen.iter().position(|(a, _)| *a == st) {
                    Some(i) if seen[i].1 < caps.1 => {
                        seen[i].1 += 1;
                        listed.push(v.clone());
                    }
                    None if seen.len() < caps.0 => {
                        seen.push((st, 1));
                        listed.push(v.clone());
                    }
                    _ => more = true,
                }
            }
            OGroup {
                key,
                weight: weight.0,
                edits: weight.1,
                variants: listed,
                more,
            }
        })
        .collect();
    groups.sort_by(|a, b| (a.weight, &a.key).cmp(&(b.weight, &b.key)));
    groups
}

fn shape(groups: &[Group]) -> Vec<OGroup> {
    groups
        .iter()
        .map(|g| OGroup {
            key: g.key.clone(),
            weight: g.weight,
            edits: g.edits,
            more: g.more_variants,
            variants: g
                .variants
                .iter()
                .map(|v| OVariant {
                    surface: v.surface.clone(),
                    weight: v.weight,
                    edits: v.edits,
                    stack: v.stack_distance,
                    case: v.case_distance,
                    roots: v.features.roots.iter().map(|r| r.0).collect(),
                    rules: v.features.rules.iter().map(|r| r.0).collect(),
                })
                .collect(),
        })
        .collect()
}

fn complete(o: SpellOutcome) -> (Vec<Group>, Vec<(String, Membership)>, bool, Counters) {
    match o {
        SpellOutcome::Complete {
            groups,
            probes,
            confirm_complete,
            counters,
            ..
        } => (groups, probes, confirm_complete, counters),
        other => panic!("not complete: {other:?}"),
    }
}

// ---------------------------------------------------------------- random source

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn gen_entries(rng: &mut Rng) -> Vec<Entry> {
    let n = 1 + rng.below(6) as usize;
    (0..n)
        .map(|i| {
            let len = 1 + rng.below(4) as usize;
            let mut toks: Vec<u32> = Vec::new();
            for _ in 0..len {
                toks.push(rng.below(5) as u32);
                if rng.below(6) == 0 {
                    toks.push(MARK);
                }
            }
            let mut tags = vec![Tag::root(i as u32)];
            if rng.below(3) == 0 {
                tags.push(Tag::rule(rng.below(3) as u32));
            }
            Entry { toks, tags }
        })
        .collect()
}

fn gen_word(rng: &mut Rng) -> (Vec<u32>, Vec<u32>) {
    let len = rng.below(5) as usize;
    let word: Vec<u32> = (0..len).map(|_| rng.below(3) as u32).collect();
    let exact = word
        .iter()
        .map(|&t| if t < 2 && rng.below(3) == 0 { t + 3 } else { t })
        .collect();
    (word, exact)
}

fn gen_weights(rng: &mut Rng) -> Weights {
    let pick = |r: &mut Rng| 400 + 100 * r.below(9) as u32;
    Weights {
        sub: pick(rng),
        del: pick(rng),
        ins: pick(rng),
    }
}

fn opts(e: u8, mode: Mode, theta: Option<u32>) -> SpellOptions {
    let mut o = SpellOptions::new(e, mode);
    o.weight_ceiling = theta;
    o
}

// ---------------------------------------------------------------- tests

#[derive(Default)]
struct Seen {
    cases: u32,
    nonempty: u32,
    best_stopped_early: u32,
    best_cut_list: u32,
    probes_yes: u32,
    probes_no: u32,
    probes_unknown: u32,
    theta_cut: u32,
    more_variants: u32,
}

/// All three modes against the exhaustive oracle on small random cascades.
#[test]
fn random_cascades_match_the_exhaustive_oracle() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut seen = Seen::default();
    for _ in 0..600 {
        let entries = gen_entries(&mut rng);
        let w = gen_weights(&mut rng);
        let stack = Stack::new(&entries, w, false);
        let (word, exact) = gen_word(&mut rng);
        let e = rng.below(3) as u8;
        let theta = match rng.below(3) {
            0 => Some(500 + rng.below(2500) as u32),
            _ => None,
        };
        let caps = (1 + rng.below(3) as usize, 1 + rng.below(3) as usize);
        let want = oracle(&entries, &word, &exact, w, e as u32, theta, caps);
        let mut probes: Vec<String> = entries
            .iter()
            .map(|x| x.toks.iter().map(|&t| CHARS[t as usize]).collect())
            .collect();
        probes.push("ab".into());
        probes.push("cc".into());
        probes.push("z".into());
        let mut all = opts(e, Mode::All, theta);
        all.stack_cap = caps.0;
        all.casing_cap = caps.1;
        all.probe = probes.clone();
        let (groups, pr, _, _) = complete(stack.run(&word, &exact, &all, |_| {}, None));
        assert_eq!(
            shape(&groups),
            want,
            "All, entries {entries:?} word {word:?} E {e} theta {theta:?}"
        );
        seen.cases += 1;
        seen.nonempty += !want.is_empty() as u32;
        seen.more_variants += want.iter().any(|g| g.more) as u32;
        // probes against the oracle's unlimited variant sets
        let full = oracle(
            &entries,
            &word,
            &exact,
            w,
            e as u32,
            theta,
            (usize::MAX, usize::MAX),
        );
        for (p, m) in &pr {
            let ov = full
                .iter()
                .flat_map(|g| &g.variants)
                .find(|v| v.surface == *p);
            match (ov, m) {
                (Some(v), Membership::Yes(f)) => {
                    let r: BTreeSet<u32> = f.roots.iter().map(|x| x.0).collect();
                    let u: BTreeSet<u32> = f.rules.iter().map(|x| x.0).collect();
                    assert_eq!((&v.roots, &v.rules), (&r, &u), "probe {p}");
                    seen.probes_yes += 1;
                }
                (None, Membership::No) => seen.probes_no += 1,
                (o, m) => panic!("probe {p:?}: oracle {o:?}, answer {m:?}"),
            }
        }
        if theta.is_some() {
            let uncut = oracle(&entries, &word, &exact, w, e as u32, None, caps);
            seen.theta_cut += (uncut.len() != want.len()) as u32;
        }
        for n in [1usize, 2, 3, 5] {
            let mut bo = opts(e, Mode::Best(n), theta);
            bo.stack_cap = caps.0;
            bo.casing_cap = caps.1;
            bo.probe = probes.clone();
            let (bg, bp, _, counters) = complete(stack.run(&word, &exact, &bo, |_| {}, None));
            let take = want.len().min(n);
            let got = shape(&bg);
            assert_eq!(
                got.len(),
                take,
                "Best({n}) group count, entries {entries:?} word {word:?} E {e}"
            );
            for (g, o) in got.iter().zip(&want) {
                assert_eq!(
                    (&g.key, g.weight, g.edits),
                    (&o.key, o.weight, o.edits),
                    "Best({n}) group"
                );
                assert!(
                    g.variants.len() <= o.variants.len()
                        && g.variants[..] == o.variants[..g.variants.len()],
                    "Best({n}) variants are a prefix of All's: {g:?} against {o:?}"
                );
            }
            seen.best_stopped_early += (counters.checkpoints > 0 && want.len() > n) as u32;
            seen.best_cut_list +=
                got.iter()
                    .zip(&want)
                    .any(|(g, o)| g.variants.len() < o.variants.len()) as u32;
            for (p, m) in &bp {
                let ov = full
                    .iter()
                    .flat_map(|g| &g.variants)
                    .find(|v| v.surface == *p);
                match (ov, m) {
                    (Some(v), Membership::Yes(f)) => {
                        let r: BTreeSet<u32> = f.roots.iter().map(|x| x.0).collect();
                        assert_eq!(v.roots, r, "Best probe {p}");
                    }
                    (None, Membership::Yes(_)) => {
                        panic!("Best probe {p:?} is Yes but the oracle has no such string")
                    }
                    (Some(_), Membership::No) => {
                        panic!("Best probe {p:?} is No but the oracle has it")
                    }
                    (_, Membership::Unknown { .. }) => seen.probes_unknown += 1,
                    (None, Membership::No) => {}
                }
            }
        }
    }
    eprintln!(
        "cases {} nonempty {} theta-cut {} more-variants {} probes yes/no {}/{} best early {} best cut variants {} best unknown {}",
        seen.cases, seen.nonempty, seen.theta_cut, seen.more_variants, seen.probes_yes, seen.probes_no,
        seen.best_stopped_early, seen.best_cut_list, seen.probes_unknown
    );
    assert!(
        seen.nonempty > 200,
        "the generator must find corrections: {}",
        seen.nonempty
    );
    assert!(
        seen.theta_cut > 20,
        "theta must cut something: {}",
        seen.theta_cut
    );
    assert!(
        seen.more_variants > 5,
        "the caps must cut something: {}",
        seen.more_variants
    );
    assert!(
        seen.best_stopped_early > 100,
        "Best(n) must stop the search early: {}",
        seen.best_stopped_early
    );
}

fn e(toks: &[u32], tags: &[Tag]) -> Entry {
    Entry {
        toks: toks.to_vec(),
        tags: tags.to_vec(),
    }
}

fn plain(stack: &Stack, word: &[u32], o: &SpellOptions) -> SpellOutcome {
    stack.run(word, word, o, |_| {}, None)
}

/// A scripted confirmer: the answer per exact surface, and a work cost per parse.
struct Fake {
    answers: BTreeMap<String, ParseResult>,
    cost: u64,
    in_group: u64,
    groups_begun: u32,
    asked: Vec<String>,
}

impl Fake {
    fn new(cost: u64, answers: &[(&str, ParseResult)]) -> Self {
        Fake {
            answers: answers
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            cost,
            in_group: 0,
            groups_begun: 0,
            asked: Vec::new(),
        }
    }
}

impl Confirmer for Fake {
    fn begin_group(&mut self) {
        self.in_group = 0;
        self.groups_begun += 1;
    }
    fn parse(&mut self, surface: &str, _: &Features) -> ParseResult {
        self.in_group += self.cost;
        self.asked.push(surface.to_string());
        self.answers
            .get(surface)
            .cloned()
            .unwrap_or(ParseResult::Rejected)
    }
    fn group_used(&self) -> u64 {
        self.in_group
    }
}

fn conf() -> ParseResult {
    ParseResult::Confirmed(vec![("x".into(), "y".into())])
}

/// Four casings of `ab` in one group, then a second group `ac` one substitution away.
fn casing_fixture() -> Stack {
    Stack::new(
        &[
            e(&[0, 1], &[Tag::root(0)]),
            e(&[3, 1], &[Tag::root(1)]),
            e(&[0, 4], &[Tag::root(2)]),
            e(&[3, 4], &[Tag::root(3)]),
            e(&[0, 2], &[Tag::root(4)]),
        ],
        Weights::UNIT,
        false,
    )
}

#[test]
fn one_stopped_parse_ends_its_group_and_the_loop_goes_on_c159() {
    let st = casing_fixture();
    let mut o = opts(1, Mode::All, None);
    o.confirm_top = 1;
    // every variant of the first group would stop; the second group's variant is real
    let mut f = Fake::new(
        10,
        &[
            ("ab", ParseResult::Stopped),
            ("Ab", ParseResult::Stopped),
            ("aB", ParseResult::Stopped),
            ("AB", ParseResult::Stopped),
            ("ac", conf()),
        ],
    );
    let out = st.run(&[0, 1], &[0, 1], &o, |_| {}, Some(&mut f));
    let (groups, _, cc, counters) = complete(out);
    assert_eq!(groups.len(), 2);
    assert_eq!(
        f.asked,
        vec!["ab", "ac"],
        "the stopped group's other variants were not parsed"
    );
    assert_eq!(groups[0].confirm, GroupConfirm::Open);
    assert_eq!(groups[0].variants[0].hc, HcLabel::Stopped);
    assert!(groups[0].variants[1..]
        .iter()
        .all(|v| v.hc == HcLabel::Unchecked));
    assert_eq!(groups[1].confirm, GroupConfirm::Confirmed);
    assert_eq!(f.groups_begun, 2, "one fresh budget per group");
    assert!(
        !cc,
        "an Open group and a Stopped variant: Complete does not mean confirmed"
    );
    assert_eq!((counters.parses, counters.parses_stopped), (2, 1));
}

#[test]
fn a_group_is_all_rejected_only_when_nothing_is_unlisted_or_unchecked_c142() {
    let st = casing_fixture();
    let mut o = opts(1, Mode::All, None);
    o.confirm_top = 2;
    let all_rejected =
        |f: &mut Fake, o: &SpellOptions| complete(st.run(&[0, 1], &[0, 1], o, |_| {}, Some(f)));
    let mut f = Fake::new(1, &[("ac", conf())]);
    let (g, _, cc, _) = all_rejected(&mut f, &o);
    assert_eq!(
        g[0].confirm,
        GroupConfirm::AllRejected,
        "four listed variants, all rejected"
    );
    assert!(!g[0].more_variants);
    assert_eq!(g[1].confirm, GroupConfirm::Confirmed);
    assert!(cc, "every tried group is Confirmed or AllRejected");
    // a casing cap of 2 leaves two variants unlisted: the group cannot be called rejected
    o.casing_cap = 2;
    let mut f = Fake::new(1, &[("ac", conf())]);
    let (g, _, cc, _) = all_rejected(&mut f, &o);
    assert!(g[0].more_variants);
    assert_eq!(g[0].variants.len(), 2);
    assert_eq!(g[0].confirm, GroupConfirm::Open);
    assert!(!cc);
}

#[test]
fn word_work_and_groups_tried_end_the_loop_with_the_list_so_far() {
    let st = casing_fixture();
    let mut o = opts(1, Mode::All, None);
    o.confirm_top = 2;
    let mut f = Fake::new(100, &[]);
    let out = st.run(
        &[0, 1],
        &[0, 1],
        &o,
        |l| l.confirm_word_work = Some(300),
        Some(&mut f),
    );
    match out {
        SpellOutcome::Incomplete {
            dimension: Dimension::Confirmation(ConfirmLimit::WordWork),
            value,
            limit,
            partial,
            graph_complete,
            ..
        } => {
            assert_eq!(
                (value, limit),
                (400, 300),
                "four parses at 100 passed the word total"
            );
            assert!(graph_complete);
            assert_eq!(partial[0].confirm, GroupConfirm::AllRejected);
            assert_eq!(partial[1].confirm, GroupConfirm::NotTried);
        }
        other => panic!("{other:?}"),
    }
    let mut f = Fake::new(1, &[]);
    let out = st.run(
        &[0, 1],
        &[0, 1],
        &o,
        |l| l.groups_tried = Some(1),
        Some(&mut f),
    );
    assert!(matches!(
        out,
        SpellOutcome::Incomplete {
            dimension: Dimension::Confirmation(ConfirmLimit::GroupsTried),
            value: 1,
            limit: 1,
            ..
        }
    ));
    assert_eq!(f.groups_begun, 1);
}

#[test]
fn default_confirm_allowances_follow_s8() {
    let l = SpellLimits::for_request(2, 16_000);
    assert_eq!(l.f_c, 4);
    let o = |top| {
        let mut x = l.clone();
        x.confirm_word_work = None;
        (x.groups_tried, x.confirm_word_work, top)
    };
    let _ = o(5);
    // 4 * 5 groups, each allowed 17/16 of 16,000
    let st = casing_fixture();
    let mut opt = opts(1, Mode::All, None);
    opt.confirm_top = 5;
    let mut f = Fake::new(17_001, &[]);
    let out = st.run(
        &[0, 1],
        &[0, 1],
        &opt,
        |l| l.confirm_group_work = 16_000,
        Some(&mut f),
    );
    // the first group spends 4 x 17,001 = 68,004 of a 4*5*17/16*16,000 = 340,000 total: no stop
    complete(out);
}

#[test]
fn limits_name_their_dimension_and_the_list_is_never_a_short_complete() {
    let st = casing_fixture();
    let o = opts(1, Mode::All, None);
    let full = shape(&complete(plain(&st, &[0, 1], &o)).0);
    for (name, edit) in [
        (
            "states",
            (|l: &mut SpellLimits| l.word.states = 3) as fn(&mut SpellLimits),
        ),
        ("arcs", |l| l.word.arcs = 3),
        ("steps", |l| l.word.steps = 5),
        ("bytes", |l| l.word.bytes = 200),
    ] {
        match st.run(&[0, 1], &[0, 1], &o, edit, None) {
            SpellOutcome::Incomplete {
                dimension,
                graph_complete,
                ..
            } => {
                let want = match name {
                    "states" => Dimension::ProductStates,
                    "arcs" => Dimension::ProductArcs,
                    "steps" => Dimension::ProductSteps,
                    _ => Dimension::ProductBytes,
                };
                assert_eq!(dimension, want, "{name}");
                assert!(!graph_complete);
            }
            other => panic!("{name}: {other:?}"),
        }
    }
    // Strings: groups plus listed variants over the cap
    match st.run(&[0, 1], &[0, 1], &o, |l| l.strings = 3, None) {
        SpellOutcome::Incomplete {
            dimension: Dimension::Strings,
            graph_complete: true,
            partial,
            ..
        } => assert!(!partial.is_empty()),
        other => panic!("{other:?}"),
    }
    assert_eq!(full.len(), 2);
}

#[test]
fn every_limit_is_incomplete_or_the_whole_answer() {
    let mut rng = Rng(0xABCD_1234_5678_9F01);
    let mut incomplete = 0;
    let mut whole = 0;
    for _ in 0..300 {
        let entries = gen_entries(&mut rng);
        let w = gen_weights(&mut rng);
        let st = Stack::new(&entries, w, false);
        let (word, exact) = gen_word(&mut rng);
        let o = opts(1 + rng.below(2) as u8, rng.below(2).then_all(), None);
        let full = shape(&complete(st.run(&word, &exact, &o, |_| {}, None)).0);
        let limit = 1 + rng.below(400);
        let which = rng.below(5);
        let out = st.run(
            &word,
            &exact,
            &o,
            |l| match which {
                0 => l.word.states = limit,
                1 => l.word.arcs = limit,
                2 => l.word.steps = limit * 2,
                3 => l.word.bytes = limit * 20,
                _ => l.strings = limit / 10,
            },
            None,
        );
        match out {
            SpellOutcome::Incomplete { .. } => incomplete += 1,
            SpellOutcome::Complete { groups, .. } => {
                whole += 1;
                assert_eq!(
                    shape(&groups),
                    full,
                    "a Complete under a limit is the whole answer"
                );
            }
            SpellOutcome::Refused { .. } => panic!("refused"),
        }
    }
    assert!(incomplete > 30 && whole > 30, "{incomplete} / {whole}");
}

trait ThenAll {
    fn then_all(self) -> Mode;
}
impl ThenAll for u64 {
    fn then_all(self) -> Mode {
        if self == 0 {
            Mode::All
        } else {
            Mode::Best(2)
        }
    }
}

#[test]
fn best_n_takes_every_tied_path_before_it_stops_c144() {
    // two roots spell `ab` at weight 0: the features must include both
    let st = Stack::new(
        &[
            e(&[0, 1], &[Tag::root(0)]),
            e(&[0, 1], &[Tag::root(1)]),
            e(&[0, 2], &[Tag::root(2)]),
        ],
        Weights::UNIT,
        false,
    );
    let (g, _, _, _) = complete(plain(&st, &[0, 1], &opts(1, Mode::Best(1), None)));
    assert_eq!(g.len(), 1);
    let f = &g[0].variants[0].features;
    assert_eq!(
        f.roots.iter().map(|r| r.0).collect::<BTreeSet<_>>(),
        BTreeSet::from([0, 1])
    );
}

#[test]
fn best_n_never_skips_a_better_group_than_one_it_returns() {
    // a heavier entry comes first; the cheaper group must still come first
    let st = Stack::new(
        &[
            e(&[2, 2, 2], &[Tag::root(0)]),
            e(&[0, 1], &[Tag::root(1)]),
            e(&[0], &[Tag::root(2)]),
        ],
        Weights {
            sub: 900,
            del: 700,
            ins: 800,
        },
        false,
    );
    let all = complete(plain(&st, &[0, 1], &opts(2, Mode::All, None))).0;
    for n in 1..=3 {
        let best = complete(plain(&st, &[0, 1], &opts(2, Mode::Best(n), None))).0;
        assert_eq!(shape(&best)[..], shape(&all)[..n.min(all.len())]);
    }
}

#[test]
fn membership_is_three_valued() {
    let st = Stack::new(
        &[
            e(&[0, 1], &[Tag::root(0)]),
            e(&[1, 1, 1], &[Tag::root(1)]),
            e(&[2, 2], &[Tag::root(2)]),
        ],
        Weights::UNIT,
        false,
    );
    let mut o = opts(2, Mode::All, None);
    o.probe = vec![
        "ab".into(),
        "bbb".into(),
        "cb".into(),
        "zz".into(),
        "".into(),
    ];
    let (_, p, _, _) = complete(plain(&st, &[0, 1], &o));
    assert!(matches!(&p[0].1, Membership::Yes(f) if f.roots.len() == 1));
    assert!(
        matches!(&p[1].1, Membership::Yes(_)),
        "bbb is within two edits of ab"
    );
    assert_eq!(p[2].1, Membership::No, "cb is not in the lexicon");
    assert_eq!(
        p[3].1,
        Membership::No,
        "a character the cascade cannot write, on a complete graph"
    );
    assert_eq!(p[4].1, Membership::No);
    // a Best(1) graph that stopped early: a heavier string is Unknown, never No
    let mut b = opts(2, Mode::Best(1), None);
    b.probe = vec!["ab".into(), "bbb".into(), "cc".into(), "zz".into()];
    let (g, p, _, c) = complete(plain(&st, &[0, 1], &b));
    assert_eq!(g.len(), 1);
    assert!(c.checkpoints > 0);
    assert!(matches!(&p[0].1, Membership::Yes(_)));
    for (s, m) in &p[1..] {
        assert!(matches!(m, Membership::Unknown { .. }), "{s}: {m:?}");
    }
    // a graph that is not complete: always Unknown
    let mut o = opts(2, Mode::All, None);
    o.probe = vec!["ab".into()];
    match st.run(&[0, 1], &[0, 1], &o, |l| l.word.states = 2, None) {
        SpellOutcome::Incomplete { probes, .. } => {
            assert!(matches!(probes[0].1, Membership::Unknown { .. }))
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_mark_loop_gives_endless_variants_but_finite_groups_and_best_n_ends() {
    let st = Stack::new(
        &[
            e(&[0], &[Tag::root(0)]),
            e(&[1], &[Tag::root(1)]),
            e(&[2], &[Tag::root(2)]),
        ],
        Weights::UNIT,
        true,
    );
    for mode in [Mode::All, Mode::Best(3)] {
        let (g, _, _, _) = complete(plain(&st, &[0], &opts(1, mode, None)));
        assert_eq!(g.len(), 3, "{mode:?}");
        for grp in &g {
            assert_eq!(grp.variants.len(), 4, "stack_cap 4 assignments");
            assert!(grp.more_variants);
            let st: Vec<u32> = grp.variants.iter().map(|v| v.stack_distance).collect();
            assert_eq!(st, vec![0, 1, 2, 3], "nearest stack first");
        }
    }
}

#[test]
fn variants_come_nearest_first_with_the_typed_case_before_other_casings() {
    let st = Stack::new(
        &[
            e(&[3, 1], &[Tag::root(0)]),
            e(&[0, 1], &[Tag::root(1)]),
            e(&[0, MARK, 1], &[Tag::root(2)]),
        ],
        Weights::UNIT,
        false,
    );
    let (g, _, _, _) = complete(st.run(&[0, 1], &[0, 1], &opts(0, Mode::All, None), |_| {}, None));
    let order: Vec<(&str, u32, u32)> = g[0]
        .variants
        .iter()
        .map(|v| (v.surface.as_str(), v.stack_distance, v.case_distance))
        .collect();
    assert_eq!(order, vec![("ab", 0, 0), ("Ab", 0, 1), ("a\u{301}b", 1, 0)]);
    // typed with a capital: the capital casing is nearest
    let (g, _, _, _) = complete(st.run(&[0, 1], &[3, 1], &opts(0, Mode::All, None), |_| {}, None));
    assert_eq!(g[0].variants[0].surface, "Ab");
}

#[test]
fn the_ceiling_cuts_the_search_not_only_the_list() {
    let st = casing_fixture();
    let open = complete(plain(&st, &[0, 1], &opts(1, Mode::All, None)));
    let cut = complete(plain(&st, &[0, 1], &opts(1, Mode::All, Some(500))));
    assert_eq!(open.0.len(), 2);
    assert_eq!(cut.0.len(), 1, "the one-substitution group is above 500");
    assert!(
        cut.3.states < open.3.states,
        "{} < {}",
        cut.3.states,
        open.3.states
    );
    let none = complete(plain(&st, &[0, 1], &opts(0, Mode::All, None)));
    assert!(
        none.3.states < open.3.states,
        "no edit is made past the limit"
    );
}

#[test]
fn an_edit_limit_over_two_is_refused_by_name() {
    let st = casing_fixture();
    match plain(&st, &[0, 1], &opts(3, Mode::All, None)) {
        SpellOutcome::Refused { construct, .. } => assert!(construct.contains("max_edits")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn best_n_keeps_both_roots_of_a_group_tied_with_the_stop_weight_c144() {
    let st = Stack::new(
        &[
            e(&[0, 1], &[Tag::root(0)]),
            e(&[0, 2], &[Tag::root(1)]),
            e(&[0, 2], &[Tag::root(2)]),
            e(&[1, 1], &[Tag::root(3)]),
        ],
        Weights::UNIT,
        false,
    );
    for n in [1usize, 2] {
        let (g, _, _, _) = complete(plain(&st, &[0, 1], &opts(1, Mode::Best(n), None)));
        assert_eq!(g.len(), n);
    }
    let (g, _, _, _) = complete(plain(&st, &[0, 1], &opts(1, Mode::Best(2), None)));
    let f = &g[1].variants[0].features;
    assert_eq!(
        f.roots.iter().map(|r| r.0).collect::<BTreeSet<_>>(),
        BTreeSet::from([1, 2]),
        "a group whose weight equals the stop weight is complete with all its roots"
    );
}
