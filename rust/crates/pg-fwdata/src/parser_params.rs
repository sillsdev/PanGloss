//! Parse the `<ParserParameters><HC>...</HC></ParserParameters>` XML blob FieldWorks stores as a
//! *string* on `MorphologicalDataOA.ParserParameters` (`MoMorphData.ParserParameters`, a `Uni`
//! field) into a [`ParserParameters`] value.
//!
//! ← `HCLoader`'s constructor, HCLoader.cs:92-112. That C# reads:
//! - `hcElem = root.Element("HC")` (may be entirely absent — e.g. a project still configured
//!   for the XAmple parser, like Sena 3, has `<ParserParameters><XAmple>...</XAmple></ParserParameters>`
//!   with no `<HC>` sibling at all).
//! - `notOnClitics = hcElem == null || (bool?)hcElem.Element("NotOnClitics") ?? true` — note the
//!   default-true polarity, and that it's *also* true when `<HC>` itself is absent.
//! - `noDefaultCompounding` / `acceptUnspecifiedGraphemes` default `false`, and only ever `true`
//!   when `<HC>` is present *and* the sub-element says so.
//! - `<Strata>` is read only from inside `<HC>`.
//! - `<CompoundRules>` (per-rule `maxApps`) is a *sibling* of `<HC>` directly under the root
//!   `<ParserParameters>` element, not nested inside it.

use pg_snapshot::{CompoundRuleMaxApplications, ParserParameters};

use crate::node::parse_full_document;

/// `raw` is the already-XML-unescaped text of the `<Uni>` element (i.e. `Node::uni_text`'s
/// result) — a complete `<ParserParameters>...</ParserParameters>` document in its own right.
/// Returns `ParserParameters::default()` (matching `HCLoader`'s `hcElem == null` defaults) if
/// `raw` is absent, empty, or fails to parse as XML — this is diagnostic input a user hand-edited
/// via FieldWorks' UI, not something worth a hard error over.
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
