//! Bounded public projections for the browser; independent of admin sessions.
use crate::error::AppError;
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

pub fn router(pool: PgPool) -> Router {
    Router::new()
        .route("/api/catalog/{kind}", get(list))
        .route("/api/catalog/{kind}/{id}", get(detail))
        .route("/api/catalog/CodeSystem/{id}/concepts", get(concepts))
        .with_state(pool)
}
fn table(kind: &str) -> Result<&'static str, AppError> {
    match kind {
        "CodeSystem" => Ok("code_systems"),
        "ValueSet" => Ok("value_sets"),
        "ConceptMap" => Ok("concept_maps"),
        _ => Err(AppError::NotFound("Unknown catalog".into())),
    }
}
#[derive(Default, Deserialize)]
struct Page {
    #[serde(default)]
    offset: i64,
    q: Option<String>,
    url: Option<String>,
    version: Option<String>,
}
fn validate(p: &Page) -> Result<(), AppError> {
    if p.offset < 0 || p.offset > 1_000_000 || p.q.as_ref().is_some_and(|s| s.len() > 512) {
        return Err(AppError::BadRequest("Invalid catalog page or query".into()));
    }
    Ok(())
}
const META: &str = "jsonb_build_object('id',id,'resourceType',$4::text,'url',url,'version',version,'name',name,'title',title,'status',status,'description',content->'description','publisher',content->'publisher','content',content->'content')";
async fn list(
    State(pool): State<PgPool>,
    Path(kind): Path<String>,
    Query(page): Query<Page>,
) -> Result<Json<Value>, AppError> {
    validate(&page)?;
    let table = table(&kind)?;
    let q = page
        .q
        .unwrap_or_default()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let sql = format!("WITH matched AS MATERIALIZED (SELECT id,url,version,name,title,status,updated_at FROM {table} WHERE ($1::text IS NULL OR url=$1) AND ($2::text IS NULL OR version=$2) AND (COALESCE(name,'') ILIKE $3 OR COALESCE(title,'') ILIKE $3 OR url ILIKE $3)), page_ids AS (SELECT id FROM matched ORDER BY COALESCE(title,name,url),url,version NULLS FIRST,id LIMIT 25 OFFSET $5), page AS (SELECT {META} AS resource FROM page_ids JOIN {table} USING(id) ORDER BY COALESCE(title,name,url),url,version NULLS FIRST,id) SELECT jsonb_build_object('total',(SELECT count(*) FROM matched),'items',COALESCE((SELECT jsonb_agg(resource) FROM page),'[]'::jsonb))");
    let result: Value = sqlx::query_scalar(&sql)
        .bind(page.url)
        .bind(page.version)
        .bind(format!("%{q}%"))
        .bind(kind)
        .bind(page.offset)
        .fetch_one(&pool)
        .await?;
    Ok(Json(result))
}
async fn detail(
    State(pool): State<PgPool>,
    Path((kind, id)): Path<(String, Uuid)>,
) -> Result<Json<Value>, AppError> {
    let table = table(&kind)?;
    let sql = format!("SELECT (content-'concept'-'expansion'-'group') || jsonb_build_object('id',id,'resourceType',$2::text) FROM {table} WHERE id=$1");
    let mut result: Value = sqlx::query_scalar(&sql)
        .bind(id)
        .bind(&kind)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Resource not found".into()))?;
    if kind == "CodeSystem" {
        let provider: Option<String> =
            sqlx::query_scalar("SELECT native_provider FROM code_systems WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await?;
        result["nativeProvider"] = json!(provider);
    }
    Ok(Json(result))
}
async fn concepts(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Json<Value>, AppError> {
    validate(&page)?;
    let q = page
        .q
        .unwrap_or_default()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let result: Value=sqlx::query_scalar("WITH matched AS NOT MATERIALIZED (SELECT code,display,definition FROM concepts WHERE code_system_id=$1 AND (code ILIKE $2 OR COALESCE(display,'') ILIKE $2)), page AS (SELECT * FROM matched ORDER BY code COLLATE \"C\" LIMIT 25 OFFSET $3) SELECT jsonb_build_object('total',(SELECT count(*) FROM matched),'items',COALESCE((SELECT jsonb_agg(to_jsonb(page)) FROM page),'[]'::jsonb))")
        .bind(id).bind(format!("%{q}%")).bind(page.offset).fetch_one(&pool).await?;
    Ok(Json(result))
}
