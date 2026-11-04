//! Shared complex R4 validation input admission; store evaluation owns one snapshot.
use crate::{
    api::{
        coded_input::{CodedInput, InputType},
        extractors::FhirJson,
        parameters::{Parameter, ParameterValue, Parameters},
    },
    error::AppError,
    store::{
        TerminologyStore, ValidationAnswer, ValidationCandidate, ValidationTarget, ValueSetRequest,
    },
};
use term_squid_core::Canonical;
use uuid::Uuid;

pub(super) fn fields(params: &Parameters, vs: bool, instance: bool) -> Result<(), AppError> {
    use InputType::*;
    let mut fields = vec![
        ("system", Text, false),
        ("code", Text, false),
        ("display", Text, false),
        ("coding", Coding, false),
        ("codeableConcept", CodeableConcept, false),
        ("abstract", Boolean, false),
        ("date", DateTime, false),
        ("displayLanguage", Text, false),
        (if vs { "systemVersion" } else { "version" }, Text, false),
    ];
    if !instance {
        fields.extend([
            ("url", Text, false),
            (if vs { "valueSet" } else { "codeSystem" }, Resource, false),
        ]);
        if vs {
            fields.push(("valueSetVersion", Text, false));
            fields.push(("context", Text, false));
        }
    }
    params.validate_fields(&fields)
}
struct Candidate {
    system: Option<String>,
    version: Option<String>,
    code: String,
    display: Option<String>,
}
fn candidates(
    params: &Parameters,
    version_name: &str,
    default_system: Option<&str>,
) -> Result<Vec<Candidate>, AppError> {
    if ["code", "coding", "codeableConcept"]
        .iter()
        .filter(|name| params.get_parameter(name).is_some())
        .count()
        != 1
    {
        return Err(AppError::BadRequest(
            "Choose exactly one code, coding or codeableConcept".into(),
        ));
    }
    let borrowed = if let Some(p) = params.get_parameter("codeableConcept") {
        if params.get_parameter("system").is_some() || params.get_parameter("display").is_some() {
            return Err(AppError::BadRequest(
                "Primitive system/display cannot accompany CodeableConcept".into(),
            ));
        }
        let Some(ParameterValue::ValueCodeableConcept(concept)) = p.value.as_ref() else {
            unreachable!("Admitted type")
        };
        let codings = concept.coding.as_deref().unwrap_or_default();
        if codings.is_empty() || codings.len() > 64 {
            return Err(AppError::BadRequest(
                "CodeableConcept requires 1..64 codings".into(),
            ));
        }
        codings
            .iter()
            .map(|c| {
                if c.version
                    .as_deref()
                    .zip(params.get_string(version_name))
                    .is_some_and(|(a, b)| a != b)
                {
                    return Err(AppError::BadRequest(
                        "Coding.version conflicts with the operation version".into(),
                    ));
                }
                CodedInput::primitive(
                    c.system.as_deref().or(default_system),
                    c.code.as_deref(),
                    c.version
                        .as_deref()
                        .or_else(|| params.get_string(version_name)),
                    c.display.as_deref(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![params.coded_input(version_name)?]
    };
    borrowed
        .into_iter()
        .map(|mut input| {
            input.system = input.system.or(default_system);
            let canonical = input.system.map(|_| input.canonical(None)).transpose()?;
            Ok(Candidate {
                system: canonical.as_ref().map(|c| c.url.clone()),
                version: canonical
                    .and_then(|c| c.version)
                    .or_else(|| input.version.map(str::to_owned)),
                code: input.code.to_owned(),
                display: input.display.map(str::to_owned),
            })
        })
        .collect()
}
pub(super) fn answer(answer: ValidationAnswer) -> FhirJson<Parameters> {
    let mut result = vec![Parameter::boolean("result", answer.valid)];
    if let Some(display) = answer.display {
        result.push(Parameter::string("display", display));
    }
    if let Some(version) = answer.version {
        result.push(Parameter::string("version", version));
    }
    if let Some(message) = answer.message {
        result.push(Parameter::string("message", message));
    }
    FhirJson(Parameters::with_parameters(result))
}
pub(super) async fn complex(
    store: &dyn TerminologyStore,
    params: &Parameters,
    vs: bool,
    id: Option<Uuid>,
) -> Result<Option<FhirJson<Parameters>>, AppError> {
    let alternatives = params.get_parameter("codeableConcept").is_some();
    let inline = params
        .get_parameter("codeSystem")
        .and_then(|p| p.resource.as_ref());
    let options = crate::store::ValidationOptions {
        date: params.get_date("date").map(str::to_owned),
        language: params.get_string("displayLanguage").map(str::to_owned),
        context: params.get_string("context").map(str::to_owned),
    };
    let infer = vs
        && params.get_parameter("system").is_none()
        && params
            .get_parameter("coding")
            .and_then(|p| match p.value.as_ref() {
                Some(ParameterValue::ValueCoding(c)) => c.system.as_ref(),
                _ => None,
            })
            .is_none();
    if !alternatives
        && inline.is_none()
        && params.get_parameter("abstract").is_none()
        && options.date.is_none()
        && options.language.is_none()
        && options.context.is_none()
        && !infer
    {
        return Ok(None);
    }
    let root = params
        .get_string("url")
        .or_else(|| inline.and_then(|r| r["url"].as_str()))
        .or_else(|| {
            (!vs && !alternatives)
                .then(|| params.get_string("system"))
                .flatten()
        });
    if id.is_none() && !vs && root.is_none() && inline.is_none() {
        return Err(AppError::BadRequest(
            "CodeSystem url or resource required".into(),
        ));
    }
    let version_name = if vs { "systemVersion" } else { "version" };
    let root = root.unwrap_or("urn:term-squid:inline-codesystem");
    let default_system = (!vs && id.is_none()).then_some(root);
    let candidates = candidates(params, version_name, default_system)?;
    let borrowed: Vec<_> = candidates
        .iter()
        .map(|c| ValidationCandidate {
            system: c.system.as_deref(),
            version: c.version.as_deref(),
            code: &c.code,
            display: c.display.as_deref(),
        })
        .collect();
    let target = if vs {
        let inline = params.inline_value_set(id.is_some())?;
        let url = params
            .get_string("url")
            .or_else(|| inline.map(|_| "urn:term-squid:inline"))
            .or_else(|| id.map(|_| "urn:term-squid:instance"))
            .or_else(|| options.context.as_ref().map(|_| "urn:term-squid:context"))
            .ok_or_else(|| AppError::BadRequest("ValueSet url or resource required".into()))?;
        ValidationTarget::ValueSet(ValueSetRequest {
            url,
            version: params.get_string("valueSetVersion"),
            id,
            inline,
            system_context: None,
        })
    } else {
        let canonical = Canonical::parse(root, params.get_string("version"))?;
        if let Some(resource) = inline {
            if resource["resourceType"] != "CodeSystem" {
                return Err(AppError::BadRequest(
                    "codeSystem.resource must be CodeSystem".into(),
                ));
            }
            if resource["url"]
                .as_str()
                .is_some_and(|url| url != canonical.url)
                || params
                    .get_string("version")
                    .is_some_and(|v| resource["version"].as_str() != Some(v))
            {
                return Err(AppError::BadRequest(
                    "Inline CodeSystem identity conflicts with selector".into(),
                ));
            }
        }
        // Keep canonical strings alive outside the borrowed target passed below.
        return store
            .validate_context(
                ValidationTarget::CodeSystem {
                    url: &canonical.url,
                    version: canonical.version.as_deref(),
                    id,
                    inline,
                    alternatives,
                },
                &borrowed,
                params.get_boolean("abstract").unwrap_or(true),
                &options,
            )
            .await
            .map(|r| Some(answer(r)));
    };
    store
        .validate_context(
            target,
            &borrowed,
            params.get_boolean("abstract").unwrap_or(true),
            &options,
        )
        .await
        .map(|r| Some(answer(r)))
}

pub(super) async fn get(
    store: &dyn TerminologyStore,
    params: &super::validate::ValidateCodeParams,
    vs: bool,
    id: Option<Uuid>,
) -> Result<Option<FhirJson<Parameters>>, AppError> {
    if params.date.is_none()
        && params.language.is_none()
        && params.context.is_none()
        && !(vs && params.system.is_none())
    {
        return Ok(None);
    }
    params.input(vs, id.is_some())?;
    let mut fields = Vec::new();
    for (name, value) in [
        ("url", params.url.as_ref()),
        ("system", params.system.as_ref()),
        ("code", params.code.as_ref()),
        ("display", params.display.as_ref()),
        ("version", params.version.as_ref()),
        ("systemVersion", params.system_version.as_ref()),
        ("valueSetVersion", params.value_set_version.as_ref()),
        ("displayLanguage", params.language.as_ref()),
        ("context", params.context.as_ref()),
    ] {
        if let Some(value) = value {
            fields.push(Parameter::string(name, value));
        }
    }
    if let Some(date) = &params.date {
        let mut p = Parameter::string("date", date);
        p.value = Some(ParameterValue::ValueDateTime(date.clone()));
        fields.push(p);
    }
    if let Some(allow) = params.allow_abstract {
        fields.push(Parameter::boolean("abstract", allow));
    }
    let parameters = Parameters::with_parameters(fields);
    self::fields(&parameters, vs, id.is_some())?;
    complex(store, &parameters, vs, id).await
}
