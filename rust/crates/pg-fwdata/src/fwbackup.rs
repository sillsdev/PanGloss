//! `.fwbackup` zip support: the embedded `.fwdata` plus its `WritingSystemStore/*.ldml` exemplars.

use unicode_normalization::UnicodeNormalization;

/// Text elements of the LDML main exemplar set (UnicodeSet syntax), NFD.
pub(crate) fn exemplar_characters_from_ldml(ldml: &str) -> Vec<String> {
    let Some(set) = exemplar_set_text(ldml) else {
        return Vec::new();
    };
    parse_unicode_set(&set)
        .into_iter()
        .map(|s| s.nfd().collect::<String>())
        .collect()
}

fn exemplar_set_text(ldml: &str) -> Option<String> {
    // Only the *main* set: the element with no `type` attribute (auxiliary/index/punctuation carry one).
    let mut rest = ldml;
    while let Some(start) = rest.find("<exemplarCharacters") {
        let after = &rest[start + "<exemplarCharacters".len()..];
        let close = after.find('>')?;
        let attrs = &after[..close];
        let body_start = close + 1;
        let end = after[body_start..].find("</exemplarCharacters>")?;
        let body = &after[body_start..body_start + end];
        if !attrs.contains("type=") {
            return Some(body.trim().to_string());
        }
        rest = &after[body_start + end..];
    }
    None
}

fn parse_unicode_set(set: &str) -> Vec<String> {
    let inner = set.trim().trim_start_matches('[').trim_end_matches(']');
    let chars: Vec<char> = inner.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    // Reads one atom (a literal char or a \uXXXX escape) at `i`, returning it and the next index.
    let read_atom = |i: usize| -> Option<(char, usize)> {
        let c = *chars.get(i)?;
        if c == '\\' {
            if chars.get(i + 1) == Some(&'u') {
                let hex: String = chars.get(i + 2..i + 6)?.iter().collect();
                let cp = u32::from_str_radix(&hex, 16).ok()?;
                return Some((char::from_u32(cp)?, i + 6));
            }
            return Some((*chars.get(i + 1)?, i + 2));
        }
        Some((c, i + 1))
    };
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '{' {
            let mut j = i + 1;
            let mut cluster = String::new();
            while j < chars.len() && chars[j] != '}' {
                match read_atom(j) {
                    Some((ch, nj)) => {
                        cluster.push(ch);
                        j = nj;
                    }
                    None => break,
                }
            }
            if !cluster.is_empty() {
                out.push(cluster);
            }
            i = j + 1;
            continue;
        }
        let Some((lo, ni)) = read_atom(i) else { break };
        if chars.get(ni) == Some(&'-') {
            if let Some((hi, nj)) = read_atom(ni + 1) {
                if hi >= lo {
                    for cp in (lo as u32)..=(hi as u32) {
                        if let Some(ch) = char::from_u32(cp) {
                            out.push(ch.to_string());
                        }
                    }
                    i = nj;
                    continue;
                }
            }
        }
        out.push(lo.to_string());
        i = ni;
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LDML: &str = r#"<?xml version="1.0"?><ldml><identity><language type="mgz"/></identity>
<characters><exemplarCharacters>[ABD-PR-WYabd-pr-wy\u0190\u0254{CH}{a\u0303}{ch}]</exemplarCharacters></characters></ldml>"#;

    #[test]
    fn exemplars_expand_ranges_escapes_and_braced_clusters() {
        let ex = exemplar_characters_from_ldml(LDML);
        assert!(ex.contains(&"a".to_string()));
        assert!(ex.contains(&"d".to_string()));
        assert!(ex.contains(&"e".to_string()), "range d-p includes e");
        assert!(ex.contains(&"p".to_string()));
        assert!(!ex.contains(&"q".to_string()), "q is outside every range");
        assert!(ex.contains(&"\u{0190}".to_string()));
        assert!(ex.contains(&"CH".to_string()));
        assert!(ex.contains(&"a\u{0303}".to_string()));
        assert!(!ex.iter().any(|s| s.contains('{') || s.contains('}') || s.contains('[')));
    }

    #[test]
    fn missing_characters_element_yields_empty() {
        assert!(exemplar_characters_from_ldml("<ldml/>").is_empty());
    }

    #[test]
    fn nfd_is_applied() {
        let ex = exemplar_characters_from_ldml(
            "<ldml><characters><exemplarCharacters>[\u{00E9}]</exemplarCharacters></characters></ldml>",
        );
        assert_eq!(ex, vec!["e\u{0301}".to_string()]);
    }
}
