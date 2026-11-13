//! Styled runs for USFM text, so `\c` and `\v` numbers and the markers themselves never become words.

use crate::StyledRun;

/// Style of a USFM marker (`\p`, `\v`, `\id GEN ...`): never word-forming and never reported.
pub(crate) const MARKER_STYLE: &str = "USFM Marker";

/// Markers whose next token is a chapter label.
const CHAPTER_LABELS: [&str; 3] = ["c", "cp", "ca"];
/// Markers whose next token is a verse label.
const VERSE_LABELS: [&str; 3] = ["v", "vp", "va"];
/// Markers whose whole line is metadata, not text.
const METADATA_LINES: [&str; 3] = ["id", "ide", "usfm"];
/// Character markers whose `word|attributes` form keeps the word and drops the attributes.
const ATTRIBUTE_SPANS: [&str; 8] = ["w", "wg", "wh", "wa", "rb", "jmp", "fig", "k"];
/// Markers whose body is a reference, not text.
const REFERENCE_BODIES: [&str; 3] = ["xt", "xo", "fr"];

/// Ordered code-point runs that keep USFM markers and chapter and verse numbers out of the word list.
pub fn usfm_styles(text: &str) -> Vec<StyledRun> {
    let chars: Vec<char> = text.chars().collect();
    let mut runs = Vec::new();
    let run = |start, end, style: &str| StyledRun {
        start,
        end,
        style: style.into(),
        profile: None,
    };
    let mut i = 0;
    let mut attribute_span = false;
    while i < chars.len() {
        if chars[i] != '\\' {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        while end < chars.len()
            && (chars[end].is_ascii_alphanumeric() || chars[end] == '+' || chars[end] == '-')
        {
            end += 1;
        }
        let name: String = chars[i + 1..end].iter().collect();
        let name = name.trim_start_matches('+').to_owned();
        let mut closing_star = false;
        if end < chars.len() && chars[end] == '*' {
            end += 1;
            closing_star = true;
        }
        if end == i + 1 {
            i += 1;
            continue;
        }
        if METADATA_LINES.contains(&name.as_str()) {
            while end < chars.len() && chars[end] != '\n' {
                end += 1;
            }
            runs.push(run(i, end, MARKER_STYLE));
            i = end;
            continue;
        }
        let milestone =
            name.ends_with("-s") || name.ends_with("-e") || name.starts_with('z') || name == "ts";
        if milestone && !closing_star {
            // Milestone attributes run to the closing `\*`, or to the end of the line.
            let mut stop = end;
            while stop < chars.len() && chars[stop] != '\n' {
                if chars[stop] == '\\' && chars.get(stop + 1) == Some(&'*') {
                    stop += 2;
                    break;
                }
                stop += 1;
            }
            runs.push(run(i, stop, MARKER_STYLE));
            i = stop;
            continue;
        }
        runs.push(run(i, end, MARKER_STYLE));
        if closing_star {
            attribute_span = false;
        } else if ATTRIBUTE_SPANS.contains(&name.as_str()) {
            attribute_span = true;
        }
        if REFERENCE_BODIES.contains(&name.as_str()) && !closing_star {
            let mut stop = end;
            while stop < chars.len() && chars[stop] != '\\' {
                stop += 1;
            }
            if stop > end {
                runs.push(run(end, stop, MARKER_STYLE));
            }
            i = stop;
            continue;
        }
        if attribute_span {
            // `\w word|attributes\w*`: the attributes, from `|` to the next marker, are not text.
            let mut look = end;
            while look < chars.len() && chars[look] != '\\' && chars[look] != '|' {
                look += 1;
            }
            if chars.get(look) == Some(&'|') {
                let mut stop = look;
                while stop < chars.len() && chars[stop] != '\\' {
                    stop += 1;
                }
                runs.push(run(look, stop, MARKER_STYLE));
                i = stop;
                continue;
            }
        }
        let style = if CHAPTER_LABELS.contains(&name.as_str()) {
            Some("Chapter Number")
        } else if VERSE_LABELS.contains(&name.as_str()) {
            Some("Verse Number")
        } else {
            None
        };
        i = end;
        if let Some(style) = style {
            let mut start = i;
            while start < chars.len() && chars[start].is_whitespace() {
                start += 1;
            }
            let mut stop = start;
            while stop < chars.len() && !chars[stop].is_whitespace() && chars[stop] != '\\' {
                stop += 1;
            }
            if stop > start {
                runs.push(run(start, stop, style));
                i = stop;
            }
        }
    }
    runs
}
