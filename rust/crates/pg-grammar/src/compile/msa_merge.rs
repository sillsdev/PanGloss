//! FieldWorks `HCLoader.RepresentativeMSA` / `UniqueMSAs` (HCLoader.cs:675-704) under `<HC><MergeMSAs>`.

use std::cell::RefCell;
use std::collections::BTreeMap;

use hashbrown::HashMap;

use pg_snapshot::lexicon::{LexEntry, Msa};
use pg_snapshot::Snapshot;

use crate::GrammarError;

pub(crate) struct MsaMerge<'a> {
    enabled: bool,
    owner: HashMap<&'a str, (&'a LexEntry, &'a Msa)>,
    equal_to: HashMap<&'a str, &'a [String]>,
    /// HCLoader's `m_representativeMSAs`, holding only the lookups whose answer was another MSA.
    representatives: RefCell<BTreeMap<String, String>>,
}

impl<'a> MsaMerge<'a> {
    /// Errs when merging is on but a snapshot without MSA equality has an entry with two same-class MSAs.
    pub(crate) fn new(snapshot: &'a Snapshot) -> Result<Self, GrammarError> {
        let enabled = snapshot.morphology.parser_parameters.merge_msas;
        let mut owner = HashMap::new();
        for entry in &snapshot.lexicon.entries {
            for msa in &entry.msas {
                owner.insert(msa.guid(), (entry, msa));
            }
        }
        let mut equal_to = HashMap::new();
        match &snapshot.lexicon.msa_equality {
            Some(equalities) => {
                for equal in equalities {
                    equal_to.insert(equal.msa.as_str(), equal.equal_to.as_slice());
                }
            }
            None if enabled => {
                let undecidable: Vec<&str> = snapshot
                    .lexicon
                    .entries
                    .iter()
                    .filter(|entry| {
                        entry.msas.iter().enumerate().any(|(i, msa)| {
                            entry.msas[..i]
                                .iter()
                                .any(|earlier| earlier.fw_class() == msa.fw_class())
                        })
                    })
                    .map(|entry| entry.guid.as_str())
                    .collect();
                if !undecidable.is_empty() {
                    return Err(GrammarError::Unsupported(format!(
                        "<HC><MergeMSAs> is on, but this snapshot was written without MSA \
                         equality, so whether FieldWorks merges the same-class MSAs of {} \
                         entr{} ({}) cannot be decided; re-import the project",
                        undecidable.len(),
                        if undecidable.len() == 1 { "y" } else { "ies" },
                        undecidable.join(", ")
                    )));
                }
            }
            None => {}
        }
        Ok(Self {
            enabled,
            owner,
            equal_to,
            representatives: RefCell::new(BTreeMap::new()),
        })
    }

    /// `RepresentativeMSA(msa)`: an MSA no entry owns, or any MSA while merging is off, is its own representative.
    pub(crate) fn representative(&self, msa: &'a Msa) -> &'a Msa {
        self.representative_of(msa.guid())
            .map_or(msa, |(_, rep)| rep)
    }

    /// As [`Self::representative`], for an MSA named only by guid.
    pub(crate) fn representative_guid<'g>(&self, guid: &'g str) -> &'g str
    where
        'a: 'g,
    {
        self.representative_of(guid)
            .map_or(guid, |(_, rep)| rep.guid())
    }

    fn representative_of(&self, guid: &str) -> Option<(&'a LexEntry, &'a Msa)> {
        if !self.enabled {
            return None;
        }
        let &(entry, msa) = self.owner.get(guid)?;
        let equal_to = self.equal_to.get(guid).copied().unwrap_or(&[]);
        let representative = entry.msas.iter().find(|candidate| {
            candidate.fw_class() == msa.fw_class()
                && (candidate.guid() == guid || equal_to.iter().any(|g| g == candidate.guid()))
                && entry
                    .senses
                    .iter()
                    .any(|sense| sense.msa.as_deref() == Some(candidate.guid()))
        })?;
        if representative.guid() != guid {
            self.representatives
                .borrow_mut()
                .insert(guid.to_string(), representative.guid().to_string());
        }
        Some((entry, representative))
    }

    /// `UniqueMSAs(msas)`: each MSA's representative, first occurrences kept, in order.
    pub(crate) fn unique(&self, msas: impl IntoIterator<Item = &'a Msa>) -> Vec<&'a Msa> {
        let mut out: Vec<&'a Msa> = Vec::new();
        for msa in msas {
            let rep = self.representative(msa);
            if !out.iter().any(|seen| seen.guid() == rep.guid()) {
                out.push(rep);
            }
        }
        out
    }

    /// `UniqueMSAs` over MSAs named by guid.
    pub(crate) fn unique_guids<'g>(
        &self,
        guids: impl IntoIterator<Item = &'g String>,
    ) -> Vec<&'g str>
    where
        'a: 'g,
    {
        let mut out: Vec<&'g str> = Vec::new();
        for guid in guids {
            let rep = self.representative_guid(guid.as_str());
            if !out.contains(&rep) {
                out.push(rep);
            }
        }
        out
    }

    /// The representative an earlier lookup loaded in `guid`'s place, if it was another MSA.
    pub(crate) fn merged_into(&self, guid: &str) -> Option<String> {
        self.representatives.borrow().get(guid).cloned()
    }

    pub(crate) fn into_representatives(self) -> BTreeMap<String, String> {
        self.representatives.into_inner()
    }
}
