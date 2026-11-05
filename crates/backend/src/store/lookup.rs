//! Optional rich lookup uses bounded source metadata and the existing concept projections.
use super::options;
use crate::{error::AppError, models::ResolvedConcept};
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::sync::OwnedSemaphorePermit;
use uuid::Uuid;

pub struct LookupRequest<'a> {
    pub url: &'a str,
    pub version: Option<&'a str>,
    pub id: Option<Uuid>,
    pub code: &'a str,
    pub date: Option<&'a str>,
    pub language: Option<&'a str>,
    pub relationships: bool,
    pub metadata: bool,
}
pub struct LookupAnswer {
    pub concept: ResolvedConcept,
    pub designations: Vec<Value>,
    pub properties: Vec<Value>,
}
pub(super) async fn run(
    pool: &PgPool,
    request: LookupRequest<'_>,
    permit: OwnedSemaphorePermit,
) -> Result<LookupAnswer, AppError> {
    if let Some(entry) = super::transient::select(
        super::transient::Kind::System,
        request.url,
        request.version,
        request.id,
        request.date,
    )? {
        return entry.lookup(&request);
    }
    let mut permit = Some(permit);
    let cutoff = request.date.map(options::date).transpose()?;
    if let Some(language) = request.language {
        options::language(language)?;
    }
    if request.id.is_none()
        && request.url == term_squid_core::ucum::SYSTEM
        && request
            .version
            .is_none_or(|v| v == term_squid_core::ucum::VERSION)
    {
        let value = term_squid_core::ucum::UcumProvider::lookup(request.code, request.version)?;
        let mut answer = LookupAnswer {
            concept: ResolvedConcept {
                url: request.url.into(),
                native_provider: None,
                name: Some("UCUM".into()),
                version: Some(term_squid_core::ucum::VERSION.into()),
                content_mode: Some("complete".into()),
                code: value.as_ref().map(|v| v.code.clone()),
                display: value.map(|v| v.display),
                definition: None,
                properties: None,
            },
            designations: Vec::new(),
            properties: Vec::new(),
        };
        if request.metadata && answer.concept.code.is_some() {
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
                request.url,
                Some(term_squid_core::ucum::VERSION),
                request.date,
                &mut bytes,
            )
            .await?;
            tx.commit().await?;
            if !overlays.is_empty() {
                let code = answer.concept.code.clone().expect("Known UCUM code");
                let base = serde_json::json!({"url":request.url,"version":term_squid_core::ucum::VERSION,"caseSensitive":true,"concept":[{"code":code,"display":answer.concept.display}]});
                let work = permit.take().expect("Rich lookup work permit");
                let (result, _permit) = tokio::task::spawn_blocking(move || {
                    (
                        super::supplements::enrich(&base, &overlays, &code, true),
                        work,
                    )
                })
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
                let enriched = result?;
                answer.designations = enriched.0;
                answer.properties = enriched.1;
                if let Some(display) = request
                    .language
                    .and_then(|l| super::expansion::preferred(&answer.designations, l))
                {
                    answer.concept.display = Some(display.into());
                }
            }
        }
        return Ok(answer);
    }

    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout = '10s'")
        .execute(&mut *tx)
        .await?;
    let root = term_squid_core::Canonical::parse(request.url, request.version)?;
    let rows:Vec<(Uuid,Option<String>,Option<String>,bool)>=sqlx::query_as("SELECT s.id,s.version,COALESCE(s.content->>'date',r.release_date::text),s.case_sensitive FROM code_systems s LEFT JOIN snomed_releases r ON r.code_system_id=s.id WHERE ($3::uuid IS NOT NULL AND s.id=$3) OR ($3 IS NULL AND s.url=$1 AND ($2::text IS NULL OR s.version=$2)) ORDER BY s.updated_at DESC,s.id LIMIT 129")
        .bind(&root.url).bind(&root.version).bind(request.id).fetch_all(&mut *tx).await?;
    if rows.len() > 128 {
        return Err(AppError::BadRequest(
            "Temporal lookup exceeds 128 versions; select an explicit version".into(),
        ));
    }
    let selected = if let Some(cutoff) = cutoff {
        rows.iter()
            .filter_map(|row| {
                row.2
                    .as_deref()
                    .and_then(|d| options::date(d).ok())
                    .filter(|d| *d <= cutoff)
                    .map(|date| (date, row))
            })
            .max_by_key(|(date, _)| *date)
            .map(|(_, row)| row)
            .or_else(|| {
                ((request.id.is_some() || root.version.is_some())
                    && rows.first().is_some_and(|r| r.2.is_none()))
                .then(|| &rows[0])
            })
    } else {
        rows.first()
    }
    .ok_or_else(|| {
        AppError::NotFound("CodeSystem version for the requested date is unavailable".into())
    })?;
    let selected_id = selected.0;
    if request.id.is_some()
        && (root.url != "urn:term-squid:instance" && rows.is_empty()
            || request
                .version
                .is_some_and(|v| Some(v) != selected.1.as_deref()))
    {
        return Err(AppError::BadRequest(
            "Input version conflicts with CodeSystem instance".into(),
        ));
    }
    let mut concept: ResolvedConcept = sqlx::query_as(include_str!("concept_resolution.sql"))
        .bind(&root.url)
        .bind(&root.version)
        .bind(request.code)
        .bind(selected_id)
        .fetch_one(&mut *tx)
        .await?;
    if concept.native_provider.as_deref() == Some("snomed-rf2") {
        term_squid_core::snomed::reject_expression(request.code)?;
    }
    if concept.native_provider.as_deref() == Some("loinc-csv") {
        term_squid_core::loinc::require_primary_code(request.code)?;
    }
    let mut designations = Vec::new();
    let mut properties = Vec::new();
    if let Some(code) = &concept.code {
        if concept.native_provider.as_deref() == Some("snomed-rf2") {
            let rows:Vec<(String,String,String,bool)>=sqlx::query_as("SELECT d.language_code,d.type_id,d.term,EXISTS (SELECT 1 FROM snomed_language_members l JOIN snomed_releases r ON r.code_system_id=l.code_system_id AND r.dialect=l.refset_id WHERE l.code_system_id=d.code_system_id AND l.description_id=d.id AND l.active AND l.acceptability_id='900000000000548007') preferred FROM snomed_descriptions d WHERE d.code_system_id=$1 AND d.concept_code=$2 AND d.active ORDER BY preferred DESC,d.id LIMIT 257")
                .bind(selected_id).bind(code).fetch_all(&mut *tx).await?;
            if rows.len() > 256 {
                return Err(AppError::BadRequest(
                    "Concept designations exceed 256 entries".into(),
                ));
            }
            for (language, use_code, value, _) in rows {
                designations.push(json!({"language":language,"use":{"system":"http://snomed.info/sct","code":use_code},"value":value}));
            }
            if request.relationships {
                let rows:Vec<(String,String)>=sqlx::query_as("SELECT CASE WHEN source_code=$2 THEN 'parent' ELSE 'child' END,CASE WHEN source_code=$2 THEN destination_code ELSE source_code END FROM snomed_relationships WHERE code_system_id=$1 AND active AND type_id='116680003' AND characteristic_type_id='900000000000011006' AND (source_code=$2 OR destination_code=$2) ORDER BY 1,2 LIMIT 10001")
                    .bind(selected_id).bind(code).fetch_all(&mut *tx).await?;
                if rows.len() > 10000 {
                    return Err(AppError::BadRequest(
                        "Concept relationship result exceeds 10000 entries".into(),
                    ));
                }
                for (kind, code) in rows {
                    properties.push(json!({"code":kind,"valueCode":code}));
                }
            }
        } else if concept.native_provider.is_none() {
            let resource:Option<sqlx::types::Json<Value>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(content::text)<=33554432 THEN content END FROM code_systems WHERE id=$1").bind(selected_id).fetch_one(&mut *tx).await?;
            let resource = resource
                .ok_or_else(|| AppError::BadRequest("Rich lookup source exceeds 32 MiB".into()))?
                .0;
            let code = code.clone();
            let relationships = request.relationships;
            let work = permit.take().expect("Rich lookup work permit");
            let (result, returned) = tokio::task::spawn_blocking(move || {
                (details(&resource, &code, relationships), work)
            })
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
            permit = Some(returned);
            let details = result?;
            designations = details.0;
            properties = details.1;
        }
    }
    if let Some(code) = concept.code.as_deref() {
        let mut bytes = 32 * 1024 * 1024;
        let supplements = super::supplements::load(
            &mut tx,
            &concept.url,
            concept.version.as_deref(),
            request.date,
            &mut bytes,
        )
        .await?;
        if !supplements.is_empty() {
            let base = serde_json::json!({"url":concept.url,"version":concept.version,"caseSensitive":selected.3,"concept":[{"code":code,"display":concept.display,"property":concept.properties.as_ref().map(|p|p.0.clone()).unwrap_or_else(||serde_json::json!([])),"designation":designations}]});
            let code = code.to_owned();
            let case_sensitive = selected.3;
            let work = permit.take().expect("Rich lookup work permit");
            let (result, returned) = tokio::task::spawn_blocking(move || {
                let result = super::supplements::enrich(&base, &supplements, &code, case_sensitive);
                (result, work)
            })
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
            permit = Some(returned);
            let enriched = result?;
            designations = enriched.0;
            properties.extend(enriched.1);
        }
    }
    if let Some(language) = request.language {
        if let Some(value) = super::expansion::preferred(&designations, language) {
            concept.display = Some(value.into());
        }
    }
    drop(permit);
    tx.commit().await?;
    Ok(LookupAnswer {
        concept,
        designations,
        properties,
    })
}
pub(super) fn details(
    resource: &Value,
    code: &str,
    relationships: bool,
) -> Result<(Vec<Value>, Vec<Value>), AppError> {
    let mut pending: Vec<_> = resource["concept"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| (c, None, 0))
        .collect();
    let mut count = 0;
    while let Some((concept, parent, depth)) = pending.pop() {
        count += 1;
        if count > 100000 || depth > 32 {
            return Err(AppError::BadRequest(
                "Rich lookup concept traversal limit exceeded".into(),
            ));
        }
        let children = concept["concept"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        if concept["code"] == code {
            let designations = concept["designation"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if designations.len() > 256 {
                return Err(AppError::BadRequest(
                    "Concept designations exceed 256 entries".into(),
                ));
            }
            let mut properties = Vec::new();
            if relationships && resource["hierarchyMeaning"] == "is-a" {
                if let Some(parent) = parent {
                    properties.push(json!({"code":"parent","valueCode":parent}));
                }
                for child in children {
                    if let Some(code) = child["code"].as_str() {
                        properties.push(json!({"code":"child","valueCode":code}));
                    }
                }
            }
            return Ok((designations, properties));
        }
        pending.extend(
            children
                .iter()
                .map(|c| (c, concept["code"].as_str(), depth + 1)),
        );
    }
    Ok((Vec::new(), Vec::new()))
}
