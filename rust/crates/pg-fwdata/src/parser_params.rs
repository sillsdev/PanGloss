//! Parses the `<ParserParameters><HC>...</HC></ParserParameters>` XML blob FieldWorks stores as a string into a `ParserParameters` value, matching `HCLoader`'s constructor: `<HC>` may be entirely absent (e.g. an XAmple-configured project), `notOnClitics` then defaults true, `<CompoundRules>` is a sibling of `<HC>`, not nested inside it, and `<ActiveParser>`/`<XAmple>` are siblings read the same way.

use pg_snapshot::{ActiveParser, CompoundRuleMaxApplications, ParserParameters, XAmpleParameters};

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

    let active_parser = match params_elem.child("ActiveParser").map(|n| n.text.trim()) {
        Some("HC") => ActiveParser::Hc,
        // liblcm's getter returns "XAmple" for anything else, including an absent element.
        _ => ActiveParser::XAmple,
    };

    let xa = params_elem.child("XAmple");
    fn u32_child(n: Option<&crate::node::Node>, tag: &str) -> Option<u32> {
        n.and_then(|n| n.child(tag))
            .and_then(|c| c.text.trim().parse::<u32>().ok())
    }
    let xample = XAmpleParameters {
        max_nulls: u32_child(xa, "MaxNulls"),
        max_prefixes: u32_child(xa, "MaxPrefixes"),
        max_infixes: u32_child(xa, "MaxInfixes"),
        max_suffixes: u32_child(xa, "MaxSuffixes"),
        max_interfixes: u32_child(xa, "MaxInterfixes"),
        max_roots: u32_child(xa, "MaxRoots"),
        max_analyses_to_return: xa
            .and_then(|n| n.child("MaxAnalysesToReturn"))
            .and_then(|c| c.text.trim().parse::<i32>().ok()),
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

    ParserParameters {
        not_on_clitics,
        accept_unspecified_graphemes,
        no_default_compounding,
        strata,
        compound_rule_max_applications,
        active_parser,
        xample,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_snapshot::ActiveParser;

    #[test]
    fn absent_active_parser_means_xample_like_liblcm() {
        let p = parse(Some("<ParserParameters><HC><NotOnClitics>true</NotOnClitics></HC></ParserParameters>"));
        assert_eq!(p.active_parser, ActiveParser::XAmple);
        assert_eq!(p.xample, XAmpleParameters::default());
    }

    #[test]
    fn hc_active_parser_is_read() {
        let p = parse(Some("<ParserParameters><HC/><ActiveParser>HC</ActiveParser></ParserParameters>"));
        assert_eq!(p.active_parser, ActiveParser::Hc);
    }

    #[test]
    fn unknown_active_parser_text_falls_back_to_xample() {
        let p = parse(Some("<ParserParameters><ActiveParser>Toneparser</ActiveParser></ParserParameters>"));
        assert_eq!(p.active_parser, ActiveParser::XAmple);
    }

    #[test]
    fn xample_block_is_read_field_by_field() {
        let p = parse(Some(
            "<ParserParameters><XAmple><MaxNulls>0</MaxNulls><MaxPrefixes>1</MaxPrefixes>\
             <MaxInfixes>0</MaxInfixes><MaxRoots>1</MaxRoots><MaxSuffixes>0</MaxSuffixes>\
             <MaxInterfixes>0</MaxInterfixes><MaxAnalysesToReturn>20</MaxAnalysesToReturn></XAmple>\
             <ActiveParser>XAmple</ActiveParser></ParserParameters>",
        ));
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
        let p = parse(Some("<ParserParameters><XAmple><MaxNulls>1</MaxNulls></XAmple></ParserParameters>"));
        assert_eq!(p.xample.max_nulls, Some(1));
        assert_eq!(p.xample.max_roots, None);
        assert_eq!(p.xample.max_analyses_to_return, None);
    }

    #[test]
    fn negative_max_analyses_is_kept_raw() {
        let p = parse(Some("<ParserParameters><XAmple><MaxAnalysesToReturn>-1</MaxAnalysesToReturn></XAmple></ParserParameters>"));
        assert_eq!(p.xample.max_analyses_to_return, Some(-1));
    }
}
