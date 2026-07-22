use crate::{ClassCatalog, SignatureId};
use chrono::NaiveDateTime;
#[cfg(not(target_arch = "wasm32"))]
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredError {
    pub code: String,
    pub message: String,
    pub details: serde_json::Value,
}
fn err(code: &str, message: &str) -> StructuredError {
    StructuredError {
        code: code.into(),
        message: message.into(),
        details: serde_json::Value::Null,
    }
}
fn err_details(code: &str, message: &str, details: serde_json::Value) -> StructuredError {
    StructuredError {
        code: code.into(),
        message: message.into(),
        details,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct EntryId(String);
impl EntryId {
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(format!("pgl_{}", b64(&bytes)))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn parse(value: &str) -> Result<Self, StructuredError> {
        decode_id(value).map(|_| Self(value.into()))
    }
    pub fn to_dotnet_guid_bytes(&self) -> Result<[u8; 16], StructuredError> {
        let mut b = decode_id(&self.0)?;
        b[0..4].reverse();
        b[4..6].reverse();
        b[6..8].reverse();
        Ok(b)
    }
    pub fn to_dotnet_guid_string(&self) -> Result<String, StructuredError> {
        let b = decode_id(&self.0)?;
        Ok(format!("{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",b[0],b[1],b[2],b[3],b[4],b[5],b[6],b[7],b[8],b[9],b[10],b[11],b[12],b[13],b[14],b[15]))
    }
    pub fn from_dotnet_guid_string(s: &str) -> Result<Self, StructuredError> {
        let bytes = s.as_bytes();
        if bytes.len() != 36
            || [8, 13, 18, 23].iter().any(|&i| bytes[i] != b'-')
            || bytes
                .iter()
                .enumerate()
                .any(|(i, b)| ![8, 13, 18, 23].contains(&i) && !b.is_ascii_hexdigit())
        {
            return Err(err("invalid_guid", "expected D-format GUID"));
        }
        let compact = s.replace('-', "");
        let mut b = [0; 16];
        for i in 0..16 {
            b[i] = u8::from_str_radix(&compact[i * 2..i * 2 + 2], 16)
                .map_err(|_| err("invalid_guid", "GUID contains non-hex characters"))?;
        }
        Ok(Self::from_bytes(b))
    }
}
impl<'de> Deserialize<'de> for EntryId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(|e| serde::de::Error::custom(e.message))
    }
}
fn b64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut o = String::new();
    let mut n = 0u32;
    let mut bits = 0;
    for &b in bytes {
        n = (n << 8) | b as u32;
        bits += 8;
        while bits >= 6 {
            bits -= 6;
            o.push(A[((n >> bits) & 63) as usize] as char);
        }
    }
    if bits > 0 {
        o.push(A[((n << (6 - bits)) & 63) as usize] as char)
    }
    o
}
fn decode_id(s: &str) -> Result<[u8; 16], StructuredError> {
    let x = s
        .strip_prefix("pgl_")
        .ok_or_else(|| err("invalid_entry_id", "missing pgl_ prefix"))?;
    if x.len() != 22 {
        return Err(err(
            "invalid_entry_id",
            "entry id must contain 22 base64url characters",
        ));
    }
    let mut out = [0u8; 16];
    let mut oi = 0;
    let mut n = 0u32;
    let mut bits = 0;
    for c in x.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return Err(err("invalid_entry_id", "invalid base64url")),
        };
        n = (n << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            if oi < 16 {
                out[oi] = ((n >> bits) & 255) as u8;
                oi += 1
            }
        }
    }
    if oi != 16 {
        Err(err("invalid_entry_id", "wrong decoded length"))
    } else if b64(&out) != x {
        Err(err("invalid_entry_id", "noncanonical base64url encoding"))
    } else {
        Ok(out)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct LexicalDate(String);
impl LexicalDate {
    pub fn parse(s: &str) -> Result<Self, StructuredError> {
        if NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.3f").is_err() || s.len() != 23 {
            return Err(err("invalid_date", "expected UTC yyyy-MM-dd HH:mm:ss.fff"));
        }
        Ok(Self(s.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de> Deserialize<'de> for LexicalDate {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(|e| serde::de::Error::custom(e.message))
    }
}
pub trait IdSource {
    fn next_128(&mut self) -> Result<[u8; 16], StructuredError>;
}
pub trait Clock {
    fn now(&mut self) -> LexicalDate;
}
impl<T: IdSource + ?Sized> IdSource for Box<T> {
    fn next_128(&mut self) -> Result<[u8; 16], StructuredError> {
        (**self).next_128()
    }
}
impl<T: Clock + ?Sized> Clock for Box<T> {
    fn now(&mut self) -> LexicalDate {
        (**self).now()
    }
}
pub struct OsIdSource;
impl IdSource for OsIdSource {
    fn next_128(&mut self) -> Result<[u8; 16], StructuredError> {
        let mut b = [0; 16];
        getrandom::fill(&mut b).map_err(|e| StructuredError {
            code: "entropy_failure".into(),
            message: e.to_string(),
            details: serde_json::Value::Null,
        })?;
        Ok(b)
    }
}
pub struct UtcClock;
impl Clock for UtcClock {
    fn now(&mut self) -> LexicalDate {
        #[cfg(not(target_arch = "wasm32"))]
        {
            LexicalDate(Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string())
        }
        #[cfg(target_arch = "wasm32")]
        {
            let date = js_sys::Date::new_0();
            LexicalDate(format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
                date.get_utc_full_year(),
                date.get_utc_month() + 1,
                date.get_utc_date(),
                date.get_utc_hours(),
                date.get_utc_minutes(),
                date.get_utc_seconds(),
                date.get_utc_milliseconds(),
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Revision(String);
impl Revision {
    pub(crate) fn new(number: u64) -> Self {
        Self(format!("rev_{number}"))
    }
    pub(crate) fn number(&self) -> u64 {
        self.0
            .strip_prefix("rev_")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum EntryAuthority {
    Supplied,
    SuppliedOverride {
        official_entry_id: String,
        note: Option<String>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ValidationState {
    Active,
    Inactive { diagnostics: Vec<String> },
    Superseded { official_entry_id: String },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationStateKind {
    Active,
    Inactive,
    Superseded,
}
impl ValidationState {
    fn kind(&self) -> ValidationStateKind {
        match self {
            Self::Active => ValidationStateKind::Active,
            Self::Inactive { .. } => ValidationStateKind::Inactive,
            Self::Superseded { .. } => ValidationStateKind::Superseded,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppliedEntry {
    pub id: EntryId,
    pub stem: String,
    pub gloss: String,
    pub signatures: Vec<SignatureId>,
    pub date_created: LexicalDate,
    pub date_modified: LexicalDate,
    pub authority: EntryAuthority,
    pub state: ValidationState,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddRequest {
    pub stem: String,
    pub gloss: String,
    pub signatures: Vec<SignatureId>,
    pub expected_revision: Option<Revision>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRequest {
    pub id: EntryId,
    pub stem: String,
    pub gloss: String,
    pub signatures: Vec<SignatureId>,
    pub expected_revision: Option<Revision>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveRequest {
    pub id: EntryId,
    pub expected_revision: Option<Revision>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetGlossLanguageRequest {
    pub gloss_language: Option<String>,
    pub expected_revision: Option<Revision>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAuthorityRequest {
    pub id: EntryId,
    pub authority: EntryAuthority,
    pub expected_revision: Option<Revision>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedRevision {
    pub expected_revision: Option<Revision>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    pub signature: Option<SignatureId>,
    pub state: Option<ValidationStateKind>,
    pub pos: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationResult<T> {
    pub value: T,
    pub revision: Revision,
    pub changed: bool,
}

type StemValidator = dyn Fn(&str) -> Result<(), String> + Send + Sync;

pub struct SuppliedLexiconStore<I: IdSource, C: Clock> {
    ids: I,
    clock: C,
    entries: BTreeMap<EntryId, SuppliedEntry>,
    gloss_language: Option<String>,
    counter: u64,
    revision: Revision,
    catalog: BTreeMap<SignatureId, Option<String>>,
    stem_validator: Arc<StemValidator>,
}
impl<I: IdSource, C: Clock> SuppliedLexiconStore<I, C> {
    pub fn new<V>(ids: I, clock: C, catalog: &ClassCatalog, stem_validator: V) -> Self
    where
        V: Fn(&str) -> Result<(), String> + Send + Sync + 'static,
    {
        Self {
            ids,
            clock,
            entries: BTreeMap::new(),
            gloss_language: None,
            counter: 0,
            revision: Revision("rev_0".into()),
            catalog: catalog
                .signatures()
                .iter()
                .map(|x| (x.id.clone(), x.pos.as_ref().map(|p| p.id.clone())))
                .collect(),
            stem_validator: Arc::new(stem_validator),
        }
    }
    pub fn revision(&self) -> &Revision {
        &self.revision
    }
    pub fn get(&self, id: &EntryId) -> Option<SuppliedEntry> {
        self.entries.get(id).cloned()
    }
    pub fn list(&self) -> Vec<SuppliedEntry> {
        self.entries.values().cloned().collect()
    }
    fn check(&self, x: &Option<Revision>) -> Result<(), StructuredError> {
        if x.as_ref().is_some_and(|r| r != &self.revision) {
            Err(err_details(
                "revision_conflict",
                "expected revision does not match",
                serde_json::json!({"expected":x,"current":self.revision}),
            ))
        } else {
            Ok(())
        }
    }
    fn validate(
        &self,
        stem: &str,
        gloss: &str,
        sigs: &[SignatureId],
    ) -> Result<Vec<SignatureId>, StructuredError> {
        if stem.is_empty() {
            return Err(err_details(
                "invalid_stem",
                "stem cannot be empty",
                serde_json::json!({"stem":stem}),
            ));
        }
        if sigs.is_empty() {
            return Err(err_details(
                "invalid_signatures",
                "at least one signature is required",
                serde_json::json!({"signatureIds":sigs}),
            ));
        }
        if !gloss.is_empty() && self.gloss_language.is_none() {
            return Err(err(
                "gloss_language_required",
                "set gloss language before adding a gloss",
            ));
        }
        (self.stem_validator)(stem)
            .map_err(|m| err_details("invalid_shape", &m, serde_json::json!({"stem":stem})))?;
        let mut normalized = sigs.to_vec();
        normalized.sort();
        normalized.dedup();
        let unknown: Vec<_> = normalized
            .iter()
            .filter(|id| !self.catalog.contains_key(*id))
            .map(|id| id.as_str())
            .collect();
        if !unknown.is_empty() {
            return Err(err_details(
                "unknown_signature",
                "unknown signature IDs",
                serde_json::json!({"signatureIds":unknown}),
            ));
        }
        Ok(normalized)
    }
    fn bump(&mut self) {
        self.counter += 1;
        self.revision = Revision(format!("rev_{}", self.counter))
    }
    pub fn add(&mut self, r: AddRequest) -> Result<MutationResult<SuppliedEntry>, StructuredError> {
        self.check(&r.expected_revision)?;
        let signatures = self.validate(&r.stem, &r.gloss, &r.signatures)?;
        let id = EntryId::from_bytes(self.ids.next_128()?);
        if self.entries.contains_key(&id) {
            return Err(err("duplicate_entry_id", "generated duplicate entry id"));
        }
        let now = self.clock.now();
        let e = SuppliedEntry {
            id,
            stem: r.stem,
            gloss: r.gloss,
            signatures,
            date_created: now.clone(),
            date_modified: now,
            authority: EntryAuthority::Supplied,
            state: ValidationState::Active,
        };
        self.entries.insert(e.id.clone(), e.clone());
        self.bump();
        Ok(MutationResult {
            value: e,
            revision: self.revision.clone(),
            changed: true,
        })
    }
    pub fn update(
        &mut self,
        r: UpdateRequest,
    ) -> Result<MutationResult<SuppliedEntry>, StructuredError> {
        self.check(&r.expected_revision)?;
        let signatures = self.validate(&r.stem, &r.gloss, &r.signatures)?;
        let old = self
            .entries
            .get(&r.id)
            .cloned()
            .ok_or_else(|| err("entry_not_found", "entry not found"))?;
        if old.stem == r.stem && old.gloss == r.gloss && old.signatures == signatures {
            return Ok(MutationResult {
                value: old,
                revision: self.revision.clone(),
                changed: false,
            });
        }
        let mut e = old;
        e.stem = r.stem;
        e.gloss = r.gloss;
        e.signatures = signatures;
        e.date_modified = self.clock.now();
        self.entries.insert(e.id.clone(), e.clone());
        self.bump();
        Ok(MutationResult {
            value: e,
            revision: self.revision.clone(),
            changed: true,
        })
    }
    pub fn remove(&mut self, r: RemoveRequest) -> Result<MutationResult<bool>, StructuredError> {
        self.check(&r.expected_revision)?;
        let changed = self.entries.remove(&r.id).is_some();
        if changed {
            self.bump()
        }
        Ok(MutationResult {
            value: changed,
            revision: self.revision.clone(),
            changed,
        })
    }
    pub fn clear(&mut self, r: ExpectedRevision) -> Result<MutationResult<usize>, StructuredError> {
        self.check(&r.expected_revision)?;
        let n = self.entries.len();
        if n > 0 {
            self.entries.clear();
            self.bump()
        }
        Ok(MutationResult {
            value: n,
            revision: self.revision.clone(),
            changed: n > 0,
        })
    }
    pub fn set_gloss_language(
        &mut self,
        r: SetGlossLanguageRequest,
    ) -> Result<MutationResult<Option<String>>, StructuredError> {
        self.check(&r.expected_revision)?;
        if r.gloss_language
            .as_ref()
            .is_some_and(|x| x.trim().is_empty())
        {
            return Err(err("invalid_gloss_language", "language cannot be blank"));
        }
        if r.gloss_language.is_none() && self.entries.values().any(|e| !e.gloss.is_empty()) {
            return Err(err(
                "gloss_language_required",
                "cannot clear language while glosses exist",
            ));
        }
        let changed = self.gloss_language != r.gloss_language;
        if changed {
            self.gloss_language = r.gloss_language;
            self.bump()
        }
        Ok(MutationResult {
            value: self.gloss_language.clone(),
            revision: self.revision.clone(),
            changed,
        })
    }
    pub fn set_authority(
        &mut self,
        r: SetAuthorityRequest,
    ) -> Result<MutationResult<SuppliedEntry>, StructuredError> {
        self.check(&r.expected_revision)?;
        let old = self
            .entries
            .get(&r.id)
            .cloned()
            .ok_or_else(|| err("entry_not_found", "entry not found"))?;
        if old.authority == r.authority {
            return Ok(MutationResult {
                value: old,
                revision: self.revision.clone(),
                changed: false,
            });
        }
        let mut e = old;
        e.authority = r.authority;
        e.date_modified = self.clock.now();
        self.entries.insert(e.id.clone(), e.clone());
        self.bump();
        Ok(MutationResult {
            value: e,
            revision: self.revision.clone(),
            changed: true,
        })
    }
    pub fn search(&self, r: &SearchRequest) -> Vec<SuppliedEntry> {
        self.entries
            .values()
            .filter(|e| {
                (r.query.is_empty() || e.stem.contains(&r.query) || e.gloss.contains(&r.query))
                    && r.signature
                        .as_ref()
                        .is_none_or(|s| e.signatures.contains(s))
                    && r.state.as_ref().is_none_or(|s| e.state.kind() == *s)
                    && r.pos.as_ref().is_none_or(|pos| {
                        e.signatures.iter().any(|id| {
                            self.catalog
                                .get(id)
                                .is_some_and(|p| p.as_ref() == Some(pos))
                        })
                    })
            })
            .cloned()
            .collect()
    }
}
