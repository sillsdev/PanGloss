//! Parses the `<ParserParameters><HC>...</HC></ParserParameters>` XML blob FieldWorks stores as a string into a `ParserParameters` value, matching `HCLoader`'s constructor: `<HC>` may be entirely absent (e.g. an XAmple-configured project), `notOnClitics` then defaults true, `<CompoundRules>` is a sibling of `<HC>`, not nested inside it, and `<ActiveParser>`/`<XAmple>` are siblings read the same way.

use pg_snapshot::{
    ActiveParser, CompoundRuleMaxApplications, ParserParameters, Warning, XAmpleParameters,
};

use crate::node::parse_full_document;
use crate::{extract::codes, ImportError};

/// Which optional `ParserParameters` source fields were physically present in `<Uni>`, and (for the `XAmple` caps) whether each one parsed successfully — used only for `graphToSnapshot` recording, never to decide `ParserParameters`'s own values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParserSettingsPresence {
    pub active_parser: bool,
    pub accept_unspecified_graphemes: bool,
    pub xample_fields: Vec<(&'static str, bool)>,
}

/// The `<XAmple>` cap field names, in the order `parse_with_issues` fills them.
#[cfg(test)]
const XAMPLE_FIELD_TAGS: &[&str] = &[
    "MaxNulls",
    "MaxPrefixes",
    "MaxInfixes",
    "MaxSuffixes",
    "MaxInterfixes",
    "MaxRoots",
    "MaxAnalysesToReturn",
];

/// Malformed cap metadata becomes a warning; a malformed or unknown active-parser selector is fatal.
pub fn parse_with_issues(
    raw: Option<&str>,
) -> Result<(ParserParameters, Vec<Warning>, ParserSettingsPresence), ImportError> {
    let Some(raw) = raw else {
        return Ok((
            ParserParameters::default(),
            Vec::new(),
            ParserSettingsPresence::default(),
        ));
    };
    let root = parse_full_document(raw).map_err(|error| {
        invalid_active_parser(format!("ParserParameters XML is malformed: {error}"))
    })?;
    // `root` is our synthetic document root; its first child should be `<ParserParameters>`.
    let Some(params_elem) = root
        .children
        .first()
        .filter(|node| node.tag == "ParserParameters")
    else {
        return Err(invalid_active_parser(
            "ParserParameters XML has no valid root element",
        ));
    };
    let hc = params_elem.child("HC");

    let active_parser_present = params_elem.child("ActiveParser").is_some();
    let active_parser = match params_elem.child("ActiveParser") {
        None => ActiveParser::XAmple,
        Some(node) if node.text == "HC" && node.children.is_empty() => ActiveParser::Hc,
        Some(node) if node.text == "XAmple" && node.children.is_empty() => ActiveParser::XAmple,
        Some(node) => {
            return Err(invalid_active_parser(format!(
                "ActiveParser has unrecognized value {:?}",
                node.text
            )));
        }
    };

    let mut issues = Vec::new();
    let mut xample_fields_present: Vec<(&'static str, bool)> = Vec::new();
    let xa = params_elem.child("XAmple");
    fn child_value<T: std::str::FromStr>(
        n: Option<&crate::node::Node>,
        tag: &'static str,
        issues: &mut Vec<Warning>,
        presence: &mut Vec<(&'static str, bool)>,
    ) -> Option<T> {
        let child = n.and_then(|n| n.child(tag))?;
        match child.text.trim().parse::<T>() {
            Ok(value) => {
                presence.push((tag, true));
                Some(value)
            }
            Err(_) => {
                issues.push(Warning::new(
                    codes::INVALID_PARSER_PARAMETER,
                    format!(
                        "ParserParameters XAmple {tag} has invalid value {:?}",
                        child.text.trim()
                    ),
                ));
                presence.push((tag, false));
                None
            }
        }
    }
    let xample = XAmpleParameters {
        max_nulls: child_value(xa, "MaxNulls", &mut issues, &mut xample_fields_present),
        max_prefixes: child_value(xa, "MaxPrefixes", &mut issues, &mut xample_fields_present),
        max_infixes: child_value(xa, "MaxInfixes", &mut issues, &mut xample_fields_present),
        max_suffixes: child_value(xa, "MaxSuffixes", &mut issues, &mut xample_fields_present),
        max_interfixes: child_value(xa, "MaxInterfixes", &mut issues, &mut xample_fields_present),
        max_roots: child_value(xa, "MaxRoots", &mut issues, &mut xample_fields_present),
        max_analyses_to_return: child_value(
            xa,
            "MaxAnalysesToReturn",
            &mut issues,
            &mut xample_fields_present,
        ),
    };

    let not_on_clitics = match hc {
        None => true,
        Some(hc) => hc.child_bool_text("NotOnClitics").unwrap_or(true),
    };
    let accept_unspecified_graphemes_present =
        hc.is_some_and(|hc| hc.child("AcceptUnspecifiedGraphemes").is_some());
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

    Ok((
        ParserParameters {
            not_on_clitics,
            accept_unspecified_graphemes,
            no_default_compounding,
            strata,
            compound_rule_max_applications,
            active_parser,
            xample,
        },
        issues,
        ParserSettingsPresence {
            active_parser: active_parser_present,
            accept_unspecified_graphemes: accept_unspecified_graphemes_present,
            xample_fields: xample_fields_present,
        },
    ))
}

fn invalid_active_parser(message: impl Into<String>) -> ImportError {
    ImportError::InvalidSource {
        code: codes::INVALID_ACTIVE_PARSER,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests;
