//! Exact grammatical class signatures for runtime-supplied ordinary stems.
//!
//! Identity is `sig_` plus the lowercase hexadecimal SHA-256 digest of a canonical UTF-8 JSON
//! object containing authored POS, syntactic-feature/value, and MPR IDs. Object fields have the
//! fixed order `pos`, `features`, `mpr`; feature rows and symbolic/MPR ID arrays are sorted by
//! authored ID. Display labels and grammar-local dense IDs are deliberately absent.

use pg_featstruct::{FeatureStruct, FeatureValue, FsId};
use pg_grammar::model::{Grammar, MprId, SynFeatureKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SignatureId(String);

impl SignatureId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoredRef {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalFeature {
    pub feature: AuthoredRef,
    pub value: CanonicalFeatureValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CanonicalFeatureValue {
    Symbolic(Vec<AuthoredRef>),
    Complex(Vec<CanonicalFeature>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassSignature {
    pub id: SignatureId,
    pub pos: Option<AuthoredRef>,
    pub features: Vec<CanonicalFeature>,
    pub mpr: Vec<AuthoredRef>,
    pub canonical_encoding: String,
    pub entry_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSignature {
    pub signature: ClassSignature,
    pub syn_fs: FsId,
    pub mpr: pg_grammar::model::MprSet,
}

#[derive(Debug, Clone, Default)]
pub struct ClassCatalog {
    signatures: Vec<ClassSignature>,
    resolved: Vec<ResolvedSignature>,
}

#[derive(Serialize)]
struct Identity<'a> {
    pos: Option<&'a str>,
    features: Vec<IdentityFeature<'a>>,
    mpr: Vec<&'a str>,
}

#[derive(Serialize)]
struct IdentityFeature<'a> {
    id: &'a str,
    #[serde(flatten)]
    value: IdentityValue<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum IdentityValue<'a> {
    Symbolic(Vec<&'a str>),
    Complex(Vec<IdentityFeature<'a>>),
}

impl ClassCatalog {
    pub fn from_grammar(grammar: &Grammar) -> Result<Self, String> {
        let mut resolved: Vec<ResolvedSignature> = Vec::new();
        for entry in &grammar.entries {
            if entry.partial
                || !entry.allomorphs.iter().any(|a| {
                    !a.is_bound
                        && !a.is_pattern
                        && a.environments.is_empty()
                        && a.stem_name.is_none()
                        && a.co_occurrence.is_empty()
                })
            {
                continue;
            }
            let fs = grammar.fs_interner.get(entry.syn_fs);
            let features = resolve_features(grammar, fs)?;
            let pos = resolve_pos(grammar, fs)?;
            let mut mpr = Vec::new();
            for bit in 0..grammar.mpr_features.len() {
                if entry.mpr.contains(MprId(bit as u8)) {
                    let def = grammar
                        .mpr_feature(MprId(bit as u8))
                        .ok_or_else(|| format!("MPR id {bit} has no authored identity"))?;
                    mpr.push(AuthoredRef {
                        id: def.xml_id.clone(),
                        label: def.name.clone(),
                    });
                }
            }
            mpr.sort_by(|a, b| a.id.cmp(&b.id));
            let canonical_encoding = encode_identity(pos.as_ref(), &features, &mpr)?;
            let id = signature_id(&canonical_encoding);
            if let Some(existing) = resolved
                .iter_mut()
                .find(|r| r.signature.canonical_encoding == canonical_encoding)
            {
                existing.signature.entry_count += 1;
                continue;
            }
            if resolved.iter().any(|r| r.signature.id == id) {
                return Err(format!(
                    "signature digest collision for {canonical_encoding}"
                ));
            }
            let signature = ClassSignature {
                id,
                pos,
                features,
                mpr,
                canonical_encoding,
                entry_count: 1,
            };
            resolved.push(ResolvedSignature {
                signature,
                syn_fs: entry.syn_fs,
                mpr: entry.mpr,
            });
        }
        resolved.sort_by(|a, b| a.signature.id.cmp(&b.signature.id));
        let signatures = resolved.iter().map(|r| r.signature.clone()).collect();
        Ok(Self {
            signatures,
            resolved,
        })
    }

    pub fn len(&self) -> usize {
        self.signatures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.signatures.is_empty()
    }

    pub fn signatures(&self) -> &[ClassSignature] {
        &self.signatures
    }

    pub fn resolved(&self, id: &SignatureId) -> Option<&ResolvedSignature> {
        self.resolved.iter().find(|r| &r.signature.id == id)
    }
}

fn resolve_pos(grammar: &Grammar, fs: &FeatureStruct) -> Result<Option<AuthoredRef>, String> {
    let Some(FeatureValue::Symbolic(bits)) = fs.get(grammar.syn_features.pos) else {
        return Ok(None);
    };
    let SynFeatureKind::Symbolic { symbols, .. } =
        &grammar.syn_features.features[grammar.syn_features.pos.0 as usize].kind
    else {
        return Err("POS feature is not symbolic".into());
    };
    let Some(index) = bits.first() else {
        return Ok(None);
    };
    Ok(symbols.get(index as usize).map(|(id, label)| AuthoredRef {
        id: id.clone(),
        label: label.clone(),
    }))
}

fn resolve_features(
    grammar: &Grammar,
    fs: &FeatureStruct,
) -> Result<Vec<CanonicalFeature>, String> {
    let mut out = Vec::new();
    for (feat_id, value) in fs.entries() {
        let def = grammar
            .syn_features
            .features
            .get(feat_id.0 as usize)
            .ok_or_else(|| format!("feature id {} has no definition", feat_id.0))?;
        let value = match (value, &def.kind) {
            (FeatureValue::Symbolic(bits), SynFeatureKind::Symbolic { symbols, .. }) => {
                let mut values: Vec<AuthoredRef> = symbols
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| bits.get(*i as u32))
                    .map(|(_, (id, label))| AuthoredRef {
                        id: id.clone(),
                        label: label.clone(),
                    })
                    .collect();
                values.sort_by(|a, b| a.id.cmp(&b.id));
                CanonicalFeatureValue::Symbolic(values)
            }
            (FeatureValue::Complex(nested), SynFeatureKind::Complex) => {
                CanonicalFeatureValue::Complex(resolve_features(grammar, nested)?)
            }
            _ => return Err(format!("feature {} value kind mismatch", def.xml_id)),
        };
        out.push(CanonicalFeature {
            feature: AuthoredRef {
                id: def.xml_id.clone(),
                label: def.name.clone(),
            },
            value,
        });
    }
    out.sort_by(|a, b| a.feature.id.cmp(&b.feature.id));
    Ok(out)
}

fn encode_identity(
    pos: Option<&AuthoredRef>,
    features: &[CanonicalFeature],
    mpr: &[AuthoredRef],
) -> Result<String, String> {
    fn feature(f: &CanonicalFeature) -> IdentityFeature<'_> {
        IdentityFeature {
            id: &f.feature.id,
            value: match &f.value {
                CanonicalFeatureValue::Symbolic(values) => {
                    IdentityValue::Symbolic(values.iter().map(|v| v.id.as_str()).collect())
                }
                CanonicalFeatureValue::Complex(values) => {
                    IdentityValue::Complex(values.iter().map(feature).collect())
                }
            },
        }
    }
    serde_json::to_string(&Identity {
        pos: pos.map(|p| p.id.as_str()),
        features: features.iter().map(feature).collect(),
        mpr: mpr.iter().map(|m| m.id.as_str()).collect(),
    })
    .map_err(|e| e.to_string())
}

fn signature_id(canonical: &str) -> SignatureId {
    let digest = Sha256::digest(canonical.as_bytes());
    let mut value = String::with_capacity(4 + digest.len() * 2);
    value.push_str("sig_");
    for byte in digest {
        use std::fmt::Write;
        write!(&mut value, "{byte:02x}").expect("writing to String cannot fail");
    }
    SignatureId(value)
}
