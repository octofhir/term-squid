//! Parsed R4 expansion options and transitive system-version directives.
use super::selection::{self, Family};
use crate::error::AppError;
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use std::collections::BTreeMap;
use term_squid_core::Canonical;

#[derive(Default, Clone)]
pub struct ExpansionOptions {
    pub date: Option<String>,
    pub context: Option<String>,
    pub context_direction: Option<String>,
    pub include_designations: Option<bool>,
    pub designation: Vec<String>,
    pub include_definition: Option<bool>,
    pub active_only: Option<bool>,
    pub exclude_nested: Option<bool>,
    pub exclude_not_for_ui: Option<bool>,
    pub exclude_post_coordinated: Option<bool>,
    pub display_language: Option<String>,
    pub exclude_system: Vec<String>,
    pub system_version: Vec<String>,
    pub check_system_version: Vec<String>,
    pub force_system_version: Vec<String>,
}
impl ExpansionOptions {
    pub fn validate(&self) -> Result<(), AppError> {
        if let Some(date) = &self.date {
            super::options::date(date)?;
        }
        if let Some(language) = &self.display_language {
            super::options::language(language)?;
        }
        if self
            .context_direction
            .as_deref()
            .is_some_and(|d| !matches!(d, "incoming" | "outgoing"))
            || self.context_direction.is_some() && self.context.is_none()
        {
            return Err(AppError::BadRequest(
                "contextDirection requires context and incoming or outgoing".into(),
            ));
        }
        for values in [
            &self.designation,
            &self.exclude_system,
            &self.system_version,
            &self.check_system_version,
            &self.force_system_version,
        ] {
            if values.len() > 64 || values.iter().any(|v| v.is_empty() || v.len() > 4096) {
                return Err(AppError::BadRequest(
                    "Expansion option cardinality or text limit exceeded".into(),
                ));
            }
        }
        for value in &self.exclude_system {
            Canonical::parse(value, None)?;
        }
        for values in [
            &self.system_version,
            &self.check_system_version,
            &self.force_system_version,
        ] {
            versions(values)?;
        }
        for token in &self.designation {
            if let Some((system, code)) = token.split_once('|') {
                if code.is_empty() || system.is_empty() || code.contains('|') {
                    return Err(AppError::BadRequest(
                        "designation requires language or system|code".into(),
                    ));
                }
            } else {
                super::options::language(token)?;
            }
        }
        Ok(())
    }
    pub fn is_default(&self) -> bool {
        self.date.is_none()
            && self.context.is_none()
            && self.context_direction.is_none()
            && self.include_designations.is_none()
            && self.designation.is_empty()
            && self.include_definition.is_none()
            && self.active_only.is_none()
            && self.exclude_nested.is_none()
            && self.exclude_not_for_ui.is_none()
            && self.exclude_post_coordinated.is_none()
            && self.display_language.is_none()
            && self.exclude_system.is_empty()
            && self.system_version.is_empty()
            && self.check_system_version.is_empty()
            && self.force_system_version.is_empty()
    }
    pub(super) fn wants_designations(&self) -> bool {
        self.include_designations != Some(false)
            && (self.include_designations == Some(true) || !self.designation.is_empty())
    }
    pub(super) fn designation_matches(&self, value: &Value) -> bool {
        self.designation.is_empty()
            || self.designation.iter().any(|token| {
                if let Some((system, code)) = token.split_once('|') {
                    value["use"]["system"] == system && value["use"]["code"] == code
                } else {
                    value["language"]
                        .as_str()
                        .is_some_and(|language| super::options::language_matches(token, language))
                }
            })
    }
    pub(super) async fn compose(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        resource: &mut Value,
    ) -> Result<(), AppError> {
        let defaults = versions(&self.system_version)?;
        let forced = versions(&self.force_system_version)?;
        let checked = versions(&self.check_system_version)?;
        for name in ["include", "exclude"] {
            if let Some(selections) = resource
                .get_mut("compose")
                .and_then(|c| c.get_mut(name))
                .and_then(Value::as_array_mut)
            {
                for selection in selections {
                    let Some(system) = selection["system"].as_str() else {
                        continue;
                    };
                    let mut key = Canonical::parse(system, selection["version"].as_str())?;
                    if let Some(version) = forced.get(&key.url) {
                        key.version = Some(version.clone());
                    } else if key.version.is_none() {
                        key.version = defaults.get(&key.url).cloned();
                    }
                    if let Some(expected) = checked.get(&key.url) {
                        if key.version.as_ref().is_some_and(|v| v != expected) {
                            return Err(AppError::BadRequest(format!(
                                "check-system-version conflicts for {}",
                                key.url
                            )));
                        }
                    }
                    if self.date.is_some() || checked.contains_key(&key.url) {
                        let id =
                            selection::id(tx, Family::System, &key, None, self.date.as_deref())
                                .await?;
                        let version: Option<String> =
                            sqlx::query_scalar("SELECT version FROM code_systems WHERE id=$1")
                                .bind(id)
                                .fetch_one(&mut **tx)
                                .await?;
                        if checked
                            .get(&key.url)
                            .is_some_and(|expected| Some(expected) != version.as_ref())
                        {
                            return Err(AppError::BadRequest(format!(
                                "check-system-version conflicts for {}",
                                key.url
                            )));
                        }
                        key.version = version;
                    }
                    selection["system"] = key.url.into();
                    if let Some(version) = key.version {
                        selection["version"] = version.into();
                    }
                }
            }
        }
        Ok(())
    }
}
fn versions(values: &[String]) -> Result<BTreeMap<String, String>, AppError> {
    let mut result = BTreeMap::new();
    for value in values {
        let canonical = Canonical::parse(value, None)?;
        let version = canonical.version.ok_or_else(|| {
            AppError::BadRequest("System version directives require system|version".into())
        })?;
        if result
            .insert(canonical.url, version.clone())
            .is_some_and(|old| old != version)
        {
            return Err(AppError::BadRequest(
                "Conflicting system-version directives".into(),
            ));
        }
    }
    Ok(result)
}
