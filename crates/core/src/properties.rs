//! Formal property meanings override spelling; undeclared properties retain their conventional code.
use crate::{Error, Property, PropertyDefinition, Result};
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct PropertyDefinitions {
    meanings: BTreeMap<String, Option<String>>,
    types: BTreeMap<String, String>,
}
impl PropertyDefinitions {
    pub fn prepare(definitions: &[PropertyDefinition]) -> Result<Self> {
        let mut result = Self::default();
        for definition in definitions {
            let meaning = definition.uri.as_ref().map_or_else(
                || Some(definition.code.clone()),
                |uri| {
                    uri.strip_prefix("http://hl7.org/fhir/concept-properties#")
                        .map(str::to_owned)
                },
            );
            if result
                .meanings
                .get(&definition.code)
                .is_some_and(|previous| previous != &meaning)
                || result
                    .types
                    .get(&definition.code)
                    .zip(definition.kind.as_ref())
                    .is_some_and(|(previous, current)| previous != current)
            {
                return Err(Error("Conflicting property declarations".into()));
            }
            result.meanings.insert(definition.code.clone(), meaning);
            if let Some(kind) = &definition.kind {
                result.types.insert(definition.code.clone(), kind.clone());
            }
        }
        Ok(result)
    }
    /// Strict R4 admission is opt-in; legacy ingestion can retain opaque source metadata.
    pub fn validate_r4_types(&self) -> Result<()> {
        for (code, kind) in &self.types {
            if !matches!(
                kind.as_str(),
                "code" | "Coding" | "string" | "integer" | "boolean" | "dateTime" | "decimal"
            ) {
                return Err(Error("Unknown R4 property type".into()));
            }
            let expected = match self.meaning(code) {
                Some("inactive" | "notSelectable" | "abstract") => Some("boolean"),
                Some("status" | "parent" | "child") => Some("code"),
                Some("retirementDate" | "deprecationDate") => Some("dateTime"),
                _ => None,
            };
            if expected.is_some_and(|expected| kind != expected) {
                return Err(Error(
                    "Standard property meaning conflicts with its declared type".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn meaning<'a>(&'a self, code: &'a str) -> Option<&'a str> {
        self.meanings
            .get(code)
            .map_or(Some(code), |meaning| meaning.as_deref())
    }
    pub fn kind(&self, code: &str) -> Option<&str> {
        self.types.get(code).map(String::as_str)
    }
    pub(super) fn validate_value(&self, property: &Property) -> Result<()> {
        let (typed, value) = property.typed_value()?;
        let actual = match typed {
            "valueCode" => "code",
            "valueCoding" => "Coding",
            "valueString" => "string",
            "valueInteger" => "integer",
            "valueBoolean" => "boolean",
            "valueDateTime" => "dateTime",
            "valueDecimal" => "decimal",
            _ => return Err(Error("Unknown R4 concept property value type".into())),
        };
        let kind = self.kind(&property.code).unwrap_or(actual);
        let valid = match kind {
            "code" => typed == "valueCode" && value.is_string(),
            "string" => typed == "valueString" && value.is_string(),
            "boolean" => typed == "valueBoolean" && value.is_boolean(),
            "integer" => {
                typed == "valueInteger" && value.as_i64().is_some_and(|n| i32::try_from(n).is_ok())
            }
            "Coding" => typed == "valueCoding" && value.is_object(),
            "dateTime" => typed == "valueDateTime" && value.is_string(),
            "decimal" => typed == "valueDecimal" && value.is_number(),
            _ => false,
        };
        if !valid {
            return Err(Error(format!(
                "Property {} does not match its declared R4 {kind} type",
                property.code
            )));
        }
        Ok(())
    }
}
