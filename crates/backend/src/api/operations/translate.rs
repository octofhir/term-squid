use crate::{
    api::{
        coded_input::{CodedInput, InputType},
        extractors::{
            validate_json_format, FhirJson as Json, FhirParameters, FhirPath as Path,
            FhirQuery as Query,
        },
        parameters::{Coding, Parameter, ParameterValue, Parameters},
    },
    error::AppError,
    store::TerminologyStore,
};
use axum::extract::State;
use serde::Deserialize;
use std::{collections::BTreeSet, sync::Arc};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranslateParams {
    pub url: Option<String>,
    pub code: Option<String>,
    pub system: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "targetsystem")]
    pub target_system: Option<String>,
    pub source: Option<String>,
    pub target: Option<String>,
    #[serde(rename = "conceptMapVersion")]
    pub map_version: Option<String>,
    pub reverse: Option<bool>,
    #[serde(rename = "_format")]
    pub format: Option<String>,
}
struct TranslationInput<'a> {
    url: Option<&'a str>,
    map_version: Option<&'a str>,
    inline: Option<&'a serde_json::Value>,
    codings: Vec<CodedInput<'a>>,
    source: Option<&'a str>,
    target: Option<&'a str>,
    target_system: Option<&'a str>,
    reverse: bool,
    dependencies: Vec<term_squid_core::TranslationDependency<'a>>,
}
impl<'a> TranslationInput<'a> {
    fn get(params: &'a TranslateParams) -> Result<Self, AppError> {
        validate_json_format(params.format.as_deref())?;
        Ok(Self {
            url: params.url.as_deref(),
            map_version: params.map_version.as_deref(),
            inline: None,
            codings: vec![CodedInput::primitive(
                params.system.as_deref(),
                params.code.as_deref(),
                params.version.as_deref(),
                None,
            )?],
            source: params.source.as_deref(),
            target: params.target.as_deref(),
            target_system: params.target_system.as_deref(),
            reverse: params.reverse.unwrap_or(false),
            dependencies: Vec::new(),
        })
    }
    fn post(params: &'a Parameters) -> Result<Self, AppError> {
        use InputType::*;
        params.validate_fields(&[
            ("url", Text, false),
            ("conceptMapVersion", Text, false),
            ("conceptMap", Resource, false),
            ("system", Text, false),
            ("version", Text, false),
            ("code", Text, false),
            ("coding", InputType::Coding, false),
            ("codeableConcept", CodeableConcept, false),
            ("source", Text, false),
            ("target", Text, false),
            ("targetsystem", Text, false),
            ("reverse", Boolean, false),
            ("dependency", Parts, true),
        ])?;
        let forms = ["code", "coding", "codeableConcept"]
            .iter()
            .filter(|n| params.get_parameter(n).is_some())
            .count();
        if forms != 1 {
            return Err(AppError::BadRequest(
                "Choose exactly one of code, coding or codeableConcept".into(),
            ));
        }
        let codings = if let Some(p) = params.get_parameter("codeableConcept") {
            if params.get_parameter("system").is_some() {
                return Err(AppError::BadRequest(
                    "system cannot accompany codeableConcept".into(),
                ));
            }
            let Some(ParameterValue::ValueCodeableConcept(concept)) = p.value.as_ref() else {
                unreachable!("Validated type")
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
                        .zip(params.get_string("version"))
                        .is_some_and(|(a, b)| a != b)
                    {
                        return Err(AppError::BadRequest("Conflicting coding versions".into()));
                    }
                    CodedInput::primitive(
                        c.system.as_deref(),
                        c.code.as_deref(),
                        c.version
                            .as_deref()
                            .or_else(|| params.get_string("version")),
                        c.display.as_deref(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![params.coded_input("version")?]
        };
        Ok(Self {
            url: params.get_string("url"),
            map_version: params.get_string("conceptMapVersion"),
            inline: params
                .get_parameter("conceptMap")
                .and_then(|p| p.resource.as_ref()),
            codings,
            source: params.get_string("source"),
            target: params.get_string("target"),
            target_system: params.get_string("targetsystem"),
            reverse: params.get_boolean("reverse").unwrap_or(false),
            dependencies: dependencies(params)?,
        })
    }
}
pub async fn translate_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(params): Query<TranslateParams>,
) -> Result<Json<Parameters>, AppError> {
    perform(&*store, TranslationInput::get(&params)?, None).await
}
pub async fn translate_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    FhirParameters(params): FhirParameters,
) -> Result<Json<Parameters>, AppError> {
    perform(&*store, TranslationInput::post(&params)?, None).await
}
pub async fn translate_instance_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Query(params): Query<TranslateParams>,
) -> Result<Json<Parameters>, AppError> {
    perform(&*store, TranslationInput::get(&params)?, Some(id)).await
}
pub async fn translate_instance_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    FhirParameters(params): FhirParameters,
) -> Result<Json<Parameters>, AppError> {
    perform(&*store, TranslationInput::post(&params)?, Some(id)).await
}
async fn perform(
    store: &dyn TerminologyStore,
    input: TranslationInput<'_>,
    id: Option<Uuid>,
) -> Result<Json<Parameters>, AppError> {
    for (name, value) in [
        ("url", input.url),
        ("conceptMapVersion", input.map_version),
        ("source", input.source),
        ("target", input.target),
        ("targetsystem", input.target_system),
    ] {
        CodedInput::validate_text(name, value)?;
    }
    if input.target.is_some() && input.target_system.is_some() {
        return Err(AppError::BadRequest(
            "Choose target ValueSet or targetsystem".into(),
        ));
    }
    // Inline resource admission is identical for fast and contextual evaluation.
    if let Some(resource) = input.inline {
        validate_inline(&input, id, resource)?;
    }
    if id.is_none() && input.url.is_none() && input.inline.is_none() {
        let normalized = input
            .codings
            .iter()
            .map(|c| c.system.map(|_| c.canonical(None)).transpose())
            .collect::<Result<Vec<_>, _>>()?;
        let codes: Vec<_> = normalized
            .iter()
            .zip(&input.codings)
            .map(|(canonical, code)| crate::store::TranslationCode {
                system: canonical.as_ref().map(|c| c.url.as_str()),
                version: canonical
                    .as_ref()
                    .and_then(|c| c.version.as_deref())
                    .or(code.version),
                code: code.code,
            })
            .collect();
        return finish_answers(store.translate_context(context(&input, id, &codes)).await?);
    }
    let plan = if let Some(resource) = input.inline {
        let resource = resource.clone();
        Arc::new(
            tokio::task::spawn_blocking(move || {
                term_squid_core::PreparedConceptMap::prepare(&resource)
            })
            .await
            .map_err(|e| AppError::Internal(e.into()))??,
        )
    } else {
        if id.is_some() && input.url.is_some() {
            return Err(AppError::BadRequest(
                "Instance selects the ConceptMap; omit url".into(),
            ));
        }
        store
            .prepare_concept_map(
                input.url.unwrap_or("urn:term-squid:instance"),
                input.map_version,
                id,
            )
            .await?
    };
    if id.is_some() && input.map_version.is_some_and(|v| Some(v) != plan.version()) {
        return Err(AppError::BadRequest(
            "conceptMapVersion conflicts with instance".into(),
        ));
    }
    let normalized = input
        .codings
        .iter()
        .map(|c| {
            let system = c
                .system
                .or_else(|| plan.input_system(input.reverse))
                .ok_or_else(|| {
                    AppError::BadRequest(
                        "system is required for a ConceptMap with multiple input systems".into(),
                    )
                })?;
            term_squid_core::Canonical::parse(system, c.version).map_err(AppError::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let codes: Vec<_> = normalized
        .iter()
        .zip(&input.codings)
        .map(|(canonical, code)| crate::store::TranslationCode {
            system: Some(&canonical.url),
            version: canonical.version.as_deref(),
            code: code.code,
        })
        .collect();
    if input
        .source
        .is_some_and(|v| Some(v) != plan.source_scope(input.reverse))
        || input
            .target
            .is_some_and(|v| Some(v) != plan.target_scope(input.reverse))
    {
        return finish_answers(store.translate_context(context(&input, id, &codes)).await?);
    }
    let mut answers = BTreeSet::new();
    for code in &codes {
        let resolution = plan.translate_resolved(
            code.system.expect("Normalized input system"),
            code.version,
            code.code,
            input.target_system,
            input.reverse,
            &input.dependencies,
        )?;
        if !resolution.continuations.is_empty() {
            return finish_answers(store.translate_context(context(&input, id, &codes)).await?);
        }
        for concept in resolution.matches {
            answers.insert(crate::store::TranslationAnswer {
                concept,
                source: plan.url().map(|u| {
                    plan.version()
                        .map_or_else(|| u.to_owned(), |v| format!("{u}|{v}"))
                }),
            });
            if answers.len() > 10000 {
                return Err(AppError::Overloaded);
            }
        }
    }
    finish_answers(answers.into_iter().collect())
}
fn context<'a>(
    input: &'a TranslationInput<'a>,
    id: Option<Uuid>,
    codes: &'a [crate::store::TranslationCode<'a>],
) -> crate::store::TranslationRequest<'a> {
    crate::store::TranslationRequest {
        url: input.url,
        version: input.map_version,
        id,
        inline: input.inline,
        source: input.source,
        target: input.target,
        target_system: input.target_system,
        reverse: input.reverse,
        codes,
        dependencies: &input.dependencies,
    }
}
fn validate_inline(
    input: &TranslationInput<'_>,
    id: Option<Uuid>,
    resource: &serde_json::Value,
) -> Result<(), AppError> {
    if id.is_some() {
        return Err(AppError::BadRequest(
            "An instance cannot accompany an inline ConceptMap".into(),
        ));
    }
    if resource["resourceType"] != "ConceptMap" {
        return Err(AppError::BadRequest(
            "conceptMap.resource must be a ConceptMap".into(),
        ));
    }
    if let Some(url) = input.url {
        let expected = term_squid_core::Canonical::parse(url, input.map_version)?;
        let actual = term_squid_core::Canonical::parse(
            resource["url"].as_str().ok_or_else(|| {
                AppError::BadRequest("Anonymous ConceptMap cannot assert a URL".into())
            })?,
            resource["version"].as_str(),
        )?;
        if expected.url != actual.url || expected.version.is_some_and(|v| Some(v) != actual.version)
        {
            return Err(AppError::BadRequest(
                "Inline ConceptMap identity conflicts with selector".into(),
            ));
        }
    }
    if input
        .map_version
        .is_some_and(|v| resource["version"].as_str() != Some(v))
    {
        return Err(AppError::BadRequest(
            "conceptMapVersion conflicts with inline resource".into(),
        ));
    }
    if serde_json::to_vec(resource)
        .map_err(|e| AppError::BadRequest(e.to_string()))?
        .len()
        > 256 * 1024
    {
        return Err(AppError::BadRequest(
            "Inline ConceptMap exceeds 256 KiB".into(),
        ));
    }
    Ok(())
}
fn dependencies(
    params: &Parameters,
) -> Result<Vec<term_squid_core::TranslationDependency<'_>>, AppError> {
    let mut result = Vec::new();
    for parameter in params
        .parameter
        .iter()
        .flatten()
        .filter(|p| p.name == "dependency")
    {
        let parts = Parameters::with_parameters(parameter.part.clone().unwrap_or_default());
        parts.validate_fields(&[
            ("element", InputType::Text, false),
            ("concept", InputType::CodeableConcept, false),
        ])?;
        // Read from the original request so normalized dependencies borrow no temporary values.
        let parts = parameter.part.as_deref().unwrap_or_default();
        let element = parts
            .iter()
            .find(|p| p.name == "element")
            .and_then(|p| match p.value.as_ref()? {
                ParameterValue::ValueUri(v) => Some(v.as_str()),
                _ => None,
            })
            .ok_or_else(|| AppError::BadRequest("dependency.element requires valueUri".into()))?;
        let concept = parts
            .iter()
            .find(|p| p.name == "concept")
            .and_then(|p| match p.value.as_ref()? {
                ParameterValue::ValueCodeableConcept(v) => Some(v),
                _ => None,
            })
            .ok_or_else(|| {
                AppError::BadRequest("dependency.concept requires valueCodeableConcept".into())
            })?;
        let codings = concept.coding.as_deref().unwrap_or_default();
        if codings.is_empty() || codings.len() > 64 {
            return Err(AppError::BadRequest(
                "dependency.concept requires 1..64 codings".into(),
            ));
        }
        for coding in codings {
            let candidate = CodedInput::primitive(
                coding.system.as_deref(),
                coding.code.as_deref(),
                coding.version.as_deref(),
                coding.display.as_deref(),
            )?;
            if result.len() >= 64 {
                return Err(AppError::BadRequest("Maximum 64 dependency codings".into()));
            }
            result.push(term_squid_core::TranslationDependency {
                element,
                system: candidate.system,
                version: candidate.version,
                code: candidate.code,
            });
        }
    }
    Ok(result)
}
fn finish_answers(
    answers: Vec<crate::store::TranslationAnswer>,
) -> Result<Json<Parameters>, AppError> {
    let parameters = answers
        .into_iter()
        .map(|answer| {
            let concept = answer.concept;
            let mut coding = Coding::new(&concept.system, &concept.code);
            coding.version = concept.version;
            if let Some(display) = concept.display {
                coding = coding.with_display(display);
            }
            let mut parts = vec![
                Parameter::code("equivalence", concept.equivalence),
                Parameter::coding("concept", coding),
            ];
            if let Some(source) = answer.source {
                parts.push(Parameter::uri("source", source));
            }
            for product in concept.products {
                let mut coding =
                    Coding::new(product.system.as_deref().unwrap_or(""), &product.code);
                coding.system = product.system;
                coding.version = product.version;
                coding.display = product.display;
                parts.push(Parameter::part(
                    "product",
                    vec![
                        Parameter::uri("element", product.element),
                        Parameter::coding("concept", coding),
                    ],
                ));
            }
            Parameter::part("match", parts)
        })
        .collect();
    finish(parameters)
}
#[cfg(test)]
type MatchKey<'a> = (&'a str, Option<&'a str>, &'a str, Option<&'a str>, &'a str);
#[cfg(test)]
fn append_matches<'a>(
    parameters: &mut Vec<Parameter>,
    plan: &'a term_squid_core::PreparedConceptMap,
    input: &CodedInput<'_>,
    target: Option<&str>,
    reverse: bool,
    seen: &mut BTreeSet<MatchKey<'a>>,
) -> Result<(), AppError> {
    let system = input
        .system
        .ok_or_else(|| AppError::BadRequest("Coding.system required".into()))?;
    let matches = plan
        .translate_versioned(system, input.version, input.code, target, reverse)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    for mapped in matches {
        if !seen.insert((
            mapped.concept.system,
            mapped.version,
            mapped.concept.code,
            mapped.concept.display,
            mapped.concept.equivalence,
        )) {
            continue;
        }
        if parameters.len() >= 10000 {
            return Err(AppError::Overloaded);
        }
        let mut coding = Coding::new(mapped.concept.system, mapped.concept.code);
        coding.version = mapped.version.map(str::to_owned);
        if let Some(display) = mapped.concept.display {
            coding = coding.with_display(display);
        }
        let mut parts = vec![
            Parameter::code("equivalence", mapped.concept.equivalence),
            Parameter::coding("concept", coding),
        ];
        if let Some(url) = plan.url() {
            parts.push(Parameter::uri(
                "source",
                plan.version()
                    .map_or_else(|| url.to_owned(), |v| format!("{url}|{v}")),
            ));
        }
        parameters.push(Parameter::part("match", parts));
    }
    Ok(())
}
fn finish(mut matches: Vec<Parameter>) -> Result<Json<Parameters>, AppError> {
    let valid = !matches.is_empty();
    let mut parameters = vec![Parameter::boolean("result", valid)];
    if !valid {
        parameters.push(Parameter::string(
            "message",
            "No translation found for the selected code and context",
        ));
    }
    parameters.append(&mut matches);
    Ok(Json(Parameters::with_parameters(parameters)))
}
#[cfg(test)]
fn translate_plan(
    plan: &term_squid_core::PreparedConceptMap,
    system: &str,
    code: &str,
    target: Option<&str>,
    reverse: bool,
) -> Result<Json<Parameters>, AppError> {
    let mut matches = Vec::new();
    let input = CodedInput::primitive(Some(system), Some(code), None, None)?;
    append_matches(
        &mut matches,
        plan,
        &input,
        target,
        reverse,
        &mut BTreeSet::new(),
    )?;
    finish(matches)
}
#[cfg(test)]
fn translate_map(
    content: &serde_json::Value,
    source_system: &str,
    source_code: &str,
    target_system: Option<&str>,
    reverse: bool,
) -> Result<Json<Parameters>, AppError> {
    translate_plan(
        &term_squid_core::PreparedConceptMap::prepare(content)?,
        source_system,
        source_code,
        target_system,
        reverse,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reverse_uses_target_code_and_source_equivalence() {
        let map = json!({"group":[{"source":"urn:source","target":"urn:target","element":[
            {"code":"a","display":"Source A","target":[{"code":"b","display":"Target B","equivalence":"wider"}]}]}]});
        let response = translate_map(&map, "urn:target", "b", Some("urn:source"), true)
            .unwrap()
            .0;
        let value = serde_json::to_value(response).unwrap();
        let parts = value["parameter"][1]["part"].as_array().unwrap();
        assert!(parts.iter().any(|p| p["name"] == "concept"
            && p["valueCoding"]["code"] == "a"
            && p["valueCoding"]["display"] == "Source A"));
        assert!(parts
            .iter()
            .any(|p| p["name"] == "equivalence" && p["valueCode"] == "narrower"));
        assert_eq!(
            translate_map(&map, "urn:target", "a", None, true)
                .unwrap()
                .0
                .get_boolean("result"),
            Some(false)
        );
        assert_eq!(
            translate_map(&map, "urn:source", "a", Some("urn:wrong"), false)
                .unwrap()
                .0
                .get_boolean("result"),
            Some(false)
        );
    }

    #[test]
    fn disjoint_and_conditional_mappings_are_not_positive_matches() {
        let map = json!({"group":[{"source":"urn:s","target":"urn:t","element":[{"code":"a","target":[{"code":"b","equivalence":"disjoint"}]}]}]});
        assert_eq!(
            translate_map(&map, "urn:s", "a", None, false)
                .unwrap()
                .0
                .get_boolean("result"),
            Some(false)
        );
        let map = json!({"group":[{"source":"urn:s","target":"urn:t","element":[{"code":"a","target":[{"code":"b","equivalence":"equivalent","dependsOn":[{}]}]}]}]});
        assert!(translate_map(&map, "urn:s", "a", None, false).is_err());
    }
}
