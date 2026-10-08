use serde_json::Value;

use crate::FactsError;

#[derive(Debug, Clone)]
pub(crate) struct FactsContext {
    pub baseline_token_json: String,
    pub baseline_key: String,
    pub input_kind: &'static str,
    pub dry_run_digest: Option<String>,
    pub expected_model_fingerprint: Option<String>,
}

impl FactsContext {
    pub fn parse(bytes: &[u8]) -> Result<Self, FactsError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|error| FactsError::InvalidContext(error.to_string()))?;
        let value = pg_assess::jcs::parse_strict_json(text)
            .map_err(|error| FactsError::InvalidContext(error.to_string()))?;
        let object = value
            .as_object()
            .ok_or_else(|| FactsError::InvalidContext("context must be a JSON object".into()))?;
        let allowed = [
            "format",
            "version",
            "baselineToken",
            "inputKind",
            "dryRunDigest",
            "expectedModelFingerprint",
        ];
        if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
            return Err(FactsError::InvalidContext(format!(
                "unknown context property {key:?}"
            )));
        }
        if object.get("format").and_then(Value::as_str) != Some(crate::FACTS_CONTEXT_FORMAT) {
            return Err(FactsError::InvalidContext(
                "format must be \"pangloss-facts-context\"".into(),
            ));
        }
        let version = object
            .get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| FactsError::InvalidContext("version must be an integer".into()))?;
        if version != u64::from(crate::FACTS_CONTEXT_VERSION) {
            return Err(FactsError::UnsupportedContextVersion(version));
        }
        let baseline_token = object
            .get("baselineToken")
            .ok_or_else(|| FactsError::InvalidContext("baselineToken is required".into()))?;
        let baseline_token_json = pg_assess::canonicalize(baseline_token)
            .map_err(|error| FactsError::InvalidContext(error.to_string()))?;
        let baseline_key = pg_assess::sha256_bytes(baseline_token_json.as_bytes());
        let input_kind = match object.get("inputKind").and_then(Value::as_str) {
            Some("baseline") => "baseline",
            Some("proposal-dry-run") => "proposal-dry-run",
            _ => {
                return Err(FactsError::InvalidContext(
                    "inputKind must be \"baseline\" or \"proposal-dry-run\"".into(),
                ));
            }
        };
        let dry_run_digest = match object.get("dryRunDigest") {
            Some(Value::Null) => None,
            Some(Value::String(value)) => Some(normalize_digest(value, "dryRunDigest")?),
            Some(_) => {
                return Err(FactsError::InvalidContext(
                    "dryRunDigest must be a SHA-256 string or null".into(),
                ));
            }
            None => {
                return Err(FactsError::InvalidContext(
                    "dryRunDigest is required".into(),
                ));
            }
        };
        match (input_kind, dry_run_digest.is_some()) {
            ("baseline", true) => {
                return Err(FactsError::InvalidContext(
                    "dryRunDigest must be null for a baseline".into(),
                ));
            }
            ("proposal-dry-run", false) => {
                return Err(FactsError::InvalidContext(
                    "dryRunDigest is required for a proposal dry run".into(),
                ));
            }
            _ => {}
        }
        let expected_model_fingerprint = object
            .get("expectedModelFingerprint")
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| {
                        FactsError::InvalidContext(
                            "expectedModelFingerprint must be a SHA-256 string".into(),
                        )
                    })
                    .and_then(|digest| normalize_digest(digest, "expectedModelFingerprint"))
            })
            .transpose()?;
        Ok(Self {
            baseline_token_json,
            baseline_key,
            input_kind,
            dry_run_digest,
            expected_model_fingerprint,
        })
    }
}

fn normalize_digest(value: &str, field: &str) -> Result<String, FactsError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(FactsError::InvalidContext(format!(
            "{field} must use the sha256:<64 hex digits> form"
        )));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(FactsError::InvalidContext(format!(
            "{field} must use the sha256:<64 hex digits> form"
        )));
    }
    Ok(format!("sha256:{}", hex.to_ascii_lowercase()))
}
