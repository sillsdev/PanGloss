//! Parses the `<ParserParameters><HC>...</HC></ParserParameters>` XML blob FieldWorks stores as a string into a `ParserParameters` value, matching `HCLoader`'s constructor: `<HC>` may be entirely absent (e.g. an XAmple-configured project), `notOnClitics` then defaults true, `<CompoundRules>` is a sibling of `<HC>`, not nested inside it, and `<ActiveParser>`/`<XAmple>` are siblings read the same way.

use pg_snapshot::{
    ActiveParser, CompoundRuleMaxApplications, FwClass, FwObjectRef, ParserParameters, Warning,
    XAmpleParameters,
};

use crate::node::parse_full_document;
use crate::{extract::codes, ImportError};

/// Which optional `ParserParameters` source fields were physically present in `<Uni>`, and whether each HC boolean or `XAmple` cap parsed successfully — used only for `graphToSnapshot` recording, never to decide `ParserParameters`'s own values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParserSettingsPresence {
    pub active_parser: bool,
    pub hc_fields: Vec<(&'static str, bool)>,
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
                issues.push(
                    Warning::new(
                        codes::INVALID_PARSER_PARAMETER,
                        format!("Parser parameter '{tag}' has an invalid numeric value."),
                    )
                    .with_subject(
                        FwObjectRef::new(FwClass::Project)
                            .project_settings()
                            .field(format!("XAmple.{tag}")),
                    ),
                );
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

    fn hc_bool(
        hc: Option<&crate::node::Node>,
        tag: &'static str,
        default: bool,
        issues: &mut Vec<Warning>,
        presence: &mut Vec<(&'static str, bool)>,
    ) -> bool {
        let Some(child) = hc.and_then(|hc| hc.child(tag)) else {
            return default;
        };
        match child.bool_text() {
            Some(value) => {
                presence.push((tag, true));
                value
            }
            None => {
                issues.push(
                    Warning::new(
                        codes::INVALID_PARSER_PARAMETER,
                        format!("Parser parameter '{tag}' has an invalid boolean value."),
                    )
                    .with_subject(
                        FwObjectRef::new(FwClass::Project)
                            .project_settings()
                            .field(format!("HC.{tag}")),
                    ),
                );
                presence.push((tag, false));
                default
            }
        }
    }
    let mut hc_fields_present = Vec::new();
    let not_on_clitics = hc_bool(
        hc,
        "NotOnClitics",
        true,
        &mut issues,
        &mut hc_fields_present,
    );
    let accept_unspecified_graphemes = hc_bool(
        hc,
        "AcceptUnspecifiedGraphemes",
        false,
        &mut issues,
        &mut hc_fields_present,
    );
    let no_default_compounding = hc_bool(
        hc,
        "NoDefaultCompounding",
        false,
        &mut issues,
        &mut hc_fields_present,
    );
    let deletion_reapplications = hc_delete_reapps(hc, &mut hc_fields_present)?;
    let max_roots = hc_max_roots(hc, &mut hc_fields_present)?;
    let make_partials_final = hc_parser_bool(
        hc,
        "MakePartialsFinal",
        "Make partials final",
        &mut hc_fields_present,
    )?
    .unwrap_or(false);
    let merge_analyses = hc_parser_bool(
        hc,
        "MergeAnalyses",
        "Merge analyses",
        &mut hc_fields_present,
    )?
    .unwrap_or(true);
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
            deletion_reapplications,
            max_roots,
            make_partials_final,
            merge_analyses,
            strata,
            compound_rule_max_applications,
            active_parser,
            xample,
        },
        issues,
        ParserSettingsPresence {
            active_parser: active_parser_present,
            hc_fields: hc_fields_present,
            xample_fields: xample_fields_present,
        },
    ))
}

/// `HCParser.LoadParser` reads `<HC><DelReapps>` as `(int) element` with default 0; a negative value stops the C# loop after one pass exactly like 0, so it is stored as 0.
fn hc_delete_reapps(
    hc: Option<&crate::node::Node>,
    presence: &mut Vec<(&'static str, bool)>,
) -> Result<u32, ImportError> {
    let value = hc_int(
        hc,
        "DelReapps",
        "Deletion reapplications",
        "0 to 10",
        presence,
    )?;
    Ok(value.map_or(0, |value| value.max(0) as u32))
}

/// `HCParser.LoadParser` reads `<HC><MaxRoots>` with `int.Parse` into `Morpher.MaxStemCount` (default 2 when absent). C# `AnalysisCompoundingRule` gates on `NonHeadCount + 1 >= MaxStemCount`, so every value below 1 behaves exactly like 1 and is stored as 1.
fn hc_max_roots(
    hc: Option<&crate::node::Node>,
    presence: &mut Vec<(&'static str, bool)>,
) -> Result<Option<u32>, ImportError> {
    let value = hc_int(hc, "MaxRoots", "Maximum roots", "1 to 10", presence)?;
    Ok(value.map(|value| value.max(1) as u32))
}

/// A boolean `<HC>` setting `HCParser.LoadParser` reads with `(bool) element`: `None` when absent. That cast accepts `true`/`false`/`1`/`0` in any case and throws otherwise, and FieldWorks then loads no parser, so anything else is fatal here, naming the field and the value.
fn hc_parser_bool(
    hc: Option<&crate::node::Node>,
    tag: &'static str,
    label: &str,
    presence: &mut Vec<(&'static str, bool)>,
) -> Result<Option<bool>, ImportError> {
    let Some(child) = hc.and_then(|hc| hc.child(tag)) else {
        return Ok(None);
    };
    let value = child
        .bool_text()
        .ok_or_else(|| ImportError::InvalidSource {
            code: codes::INVALID_PARSER_PARAMETER,
            message: format!(
                "Parser parameter 'HC.{tag}' ({label}) has the value {:?}, which is not true or \
             false; set it in the FieldWorks parser parameters.",
                child.text
            ),
        })?;
    presence.push((tag, true));
    Ok(Some(value))
}

/// An integer `<HC>` setting `HCParser.LoadParser` reads with `(int)` or `int.Parse`: `None` when absent. Both C# reads throw on a non-integer and FieldWorks then loads no parser, so it is fatal here too, naming the field and the value.
fn hc_int(
    hc: Option<&crate::node::Node>,
    tag: &'static str,
    label: &str,
    range: &str,
    presence: &mut Vec<(&'static str, bool)>,
) -> Result<Option<i32>, ImportError> {
    let Some(child) = hc.and_then(|hc| hc.child(tag)) else {
        return Ok(None);
    };
    let value = child
        .text
        .trim()
        .parse::<i32>()
        .map_err(|_| ImportError::InvalidSource {
            code: codes::INVALID_PARSER_PARAMETER,
            message: format!(
                "Parser parameter 'HC.{tag}' ({label}) has the value {:?}, which is not a whole \
                 number; set it to a whole number from {range} in the FieldWorks parser \
                 parameters.",
                child.text
            ),
        })?;
    presence.push((tag, true));
    Ok(Some(value))
}

fn invalid_active_parser(message: impl Into<String>) -> ImportError {
    ImportError::InvalidSource {
        code: codes::INVALID_ACTIVE_PARSER,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests;
