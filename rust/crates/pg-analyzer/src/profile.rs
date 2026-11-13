use crate::SegmentationProfile;
use quick_xml::{events::Event, Reader};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub(crate) fn from_ldml(identity: &str, xml: &str) -> Result<SegmentationProfile, String> {
    let mut reader = Reader::from_str(xml);
    let mut in_main = false;
    let mut in_aux = false;
    let mut found = false;
    let mut pattern = String::new();
    let mut aux_pattern = String::new();
    let mut depth = 0usize;
    let mut root = false;
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        match &event {
            Event::Start(e) => {
                if depth == 0 {
                    if root || e.name().as_ref() != b"ldml" {
                        return Err("expected one ldml root".into());
                    }
                    root = true;
                }
                depth += 1;
            }
            Event::Empty(e) if depth == 0 => {
                if root || e.name().as_ref() != b"ldml" {
                    return Err("expected one ldml root".into());
                }
                root = true;
            }
            Event::End(_) => {
                depth = depth.checked_sub(1).ok_or("unexpected closing element")?;
            }
            _ => {}
        }
        match event {
            Event::Start(e) if e.name().as_ref() == b"exemplarCharacters" => {
                let mut main = true;
                let mut aux = false;
                let mut alternate = false;
                for attr in e.attributes() {
                    let attr = attr.map_err(|e| e.to_string())?;
                    if attr.key.as_ref() == b"type" {
                        let kind = attr.unescape_value().map_err(|e| e.to_string())?;
                        main = kind == "main";
                        aux = kind == "auxiliary";
                    }
                    if attr.key.as_ref() == b"alt" || attr.key.as_ref() == b"draft" {
                        alternate = true;
                    }
                }
                if aux && !alternate {
                    in_aux = true;
                }
                if main && !alternate {
                    if found {
                        return Err("multiple main exemplar character sets".into());
                    }
                    in_main = true;
                    found = true;
                }
            }
            Event::Text(e) if in_main => {
                pattern.push_str(&e.unescape().map_err(|e| e.to_string())?)
            }
            Event::Text(e) if in_aux => {
                aux_pattern.push_str(&e.unescape().map_err(|e| e.to_string())?)
            }
            Event::End(e) if e.name().as_ref() == b"exemplarCharacters" && (in_main || in_aux) => {
                in_main = false;
                in_aux = false;
            }
            Event::Eof => {
                if depth != 0 || !root {
                    return Err("incomplete LDML document".into());
                }
                break;
            }
            _ => {}
        }
    }
    let word_forming = if found {
        parse_set(&pattern)?
    } else {
        Vec::new()
    };
    let auxiliary = if aux_pattern.is_empty() {
        Vec::new()
    } else {
        parse_set(&aux_pattern)?
    };
    Ok(SegmentationProfile {
        version: 1,
        auxiliary,
        source_id: identity.into(),
        source_sha256: format!("{:x}", Sha256::digest(xml.as_bytes())),
        word_forming,
        reject_pattern: None,
        accept_words: Vec::new(),
        accept_patterns: Vec::new(),
    })
}

fn parse_set(pattern: &str) -> Result<Vec<char>, String> {
    let s = pattern
        .trim_matches(pattern_whitespace)
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or("invalid Unicode set")?;
    if s.len() > 2
        && s.starts_with(':')
        && s.ends_with(':')
        && s[..s.len() - 1]
            .chars()
            .rev()
            .take_while(|c| *c == '\\')
            .count()
            % 2
            == 0
    {
        return Err("unsupported Unicode set property".into());
    }
    let mut chars = s.chars().peekable();
    let mut values = BTreeSet::new();
    while chars.peek().is_some() {
        if chars.peek().is_some_and(|c| pattern_whitespace(*c)) {
            chars.next();
            continue;
        }
        let first_is_string = chars.peek() == Some(&'{');
        let first = scalar(&mut chars)?;
        while chars.peek().is_some_and(|c| pattern_whitespace(*c)) {
            chars.next();
        }
        if chars.peek() == Some(&'-') {
            chars.next();
            while chars.peek().is_some_and(|c| pattern_whitespace(*c)) {
                chars.next();
            }
            if chars.peek().is_none() {
                values.insert(first);
                values.insert('-');
                break;
            }
            if first_is_string {
                return Err("unsupported Unicode set string range".into());
            }
            if chars.peek() == Some(&'{') || chars.peek() == Some(&'-') {
                return Err("unsupported Unicode set range".into());
            }
            let last = scalar(&mut chars)?;
            if first > last {
                return Err("descending Unicode set range".into());
            }
            for n in first as u32..=last as u32 {
                if let Some(c) = char::from_u32(n) {
                    values.insert(c);
                }
            }
        } else {
            values.insert(first);
        }
    }
    Ok(values.into_iter().collect())
}

fn pattern_whitespace(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{0085}' | '\u{200e}'..='\u{200f}' | '\u{2028}'..='\u{2029}')
}

fn scalar(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<char, String> {
    let c = chars.next().ok_or("incomplete Unicode set")?;
    if c == '{' {
        while chars.peek().is_some_and(|c| pattern_whitespace(*c)) {
            chars.next();
        }
        if chars.peek() == Some(&'}') {
            return Err("empty Unicode set string".into());
        }
        if chars.peek() == Some(&'{') {
            return Err("nested Unicode set string".into());
        }
        let first = scalar(chars)?;
        loop {
            while chars.peek().is_some_and(|c| pattern_whitespace(*c)) {
                chars.next();
            }
            match chars.peek() {
                Some('}') => {
                    chars.next();
                    break;
                }
                Some('{') => return Err("nested Unicode set string".into()),
                Some(_) => {
                    scalar(chars)?;
                }
                None => return Err("unclosed Unicode set string".into()),
            }
        }
        return Ok(first);
    }
    if matches!(c, '[' | ']' | '&' | '^' | '$' | '}') {
        return Err("unsupported Unicode set operation".into());
    }
    if c != '\\' {
        return Ok(c);
    }
    let escaped = chars.next().ok_or("incomplete escape")?;
    let digits = match escaped {
        'u' => 4,
        'U' => 8,
        'x' => {
            if chars.next() != Some('{') {
                return Err("expected braced hex escape".into());
            }
            let mut hex = String::new();
            loop {
                match chars.next() {
                    Some('}') => break,
                    Some(c) if c.is_ascii_hexdigit() && hex.len() < 6 => hex.push(c),
                    _ => return Err("invalid braced hex escape".into()),
                }
            }
            return char::from_u32(u32::from_str_radix(&hex, 16).map_err(|e| e.to_string())?)
                .ok_or_else(|| "invalid Unicode scalar".into());
        }
        c if !c.is_alphanumeric() => return Ok(c),
        _ => return Err("unsupported Unicode set escape".into()),
    };
    let hex: String = chars.take(digits).collect();
    if hex.len() != digits {
        return Err("incomplete Unicode escape".into());
    }
    char::from_u32(u32::from_str_radix(&hex, 16).map_err(|e| e.to_string())?)
        .ok_or_else(|| "invalid Unicode scalar".into())
}
