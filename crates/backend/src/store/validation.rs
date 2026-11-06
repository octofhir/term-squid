//! Complex validation keeps all candidates on one read-only database snapshot.
use super::{compose_loader, native_valueset, ValueSetOperation, ValueSetRequest, ValueSetResult};
use crate::{error::AppError, models::ResolvedConcept};
use serde_json::Value;
use sqlx::PgPool;
use term_squid_core::{Canonical, PreparedSystem};
use tokio::sync::OwnedSemaphorePermit;
use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub enum ValidationInput {
    Primitive,
    Coding,
    CodeableConcept(usize),
}
impl ValidationInput {
    pub fn code_path(self) -> String {
        match self {
            Self::CodeableConcept(index) => format!("CodeableConcept.coding[{index}].code"),
            _ => "Coding.code".into(),
        }
    }
    pub fn display_path(self) -> String {
        match self {
            Self::Primitive => "display".into(),
            Self::Coding => "Coding.display".into(),
            Self::CodeableConcept(index) => format!("CodeableConcept.coding[{index}].display"),
        }
    }
}

pub struct ValidationCandidate<'a> {
    pub origin: ValidationInput,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationFailure {
    Display,
    Abstract,
    NotInSet,
    UnknownCode,
}

pub struct ValidationAnswer {
    pub valid: bool,
    pub display: Option<String>,
    pub version: Option<String>,
    pub message: Option<String>,
    pub coding: Option<ValidationCoding>,
    pub failure: Option<ValidationFailure>,
}
pub struct ValidationCoding {
    pub system: String,
    pub code: String,
    pub normalized: Option<String>,
    pub issues: Vec<Value>,
}
#[derive(Clone, Default)]
pub struct ValidationOptions {
    pub date: Option<String>,
    pub language: Option<String>,
    pub context: Option<String>,
    pub coding_diagnostics: bool,
    pub lenient_display: bool,
    pub system_version: Vec<String>,
    pub check_system_version: Vec<String>,
    pub force_system_version: Vec<String>,
}
impl ValidationAnswer {
    fn display_policy(mut self, lenient: bool) -> Self {
        if lenient && self.failure == Some(ValidationFailure::Display) {
            self.valid = true;
        }
        self
    }
    pub fn with_code(
        mut self,
        system: &str,
        version: Option<&str>,
        code: &str,
        case_sensitive: bool,
        input: &ValidationCandidate<'_>,
    ) -> Self {
        super::validation_diagnostics::coding(
            &mut self,
            system,
            version,
            code,
            case_sensitive,
            input,
        );
        self
    }
    pub fn unknown_code(
        system: &str,
        version: Option<&str>,
        input: &ValidationCandidate<'_>,
    ) -> Self {
        super::validation_diagnostics::unknown_code(system, version, input)
    }
    pub fn missing_member(
        result: &ValueSetResult,
        system: &str,
        input: &ValidationCandidate<'_>,
    ) -> Self {
        match result {
            ValueSetResult::Finite(plan) => super::validation_diagnostics::missing_member(
                &plan.resource,
                system,
                input,
                &plan.sources,
                Some(&plan.knowledge),
            ),
            ValueSetResult::Native(page) => super::validation_diagnostics::missing_member(
                &page.resource,
                system,
                input,
                &page.sources,
                page.finite.as_ref().map(|finite| &finite.knowledge),
            ),
        }
    }
    pub fn with_coding(
        mut self,
        member: &term_squid_core::Member,
        input: &ValidationCandidate<'_>,
    ) -> Self {
        super::validation_diagnostics::member(&mut self, member, input);
        self
    }
    fn negative() -> Self {
        Self {
            valid: false,
            display: None,
            version: None,
            message: Some("No coding is valid in the selected terminology context".into()),
            coding: None,
            failure: Some(ValidationFailure::NotInSet),
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
            failure: if abstract_code && !allow_abstract {
                Some(ValidationFailure::Abstract)
            } else {
                (!valid).then_some(ValidationFailure::Display)
            },
            display: display.map(str::to_owned),
            version: version.map(str::to_owned),
            coding: None,
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
    if matches!(target, ValidationTarget::CodeSystem { .. })
        && (!options.system_version.is_empty()
            || !options.check_system_version.is_empty()
            || !options.force_system_version.is_empty())
    {
        return Err(AppError::BadRequest(
            "System version directives apply to ValueSet validation".into(),
        ));
    }
    if let Some(date) = &options.date {
        super::options::date(date)?;
    }
    if let Some(language) = &options.language {
        super::options::language(language)?;
    }
    let expansion_options = super::ExpansionOptions {
        date: options.date.clone(),
        system_version: options.system_version.clone(),
        check_system_version: options.check_system_version.clone(),
        force_system_version: options.force_system_version.clone(),
        ..Default::default()
    };
    expansion_options.validate()?;
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
                            c.origin,
                        )
                    })
                    .collect();
                let language = language.clone();
                let version = version.map(str::to_owned);
                let diagnostics = options.coding_diagnostics;
                let lenient = options.lenient_display;
                let work = permit.take().expect("Complex validation work permit");
                return tokio::task::spawn_blocking(move || {
                    let _work=work;
                    let mut unknown=None;let mut negative=ValidationAnswer::negative();
                    for (system,candidate_version,code,display,origin) in &candidates {
                        let input=ValidationCandidate {origin:*origin,system:system.as_deref(),version:candidate_version.as_deref(),code,display:display.as_deref()};
                        if system.as_deref().is_some_and(|s|s!=term_squid_core::ucum::SYSTEM) {continue;}
                        match term_squid_core::ucum::UcumProvider::lookup(code,candidate_version.as_deref().or(version.as_deref())) {
                            Ok(Some(concept))=>{
                                let base=serde_json::json!({"url":term_squid_core::ucum::SYSTEM,"version":term_squid_core::ucum::VERSION,"caseSensitive":true,"concept":[{"code":concept.code,"display":concept.display}]});
                                let (designations,_)=super::supplements::enrich(&base,&overlays,&concept.code,true)?;
                                let mut answer=localized_answer(Some(&concept.display),Some(term_squid_core::ucum::VERSION),display.as_deref(),false,allow_abstract,LabelView { primary: Some(&concept.display), designations: &designations },Some(&language)).display_policy(lenient);
                                if diagnostics {answer=answer.with_code(term_squid_core::ucum::SYSTEM,Some(term_squid_core::ucum::VERSION),&concept.code,true,&input);}
                                if answer.valid {return Ok(answer);} negative=answer;
                            },
                            Ok(None)=>{if diagnostics {negative=ValidationAnswer::unknown_code(term_squid_core::ucum::SYSTEM,Some(term_squid_core::ucum::VERSION),&input);}},Err(error)=>{unknown.get_or_insert(AppError::from(error));},
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
                        let mut answer = ValidationAnswer::member(
                            Some(&concept.display),
                            Some(term_squid_core::ucum::VERSION),
                            candidate.display,
                            false,
                            allow_abstract,
                        )
                        .display_policy(options.lenient_display);
                        if options.coding_diagnostics {
                            answer = answer.with_code(
                                term_squid_core::ucum::SYSTEM,
                                Some(term_squid_core::ucum::VERSION),
                                &concept.code,
                                true,
                                candidate,
                            );
                        }
                        if answer.valid {
                            return Ok(answer);
                        }
                        negative = answer;
                    }
                    Ok(None) => {
                        if options.coding_diagnostics {
                            negative = ValidationAnswer::unknown_code(
                                term_squid_core::ucum::SYSTEM,
                                Some(term_squid_core::ucum::VERSION),
                                candidate,
                            );
                        }
                    }
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
                    PreparedSystem::prepare_for_r4_source(definition).map_err(AppError::from)
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
        let row: Option<(String, Option<String>, Uuid)> = if let Some(entry) =
            super::transient::select(
                super::transient::Kind::System,
                &canonical.url,
                canonical.version.as_deref(),
                temporal_id,
                options.date.as_deref(),
            )? {
            Some((
                entry.resource["url"].as_str().unwrap().into(),
                entry.resource["version"].as_str().map(str::to_owned),
                entry.id,
            ))
        } else {
            sqlx::query_as("SELECT url,version,id FROM code_systems WHERE ($3::uuid IS NOT NULL AND id=$3) OR ($3::uuid IS NULL AND url=$1 AND ($2::text IS NULL OR version=$2)) ORDER BY updated_at DESC,id LIMIT 1").bind(&canonical.url).bind(&canonical.version).bind(temporal_id).fetch_optional(&mut *tx).await?
        };
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
        properties: std::collections::BTreeMap::new(),
        remaining: 32 * 1024 * 1024,
        date: options.date.clone(),
        permit,
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
                    let temporary = super::transient::select(
                        super::transient::Kind::System,
                        &selected.url,
                        if *fixed_version {
                            selected_version.as_deref()
                        } else {
                            candidate.version
                        },
                        if *fixed_version {
                            Some(*selected_id)
                        } else {
                            id
                        },
                        options.date.as_deref(),
                    )?;
                    let resolved = if let Some(entry) = temporary {
                        entry.concept(candidate.code)?
                    } else {
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
                        resolved
                    };
                    if resolved.native_provider.as_deref() == Some("loinc-csv") {
                        term_squid_core::loinc::require_primary_code(candidate.code)?;
                    }
                    if resolved.native_provider.as_deref() == Some("snomed-rf2") {
                        term_squid_core::snomed::reject_expression(candidate.code)?;
                    }
                    if resolved.native_provider.as_deref() == Some("rxnorm-rrf") {
                        term_squid_core::rxnorm::require_code(candidate.code)?;
                        if resolved.code.is_none() {
                            return Err(native_valueset::unsupported("RXCUI is outside the loaded active RxNorm projection; historical existence is unavailable"));
                        }
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
                        return Ok(if options.coding_diagnostics {
                            ValidationAnswer::unknown_code(
                                &resolved.url,
                                resolved.version.as_deref(),
                                candidate,
                            )
                        } else {
                            ValidationAnswer::negative()
                        });
                    }
                    let properties = resolved.properties.as_ref().map(|properties| &properties.0);
                    let needs_abstract = !allow_abstract && properties.is_some_and(has_properties);
                    let source_id = if options.language.is_some()
                        || candidate.display.is_some()
                        || needs_abstract
                    {
                        Some(if *fixed_version {
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
                        })
                    } else {
                        None
                    };
                    let abstract_code = if needs_abstract {
                        abstract_property(
                            &mut tx,
                            source_id.unwrap(),
                            properties.unwrap(),
                            &mut metadata,
                        )
                        .await?
                    } else {
                        false
                    };
                    let labels = if options.language.is_some() || candidate.display.is_some() {
                        let id = source_id.expect("Localized source identity");
                        labels(
                            &mut tx,
                            id,
                            resolved.native_provider.as_deref(),
                            resolved.code.as_deref().unwrap(),
                            &mut metadata,
                        )
                        .await?
                    } else {
                        ValidationLabels::default()
                    };
                    let mut answer = localized_answer_case(
                        resolved.display.as_deref(),
                        resolved.version.as_deref(),
                        candidate.display,
                        abstract_code,
                        allow_abstract,
                        labels.view(),
                        options.language.as_deref(),
                        resolved.url == term_squid_core::rxnorm::SYSTEM,
                    )
                    .display_policy(options.lenient_display);
                    if options.coding_diagnostics {
                        answer = answer.with_code(
                            &resolved.url,
                            resolved.version.as_deref(),
                            resolved.code.as_deref().expect("Known code"),
                            false,
                            candidate,
                        );
                    }
                    Ok(answer)
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
                || term_squid_core::rxnorm::implicit(&canonical)?.is_some()
                    };
                    let mut projected_labels = None;
                    let result = if implicit {
                        native_valueset::query_options(
                            &mut tx,
                            request,
                            operation,
                            Some(&expansion_options),
                            &mut metadata.permit,
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
                                let ucum_code = (system == term_squid_core::ucum::SYSTEM
                                    && (options.language.is_some() || candidate.display.is_some()))
                                .then(|| candidate.code.to_owned());
                                let (result, returned) = tokio::task::spawn_blocking(move || {
                                    let result = compose_loader::prepare_rich(loaded).map(
                                        |(plan, sources)| {
                                            let labels = ucum_code.as_deref().and_then(|code| {
                                                ucum_projection_labels(&sources, code)
                                            });
                                            (plan, labels)
                                        },
                                    );
                                    (result, work)
                                })
                                .await
                                .map_err(|e| AppError::Internal(e.into()))?;
                                metadata.permit = Some(returned);
                                let (plan, labels) = result?;
                                projected_labels = labels;
                                ValueSetResult::Finite(plan)
                            }
                            Err(AppError::NativeValueSet) => {
                                native_valueset::query_options(
                                    &mut tx,
                                    request,
                                    operation,
                                    Some(&expansion_options),
                                    &mut metadata.permit,
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
                        if options.coding_diagnostics {
                            return Ok(ValidationAnswer::missing_member(
                                &result, system, candidate,
                            ));
                        }
                        return Ok(ValidationAnswer::negative());
                    };
                    let labels = if let Some(labels) = projected_labels {
                        labels
                    } else if options.language.is_some() || candidate.display.is_some() {
                        if let Some(labels) = match &result {
                            ValueSetResult::Native(page) => {
                                page.finite.as_ref().and_then(|finite| finite.get(member))
                            }
                            ValueSetResult::Finite(_) => None,
                        } {
                            ValidationLabels {
                                primary: labels.primary.clone(),
                                designations: labels.designations.clone(),
                            }
                        } else {
                            let key = Canonical::parse(&member.system, member.version.as_deref())?;
                            let id = super::selection::id(
                                &mut tx,
                                super::selection::Family::System,
                                &key,
                                None,
                                None,
                            )
                            .await?;
                            let provider: Option<String> = if super::transient::by_id(id).is_some()
                            {
                                None
                            } else {
                                sqlx::query_scalar(
                                    "SELECT native_provider FROM code_systems WHERE id=$1",
                                )
                                .bind(id)
                                .fetch_one(&mut *tx)
                                .await?
                            };
                            labels(
                                &mut tx,
                                id,
                                provider.as_deref(),
                                &member.code,
                                &mut metadata,
                            )
                            .await?
                        }
                    } else {
                        ValidationLabels::default()
                    };
                    let mut answer = localized_answer_case(
                        member.display.as_deref(),
                        member.version.as_deref(),
                        candidate.display,
                        member.abstract_code,
                        allow_abstract,
                        labels.view(),
                        options.language.as_deref(),
                        member.system == term_squid_core::rxnorm::SYSTEM,
                    )
                    .display_policy(options.lenient_display);
                    if options.coding_diagnostics {
                        super::validation_diagnostics::member(&mut answer, member, candidate);
                    }
                    Ok(answer)
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
                if answer.display.is_some() || answer.coding.is_some() {
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
fn has_properties(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Array(properties) => !properties.is_empty(),
        Value::Object(properties) => !properties.is_empty(),
        _ => true,
    }
}
async fn abstract_property(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    properties: &Value,
    metadata: &mut Metadata,
) -> Result<bool, AppError> {
    if !metadata.properties.contains_key(&id) {
        let (definitions, bytes) = if let Some(entry) = super::transient::by_id(id) {
            let definitions = entry
                .resource
                .get("property")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([]));
            let bytes = serde_json::to_vec(&definitions)
                .map_err(|error| AppError::Internal(error.into()))?
                .len() as i64;
            (Some(sqlx::types::Json(definitions)), bytes)
        } else {
            sqlx::query_as::<_,(Option<sqlx::types::Json<Value>>,i64)>("SELECT CASE WHEN octet_length(COALESCE(content->'property','[]')::text)<=65536 THEN COALESCE(content->'property','[]') END,octet_length(COALESCE(content->'property','[]')::text)::bigint FROM code_systems WHERE id=$1").bind(id).fetch_one(&mut **tx).await?
        };
        metadata.remaining -= bytes;
        if bytes > 65536 || metadata.remaining < 0 {
            return Err(AppError::BadRequest(
                "Validation property metadata limit exceeded".into(),
            ));
        }
        let definitions = definitions
            .ok_or_else(|| {
                AppError::BadRequest("Validation property metadata limit exceeded".into())
            })?
            .0;
        let definitions =
            serde_json::from_value::<Vec<term_squid_core::PropertyDefinition>>(definitions)
                .map_err(|error| {
                    AppError::BadRequest(format!("Invalid property metadata: {error}"))
                })?;
        let definitions = term_squid_core::PropertyDefinitions::prepare(&definitions)?;
        definitions.validate_r4_types()?;
        metadata.properties.insert(id, definitions);
    }
    let definitions = &metadata.properties[&id];
    let grouping = |code: &str| {
        matches!(
            definitions.meaning(code),
            Some("abstract" | "notSelectable")
        )
    };
    let invalid = || {
        AppError::Terminology(term_squid_core::Error(
            "Abstract/notSelectable properties require one valueBoolean".into(),
        ))
    };
    let mut abstract_code = false;
    if let Some(fields) = properties.as_object() {
        for (code, value) in fields {
            if grouping(code) {
                abstract_code |= value.as_bool().ok_or_else(invalid)?;
            }
        }
        return Ok(abstract_code);
    }
    let properties = properties.as_array().ok_or_else(invalid)?;
    for property in properties {
        if property["code"].as_str().is_some_and(grouping) {
            let property: term_squid_core::Property =
                serde_json::from_value(property.clone()).map_err(|_| invalid())?;
            let (kind, value) = property.typed_value().map_err(|_| invalid())?;
            if kind != "valueBoolean" {
                return Err(invalid());
            }
            abstract_code |= value.as_bool().ok_or_else(invalid)?;
        }
    }
    Ok(abstract_code)
}

struct Metadata {
    properties: std::collections::BTreeMap<Uuid, term_squid_core::PropertyDefinitions>,
    cache: std::collections::BTreeMap<Uuid, std::sync::Arc<Value>>,
    remaining: i64,
    date: Option<String>,
    permit: Option<OwnedSemaphorePermit>,
}
/// These labels are already validated and enriched on the membership snapshot.
/// The bundled projection has no database resource identity to resolve afterwards.
fn ucum_projection_labels(
    sources: &std::collections::BTreeMap<Canonical, Value>,
    code: &str,
) -> Option<ValidationLabels> {
    let source = sources.values().find(|source| {
        source["url"] == term_squid_core::ucum::SYSTEM
            && source["version"] == term_squid_core::ucum::VERSION
            && source["content"] == "fragment"
    })?;
    let concept = source["concept"]
        .as_array()?
        .iter()
        .find(|concept| concept["code"] == code)?;
    Some(ValidationLabels {
        primary: concept["display"].as_str().map(str::to_owned),
        designations: concept["designation"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
    })
}

#[derive(Default)]
struct ValidationLabels {
    primary: Option<String>,
    designations: Vec<Value>,
}
struct LabelView<'a> {
    primary: Option<&'a str>,
    designations: &'a [Value],
}
impl ValidationLabels {
    fn view(&self) -> LabelView<'_> {
        LabelView {
            primary: self.primary.as_deref(),
            designations: &self.designations,
        }
    }
}
type ProjectedLabels = (Option<String>, Option<sqlx::types::Json<Value>>, i64, bool);
async fn labels(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    provider: Option<&str>,
    code: &str,
    metadata: &mut Metadata,
) -> Result<ValidationLabels, AppError> {
    if let Some(entry) = super::transient::by_id(id) {
        if !metadata.cache.contains_key(&id) {
            metadata.remaining -= entry.bytes as i64;
            if metadata.remaining < 0 {
                return Err(AppError::BadRequest(
                    "Validation source metadata exceeds 32 MiB".into(),
                ));
            }
            metadata.cache.insert(id, entry.resource.clone());
        }
        return Ok(ValidationLabels {
            primary: entry
                .system
                .as_ref()
                .and_then(|system| system.concept(code))
                .and_then(|concept| concept.display.clone()),
            designations: entry.designations(code)?,
        });
    }
    let supplemental = super::supplements::designations(
        tx,
        id,
        code,
        metadata.date.as_deref(),
        &mut metadata.remaining,
    )
    .await?;
    if provider == Some("snomed-rf2") {
        let mut result =
            super::designations::snomed(tx, id, code, true, &mut metadata.remaining).await?;
        super::designations::extend(&mut result, supplemental)?;
        return Ok(ValidationLabels {
            primary: None,
            designations: result,
        });
    }
    if provider.is_some() {
        let mut designations = if provider == Some("rxnorm-rrf") {
            super::designations::projected(tx, id, code, &mut metadata.remaining).await?
        } else {
            Vec::new()
        };
        super::designations::extend(&mut designations, supplemental)?;
        let row: Option<(Option<String>, i64)> = sqlx::query_as(
            "SELECT CASE WHEN octet_length(COALESCE(display,''))<=$3 THEN display END,octet_length(COALESCE(display,''))::bigint FROM concepts WHERE code_system_id=$1 AND code=$2",
        )
        .bind(id)
        .bind(code)
        .bind(metadata.remaining)
        .fetch_optional(&mut **tx)
        .await?;
        let primary = if let Some((display, bytes)) = row {
            metadata.remaining -= bytes;
            if metadata.remaining < 0 {
                return Err(AppError::BadRequest(
                    "Validation label metadata exceeds 32 MiB".into(),
                ));
            }
            display
        } else {
            None
        };
        return Ok(ValidationLabels {
            primary,
            designations,
        });
    }
    let projected:Option<ProjectedLabels>=sqlx::query_as("SELECT CASE WHEN octet_length(COALESCE(display,''))<=$3 THEN display END,CASE WHEN designations IS NOT NULL AND octet_length(designations::text)+octet_length(COALESCE(display,''))<=$3 THEN designations END,COALESCE(octet_length(designations::text),0)::bigint+octet_length(COALESCE(display,'')),designations IS NULL FROM concepts WHERE code_system_id=$1 AND code=$2").bind(id).bind(code).bind(metadata.remaining).fetch_optional(&mut **tx).await?;
    let mut primary = None;
    if let Some((display, designations, bytes, legacy)) = projected {
        if bytes > metadata.remaining {
            return Err(AppError::BadRequest(
                "Validation label metadata exceeds 32 MiB".into(),
            ));
        }
        primary = display;
        if !legacy {
            metadata.remaining -= bytes;
            let mut result = designations
                .ok_or_else(|| {
                    AppError::BadRequest("Validation label metadata exceeds 32 MiB".into())
                })?
                .0
                .as_array()
                .cloned()
                .ok_or_else(|| {
                    AppError::BadRequest("Concept designations must be an array".into())
                })?;
            if result.len() > 256 {
                return Err(AppError::BadRequest(
                    "Concept designations exceed 256 entries".into(),
                ));
            }
            super::designations::extend(&mut result, supplemental)?;
            return Ok(ValidationLabels {
                primary,
                designations: result,
            });
        }
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
    result.and_then(|r| {
        let mut result = r.0;
        super::designations::extend(&mut result, supplemental)?;
        Ok(ValidationLabels {
            primary,
            designations: result,
        })
    })
}
fn localized_answer(
    display: Option<&str>,
    version: Option<&str>,
    expected: Option<&str>,
    abstract_code: bool,
    allow_abstract: bool,
    labels: LabelView<'_>,
    language: Option<&str>,
) -> ValidationAnswer {
    localized_answer_case(
        display,
        version,
        expected,
        abstract_code,
        allow_abstract,
        labels,
        language,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn localized_answer_case(
    display: Option<&str>,
    version: Option<&str>,
    expected: Option<&str>,
    abstract_code: bool,
    allow_abstract: bool,
    labels: LabelView<'_>,
    language: Option<&str>,
    case_insensitive: bool,
) -> ValidationAnswer {
    let equal = |left: &str, right: &str| {
        left == right || case_insensitive && left.to_lowercase() == right.to_lowercase()
    };
    let designations = labels.designations;
    let selected_language = language
        .and_then(|language| super::expansion::display_language_range(designations, language));
    let preferred = language.and_then(|language| {
        if case_insensitive {
            super::designations::rxnorm_display(display.or(labels.primary), designations, language)
        } else {
            super::expansion::preferred(designations, language)
        }
    });
    let mut answer = ValidationAnswer::member(
        preferred.or(display),
        version,
        expected,
        abstract_code,
        allow_abstract,
    );
    if answer.failure == Some(ValidationFailure::Display)
        && expected.is_some_and(|expected| {
            preferred
                .or(display)
                .is_some_and(|actual| equal(actual, expected))
        })
    {
        answer.valid = true;
        answer.message = None;
        answer.failure = None;
    }
    let matching_designation = expected.is_some_and(|expected| {
        if language.is_none() && labels.primary.is_some_and(|actual| equal(actual, expected)) {
            return true;
        }

        designations.iter().any(|d| {
            d["value"]
                .as_str()
                .is_some_and(|actual| equal(actual, expected))
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
        answer.failure = None;
    }
    answer
}

fn inline_answer(
    system: &PreparedSystem,
    candidates: &[ValidationCandidate<'_>],
    complete: bool,
    allow_abstract: bool,
    alternatives: bool,
    options: &ValidationOptions,
) -> Result<ValidationAnswer, AppError> {
    let mut unknown = false;
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
        let concept = system.concept(candidate.code);
        if let Some(concept) = concept {
            let abstract_code = system.boolean_property(concept, "abstract")
                || system.boolean_property(concept, "notSelectable");
            let designations = if options.language.is_some() || candidate.display.is_some() {
                concept.designation.as_slice()
            } else {
                &[]
            };
            if designations.len() > 256 {
                return Err(AppError::BadRequest(
                    "Concept designations exceed 256 entries".into(),
                ));
            }
            let mut answer = localized_answer_case(
                concept.display.as_deref(),
                system.canonical.version.as_deref(),
                candidate.display,
                abstract_code,
                allow_abstract,
                LabelView {
                    primary: concept.display.as_deref(),
                    designations,
                },
                options.language.as_deref(),
                system.canonical.url == term_squid_core::rxnorm::SYSTEM,
            )
            .display_policy(options.lenient_display);
            if options.coding_diagnostics {
                answer = answer.with_code(
                    &system.canonical.url,
                    system.canonical.version.as_deref(),
                    &concept.code,
                    system.case_sensitive,
                    candidate,
                );
            }
            if answer.valid {
                return Ok(answer);
            }
            negative = answer;
        } else if !complete {
            unknown = true;
        } else if options.coding_diagnostics {
            negative = ValidationAnswer::unknown_code(
                &system.canonical.url,
                system.canonical.version.as_deref(),
                candidate,
            );
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
