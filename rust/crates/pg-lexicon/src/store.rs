use crate::SignatureId;
use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
        let b = self.to_dotnet_guid_bytes()?;
        Ok(format!("{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",b[0],b[1],b[2],b[3],b[4],b[5],b[6],b[7],b[8],b[9],b[10],b[11],b[12],b[13],b[14],b[15]))
    }
    pub fn from_dotnet_guid_string(s: &str) -> Result<Self, StructuredError> {
        let compact = s.replace('-', "");
        if compact.len() != 32 {
            return Err(err("invalid_guid", "expected D-format GUID"));
        }
        let mut b = [0; 16];
        for i in 0..16 {
            b[i] = u8::from_str_radix(&compact[i * 2..i * 2 + 2], 16)
                .map_err(|_| err("invalid_guid", "GUID contains non-hex characters"))?;
        }
        b[0..4].reverse();
        b[4..6].reverse();
        b[6..8].reverse();
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
        LexicalDate(Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Revision(String);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryAuthority {
    Supplied,
    SuppliedOverride {
        official_entry_id: String,
        note: Option<String>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationState {
    Active,
    Inactive { diagnostics: Vec<String> },
    Superseded { official_entry_id: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct AddRequest {
    pub stem: String,
    pub gloss: String,
    pub signatures: Vec<SignatureId>,
    pub expected_revision: Option<Revision>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRequest {
    pub id: EntryId,
    pub stem: String,
    pub gloss: String,
    pub signatures: Vec<SignatureId>,
    pub expected_revision: Option<Revision>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveRequest {
    pub id: EntryId,
    pub expected_revision: Option<Revision>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetGlossLanguageRequest {
    pub gloss_language: Option<String>,
    pub expected_revision: Option<Revision>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetAuthorityRequest {
    pub id: EntryId,
    pub authority: EntryAuthority,
    pub expected_revision: Option<Revision>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExpectedRevision {
    pub expected_revision: Option<Revision>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub signature: Option<SignatureId>,
    pub state: Option<ValidationState>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationResult<T> {
    pub value: T,
    pub revision: Revision,
    pub changed: bool,
}

pub struct SuppliedLexiconStore<I: IdSource, C: Clock> {
    ids: I,
    clock: C,
    entries: BTreeMap<EntryId, SuppliedEntry>,
    gloss_language: Option<String>,
    counter: u64,
    revision: Revision,
}
impl<I: IdSource, C: Clock> SuppliedLexiconStore<I, C> {
    pub fn new(ids: I, clock: C) -> Self {
        Self {
            ids,
            clock,
            entries: BTreeMap::new(),
            gloss_language: None,
            counter: 0,
            revision: Revision("rev_0".into()),
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
            Err(err("revision_conflict", "expected revision does not match"))
        } else {
            Ok(())
        }
    }
    fn validate(
        &self,
        stem: &str,
        gloss: &str,
        sigs: &[SignatureId],
    ) -> Result<(), StructuredError> {
        if stem.is_empty() {
            return Err(err("invalid_stem", "stem cannot be empty"));
        }
        if sigs.is_empty() {
            return Err(err(
                "invalid_signatures",
                "at least one signature is required",
            ));
        }
        if !gloss.is_empty() && self.gloss_language.is_none() {
            return Err(err(
                "gloss_language_required",
                "set gloss language before adding a gloss",
            ));
        }
        Ok(())
    }
    fn bump(&mut self) {
        self.counter += 1;
        self.revision = Revision(format!("rev_{}", self.counter))
    }
    pub fn add(&mut self, r: AddRequest) -> Result<MutationResult<SuppliedEntry>, StructuredError> {
        self.check(&r.expected_revision)?;
        self.validate(&r.stem, &r.gloss, &r.signatures)?;
        let id = EntryId::from_bytes(self.ids.next_128()?);
        if self.entries.contains_key(&id) {
            return Err(err("duplicate_entry_id", "generated duplicate entry id"));
        }
        let now = self.clock.now();
        let e = SuppliedEntry {
            id,
            stem: r.stem,
            gloss: r.gloss,
            signatures: r.signatures,
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
        self.validate(&r.stem, &r.gloss, &r.signatures)?;
        let old = self
            .entries
            .get(&r.id)
            .cloned()
            .ok_or_else(|| err("entry_not_found", "entry not found"))?;
        if old.stem == r.stem && old.gloss == r.gloss && old.signatures == r.signatures {
            return Ok(MutationResult {
                value: old,
                revision: self.revision.clone(),
                changed: false,
            });
        }
        let mut e = old;
        e.stem = r.stem;
        e.gloss = r.gloss;
        e.signatures = r.signatures;
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
                    && r.state.as_ref().is_none_or(|s| &e.state == s)
            })
            .cloned()
            .collect()
    }
}
