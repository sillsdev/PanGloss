//! Parses the `<ParserParameters><HC>...</HC></ParserParameters>` XML blob FieldWorks stores as a string into a `ParserParameters` value, matching `HCLoader`'s constructor: `<HC>` may be entirely absent (e.g. an XAmple-configured project), `notOnClitics` then defaults true, `<CompoundRules>` is a sibling of `<HC>`, not nested inside it, and `<ActiveParser>`/`<XAmple>` are siblings read the same way.

use pg_snapshot::{
    ActiveParser, CompoundRuleMaxApplications, ParserParameters, Warning, XAmpleParameters,
};

use crate::node::parse_full_document;
use crate::{extract::codes, ImportError};

/// Malformed cap metadata becomes a warning; a malformed or unknown active-parser selector is fatal.
pub fn parse_with_issues(
    raw: Option<&str>,
) -> Result<(ParserParameters, Vec<Warning>), ImportError> {
    let Some(raw) = raw else {
        return Ok((ParserParameters::default(), Vec::new()));
    };
    let root = parse_full_document(raw)
        .map_err(|error| invalid_active_parser(format!("ParserParameters XML is malformed: {error}")))?;
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
    let xa = params_elem.child("XAmple");
    fn child_value<T: std::str::FromStr>(
        n: Option<&crate::node::Node>,
        tag: &str,
        issues: &mut Vec<Warning>,
    ) -> Option<T> {
        let child = n.and_then(|n| n.child(tag))?;
        match child.text.trim().parse::<T>() {
            Ok(value) => Some(value),
            Err(_) => {
                issues.push(Warning::new(
                    codes::INVALID_PARSER_PARAMETER,
                    format!(
                        "ParserParameters XAmple {tag} has invalid value {:?}",
                        child.text.trim()
                    ),
                ));
                None
            }
        }
    }
    let xample = XAmpleParameters {
        max_nulls: child_value(xa, "MaxNulls", &mut issues),
        max_prefixes: child_value(xa, "MaxPrefixes", &mut issues),
        max_infixes: child_value(xa, "MaxInfixes", &mut issues),
        max_suffixes: child_value(xa, "MaxSuffixes", &mut issues),
        max_interfixes: child_value(xa, "MaxInterfixes", &mut issues),
        max_roots: child_value(xa, "MaxRoots", &mut issues),
        max_analyses_to_return: child_value(xa, "MaxAnalysesToReturn", &mut issues),
    };

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
    ))
}

fn invalid_active_parser(message: impl Into<String>) -> ImportError {
    ImportError::InvalidSource {
        code: codes::INVALID_ACTIVE_PARSER,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_snapshot::ActiveParser;

    #[test]
    fn absent_active_parser_means_xample_like_liblcm() {
        let p = parse_with_issues(Some(
            "<ParserParameters><HC><NotOnClitics>true</NotOnClitics></HC></ParserParameters>",
        ))
        .unwrap()
        .0;
        assert_eq!(p.active_parser, ActiveParser::XAmple);
        assert_eq!(p.xample, XAmpleParameters::default());
    }

    #[test]
    fn hc_active_parser_is_read() {
        let p = parse_with_issues(Some(
            "<ParserParameters><HC/><ActiveParser>HC</ActiveParser></ParserParameters>",
        ))
        .unwrap()
        .0;
        assert_eq!(p.active_parser, ActiveParser::Hc);
    }

    #[test]
    fn unknown_active_parser_is_fatal() {
        let err = parse_with_issues(Some(
            "<ParserParameters><ActiveParser>Toneparser</ActiveParser></ParserParameters>",
        ))
        .unwrap_err();
        assert!(matches!(
            err,
            ImportError::InvalidSource { code, .. } if code == codes::INVALID_ACTIVE_PARSER
        ));
    }

    #[test]
    fn active_parser_with_nested_element_is_fatal() {
        let err = parse_with_issues(Some(
            "<ParserParameters><ActiveParser>HC<Unexpected/></ActiveParser></ParserParameters>",
        ))
        .unwrap_err();
        assert!(matches!(
            err,
            ImportError::InvalidSource { code, .. } if code == codes::INVALID_ACTIVE_PARSER
        ));
    }

    #[test]
    fn xample_block_is_read_field_by_field() {
        let p = parse_with_issues(Some(
            "<ParserParameters><XAmple><MaxNulls>0</MaxNulls><MaxPrefixes>1</MaxPrefixes>\
             <MaxInfixes>0</MaxInfixes><MaxRoots>1</MaxRoots><MaxSuffixes>0</MaxSuffixes>\
             <MaxInterfixes>0</MaxInterfixes><MaxAnalysesToReturn>20</MaxAnalysesToReturn></XAmple>\
             <ActiveParser>XAmple</ActiveParser></ParserParameters>",
        ))
        .unwrap()
        .0;
        assert_eq!(p.xample.max_nulls, Some(0));
        assert_eq!(p.xample.max_prefixes, Some(1));
        assert_eq!(p.xample.max_infixes, Some(0));
        assert_eq!(p.xample.max_roots, Some(1));
        assert_eq!(p.xample.max_suffixes, Some(0));
        assert_eq!(p.xample.max_interfixes, Some(0));
        assert_eq!(p.xample.max_analyses_to_return, Some(20));
    }

    #[test]
    fn partial_xample_block_leaves_missing_values_none() {
        let p = parse_with_issues(Some(
            "<ParserParameters><XAmple><MaxNulls>1</MaxNulls></XAmple></ParserParameters>",
        ))
        .unwrap()
        .0;
        assert_eq!(p.xample.max_nulls, Some(1));
        assert_eq!(p.xample.max_roots, None);
        assert_eq!(p.xample.max_analyses_to_return, None);
    }

    #[test]
    fn negative_max_analyses_is_kept_raw() {
        let p = parse_with_issues(Some(
            "<ParserParameters><XAmple><MaxAnalysesToReturn>-1</MaxAnalysesToReturn></XAmple></ParserParameters>",
        ))
        .unwrap()
        .0;
        assert_eq!(p.xample.max_analyses_to_return, Some(-1));
    }

    #[test]
    fn malformed_xample_cap_is_none_and_reported() {
        let (params, issues) = parse_with_issues(Some(
            "<ParserParameters><XAmple><MaxPrefixes>many</MaxPrefixes></XAmple></ParserParameters>",
        ))
        .unwrap();
        assert_eq!(params.xample.max_prefixes, None);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "fwdata.invalid-parser-parameter");
        assert!(issues[0].message.contains("MaxPrefixes"));
    }

    #[test]
    fn malformed_parser_parameters_are_fatal_for_active_parser() {
        let err = parse_with_issues(Some("<ParserParameters><ActiveParser>XAmple")).unwrap_err();
        assert!(matches!(
            err,
            ImportError::InvalidSource { code, .. } if code == codes::INVALID_ACTIVE_PARSER
        ));
    }

    #[test]
    fn explicit_xample_and_untrimmed_values_follow_exact_selector_rules() {
        let p = parse_with_issues(Some(
            "<ParserParameters><ActiveParser>XAmple</ActiveParser></ParserParameters>",
        ))
        .unwrap()
        .0;
        assert_eq!(p.active_parser, ActiveParser::XAmple);

        for raw in [
            "<ParserParameters><ActiveParser/></ParserParameters>",
            "<ParserParameters><ActiveParser> XAmple</ActiveParser></ParserParameters>",
            "<ParserParameters><ActiveParser>XAmple </ActiveParser></ParserParameters>",
            "<ParserParameters><ActiveParser> \n\t</ActiveParser></ParserParameters>",
        ] {
            assert!(parse_with_issues(Some(raw)).is_err(), "{raw:?} must be fatal");
        }
    }

    #[test]
    fn only_absent_raw_uses_parser_defaults() {
        assert_eq!(parse_with_issues(None).unwrap().0, ParserParameters::default());
        assert!(parse_with_issues(Some("")).is_err());
        assert!(parse_with_issues(Some(" \n\t")).is_err());
    }
}
