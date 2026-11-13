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
    while i < chars.len() {
        if chars[i] != '\\' {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '+') {
            end += 1;
        }
        let name: String = chars[i + 1..end].iter().collect();
        let name = name.trim_start_matches('+').to_owned();
        if end < chars.len() && chars[end] == '*' {
            end += 1;
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
        runs.push(run(i, end, MARKER_STYLE));
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
