use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::api::parameters::Parameter;
use crate::api::{
    coded_input::{CodedInput, InputType},
    extractors::{
        validate_json_format, FhirJson as Json, FhirParameters, FhirPath as Path,
        FhirQuery as Query,
    },
};
use crate::error::AppError;
use crate::store::TerminologyStore;

/// Keep the existing typed header fields while preserving all supported property value types.
#[derive(Serialize)]
pub struct LookupResponse {
    #[serde(rename = "resourceType")]
    resource_type: &'static str,
    parameter: Vec<LookupParameter>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum LookupParameter {
    Standard(Parameter),
    Property(Value),
}

impl From<Parameter> for LookupParameter {
    fn from(parameter: Parameter) -> Self {
        Self::Standard(parameter)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookupParams {
    #[serde(rename = "_format")]
    pub format: Option<String>,
    pub url: Option<String>,
    pub system: Option<String>,
    pub code: Option<String>,
    pub version: Option<String>,
    pub date: Option<String>,
    #[serde(rename = "displayLanguage")]
    pub language: Option<String>,
    #[serde(default)]
    pub property: Vec<String>,
}

struct Options<'a> {
    date: Option<&'a str>,
    language: Option<&'a str>,
    properties: Vec<&'a str>,
}
impl<'a> Options<'a> {
    fn get(params: &'a LookupParams) -> Result<Self, AppError> {
        validate_json_format(params.format.as_deref())?;
        Ok(Self {
            date: params.date.as_deref(),
            language: params.language.as_deref(),
            properties: params.property.iter().map(String::as_str).collect(),
        })
    }
    fn post(
        params: &'a crate::api::parameters::Parameters,
        instance: bool,
    ) -> Result<Self, AppError> {
        use InputType::*;
        let mut fields = vec![
            ("system", Text, false),
            ("code", Text, false),
            ("version", Text, false),
            ("coding", Coding, false),
            ("date", DateTime, false),
            ("displayLanguage", Text, false),
            ("property", Text, true),
        ];
        if !instance {
            fields.push(("url", Text, false));
        }
        params.validate_fields(&fields)?;
        let date = params.get_date("date");
        let properties = params
            .parameter
            .iter()
            .flatten()
            .filter(|p| p.name == "property")
            .filter_map(|p| match p.value.as_ref() {
                Some(
                    crate::api::parameters::ParameterValue::ValueString(v)
                    | crate::api::parameters::ParameterValue::ValueCode(v)
                    | crate::api::parameters::ParameterValue::ValueUri(v)
                    | crate::api::parameters::ParameterValue::ValueCanonical(v)
                    | crate::api::parameters::ParameterValue::ValueUrl(v),
                ) => Some(v.as_str()),
                _ => None,
            })
            .collect();
        Ok(Self {
            date,
            language: params.get_string("displayLanguage"),
            properties,
        })
    }
    fn wants(&self, name: &str) -> bool {
        self.properties.is_empty()
            || self.properties.contains(&"*")
            || self.properties.contains(&name)
    }
}
pub async fn lookup_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(params): Query<LookupParams>,
) -> Result<Json<LookupResponse>, AppError> {
    let input = CodedInput::primitive(
        params.system.as_deref(),
        params.code.as_deref(),
        params.version.as_deref(),
        None,
    )?;
    perform_lookup(
        &*store,
        input,
        params.url.as_deref(),
        None,
        Options::get(&params)?,
    )
    .await
}
pub async fn lookup_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    FhirParameters(params): FhirParameters,
) -> Result<Json<LookupResponse>, AppError> {
    let options = Options::post(&params, false)?;
    perform_lookup(
        &*store,
        params.coded_input("version")?,
        params.get_string("url"),
        None,
        options,
    )
    .await
}
pub async fn lookup_instance_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Query(params): Query<LookupParams>,
) -> Result<Json<LookupResponse>, AppError> {
    if params.url.is_some() {
        return Err(AppError::BadRequest(
            "The instance selects its CodeSystem; omit url".into(),
        ));
    }
    perform_lookup(
        &*store,
        CodedInput::primitive(
            params.system.as_deref(),
            params.code.as_deref(),
            params.version.as_deref(),
            None,
        )?,
        None,
        Some(id),
        Options::get(&params)?,
    )
    .await
}
pub async fn lookup_instance_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    FhirParameters(params): FhirParameters,
) -> Result<Json<LookupResponse>, AppError> {
    let options = Options::post(&params, true)?;
    perform_lookup(
        &*store,
        params.coded_input("version")?,
        None,
        Some(id),
        options,
    )
    .await
}
async fn perform_lookup(
    store: &dyn TerminologyStore,
    input: CodedInput<'_>,
    url: Option<&str>,
    id: Option<Uuid>,
    options: Options<'_>,
) -> Result<Json<LookupResponse>, AppError> {
    if options.properties.len() > 64 {
        return Err(AppError::BadRequest(
            "Maximum 64 requested properties".into(),
        ));
    }
    for property in &options.properties {
        CodedInput::validate_text("property", Some(property))?;
    }
    if let Some(date) = options.date {
        crate::store::options::date(date)?;
    }
    if let Some(language) = options.language {
        crate::store::options::language(language)?;
    }
    if id.is_none() && input.system.is_none() {
        return Err(AppError::BadRequest(
            "system or Coding.system parameter required".into(),
        ));
    }
    let selected = if id.is_some() {
        term_squid_core::Canonical::parse("urn:term-squid:instance", input.version)?
    } else {
        input.canonical(url)?
    };
    let rich =
        options.date.is_some() || options.language.is_some() || !options.properties.is_empty();
    let answer = if rich {
        store
            .lookup_details(crate::store::LookupRequest {
                url: &selected.url,
                version: selected.version.as_deref(),
                id,
                code: input.code,
                metadata: options.language.is_some() || !options.properties.is_empty(),
                date: options.date,
                language: options.language,
                relationships: options.wants("parent") || options.wants("child"),
            })
            .await?
    } else {
        let concept = if let Some(id) = id {
            store.resolve_concept_by_id(&id, input.code).await?
        } else {
            store
                .resolve_concept(&selected.url, selected.version.as_deref(), input.code)
                .await?
        }
        .ok_or_else(|| AppError::NotFound("CodeSystem not found".into()))?;
        crate::store::LookupAnswer {
            concept,
            designations: Vec::new(),
            properties: Vec::new(),
        }
    };
    if id.is_some() {
        input.check_instance(&answer.concept)?;
    }
    lookup_result(answer, input.code, &options)
}

fn lookup_result(
    answer: crate::store::LookupAnswer,
    code: &str,
    options: &Options<'_>,
) -> Result<Json<LookupResponse>, AppError> {
    let concept = answer.concept;
    if concept.content_mode.as_deref() == Some("supplement") {
        return Err(AppError::BadRequest(
            "A supplement URL cannot identify Coding.system; use its supplemented CodeSystem"
                .into(),
        ));
    }
    if concept.code.is_none() {
        return Err(if concept.content_mode.as_deref() == Some("complete") {
            AppError::NotFound(format!("Code '{code}' not found"))
        } else {
            AppError::BadRequest("CodeSystem content is incomplete; code cannot be resolved".into())
        });
    }
    // Build result Parameters
    let mut result_params: Vec<LookupParameter> = vec![
        Parameter::string(
            "name",
            concept
                .name
                .as_deref()
                .filter(|n| !n.is_empty())
                .unwrap_or(&concept.url),
        )
        .into(),
        Parameter::string(
            "display",
            concept
                .display
                .as_deref()
                .filter(|display| !display.is_empty())
                .unwrap_or_else(|| concept.code.as_deref().unwrap()),
        )
        .into(),
    ];

    if let Some(version) = &concept.version {
        result_params.push(Parameter::string("version", version).into());
    }
    if let Some(definition) = concept.definition.filter(|v| !v.is_empty()) {
        if options.wants("definition") {
            result_params.push(LookupParameter::Property(typed_property(
                json!({"code":"definition","valueString":definition}),
            )?));
        }
    }
    if options.wants("designation") {
        for designation in answer.designations {
            let mut parts = Vec::new();
            if let Some(language) = designation["language"].as_str() {
                crate::store::options::language(language)?;
                parts.push(json!({"name":"language","valueCode":language}));
            }
            if let Some(usage) = designation.get("use") {
                if !valid_coding(usage) {
                    return Err(property_error("Invalid designation.use Coding"));
                }
                parts.push(json!({"name":"use","valueCoding":usage}));
            }
            let value = designation["value"]
                .as_str()
                .filter(|v| !v.is_empty())
                .ok_or_else(|| property_error("Invalid designation value"))?;
            parts.push(json!({"name":"value","valueString":value}));
            result_params.push(LookupParameter::Property(
                json!({"name":"designation","part":parts}),
            ));
        }
    }
    for property in answer.properties {
        if property["code"].as_str().is_some_and(|c| options.wants(c)) {
            result_params.push(LookupParameter::Property(typed_property(property)?));
        }
    }

    if let Some(properties) = concept.properties {
        match properties.0 {
            Value::Array(properties) => {
                for property in properties {
                    let property = typed_property(property)?;
                    if property["part"][0]["valueCode"]
                        .as_str()
                        .is_some_and(|c| options.wants(c))
                    {
                        result_params.push(LookupParameter::Property(property));
                    }
                }
            }
            // Older stored projections use a string-valued map. Preserve that wire contract.
            Value::Object(properties) => {
                for (code, value) in properties {
                    if !options.wants(&code) {
                        continue;
                    }
                    if !valid_code(&code) {
                        return Err(property_error("invalid legacy property code"));
                    }
                    result_params.push(
                        Parameter::part(
                            "property",
                            vec![
                                Parameter::code("code", code),
                                Parameter::string("value", value.to_string()),
                            ],
                        )
                        .into(),
                    );
                }
            }
            _ => return Err(property_error("expected an array or legacy object")),
        }
    }

    Ok(Json(LookupResponse {
        resource_type: "Parameters",
        parameter: result_params,
    }))
}

fn property_error(message: &str) -> AppError {
    AppError::Terminology(term_squid_core::Error(format!(
        "Cannot return stored concept property: {message}"
    )))
}

fn valid_code(value: &str) -> bool {
    let mut whitespace = true;
    for character in value.chars() {
        let next = character.is_whitespace();
        if whitespace && next {
            return false;
        }
        whitespace = next;
    }
    !whitespace
}

fn valid_date_time(value: &str) -> bool {
    let Some(year) = value.get(..4).and_then(|s| s.parse::<u32>().ok()) else {
        return false;
    };
    if year == 0 || !value.as_bytes()[..4].iter().all(u8::is_ascii_digit) {
        return false;
    }
    match value.len() {
        4 => true,
        7 => {
            value.as_bytes()[4] == b'-'
                && value[5..].bytes().all(|b| b.is_ascii_digit())
                && value[5..]
                    .parse::<u32>()
                    .is_ok_and(|month| (1..=12).contains(&month))
        }
        10 => {
            value.as_bytes()[4] == b'-'
                && value.as_bytes()[7] == b'-'
                && value.as_bytes()[5..7].iter().all(u8::is_ascii_digit)
                && value.as_bytes()[8..10].iter().all(u8::is_ascii_digit)
                && chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
        }
        _ => {
            value.as_bytes().get(10) == Some(&b'T')
                && chrono::DateTime::parse_from_rfc3339(value).is_ok_and(|date| {
                    date.offset().local_minus_utc().abs() <= 14 * 60 * 60
                        && (value.ends_with('Z')
                            || value
                                .as_bytes()
                                .get(value.len().saturating_sub(6))
                                .is_some_and(|b| matches!(b, b'+' | b'-')))
                })
        }
    }
}

fn valid_coding(value: &Value) -> bool {
    let Some(coding) = value.as_object() else {
        return false;
    };
    !coding.is_empty()
        && coding.iter().all(|(key, value)| match key.as_str() {
            "system" => value
                .as_str()
                .is_some_and(|s| !s.is_empty() && !s.chars().any(char::is_whitespace)),
            "version" | "display" => value.as_str().is_some_and(|s| !s.is_empty()),
            "code" => value.as_str().is_some_and(valid_code),
            "userSelected" => value.is_boolean(),
            _ => false,
        })
}

fn typed_property(property: Value) -> Result<Value, AppError> {
    let Value::Object(mut property) = property else {
        return Err(property_error("property item must be an object"));
    };
    let code = property
        .remove("code")
        .ok_or_else(|| property_error("missing code"))?;
    if !code.as_str().is_some_and(valid_code) {
        return Err(property_error("invalid code"));
    }
    if property.len() != 1 {
        return Err(property_error(
            "expected exactly one supported value[x]; property metadata is unsupported",
        ));
    }
    let (kind, value) = property.into_iter().next().unwrap();
    // These are the seven R4 CodeSystem.concept.property / $lookup value types.
    // Do not coerce or discard unsupported value types and primitive metadata.
    let valid = match kind.as_str() {
        "valueCode" => value.as_str().is_some_and(valid_code),
        "valueCoding" => valid_coding(&value),
        "valueString" => value.as_str().is_some_and(|s| !s.is_empty()),
        "valueInteger" => value.as_i64().is_some_and(|v| i32::try_from(v).is_ok()),
        "valueBoolean" => value.is_boolean(),
        "valueDateTime" => value.as_str().is_some_and(valid_date_time),
        "valueDecimal" => value.is_number(),
        _ => return Err(property_error("unsupported value[x] type")),
    };
    if !valid {
        return Err(property_error(
            "invalid value[x] or unsupported Coding metadata",
        ));
    }
    let mut value_part = serde_json::Map::new();
    value_part.insert("name".into(), "value".into());
    value_part.insert(kind, value);
    Ok(json!({"name":"property","part":[{"name":"code","valueCode":code},value_part]}))
}
