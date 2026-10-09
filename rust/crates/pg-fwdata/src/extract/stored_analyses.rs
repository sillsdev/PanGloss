use crate::xml::{RawGraph, Record};
use crate::{StoredAnalysis, StoredAnalysisMorph};

pub(crate) fn extract(
    graph: &RawGraph,
    vernacular_writing_systems: &[String],
) -> Vec<StoredAnalysis> {
    let mut wordforms: Vec<_> = graph.by_class("WfiWordform").collect();
    wordforms.sort_by(|left, right| left.guid.cmp(&right.guid));
    let mut analyses = Vec::new();
    for wordform in wordforms {
        let form = preferred_form(&wordform.node, "Form", vernacular_writing_systems);
        let analysis_guids = wordform.node.objsur_list("Analyses");
        for analysis_guid in analysis_guids {
            let mut issue = None;
            let morphs = match graph.get(&analysis_guid) {
                Some(analysis) if analysis.class == "WfiAnalysis" => {
                    extract_morphs(graph, analysis, vernacular_writing_systems, &mut issue)
                }
                Some(analysis) => {
                    issue = Some(format!(
                        "wordform analysis reference {analysis_guid} resolves to {}, not WfiAnalysis",
                        analysis.class
                    ));
                    Vec::new()
                }
                None => {
                    issue = Some(format!(
                        "wordform analysis reference {analysis_guid} has no source object"
                    ));
                    Vec::new()
                }
            };
            if form.is_none() {
                issue.get_or_insert_with(|| "wordform has no usable Form".to_string());
            }
            analyses.push(StoredAnalysis {
                wordform_guid: canonical_guid(&wordform.guid),
                wordform: form.clone(),
                analysis_guid: canonical_guid(&analysis_guid),
                morphs,
                issue,
            });
        }
    }
    analyses.sort_by(|left, right| {
        (&left.wordform, &left.wordform_guid, &left.analysis_guid).cmp(&(
            &right.wordform,
            &right.wordform_guid,
            &right.analysis_guid,
        ))
    });
    analyses
}

fn extract_morphs(
    graph: &RawGraph,
    analysis: &Record,
    vernacular_writing_systems: &[String],
    issue: &mut Option<String>,
) -> Vec<StoredAnalysisMorph> {
    let mut morphs = Vec::new();
    for bundle_guid in analysis.node.objsur_list("MorphBundles") {
        let bundle = match graph.get(&bundle_guid) {
            Some(bundle) if bundle.class == "WfiMorphBundle" => bundle,
            Some(bundle) => {
                issue.get_or_insert_with(|| {
                    format!(
                        "morph-bundle reference {bundle_guid} resolves to {}, not WfiMorphBundle",
                        bundle.class
                    )
                });
                morphs.push(StoredAnalysisMorph {
                    allomorph_guid: None,
                    msa_guid: None,
                    inflection_type_guid: None,
                    form: None,
                });
                continue;
            }
            None => {
                issue.get_or_insert_with(|| {
                    format!("morph-bundle reference {bundle_guid} has no source object")
                });
                morphs.push(StoredAnalysisMorph {
                    allomorph_guid: None,
                    msa_guid: None,
                    inflection_type_guid: None,
                    form: None,
                });
                continue;
            }
        };
        let allomorph_guid = bundle.node.objsur_one("Morph");
        let msa_guid = bundle.node.objsur_one("Msa");
        let inflection_type_guid = bundle.node.objsur_one("InflType");
        if allomorph_guid.is_none() || msa_guid.is_none() {
            issue.get_or_insert_with(|| {
                format!("morph bundle {bundle_guid} has no Morph or Msa reference")
            });
        }
        let form = preferred_form(&bundle.node, "Form", vernacular_writing_systems).or_else(|| {
            allomorph_guid.as_deref().and_then(|guid| {
                graph.get(guid).and_then(|source| {
                    preferred_form(&source.node, "Form", vernacular_writing_systems)
                })
            })
        });
        morphs.push(StoredAnalysisMorph {
            allomorph_guid: allomorph_guid.map(|guid| canonical_guid(&guid)),
            msa_guid: msa_guid.map(|guid| canonical_guid(&guid)),
            inflection_type_guid: inflection_type_guid.map(|guid| canonical_guid(&guid)),
            form,
        });
    }
    morphs
}

fn preferred_form(
    node: &crate::node::Node,
    field: &str,
    preferred_writing_systems: &[String],
) -> Option<String> {
    let forms = node.ws_forms(field);
    preferred_writing_systems
        .iter()
        .find_map(|writing_system| {
            forms
                .iter()
                .find(|form| form.ws == *writing_system)
                .map(|form| form.form.clone())
        })
        .or_else(|| forms.first().map(|form| form.form.clone()))
}

fn canonical_guid(guid: &str) -> String {
    pg_snapshot::canonical_guid(guid).unwrap_or_else(|| guid.to_ascii_lowercase())
}
