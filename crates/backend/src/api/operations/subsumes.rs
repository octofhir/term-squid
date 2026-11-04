use crate::{
    api::{
        coded_input::{CodedInput, InputType},
        extractors::{
            validate_json_format, FhirJson as Json, FhirParameters, FhirPath as Path,
            FhirQuery as Query,
        },
        parameters::{Parameter, ParameterValue, Parameters},
    },
    error::AppError,
    store::TerminologyStore,
};
use axum::extract::State;
use serde::Deserialize;
use std::sync::Arc;
use term_squid_core::Canonical;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubsumesParams {
    pub system: Option<String>,
    #[serde(rename = "codeA")]
    pub code_a: Option<String>,
    #[serde(rename = "codeB")]
    pub code_b: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "_format")]
    pub format: Option<String>,
}
struct Pair<'a> {
    a: CodedInput<'a>,
    b: CodedInput<'a>,
    system: Option<&'a str>,
    version: Option<&'a str>,
}
impl<'a> Pair<'a> {
    fn get(params: &'a SubsumesParams) -> Result<Self, AppError> {
        validate_json_format(params.format.as_deref())?;
        Ok(Self {
            a: CodedInput::primitive(None, params.code_a.as_deref(), None, None)?,
            b: CodedInput::primitive(None, params.code_b.as_deref(), None, None)?,
            system: params.system.as_deref(),
            version: params.version.as_deref(),
        })
    }
    fn post(params: &'a Parameters) -> Result<Self, AppError> {
        use InputType::*;
        params.validate_fields(&[
            ("system", Text, false),
            ("version", Text, false),
            ("codeA", Text, false),
            ("codeB", Text, false),
            ("codingA", Coding, false),
            ("codingB", Coding, false),
        ])?;
        Ok(Self {
            a: side(params, "codeA", "codingA")?,
            b: side(params, "codeB", "codingB")?,
            system: params.get_string("system"),
            version: params.get_string("version"),
        })
    }
    fn context(&self, instance: bool) -> Result<Option<Canonical>, AppError> {
        CodedInput::validate_text("system", self.system)?;
        CodedInput::validate_text("version", self.version)?;
        let system = self.system.or(self.a.system).or(self.b.system);
        if !instance && system.is_none() {
            return Err(AppError::BadRequest(
                "system parameter required at type level".into(),
            ));
        }
        // Resource identity wins over the legacy top-level version selector.
        // Coding.version still asserts the coded value's actual business version.
        let mut version = if instance {
            None
        } else {
            self.version.map(str::to_owned)
        };
        for input in [&self.a, &self.b] {
            let candidate = if input.system.is_some() {
                input.canonical(None)?.version
            } else {
                input.version.map(str::to_owned)
            };
            if version
                .as_deref()
                .zip(candidate.as_deref())
                .is_some_and(|(a, b)| a != b)
            {
                return Err(AppError::BadRequest(
                    "Conflicting subsumption business versions".into(),
                ));
            }
            version = version.or(candidate);
        }
        system
            .map(|system| Canonical::parse(system, version.as_deref()).map_err(AppError::from))
            .transpose()
    }
}
fn side<'a>(
    params: &'a Parameters,
    primitive: &str,
    complex: &str,
) -> Result<CodedInput<'a>, AppError> {
    if let Some(p) = params.get_parameter(complex) {
        if params.get_parameter(primitive).is_some() {
            return Err(AppError::BadRequest(format!(
                "Choose {primitive} or {complex}"
            )));
        }
        let Some(ParameterValue::ValueCoding(coding)) = p.value.as_ref() else {
            return Err(AppError::BadRequest(format!(
                "{complex} requires valueCoding"
            )));
        };
        CodedInput::primitive(
            coding.system.as_deref(),
            coding.code.as_deref(),
            coding.version.as_deref(),
            coding.display.as_deref(),
        )
    } else {
        CodedInput::primitive(None, params.get_string(primitive), None, None)
    }
}
pub async fn subsumes_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(params): Query<SubsumesParams>,
) -> Result<Json<Parameters>, AppError> {
    perform(&*store, Pair::get(&params)?, None).await
}
pub async fn subsumes_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    FhirParameters(params): FhirParameters,
) -> Result<Json<Parameters>, AppError> {
    perform(&*store, Pair::post(&params)?, None).await
}
pub async fn subsumes_instance_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Query(params): Query<SubsumesParams>,
) -> Result<Json<Parameters>, AppError> {
    perform(&*store, Pair::get(&params)?, Some(id)).await
}
pub async fn subsumes_instance_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    FhirParameters(params): FhirParameters,
) -> Result<Json<Parameters>, AppError> {
    perform(&*store, Pair::post(&params)?, Some(id)).await
}
async fn perform(
    store: &dyn TerminologyStore,
    pair: Pair<'_>,
    id: Option<Uuid>,
) -> Result<Json<Parameters>, AppError> {
    let context = pair.context(id.is_some())?;
    let resolved = if let Some(id) = id {
        store
            .resolve_subsumption_by_id(&id, pair.a.code, pair.b.code)
            .await?
    } else {
        let context = context.as_ref().expect("Type context checked");
        store
            .resolve_subsumption(
                &context.url,
                context.version.as_deref(),
                pair.a.code,
                pair.b.code,
            )
            .await?
    }
    .ok_or_else(|| AppError::NotFound("CodeSystem not found".into()))?;
    if context.as_ref().is_some_and(|c| {
        c.url != resolved.url
            || c.version
                .as_deref()
                .is_some_and(|v| Some(v) != resolved.version.as_deref())
    }) {
        return Err(AppError::BadRequest(
            "Subsumption context conflicts with the selected instance".into(),
        ));
    }
    for input in [&pair.a, &pair.b] {
        if let Some(system) = input.system {
            let context = Canonical::parse(system, input.version)?;
            if context.url != resolved.url {
                return Err(AppError::Terminology(term_squid_core::Error("Cross-system subsumption cannot be determined without established relationships".into())));
            }
            if context
                .version
                .as_deref()
                .is_some_and(|v| Some(v) != resolved.version.as_deref())
            {
                return Err(AppError::BadRequest(
                    "Coding version conflicts with the selected CodeSystem".into(),
                ));
            }
        } else if input
            .version
            .is_some_and(|v| Some(v) != resolved.version.as_deref())
        {
            return Err(AppError::BadRequest(
                "Coding version conflicts with the selected CodeSystem".into(),
            ));
        }
    }
    if resolved.hierarchy_meaning.as_deref() != Some("is-a") {
        return Err(AppError::BadRequest(
            "Subsumption requires an is-a hierarchy".into(),
        ));
    }
    if resolved.code_a.is_none() || resolved.code_b.is_none() {
        return Err(AppError::NotFound("Subsumption code not found".into()));
    }
    let outcome = if resolved.code_a == resolved.code_b {
        "equivalent"
    } else {
        match resolved.relationship {
            Some(true) => "subsumes",
            Some(false) => "subsumed-by",
            None => "not-subsumed",
        }
    };
    Ok(Json(Parameters::with_parameters(vec![Parameter::code(
        "outcome", outcome,
    )])))
}
