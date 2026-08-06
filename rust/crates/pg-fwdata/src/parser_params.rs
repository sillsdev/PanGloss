//! Parses the `<ParserParameters><HC>...</HC></ParserParameters>` XML blob FieldWorks stores as a string into a `ParserParameters` value, matching `HCLoader`'s constructor: `<HC>` may be entirely absent (e.g. an XAmple-configured project), `notOnClitics` then defaults true, and `<CompoundRules>` is a sibling of `<HC>`, not nested inside it.

use pg_snapshot::{CompoundRuleMaxApplications, ParserParameters};

use crate::node::parse_full_document;

/// `raw` is `Node::uni_text`'s already-unescaped `<Uni>` text; returns `ParserParameters::default()` if absent, empty, or unparsable XML, since this is user-hand-edited input, not worth a hard error over.
pub fn parse(raw: Option<&str>) -> ParserParameters {
    let Some(raw) = raw else {
        return ParserParameters::default();
    };
    let Some(root) = parse_full_document(raw) else {
        return ParserParameters::default();
    };
    // `root` is our synthetic document root; its first child should be `<ParserParameters>`.
    let Some(params_elem) = root.children.first() else {
        return ParserParameters::default();
    };
    let hc = params_elem.child("HC");

    let not_on_clitics = match hc {
        None => true,
        Some(hc) => hc.child_bool_text("NotOnClitics").unwrap_or(true),
    };
    let accept_unspecified_graphemes = hc
        .map(|hc| {
            hc.child_bool_text("AcceptUnspecifiedGraphemes")
                .unwrap_or(false)
        })
        .unwrap_or(false);
    let no_default_compounding = hc
        .map(|hc| hc.child_bool_text("NoDefaultCompounding").unwrap_or(false))
        .unwrap_or(false);
    let strata = hc.and_then(|hc| hc.child("Strata")).map(|s| s.text.clone());

    let compound_rule_max_applications = params_elem
        .child("CompoundRules")
        .map(|cr| {
            cr.children
                .iter()
                .filter_map(|rule_elem| {
                    let guid = rule_elem.attr("guid")?.to_string();
                    let max_applications = rule_elem.attr("maxApps")?.parse().ok()?;
                    Some(CompoundRuleMaxApplications {
                        compound_rule: guid,
                        max_applications,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    ParserParameters {
        not_on_clitics,
        accept_unspecified_graphemes,
        no_default_compounding,
        strata,
        compound_rule_max_applications,
    }
}
