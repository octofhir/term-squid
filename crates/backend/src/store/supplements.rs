//! Supplements are loaded only during cold composition or explicitly rich reads.
use crate::error::AppError;
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use std::collections::BTreeMap;
use uuid::Uuid;

pub(super) async fn ids(
    tx: &mut Transaction<'_, Postgres>,
    bases: &[Uuid],
    date: Option<&str>,
) -> Result<Vec<Uuid>, AppError> {
    let rows:Vec<(Uuid,Uuid,String,Option<String>)>=sqlx::query_as("SELECT b.id,s.id,s.url,s.content->>'date' FROM code_systems b JOIN code_systems s ON s.content_mode='supplement' AND split_part(s.content->>'supplements','|',1)=b.url AND (split_part(s.content->>'supplements','|',2)='' OR split_part(s.content->>'supplements','|',2)=b.version) AND s.status<>'retired' WHERE b.id=ANY($1::uuid[]) ORDER BY b.id,s.url,s.updated_at DESC,s.id LIMIT 4097").bind(bases).fetch_all(&mut **tx).await?;
    if rows.len() > 4096 {
        return Err(AppError::BadRequest(
            "Supplement metadata inventory exceeds 4096 versions".into(),
        ));
    }
    let cutoff = date.map(super::options::date).transpose()?;
    let mut selected = BTreeMap::new();
    let mut dates = BTreeMap::new();
    for (base, id, url, published) in rows {
        let key = (base, url);
        let mut selected_date = None;
        if let Some(cutoff) = cutoff {
            let published = published.as_deref().ok_or_else(|| {
                AppError::BadRequest("Historical supplement requires its publication date".into())
            })?;
            let published = super::options::date(published)?;
            if published > cutoff {
                continue;
            }
            selected_date = Some(published);
        }
        if !selected.contains_key(&key)
            || selected_date
                .is_some_and(|date| dates.get(&key).is_some_and(|previous| date > *previous))
        {
            if let Some(date) = selected_date {
                dates.insert(key.clone(), date);
            }
            selected.insert(key, id);
        }
    }
    let mut per_base = BTreeMap::<Uuid, usize>::new();
    for (base, _) in selected.keys() {
        *per_base.entry(*base).or_default() += 1;
    }
    if per_base.values().any(|n| *n > 32) || selected.len() > 128 {
        return Err(AppError::BadRequest(
            "Supplement dependency limit exceeded".into(),
        ));
    }
    Ok(selected.into_values().collect())
}
pub(super) async fn designations(
    tx: &mut Transaction<'_, Postgres>,
    base: Uuid,
    code: &str,
    date: Option<&str>,
) -> Result<Vec<Value>, AppError> {
    let selected = ids(tx, &[base], date).await?;
    if selected.is_empty() {
        return Ok(Vec::new());
    }
    let rows:Vec<sqlx::types::Json<Value>>=sqlx::query_scalar("SELECT d FROM code_systems b JOIN concepts c ON c.code_system_id=ANY($2::uuid[]) AND (c.code=$3 OR NOT b.case_sensitive AND lower(c.code)=lower($3)) CROSS JOIN LATERAL jsonb_array_elements(COALESCE(c.designations,'[]')) d WHERE b.id=$1 ORDER BY c.code_system_id LIMIT 257").bind(base).bind(selected).bind(code).fetch_all(&mut **tx).await?;
    if rows.len() > 256 {
        return Err(AppError::BadRequest(
            "Supplement designations exceed 256 per concept".into(),
        ));
    }
    Ok(rows.into_iter().map(|r| r.0).collect())
}
pub(super) async fn load(
    tx: &mut Transaction<'_, Postgres>,
    url: &str,
    version: Option<&str>,
    date: Option<&str>,
    budget: &mut i64,
) -> Result<Vec<Value>, AppError> {
    let rows:Vec<(String,Option<sqlx::types::Json<Value>>,i64)>=sqlx::query_as("SELECT url,CASE WHEN octet_length(content::text)<=$3 THEN content END,octet_length(content::text)::bigint FROM code_systems WHERE content_mode='supplement' AND split_part(content->>'supplements','|',1)=$1 AND (split_part(content->>'supplements','|',2)='' OR split_part(content->>'supplements','|',2)=$2) AND status<>'retired' ORDER BY url,updated_at DESC,id LIMIT 129").bind(url).bind(version).bind(*budget).fetch_all(&mut **tx).await?;
    if rows.len() > 128 {
        return Err(AppError::BadRequest(
            "Supplement version inventory exceeds 128 entries".into(),
        ));
    }
    let cutoff = date.map(super::options::date).transpose()?;
    let mut selected = BTreeMap::new();
    let mut dates = BTreeMap::new();
    for (url, resource, bytes) in rows {
        *budget -= bytes;
        if *budget < 0 {
            return Err(AppError::BadRequest(
                "Supplement source byte limit exceeded".into(),
            ));
        }
        let resource = resource
            .ok_or_else(|| AppError::BadRequest("Supplement source byte limit exceeded".into()))?
            .0;
        let published = resource["date"]
            .as_str()
            .map(super::options::date)
            .transpose()?;
        if let Some(cutoff) = cutoff {
            let published = published.ok_or_else(|| {
                AppError::BadRequest("Historical supplement requires its publication date".into())
            })?;
            if published > cutoff {
                continue;
            }
        }
        if !selected.contains_key(&url)
            || cutoff.is_some()
                && published
                    .is_some_and(|date| dates.get(&url).is_some_and(|previous| date > *previous))
        {
            if let Some(date) = published {
                dates.insert(url.clone(), date);
            }
            selected.insert(url, resource);
        }
    }
    if selected.len() > 32 {
        return Err(AppError::BadRequest(
            "At most 32 supplements may enrich one CodeSystem".into(),
        ));
    }
    Ok(selected.into_values().collect())
}

pub(super) fn for_code(
    resource: &Value,
    code: &str,
    case_sensitive: bool,
) -> Result<Value, AppError> {
    let mut result = resource.clone();
    let mut pending: Vec<_> = resource["concept"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    let mut selected = Vec::new();
    let mut visited = 0;
    while let Some(concept) = pending.pop() {
        visited += 1;
        if visited > 100000 {
            return Err(AppError::BadRequest(
                "Supplement concept limit exceeded".into(),
            ));
        }
        if concept["code"].as_str().is_some_and(|c| {
            c == code || !case_sensitive && c.to_lowercase() == code.to_lowercase()
        }) {
            let mut concept = concept.clone();
            concept
                .as_object_mut()
                .expect("Supplement concept")
                .remove("concept");
            selected.push(concept);
        }
        pending.extend(concept["concept"].as_array().into_iter().flatten());
    }
    result["concept"] = selected.into();
    Ok(result)
}

pub(super) fn enrich(
    base: &Value,
    supplements: &[Value],
    code: &str,
    case_sensitive: bool,
) -> Result<(Vec<Value>, Vec<Value>), AppError> {
    let targeted = supplements
        .iter()
        .map(|r| for_code(r, code, case_sensitive))
        .collect::<Result<Vec<_>, _>>()?;
    let merged = term_squid_core::supplements::apply(base, &targeted)?;
    let (designations, _) = super::lookup::details(&merged, code, false)?;
    let properties = targeted
        .iter()
        .flat_map(|s| s["concept"].as_array().into_iter().flatten())
        .flat_map(|c| c["property"].as_array().into_iter().flatten())
        .cloned()
        .collect();
    Ok((designations, properties))
}
