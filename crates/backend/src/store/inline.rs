use crate::error::AppError;
use serde_json::Value;
use sha2::{Digest, Sha256};
use term_squid_core::Canonical;

pub(super) const MAX_INLINE_BYTES: usize = 256 * 1024;

pub(super) struct InlineValueSet {
    pub resource: Value,
    pub canonical: Canonical,
    pub fingerprint: [u8; 32],
    pub bytes: usize,
}

impl InlineValueSet {
    pub fn new(resource: &Value, version: Option<&str>) -> Result<Self, AppError> {
        if resource.get("resourceType").and_then(Value::as_str) != Some("ValueSet") {
            return Err(AppError::BadRequest(
                "valueSet.resource must be a ValueSet".into(),
            ));
        }
        let bytes =
            serde_json::to_vec(resource).map_err(|e| AppError::BadRequest(e.to_string()))?;
        if bytes.len() > MAX_INLINE_BYTES {
            return Err(AppError::BadRequest(
                "Inline ValueSet exceeds 256 KiB".into(),
            ));
        }
        let text = |field| -> Result<Option<&str>, AppError> {
            resource
                .get(field)
                .map(|v| {
                    v.as_str().filter(|s| !s.is_empty()).ok_or_else(|| {
                        AppError::BadRequest(format!(
                            "Inline ValueSet {field} must be a nonempty string"
                        ))
                    })
                })
                .transpose()
        };
        let business_version = text("version")?;
        if version.is_some() && version != business_version {
            return Err(AppError::BadRequest(
                "valueSetVersion conflicts with inline ValueSet.version".into(),
            ));
        }
        let digest = Sha256::digest(&bytes);
        let fingerprint: [u8; 32] = digest.into();
        let anonymous = format!("urn:term-squid:inline:{}", hex::encode(fingerprint));
        let canonical = Canonical::parse(text("url")?.unwrap_or(&anonymous), business_version)?;
        if !resource.get("compose").is_some_and(Value::is_object) {
            return Err(AppError::BadRequest(
                "Inline ValueSet requires a compose object".into(),
            ));
        }
        Ok(Self {
            resource: resource.clone(),
            canonical,
            fingerprint,
            bytes: bytes.len(),
        })
    }
}
