//! The `project` snapshot section: identifying metadata that isn't part of the
//! phonology/morphology/lexicon proper.

use serde::{Deserialize, Serialize};

/// The `project` snapshot section.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// ← `LcmCache.ProjectId.Name` (`HCLoader.LoadLanguage`, HCLoader.cs:166,
    /// `m_language = new Language { Name = m_cache.ProjectId.Name }`).
    pub name: String,
    /// Vernacular writing system tags (ICU locale ids, e.g. `"sen"`), default first.
    /// ← `LangProject.CurrentVernacularWritingSystems` (the `VernacularDefaultWritingSystem`
    /// that `HCLoader` reads throughout, e.g. HCLoader.cs:542/584/811, is this list's first
    /// entry). Every writing-system tag used as a key in a [`crate::common::WsForm`] elsewhere
    /// in this snapshot is expected to appear in this list or `analysis_writing_systems`.
    pub vernacular_writing_systems: Vec<String>,
    /// Analysis writing system tags, default first. ← `LangProject.
    /// CurrentAnalysisWritingSystems` (the `BestAnalysisAlternative` that `HCLoader` reads for
    /// names/glosses/abbreviations throughout is resolved against this list).
    pub analysis_writing_systems: Vec<String>,
}
