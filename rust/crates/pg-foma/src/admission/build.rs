//! Builds the admission transducer: tagged acceptor, composed with the phonology cascade and orthography.

use super::meter::{size, size_sum, Meter};
use super::{AdmissionEvidence, AdmissionPlan, AdmissionRefusal, Composition, Piece, RulePartSize};
use crate::cascade_parts::{CascadeParts, CascadeShare, UpperTokens};
use crate::replace::SegAlphabet;
use foma::constructions::fsm_count;
use foma::options::FomaOptions;
use foma::structures::fsm_empty_string;
use foma::types::Fsm;
use foma::utf8::is_combining;
use pg_grammar::chardef::{CharDefKind, CharDefTable};
use pg_grammar::model::{Grammar, PhonRuleDef};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

pub(super) fn union_all(meter: &Meter, opts: &FomaOptions, mut nets: Vec<Fsm>) -> Option<Fsm> {
    while nets.len() > 1 {
        let mut next = Vec::with_capacity(nets.len() / 2 + 1);
        let mut iter = nets.into_iter();
        while let Some(first) = iter.next() {
            next.push(match iter.next() {
                Some(second) => meter.union(opts, first, second),
                None => first,
            });
        }
        nets = next;
    }
    nets.pop()
}

pub(super) fn symbols_union(
    meter: &Meter,
    opts: &FomaOptions,
    symbols: impl IntoIterator<Item = String>,
) -> Option<Fsm> {
    union_all(
        meter,
        opts,
        symbols.into_iter().map(|s| meter.symbol(&s)).collect(),
    )
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

fn symbols_net(meter: &Meter, opts: &FomaOptions, symbols: &[String]) -> Fsm {
    symbols
        .iter()
        .map(|s| meter.symbol(s))
        .reduce(|left, right| meter.concat(opts, left, right))
        .unwrap_or_else(fsm_empty_string)
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
pub(super) fn identity_cascade(meter: &Meter, opts: &FomaOptions, table: &CharDefTable) -> Fsm {
    let segments = symbols_union(
        meter,
        opts,
        tokens_of(table, CharDefKind::Segment)
            .into_iter()
            .map(String::from),
    );
    let boundaries = tokens_of(table, CharDefKind::Boundary)
        .into_iter()
        .map(|c| meter.cross(opts, meter.symbol(&c.to_string()), fsm_empty_string()))
        .collect();
    let all = [segments, union_all(meter, opts, boundaries)]
        .into_iter()
        .flatten()
        .collect();
    union_all(meter, opts, all)
        .map(|net| meter.star(opts, net))
        .unwrap_or_else(fsm_empty_string)
}

/// Segment tokens become their NFD spellings, with a mark segment after a segment read as the one symbol foma's apply merges them into.
pub(super) fn orthography(
    meter: &Meter,
    opts: &FomaOptions,
    table: &CharDefTable,
) -> (Fsm, Vec<String>) {
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
        meter.cross(
            opts,
            symbols_net(meter, opts, &upper),
            symbols_net(meter, opts, lower),
        )
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
    let net = union_all(meter, opts, arcs)
        .map(|net| meter.star(opts, net))
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
    meter.check(size_sum(&left) + size_sum(&right))?;
    let mut raw = meter.compose(opts, left, right);
    fsm_count(&mut raw);
    let before = size(&raw);
    meter.observe(&format!("{structure} before minimizing"), before, items)?;
    meter.check(size_sum(&raw))?;
    let net = meter.minimize(opts, raw);
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
pub(super) fn on_large_stack<T: Send>(work: impl FnOnce() -> T + Send) -> T {
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
    meter: &Meter,
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
        let Some(net) = meter.regex(opts, &piece.regex) else {
            return Err(piece.grammar_item.clone());
        };
        let text = meter.star(opts, tokens.clone());
        let tail = meter.concat(opts, net, text.clone());
        let around = meter.concat(opts, text.clone(), tail);
        let strings = meter.intersect(opts, around, text);
        alternatives.push(meter.cross(opts, meter.symbol(tag), strings));
    }
    Ok(union_all(meter, opts, alternatives))
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
        let compiled =
            tagged_acceptor(meter, opts, &plan.pieces, tag_of, tokens).and_then(|inner| {
                tagged_acceptor(meter, opts, &plan.outer_pieces, tag_of, tokens)
                    .map(|outer| (inner, outer))
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
    pub cascade: Option<Arc<CascadeShare>>,
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
        self.meter.check(size_sum(&raw))?;
        let net = self.meter.minimize(self.opts, raw);
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
fn pad(meter: &Meter, opts: &FomaOptions, inner: Fsm, symbols: &[String]) -> Fsm {
    let any = symbols_union(meter, opts, symbols.iter().cloned())
        .map(|net| meter.star(opts, net))
        .unwrap_or_else(fsm_empty_string);
    let around = meter.cross(opts, fsm_empty_string(), any);
    let left = meter.concat(opts, around.clone(), inner);
    meter.concat(opts, left, around)
}

/// The segment tokens and the boundary tokens of the cascade alphabet.
pub(super) fn token_sets(g: &Grammar, parts: Option<&UpperTokens>) -> (Vec<char>, Vec<char>) {
    let table = &g.char_tables[0];
    match parts {
        Some(tokens) => {
            let segments = tokens
                .attested_segments
                .iter()
                .chain(&tokens.private_segments)
                .copied()
                .collect();
            (segments, tokens.boundaries.to_vec())
        }
        None => (
            tokens_of(table, CharDefKind::Segment).into_iter().collect(),
            tokens_of(table, CharDefKind::Boundary)
                .into_iter()
                .collect(),
        ),
    }
}

pub(super) fn token_alphabet(g: &Grammar, parts: Option<&UpperTokens>) -> Vec<char> {
    let table = &g.char_tables[0];
    match parts {
        Some(tokens) => [
            &tokens.attested_segments,
            &tokens.private_segments,
            &tokens.boundaries,
        ]
        .into_iter()
        .flatten()
        .copied()
        .collect(),
        None => tokens_of(table, CharDefKind::Segment)
            .into_iter()
            .chain(tokens_of(table, CharDefKind::Boundary))
            .collect(),
    }
}

fn rule_part_sizes(parts: &CascadeShare) -> Vec<RulePartSize> {
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
    let built = CascadeParts::build_within(opts, g, &prules, meter);
    meter.check(0)?;
    let parts = match built {
        Ok(parts) => Some(Arc::new(parts.into_share())),
        Err(_) if prules.is_empty() => None,
        Err(error) => return Err(AdmissionRefusal::Cascade(error)),
    };
    evidence.rule_parts = parts.as_deref().map(rule_part_sizes).unwrap_or_default();
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
        meter,
        opts,
        token_alphabet(g, parts.as_deref().map(|share| &share.tokens))
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
    let (spelled, lower_symbols) = orthography(meter, opts, &g.char_tables[0]);
    let (spelling, rules) = match &parts {
        Some(parts) => (Some(parts.spelling.clone()), parts.rules.clone()),
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
                let identity = identity_cascade(steps.meter, opts, &g.char_tables[0]);
                net = steps.compose("cascade", net, identity)?;
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
            Some(steps.minimize("padded inner", pad(steps.meter, opts, net, &lower_symbols))?)
        }
        other => other,
    };
    let net = match (inner, outer) {
        (Some(inner), None) => inner,
        (None, Some(outer)) => outer,
        (Some(padded), Some(outer)) => {
            steps.meter.check(size_sum(&padded) + size_sum(&outer))?;
            let both = steps.meter.union(opts, padded, outer);
            steps.minimize("inner and outer union", both)?
        }
        (None, None) => return Err(AdmissionRefusal::NoUsablePiece(plan.counts.affix_rules)),
    };
    steps.meter.check(0)?;
    steps.evidence.final_net = size(&net);
    let by_text = tag_of
        .iter()
        .map(|(rule, text)| (text.clone(), *rule))
        .collect();
    Ok(Built {
        net,
        tags: by_text,
        free: tagged.free,
        cascade: parts,
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
