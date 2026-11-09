//! Builds the admission transducer: tagged acceptor, composed with the phonology cascade and orthography.

use super::{AdmissionEvidence, AdmissionPlan, AdmissionRefusal, Composition, Size};
use crate::replace::{self, SegAlphabet};
use foma::constructions::{
    fsm_compose, fsm_concat, fsm_count, fsm_cross_product, fsm_intersect, fsm_kleene_star,
    fsm_symbol, fsm_union, fsm_universal,
};
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::structures::fsm_empty_string;
use foma::types::Fsm;
use foma::utf8::is_combining;
use pg_grammar::chardef::{CharDefKind, CharDefTable};
use pg_grammar::model::{Grammar, PhonRuleDef};
use std::collections::HashMap;

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

pub(super) struct Machine {
    pub net: Fsm,
    pub tags: HashMap<String, usize>,
    pub free: Vec<bool>,
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
fn orthography(opts: &FomaOptions, table: &CharDefTable) -> Fsm {
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
    let relate = |tokens: &str, lower: &[String]| {
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
    union_all(opts, arcs)
        .map(|net| fsm_kleene_star(opts, net))
        .unwrap_or_else(fsm_empty_string)
}

pub(super) fn cascade(
    opts: &FomaOptions,
    g: &Grammar,
    evidence: &mut AdmissionEvidence,
    meter: &mut Meter,
) -> Result<(Fsm, Vec<String>), AdmissionRefusal> {
    let rules: Vec<&PhonRuleDef> = g
        .strata
        .iter()
        .flat_map(|stratum| &stratum.prules)
        .map(|id| &g.prules[id.0 as usize])
        .collect();
    let items: Vec<String> = rules
        .iter()
        .map(|rule| match rule {
            PhonRuleDef::Rewrite(r) => r.xml_id.clone(),
            PhonRuleDef::Metathesis(r) => r.xml_id.clone(),
        })
        .collect();
    let net = if rules.is_empty() {
        let mut net = identity_cascade(opts, &g.char_tables[0]);
        fsm_count(&mut net);
        net
    } else {
        let (net, detail) = replace::compile_proposing_cascade(opts, g, &rules, &mut Vec::new())
            .map_err(AdmissionRefusal::Cascade)?;
        evidence.cascade_detail = Some(detail);
        net
    };
    evidence.cascade = size(&net);
    meter.observe("cascade", evidence.cascade, &items)?;
    Ok((net, items))
}

fn compose_min(
    opts: &FomaOptions,
    structure: &str,
    (left, right): (Fsm, Fsm),
    items: &[String],
    evidence: &mut AdmissionEvidence,
    meter: &mut Meter,
) -> Result<Fsm, AdmissionRefusal> {
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
    });
    Ok(net)
}

/// `?* [ piece TAG | .. ] ?*` restricted to one tag per string, as an acceptor.
fn tagged_acceptor(
    opts: &FomaOptions,
    plan: &AdmissionPlan,
    tag_of: &HashMap<usize, String>,
    tokens: &Fsm,
    tags: &Fsm,
) -> Result<Fsm, String> {
    let mut alternatives = Vec::new();
    for piece in &plan.pieces {
        let Some(tag) = tag_of.get(&(piece.rule.0 as usize)) else {
            continue;
        };
        let Some(net) = fsm_parse_regex(opts, &piece.regex, None, None) else {
            return Err(piece.grammar_item.clone());
        };
        alternatives.push(fsm_concat(opts, net, fsm_symbol(tag)));
    }
    let Some(any_piece) = union_all(opts, alternatives) else {
        return Err("no pieces".to_owned());
    };
    let around = fsm_concat(
        opts,
        fsm_universal(),
        fsm_concat(opts, any_piece, fsm_universal()),
    );
    let one_tag = fsm_concat(
        opts,
        fsm_kleene_star(opts, tokens.clone()),
        fsm_concat(opts, tags.clone(), fsm_kleene_star(opts, tokens.clone())),
    );
    Ok(fsm_minimize(opts, fsm_intersect(opts, around, one_tag)))
}

/// Pieces whose regex does not compile make their rule free instead of failing the build.
fn compile_tagged(
    opts: &FomaOptions,
    plan: &AdmissionPlan,
    tag_of: &mut HashMap<usize, String>,
    tokens: &Fsm,
    meter: &mut Meter,
) -> Option<(Fsm, Fsm, Vec<bool>)> {
    let mut free = plan.free.clone();
    loop {
        let tags = symbols_union(opts, tag_of.values().cloned())?;
        match tagged_acceptor(opts, plan, tag_of, tokens, &tags) {
            Ok(net) => return Some((net, tags, free)),
            Err(item) if item == "no pieces" => return None,
            Err(item) => {
                meter.warnings.push(format!(
                    "piece for {item} did not compile; the rule is admitted for every word."
                ));
                for piece in plan.pieces.iter().filter(|p| p.grammar_item == item) {
                    tag_of.remove(&(piece.rule.0 as usize));
                    free[piece.rule.0 as usize] = true;
                }
            }
        }
    }
}

pub(super) fn machine(
    opts: &FomaOptions,
    g: &Grammar,
    plan: &AdmissionPlan,
    evidence: &mut AdmissionEvidence,
    meter: &mut Meter,
) -> Result<Machine, AdmissionRefusal> {
    let table = &g.char_tables[0];
    let mut tag_of: HashMap<usize, String> = plan
        .pieces
        .iter()
        .map(|p| {
            let rule = p.rule.0 as usize;
            (rule, format!("<A:{rule}>"))
        })
        .collect();
    let all_tokens = tokens_of(table, CharDefKind::Segment)
        .into_iter()
        .chain(tokens_of(table, CharDefKind::Boundary));
    let tokens = symbols_union(opts, all_tokens.map(String::from))
        .ok_or_else(|| AdmissionRefusal::Construction("the character table is empty".into()))?;
    let items: Vec<String> = plan.pieces.iter().map(|p| p.grammar_item.clone()).collect();
    let Some((acceptor, tags, free)) = compile_tagged(opts, plan, &mut tag_of, &tokens, meter)
    else {
        return Err(AdmissionRefusal::NoUsablePiece(plan.counts.affix_rules));
    };
    evidence.acceptor = size(&acceptor);
    meter.observe("tagged acceptor", evidence.acceptor, &items)?;
    let tags_up = fsm_kleene_star(
        opts,
        union_all(
            opts,
            vec![
                fsm_cross_product(opts, fsm_empty_string(), tokens.clone()),
                tags.clone(),
            ],
        )
        .expect("two alternatives"),
    );
    let tags_down = fsm_kleene_star(
        opts,
        union_all(
            opts,
            vec![
                tokens.clone(),
                fsm_cross_product(opts, tags.clone(), fsm_empty_string()),
            ],
        )
        .expect("two alternatives"),
    );
    let upper = compose_min(
        opts,
        "tagged acceptor lifted to tag-only upper side",
        (tags_up, acceptor),
        &items,
        evidence,
        meter,
    )?;
    let net = compose_min(
        opts,
        "tag-to-tokens transducer",
        (upper, tags_down),
        &items,
        evidence,
        meter,
    )?;
    let by_text = tag_of
        .iter()
        .map(|(rule, text)| (text.clone(), *rule))
        .collect();
    Ok(Machine {
        net,
        tags: by_text,
        free,
    })
}

pub(super) fn finish(
    opts: &FomaOptions,
    g: &Grammar,
    tagged: Fsm,
    cascade: Fsm,
    items: &[String],
    evidence: &mut AdmissionEvidence,
    meter: &mut Meter,
) -> Result<Fsm, AdmissionRefusal> {
    let through = compose_min(
        opts,
        "tagged transducer composed with the cascade",
        (tagged, cascade),
        items,
        evidence,
        meter,
    )?;
    let spelling = orthography(opts, &g.char_tables[0]);
    let net = compose_min(
        opts,
        "composition with the orthography",
        (through, spelling),
        items,
        evidence,
        meter,
    )?;
    evidence.final_net = size(&net);
    Ok(net)
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
