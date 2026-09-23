//! Rendering a `TreeTraceSink` for `pangloss parse --trace`: an indented, diffable text tree and a nested-object JSON tree. Lives here, not in `pg-parse`/`pg-rules`, since both need `Grammar` to resolve names and render a `Word`'s shape as text -- display-only, grammar-aware work `pg-cli` already does elsewhere.

use pg_grammar::model::{Grammar, MorphRuleDef, PhonRuleDef};
use pg_rules::trace::{TraceHandle, TraceNode, TraceSource, TreeTraceSink};
use pg_rules::word::Word;

/// A morphological rule's display name (`<Name>`), falling back to a numeric id for a hand-built test grammar that never named it.
fn mrule_name(g: &Grammar, id: pg_grammar::model::MRuleId) -> String {
    let idx = id.0 as usize;
    let Some(rule) = g.mrules.get(idx) else {
        return format!("mrule#{idx}");
    };
    let name = match rule {
        MorphRuleDef::AffixProcess(d) => d.name.as_deref(),
        MorphRuleDef::Realizational(d) => d.name.as_deref(),
        MorphRuleDef::Compounding(d) => d.name.as_deref(),
    };
    name.map(str::to_string)
        .unwrap_or_else(|| format!("mrule#{idx}"))
}

fn prule_name(g: &Grammar, id: pg_grammar::model::PRuleId) -> String {
    let idx = id.0 as usize;
    let Some(rule) = g.prules.get(idx) else {
        return format!("prule#{idx}");
    };
    let name = match rule {
        PhonRuleDef::Rewrite(d) => d.name.as_deref(),
        PhonRuleDef::Metathesis(d) => d.name.as_deref(),
    };
    name.map(str::to_string)
        .unwrap_or_else(|| format!("prule#{idx}"))
}

fn stratum_name(g: &Grammar, id: pg_grammar::model::StratumId) -> String {
    g.strata
        .get(id.0 as usize)
        .and_then(|s| s.name.clone())
        .unwrap_or_else(|| format!("stratum#{}", id.0))
}

fn template_name(g: &Grammar, id: pg_grammar::model::TemplateId) -> String {
    g.templates
        .get(id.0 as usize)
        .and_then(|t| t.name.clone())
        .unwrap_or_else(|| format!("template#{}", id.0))
}

/// Renders a `Word`'s shape using its own `stratum`'s character table, so a mid-derivation word is rendered in the stratum that produced it, never forced through the surface one.
fn render_word_shape(g: &Grammar, w: &Word) -> String {
    let table = &g.char_tables[g.strata[w.stratum.0 as usize].table.0 as usize];
    pg_parse::surface::to_plain_string(table, &w.shape, false)
}

fn source_label(g: &Grammar, source: TraceSource) -> Option<String> {
    match source {
        TraceSource::Language | TraceSource::None => None,
        TraceSource::Stratum(id) => Some(stratum_name(g, id)),
        TraceSource::Template(id) => Some(template_name(g, id)),
        TraceSource::MorphRule(id) => Some(mrule_name(g, id)),
        TraceSource::PhonRule(id) => Some(prule_name(g, id)),
    }
}

/// The indented plain-text renderer.
pub fn render_text(g: &Grammar, sink: &TreeTraceSink, root: TraceHandle) -> String {
    let mut out = String::new();
    render_text_node(g, sink, root, 0, &mut out);
    out
}

fn render_text_node(
    g: &Grammar,
    sink: &TreeTraceSink,
    h: TraceHandle,
    depth: usize,
    out: &mut String,
) {
    let n: TraceNode = sink.node(h);
    for _ in 0..depth {
        out.push_str("  ");
    }
    out.push_str(&format!("{:?}", n.type_));
    if let Some(label) = source_label(g, n.source) {
        out.push_str(&format!(" \"{label}\""));
    }
    if let Some(si) = n.subrule_index {
        if si >= 0 {
            out.push_str(&format!(" subrule={si}"));
        }
    }
    if let Some(reason) = n.failure_reason {
        out.push_str(&format!("  [{reason:?}]"));
    }
    if let Some(w) = &n.output {
        out.push_str(&format!("  shape={}", render_word_shape(g, w)));
    } else if let Some(w) = &n.input {
        out.push_str(&format!("  input={}", render_word_shape(g, w)));
    }
    out.push('\n');
    for &c in &n.children {
        render_text_node(g, sink, c, depth + 1, out);
    }
}

/// Minimal hand-rolled JSON emission: `pg-cli` has no `serde` dependency, and adding one purely for this secondary, tooling-facing format is more than this needs.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub fn render_json(g: &Grammar, sink: &TreeTraceSink, root: TraceHandle) -> String {
    let mut out = String::new();
    render_json_node(g, sink, root, &mut out);
    out
}

fn render_json_node_fields(g: &Grammar, n: &TraceNode, out: &mut String) {
    out.push('{');
    out.push_str(&format!("\"type\":\"{:?}\"", n.type_));
    if let Some(label) = source_label(g, n.source) {
        out.push_str(&format!(",\"source\":\"{}\"", json_escape(&label)));
    }
    if let Some(si) = n.subrule_index {
        if si >= 0 {
            out.push_str(&format!(",\"subrule\":{si}"));
        }
    }
    if let Some(reason) = n.failure_reason {
        out.push_str(&format!(",\"failureReason\":\"{reason:?}\""));
    }
    if let Some(w) = &n.output {
        out.push_str(&format!(
            ",\"outputShape\":\"{}\"",
            json_escape(&render_word_shape(g, w))
        ));
    }
    if let Some(w) = &n.input {
        out.push_str(&format!(
            ",\"inputShape\":\"{}\"",
            json_escape(&render_word_shape(g, w))
        ));
    }
}

fn render_json_node(g: &Grammar, sink: &TreeTraceSink, h: TraceHandle, out: &mut String) {
    let n: TraceNode = sink.node(h);
    render_json_node_fields(g, &n, out);
    out.push_str(",\"children\":[");
    for (i, &c) in n.children.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        render_json_node(g, sink, c, out);
    }
    out.push_str("]}");
}

pub fn render_json_node_shallow(g: &Grammar, sink: &TreeTraceSink, h: TraceHandle) -> String {
    let n: TraceNode = sink.node(h);
    let mut out = String::new();
    render_json_node_fields(g, &n, &mut out);
    out.push_str(",\"children\":[]}");
    out
}

#[cfg(test)]
mod tests;
