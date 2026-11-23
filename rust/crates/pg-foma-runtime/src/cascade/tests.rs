//! Random oracle comparison and named cases for the product (mutations C20-C77).

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use super::*;

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
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

// ---------------------------------------------------------------- raw layers

#[derive(Clone, Debug)]
enum RIn {
    Eps,
    Tok(u32),
    Class(Vec<u32>),
}
#[derive(Clone, Debug)]
enum ROut {
    Eps,
    Tok(u32),
    Same,
    /// image[t] for members t (indexed by token over the universe)
    Image(Vec<u32>),
}
#[derive(Clone, Debug)]
struct RArc {
    from: u32,
    input: RIn,
    output: ROut,
    to: u32,
    tags: Vec<u32>,
    w: u32,
    e: u8,
}
#[derive(Clone, Debug)]
struct RLayer {
    n: u32,
    finals: Vec<bool>,
    arcs: Vec<RArc>,
}

fn mask(tags: &[u32]) -> u32 {
    tags.iter().fold(0, |m, t| m | 1 << t)
}

impl RLayer {
    fn build(&self, universe: u32) -> NfaLayer {
        let mut meter = BuildMeter::default_limits();
        let mut b = NfaBuilder::new("test", universe, &mut meter);
        for s in 0..self.n {
            b.add_state(self.finals[s as usize]).unwrap();
        }
        for a in &self.arcs {
            let (input, output) = match (&a.input, &a.output) {
                (RIn::Eps, o) => (Input::Eps, Self::out(o, &mut b)),
                (RIn::Tok(t), o) => (Input::Tok(*t), Self::out(o, &mut b)),
                (RIn::Class(m), o) => {
                    let set = b.add_set(TokenSet::from_tokens(universe, m)).unwrap();
                    (Input::Class(set), Self::out(o, &mut b))
                }
            };
            let tags: Vec<Tag> = a.tags.iter().map(|&t| Tag(t)).collect();
            b.add_arc(a.from, input, output, a.to, &tags, a.w, a.e)
                .unwrap();
        }
        b.finish()
    }

    fn out(o: &ROut, b: &mut NfaBuilder) -> Output {
        match o {
            ROut::Eps => Output::Eps,
            ROut::Tok(t) => Output::Tok(*t),
            ROut::Same => Output::Same,
            ROut::Image(img) => Output::Image(b.add_image(img.clone()).unwrap()),
        }
    }

    /// Per-token relation arcs: (from, in, out, to, tagmask, w, e).
    fn expand(&self) -> Vec<Rel> {
        let mut v = Vec::new();
        for a in &self.arcs {
            let ins: Vec<Option<u32>> = match &a.input {
                RIn::Eps => vec![None],
                RIn::Tok(t) => vec![Some(*t)],
                RIn::Class(m) => m.iter().map(|&t| Some(t)).collect(),
            };
            for i in ins {
                let out = match (&a.output, i) {
                    (ROut::Eps, _) => None,
                    (ROut::Tok(t), _) => Some(*t),
                    (ROut::Same, Some(t)) => Some(t),
                    (ROut::Image(img), Some(t)) => Some(img[t as usize]),
                    _ => unreachable!(),
                };
                v.push(Rel {
                    from: a.from,
                    i,
                    o: out,
                    to: a.to,
                    mask: mask(&a.tags),
                    w: a.w,
                    e: a.e,
                });
            }
        }
        v
    }
}

#[derive(Clone, Copy, Debug)]
struct Rel {
    from: u32,
    i: Option<u32>,
    o: Option<u32>,
    to: u32,
    mask: u32,
    w: u32,
    e: u8,
}

// ---------------------------------------------------------------- oracle

struct Machine {
    dims: Vec<u32>,
    n: u32,
    fin: Vec<bool>,
    arcs: Vec<Rel>,
}

fn oracle_machine(layers: &[RLayer]) -> Machine {
    let top = &layers[0];
    let mut m = Machine {
        dims: vec![top.n],
        n: top.n,
        fin: top.finals.clone(),
        arcs: top.expand(),
    };
    for l in &layers[1..] {
        let b = l.expand();
        let nb = l.n;
        let mut by_a: Vec<Vec<Rel>> = vec![Vec::new(); m.n as usize];
        for a in &m.arcs {
            by_a[a.from as usize].push(*a);
        }
        let mut by_b: Vec<Vec<Rel>> = vec![Vec::new(); nb as usize];
        for y in &b {
            by_b[y.from as usize].push(*y);
        }
        let mut arcs = Vec::new();
        for a in 0..m.n {
            for bs in 0..nb {
                let from = a * nb + bs;
                for x in &by_a[a as usize] {
                    match x.o {
                        None => arcs.push(Rel {
                            from,
                            i: None,
                            o: None,
                            to: x.to * nb + bs,
                            ..*x
                        }),
                        Some(t) => {
                            for y in &by_b[bs as usize] {
                                if y.i == Some(t) {
                                    arcs.push(Rel {
                                        from,
                                        i: None,
                                        o: y.o,
                                        to: x.to * nb + y.to,
                                        mask: x.mask | y.mask,
                                        w: x.w + y.w,
                                        e: x.e + y.e,
                                    });
                                }
                            }
                        }
                    }
                }
                for y in &by_b[bs as usize] {
                    if y.i.is_none() {
                        arcs.push(Rel {
                            from,
                            i: None,
                            o: y.o,
                            to: a * nb + y.to,
                            mask: y.mask,
                            w: y.w,
                            e: y.e,
                        });
                    }
                }
            }
        }
        let mut fin = vec![false; (m.n * nb) as usize];
        for a in 0..m.n {
            for bs in 0..nb {
                fin[(a * nb + bs) as usize] = m.fin[a as usize] && l.finals[bs as usize];
            }
        }
        m.dims.push(nb);
        m.n *= nb;
        m.fin = fin;
        m.arcs = arcs;
    }
    m
}

fn decode(dims: &[u32], mut id: u32) -> Vec<u32> {
    let mut v = vec![0; dims.len()];
    for k in (0..dims.len()).rev() {
        v[k] = id % dims[k];
        id /= dims[k];
    }
    v
}

type Key = Vec<u32>;
/// (src key, dst key, weight, edits) -> tag mask
type ArcMap = BTreeMap<(Key, Key, u32, u8), u32>;

struct OracleGraph {
    states: BTreeSet<Key>,
    arcs: ArcMap,
    /// least weight to accept from each state (trimmed states only)
    best_weight: Option<u64>,
    through: BTreeMap<(Key, Key, u32, u8), u64>,
}

fn oracle_graph(m: &Machine, word: &[u32], e_max: u32) -> OracleGraph {
    let mut by_from: Vec<Vec<Rel>> = vec![Vec::new(); m.n as usize];
    for a in &m.arcs {
        by_from[a.from as usize].push(*a);
    }
    let key = |id: u32, i: usize, u: u32| -> Key {
        let mut k = vec![i as u32, u];
        k.extend(decode(&m.dims, id));
        k
    };
    let mut seen: BTreeSet<(u32, usize, u32)> = BTreeSet::new();
    let mut q = VecDeque::new();
    seen.insert((0, 0, 0));
    q.push_back((0u32, 0usize, 0u32));
    let mut arcs: ArcMap = BTreeMap::new();
    while let Some((id, i, u)) = q.pop_front() {
        for a in &by_from[id as usize] {
            let u2 = u + a.e as u32;
            if u2 > e_max {
                continue;
            }
            let i2 = match a.o {
                None => i,
                Some(t) => {
                    if i < word.len() && word[i] == t {
                        i + 1
                    } else {
                        continue;
                    }
                }
            };
            *arcs
                .entry((key(id, i, u), key(a.to, i2, u2), a.w, a.e))
                .or_insert(0) |= a.mask;
            if seen.insert((a.to, i2, u2)) {
                q.push_back((a.to, i2, u2));
            }
        }
    }
    // co-reachability (fixpoint, deliberately naive)
    let acc: BTreeSet<Key> = seen
        .iter()
        .filter(|(id, i, _)| *i == word.len() && m.fin[*id as usize])
        .map(|&(id, i, u)| key(id, i, u))
        .collect();
    let mut co = acc.clone();
    loop {
        let before = co.len();
        for (s, d, _, _) in arcs.keys() {
            if co.contains(d) {
                co.insert(s.clone());
            }
        }
        if co.len() == before {
            break;
        }
    }
    let start = key(0, 0, 0);
    let mut arcs_t = ArcMap::new();
    let mut states = BTreeSet::new();
    if co.contains(&start) {
        states.insert(start.clone());
        for (k, v) in &arcs {
            if co.contains(&k.0) && co.contains(&k.1) {
                arcs_t.insert(k.clone(), *v);
                states.insert(k.0.clone());
                states.insert(k.1.clone());
            }
        }
    }
    // weights: least from start, least to accept (Bellman-Ford; small graphs)
    let inf = u64::MAX / 4;
    let mut from_start: BTreeMap<Key, u64> = states.iter().map(|s| (s.clone(), inf)).collect();
    let mut to_acc: BTreeMap<Key, u64> = states
        .iter()
        .map(|s| (s.clone(), if acc.contains(s) { 0 } else { inf }))
        .collect();
    if let Some(v) = from_start.get_mut(&start) {
        *v = 0;
    }
    for _ in 0..states.len() + 1 {
        for (s, d, w, _) in arcs_t.keys() {
            let fs = from_start[s] + *w as u64;
            if fs < from_start[d] {
                from_start.insert(d.clone(), fs);
            }
            let ta = to_acc[d] + *w as u64;
            if ta < to_acc[s] {
                to_acc.insert(s.clone(), ta);
            }
        }
    }
    let best_weight = acc
        .iter()
        .filter(|a| states.contains(*a))
        .map(|a| from_start[a])
        .min();
    let mut through = BTreeMap::new();
    for k in arcs_t.keys() {
        through.insert(k.clone(), from_start[&k.0] + k.2 as u64 + to_acc[&k.1]);
    }
    OracleGraph {
        states,
        arcs: arcs_t,
        best_weight,
        through,
    }
}

fn product_arcs(g: &TrimmedGraph) -> (BTreeSet<Key>, ArcMap) {
    let mut states = BTreeSet::new();
    for s in 0..g.state_count() as u32 {
        states.insert(g.key(s).to_vec());
    }
    let mut arcs = ArcMap::new();
    for a in &g.arcs {
        let m = g.arc_tags(a).iter().fold(0, |m, t| m | 1 << t.0);
        let k = (
            g.key(a.src).to_vec(),
            g.key(a.dst).to_vec(),
            a.weight,
            a.edits,
        );
        assert!(arcs.insert(k, m).is_none(), "duplicate product arc key");
    }
    (states, arcs)
}

// ---------------------------------------------------------------- generator

fn gen_layer(rng: &mut Rng, commit: bool, uni: u32) -> RLayer {
    let n = 1 + rng.below(3) as u32;
    let finals: Vec<bool> = (0..n).map(|_| rng.chance(60)).collect();
    let mut arcs = Vec::new();
    let count = 2 + rng.below(6);
    for _ in 0..count {
        let mut from = rng.below(n as u64) as u32;
        if commit && n > 1 && from == 0 {
            from = 1;
        }
        let to = rng.below(n as u64) as u32;
        let tags = if rng.chance(50) {
            vec![rng.below(4) as u32]
        } else {
            vec![]
        };
        let (w, e) = match rng.below(8) {
            0 => (1000, 0),
            1 => (500, 1),
            2 => (0, 1),
            3 => (700, 2),
            _ => (0, 0),
        };
        let kind = rng.below(8);
        let tok = |r: &mut Rng| r.below(uni as u64) as u32;
        let class = |r: &mut Rng| -> Vec<u32> {
            let mut m: Vec<u32> = (0..uni).filter(|_| r.chance(60)).collect();
            if m.is_empty() {
                m.push(r.below(uni as u64) as u32);
            }
            m
        };
        let (input, output) = match kind {
            0 => (RIn::Eps, ROut::Eps),
            1 => (RIn::Eps, ROut::Tok(tok(rng))),
            2 => (RIn::Tok(tok(rng)), ROut::Same),
            3 => (RIn::Tok(tok(rng)), ROut::Tok(tok(rng))),
            4 => (RIn::Tok(tok(rng)), ROut::Eps),
            5 => (RIn::Class(class(rng)), ROut::Same),
            6 => {
                let img: Vec<u32> = (0..uni).map(|_| tok(rng)).collect();
                (RIn::Class(class(rng)), ROut::Image(img))
            }
            _ => (
                RIn::Class(class(rng)),
                if rng.chance(50) {
                    ROut::Eps
                } else {
                    ROut::Tok(tok(rng))
                },
            ),
        };
        arcs.push(RArc {
            from,
            input,
            output,
            to,
            tags,
            w,
            e,
        });
    }
    if commit && n > 1 {
        let tags = vec![rng.below(4) as u32];
        arcs.push(RArc {
            from: 0,
            input: RIn::Eps,
            output: ROut::Eps,
            to: 1,
            tags,
            w: 0,
            e: 0,
        });
    }
    RLayer { n, finals, arcs }
}

fn gen_instance(rng: &mut Rng, uni: u32) -> Vec<RLayer> {
    let l = 2 + rng.below(4) as usize;
    let commit = rng.chance(25);
    (0..l)
        .map(|k| gen_layer(rng, commit && k > 0, uni))
        .collect()
}

fn gen_word(rng: &mut Rng, m: &Machine, uni: u32) -> Vec<u32> {
    let mut by_from: Vec<Vec<Rel>> = vec![Vec::new(); m.n as usize];
    for a in &m.arcs {
        by_from[a.from as usize].push(*a);
    }
    let mut word = Vec::new();
    let mut s = 0u32;
    for _ in 0..12 {
        if word.len() >= 4 {
            break;
        }
        if m.fin[s as usize] && rng.chance(25) {
            break;
        }
        let arcs = &by_from[s as usize];
        if arcs.is_empty() {
            break;
        }
        let a = arcs[rng.below(arcs.len() as u64) as usize];
        if let Some(t) = a.o {
            word.push(t);
        }
        s = a.to;
    }
    if rng.chance(25) {
        match rng.below(3) {
            0 if !word.is_empty() => {
                let p = rng.below(word.len() as u64) as usize;
                word[p] = rng.below(uni as u64) as u32;
            }
            1 => word.insert(
                rng.below(word.len() as u64 + 1) as usize,
                rng.below(uni as u64) as u32,
            ),
            _ if !word.is_empty() => {
                word.remove(rng.below(word.len() as u64) as usize);
            }
            _ => {}
        }
    }
    word
}

// ---------------------------------------------------------------- the equality test

fn compare(
    layers: &[RLayer],
    m: &Machine,
    word: &[u32],
    rng: &mut Rng,
    scratch: &mut ProductScratch,
    nfa: &[&dyn Layer],
    stats: &mut Stats,
) {
    let l = layers.len();
    let e = rng.below(3) as u8;
    let og = oracle_graph(m, word, e as u32);
    let prod = Product::new(nfa).unwrap();
    let mut req = Request::new(word, e);
    // cut position varied; sometimes a tiny pre-pass budget so the fallback runs
    let c1 = rng.below(l as u64 + 1) as usize;
    let c2 = rng.below(l as u64 + 1) as usize;
    req.prepass.cuts = vec![c1, c2];
    if rng.chance(25) {
        req.prepass.budget = PrepassBudget {
            states: 1 + rng.below(6),
            steps: 1 + rng.below(40),
            bytes: 1 << 30,
        };
    }
    let theta = if rng.chance(30) {
        Some(rng.below(3000) as u32)
    } else {
        None
    };
    req.theta = theta;
    let out = prod.run(scratch, &req);
    let Outcome::Complete { graph, report } = out else {
        panic!("incomplete under default budgets: {out:?}")
    };
    assert!(report.f_monotone, "f decreased: h is not consistent");
    let (pstates, parcs) = product_arcs(&graph);
    if !og.states.is_empty() {
        stats.accepted += 1;
    }
    if report.prepass != PrepassUsed::None {
        stats.prepass += 1;
    }
    if report.prepass_attempts_overrun > 0 {
        stats.fallback += 1;
    }
    match theta {
        None => {
            assert_eq!(
                pstates, og.states,
                "states differ: word {word:?} e {e} layers {layers:#?}"
            );
            assert_eq!(
                parcs, og.arcs,
                "arcs differ: word {word:?} e {e} cuts {:?} layers {layers:#?}",
                req.prepass.cuts
            );
            assert_eq!(report.best_weight, og.best_weight);
        }
        Some(t) => {
            // every arc on a path of weight <= theta is present with the same tags; nothing is invented
            for (k, v) in &og.arcs {
                if og.through[k] <= t as u64 {
                    assert_eq!(
                        parcs.get(k),
                        Some(v),
                        "arc on a <= theta path missing: {k:?}"
                    );
                }
            }
            for (k, v) in &parcs {
                assert_eq!(
                    og.arcs.get(k),
                    Some(v),
                    "product arc not in the oracle: {k:?}"
                );
            }
            if let Some(b) = og.best_weight {
                if b <= t as u64 {
                    assert_eq!(report.best_weight, Some(b));
                }
            }
        }
    }
}

#[derive(Default, Debug)]
struct Stats {
    instances: u32,
    cases: u32,
    accepted: u32,
    prepass: u32,
    fallback: u32,
    lazy: u32,
}

#[test]
fn random_composition_equals_oracle() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut stats = Stats::default();
    while stats.instances < 10_000 {
        // alphabet 3 to 5; one layer in three instances is lazy, like the speller's copy layer
        let uni = 3 + rng.below(3) as u32;
        let layers = gen_instance(&mut rng, uni);
        let m = oracle_machine(&layers);
        let built: Vec<NfaLayer> = layers.iter().map(|l| l.build(uni)).collect();
        let lazies: Vec<Lazy> = built.iter().map(Lazy).collect();
        let lazy_at = if rng.chance(33) {
            stats.lazy += 1;
            Some(rng.below(layers.len() as u64) as usize)
        } else {
            None
        };
        let refs: Vec<&dyn Layer> = (0..layers.len())
            .map(|k| {
                if lazy_at == Some(k) {
                    &lazies[k] as &dyn Layer
                } else {
                    &built[k] as &dyn Layer
                }
            })
            .collect();
        // one scratch for several words, in random order (C77)
        let mut scratch = ProductScratch::new(layers.len());
        for _ in 0..3 {
            let word = gen_word(&mut rng, &m, uni);
            compare(
                &layers,
                &m,
                &word,
                &mut rng,
                &mut scratch,
                &refs,
                &mut stats,
            );
            stats.cases += 1;
        }
        stats.instances += 1;
    }
    eprintln!("W3 random: {stats:?}");
    assert!(stats.accepted > 3000, "too few non-empty cases: {stats:?}");
    assert!(stats.lazy > 2500, "lazy layers not exercised: {stats:?}");
    assert!(
        stats.prepass > 2500 && stats.fallback > 300,
        "pre-pass not exercised: {stats:?}"
    );
}

// ---------------------------------------------------------------- named cases

fn simple(n: u32, finals: &[u32], arcs: Vec<RArc>) -> RLayer {
    RLayer {
        n,
        finals: (0..n).map(|s| finals.contains(&s)).collect(),
        arcs,
    }
}

fn arc(from: u32, input: RIn, output: ROut, to: u32, tags: &[u32]) -> RArc {
    RArc {
        from,
        input,
        output,
        to,
        tags: tags.to_vec(),
        w: 0,
        e: 0,
    }
}

struct Fixture {
    raw: Vec<RLayer>,
    built: Vec<NfaLayer>,
}

impl Fixture {
    fn new(raw: Vec<RLayer>, universe: u32) -> Self {
        let built = raw.iter().map(|l| l.build(universe)).collect();
        Fixture { raw, built }
    }
    fn refs(&self) -> Vec<&dyn Layer> {
        self.built.iter().map(|l| l as &dyn Layer).collect()
    }
    fn run(&self, word: &[u32], req: impl FnOnce(&mut Request)) -> Outcome {
        let refs = self.refs();
        let prod = Product::new(&refs).unwrap();
        let mut scratch = ProductScratch::new(refs.len());
        let mut r = Request::new(word, 0);
        req(&mut r);
        prod.run(&mut scratch, &r)
    }
    fn graph(&self, word: &[u32]) -> (TrimmedGraph, Report) {
        match self.run(word, |_| {}) {
            Outcome::Complete { graph, report } => (graph, report),
            o => panic!("{o:?}"),
        }
    }
    fn check_oracle(&self, word: &[u32]) -> TrimmedGraph {
        let (g, _) = self.graph(word);
        let og = oracle_graph(&oracle_machine(&self.raw), word, 0);
        let (s, a) = product_arcs(&g);
        assert_eq!(s, og.states);
        assert_eq!(a, og.arcs);
        g
    }
}

fn tags_of(g: &TrimmedGraph) -> BTreeSet<u32> {
    g.arcs
        .iter()
        .flat_map(|a| g.arc_tags(a).iter().map(|t| t.0))
        .collect()
}

/// C63: epsilon-input arcs are moves at every state, even when a lower layer has no token arc to ask with.
#[test]
fn two_committed_layers_both_commit_c63() {
    let f = Fixture::new(
        vec![
            simple(2, &[1], vec![arc(0, RIn::Eps, ROut::Tok(0), 1, &[])]),
            simple(
                3,
                &[2],
                vec![
                    arc(0, RIn::Eps, ROut::Eps, 1, &[1]),
                    arc(1, RIn::Tok(0), ROut::Tok(1), 2, &[]),
                ],
            ),
            simple(
                3,
                &[2],
                vec![
                    arc(0, RIn::Eps, ROut::Eps, 1, &[2]),
                    arc(1, RIn::Tok(1), ROut::Tok(2), 2, &[]),
                ],
            ),
        ],
        3,
    );
    let g = f.check_oracle(&[2]);
    assert!(g.start.is_some(), "word must be accepted");
    assert_eq!(tags_of(&g), BTreeSet::from([1, 2]));
}

/// C39: two product arcs with the same endpoints keep the UNION of their tag sets.
#[test]
fn duplicate_product_arcs_union_tags_c39() {
    let f = Fixture::new(
        vec![
            simple(2, &[1], vec![arc(0, RIn::Eps, ROut::Tok(0), 1, &[])]),
            simple(
                2,
                &[1],
                vec![
                    arc(0, RIn::Tok(0), ROut::Tok(1), 1, &[1]),
                    arc(0, RIn::Tok(0), ROut::Tok(2), 1, &[2]),
                ],
            ),
            simple(
                2,
                &[1],
                vec![
                    arc(0, RIn::Tok(1), ROut::Tok(1), 1, &[3]),
                    arc(0, RIn::Tok(2), ROut::Tok(1), 1, &[0]),
                ],
            ),
        ],
        3,
    );
    let g = f.check_oracle(&[1]);
    let a = g
        .arcs
        .iter()
        .find(|a| g.arc_tags(a).len() > 1)
        .expect("a merged arc");
    assert_eq!(
        g.arc_tags(a).iter().map(|t| t.0).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
}

/// Layer tables also store one arc with the union of the tags of identical arcs.
#[test]
fn builder_unions_tags_of_identical_arcs() {
    let mut meter = BuildMeter::default_limits();
    let mut b = NfaBuilder::new("null affixes", 2, &mut meter);
    let s0 = b.add_state(false).unwrap();
    let s1 = b.add_state(true).unwrap();
    b.add_arc(s0, Input::Eps, Output::Eps, s1, &[Tag(7)], 0, 0)
        .unwrap();
    b.add_arc(s0, Input::Eps, Output::Eps, s1, &[Tag(3)], 0, 0)
        .unwrap();
    let l = b.finish();
    assert_eq!(l.arc_count(), 1);
    assert_eq!(l.tags_between(0, 1), vec![vec![Tag(3), Tag(7)]]);
}

/// C47: a tagged ε:ε loop is a self-loop ARC of the product (no state mark, none dropped).
#[test]
fn tagged_self_loop_is_a_product_arc_c47() {
    let f = Fixture::new(
        vec![
            simple(2, &[1], vec![arc(0, RIn::Eps, ROut::Tok(0), 1, &[])]),
            simple(
                1,
                &[0],
                vec![
                    arc(0, RIn::Eps, ROut::Eps, 0, &[2]),
                    arc(0, RIn::Tok(0), ROut::Tok(0), 0, &[]),
                ],
            ),
        ],
        2,
    );
    let g = f.check_oracle(&[0]);
    assert!(
        g.arcs
            .iter()
            .any(|a| a.src == a.dst && g.arc_tags(a) == [Tag(2)]),
        "tagged self-loop arc missing"
    );
}

fn wide() -> Fixture {
    // L0 emits any word over {a,b}; L1 and L2 copy it. Many interleavings, many states.
    let l0 = simple(
        1,
        &[0],
        vec![
            arc(0, RIn::Eps, ROut::Tok(0), 0, &[]),
            arc(0, RIn::Eps, ROut::Tok(1), 0, &[]),
        ],
    );
    let copy = |t: u32| {
        simple(
            2,
            &[0, 1],
            vec![
                arc(0, RIn::Class(vec![0, 1]), ROut::Same, 0, &[t]),
                arc(0, RIn::Eps, ROut::Eps, 1, &[]),
                arc(1, RIn::Class(vec![0, 1]), ROut::Same, 1, &[]),
            ],
        )
    };
    Fixture::new(vec![l0, copy(1), copy(2)], 2)
}

/// C20, C36: each budget ends the word with `Incomplete` naming it, exactly at the unlimited run's figure.
#[test]
fn each_budget_is_incomplete_not_truncation_c20_c36() {
    let f = wide();
    let word = [0, 1, 0, 1, 1];
    let (g0, rep) = f.graph(&word);
    assert!(g0.start.is_some());
    let used = [
        (CascadeStop::ProductStates, rep.states),
        (CascadeStop::ProductArcs, rep.arcs),
        (CascadeStop::ProductSteps, rep.steps),
        (CascadeStop::ProductBytes, rep.bytes),
    ];
    for (stop, n) in used {
        assert!(n > 10, "{stop:?} too small to test");
        let limit = |b: &mut WordBudget, v: u64| match stop {
            CascadeStop::ProductStates => b.states = v,
            CascadeStop::ProductArcs => b.arcs = v,
            CascadeStop::ProductSteps => b.steps = v,
            CascadeStop::ProductBytes => b.bytes = v,
        };
        match f.run(&word, |r| {
            r.budget = WordBudget::UNLIMITED;
            limit(&mut r.budget, n - 1)
        }) {
            Outcome::Incomplete { stop: s, report } => {
                assert_eq!(s, stop);
                let counted = match stop {
                    CascadeStop::ProductStates => report.states,
                    CascadeStop::ProductArcs => report.arcs,
                    CascadeStop::ProductSteps => report.steps,
                    CascadeStop::ProductBytes => report.bytes,
                };
                assert!(
                    counted < n,
                    "{stop:?}: counter {counted} reached the limit {}",
                    n - 1
                );
            }
            Outcome::Complete { .. } => panic!("{stop:?}: budget overrun returned Complete"),
        }
        match f.run(&word, |r| {
            r.budget = WordBudget::UNLIMITED;
            limit(&mut r.budget, n)
        }) {
            Outcome::Complete { graph, .. } => assert_eq!(graph, g0),
            o => panic!("{stop:?}: exact limit must complete: {o:?}"),
        }
    }
}

/// C51: an overrunning pre-pass is dropped, then shortened, then none; the graph is unchanged.
#[test]
fn prepass_overrun_falls_back_and_never_incomplete_c51() {
    let f = wide();
    let word = [0, 1, 1, 0];
    let (g0, _) = f.graph(&word);
    let tiny = PrepassBudget {
        states: 2,
        steps: 1_000_000,
        bytes: 1 << 30,
    };
    let out = f.run(&word, |r| {
        r.prepass = PrepassPlan {
            cuts: vec![0, 1, 2],
            budget: tiny,
        }
    });
    let Outcome::Complete { graph, report } = out else {
        panic!("{out:?}")
    };
    assert_eq!(graph, g0);
    assert_eq!(report.prepass, PrepassUsed::None);
    assert_eq!(report.prepass_attempts_overrun, 3);
    // and a budget that fits keeps the shortest overrun-free suffix
    let out = f.run(&word, |r| {
        r.prepass = PrepassPlan {
            cuts: vec![0],
            budget: PrepassBudget::default(),
        }
    });
    let Outcome::Complete { graph, report } = out else {
        panic!("{out:?}")
    };
    assert_eq!(graph, g0);
    assert_eq!(report.prepass, PrepassUsed::Suffix(0));
}

/// C65: the pre-pass is never charged to the word, so a run with one completes at the same limits.
#[test]
fn prepass_is_not_charged_to_the_word_c65() {
    let f = wide();
    let word = [0, 1, 1, 0, 1];
    let (g0, rep) = f.graph(&word);
    let out = f.run(&word, |r| {
        r.prepass = PrepassPlan {
            cuts: vec![1],
            budget: PrepassBudget::default(),
        };
        r.budget = WordBudget {
            states: rep.states,
            arcs: rep.arcs,
            steps: rep.steps,
            bytes: rep.bytes,
        };
    });
    let Outcome::Complete { graph, report } = out else {
        panic!("{out:?}")
    };
    assert_eq!(graph, g0);
    assert!(report.prepass_states > 0 && report.prepass_steps > 0 && report.prepass_bytes > 0);
}

/// C22: the pre-pass's top tape is FREE. Layer 1 reads `a`, which the typed word `d` never contains.
#[test]
fn prepass_top_tape_is_free_c22() {
    let f = Fixture::new(
        vec![
            simple(2, &[1], vec![arc(0, RIn::Eps, ROut::Tok(0), 1, &[])]),
            simple(2, &[1], vec![arc(0, RIn::Tok(0), ROut::Tok(1), 1, &[])]),
            simple(2, &[1], vec![arc(0, RIn::Tok(1), ROut::Tok(2), 1, &[])]),
            simple(2, &[1], vec![arc(0, RIn::Tok(2), ROut::Tok(3), 1, &[])]),
        ],
        4,
    );
    // cut = 1: suffix is layers 1..3, whose input tokens (a, b, c) are not in the word [d]
    let out = f.run(&[3], |r| {
        r.prepass = PrepassPlan {
            cuts: vec![1],
            budget: PrepassBudget::default(),
        }
    });
    let Outcome::Complete { graph, report } = out else {
        panic!("{out:?}")
    };
    assert_eq!(report.prepass, PrepassUsed::Suffix(1));
    assert!(
        graph.start.is_some(),
        "a pre-pass with a constrained top tape cuts the only path"
    );
}

/// C34: the pre-pass suffix is contiguous. Same chain; the cut leaves a layer in the middle of the suffix.
#[test]
fn prepass_suffix_is_contiguous_c34() {
    let f = Fixture::new(
        vec![
            simple(2, &[1], vec![arc(0, RIn::Eps, ROut::Tok(0), 1, &[])]),
            simple(2, &[1], vec![arc(0, RIn::Tok(0), ROut::Tok(1), 1, &[])]),
            simple(2, &[1], vec![arc(0, RIn::Tok(1), ROut::Tok(2), 1, &[])]),
            simple(2, &[1], vec![arc(0, RIn::Tok(2), ROut::Tok(3), 1, &[])]),
        ],
        4,
    );
    for cut in 1..=3 {
        let out = f.run(&[3], |r| {
            r.prepass = PrepassPlan {
                cuts: vec![cut],
                budget: PrepassBudget::default(),
            }
        });
        let Outcome::Complete { graph, .. } = out else {
            panic!("{out:?}")
        };
        assert!(graph.start.is_some(), "cut {cut} lost the path");
        assert_eq!(graph.arcs.len(), 1, "one chain through all four layers");
    }
}

/// C21: the trim keeps every co-reachable state, including routes through self-loops and tagged arcs.
#[test]
fn trim_keeps_every_co_reachable_state_c21() {
    let f = Fixture::new(
        vec![
            simple(
                3,
                &[2],
                vec![
                    arc(0, RIn::Eps, ROut::Tok(0), 1, &[0]),
                    arc(1, RIn::Eps, ROut::Tok(1), 2, &[1]),
                ],
            ),
            simple(
                1,
                &[0],
                vec![
                    arc(0, RIn::Class(vec![0, 1]), ROut::Same, 0, &[2]),
                    arc(0, RIn::Eps, ROut::Eps, 0, &[3]),
                ],
            ),
        ],
        2,
    );
    let g = f.check_oracle(&[0, 1]);
    assert_eq!(g.state_count(), 3);
    assert_eq!(
        g.arcs.iter().filter(|a| a.src == a.dst).count(),
        3,
        "the tagged loop at every state"
    );
    // a dead branch is dropped
    let (dead, _) = f.graph(&[1, 1]);
    assert!(dead.start.is_none() && dead.arcs.is_empty());
}

/// C77: per-word scratch is reset: a second, different word on the same scratch equals a fresh run.
#[test]
fn scratch_reset_between_words_c77() {
    let f = wide();
    let refs = f.refs();
    let prod = Product::new(&refs).unwrap();
    let mut shared = ProductScratch::new(refs.len());
    // an aborted word leaves its tables behind; the next word must not see them
    let mut tight = Request::new(&[0, 1, 1, 0, 1], 0);
    tight.budget = WordBudget {
        states: 6,
        ..WordBudget::UNLIMITED
    };
    assert!(matches!(
        prod.run(&mut shared, &tight),
        Outcome::Incomplete { .. }
    ));
    let words: [&[u32]; 4] = [&[0, 1, 1, 0], &[1], &[], &[0, 0, 1, 1, 0, 1]];
    for w in words {
        let a = prod.run(&mut shared, &Request::new(w, 0));
        let b = prod.run(&mut ProductScratch::new(refs.len()), &Request::new(w, 0));
        let (Outcome::Complete { graph: ga, .. }, Outcome::Complete { graph: gb, .. }) = (a, b)
        else {
            panic!()
        };
        assert_eq!(ga, gb, "word {w:?}");
    }
}

/// C50: build bytes are charged BEFORE the allocation, and class arcs stay symbolic.
#[test]
fn build_bytes_charged_before_allocation_and_symbolic_c50() {
    // symbolic: a class over 5000 tokens is one arc and a few hundred bytes
    let mut meter = BuildMeter::new(4000, u64::MAX);
    let l = identity_layer(5000, &mut meter).expect("one symbolic arc fits a tiny ceiling");
    assert_eq!(l.arc_count(), 1);
    // before: a refused charge leaves the table unchanged and the meter under its limit
    let mut meter = BuildMeter::new(64 + 48 + 16, u64::MAX);
    let mut b = NfaBuilder::new("tiny", 2, &mut meter);
    let s = b.add_state(true).unwrap();
    let r = b.add_arc(s, Input::Eps, Output::Eps, s, &[], 0, 0);
    let r2 = b.add_arc(s, Input::Tok(0), Output::Tok(0), s, &[], 0, 0);
    assert!(r.is_ok() && r2.is_err());
    let err = r2.unwrap_err();
    assert_eq!(err.dim, BuildStop::BuildBytes);
    let l = b.finish();
    assert_eq!(
        l.arc_count(),
        1,
        "the refused arc was allocated before it was charged"
    );
    assert!(meter.bytes() <= 64 + 48 + 16);
}

/// C60, C70, C76: the identity gate refuses an obligatory rule, or replaces it in `Id` mode.
#[test]
fn identity_gate_and_wrap_c60_c70_c76() {
    let net = |universe| {
        // obligatory a -> b on {a, b}: no x:x arc for a
        let mut meter = BuildMeter::default_limits();
        let mut b = NfaBuilder::new("rule", universe, &mut meter);
        let s = b.add_state(true).unwrap();
        b.add_arc(s, Input::Tok(0), Output::Tok(1), s, &[Tag(9)], 0, 0)
            .unwrap();
        b.add_arc(s, Input::Tok(1), Output::Same, s, &[], 0, 0)
            .unwrap();
        b.finish()
    };
    let mut meter = BuildMeter::default_limits();
    assert!(
        !net(2).identity_test(&mut meter).unwrap(),
        "C60: a net without identity passed"
    );
    assert!(identity_layer(2, &mut meter)
        .unwrap()
        .identity_test(&mut meter)
        .unwrap());
    // Id mode: the net is replaced, the word `a` passes; run mode: the rule rewrites a to b
    let top = simple(2, &[1], vec![arc(0, RIn::Eps, ROut::Tok(0), 1, &[])]);
    let run_with = |runs: bool, word: u32| {
        let mut meter = BuildMeter::default_limits();
        let layer = prepare_layer(net(2), runs, &mut meter).unwrap();
        let top = top.build(2);
        let refs: Vec<&dyn Layer> = vec![&top, &layer];
        let p = Product::new(&refs).unwrap();
        let o = p.run(&mut ProductScratch::new(2), &Request::new(&[word], 0));
        let Outcome::Complete { graph, .. } = o else {
            panic!()
        };
        graph.start.is_some()
    };
    assert!(
        run_with(false, 0),
        "C70: Id mode must pass the token unchanged"
    );
    assert!(
        !run_with(true, 0) && run_with(true, 1),
        "run mode rewrites a to b"
    );
    // C76
    let mut tiny = BuildMeter::new(u64::MAX, 0);
    let r = identity_layer(2, &mut BuildMeter::default_limits())
        .unwrap()
        .identity_test(&mut tiny);
    assert!(
        matches!(
            r,
            Err(BuildRefused {
                dim: BuildStop::BuildSteps,
                ..
            })
        ),
        "C76: {r:?}"
    );
    let mut tiny = BuildMeter::new(u64::MAX, 0);
    assert!(prepare_layer(net(2), false, &mut tiny).is_err());
}

/// The lazy-layer scratch: a layer that interns its states per word, behind the same interface.
struct Lazy<'a>(&'a NfaLayer);
impl Layer for Lazy<'_> {
    fn universe(&self) -> u32 {
        self.0.universe()
    }
    fn is_eager(&self) -> bool {
        false
    }
    fn has_edits(&self) -> bool {
        self.0.has_edits()
    }
    fn start(&self, s: &mut LayerScratch) -> StateId {
        // ids equal the underlying ids, so keys compare with the oracle's
        for k in 0..self.0.state_count() as u32 {
            s.intern(&[k]);
        }
        let u = self.0.start(s);
        s.intern(&[u])
    }
    fn is_final(&self, s: &LayerScratch, id: StateId) -> bool {
        self.0.is_final(s, s.interned[id as usize][0])
    }
    fn arcs_from<'a>(
        &'a self,
        s: &mut LayerScratch,
        id: StateId,
        input: Option<Token>,
        f: &mut dyn FnMut(Arc<'a>),
    ) -> u64 {
        let under = s.interned[id as usize][0];
        let mut v = Vec::new();
        let st = self.0.arcs_from(s, under, input, &mut |a| v.push(a));
        for a in v {
            let to = s.intern(&[a.to]);
            f(Arc { to, ..a });
        }
        st
    }
    fn arcs_free<'a>(
        &'a self,
        s: &mut LayerScratch,
        id: StateId,
        f: &mut dyn FnMut(Arc<'a>),
    ) -> u64 {
        let under = s.interned[id as usize][0];
        let mut v = Vec::new();
        let st = self.0.arcs_free(s, under, &mut |a| v.push(a));
        for a in v {
            let to = s.intern(&[a.to]);
            f(Arc { to, ..a });
        }
        st
    }
}

#[test]
fn lazy_layer_is_kept_out_of_the_suffix_and_scratch_is_per_word() {
    let f = wide();
    let lazy = Lazy(&f.built[1]);
    let refs: Vec<&dyn Layer> = vec![&f.built[0], &lazy, &f.built[2]];
    let p = Product::new(&refs).unwrap();
    let mut scratch = ProductScratch::new(3);
    for word in [&[0u32, 1, 1][..], &[1], &[0, 0, 1, 0]] {
        let mut req = Request::new(word, 0);
        // cuts 0 and 1 would put the lazy layer in the suffix: skipped; cut 2 is allowed
        req.prepass = PrepassPlan {
            cuts: vec![0, 1, 2],
            budget: PrepassBudget::default(),
        };
        let Outcome::Complete { graph, report } = p.run(&mut scratch, &req) else {
            panic!()
        };
        assert_eq!(report.prepass, PrepassUsed::Suffix(2));
        let (want, _) = f.graph(word);
        assert_eq!(product_arcs(&graph).1.len(), product_arcs(&want).1.len());
    }
}

/// The weight ceiling and best-first order: the cheapest accepting weight is found first.
#[test]
fn best_first_finds_least_weight_and_theta_cuts() {
    let mut cheap = arc(0, RIn::Eps, ROut::Tok(0), 1, &[0]);
    cheap.w = 4000;
    let mut dear = arc(0, RIn::Eps, ROut::Tok(0), 1, &[1]);
    dear.w = 1000;
    dear.e = 1;
    let f = Fixture::new(vec![simple(2, &[1], vec![cheap, dear])], 1);
    let out = f.run(&[0], |r| r.max_edits = 1);
    let Outcome::Complete { graph, report } = out else {
        panic!()
    };
    assert_eq!(report.best_weight, Some(1000));
    assert_eq!(graph.arcs.len(), 2);
    let out = f.run(&[0], |r| {
        r.max_edits = 1;
        r.theta = Some(1500)
    });
    let Outcome::Complete { graph, .. } = out else {
        panic!()
    };
    assert_eq!(graph.arcs.len(), 1);
    let out = f.run(&[0], |r| r.max_edits = 0);
    let Outcome::Complete { graph, .. } = out else {
        panic!()
    };
    assert_eq!(graph.arcs.len(), 1, "the edit arc is not created at E = 0");
    let _ = HashMap::<u8, u8>::new();
}

/// The cut is pulled up to the first edit layer, or the table's edit count disagrees and the word is rejected.
#[test]
fn cut_is_pulled_up_to_the_first_edit_layer() {
    let mut edit = arc(0, RIn::Tok(0), ROut::Tok(1), 1, &[]);
    edit.w = 1000;
    edit.e = 1;
    let f = Fixture::new(
        vec![
            simple(2, &[1], vec![arc(0, RIn::Eps, ROut::Tok(0), 1, &[])]),
            simple(2, &[1], vec![edit]),
            simple(2, &[1], vec![arc(0, RIn::Tok(1), ROut::Tok(2), 1, &[])]),
        ],
        3,
    );
    let out = f.run(&[2], |r| {
        r.max_edits = 1;
        r.prepass = PrepassPlan {
            cuts: vec![2],
            budget: PrepassBudget::default(),
        };
    });
    let Outcome::Complete { graph, report } = out else {
        panic!("{out:?}")
    };
    assert!(graph.start.is_some(), "the word is accepted with one edit");
    assert_eq!(report.prepass, PrepassUsed::Suffix(1));
    assert!(report
        .prepass_notes
        .contains(&PrepassNote::PulledUpToFirstEdit {
            requested: 2,
            cut: 1
        }));
}

/// The report says why the pre-pass ladder collapsed: lazy layer in the suffix, empty suffix, duplicates.
#[test]
fn report_says_why_no_prepass() {
    let f = wide();
    let lazy = Lazy(&f.built[1]);
    let refs: Vec<&dyn Layer> = vec![&f.built[0], &lazy, &f.built[2]];
    let p = Product::new(&refs).unwrap();
    let mut req = Request::new(&[0, 1], 0);
    req.prepass.cuts = vec![0, 1, 3, 2, 2];
    let Outcome::Complete { report, .. } = p.run(&mut ProductScratch::new(3), &req) else {
        panic!()
    };
    assert_eq!(report.prepass, PrepassUsed::Suffix(2));
    assert_eq!(
        report.prepass_notes,
        vec![
            PrepassNote::LazyLayerInSuffix {
                requested: 0,
                cut: 0
            },
            PrepassNote::LazyLayerInSuffix {
                requested: 1,
                cut: 1
            },
            PrepassNote::EmptySuffix { requested: 3 },
        ]
    );
}

/// A suffix weight at the u32 limit stays below the INF sentinel, so the accepted word is kept.
#[test]
fn saturated_prepass_distance_is_not_a_proven_non_word() {
    let mut a = arc(0, RIn::Eps, ROut::Tok(0), 1, &[]);
    a.w = u32::MAX - 1;
    let mut b = arc(1, RIn::Eps, ROut::Tok(0), 2, &[]);
    b.w = 1;
    let f = Fixture::new(vec![simple(3, &[2], vec![a, b])], 1);
    let word = [0, 0];
    let (plain, _) = f.graph(&word);
    assert!(plain.start.is_some());
    assert_eq!(plain.arcs.len(), 2);
    for theta in [None, Some(u32::MAX)] {
        let Outcome::Complete { graph, report } = f.run(&word, |r| {
            r.prepass.cuts = vec![0];
            r.theta = theta;
        }) else {
            panic!()
        };
        assert_eq!(report.prepass, PrepassUsed::Suffix(0));
        assert_eq!(graph.start, Some(0), "theta {theta:?}: accepted word lost");
        assert_eq!(graph.arcs.len(), 2);
    }
}

#[test]
fn distance_to_accept_clamps_below_the_sentinel() {
    use super::trim::distance_to_accept;
    let d = distance_to_accept(3, &[(0, 1, u32::MAX - 1), (1, 2, 5)], &[false, false, true]);
    assert_eq!(d, vec![u32::MAX - 1, 5, 0]);
    // unreachable stays the sentinel
    let d = distance_to_accept(2, &[], &[false, true]);
    assert_eq!(d, vec![u32::MAX, 0]);
}

/// Weights near the u32 limit never turn an accepted word into a non-word: the states equal the zero-weight oracle's.
#[test]
fn random_near_limit_weights_never_lose_a_word() {
    let mut rng = Rng(0x0123_4567_89ab_cdef);
    let (mut instances, mut accepted, mut prepassed) = (0, 0, 0);
    while instances < 4_000 {
        let uni = 3 + rng.below(3) as u32;
        let mut layers = gen_instance(&mut rng, uni);
        let zero = layers.clone();
        for l in &mut layers {
            for a in &mut l.arcs {
                a.w = match rng.below(4) {
                    0 => u32::MAX - 1,
                    1 => u32::MAX,
                    2 => u32::MAX - 1000 + a.w % 7,
                    _ => a.w,
                };
            }
        }
        let zero: Vec<RLayer> = zero
            .into_iter()
            .map(|mut l| {
                l.arcs.iter_mut().for_each(|a| a.w = 0);
                l
            })
            .collect();
        let m = oracle_machine(&zero);
        let built: Vec<NfaLayer> = layers.iter().map(|l| l.build(uni)).collect();
        let refs: Vec<&dyn Layer> = built.iter().map(|l| l as &dyn Layer).collect();
        let prod = Product::new(&refs).unwrap();
        let mut scratch = ProductScratch::new(layers.len());
        for _ in 0..3 {
            let word = gen_word(&mut rng, &m, uni);
            let e = rng.below(3) as u8;
            let want = oracle_graph(&m, &word, e as u32).states;
            let mut req = Request::new(&word, e);
            req.prepass.cuts = vec![rng.below(layers.len() as u64) as usize];
            let Outcome::Complete { graph, report } = prod.run(&mut scratch, &req) else {
                panic!("incomplete")
            };
            let got: BTreeSet<Key> = (0..graph.state_count() as u32)
                .map(|s| graph.key(s).to_vec())
                .collect();
            assert_eq!(got, want, "word {word:?} e {e} layers {layers:#?}");
            accepted += !want.is_empty() as u32;
            prepassed += (report.prepass != PrepassUsed::None) as u32;
        }
        instances += 1;
    }
    assert!(
        accepted > 1000 && prepassed > 1000,
        "{accepted} {prepassed}"
    );
}

/// A lazy layer's interned states count against the product bytes: the eager run's figure no longer suffices.
#[test]
fn lazy_layer_interning_is_charged_to_product_bytes() {
    let f = wide();
    let word = [0, 1, 0, 1, 1];
    let (_, eager) = f.graph(&word);
    let lazy = Lazy(&f.built[1]);
    let refs: Vec<&dyn Layer> = vec![&f.built[0], &lazy, &f.built[2]];
    let p = Product::new(&refs).unwrap();
    let mut scratch = ProductScratch::new(3);
    let run = |scratch: &mut ProductScratch, bytes: u64| {
        let mut req = Request::new(&word, 0);
        req.budget.bytes = bytes;
        p.run(scratch, &req)
    };
    let Outcome::Complete {
        report: unlimited, ..
    } = run(&mut scratch, u64::MAX)
    else {
        panic!()
    };
    let interned = budget::interned_bytes(1) * f.built[1].state_count() as u64;
    assert_eq!(unlimited.bytes, eager.bytes + interned);
    match run(&mut scratch, eager.bytes) {
        Outcome::Incomplete { stop, report } => {
            assert_eq!(stop, CascadeStop::ProductBytes);
            assert!(report.bytes <= eager.bytes);
        }
        o => panic!("{o:?}"),
    }
    assert!(matches!(
        run(&mut scratch, unlimited.bytes),
        Outcome::Complete { .. }
    ));
}

#[test]
fn intern_refuses_before_allocating_past_the_allowance() {
    let mut s = LayerScratch::default();
    s.begin(budget::interned_bytes(1));
    assert_eq!(s.intern(&[7]), 0);
    assert_eq!(s.intern(&[7]), 0, "a hit is free");
    assert!(!s.refused());
    assert_eq!(s.intern(&[8]), u32::MAX);
    assert!(s.refused());
    assert_eq!(s.interned.len(), 1);
    assert_eq!(s.index.len(), 1);
    assert_eq!(s.take_charged(), budget::interned_bytes(1));
}

/// A top layer with a token input arc is a free-tape move, not a silently dropped word.
#[test]
fn top_layer_token_and_class_inputs_are_moves() {
    let a = arc(0, RIn::Tok(0), ROut::Same, 0, &[5]);
    let f = Fixture::new(vec![simple(1, &[0], vec![a])], 1);
    let (g, _) = f.graph(&[0]);
    assert_eq!(g.start, Some(0), "accepted word lost");
    assert_eq!(tags_of(&g), BTreeSet::from([5]));
    let b = arc(0, RIn::Class(vec![0, 1]), ROut::Same, 0, &[6]);
    let f = Fixture::new(vec![simple(1, &[0], vec![b])], 2);
    let g = f.check_oracle(&[1, 0]);
    assert_eq!(tags_of(&g), BTreeSet::from([6]));
    for cuts in [vec![], vec![0]] {
        let Outcome::Complete { graph, .. } = f.run(&[1, 0], |r| r.prepass.cuts = cuts.clone())
        else {
            panic!()
        };
        assert_eq!(graph, g);
    }
}

#[test]
fn every_packed_tag_decodes_to_the_kind_it_was_built_as() {
    let max = (1u32 << Tag::ID_BITS) - 1;
    let ids = [0, 1, 2, 3, 1000, 1 << 20, max - 1, max];
    let mut seen = BTreeSet::new();
    for id in ids {
        for (tag, want) in [
            (Tag::root(id), TagKind::Root(id)),
            (Tag::nonhead(id), TagKind::NonHead(id)),
            (Tag::rule(id), TagKind::Rule(id)),
        ] {
            assert_eq!(tag.kind(), Ok(want));
            assert!(seen.insert(tag.0), "two kinds packed to one word");
        }
    }
    // the unused kind and every word of it is an error, never a default kind
    for w in [3u32 << Tag::ID_BITS, u32::MAX, (3 << Tag::ID_BITS) | 7] {
        assert_eq!(Tag(w).kind(), Err(BadTag(Tag(w))));
    }
    let mut rng = Rng(77);
    for _ in 0..10_000 {
        let w = rng.next() as u32;
        match Tag(w).kind() {
            Ok(TagKind::Root(i)) => assert_eq!(Tag::root(i), Tag(w)),
            Ok(TagKind::NonHead(i)) => assert_eq!(Tag::nonhead(i), Tag(w)),
            Ok(TagKind::Rule(i)) => assert_eq!(Tag::rule(i), Tag(w)),
            Err(BadTag(t)) => assert_eq!(t.0 >> Tag::ID_BITS, 3),
        }
    }
}

#[test]
#[should_panic(expected = "30 bits")]
fn tag_id_over_thirty_bits_is_refused() {
    let _ = Tag::rule(1 << Tag::ID_BITS);
}

#[test]
fn product_refuses_layers_of_different_alphabets() {
    let a = Fixture::new(vec![simple(1, &[0], vec![])], 2);
    let b = Fixture::new(vec![simple(1, &[0], vec![])], 3);
    let refs: Vec<&dyn Layer> = vec![&a.built[0], &b.built[0]];
    assert_eq!(
        Product::new(&refs).err(),
        Some(ProductError::UniverseMismatch {
            layer: 1,
            expected: 2,
            found: 3
        })
    );
    assert_eq!(Product::new(&[]).err(), Some(ProductError::NoLayers));
}

#[test]
fn id_space_is_refused_even_when_unlimited() {
    let mut m = budget::WordMeter::new(WordBudget::UNLIMITED);
    m.states = budget::ID_LIMIT;
    assert_eq!(m.charge_state(1), Err(CascadeStop::ProductStates));
    m.arcs = budget::ID_LIMIT;
    assert_eq!(m.charge_arc(), Err(CascadeStop::ProductArcs));
    m.states = budget::ID_LIMIT - 1;
    assert_eq!(m.charge_state(1), Ok(()));
}

/// A lazy layer whose interned key is longer than the product key, interning on demand.
struct Fat<'a>(&'a NfaLayer);
const FAT_WORDS: usize = 20;
impl Fat<'_> {
    fn key(u: u32) -> [u32; FAT_WORDS] {
        let mut k = [0; FAT_WORDS];
        k[0] = u;
        k
    }
}
impl Layer for Fat<'_> {
    fn universe(&self) -> u32 {
        self.0.universe()
    }
    fn is_eager(&self) -> bool {
        false
    }
    fn has_edits(&self) -> bool {
        self.0.has_edits()
    }
    fn start(&self, s: &mut LayerScratch) -> StateId {
        let u = self.0.start(s);
        s.intern(&Self::key(u))
    }
    fn is_final(&self, s: &LayerScratch, id: StateId) -> bool {
        self.0.is_final(s, s.interned[id as usize][0])
    }
    fn arcs_from<'a>(
        &'a self,
        s: &mut LayerScratch,
        id: StateId,
        input: Option<Token>,
        f: &mut dyn FnMut(Arc<'a>),
    ) -> u64 {
        let under = s.interned[id as usize][0];
        let mut v = Vec::new();
        let st = self.0.arcs_from(s, under, input, &mut |a| v.push(a));
        for a in v {
            let to = s.intern(&Self::key(a.to));
            f(Arc { to, ..a });
        }
        st
    }
    fn arcs_free<'a>(
        &'a self,
        s: &mut LayerScratch,
        id: StateId,
        f: &mut dyn FnMut(Arc<'a>),
    ) -> u64 {
        let under = s.interned[id as usize][0];
        let mut v = Vec::new();
        let st = self.0.arcs_free(s, under, &mut |a| v.push(a));
        for a in v {
            let to = s.intern(&Self::key(a.to));
            f(Arc { to, ..a });
        }
        st
    }
}

/// A refused intern ends the word on bytes at every limit; the refused state id is never used.
#[test]
fn refused_intern_ends_the_word_at_every_byte_limit() {
    let top = simple(2, &[1], vec![arc(0, RIn::Eps, ROut::Tok(0), 1, &[])]);
    let bottom = simple(2, &[1], vec![arc(0, RIn::Tok(0), ROut::Same, 1, &[])]);
    let f = Fixture::new(vec![top, bottom], 1);
    let fat = Fat(&f.built[1]);
    let refs: Vec<&dyn Layer> = vec![&f.built[0], &fat];
    let p = Product::new(&refs).unwrap();
    let word = [0];
    let run = |bytes: u64| {
        let mut req = Request::new(&word, 0);
        req.budget.bytes = bytes;
        p.run(&mut ProductScratch::new(2), &req)
    };
    let Outcome::Complete { report, graph } = run(u64::MAX) else {
        panic!()
    };
    assert!(graph.start.is_some());
    let full = report.bytes;
    let mut at_intern = 0;
    for limit in 0..full {
        match run(limit) {
            Outcome::Incomplete { stop, report } => {
                assert_eq!(stop, CascadeStop::ProductBytes, "limit {limit}");
                assert!(report.bytes <= limit);
                at_intern += 1;
            }
            Outcome::Complete { .. } => panic!("limit {limit} of {full} completed"),
        }
    }
    assert_eq!(at_intern, full);
    assert!(matches!(run(full), Outcome::Complete { .. }));
}

/// The trim's allocations are charged to the product bytes before it runs.
#[test]
fn trim_allocations_are_charged_to_product_bytes() {
    let f = wide();
    let word = [0, 1, 0, 1, 1];
    let (_, rep) = f.graph(&word);
    let trim_cost = trim::trim_bytes(rep.states as usize, rep.arcs as usize, trim::KEY_PREFIX + 3);
    assert!(rep.bytes > trim_cost);
    let run = |bytes: u64| f.run(&word, |r| r.budget.bytes = bytes);
    match run(rep.bytes - 1) {
        Outcome::Incomplete { stop, report } => {
            assert_eq!(stop, CascadeStop::ProductBytes);
            assert!(
                report.bytes <= rep.bytes - trim_cost,
                "stopped before the trim charge"
            );
        }
        o => panic!("{o:?}"),
    }
    assert!(matches!(run(rep.bytes), Outcome::Complete { .. }));
}
