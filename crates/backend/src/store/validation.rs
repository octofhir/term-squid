//! Complex validation keeps all candidates on one read-only database snapshot.
use super::{compose_loader, native_valueset, ValueSetOperation, ValueSetRequest, ValueSetResult};
use crate::{error::AppError, models::ResolvedConcept};
use serde_json::Value;
use sqlx::PgPool;
use term_squid_core::{Canonical, PreparedSystem};
use tokio::sync::OwnedSemaphorePermit;
use uuid::Uuid;

pub struct ValidationCandidate<'a> {
    pub system: Option<&'a str>,
    pub version: Option<&'a str>,
    pub code: &'a str,
    pub display: Option<&'a str>,
}
#[derive(Clone, Copy)]
pub enum ValidationTarget<'a> {
    CodeSystem {
        url: &'a str,
        version: Option<&'a str>,
        id: Option<Uuid>,
        inline: Option<&'a Value>,
        alternatives: bool,
    },
    ValueSet(ValueSetRequest<'a>),
}
pub struct ValidationAnswer {
    pub valid: bool,
    pub display: Option<String>,
    pub version: Option<String>,
    pub message: Option<String>,
}
#[derive(Default)]
pub struct ValidationOptions {
    pub date: Option<String>,
    pub language: Option<String>,
    pub context: Option<String>,
}
impl ValidationAnswer {
    fn negative() -> Self {
        Self {
            valid: false,
            display: None,
            version: None,
            message: Some("No coding is valid in the selected terminology context".into()),
        }
    }
    fn member(
        display: Option<&str>,
        version: Option<&str>,
        expected: Option<&str>,
        abstract_code: bool,
        allow_abstract: bool,
    ) -> Self {
        let valid =
            (!abstract_code || allow_abstract) && expected.is_none_or(|v| Some(v) == display);
        Self {
            valid,
            display: display.map(str::to_owned),
            version: version.map(str::to_owned),
            message: (!valid).then(|| {
                if abstract_code && !allow_abstract {
                    "Abstract codes are not permitted".into()
                } else {
                    "Display does not match the registered concept".into()
                }
            }),
        }
    }
}
pub(super) async fn run(
    pool: &PgPool,
    mut target: ValidationTarget<'_>,
    candidates: &[ValidationCandidate<'_>],
    allow_abstract: bool,
    permit: OwnedSemaphorePermit,
    options: &ValidationOptions,
) -> Result<ValidationAnswer, AppError> {
    if let Some(date) = &options.date {
        super::options::date(date)?;
    }
    if let Some(language) = &options.language {
        super::options::language(language)?;
    }
    let context = options
        .context
        .as_deref()
        .map(super::selection::context)
        .transpose()?;
    if let Some((canonical, id)) = &context {
        let ValidationTarget::ValueSet(mut request) = target else {
            return Err(AppError::BadRequest(
                "context applies to ValueSet validation".into(),
            ));
        };
        if request.id.is_some()
            || request.inline.is_some()
            || request.url != "urn:term-squid:context"
        {
            return Err(AppError::BadRequest(
                "Choose context, URL, instance or inline ValueSet".into(),
            ));
        }
        request.url = &canonical.url;
        request.version = canonical.version.as_deref();
        request.id = *id;
        target = ValidationTarget::ValueSet(request);
    }
    if candidates.is_empty() || candidates.len() > 64 {
        return Err(AppError::BadRequest(
            "Validation requires 1..64 codings".into(),
        ));
    }
    let mut permit = Some(permit);
    let mut inline_system = None;
    let mut inline_complete = false;
    if let ValidationTarget::CodeSystem {
        url,
        version,
        id,
        inline,
        ..
    } = target
    {
        if id.is_none()
            && inline.is_none()
            && url == term_squid_core::ucum::SYSTEM
            && version.is_none_or(|v| v == term_squid_core::ucum::VERSION)
        {
            if let Some(language) = &options.language {
                let mut tx = pool.begin().await?;
                sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("SET LOCAL statement_timeout='10s'")
                    .execute(&mut *tx)
                    .await?;
                let mut bytes = 32 * 1024 * 1024;
                let overlays = super::supplements::load(
                    &mut tx,
                    url,
                    Some(term_squid_core::ucum::VERSION),
                    options.date.as_deref(),
                    &mut bytes,
                )
                .await?;
                tx.commit().await?;
                let candidates: Vec<_> = candidates
                    .iter()
                    .map(|c| {
                        (
                            c.system.map(str::to_owned),
                            c.version.map(str::to_owned),
                            c.code.to_owned(),
                            c.display.map(str::to_owned),
                        )
                    })
                    .collect();
                let language = language.clone();
                let version = version.map(str::to_owned);
                let work = permit.take().expect("Complex validation work permit");
                return tokio::task::spawn_blocking(move || {
                    let _work=work;
                    let mut unknown=None;let mut negative=ValidationAnswer::negative();
                    for (system,candidate_version,code,display) in &candidates {
                        if system.as_deref().is_some_and(|s|s!=term_squid_core::ucum::SYSTEM) {continue;}
                        match term_squid_core::ucum::UcumProvider::lookup(code,candidate_version.as_deref().or(version.as_deref())) {
                            Ok(Some(concept))=>{
                                let base=serde_json::json!({"url":term_squid_core::ucum::SYSTEM,"version":term_squid_core::ucum::VERSION,"caseSensitive":true,"concept":[{"code":concept.code,"display":concept.display}]});
                                let (designations,_)=super::supplements::enrich(&base,&overlays,&concept.code,true)?;
                                let answer=localized_answer(Some(&concept.display),Some(term_squid_core::ucum::VERSION),display.as_deref(),false,allow_abstract,&designations,Some(&language));
                                if answer.valid {return Ok(answer);} negative=answer;
                            },
                            Ok(None)=>{},Err(error)=>{unknown.get_or_insert(AppError::from(error));},
                        }
                    }
                    unknown.map_or(Ok(negative),Err)
                }).await.map_err(|e|AppError::Internal(e.into()))?;
            }
            let mut unknown = None;
            let mut negative = ValidationAnswer::negative();
            for candidate in candidates {
                if candidate.system.is_some_and(|s| s != url) {
                    continue;
                }
                match term_squid_core::ucum::UcumProvider::lookup(
                    candidate.code,
                    candidate.version.or(version),
                ) {
                    Ok(Some(concept)) => {
                        let answer = ValidationAnswer::member(
                            Some(&concept.display),
                            Some(term_squid_core::ucum::VERSION),
                            candidate.display,
                            false,
                            allow_abstract,
                        );
                        if answer.valid {
                            return Ok(answer);
                        }
                        negative = answer;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        unknown.get_or_insert(AppError::from(error));
                    }
                }
            }
            return unknown.map_or(Ok(negative), Err);
        }
        if let Some(resource) = inline {
            if resource["content"] == "supplement" {
                return Err(AppError::BadRequest(
                    "A supplement is not an independent CodeSystem".into(),
                ));
            }
            super::options::check_resource_date(resource, options.date.as_deref())?;
            if resource["resourceType"] != "CodeSystem"
                || serde_json::to_vec(resource)
                    .map_err(|e| AppError::BadRequest(e.to_string()))?
                    .len()
                    > 256 * 1024
            {
                return Err(AppError::BadRequest(
                    "Expected an inline CodeSystem of at most 256 KiB".into(),
                ));
            }
            inline_complete = resource["content"] == "complete";
            let mut resource = resource.clone();
            if resource.get("url").is_none() {
                resource["url"] = url.into();
            }
            let work = permit.take().expect("Complex operation work permit");
            let (result, returned) = tokio::task::spawn_blocking(move || {
                let result = (|| {
                    let definition = serde_json::from_value(resource)
                        .map_err(|e| AppError::BadRequest(format!("Invalid CodeSystem: {e}")))?;
                    PreparedSystem::prepare_for_import(definition).map_err(AppError::from)
                })();
                (result, work)
            })
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
            permit = Some(returned);
            inline_system = Some(result?);
        }
    }
    if let Some(system) = inline_system {
        let alternatives = matches!(
            target,
            ValidationTarget::CodeSystem {
                alternatives: true,
                ..
            }
        );
        return inline_answer(
            &system,
            candidates,
            inline_complete,
            allow_abstract,
            alternatives,
            match target {
                ValidationTarget::CodeSystem { inline, .. } => inline,
                _ => None,
            },
            options,
        );
    }
    let mut tx = pool.begin().await?;
    sqlx::Executor::execute(&mut *tx,sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY; SET LOCAL statement_timeout='10s'; SET LOCAL work_mem='4MB'")).await?;
    let fixed = if let ValidationTarget::CodeSystem {
        url,
        version,
        id,
        inline: None,
        ..
    } = target
    {
        let canonical = Canonical::parse(url, version)?;
        let temporal_id = if options.date.is_some() {
            Some(
                super::selection::id(
                    &mut tx,
                    super::selection::Family::System,
                    &canonical,
                    id,
                    options.date.as_deref(),
                )
                .await?,
            )
        } else {
            id
        };
        let row:Option<(String,Option<String>,Uuid)>=sqlx::query_as("SELECT url,version,id FROM code_systems WHERE ($3::uuid IS NOT NULL AND id=$3) OR ($3::uuid IS NULL AND url=$1 AND ($2::text IS NULL OR version=$2)) ORDER BY updated_at DESC,id LIMIT 1").bind(&canonical.url).bind(&canonical.version).bind(temporal_id).fetch_optional(&mut *tx).await?;
        Some((
            row.ok_or_else(|| AppError::NotFound("CodeSystem context not found".into()))?,
            id.is_some() || canonical.version.is_some() || options.date.is_some(),
        ))
    } else {
        None
    };
    let mut unknown = None;
    let mut negative = ValidationAnswer::negative();
    let mut metadata = Metadata {
        cache: std::collections::BTreeMap::new(),
        remaining: 32 * 1024 * 1024,
        date: options.date.clone(),
        permit,
    };
    let expansion_options = super::ExpansionOptions {
        date: options.date.clone(),
        ..Default::default()
    };
    for candidate in candidates {
        let outcome: Result<ValidationAnswer, AppError> = async {
            match target {
                ValidationTarget::CodeSystem {
                    url,
                    version,
                    id,
                    alternatives,
                    ..
                } => {
                    let ((selected_url, selected_version, selected_id), fixed_version) =
                        fixed.as_ref().expect("Stored context resolved");
                    if candidate.system.is_some_and(|s| s != selected_url)
                        || *fixed_version
                            && candidate
                                .version
                                .is_some_and(|v| Some(v) != selected_version.as_deref())
                    {
                        if !alternatives {
                            return Err(AppError::BadRequest(
                                "Input context conflicts with selected CodeSystem".into(),
                            ));
                        }
                        return Ok(ValidationAnswer::negative());
                    }
                    let selected = Canonical::parse(url, version)?;
                    let resolved = sqlx::query_as::<_, ResolvedConcept>(include_str!(
                        "concept_resolution.sql"
                    ))
                    .bind(&selected.url)
                    .bind(if *fixed_version {
                        selected_version.as_deref()
                    } else {
                        candidate.version
                    })
                    .bind(candidate.code)
                    .bind(if *fixed_version {
                        Some(*selected_id)
                    } else {
                        id
                    })
                    .fetch_optional(&mut *tx)
                    .await?
                    .ok_or_else(|| {
                        AppError::NotFound("Coding business version is unavailable".into())
                    })?;
                    if resolved.native_provider.as_deref() == Some("loinc-csv") {
                        term_squid_core::loinc::require_primary_code(candidate.code)?;
                    }
                    if resolved.native_provider.as_deref() == Some("snomed-rf2") {
                        term_squid_core::snomed::reject_expression(candidate.code)?;
                    }
                    if resolved.content_mode.as_deref() == Some("supplement") {
                        return Err(AppError::BadRequest(
                            "A supplement URL cannot identify Coding.system".into(),
                        ));
                    }
                    if resolved.code.is_none() {
                        if resolved.content_mode.as_deref() != Some("complete") {
                            return Err(AppError::BadRequest(
                                "CodeSystem content is incomplete; membership is unknown".into(),
                            ));
                        }
                        return Ok(ValidationAnswer::negative());
                    }
                    let abstract_code = resolved.properties.as_ref().is_some_and(|properties| {
                        flag(&properties.0, "abstract") || flag(&properties.0, "notSelectable")
                    });
                    let designations = if options.language.is_some() {
                        let id = if *fixed_version {
                            *selected_id
                        } else {
                            super::selection::id(
                                &mut tx,
                                super::selection::Family::System,
                                &Canonical::parse(&resolved.url, resolved.version.as_deref())?,
                                None,
                                None,
                            )
                            .await?
                        };
                        designations(
                            &mut tx,
                            id,
                            resolved.native_provider.as_deref(),
                            resolved.code.as_deref().unwrap(),
                            &mut metadata,
                        )
                        .await?
                    } else {
                        Vec::new()
                    };
                    Ok(localized_answer(
                        resolved.display.as_deref(),
                        resolved.version.as_deref(),
                        candidate.display,
                        abstract_code,
                        allow_abstract,
                        &designations,
                        options.language.as_deref(),
                    ))
                }
                ValidationTarget::ValueSet(mut request) => {
                    let inferred = if candidate.system.is_none() {
                        Some(
                            super::selection::infer_system(
                                &mut tx,
                                request,
                                options.date.as_deref(),
                            )
                            .await?,
                        )
                    } else {
                        None
                    };
                    let system = candidate
                        .system
                        .or(inferred.as_deref())
                        .expect("Validated or inferred system");
                    request.system_context = candidate.version.map(|v| (system, v));
                    let operation = ValueSetOperation::Find {
                        system,
                        version: candidate.version,
                        code: candidate.code,
                    };
                    let implicit = request.id.is_none() && request.inline.is_none() && {
                        let canonical = Canonical::parse(request.url, request.version)?;
                        term_squid_core::snomed::implicit_definition(&canonical)?.is_some()
                            || term_squid_core::loinc::implicit(&canonical)?.is_some()
                    };
                    let result = if implicit {
                        native_valueset::query_options(
                            &mut tx,
                            request,
                            operation,
                            Some(&expansion_options),
                        )
                        .await?
                    } else {
                        match compose_loader::load_options(
                            &mut tx,
                            request,
                            &mut metadata.remaining,
                            Some(&expansion_options),
                        )
                        .await
                        {
                            Ok(loaded) => {
                                let work = metadata
                                    .permit
                                    .take()
                                    .expect("Complex operation work permit");
                                let (result, returned) = tokio::task::spawn_blocking(move || {
                                    let result = compose_loader::prepare(loaded);
                                    (result, work)
                                })
                                .await
                                .map_err(|e| AppError::Internal(e.into()))?;
                                metadata.permit = Some(returned);
                                ValueSetResult::Finite(result?)
                            }
                            Err(AppError::NativeValueSet) => {
                                native_valueset::query_options(
                                    &mut tx,
                                    request,
                                    operation,
                                    Some(&expansion_options),
                                )
                                .await?
                            }
                            Err(error) => return Err(error),
                        }
                    };
                    let member = match &result {
                        ValueSetResult::Finite(plan) => {
                            plan.members.find(system, candidate.version, candidate.code)
                        }
                        ValueSetResult::Native(page) => page.members.first(),
                    };
                    let Some(member) = member else {
                        return Ok(ValidationAnswer::negative());
                    };
                    let designations = if options.language.is_some() {
                        let key = Canonical::parse(&member.system, member.version.as_deref())?;
                        let id = super::selection::id(
                            &mut tx,
                            super::selection::Family::System,
                            &key,
                            None,
                            None,
                        )
                        .await?;
                        let provider: Option<String> = sqlx::query_scalar(
                            "SELECT native_provider FROM code_systems WHERE id=$1",
                        )
                        .bind(id)
                        .fetch_one(&mut *tx)
                        .await?;
                        designations(
                            &mut tx,
                            id,
                            provider.as_deref(),
                            &member.code,
                            &mut metadata,
                        )
                        .await?
                    } else {
                        Vec::new()
                    };
                    Ok(localized_answer(
                        member.display.as_deref(),
                        member.version.as_deref(),
                        candidate.display,
                        member.abstract_code,
                        allow_abstract,
                        &designations,
                        options.language.as_deref(),
                    ))
                }
            }
        }
        .await;
        match outcome {
            Ok(answer) if answer.valid => {
                tx.commit().await?;
                return Ok(answer);
            }
            Ok(answer) => {
                if answer.display.is_some() {
                    negative = answer;
                }
            }
            Err(
                error @ (AppError::Database(_)
                | AppError::Internal(_)
                | AppError::Overloaded
                | AppError::Generation(_)),
            ) => return Err(error),
            Err(error) => {
                if unknown.is_none() {
                    unknown = Some(error);
                }
            }
        }
    }
    tx.commit().await?;
    if let Some(error) = unknown {
        Err(error)
    } else {
        Ok(negative)
    }
}
fn flag(value: &Value, name: &str) -> bool {
    value.get(name) == Some(&Value::Bool(true))
        || value.as_array().is_some_and(|properties| {
            properties
                .iter()
                .any(|p| p["code"] == name && p["valueBoolean"] == true)
        })
}

struct Metadata {
    cache: std::collections::BTreeMap<Uuid, std::sync::Arc<Value>>,
    remaining: i64,
    date: Option<String>,
    permit: Option<OwnedSemaphorePermit>,
}
async fn designations(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    provider: Option<&str>,
    code: &str,
    metadata: &mut Metadata,
) -> Result<Vec<Value>, AppError> {
    let supplemental =
        super::supplements::designations(tx, id, code, metadata.date.as_deref()).await?;
    if provider == Some("snomed-rf2") {
        let rows:Vec<(String,String,String)>=sqlx::query_as("SELECT d.language_code,d.type_id,d.term FROM snomed_descriptions d WHERE d.code_system_id=$1 AND d.concept_code=$2 AND d.active ORDER BY EXISTS(SELECT 1 FROM snomed_language_members l JOIN snomed_releases r ON r.code_system_id=l.code_system_id AND r.dialect=l.refset_id WHERE l.code_system_id=d.code_system_id AND l.description_id=d.id AND l.active AND l.acceptability_id='900000000000548007') DESC,d.id LIMIT 257").bind(id).bind(code).fetch_all(&mut **tx).await?;
        if rows.len() > 256 {
            return Err(AppError::BadRequest(
                "Concept designations exceed 256 entries".into(),
            ));
        }
        let mut result:Vec<_>=rows.into_iter().map(|(language,kind,value)|serde_json::json!({"language":language,"use":{"system":"http://snomed.info/sct","code":kind},"value":value})).collect();
        result.extend(supplemental);
        return Ok(result);
    }
    if provider.is_some() {
        return Ok(supplemental);
    }
    if !metadata.cache.contains_key(&id) {
        let (content,bytes):(Option<sqlx::types::Json<Value>>,i64)=sqlx::query_as("SELECT CASE WHEN octet_length(content::text)<=$2 THEN content END,octet_length(content::text)::bigint FROM code_systems WHERE id=$1").bind(id).bind(metadata.remaining).fetch_one(&mut **tx).await?;
        metadata.remaining -= bytes;
        let content = content
            .ok_or_else(|| {
                AppError::BadRequest("Validation source metadata exceeds 32 MiB".into())
            })?
            .0;
        metadata.cache.insert(id, std::sync::Arc::new(content));
    }
    let resource = metadata.cache[&id].clone();
    let code = code.to_owned();
    let permit = metadata
        .permit
        .take()
        .expect("Complex operation work permit");
    let (result, permit) = tokio::task::spawn_blocking(move || {
        (super::lookup::details(&resource, &code, false), permit)
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    metadata.permit = Some(permit);
    result.map(|r| {
        let mut result = r.0;
        result.extend(supplemental);
        result
    })
}
fn localized_answer(
    display: Option<&str>,
    version: Option<&str>,
    expected: Option<&str>,
    abstract_code: bool,
    allow_abstract: bool,
    designations: &[Value],
    language: Option<&str>,
) -> ValidationAnswer {
    let selected_language = language
        .and_then(|language| super::expansion::display_language_range(designations, language));
    let preferred =
        language.and_then(|language| super::expansion::preferred(designations, language));
    let mut answer = ValidationAnswer::member(
        preferred.or(display),
        version,
        expected,
        abstract_code,
        allow_abstract,
    );
    let matching_designation = expected.is_some_and(|expected| {
        designations.iter().any(|d| {
            d["value"] == expected
                && match language {
                    None => true,
                    Some(_) => selected_language.is_some_and(|range| {
                        d["language"]
                            .as_str()
                            .is_some_and(|actual| super::options::language_matches(range, actual))
                    }),
                }
        })
    });
    if (!abstract_code || allow_abstract) && matching_designation {
        answer.valid = true;
        answer.message = None;
    }
    answer
}

fn inline_answer(
    system: &PreparedSystem,
    candidates: &[ValidationCandidate<'_>],
    complete: bool,
    allow_abstract: bool,
    alternatives: bool,
    resource: Option<&Value>,
    options: &ValidationOptions,
) -> Result<ValidationAnswer, AppError> {
    let mut unknown = false;
    let folded = (!system.case_sensitive).then(|| {
        system
            .concepts
            .iter()
            .map(|(code, concept)| (code.to_lowercase(), concept))
            .collect::<std::collections::BTreeMap<_, _>>()
    });
    let mut negative = ValidationAnswer::negative();
    for candidate in candidates {
        if candidate.system.is_some_and(|s| s != system.canonical.url)
            || candidate
                .version
                .is_some_and(|v| Some(v) != system.canonical.version.as_deref())
        {
            if !alternatives {
                return Err(AppError::BadRequest(
                    "Input context conflicts with inline CodeSystem".into(),
                ));
            }
            continue;
        }
        let concept = if let Some(folded) = &folded {
            folded.get(&candidate.code.to_lowercase()).copied()
        } else {
            system.concepts.get(candidate.code)
        };
        if let Some(concept) = concept {
            let abstract_code = concept.property.iter().any(|p| {
                matches!(p.code.as_str(), "abstract" | "notSelectable")
                    && p.scalar() == Some(&Value::Bool(true))
            });
            let designations = resource
                .filter(|_| options.language.is_some())
                .map(|r| super::lookup::details(r, &concept.code, false).map(|r| r.0))
                .transpose()?
                .unwrap_or_default();
            let answer = localized_answer(
                concept.display.as_deref(),
                system.canonical.version.as_deref(),
                candidate.display,
                abstract_code,
                allow_abstract,
                &designations,
                options.language.as_deref(),
            );
            if answer.valid {
                return Ok(answer);
            }
            negative = answer;
        } else if !complete {
            unknown = true;
        }
    }
    if unknown {
        Err(AppError::BadRequest(
            "Inline CodeSystem content is incomplete; membership is unknown".into(),
        ))
    } else {
        Ok(negative)
    }
}
