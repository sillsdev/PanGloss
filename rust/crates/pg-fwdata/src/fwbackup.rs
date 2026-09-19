//! `.fwbackup` zip support: the embedded `.fwdata` plus its `WritingSystemStore/*.ldml` exemplars.

use std::io::Read;
use std::path::Path;

use unicode_normalization::UnicodeNormalization;

use pg_snapshot::{InventoryDelta, Snapshot};

use crate::{extract, xml, ImportError, ImportReport};

/// The embedded `.fwdata` graph, the archive member's stem, and each exemplar LDML as (name, text).
type BackupContents = (xml::RawGraph, String, Vec<(String, String)>);

/// Reads the embedded `.fwdata` graph plus exemplar LDML, shared by `import_fwbackup`/`import_fwbackup_measured`.
fn read_backup(path: &Path) -> Result<BackupContents, ImportError> {
    let file = std::fs::File::open(path).map_err(ImportError::Io)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ImportError::Backup(format!("{}: {e}", path.display())))?;

    let mut fwdata_name: Option<String> = None;
    let mut ldml: Vec<(String, String)> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ImportError::Backup(e.to_string()))?;
        let name = entry.name().to_string();
        if name.ends_with(".fwdata") && !name.contains('/') {
            fwdata_name = Some(name);
        } else if let Some(tag) = name
            .strip_prefix("WritingSystemStore/")
            .and_then(|n| n.strip_suffix(".ldml"))
            .filter(|n| !n.contains('/'))
        {
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .map_err(|e| ImportError::Backup(format!("{name}: {e}")))?;
            ldml.push((tag.to_string(), text));
        }
    }
    let fwdata_name = fwdata_name.ok_or_else(|| {
        ImportError::Backup(format!("{}: no top-level .fwdata entry", path.display()))
    })?;

    let graph = {
        let entry = archive
            .by_name(&fwdata_name)
            .map_err(|e| ImportError::Backup(e.to_string()))?;
        xml::parse_fwdata_reader(std::io::BufReader::new(entry))?
    };
    let stem = crate::file_stem(Path::new(&fwdata_name));
    Ok((graph, stem, ldml))
}

/// Applies the default vernacular writing system's LDML exemplar characters onto `snapshot`, if present -- also called for a plain `.fwdata` import against its sibling `WritingSystemStore/` (`lib.rs::read_sibling_writing_system_store`), not only from an embedded backup copy.
pub(crate) fn apply_exemplars(snapshot: &mut Snapshot, ldml: &[(String, String)]) {
    if let Some(default_ws) = snapshot.project.vernacular_writing_systems.first().cloned() {
        if let Some((_, text)) = ldml.iter().find(|(tag, _)| *tag == default_ws) {
            snapshot.project.exemplar_characters = exemplar_characters_from_ldml(text);
        }
    }
}

pub(crate) fn import_fwbackup(path: &Path) -> Result<(Snapshot, ImportReport), ImportError> {
    let (graph, stem, ldml) = read_backup(path)?;
    let (mut snapshot, warnings) = extract::extract(&graph, &stem)?;
    let provenance = snapshot.conversion_provenance.clone();
    apply_exemplars(&mut snapshot, &ldml);
    Ok((
        snapshot,
        ImportReport {
            warnings,
            provenance,
        },
    ))
}

/// As [`import_fwbackup`], but also returns the [`InventoryDelta`]; panics if the recorder's own invariants are violated rather than return an untrustworthy measurement.
pub(crate) fn import_fwbackup_measured(
    path: &Path,
) -> Result<(Snapshot, ImportReport, InventoryDelta), ImportError> {
    let (graph, stem, ldml) = read_backup(path)?;
    let (mut snapshot, warnings, recorder) = extract::extract_recording(&graph, &stem)?;
    let provenance = snapshot.conversion_provenance.clone();
    apply_exemplars(&mut snapshot, &ldml);
    if let Err(violation) = recorder.check_invariants() {
        panic!("import_fwbackup_measured: selection recorder invariant violated: {violation}");
    }
    let (inventory, issues) = recorder.finish();
    Ok((
        snapshot,
        ImportReport {
            warnings,
            provenance,
        },
        InventoryDelta::from_stage(inventory, issues),
    ))
}

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
        assert!(!ex
            .iter()
            .any(|s| s.contains('{') || s.contains('}') || s.contains('[')));
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
