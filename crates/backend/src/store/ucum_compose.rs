//! Only explicit finite selections can project the bundled, non-enumerable grammar.
use super::options;
use crate::error::AppError;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use term_squid_core::{ucum, Canonical, Compose};

pub(super) fn bundled(key: &Canonical) -> bool {
    key.url == ucum::SYSTEM && key.version.as_deref().is_none_or(|v| v == ucum::VERSION)
}

#[derive(Default)]
pub(super) struct Projection {
    pub keys: BTreeSet<Canonical>,
    codes: BTreeSet<String>,
}
impl Projection {
    pub fn collect(
        &mut self,
        compose: &Compose,
        context: Option<(&str, &str)>,
    ) -> Result<(), AppError> {
        for selection in compose.include.iter().chain(&compose.exclude) {
            let Some(url) = &selection.system else {
                continue;
            };
            let key = Canonical::parse(url, selection.version.as_deref())?;
            let effective = Canonical::parse(
                &key.url,
                key.version.as_deref().or_else(|| {
                    context
                        .filter(|(url, _)| *url == key.url)
                        .map(|(_, version)| version)
                }),
            )?;
            if !bundled(&effective) {
                continue;
            }
            if selection.concept.is_empty() || !selection.filter.is_empty() {
                return Err(AppError::Terminology(term_squid_core::Error(
                    "UCUM ValueSet selections require explicit codes without filters; whole-grammar expansion is unavailable".into(),
                )));
            }
            self.keys.insert(key);
            for concept in &selection.concept {
                if concept.code.len() > ucum::MAX_EXPRESSION_BYTES {
                    return Err(AppError::BadRequest(
                        "UCUM expression exceeds 1024 bytes".into(),
                    ));
                }
                self.codes.insert(concept.code.clone());
                if self.codes.len() > ucum::MAX_EXPLICIT_CODES {
                    return Err(AppError::BadRequest(
                        "UCUM composition exceeds 1024 distinct explicit expressions".into(),
                    ));
                }
            }
        }
        Ok(())
    }
    pub fn resource(&self, date: Option<&str>) -> Result<Value, AppError> {
        let resource = json!({"resourceType":"CodeSystem","url":ucum::SYSTEM,"version":ucum::VERSION,"status":"active","content":"fragment","caseSensitive":true,"date":ucum::REVISION_DATE,"concept":self.codes.iter().map(|code| json!({"code":code})).collect::<Vec<_>>()});
        options::check_resource_date(&resource, date)?;
        Ok(resource)
    }
}
