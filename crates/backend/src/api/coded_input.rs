//! Shared admission and normalization for the declared single-code R4 operations.
use super::parameters::{ParameterValue, Parameters};
use crate::{error::AppError, models::ResolvedConcept};
use term_squid_core::Canonical;

const MAX_PARAMETERS: usize = 64;
const MAX_TEXT: usize = 4096;

pub struct CodedInput<'a> {
    pub system: Option<&'a str>,
    pub code: &'a str,
    pub version: Option<&'a str>,
    pub display: Option<&'a str>,
}
impl<'a> CodedInput<'a> {
    pub fn validate_text(name: &str, value: Option<&str>) -> Result<(), AppError> {
        if value.is_some_and(|s| s.is_empty() || s.len() > MAX_TEXT) {
            return Err(AppError::BadRequest(format!(
                "{name} must contain 1..{MAX_TEXT} UTF-8 bytes"
            )));
        }
        Ok(())
    }
    pub fn primitive(
        system: Option<&'a str>,
        code: Option<&'a str>,
        version: Option<&'a str>,
        display: Option<&'a str>,
    ) -> Result<Self, AppError> {
        for (name, value) in [
            ("system", system),
            ("code", code),
            ("version", version),
            ("display", display),
        ] {
            Self::validate_text(name, value)?;
        }
        Ok(Self {
            system,
            code: code.ok_or_else(|| AppError::BadRequest("code parameter required".into()))?,
            version,
            display,
        })
    }

    /// Resolve URL/version constraints before entering the store. This also merges
    /// embedded canonical versions without allowing either selector to win silently.
    pub fn canonical(&self, target: Option<&str>) -> Result<Canonical, AppError> {
        Self::validate_text("url", target)?;
        let parsed = self
            .system
            .map(|s| Canonical::parse(s, self.version))
            .transpose()?;
        let selected = target
            .map(|s| Canonical::parse(s, self.version))
            .transpose()?;
        match (selected, parsed) {
            (Some(mut selected), Some(input)) => {
                if selected.url != input.url
                    || selected
                        .version
                        .as_ref()
                        .zip(input.version.as_ref())
                        .is_some_and(|(a, b)| a != b)
                {
                    return Err(AppError::BadRequest("Coding/system conflicts with the selected CodeSystem URL or business version".into()));
                }
                selected.version = selected.version.or(input.version);
                Ok(selected)
            }
            (Some(selected), None) | (None, Some(selected)) => Ok(selected),
            (None, None) => Err(AppError::BadRequest(
                "system or CodeSystem url parameter required".into(),
            )),
        }
    }

    /// Instance metadata comes from the same snapshot as the concept, including
    /// SQL fallback/native projections; no second resource/auth query is needed.
    pub fn check_instance(&self, resolved: &ResolvedConcept) -> Result<(), AppError> {
        let selected = if self.system.is_some() {
            Some(self.canonical(None)?)
        } else {
            None
        };
        if selected.as_ref().is_some_and(|s| s.url != resolved.url)
            || selected
                .as_ref()
                .and_then(|s| s.version.as_deref())
                .or(self.version)
                .is_some_and(|v| Some(v) != resolved.version.as_deref())
        {
            return Err(AppError::BadRequest(
                "Input system/version conflicts with the selected CodeSystem instance".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub enum InputType {
    Text,
    DateTime,
    Boolean,
    Integer,
    Coding,
    CodeableConcept,
    Resource,
    Parts,
}

impl Parameters {
    pub fn validate_fields(&self, fields: &[(&str, InputType, bool)]) -> Result<(), AppError> {
        if self.resource_type != "Parameters" {
            return Err(AppError::BadRequest(
                "Expected a Parameters resource".into(),
            ));
        }
        let parameters = self.parameter.as_deref().unwrap_or_default();
        if parameters.len() > MAX_PARAMETERS {
            return Err(AppError::BadRequest(
                "Too many operation parameters (maximum 64)".into(),
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for p in parameters {
            let Some((_, kind, repeated)) = fields.iter().find(|(name, _, _)| *name == p.name)
            else {
                return Err(AppError::BadRequest(format!(
                    "Unsupported operation parameter {}",
                    p.name
                )));
            };
            if !seen.insert(p.name.as_str()) && !repeated {
                return Err(AppError::BadRequest(format!(
                    "Duplicate {} parameter",
                    p.name
                )));
            }
            let valid = match kind {
                InputType::Resource => {
                    p.resource.is_some() && p.value.is_none() && p.part.is_none()
                }
                InputType::Parts => p.part.is_some() && p.value.is_none() && p.resource.is_none(),
                kind => {
                    p.resource.is_none()
                        && p.part.is_none()
                        && match (kind, p.value.as_ref()) {
                            (
                                InputType::Text,
                                Some(
                                    ParameterValue::ValueString(s)
                                    | ParameterValue::ValueCode(s)
                                    | ParameterValue::ValueUri(s)
                                    | ParameterValue::ValueUrl(s)
                                    | ParameterValue::ValueCanonical(s),
                                ),
                            ) => !s.is_empty() && s.len() <= MAX_TEXT,
                            (InputType::DateTime, Some(ParameterValue::ValueDateTime(s))) => {
                                crate::store::options::date(s)?;
                                true
                            }
                            (InputType::Boolean, Some(ParameterValue::ValueBoolean(_)))
                            | (InputType::Integer, Some(ParameterValue::ValueInteger(_)))
                            | (InputType::Coding, Some(ParameterValue::ValueCoding(_)))
                            | (
                                InputType::CodeableConcept,
                                Some(ParameterValue::ValueCodeableConcept(_)),
                            ) => true,
                            _ => false,
                        }
                }
            };
            if !valid || !p.other.is_empty() {
                return Err(AppError::BadRequest(format!(
                    "Invalid or mixed parameter representation: {}",
                    p.name
                )));
            }
        }
        Ok(())
    }
    pub fn coded_input(&self, version_name: &str) -> Result<CodedInput<'_>, AppError> {
        let version = self.get_string(version_name);
        if let Some(p) = self.get_parameter("coding") {
            if ["system", "code", "display"]
                .iter()
                .any(|name| self.get_parameter(name).is_some())
            {
                return Err(AppError::BadRequest(
                    "Choose one code form: coding or primitive code/system/display".into(),
                ));
            }
            let Some(ParameterValue::ValueCoding(coding)) = p.value.as_ref() else {
                return Err(AppError::BadRequest("coding requires valueCoding".into()));
            };
            if coding
                .version
                .as_deref()
                .zip(version)
                .is_some_and(|(a, b)| a != b)
            {
                return Err(AppError::BadRequest(
                    "Coding.version conflicts with the version parameter".into(),
                ));
            }
            CodedInput::primitive(
                coding.system.as_deref(),
                coding.code.as_deref(),
                coding.version.as_deref().or(version),
                coding.display.as_deref(),
            )
        } else {
            CodedInput::primitive(
                self.get_string("system"),
                self.get_string("code"),
                version,
                self.get_string("display"),
            )
        }
    }
}
