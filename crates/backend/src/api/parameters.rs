use serde::{Deserialize, Serialize};

/// FHIR Parameters resource for operation inputs/outputs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameters {
    #[serde(rename = "resourceType")]
    pub resource_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter: Option<Vec<Parameter>>,
}

/// Individual parameter within Parameters resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    #[serde(flatten)]
    pub value: Option<ParameterValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<Vec<Parameter>>,
    // Retain unrecognized value[x] keys so resource inputs cannot hide mixed forms.
    #[serde(flatten, skip_serializing)]
    pub other: std::collections::BTreeMap<String, serde_json::Value>,
}

/// Parameter value types according to FHIR spec
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::enum_variant_names)]
pub enum ParameterValue {
    ValueString(String),
    ValueBoolean(bool),
    ValueInteger(i64),
    ValueDecimal(f64),
    ValueCode(String),
    ValueUri(String),
    ValueUrl(String),
    ValueCanonical(String),
    ValueCoding(Coding),
    ValueCodeableConcept(CodeableConcept),
}

/// FHIR Coding datatype
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

/// FHIR CodeableConcept datatype
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeableConcept {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coding: Option<Vec<Coding>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl Parameters {
    /// Do not silently discard result-affecting options on the supported inline subset.
    pub fn validate_inline_fields(
        &self,
        strings: &[&str],
        integers: &[&str],
    ) -> Result<(), crate::error::AppError> {
        use crate::error::AppError;
        let mut seen = std::collections::BTreeSet::new();
        for parameter in self.parameter.iter().flatten() {
            let name = parameter.name.as_str();
            if !seen.insert(name) {
                return Err(AppError::BadRequest(format!("Duplicate {name} parameter")));
            }
            if name == "valueSet" {
                continue; // The resource selector has its own structural check.
            }
            let typed = if strings.contains(&name) {
                self.get_string(name).is_some()
            } else if integers.contains(&name) {
                self.get_integer(name).is_some()
            } else {
                return Err(AppError::BadRequest(format!(
                    "Unsupported inline parameter {name}"
                )));
            };
            if !typed
                || parameter.resource.is_some()
                || parameter.part.is_some()
                || parameter.other.keys().any(|key| key.starts_with("value"))
            {
                return Err(AppError::BadRequest(format!(
                    "Invalid inline parameter type: {name}"
                )));
            }
        }
        Ok(())
    }

    /// Validate the request-local ValueSet selector without silently using a stored root.
    pub fn inline_value_set(
        &self,
        instance: bool,
    ) -> Result<Option<&serde_json::Value>, crate::error::AppError> {
        use crate::error::AppError;
        if self.resource_type != "Parameters" {
            return Err(AppError::BadRequest(
                "Expected a Parameters resource".into(),
            ));
        }
        for name in ["valueSet", "url", "valueSetVersion"] {
            if self
                .parameter
                .iter()
                .flatten()
                .filter(|p| p.name == name)
                .count()
                > 1
            {
                return Err(AppError::BadRequest(format!("Duplicate {name} parameter")));
            }
        }
        let Some(parameter) = self.get_parameter("valueSet") else {
            return Ok(None);
        };
        if parameter.value.is_some()
            || parameter.part.is_some()
            || parameter.resource.is_none()
            || parameter.other.keys().any(|key| key.starts_with("value"))
        {
            return Err(AppError::BadRequest(
                "valueSet must contain exactly one resource, not value[x] or part".into(),
            ));
        }
        if instance || self.get_parameter("url").is_some() {
            return Err(AppError::BadRequest(
                "Choose one ValueSet selector: instance, url or inline valueSet".into(),
            ));
        }
        if let Some(version) = self.get_parameter("valueSetVersion") {
            if version.resource.is_some()
                || version.part.is_some()
                || self.get_string("valueSetVersion").is_none()
                || version.other.keys().any(|key| key.starts_with("value"))
            {
                return Err(AppError::BadRequest(
                    "valueSetVersion must be a string".into(),
                ));
            }
        }
        Ok(parameter.resource.as_ref())
    }

    pub fn new() -> Self {
        Self {
            resource_type: "Parameters".to_string(),
            parameter: None,
        }
    }

    pub fn with_parameters(params: Vec<Parameter>) -> Self {
        Self {
            resource_type: "Parameters".to_string(),
            parameter: Some(params),
        }
    }

    pub fn get_parameter(&self, name: &str) -> Option<&Parameter> {
        self.parameter.as_ref()?.iter().find(|p| p.name == name)
    }

    pub fn get_string(&self, name: &str) -> Option<&str> {
        match self.get_parameter(name)?.value.as_ref()? {
            ParameterValue::ValueString(s)
            | ParameterValue::ValueUri(s)
            | ParameterValue::ValueUrl(s)
            | ParameterValue::ValueCanonical(s)
            | ParameterValue::ValueCode(s) => Some(s),
            _ => None,
        }
    }

    pub fn get_integer(&self, name: &str) -> Option<i64> {
        match self.get_parameter(name)?.value.as_ref()? {
            ParameterValue::ValueInteger(n) => Some(*n),
            _ => None,
        }
    }

    pub fn get_boolean(&self, name: &str) -> Option<bool> {
        match self.get_parameter(name)?.value.as_ref()? {
            ParameterValue::ValueBoolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn get_code(&self, name: &str) -> Option<&str> {
        match self.get_parameter(name)?.value.as_ref()? {
            ParameterValue::ValueCode(c) => Some(c),
            _ => None,
        }
    }

    pub fn get_uri(&self, name: &str) -> Option<&str> {
        match self.get_parameter(name)?.value.as_ref()? {
            ParameterValue::ValueUri(u) => Some(u),
            _ => None,
        }
    }
}

impl Default for Parameters {
    fn default() -> Self {
        Self::new()
    }
}

impl Parameter {
    pub fn string(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: Some(ParameterValue::ValueString(value.into())),
            resource: None,
            part: None,
            other: Default::default(),
        }
    }

    pub fn boolean(name: impl Into<String>, value: bool) -> Self {
        Self {
            name: name.into(),
            value: Some(ParameterValue::ValueBoolean(value)),
            resource: None,
            part: None,
            other: Default::default(),
        }
    }

    pub fn code(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: Some(ParameterValue::ValueCode(value.into())),
            resource: None,
            part: None,
            other: Default::default(),
        }
    }

    pub fn coding(name: impl Into<String>, coding: Coding) -> Self {
        Self {
            name: name.into(),
            value: Some(ParameterValue::ValueCoding(coding)),
            resource: None,
            part: None,
            other: Default::default(),
        }
    }

    pub fn part(name: impl Into<String>, parts: Vec<Parameter>) -> Self {
        Self {
            name: name.into(),
            value: None,
            resource: None,
            part: Some(parts),
            other: Default::default(),
        }
    }
}

impl Coding {
    pub fn new(system: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            system: Some(system.into()),
            version: None,
            code: Some(code.into()),
            display: None,
        }
    }

    pub fn with_display(mut self, display: impl Into<String>) -> Self {
        self.display = Some(display.into());
        self
    }
}
