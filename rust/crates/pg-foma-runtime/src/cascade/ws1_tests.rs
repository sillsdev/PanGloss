//! Tests of the `Units`, `Fold`, `Err1` and `Err2` layers, the fold table, the weight loader and the generator fixture.

use std::collections::{BTreeMap, BTreeSet};

use super::budget::{BuildMeter, PrepassBudget, WordBudget};
use super::casefold_table::{CASEFOLD, CASEFOLD_UNICODE_VERSION};
use super::*;

const M1: char = '\u{301}';
const M2: char = '\u{300}';
const M3: char = '\u{302}';

// ---------------------------------------------------------------- harness

/// Any concatenation of representations; each arc carries its character token as a tag.
fn rep_layer(model: &UnitModel, reps: &[String], meter: &mut BuildMeter) -> NfaLayer {
    let mut b = NfaBuilder::new("Rep", model.universe(), meter);
    let hub = b.add_state(true).unwrap();
    b.set_start(hub);
    for r in reps {
        let cs: Vec<char> = nfd_string(r).chars().collect();
        let mut cur = hub;
        for (i, &c) in cs.iter().enumerate() {
            let t = model.char_token(c).expect("rep character in the model");
            let to = if i + 1 == cs.len() {
                hub
            } else {
                b.add_state(false).unwrap()
            };
            b.add_arc(cur, Input::Eps, Output::Tok(t), to, &[Tag(t)], 0, 0)
                .unwrap();
            cur = to;
        }
    }
    b.finish()
}

struct Harness {
    model: UnitModel,
    rep: NfaLayer,
    units: NfaLayer,
    fold: NfaLayer,
    err1: NfaLayer,
    err2: NfaLayer,
    typed: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Found {
    dist: u8,
    weight: u32,
}

impl Harness {
    fn new(reps: &[&str], typed: &str) -> Harness {
        Self::with_weights(reps, typed, &ErrWeights::default())
    }

    fn with_weights(reps: &[&str], typed: &str, w: &ErrWeights) -> Harness {
        let reps: Vec<String> = reps.iter().map(|r| nfd_string(r)).collect();
        let chars: Vec<char> = reps.iter().flat_map(|r| r.chars()).collect();
        let model = UnitModel::new(&chars, typed);
        let mut m = BuildMeter::default_limits();
        let rep = rep_layer(&model, &reps, &mut m);
        let units = build_units_layer(&model, &mut m).unwrap();
        let fold = build_fold_layer(&model, &mut m).unwrap();
        let err1 = build_err_layer("Err1", &model, w, &mut m).unwrap();
        let err2 = build_err_layer("Err2", &model, w, &mut m).unwrap();
        Harness {
            model,
            rep,
            units,
            fold,
            err1,
            err2,
            typed: nfd_string(typed),
        }
    }

    fn graph(&self, max_edits: u8, cuts: &[usize], tiny_prepass: bool) -> (TrimmedGraph, Report) {
        self.graph_with(max_edits, cuts, tiny_prepass, WordBudget::UNLIMITED)
    }

    fn graph_with(
        &self,
        max_edits: u8,
        cuts: &[usize],
        tiny_prepass: bool,
        budget: WordBudget,
    ) -> (TrimmedGraph, Report) {
        let layers: [&dyn Layer; 5] = [&self.rep, &self.units, &self.fold, &self.err1, &self.err2];
        let product = Product::new(&layers).unwrap();
        let mut scratch = ProductScratch::new(5);
        let mut req = Request::new(self.model.typed_tokens(), max_edits);
        req.budget = budget;
        req.prepass = PrepassPlan {
            cuts: cuts.to_vec(),
            budget: if tiny_prepass {
                PrepassBudget {
                    states: 3,
                    steps: 1_000_000,
                    bytes: 1 << 20,
                }
            } else {
                PrepassBudget::default()
            },
        };
        match product.run(&mut scratch, &req) {
            Outcome::Complete { graph, report } => (graph, report),
            Outcome::Incomplete { stop, .. } => panic!("incomplete: {stop:?}"),
        }
    }

    /// Strings within `max_edits` with at most `k` marks per unit outside the typed word, with least edits and weight.
    fn strings(&self, max_edits: u8, k: usize, cuts: &[usize]) -> BTreeMap<String, Found> {
        let (g, _) = self.graph(max_edits, cuts, false);
        self.read_strings(&g, k)
    }

    fn read_strings(&self, g: &TrimmedGraph, k: usize) -> BTreeMap<String, Found> {
        let mut out = BTreeMap::new();
        self.walk(g, k, &mut |s, f| {
            out.insert(s.to_string(), f);
        });
        out
    }

    /// Calls `emit` once per distinct accepted string, without collecting them.
    fn walk(&self, g: &TrimmedGraph, k: usize, emit: &mut dyn FnMut(&str, Found)) {
        let Some(start) = g.start else { return };
        let mut by_src: Vec<Vec<(u32, u32, Vec<char>)>> = vec![Vec::new(); g.state_count()];
        for a in &g.arcs {
            let tags: Vec<char> = g
                .arc_tags(a)
                .iter()
                .map(|t| self.model.chars()[t.0 as usize])
                .collect();
            by_src[a.src as usize].push((a.dst, a.weight, tags));
        }
        let bound = k.max(self.model.max_typed_marks());
        let typed_units: BTreeSet<(String, String)> = cut_units(&self.typed)
            .into_iter()
            .map(|(b, m)| (fold_base(&b), m))
            .collect();
        let mut cfg: BTreeMap<u32, u32> = BTreeMap::new();
        cfg.insert(start, 0);
        let cfg = closure(&by_src, cfg);
        let mut s = String::new();
        let ctx = Ctx {
            g,
            by_src: &by_src,
            bound,
            k,
            typed_units: &typed_units,
            typed: &self.typed,
        };
        dfs(&ctx, &mut s, 0, cfg, emit);
    }
}

struct Ctx<'a> {
    g: &'a TrimmedGraph,
    by_src: &'a [Vec<(u32, u32, Vec<char>)>],
    bound: usize,
    k: usize,
    typed_units: &'a BTreeSet<(String, String)>,
    typed: &'a str,
}

/// Relax untagged (character-free) arcs to a fixpoint, keeping the least weight per state.
fn closure(
    by_src: &[Vec<(u32, u32, Vec<char>)>],
    mut cfg: BTreeMap<u32, u32>,
) -> BTreeMap<u32, u32> {
    let mut work: Vec<u32> = cfg.keys().copied().collect();
    while let Some(s) = work.pop() {
        let w = cfg[&s];
        for (dst, aw, tags) in &by_src[s as usize] {
            if !tags.is_empty() {
                continue;
            }
            let nw = w + aw;
            if cfg.get(dst).is_none_or(|&old| nw < old) {
                cfg.insert(*dst, nw);
                work.push(*dst);
            }
        }
    }
    cfg
}

fn unit_ok(ctx: &Ctx, s: &str) -> bool {
    cut_units(s).iter().all(|(b, m)| {
        m.chars().count() <= ctx.k || ctx.typed_units.contains(&(fold_base(b), m.clone()))
    })
}

/// A mark run deeper than k is only worth walking while it is still a prefix of a typed unit's stack.
fn deep_prefix_ok(ctx: &Ctx, s: &str, c: char) -> bool {
    let mut t = String::from(s);
    t.push(c);
    let units = cut_units(&t);
    let Some((b, m)) = units.last() else {
        return true;
    };
    let fb = fold_base(b);
    ctx.typed_units
        .iter()
        .any(|(tb, tm)| *tb == fb && tm.starts_with(m.as_str()))
}

fn dfs(
    ctx: &Ctx,
    s: &mut String,
    run: usize,
    cfg: BTreeMap<u32, u32>,
    emit: &mut dyn FnMut(&str, Found),
) {
    let mut best: Option<(u8, u32)> = None;
    for (&st, &w) in &cfg {
        if ctx.g.accepting[st as usize] {
            let u = ctx.g.key(st)[1] as u8;
            best = Some(match best {
                None => (u, w),
                Some((bu, bw)) => (bu.min(u), bw.min(w)),
            });
        }
    }
    if let Some((dist, weight)) = best {
        if s != ctx.typed && unit_ok(ctx, s) {
            emit(s, Found { dist, weight });
        }
    }
    let mut next: BTreeMap<char, BTreeMap<u32, u32>> = BTreeMap::new();
    for (&st, &w) in &cfg {
        for (dst, aw, tags) in &ctx.by_src[st as usize] {
            for &c in tags {
                let m = next.entry(c).or_default();
                let nw = w + aw;
                if m.get(dst).is_none_or(|&old| nw < old) {
                    m.insert(*dst, nw);
                }
            }
        }
    }
    for (c, m) in next {
        let nrun = if super::units::is_mark(c) { run + 1 } else { 0 };
        if nrun > ctx.bound {
            continue;
        }
        if nrun > ctx.k && !deep_prefix_ok(ctx, s, c) {
            continue;
        }
        let m = closure(ctx.by_src, m);
        s.push(c);
        dfs(ctx, s, nrun, m, emit);
        s.pop();
    }
}

fn nfd(s: &str) -> String {
    nfd_string(s)
}

// ---------------------------------------------------------------- Fold and the generated table

#[test]
fn casefold_table_covers_the_full_foldings() {
    assert_eq!(casefold_char('\u{DF}'), Some("ss"));
    assert_eq!(casefold_char('\u{1E9E}'), Some("ss"));
    assert_eq!(casefold_char('\u{3C2}'), Some("\u{3C3}"));
    assert_eq!(casefold_char('\u{17F}'), Some("s"));
    assert_eq!(casefold_char('\u{212A}'), Some("k"));
    assert_eq!(casefold_char('A'), Some("a"));
    assert_eq!(casefold_char('a'), None);
    // Cherokee small letters fold to the capitals (U+AB70..U+ABBF to U+13A0..U+13EF).
    for i in 0..0x50u32 {
        let small = char::from_u32(0xAB70 + i).unwrap();
        let cap = char::from_u32(0x13A0 + i).unwrap().to_string();
        assert_eq!(
            casefold_char(small),
            Some(cap.as_str()),
            "U+{:X}",
            0xAB70 + i
        );
    }
    assert_eq!(fold_base("\u{DF}"), "ss");
    assert_ne!(fold_base("\u{DF}"), "s");
    // Sorted and without duplicates, so the binary search is sound.
    assert!(CASEFOLD.windows(2).all(|w| w[0].0 < w[1].0));
}

#[test]
fn casefold_table_header_states_its_unicode_version() {
    let src = include_str!("casefold_table.rs");
    let header: Vec<&str> = src.lines().take(4).collect();
    let line = header
        .iter()
        .find(|l| l.contains("unidata_version"))
        .expect("header names unicodedata.unidata_version");
    assert!(
        line.contains(&format!("unidata_version = {CASEFOLD_UNICODE_VERSION}.")),
        "header {line:?} and constant {CASEFOLD_UNICODE_VERSION:?} disagree"
    );
    assert!(src.contains("//! @generated by rust/tools/cascade/gen_casefold_table.py"));
}

#[test]
fn casefold_unicode_version_is_pinned_to_the_reference() {
    // Known limit: Unicode 17 additions are not folded; the Python reference (3.14.8) is 16.0.0 too.
    assert_eq!(CASEFOLD_UNICODE_VERSION, "16.0.0");
}

#[test]
fn casefold_table_is_not_newer_than_the_normalisation_crate() {
    // A table from a newer Unicode than the crate's NFD would fold characters NFD cannot decompose.
    let table: Vec<u8> = CASEFOLD_UNICODE_VERSION
        .split('.')
        .map(|x| x.parse().unwrap())
        .collect();
    let (a, b, c) = unicode_normalization::UNICODE_VERSION;
    assert!((table[0], table[1], table[2]) <= (a, b, c));
}

#[test]
fn fold_keeps_one_unit_and_the_marks() {
    // C141: a base that folds to two characters is still ONE unit, and the marks are kept.
    let h = Harness::new(&["\u{DF}", "s", "\u{301}"], "\u{DF}\u{301}");
    let t = h.model.typed_tokens();
    assert_eq!(t.len(), 1);
    let u = h.model.unit_tok(t[0]);
    assert_eq!(h.model.base_str(u.base), "ss");
    assert!(matches!(u.stack, Stack::Node(_)));
}

// ---------------------------------------------------------------- weight table (C121)

#[test]
fn weight_table_is_validated_at_load() {
    assert_eq!(parse_thousandths("w", "0.8"), Ok(800));
    assert_eq!(parse_thousandths("w", "1"), Ok(1000));
    assert_eq!(parse_thousandths("w", "0.500"), Ok(500));
    assert_eq!(parse_thousandths("w", "0.5000"), Ok(500));
    for bad in [
        "-1",
        "+1",
        "nan",
        "inf",
        "1e3",
        "0.0005",
        "0.5005",
        "",
        ".5",
        "1.",
        "0x10",
        "1,5",
        "99999999999",
    ] {
        assert!(
            parse_thousandths("w", bad).is_err(),
            "{bad:?} must be refused"
        );
    }
    let ok = "# defaults\nsub = 1.0\nkb = 0.8\ndia = 0.5\ndel = 1.0\nins = 1.0\ntrans = 0.9\n";
    assert_eq!(ErrWeights::from_table(ok), Ok(ErrWeights::default()));
    assert!(
        ErrWeights::from_table("sub = 1.0\n").is_err(),
        "missing entries"
    );
    assert!(
        ErrWeights::from_table(&format!("{ok}sub = 2.0\n")).is_err(),
        "duplicate"
    );
    assert!(
        ErrWeights::from_table(&format!("{ok}case = 1.0\n")).is_err(),
        "unknown name"
    );
    assert!(ErrWeights::from_table(&ok.replace("0.8", "-0.8")).is_err());
    assert!(ErrWeights::from_table(&ok.replace("0.8", "0.0008")).is_err());
}

// ---------------------------------------------------------------- Units

#[test]
fn units_cut_by_character_class_not_by_code_point() {
    // C84, C106: a base and its marks are one unit; the typed word is NFD first.
    let h = Harness::new(&["\u{E9}", "e", "t", "\u{301}"], "t\u{E9}");
    assert_eq!(h.model.typed_tokens().len(), 2);
    let lead = Harness::new(&["e", "\u{301}"], "\u{301}\u{301}e");
    assert_eq!(
        lead.model.typed_tokens().len(),
        2,
        "word-initial marks are one unit"
    );
    assert_eq!(
        cut_units("\u{301}\u{300}e"),
        vec![
            ("".to_string(), "\u{301}\u{300}".to_string()),
            ("e".to_string(), "".to_string())
        ]
    );
}

#[test]
fn cut_units_decomposes_before_cutting_c106() {
    assert_eq!(
        cut_units("\u{E9}"),
        vec![("e".to_string(), "\u{301}".to_string())]
    );
}

#[test]
fn nfc_and_nfd_typed_words_are_the_same_word_c106() {
    let reps = ["e", "t", "\u{301}"];
    let nfc = Harness::new(&reps, "t\u{E9}");
    let nfd_ = Harness::new(&reps, "te\u{301}");
    let a = nfc.strings(2, 2, &[]);
    assert_eq!(a, nfd_.strings(2, 2, &[]));
    assert_eq!(
        a["te"],
        Found {
            dist: 1,
            weight: 500
        }
    );
}

#[test]
fn nine_marks_typed_exactly_is_a_word_with_no_edit_c130_c147() {
    let nine: String = "e".to_string() + &[M1, M2, M3].repeat(3).iter().collect::<String>();
    let h = Harness::new(&["e", "\u{301}", "\u{300}", "\u{302}"], &nine);
    let (g, _) = h.graph(0, &[], false);
    assert!(
        g.start.is_some(),
        "the exactly typed nine-mark word is accepted with 0 edits"
    );
    assert!(g.accepting.iter().any(|&a| a));
    // And with the pre-pass over `Units | Fold | Err1 | Err2` (the same abstraction in both passes).
    let (g2, rep) = h.graph(0, &[1], false);
    assert!(g2.start.is_some());
    assert_eq!(rep.prepass, PrepassUsed::Suffix(1));
    // A typed prefix of the stack is a different stack: one whole-stack edit, never the typed word.
    let eight: String = nfd(&nine).chars().take(9).collect();
    let s = h.strings(1, 8, &[]);
    assert_eq!(
        s[&eight],
        Found {
            dist: 1,
            weight: 500
        }
    );
}

#[test]
fn whole_stack_dia_is_one_edit_of_half_weight_c113_c148() {
    let reps = ["e", "\u{301}", "\u{300}", "\u{302}"];
    // typed `e`, candidate `e` + two marks (the stack leaves every typed prefix: `other`)
    let h = Harness::new(&reps, "e");
    let s = h.strings(2, 2, &[]);
    let two = nfd(&format!("e{M2}{M3}"));
    assert_eq!(
        s[&two],
        Found {
            dist: 1,
            weight: 500
        },
        "two missing marks cost 1 edit, 0.5"
    );
    // typed `e` + 2 marks, candidate `e`
    let h = Harness::new(&reps, &format!("e{M1}{M2}"));
    let s = h.strings(2, 2, &[]);
    assert_eq!(
        s["e"],
        Found {
            dist: 1,
            weight: 500
        }
    );
    // an `other` stack is NOT equal to a typed stack (C148, second half): typed e+M1, candidate e+M2+M3
    let h = Harness::new(&reps, &format!("e{M1}"));
    let s = h.strings(2, 2, &[]);
    assert_eq!(s[&nfd(&format!("e{M2}{M3}"))].dist, 1);
    assert_eq!(
        s[&nfd(&format!("e{M1}{M2}"))].dist,
        1,
        "a typed prefix then another mark is a different unit"
    );
}

#[test]
fn marks_are_not_reordered_c131() {
    let reps = ["e", "\u{323}", "\u{301}"]; // classes 220 and 230
    let h = Harness::new(&reps, "e\u{323}\u{301}");
    let s = h.strings(0, 2, &[]);
    assert!(
        s.is_empty(),
        "E = 0: only the typed string itself, which is excluded: {s:?}"
    );
    let s = h.strings(1, 2, &[]);
    assert!(
        !s.contains_key("e\u{301}\u{323}"),
        "the out-of-order concatenation is not a string of the model"
    );
    assert!(s.contains_key("e\u{323}"));
    // the rep `e` followed by a class-0 mark between two others is its own barrier
    let reps = ["e", "\u{301}", "\u{323}", "\u{20DD}"]; // 230, 220, class 0
    let h = Harness::new(&reps, "e");
    let s = h.strings(1, 3, &[]);
    assert!(
        s.contains_key("e\u{301}\u{20DD}\u{323}"),
        "230, 0, 220 is canonical"
    );
    assert!(!s.contains_key("e\u{301}\u{323}"), "230 then 220 is cut");
    assert!(
        s.contains_key("e\u{323}\u{301}"),
        "220 then 230 is the canonical order"
    );
}

#[test]
fn leading_marks_are_one_empty_base_unit_costing_dia() {
    let h = Harness::new(&["e", "\u{301}"], "e");
    let s = h.strings(1, 1, &[]);
    assert_eq!(
        s["\u{301}e"],
        Found {
            dist: 1,
            weight: 500
        },
        "insert of an empty-base unit is a dia"
    );
    assert_eq!(
        s["ee"],
        Found {
            dist: 1,
            weight: 1000
        }
    );
    // the mark cannot start a unit in the middle of a word
    assert!(s.keys().all(|k| !k.starts_with("e\u{301}\u{301}")));
}

// ---------------------------------------------------------------- Fold and the error layers: named cases

#[test]
fn fold_unit_without_a_character_definition_folds_c139() {
    // No definition names `T` plus a mark, yet typed `t` plus a mark reaches it at distance 0.
    let h = Harness::new(&["t", "T", "\u{301}"], "t\u{301}");
    let s = h.strings(0, 2, &[]);
    assert_eq!(s["T\u{301}"], Found { dist: 0, weight: 0 });
    let h = Harness::new(&["t", "T", "\u{301}"], "T\u{301}");
    let s = h.strings(0, 2, &[]);
    assert_eq!(s["t\u{301}"], Found { dist: 0, weight: 0 });
}

#[test]
fn named_cases_ab_xba_and_xirabu_kitabu_c140() {
    let h = Harness::new(&["a", "b", "x", "A", "B"], "ab");
    let s = h.strings(2, 1, &[]);
    assert_eq!(
        s["xBA"].dist, 2,
        "insertion, swap, case on the swapped units"
    );
    assert!(!h.strings(1, 1, &[]).contains_key("xBA"));
    let h = Harness::new(&["k", "i", "t", "a", "b", "u"], "XIRABU");
    let s = h.strings(2, 1, &[]);
    assert_eq!(s["kitabu"].dist, 2, "two substitutions; case after both");
    // C140, second half: `Fold` must still apply once an error layer has left `clean`.
    let h = Harness::new(&["a", "b", "c", "A", "B", "C"], "Ab");
    let s = h.strings(2, 1, &[]);
    assert_eq!(
        s["Bca"].dist, 2,
        "trans then ins, case differs on every unit"
    );
}

#[test]
fn sharp_s_is_one_unit_and_not_two_c141() {
    let h = Harness::new(&["\u{DF}", "s"], "ss");
    let s = h.strings(2, 1, &[]);
    assert_eq!(
        s["\u{DF}"].dist, 2,
        "one sub and one del: the unit ss is not the two units s s"
    );
    let h = Harness::new(&["\u{DF}", "s", "x"], "\u{DF}");
    let s = h.strings(2, 1, &[]);
    assert_eq!(s["ss"].dist, 2);
    assert_eq!(s["x"].dist, 1);
}

#[test]
fn weights_and_edit_counts_of_each_operation() {
    let reps = ["t", "r", "e", "a", "b", "\u{301}"];
    let h = Harness::new(&reps, "t");
    let s = h.strings(1, 1, &[]);
    assert_eq!(
        s["r"],
        Found {
            dist: 1,
            weight: 800
        },
        "kb"
    );
    assert_eq!(
        s["e"],
        Found {
            dist: 1,
            weight: 1000
        },
        "sub"
    );
    assert_eq!(
        s["t\u{301}"],
        Found {
            dist: 1,
            weight: 500
        },
        "dia"
    );
    assert_eq!(
        s["te"],
        Found {
            dist: 1,
            weight: 1000
        },
        "ins"
    );
    let h = Harness::new(&reps, "ab");
    let s = h.strings(1, 1, &[]);
    assert_eq!(
        s["ba"],
        Found {
            dist: 1,
            weight: 900
        },
        "trans"
    );
    assert_eq!(
        s["a"],
        Found {
            dist: 1,
            weight: 1000
        },
        "del"
    );
    let h = Harness::new(&reps, "aa");
    assert!(
        !h.strings(2, 1, &[]).contains_key("aa"),
        "a swap of equal units is not an edit"
    );
}

#[test]
fn err2_may_edit_what_err1_wrote_c83() {
    let h = Harness::new(&["a", "b", "x"], "ab");
    let s2 = h.strings(2, 1, &[]);
    assert_eq!(s2["bxa"].dist, 2);
    assert!(!h.strings(1, 1, &[]).contains_key("bxa"));
    // C80, C81, C82: swap arcs exist, an insertion after the last unit exists, Err2 is live.
    assert_eq!(h.strings(1, 1, &[])["ba"].dist, 1);
    assert_eq!(h.strings(1, 1, &[])["abx"].dist, 1);
    assert_eq!(s2["xxab"].dist, 2);
}

#[test]
fn edit_limit_bounds_the_ball() {
    let h = Harness::new(&["a", "b", "c", "x"], "ab");
    for e in 0..3u8 {
        for f in h.strings(e, 1, &[]).values() {
            assert!(f.dist <= e);
        }
    }
    assert!(h.strings(0, 1, &[]).is_empty());
}

#[test]
fn prepass_cuts_give_the_same_strings() {
    let reps = ["e", "t", "T", "\u{301}", "\u{300}"];
    let h = Harness::new(&reps, "te");
    let base = h.strings(2, 2, &[]);
    assert!(base.len() > 100);
    for cuts in [&[1usize][..], &[3], &[2], &[4], &[1, 3]] {
        let (g, rep) = h.graph(2, cuts, false);
        assert_ne!(rep.prepass, PrepassUsed::None, "cuts {cuts:?}");
        assert_eq!(h.read_strings(&g, 2), base, "cuts {cuts:?}");
    }
    // C124/C151 shape: an overrun pre-pass falls back and the answer is unchanged.
    let (g, rep) = h.graph(2, &[1, 3], true);
    assert_eq!(rep.prepass, PrepassUsed::None);
    assert!(rep.prepass_attempts_overrun >= 1);
    assert_eq!(h.read_strings(&g, 2), base);
}

#[test]
fn prepass_is_not_charged_to_the_word_budget_c151() {
    // Three same-class mark-only segments on one base; the word budget is exactly what the main pass alone needs.
    let reps = ["e", "\u{301}", "\u{300}", "\u{302}"];
    let h = Harness::new(&reps, "e\u{301}\u{300}\u{302}");
    let (_, plain) = h.graph(1, &[], false);
    let budget = WordBudget {
        states: plain.states,
        arcs: plain.arcs,
        steps: plain.steps,
        bytes: plain.bytes,
    };
    let (_, with) = h.graph_with(1, &[1], false, budget);
    assert_ne!(with.prepass, PrepassUsed::None);
    assert!(with.prepass_bytes > 0 && with.prepass_states > 0);
}

// ---------------------------------------------------------------- KC-S1

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn unhex(s: &str) -> String {
    s.split_whitespace()
        .map(|h| char::from_u32(u32::from_str_radix(h, 16).unwrap()).unwrap())
        .collect()
}

fn hex(s: &str) -> String {
    s.chars()
        .map(|c| format!("{:04X}", c as u32))
        .collect::<Vec<_>>()
        .join(" ")
}

struct KcSet {
    tier: String,
    k: usize,
    n: u64,
    xor: u64,
    sum: u64,
    typed: String,
    rows: Vec<String>,
}

fn load_fixture() -> (Vec<String>, Vec<KcSet>) {
    let text = include_str!("fixtures/kcs1.txt");
    let mut reps = Vec::new();
    let mut sets: Vec<KcSet> = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(r) = line
            .strip_prefix("seg ")
            .or_else(|| line.strip_prefix("bnd "))
        {
            reps.push(unhex(r));
        } else if let Some(r) = line.strip_prefix("row ") {
            sets.last_mut().unwrap().rows.push(r.to_string());
        } else if let Some(r) = line.strip_prefix("set ") {
            let f: Vec<&str> = r.splitn(7, ' ').collect();
            let get = |i: usize, p: &str| f[i].strip_prefix(p).unwrap().to_string();
            sets.push(KcSet {
                tier: f[0].to_string(),
                k: get(1, "k=").parse().unwrap(),
                n: get(2, "n=").parse().unwrap(),
                xor: u64::from_str_radix(&get(3, "xor="), 16).unwrap(),
                sum: u64::from_str_radix(&get(4, "sum="), 16).unwrap(),
                typed: unhex(f[6].strip_prefix("typed=").unwrap()),
                rows: Vec::new(),
            });
        } else {
            panic!("fixture line {line:?}");
        }
    }
    (reps, sets)
}

/// The row key the generator writes: dist and, for dist <= 1, the weight.
fn row_key(s: &str, f: &Found) -> String {
    let w = if f.dist <= 1 {
        f.weight.to_string()
    } else {
        "-".to_string()
    };
    format!("{}|{}|{}", hex(s), f.dist, w)
}

fn check_set(reps: &[String], set: &KcSet) {
    let refs: Vec<&str> = reps.iter().map(String::as_str).collect();
    let h = Harness::new(&refs, &set.typed);
    let (g, _) = h.graph(2, &[1], false);
    let (mut n, mut x, mut sm) = (0u64, 0u64, 0u64);
    let mut keys = BTreeSet::new();
    let keep = !set.rows.is_empty();
    h.walk(&g, set.k, &mut |s, f| {
        let key = row_key(s, &f);
        let hh = fnv(&key);
        n += 1;
        x ^= hh;
        sm = sm.wrapping_add(hh);
        if keep {
            keys.insert(key);
        }
    });
    if (n, x, sm) != (set.n, set.xor, set.sum) {
        let want: BTreeSet<String> = set.rows.iter().cloned().collect();
        let missing: Vec<&String> = want.difference(&keys).take(5).collect();
        let extra: Vec<&String> = keys.difference(&want).take(5).collect();
        panic!(
            "KC-S1 {} k={} typed {:?}: model has {} strings, generator {}; digest {:016x}/{:016x} vs {:016x}/{:016x}; \
             (rows only for some sets) missing from model {:?}, extra in model {:?}",
            set.tier, set.k, set.typed, n, set.n, x, sm, set.xor, set.sum, missing, extra
        );
    }
    if !set.rows.is_empty() {
        let want: BTreeSet<String> = set.rows.iter().cloned().collect();
        assert_eq!(keys, want);
    }
}

/// Both directions as a digest over (string, dist, weight for dist <= 1); the fast tier runs by default.
#[test]
fn kcs1_model_equals_the_generator_fast_tier() {
    let (reps, sets) = load_fixture();
    let mut ran = 0;
    for set in sets.iter().filter(|s| s.n <= 90_000) {
        check_set(&reps, set);
        ran += 1;
    }
    assert!(ran >= 50, "only {ran} sets ran");
}

/// The whole fixture: 50 words, each re-cased three ways, k = 2 and 3. Run with `-- --ignored`.
#[test]
#[ignore = "full KC-S1 gate; minutes"]
fn kcs1_model_equals_the_generator_full() {
    let (reps, sets) = load_fixture();
    for set in &sets {
        check_set(&reps, set);
    }
}
