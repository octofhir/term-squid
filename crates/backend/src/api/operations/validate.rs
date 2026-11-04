use axum::extract::State;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::parameters::{Parameter, Parameters};
use crate::api::{
    coded_input::CodedInput,
    extractors::{
        validate_json_format, FhirJson as Json, FhirParameters, FhirPath as Path,
        FhirQuery as Query,
    },
};
use crate::error::AppError;
use crate::store::{TerminologyStore, ValueSetOperation, ValueSetRequest, ValueSetResult};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateCodeParams {
    #[serde(rename = "_format")]
    pub format: Option<String>,
    pub url: Option<String>,
    pub system: Option<String>,
    pub date: Option<String>,
    #[serde(rename = "displayLanguage")]
    pub language: Option<String>,
    pub context: Option<String>,
    pub code: Option<String>,
    pub version: Option<String>,
    pub display: Option<String>,
    #[serde(rename = "abstract")]
    pub allow_abstract: Option<bool>,
    #[serde(rename = "valueSetVersion")]
    pub value_set_version: Option<String>,
    #[serde(rename = "systemVersion")]
    pub system_version: Option<String>,
}

impl ValidateCodeParams {
    pub(super) fn input(
        &self,
        value_set: bool,
        instance: bool,
    ) -> Result<CodedInput<'_>, AppError> {
        validate_json_format(self.format.as_deref())?;
        CodedInput::validate_text("url", self.url.as_deref())?;
        CodedInput::validate_text("valueSetVersion", self.value_set_version.as_deref())?;
        if value_set && self.version.is_some()
            || !value_set && (self.system_version.is_some() || self.value_set_version.is_some())
        {
            return Err(AppError::BadRequest(
                "Use version for CodeSystem, systemVersion/valueSetVersion for ValueSet".into(),
            ));
        }
        if instance && (self.url.is_some() || value_set && self.value_set_version.is_some()) {
            return Err(AppError::BadRequest(
                "The instance selects its resource and business version; omit url/valueSetVersion"
                    .into(),
            ));
        }
        CodedInput::primitive(
            self.system.as_deref(),
            self.code.as_deref(),
            if value_set {
                self.system_version.as_deref()
            } else {
                self.version.as_deref()
            },
            self.display.as_deref(),
        )
    }
}

/// GET /CodeSystem/$validate-code?url=...&code=...
pub async fn validate_code_cs_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(params): Query<ValidateCodeParams>,
) -> Result<Json<Parameters>, AppError> {
    if let Some(answer) = super::validation_input::get(&*store, &params, false, None).await? {
        return Ok(answer);
    }
    let input = params.input(false, false)?;
    let selected = input.canonical(params.url.as_deref())?;
    if let Some(allow) = params.allow_abstract {
        return validate_options(
            &*store,
            crate::store::ValidationTarget::CodeSystem {
                url: &selected.url,
                version: selected.version.as_deref(),
                id: None,
                inline: None,
                alternatives: false,
            },
            input,
            allow,
        )
        .await;
    }
    perform_validate_code(
        store,
        &selected.url,
        input.code,
        selected.version.as_deref(),
        input.display,
    )
    .await
}

/// POST /CodeSystem/$validate-code with Parameters body.
pub async fn validate_code_cs_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    FhirParameters(params): FhirParameters,
) -> Result<Json<Parameters>, AppError> {
    super::validation_input::fields(&params, false, false)?;
    if let Some(answer) = super::validation_input::complex(&*store, &params, false, None).await? {
        return Ok(answer);
    }
    if params.get_parameter("coding").is_some() && params.get_parameter("url").is_none() {
        return Err(AppError::BadRequest(
            "url is required to select the CodeSystem when validating a Coding".into(),
        ));
    }
    let input = params.coded_input("version")?;
    if params.get_parameter("coding").is_some() {
        let target = term_squid_core::Canonical::parse(
            params.get_string("url").expect("Required Coding target"),
            None,
        )?;
        if target
            .version
            .as_deref()
            .zip(input.version)
            .is_some_and(|(a, b)| a != b)
        {
            return Err(AppError::BadRequest(
                "Coding.version conflicts with the selected CodeSystem URL version".into(),
            ));
        }
    }
    let selected = input.canonical(params.get_string("url"))?;
    perform_validate_code(
        store,
        &selected.url,
        input.code,
        selected.version.as_deref(),
        input.display,
    )
    .await
}

/// The instance and its asserted coding context are checked in one resolved snapshot.
pub async fn validate_code_cs_instance_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Query(params): Query<ValidateCodeParams>,
) -> Result<Json<Parameters>, AppError> {
    if let Some(answer) = super::validation_input::get(&*store, &params, false, Some(id)).await? {
        return Ok(answer);
    }
    let input = params.input(false, true)?;
    if let Some(allow) = params.allow_abstract {
        return validate_options(
            &*store,
            crate::store::ValidationTarget::CodeSystem {
                url: "urn:term-squid:instance",
                version: params.version.as_deref(),
                id: Some(id),
                inline: None,
                alternatives: false,
            },
            input,
            allow,
        )
        .await;
    }
    validate_code_instance(&*store, id, input).await
}
pub async fn validate_code_cs_instance_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    FhirParameters(params): FhirParameters,
) -> Result<Json<Parameters>, AppError> {
    super::validation_input::fields(&params, false, true)?;
    if let Some(answer) =
        super::validation_input::complex(&*store, &params, false, Some(id)).await?
    {
        return Ok(answer);
    }
    validate_code_instance(&*store, id, params.coded_input("version")?).await
}
async fn validate_code_instance(
    store: &dyn TerminologyStore,
    id: Uuid,
    input: CodedInput<'_>,
) -> Result<Json<Parameters>, AppError> {
    let resolved = store
        .resolve_concept_by_id(&id, input.code)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("CodeSystem {id} not found")))?;
    input.check_instance(&resolved)?;
    validate_result(resolved, input.code, input.display)
}

/// GET /ValueSet/$validate-code?url=...&code=...&system=...
pub async fn validate_code_vs_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(params): Query<ValidateCodeParams>,
) -> Result<Json<Parameters>, AppError> {
    if let Some(answer) = super::validation_input::get(&*store, &params, true, None).await? {
        return Ok(answer);
    }
    let input = params.input(true, false)?;
    let system = input.canonical(None)?;
    let url = params
        .url
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("url parameter required for ValueSet".into()))?;
    if let Some(allow) = params.allow_abstract {
        return validate_options(
            &*store,
            crate::store::ValidationTarget::ValueSet(ValueSetRequest {
                url,
                version: params.value_set_version.as_deref(),
                id: None,
                inline: None,
                system_context: None,
            }),
            input,
            allow,
        )
        .await;
    }
    perform_validate_code_valueset(
        store,
        url,
        params.value_set_version.as_deref(),
        system.version.as_deref(),
        &system.url,
        input.code,
        input.display,
    )
    .await
}

/// POST /ValueSet/$validate-code supports stored or request-local ValueSets.
pub async fn validate_code_vs_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    FhirParameters(params): FhirParameters,
) -> Result<Json<Parameters>, AppError> {
    super::validation_input::fields(&params, true, false)?;
    if let Some(answer) = super::validation_input::complex(&*store, &params, true, None).await? {
        return Ok(answer);
    }
    let inline = params.inline_value_set(false)?;
    let input = params.coded_input("systemVersion")?;
    let system = input.canonical(None)?;
    let url = params
        .get_string("url")
        .or_else(|| inline.map(|_| "urn:term-squid:inline"))
        .ok_or_else(|| {
            AppError::BadRequest("url or valueSet parameter required for ValueSet".into())
        })?;
    validate_value_set(
        &*store,
        ValueSetRequest {
            url,
            version: params.get_string("valueSetVersion"),
            id: None,
            inline,
            system_context: system.version.as_deref().map(|v| (system.url.as_str(), v)),
        },
        system.version.as_deref(),
        &system.url,
        input.code,
        input.display,
    )
    .await
}

/// Instance ValueSets retain their resource context; Coding.version selects the code system.
pub async fn validate_code_vs_instance_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Query(params): Query<ValidateCodeParams>,
) -> Result<Json<Parameters>, AppError> {
    if let Some(answer) = super::validation_input::get(&*store, &params, true, Some(id)).await? {
        return Ok(answer);
    }
    let input = params.input(true, true)?;
    if let Some(allow) = params.allow_abstract {
        return validate_options(
            &*store,
            crate::store::ValidationTarget::ValueSet(ValueSetRequest {
                url: "urn:term-squid:instance",
                version: None,
                id: Some(id),
                inline: None,
                system_context: None,
            }),
            input,
            allow,
        )
        .await;
    }
    validate_value_set_instance(&*store, id, input).await
}
pub async fn validate_code_vs_instance_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    FhirParameters(params): FhirParameters,
) -> Result<Json<Parameters>, AppError> {
    super::validation_input::fields(&params, true, true)?;
    if let Some(answer) = super::validation_input::complex(&*store, &params, true, Some(id)).await?
    {
        return Ok(answer);
    }
    validate_value_set_instance(&*store, id, params.coded_input("systemVersion")?).await
}
async fn validate_value_set_instance(
    store: &dyn TerminologyStore,
    id: Uuid,
    input: CodedInput<'_>,
) -> Result<Json<Parameters>, AppError> {
    let system = input.canonical(None)?;
    validate_value_set(
        store,
        ValueSetRequest {
            url: "urn:term-squid:instance",
            version: None,
            id: Some(id),
            inline: None,
            system_context: system.version.as_deref().map(|v| (system.url.as_str(), v)),
        },
        system.version.as_deref(),
        &system.url,
        input.code,
        input.display,
    )
    .await
}

async fn validate_options(
    store: &dyn TerminologyStore,
    target: crate::store::ValidationTarget<'_>,
    input: CodedInput<'_>,
    allow: bool,
) -> Result<Json<Parameters>, AppError> {
    let canonical = input.system.map(|_| input.canonical(None)).transpose()?;
    let candidates = [crate::store::ValidationCandidate {
        system: canonical.as_ref().map(|c| c.url.as_str()),
        version: canonical
            .as_ref()
            .and_then(|c| c.version.as_deref())
            .or(input.version),
        code: input.code,
        display: input.display,
    }];
    store
        .validate_candidates(target, &candidates, allow)
        .await
        .map(super::validation_input::answer)
}

async fn perform_validate_code(
    store: Arc<dyn TerminologyStore>,
    system: &str,
    code: &str,
    version: Option<&str>,
    display: Option<&str>,
) -> Result<Json<Parameters>, AppError> {
    let resolved = store
        .resolve_concept(system, version, code)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "CodeSystem '{system}' version {version:?} not found"
            ))
        })?;
    validate_result(resolved, code, display)
}

fn validate_result(
    resolved: crate::models::ResolvedConcept,
    code: &str,
    display: Option<&str>,
) -> Result<Json<Parameters>, AppError> {
    if resolved.content_mode.as_deref() == Some("supplement") {
        return Err(AppError::BadRequest(
            "A supplement URL cannot identify Coding.system".into(),
        ));
    }
    if resolved.code.is_none() && resolved.content_mode.as_deref() != Some("complete") {
        return Err(AppError::BadRequest(
            "CodeSystem content is incomplete; membership is unknown".into(),
        ));
    }
    let mut valid = resolved.code.is_some();
    let mut message = if valid {
        None
    } else {
        Some(format!("Code '{code}' not found"))
    };
    if let Some(expected) = display {
        if resolved.display.as_deref() != Some(expected) && valid {
            valid = false;
            message = Some(format!(
                "Display '{expected}' does not match {:?}",
                resolved.display
            ));
        }
    }
    let mut result = vec![Parameter::boolean("result", valid)];
    if let Some(display) = resolved.display {
        result.push(Parameter::string("display", display));
    }
    if let Some(version) = resolved.version {
        result.push(Parameter::string("version", version));
    }
    if let Some(message) = message {
        result.push(Parameter::string("message", message));
    }
    Ok(Json(Parameters::with_parameters(result)))
}

async fn perform_validate_code_valueset(
    store: Arc<dyn TerminologyStore>,
    value_set_url: &str,
    value_set_version: Option<&str>,
    system_version: Option<&str>,
    system: &str,
    code: &str,
    display: Option<&str>,
) -> Result<Json<Parameters>, AppError> {
    validate_value_set(
        &*store,
        ValueSetRequest {
            url: value_set_url,
            version: value_set_version,
            id: None,
            inline: None,
            system_context: system_version.map(|v| (system, v)),
        },
        system_version,
        system,
        code,
        display,
    )
    .await
}

async fn validate_value_set(
    store: &dyn TerminologyStore,
    request: ValueSetRequest<'_>,
    system_version: Option<&str>,
    system: &str,
    code: &str,
    display: Option<&str>,
) -> Result<Json<Parameters>, AppError> {
    let result = store
        .query_value_set(
            request,
            ValueSetOperation::Find {
                system,
                version: system_version,
                code,
            },
        )
        .await?;
    let member = match &result {
        ValueSetResult::Native(page) => page.members.first(),
        ValueSetResult::Finite(plan) => plan.members.find(system, system_version, code),
    };
    let valid = member.is_some_and(|m| display.is_none_or(|d| m.display.as_deref() == Some(d)));
    let mut result = vec![Parameter::boolean("result", valid)];
    if let Some(m) = member {
        if let Some(display) = &m.display {
            result.push(Parameter::string("display", display));
        }
        if let Some(version) = &m.version {
            result.push(Parameter::string("version", version));
        }
    }
    if !valid {
        result.push(Parameter::string(
            "message",
            "Code, version or display is not in the ValueSet",
        ));
    }
    Ok(Json(Parameters::with_parameters(result)))
}
