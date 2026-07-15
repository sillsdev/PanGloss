//! Grammar augmentation via XML injection (add-to-dictionary design doc, Sub-project 1,
//! component 5): splice fresh `<LexicalEntry>` elements — cloned from a class's own exemplar —
//! into a copy of the grammar's original XML text, so the caller (hc-wasm, a later phase) can
//! reload the augmented text through the normal `hc_grammar::load` + `Morpher` construction path.
//! Recognition of user words on future text comes ENTIRELY from that reload; this module never
//! touches the in-memory [`Grammar`].
//!
//! `hc-grammar`'s own loader (`hc_grammar::load`) parses XML into a private, read-only, non-`pub`
//! DOM (`load.rs`'s internal `Node`) with no exposed byte-position tracking, so it can't be reused
//! here. Per the design doc ("if it's a read-only parser ... splice by node byte-ranges — that is
//! acceptable and expected"), this module does its own minimal byte-range text surgery instead of
//! parsing/re-serializing the whole document — the exemplar element is cloned by locating its
//! `[start, end)` byte span in the original text, patching a handful of well-known sub-elements/
//! attributes by further byte-range substitution within that clone, and splicing the result back
//! in as a sibling. This assumes double-quoted attribute values throughout (true of every
//! `HCLoader`/hand-built fixture this port has seen — .NET's default `XmlWriter` quoting) and a
//! single `<Property name="ID">` ordering (`name` before the text content) when patching that one
//! property — both flagged, known limitations of the splice-not-parse approach, acceptable for v1.

use std::collections::HashSet;

use hc_grammar::model::Grammar;
use serde::Serialize;

use crate::classes::{validate_shape, ClassCandidate};
use crate::model::UserLexicon;

/// Per-entry skip report from [`augment_xml`]: user-lexicon entries that could not be spliced in
/// (an unresolvable `class_key` after a project reconversion, an invalid shape, or an exemplar
/// element gone missing), each with a human-readable reason.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AugmentReport {
    pub skipped: Vec<String>,
}

/// Clone `candidates`-resolved exemplar `<LexicalEntry>` elements out of `original_xml`, once per
/// [`crate::model::UserLexEntry`] in `lexicon`, patching each clone's ids, `<PhoneticShape>`,
/// `<Gloss>`, and `<Property name="ID">` (set to `user:<entry.id>`), reduced to a single
/// allomorph, and insert it as a sibling of its exemplar. POS/MPR/stratum attributes on the clone
/// are untouched (copied verbatim from the exemplar) — that is the whole point of exemplar
/// cloning: every reference the clone carries is guaranteed valid because it is copied from a
/// real, already-loaded entry, never authored fresh by this crate.
///
/// Entries whose `class_key` no longer resolves against `candidates`, whose `shape` fails
/// [`validate_shape`], or whose exemplar element can't be found in `original_xml` are skipped and
/// reported in the returned [`AugmentReport`] rather than failing the whole call.
pub fn augment_xml(
    original_xml: &str,
    grammar: &Grammar,
    lexicon: &UserLexicon,
    candidates: &[ClassCandidate],
) -> Result<(String, AugmentReport), String> {
    let mut used_ids = collect_existing_ids(original_xml);
    let mut next_counter: u64 = 1;
    let mut skipped = Vec::new();
    let mut insertions: Vec<(usize, String)> = Vec::new();

    for entry in &lexicon.entries {
        let Some(candidate) = candidates.iter().find(|c| c.key == entry.class_key) else {
            skipped.push(format!(
                "{} ({:?}): unknown class '{}'",
                entry.id, entry.shape, entry.class_key
            ));
            continue;
        };
        if let Err(msg) = validate_shape(grammar, &entry.shape) {
            skipped.push(format!("{} ({:?}): {msg}", entry.id, entry.shape));
            continue;
        }
        let Some((start, end)) =
            find_tagged_element_span(original_xml, "LexicalEntry", &candidate.exemplar_xml_key)
        else {
            skipped.push(format!(
                "{} ({:?}): exemplar '{}' not found in the grammar XML",
                entry.id, entry.shape, candidate.exemplar_xml_key
            ));
            continue;
        };

        let new_entry_id = fresh_id(&mut used_ids, &mut next_counter);
        let new_allo_id = fresh_id(&mut used_ids, &mut next_counter);
        let marker = format!("user:{}", entry.id);

        match build_clone_xml(
            &original_xml[start..end],
            &new_entry_id,
            &new_allo_id,
            &entry.shape,
            &entry.gloss,
            &marker,
        ) {
            Ok(clone_xml) => insertions.push((end, clone_xml)),
            Err(msg) => skipped.push(format!("{} ({:?}): {msg}", entry.id, entry.shape)),
        }
    }

    // All insertion points were computed against the PRISTINE `original_xml` byte offsets, so they
    // must be applied in one pass over that same untouched text (applying them one at a time onto
    // a growing string would invalidate every later offset).
    insertions.sort_by_key(|(at, _)| *at);
    let extra: usize = insertions.iter().map(|(_, s)| s.len()).sum();
    let mut out = String::with_capacity(original_xml.len() + extra);
    let mut cursor = 0usize;
    for (at, xml) in insertions {
        out.push_str(&original_xml[cursor..at]);
        out.push_str(&xml);
        cursor = at;
    }
    out.push_str(&original_xml[cursor..]);

    Ok((out, AugmentReport { skipped }))
}

/// Mint an id of the form `userlex-<n>` guaranteed not to collide with any `id="..."` value
/// already present in the document (tracked in `used`, seeded by [`collect_existing_ids`] and
/// updated as each fresh id is minted, so two calls within the same [`augment_xml`] run never
/// collide with each other either).
fn fresh_id(used: &mut HashSet<String>, counter: &mut u64) -> String {
    loop {
        let candidate = format!("userlex-{counter}");
        *counter += 1;
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
}

/// Every `id="..."` attribute value anywhere in the document (regardless of element), for
/// collision-avoidance when minting fresh ids. Boundary-aware (the char before the match must be
/// whitespace or absent) so a longer attribute name ending in `id` (there are none in the HC DTD,
/// but this stays robust regardless) can't be mistaken for a plain `id=` match.
fn collect_existing_ids(xml: &str) -> HashSet<String> {
    let mut ids = HashSet::new();
    let needle = "id=\"";
    let mut from = 0usize;
    while let Some(rel) = xml[from..].find(needle) {
        let pos = from + rel;
        let boundary_ok = pos == 0 || xml.as_bytes()[pos - 1].is_ascii_whitespace();
        if boundary_ok {
            let val_start = pos + needle.len();
            if let Some(end_rel) = xml[val_start..].find('"') {
                let val_end = val_start + end_rel;
                ids.insert(xml[val_start..val_end].to_string());
                from = val_end;
                continue;
            }
        }
        from = pos + needle.len();
    }
    ids
}

/// Find the `[start, end)` byte span of the first `<tag ...>...</tag>` (or self-closing
/// `<tag .../>`) element at or after `from`, boundary-aware so e.g. searching for `"Allomorph"`
/// does not match `"Allomorphs"`. Assumes `tag` never nests within itself in the HC schema (true
/// of every element this module searches for).
fn find_element_span(xml: &str, tag: &str, from: usize) -> Option<(usize, usize)> {
    let open_needle = format!("<{tag}");
    let mut search_from = from;
    loop {
        let rel = xml[search_from..].find(&open_needle)?;
        let start = search_from + rel;
        let after_idx = start + open_needle.len();
        let after = xml.as_bytes().get(after_idx).copied();
        let boundary_ok = matches!(
            after,
            Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') | Some(b'>') | Some(b'/')
        );
        if !boundary_ok {
            search_from = start + 1;
            continue;
        }
        let gt = xml[start..].find('>')? + start;
        if xml.as_bytes()[gt - 1] == b'/' {
            return Some((start, gt + 1));
        }
        let content_start = gt + 1;
        let close_needle = format!("</{tag}>");
        let close_rel = xml[content_start..].find(&close_needle)?;
        let end = content_start + close_rel + close_needle.len();
        return Some((start, end));
    }
}

/// [`find_element_span`], further filtered to the element whose opening tag's `id="..."` equals
/// `id_value` — how the exemplar `<LexicalEntry>` is located by `MorphemeInfo::xml_key`.
fn find_tagged_element_span(xml: &str, tag: &str, id_value: &str) -> Option<(usize, usize)> {
    let mut from = 0usize;
    loop {
        let (start, end) = find_element_span(xml, tag, from)?;
        let open_end = xml[start..end].find('>')? + start + 1;
        if get_attr_value(&xml[start..open_end], "id").as_deref() == Some(id_value) {
            return Some((start, end));
        }
        from = end;
    }
}

/// The value of a double-quoted `name="..."` attribute within `tag` (the element's opening-tag
/// text), boundary-aware (rejects a match where `name` is a suffix of a longer attribute name).
fn get_attr_value(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let mut search_from = 0usize;
    loop {
        let rel = tag[search_from..].find(&needle)?;
        let pos = search_from + rel;
        let boundary_ok = pos == 0
            || tag.as_bytes()[pos - 1].is_ascii_whitespace()
            || tag.as_bytes()[pos - 1] == b'<';
        if boundary_ok {
            let val_start = pos + needle.len();
            let val_end = tag[val_start..].find('"')? + val_start;
            return Some(tag[val_start..val_end].to_string());
        }
        search_from = pos + 1;
    }
}

/// Replace the value of a double-quoted `name="..."` attribute within `tag` (an opening-tag
/// slice), boundary-aware like [`get_attr_value`]. Returns `tag` unchanged if the attribute isn't
/// present.
fn replace_attr_value(tag: &str, name: &str, new_value: &str) -> String {
    let needle = format!("{name}=\"");
    let mut search_from = 0usize;
    loop {
        let Some(rel) = tag[search_from..].find(&needle) else {
            return tag.to_string();
        };
        let pos = search_from + rel;
        let boundary_ok = pos == 0
            || tag.as_bytes()[pos - 1].is_ascii_whitespace()
            || tag.as_bytes()[pos - 1] == b'<';
        if !boundary_ok {
            search_from = pos + 1;
            continue;
        }
        let val_start = pos + needle.len();
        let Some(val_end_rel) = tag[val_start..].find('"') else {
            return tag.to_string();
        };
        let val_end = val_start + val_end_rel;
        let mut out = String::with_capacity(tag.len());
        out.push_str(&tag[..val_start]);
        out.push_str(&escape_xml(new_value));
        out.push_str(&tag[val_end..]);
        return out;
    }
}

/// Replace the text content of the first `<tag>...</tag>` in `xml` with `new_text` (XML-escaped).
/// `None` if `tag` isn't found or is self-closing (no content slot to replace).
fn replace_element_text(xml: &str, tag: &str, new_text: &str) -> Option<String> {
    let (start, end) = find_element_span(xml, tag, 0)?;
    let gt = xml[start..end].find('>')? + start;
    if xml.as_bytes()[gt - 1] == b'/' {
        return None;
    }
    let content_start = gt + 1;
    let close_needle = format!("</{tag}>");
    let content_end = end - close_needle.len();
    let mut out = String::with_capacity(xml.len());
    out.push_str(&xml[..content_start]);
    out.push_str(&escape_xml(new_text));
    out.push_str(&xml[content_end..]);
    Some(out)
}

/// Set (or, if absent, inject) `<Property name="ID">` to `marker`'s text, matching the DEMO's
/// `user:<id>` recognition convention. Child element order is irrelevant to `hc_grammar::load`'s
/// own DOM (`Node::child`/`text_of` search by tag name, not position), so injecting the fresh
/// `<Properties>` block just before `</LexicalEntry>` is always schema-safe for THIS loader.
fn set_user_id_property(entry_xml: &str, marker: &str) -> String {
    if let Some(pos) = entry_xml.find("name=\"ID\"") {
        if let Some(gt_rel) = entry_xml[pos..].find('>') {
            let content_start = pos + gt_rel + 1;
            if let Some(close_rel) = entry_xml[content_start..].find("</Property>") {
                let content_end = content_start + close_rel;
                let mut out = String::with_capacity(entry_xml.len());
                out.push_str(&entry_xml[..content_start]);
                out.push_str(&escape_xml(marker));
                out.push_str(&entry_xml[content_end..]);
                return out;
            }
        }
    }
    match entry_xml.rfind("</LexicalEntry>") {
        Some(pos) => {
            let mut out = String::with_capacity(entry_xml.len() + 64);
            out.push_str(&entry_xml[..pos]);
            out.push_str("<Properties><Property name=\"ID\">");
            out.push_str(&escape_xml(marker));
            out.push_str("</Property></Properties>");
            out.push_str(&entry_xml[pos..]);
            out
        }
        None => entry_xml.to_string(),
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Build the patched clone of one exemplar `<LexicalEntry>` element: fresh entry/allomorph ids,
/// reduced to a single allomorph, new `<PhoneticShape>`/`<Gloss>`, and the `user:<id>` ID
/// property.
fn build_clone_xml(
    exemplar_span: &str,
    new_entry_id: &str,
    new_allo_id: &str,
    shape: &str,
    gloss: &str,
    marker: &str,
) -> Result<String, String> {
    let open_end = exemplar_span
        .find('>')
        .ok_or_else(|| "malformed <LexicalEntry> (no '>')".to_string())?
        + 1;
    let mut out = replace_attr_value(&exemplar_span[..open_end], "id", new_entry_id);
    out.push_str(&exemplar_span[open_end..]);

    let (allos_start, allos_end) = find_element_span(&out, "Allomorphs", 0)
        .ok_or_else(|| "exemplar has no <Allomorphs> block".to_string())?;
    let allos_block = out[allos_start..allos_end].to_string();
    let allos_open_end = allos_block
        .find('>')
        .ok_or_else(|| "malformed <Allomorphs>".to_string())?
        + 1;
    let allos_close_start = allos_block
        .len()
        .checked_sub("</Allomorphs>".len())
        .ok_or_else(|| "malformed <Allomorphs>".to_string())?;
    let (first_start, first_end) = find_element_span(&allos_block, "Allomorph", allos_open_end)
        .ok_or_else(|| "exemplar has no <Allomorph> element".to_string())?;
    let first_allo = allos_block[first_start..first_end].to_string();
    let allo_open_end = first_allo
        .find('>')
        .ok_or_else(|| "malformed <Allomorph>".to_string())?
        + 1;
    let mut new_allo = replace_attr_value(&first_allo[..allo_open_end], "id", new_allo_id);
    new_allo.push_str(&first_allo[allo_open_end..]);
    new_allo = replace_element_text(&new_allo, "PhoneticShape", shape)
        .ok_or_else(|| "exemplar allomorph has no <PhoneticShape>".to_string())?;

    let mut new_allos_block = String::with_capacity(allos_block.len());
    new_allos_block.push_str(&allos_block[..allos_open_end]);
    new_allos_block.push_str(&new_allo);
    new_allos_block.push_str(&allos_block[allos_close_start..]);

    let mut out2 = String::with_capacity(out.len());
    out2.push_str(&out[..allos_start]);
    out2.push_str(&new_allos_block);
    out2.push_str(&out[allos_end..]);
    out = out2;

    out = replace_element_text(&out, "Gloss", gloss).unwrap_or(out);
    out = set_user_id_property(&out, marker);

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_xml_handles_ampersand_first() {
        assert_eq!(escape_xml("a & b < c > d"), "a &amp; b &lt; c &gt; d");
    }

    #[test]
    fn find_element_span_does_not_confuse_allomorph_with_allomorphs() {
        let xml = "<Allomorphs><Allomorph id=\"a1\"><PhoneticShape>x</PhoneticShape></Allomorph></Allomorphs>";
        let (start, end) = find_element_span(xml, "Allomorph", 12).unwrap();
        assert_eq!(&xml[start..end], "<Allomorph id=\"a1\"><PhoneticShape>x</PhoneticShape></Allomorph>");
    }

    #[test]
    fn replace_attr_value_only_touches_the_named_attribute() {
        let tag = "<LexicalEntry id=\"e1\" partOfSpeech=\"posN\">";
        let out = replace_attr_value(tag, "id", "userlex-1");
        assert_eq!(out, "<LexicalEntry id=\"userlex-1\" partOfSpeech=\"posN\">");
    }

    #[test]
    fn set_user_id_property_replaces_an_existing_property() {
        let entry = "<LexicalEntry id=\"e1\"><Gloss>x</Gloss><Properties><Property name=\"ID\">101</Property></Properties></LexicalEntry>";
        let out = set_user_id_property(entry, "user:abc-123");
        assert!(out.contains("<Property name=\"ID\">user:abc-123</Property>"));
        assert!(!out.contains(">101<"), "the stale FieldWorks hvo must be gone: {out}");
        // Exactly one Properties block -- no duplicate injected alongside the existing one.
        assert_eq!(out.matches("<Properties>").count(), 1);
    }

    #[test]
    fn set_user_id_property_injects_when_absent() {
        let entry = "<LexicalEntry id=\"e1\"><Gloss>x</Gloss></LexicalEntry>";
        let out = set_user_id_property(entry, "user:abc-123");
        assert!(out.contains("<Property name=\"ID\">user:abc-123</Property>"));
        assert!(out.ends_with("</LexicalEntry>"));
    }
}
