//! Builds the admission transducer: tagged acceptor, composed with the phonology cascade and orthography.

use super::{
    AdmissionEvidence, AdmissionPlan, AdmissionRefusal, Composition, Piece, RulePartSize, Size,
};
use crate::cascade_parts::CascadeParts;
use crate::replace::SegAlphabet;
use foma::constructions::{
    fsm_compose, fsm_concat, fsm_count, fsm_cross_product, fsm_intersect, fsm_kleene_star,
    fsm_symbol, fsm_union,
};
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::structures::fsm_empty_string;
use foma::types::Fsm;
use foma::utf8::is_combining;
use pg_grammar::chardef::{CharDefKind, CharDefTable};
use pg_grammar::model::{Grammar, PhonRuleDef};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};

const WARN_BOUND: usize = 1_000_000;
const REFUSE_BOUND: usize = 50_000_000;
const ITEMS_SHOWN: usize = 12;

pub(super) fn size(net: &Fsm) -> Size {
    Size {
        states: net.statecount.max(0) as usize,
        arcs: net.arccount.max(0) as usize,
    }
}

fn union_all(opts: &FomaOptions, mut nets: Vec<Fsm>) -> Option<Fsm> {
    while nets.len() > 1 {
        let mut next = Vec::with_capacity(nets.len() / 2 + 1);
        let mut iter = nets.into_iter();
        while let Some(first) = iter.next() {
            next.push(match iter.next() {
                Some(second) => fsm_union(opts, first, second),
                None => first,
            });
        }
        nets = next;
    }
    nets.pop()
}

fn symbols_union(opts: &FomaOptions, symbols: impl IntoIterator<Item = String>) -> Option<Fsm> {
    union_all(opts, symbols.into_iter().map(|s| fsm_symbol(&s)).collect())
}

fn is_mark(c: char) -> bool {
    let mut buf = [0u8; 4];
    is_combining(c.encode_utf8(&mut buf).as_bytes()) != 0
}

/// Lower-side symbols of a spelling: each base character with the combining marks that follow it.
fn spelling_symbols(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in text.chars() {
        match out.last_mut() {
            Some(last) if is_mark(c) => last.push(c),
            _ => out.push(c.to_string()),
        }
    }
    out
}

fn symbols_net(opts: &FomaOptions, symbols: &[String]) -> Fsm {
    symbols
        .iter()
        .map(|s| fsm_symbol(s))
        .reduce(|left, right| fsm_concat(opts, left, right))
        .unwrap_or_else(fsm_empty_string)
}

/// Reports sizes against the warning and containment bounds.
#[derive(Default)]
pub(super) struct Meter {
    pub warnings: Vec<String>,
}

impl Meter {
    pub(super) fn observe(
        &mut self,
        structure: &str,
        size: Size,
        items: &[String],
    ) -> Result<(), AdmissionRefusal> {
        let total = size.states + size.arcs;
        if total > REFUSE_BOUND {
            return Err(AdmissionRefusal::Containment {
                structure: structure.to_owned(),
                size: total,
            });
        }
        if total > WARN_BOUND {
            let shown: Vec<&str> = items.iter().take(ITEMS_SHOWN).map(String::as_str).collect();
            let more = items.len().saturating_sub(ITEMS_SHOWN);
            let suffix = if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            };
            self.warnings.push(format!(
                "{structure}: observed {total} states + arcs, bound {WARN_BOUND}; grammar items: {}{suffix}. This is a sign that there may be a better representation, such as widening, factoring or filtering. Action: the build continued and used the structure as built.",
                shown.join(", ")
            ));
        }
        Ok(())
    }
}

fn tokens_of(table: &CharDefTable, kind: CharDefKind) -> Vec<char> {
    let alphabet = SegAlphabet::new(table);
    table
        .iter()
        .filter(|(_, def)| def.kind() == kind)
        .map(|(id, _)| alphabet.token(id))
        .collect()
}

/// Segment tokens copy through and boundary tokens vanish.
fn identity_cascade(opts: &FomaOptions, table: &CharDefTable) -> Fsm {
    let segments = symbols_union(
        opts,
        tokens_of(table, CharDefKind::Segment)
            .into_iter()
            .map(String::from),
    );
    let boundaries = tokens_of(table, CharDefKind::Boundary)
        .into_iter()
        .map(|c| fsm_cross_product(opts, fsm_symbol(&c.to_string()), fsm_empty_string()))
        .collect();
    let all = [segments, union_all(opts, boundaries)]
        .into_iter()
        .flatten()
        .collect();
    union_all(opts, all)
        .map(|net| fsm_kleene_star(opts, net))
        .unwrap_or_else(fsm_empty_string)
}

/// Segment tokens become their NFD spellings, with a mark segment after a segment read as the one symbol foma's apply merges them into.
fn orthography(opts: &FomaOptions, table: &CharDefTable) -> (Fsm, Vec<String>) {
    let alphabet = SegAlphabet::new(table);
    let reps: Vec<(char, Vec<String>)> = table
        .iter()
        .filter(|(_, def)| def.kind() == CharDefKind::Segment)
        .flat_map(|(id, def)| {
            let token = alphabet.token(id);
            def.representations_nfd()
                .iter()
                .map(move |rep| (token, spelling_symbols(rep)))
        })
        .collect();
    let marks: Vec<&(char, Vec<String>)> = reps
        .iter()
        .filter(|(_, syms)| {
            syms.first()
                .and_then(|s| s.chars().next())
                .is_some_and(is_mark)
        })
        .collect();
    let mut arcs: Vec<Fsm> = Vec::new();
    let lowers: RefCell<BTreeSet<String>> = RefCell::new(BTreeSet::new());
    let relate = |tokens: &str, lower: &[String]| {
        lowers.borrow_mut().extend(lower.iter().cloned());
        let upper: Vec<String> = tokens.chars().map(String::from).collect();
        fsm_cross_product(opts, symbols_net(opts, &upper), symbols_net(opts, lower))
    };
    for (token, syms) in &reps {
        arcs.push(relate(&token.to_string(), syms));
        let Some((last, head)) = syms.split_last() else {
            continue;
        };
        for (mark, mark_syms) in &marks {
            let Some((lead, rest)) = mark_syms.split_first() else {
                continue;
            };
            let merged = |tail: &str| -> Vec<String> {
                let mut lower = head.to_vec();
                lower.push(format!("{last}{tail}"));
                lower.extend(rest.iter().cloned());
                lower
            };
            arcs.push(relate(&format!("{token}{mark}"), &merged(lead)));
            if !rest.is_empty() {
                continue;
            }
            for (second, second_syms) in &marks {
                let Some((second_lead, second_rest)) = second_syms.split_first() else {
                    continue;
                };
                let mut lower = head.to_vec();
                lower.push(format!("{last}{lead}{second_lead}"));
                lower.extend(second_rest.iter().cloned());
                arcs.push(relate(&format!("{token}{mark}{second}"), &lower));
            }
        }
    }
    let net = union_all(opts, arcs)
        .map(|net| fsm_kleene_star(opts, net))
        .unwrap_or_else(fsm_empty_string);
    (net, lowers.into_inner().into_iter().collect())
}

fn compose_min(
    opts: &FomaOptions,
    structure: &str,
    (left, right): (Fsm, Fsm),
    items: &[String],
    evidence: &mut AdmissionEvidence,
    meter: &mut Meter,
) -> Result<Fsm, AdmissionRefusal> {
    let started = std::time::Instant::now();
    let mut raw = fsm_compose(opts, left, right);
    fsm_count(&mut raw);
    let before = size(&raw);
    meter.observe(&format!("{structure} before minimizing"), before, items)?;
    let net = fsm_minimize(opts, raw);
    let after = size(&net);
    meter.observe(&format!("{structure} minimized"), after, items)?;
    evidence.compositions.push(Composition {
        structure: structure.to_owned(),
        before,
        after,
        ms: started.elapsed().as_millis() as u64,
    });
    Ok(net)
}

const LARGE_STACK_BYTES: usize = 1 << 30;

// Minimizing and composing large nets drops linked structures hundreds of thousands deep, so the build runs on its own stack.
fn on_large_stack<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(LARGE_STACK_BYTES)
            .spawn_scoped(scope, work)
            .expect("spawn the large-stack thread")
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

/// `?* [ piece TAG | .. ] ?*` restricted to one tag per string, as an acceptor; `None` when no piece has a tag.
fn tagged_acceptor(
    opts: &FomaOptions,
    pieces: &[Piece],
    tag_of: &HashMap<usize, String>,
    tokens: &Fsm,
) -> Result<Option<Fsm>, String> {
    let mut alternatives = Vec::new();
    for piece in pieces {
        let Some(tag) = tag_of.get(&(piece.rule.0 as usize)) else {
            continue;
        };
        let Some(net) = fsm_parse_regex(opts, &piece.regex, None, None) else {
            return Err(piece.grammar_item.clone());
        };
        let text = fsm_kleene_star(opts, tokens.clone());
        let around = fsm_concat(opts, text.clone(), fsm_concat(opts, net, text.clone()));
        let strings = fsm_intersect(opts, around, text);
        alternatives.push(fsm_cross_product(opts, fsm_symbol(tag), strings));
    }
    Ok(union_all(opts, alternatives))
}

/// The acceptors of the inner and outer regions, the tag alphabet and the free flags.
struct Tagged {
    inner: Option<Fsm>,
    outer: Option<Fsm>,
    free: Vec<bool>,
}

/// Pieces whose regex does not compile make their rule free instead of failing the build.
fn compile_tagged(
    opts: &FomaOptions,
    plan: &AdmissionPlan,
    tag_of: &mut HashMap<usize, String>,
    tokens: &Fsm,
    meter: &mut Meter,
) -> Option<Tagged> {
    let mut free = plan.free.clone();
    loop {
        let compiled = tagged_acceptor(opts, &plan.pieces, tag_of, tokens).and_then(|inner| {
            tagged_acceptor(opts, &plan.outer_pieces, tag_of, tokens).map(|outer| (inner, outer))
        });
        match compiled {
            Ok((None, None)) => return None,
            Ok((inner, outer)) => return Some(Tagged { inner, outer, free }),
            Err(item) => {
                meter.warnings.push(format!(
                    "piece for {item} did not compile; the rule is admitted for every word."
                ));
                let both = plan.pieces.iter().chain(&plan.outer_pieces);
                for piece in both.filter(|p| p.grammar_item == item) {
                    tag_of.remove(&(piece.rule.0 as usize));
                    free[piece.rule.0 as usize] = true;
                }
            }
        }
    }
}

/// The lookup net, its tag-to-rule map and the free rules.
pub(super) struct Built {
    pub net: Fsm,
    pub tags: HashMap<String, usize>,
    pub free: Vec<bool>,
}

pub(super) fn ordered_prules(g: &Grammar) -> Vec<&PhonRuleDef> {
    g.strata
        .iter()
        .flat_map(|stratum| &stratum.prules)
        .map(|id| &g.prules[id.0 as usize])
        .collect()
}

/// Everything the composition steps share.
struct Steps<'a> {
    opts: &'a FomaOptions,
    items: Vec<String>,
    evidence: &'a mut AdmissionEvidence,
    meter: &'a mut Meter,
}

impl Steps<'_> {
    fn compose(&mut self, structure: &str, left: Fsm, right: Fsm) -> Result<Fsm, AdmissionRefusal> {
        compose_min(
            self.opts,
            structure,
            (left, right),
            &self.items,
            self.evidence,
            self.meter,
        )
    }

    fn minimize(&mut self, structure: &str, net: Fsm) -> Result<Fsm, AdmissionRefusal> {
        let started = std::time::Instant::now();
        let mut raw = net;
        fsm_count(&mut raw);
        let before = size(&raw);
        self.meter.observe(
            &format!("{structure} before minimizing"),
            before,
            &self.items,
        )?;
        let net = fsm_minimize(self.opts, raw);
        let after = size(&net);
        self.meter
            .observe(&format!("{structure} minimized"), after, &self.items)?;
        self.evidence.compositions.push(Composition {
            structure: structure.to_owned(),
            before,
            after,
            ms: started.elapsed().as_millis() as u64,
        });
        Ok(net)
    }
}

/// Any text may surround the inner net's lower side.
fn pad(opts: &FomaOptions, inner: Fsm, symbols: &[String]) -> Fsm {
    let any = symbols_union(opts, symbols.iter().cloned())
        .map(|net| fsm_kleene_star(opts, net))
        .unwrap_or_else(fsm_empty_string);
    let around = fsm_cross_product(opts, fsm_empty_string(), any);
    fsm_concat(opts, fsm_concat(opts, around.clone(), inner), around)
}

pub(super) fn token_alphabet(g: &Grammar, parts: Option<&CascadeParts>) -> Vec<char> {
    let table = &g.char_tables[0];
    match parts {
        Some(parts) => {
            let tokens = &parts.tokens;
            [
                &tokens.attested_segments,
                &tokens.private_segments,
                &tokens.boundaries,
            ]
            .into_iter()
            .flatten()
            .copied()
            .collect()
        }
        None => tokens_of(table, CharDefKind::Segment)
            .into_iter()
            .chain(tokens_of(table, CharDefKind::Boundary))
            .collect(),
    }
}

fn rule_part_sizes(parts: &CascadeParts) -> Vec<RulePartSize> {
    parts
        .rules
        .iter()
        .map(|part| RulePartSize {
            rule: part.rule.clone(),
            construction: format!("{:?}", part.construction),
            size: size(&part.net),
            blocks: part.blocks,
            block_tuples: part.block_tuples,
            block_widened: part.block_widened,
        })
        .collect()
}

/// Builds the admission net: the inner region folded through the rules one at a time, the outer region through the spelling only.
pub(super) fn build_net(
    opts: &FomaOptions,
    g: &Grammar,
    plan: &AdmissionPlan,
    evidence: &mut AdmissionEvidence,
    meter: &mut Meter,
) -> Result<Built, AdmissionRefusal> {
    on_large_stack(|| build_on_this_stack(opts, g, plan, evidence, meter))
}

fn build_on_this_stack(
    opts: &FomaOptions,
    g: &Grammar,
    plan: &AdmissionPlan,
    evidence: &mut AdmissionEvidence,
    meter: &mut Meter,
) -> Result<Built, AdmissionRefusal> {
    let prules = ordered_prules(g);
    // Without phonological rules the table alone is a safe alphabet, so a failed build falls back to it.
    let parts = match CascadeParts::build(opts, g, &prules) {
        Ok(parts) => Some(parts),
        Err(_) if prules.is_empty() => None,
        Err(error) => return Err(AdmissionRefusal::Cascade(error)),
    };
    evidence.rule_parts = parts.as_ref().map(rule_part_sizes).unwrap_or_default();
    let items: Vec<String> = plan
        .pieces
        .iter()
        .chain(&plan.outer_pieces)
        .map(|p| p.grammar_item.clone())
        .collect();
    for part in &evidence.rule_parts {
        meter.observe(&format!("rule {}", part.rule), part.size, &items)?;
    }
    let tokens = symbols_union(
        opts,
        token_alphabet(g, parts.as_ref())
            .into_iter()
            .map(String::from),
    )
    .ok_or_else(|| AdmissionRefusal::Construction("the character table is empty".into()))?;
    let mut tag_of: HashMap<usize, String> = plan
        .pieces
        .iter()
        .chain(&plan.outer_pieces)
        .map(|p| {
            let rule = p.rule.0 as usize;
            (rule, format!("<A:{rule}>"))
        })
        .collect();
    let Some(tagged) = compile_tagged(opts, plan, &mut tag_of, &tokens, meter) else {
        return Err(AdmissionRefusal::NoUsablePiece(plan.counts.affix_rules));
    };
    evidence.acceptor = tagged.inner.as_ref().map(size).unwrap_or_default();
    evidence.outer_acceptor = tagged.outer.as_ref().map(size).unwrap_or_default();
    meter.observe("tagged acceptor", evidence.acceptor, &items)?;
    meter.observe("outer tagged acceptor", evidence.outer_acceptor, &items)?;
    let (spelled, lower_symbols) = orthography(opts, &g.char_tables[0]);
    let (spelling, rules) = match parts {
        Some(parts) => (Some(parts.spelling), parts.rules),
        None => (None, Vec::new()),
    };
    let mut steps = Steps {
        opts,
        items,
        evidence,
        meter,
    };
    let inner = match tagged.inner {
        None => None,
        Some(acceptor) => {
            let mut net = acceptor;
            if spelling.is_none() {
                net = steps.compose("cascade", net, identity_cascade(opts, &g.char_tables[0]))?;
            }
            for part in rules {
                net = steps.compose(&part.rule, net, part.net)?;
            }
            if let Some(spelling) = &spelling {
                net = steps.compose("spelling", net, spelling.clone())?;
            }
            Some(steps.compose("orthography", net, spelled.clone())?)
        }
    };
    let outer = match tagged.outer {
        None => None,
        Some(acceptor) => {
            let mut net = acceptor;
            if let Some(spelling) = spelling {
                net = steps.compose("outer spelling", net, spelling)?;
            }
            Some(steps.compose("outer orthography", net, spelled)?)
        }
    };
    // Outer rules, even free ones, may add text around what the inner net reads.
    let inner = match inner {
        Some(net) if plan.counts.outer_rules > 0 => {
            Some(steps.minimize("padded inner", pad(opts, net, &lower_symbols))?)
        }
        other => other,
    };
    let net = match (inner, outer) {
        (Some(inner), None) => inner,
        (None, Some(outer)) => outer,
        (Some(padded), Some(outer)) => {
            let both = fsm_union(opts, padded, outer);
            steps.minimize("inner and outer union", both)?
        }
        (None, None) => return Err(AdmissionRefusal::NoUsablePiece(plan.counts.affix_rules)),
    };
    steps.evidence.final_net = size(&net);
    let by_text = tag_of
        .iter()
        .map(|(rule, text)| (text.clone(), *rule))
        .collect();
    Ok(Built {
        net,
        tags: by_text,
        free: tagged.free,
    })
}
pub(super) struct CountingWriter(pub usize);

impl std::io::Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
