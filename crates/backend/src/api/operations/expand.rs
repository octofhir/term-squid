use crate::api::{
    coded_input::InputType,
    extractors::{
        validate_json_format, FhirJson as Json, FhirParameters, FhirPath as Path,
        FhirQuery as Query,
    },
};
use axum::extract::State;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::parameters::Parameters;
use crate::error::AppError;
use crate::store::{TerminologyStore, ValueSetOperation, ValueSetRequest, ValueSetResult};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpandParams {
    #[serde(rename = "_format")]
    pub format: Option<String>,
    pub url: Option<String>,
    #[serde(rename = "valueSetVersion")]
    pub value_set_version: Option<String>,
    pub filter: Option<String>,
    pub offset: Option<i64>,
    pub count: Option<i64>,
    #[serde(rename = "date")]
    pub date: Option<String>,
    #[serde(rename = "context")]
    pub context: Option<String>,
    #[serde(rename = "contextDirection")]
    pub context_direction: Option<String>,
    #[serde(rename = "includeDesignations")]
    pub include_designations: Option<bool>,
    #[serde(rename = "designation", default)]
    pub designation: Vec<String>,
    #[serde(rename = "includeDefinition")]
    pub include_definition: Option<bool>,
    #[serde(rename = "activeOnly")]
    pub active_only: Option<bool>,
    #[serde(rename = "excludeNested")]
    pub exclude_nested: Option<bool>,
    #[serde(rename = "excludeNotForUI")]
    pub exclude_not_for_ui: Option<bool>,
    #[serde(rename = "excludePostCoordinated")]
    pub exclude_post_coordinated: Option<bool>,
    #[serde(rename = "displayLanguage")]
    pub display_language: Option<String>,
    #[serde(rename = "exclude-system", default)]
    pub exclude_system: Vec<String>,
    #[serde(rename = "system-version", default)]
    pub system_version: Vec<String>,
    #[serde(rename = "check-system-version", default)]
    pub check_system_version: Vec<String>,
    #[serde(rename = "force-system-version", default)]
    pub force_system_version: Vec<String>,
}

impl ExpandParams {
    fn options(&self) -> crate::store::ExpansionOptions {
        crate::store::ExpansionOptions {
            date: self.date.clone(),
            context: self.context.clone(),
            context_direction: self.context_direction.clone(),
            include_designations: self.include_designations,
            designation: self.designation.clone(),
            include_definition: self.include_definition,
            active_only: self.active_only,
            exclude_nested: self.exclude_nested,
            exclude_not_for_ui: self.exclude_not_for_ui,
            exclude_post_coordinated: self.exclude_post_coordinated,
            display_language: self.display_language.clone(),
            exclude_system: self.exclude_system.clone(),
            system_version: self.system_version.clone(),
            check_system_version: self.check_system_version.clone(),
            force_system_version: self.force_system_version.clone(),
        }
    }
    fn post(params: &Parameters) -> Result<Self, AppError> {
        let mut value = serde_json::Map::new();
        for parameter in params
            .parameter
            .iter()
            .flatten()
            .filter(|p| p.name != "valueSet")
        {
            let encoded = match parameter.value.as_ref() {
                Some(
                    crate::api::parameters::ParameterValue::ValueDateTime(v)
                    | crate::api::parameters::ParameterValue::ValueString(v)
                    | crate::api::parameters::ParameterValue::ValueCode(v)
                    | crate::api::parameters::ParameterValue::ValueUri(v)
                    | crate::api::parameters::ParameterValue::ValueUrl(v)
                    | crate::api::parameters::ParameterValue::ValueCanonical(v),
                ) => json!(v),
                Some(crate::api::parameters::ParameterValue::ValueBoolean(v)) => json!(v),
                Some(crate::api::parameters::ParameterValue::ValueInteger(v)) => json!(v),
                _ => return Err(AppError::BadRequest("Invalid expansion parameter".into())),
            };
            if matches!(
                parameter.name.as_str(),
                "designation"
                    | "exclude-system"
                    | "system-version"
                    | "check-system-version"
                    | "force-system-version"
            ) {
                value
                    .entry(parameter.name.clone())
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .expect("Repeated parameter array")
                    .push(encoded);
            } else {
                value.insert(parameter.name.clone(), encoded);
            }
        }
        serde_json::from_value(value.into()).map_err(|e| AppError::BadRequest(e.to_string()))
    }
}

/// GET /ValueSet/$expand?url=...
pub async fn expand_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(params): Query<ExpandParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    let url = params
        .url
        .clone()
        .unwrap_or_else(|| "urn:term-squid:context".into());
    perform_expand(store, &url, params, None, None).await
}

/// POST /ValueSet/$expand with Parameters body
pub async fn expand_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    FhirParameters(params): FhirParameters,
) -> Result<Json<serde_json::Value>, AppError> {
    validate_fields(&params)?;
    let inline = params.inline_value_set(false)?;
    let expand_params = ExpandParams::post(&params)?;
    let url = expand_params.url.clone().unwrap_or_else(|| {
        if inline.is_some() {
            "urn:term-squid:inline".into()
        } else {
            "urn:term-squid:context".into()
        }
    });

    perform_expand(store, &url, expand_params, None, inline).await
}

/// GET /ValueSet/{id}/$expand
pub async fn expand_instance_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Query(params): Query<ExpandParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    perform_expand(store, "urn:term-squid:instance", params, Some(id), None).await
}

/// POST /ValueSet/{id}/$expand with Parameters body
pub async fn expand_instance_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    FhirParameters(params): FhirParameters,
) -> Result<Json<serde_json::Value>, AppError> {
    validate_fields(&params)?;
    params.inline_value_set(true)?;
    let expand_params = ExpandParams::post(&params)?;
    perform_expand(
        store,
        "urn:term-squid:instance",
        expand_params,
        Some(id),
        None,
    )
    .await
}

fn validate_fields(params: &Parameters) -> Result<(), AppError> {
    use InputType::*;
    params.validate_fields(&[
        ("url", Text, false),
        ("valueSetVersion", Text, false),
        ("valueSet", Resource, false),
        ("filter", Text, false),
        ("offset", Integer, false),
        ("count", Integer, false),
        ("date", DateTime, false),
        ("context", Text, false),
        ("contextDirection", Text, false),
        ("includeDesignations", Boolean, false),
        ("designation", Text, true),
        ("includeDefinition", Boolean, false),
        ("activeOnly", Boolean, false),
        ("excludeNested", Boolean, false),
        ("excludeNotForUI", Boolean, false),
        ("excludePostCoordinated", Boolean, false),
        ("displayLanguage", Text, false),
        ("exclude-system", Text, true),
        ("system-version", Text, true),
        ("check-system-version", Text, true),
        ("force-system-version", Text, true),
    ])
}

async fn perform_expand(
    store: Arc<dyn TerminologyStore>,
    url: &str,
    params: ExpandParams,
    id: Option<Uuid>,
    inline: Option<&serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    validate_json_format(params.format.as_deref())?;
    crate::api::coded_input::CodedInput::validate_text("url", params.url.as_deref())?;
    crate::api::coded_input::CodedInput::validate_text(
        "valueSetVersion",
        params.value_set_version.as_deref(),
    )?;
    crate::api::coded_input::CodedInput::validate_text("filter", params.filter.as_deref())?;
    if params.offset.is_some_and(|n| n < 0)
        || params.count.is_some_and(|n| !(0..=10000).contains(&n))
    {
        return Err(AppError::BadRequest(
            "offset must be nonnegative and count must be between 0 and 10000".into(),
        ));
    }
    let options = params.options();
    options.validate()?;
    if params.url.is_none() && inline.is_none() && id.is_none() && options.context.is_none() {
        return Err(AppError::BadRequest(
            "url, valueSet or context is required".into(),
        ));
    }
    let offset = params.offset.unwrap_or(0);
    let count = params
        .count
        .unwrap_or(if options.exclude_nested == Some(false) {
            10000
        } else {
            100
        });
    let request = ValueSetRequest {
        url,
        version: params.value_set_version.as_deref(),
        id,
        inline,
        system_context: None,
    };
    let rich = if !options.is_default() {
        Some(
            store
                .expand_options(request, &options, params.filter.as_deref(), offset, count)
                .await?,
        )
    } else {
        None
    };
    let (mut result, total, contains, sources) = if let Some(page) = rich {
        (
            page.resource,
            page.total,
            Ok(serde_json::Value::Array(page.contains)),
            page.sources,
        )
    } else {
        let expanded = store
            .query_value_set(
                ValueSetRequest {
                    url,
                    version: params.value_set_version.as_deref(),
                    id,
                    inline,
                    system_context: None,
                },
                ValueSetOperation::Expand {
                    filter: params.filter.as_deref(),
                    offset,
                    count,
                },
            )
            .await?;

        let (result, total, contains, sources) = match expanded {
            ValueSetResult::Native(page) => (
                page.resource,
                page.total,
                serde_json::to_value(page.members),
                Arc::new(page.sources),
            ),
            ValueSetResult::Finite(expanded) => {
                let filter = params.filter.as_ref().map(|s| s.to_lowercase());
                let count = count as usize;
                let mut total = 0;
                let mut entries = Vec::new();
                for member in expanded.members.values().filter(|m| {
                    filter.as_ref().is_none_or(|f| {
                        m.code.to_lowercase().contains(f)
                            || m.display
                                .as_ref()
                                .is_some_and(|d| d.to_lowercase().contains(f))
                    })
                }) {
                    if total >= offset && entries.len() < count {
                        entries.push(member);
                    }
                    total += 1;
                }
                (
                    expanded.resource.as_ref().clone(),
                    total,
                    serde_json::to_value(entries),
                    expanded.sources.clone(),
                )
            }
        };
        (result, total, contains, sources)
    };
    if id.is_some() {
        if let Some(url) = &params.url {
            let expected =
                term_squid_core::Canonical::parse(url, params.value_set_version.as_deref())?;
            if result["url"].as_str() != Some(expected.url.as_str())
                || expected
                    .version
                    .as_deref()
                    .is_some_and(|v| result["version"].as_str() != Some(v))
            {
                return Err(AppError::BadRequest(
                    "Requested ValueSet context conflicts with the selected instance".into(),
                ));
            }
        } else if params
            .value_set_version
            .as_deref()
            .is_some_and(|v| result["version"].as_str() != Some(v))
        {
            return Err(AppError::BadRequest(
                "valueSetVersion conflicts with selected instance".into(),
            ));
        }
    }
    let contains = contains.map_err(|e| AppError::Internal(e.into()))?;
    let mut applied = expansion_parameters(&params, offset, count, total);
    applied.extend(sources.parameters());
    let mut expansion = json!({
        "identifier": format!("urn:uuid:{}", Uuid::new_v4()),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "total":total,"parameter":applied
    });
    if params.offset.is_some() || offset > 0 {
        expansion["offset"] = offset.into();
    }
    expansion["contains"] = contains;
    if options.include_definition == Some(false) {
        if let Some(object) = result.as_object_mut() {
            object.remove("compose");
        }
    }

    if let Some(obj) = result.as_object_mut() {
        obj.insert("expansion".to_string(), expansion);
    }
    Ok(Json(result))
}

fn expansion_parameters(
    params: &ExpandParams,
    offset: i64,
    count: i64,
    total: i64,
) -> Vec<serde_json::Value> {
    let mut result = Vec::new();
    if params.offset.is_some() || offset > 0 {
        result.push(json!({"name":"offset","valueInteger":offset}));
    }
    if params.count.is_some() || total > count {
        result.push(json!({"name":"count","valueInteger":count}));
    }
    if let Some(filter) = &params.filter {
        result.push(json!({"name":"filter","valueString":filter}));
    }
    if let Some(value) = &params.date {
        result.push(json!({"name":"date","valueDateTime":value}));
    }
    if let Some(value) = &params.context {
        result.push(json!({"name":"context","valueString":value}));
    }
    if let Some(value) = &params.context_direction {
        result.push(json!({"name":"contextDirection","valueString":value}));
    }
    if let Some(value) = &params.include_designations {
        result.push(json!({"name":"includeDesignations","valueBoolean":value}));
    }
    for value in &params.designation {
        result.push(json!({"name":"designation","valueString":value}));
    }
    if let Some(value) = &params.include_definition {
        result.push(json!({"name":"includeDefinition","valueBoolean":value}));
    }
    if let Some(value) = &params.active_only {
        result.push(json!({"name":"activeOnly","valueBoolean":value}));
    }
    if let Some(value) = &params.exclude_nested {
        result.push(json!({"name":"excludeNested","valueBoolean":value}));
    }
    if let Some(value) = &params.exclude_not_for_ui {
        result.push(json!({"name":"excludeNotForUI","valueBoolean":value}));
    }
    if let Some(value) = &params.exclude_post_coordinated {
        result.push(json!({"name":"excludePostCoordinated","valueBoolean":value}));
    }
    if let Some(value) = &params.display_language {
        result.push(json!({"name":"displayLanguage","valueString":value}));
    }
    for value in &params.exclude_system {
        result.push(json!({"name":"exclude-system","valueString":value}));
    }
    for value in &params.system_version {
        result.push(json!({"name":"system-version","valueString":value}));
    }
    for value in &params.check_system_version {
        result.push(json!({"name":"check-system-version","valueString":value}));
    }
    for value in &params.force_system_version {
        result.push(json!({"name":"force-system-version","valueString":value}));
    }
    result
}
