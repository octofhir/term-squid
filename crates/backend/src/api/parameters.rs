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
    pub part: Option<Vec<Parameter>>,
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
            ParameterValue::ValueString(s) => Some(s),
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
            part: None,
        }
    }

    pub fn boolean(name: impl Into<String>, value: bool) -> Self {
        Self {
            name: name.into(),
            value: Some(ParameterValue::ValueBoolean(value)),
            part: None,
        }
    }

    pub fn code(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: Some(ParameterValue::ValueCode(value.into())),
            part: None,
        }
    }

    pub fn coding(name: impl Into<String>, coding: Coding) -> Self {
        Self {
            name: name.into(),
            value: Some(ParameterValue::ValueCoding(coding)),
            part: None,
        }
    }

    pub fn part(name: impl Into<String>, parts: Vec<Parameter>) -> Self {
        Self {
            name: name.into(),
            value: None,
            part: Some(parts),
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
