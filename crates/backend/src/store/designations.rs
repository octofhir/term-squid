//! Rich label reads enforce aggregate bytes in PostgreSQL before JSON decoding.
use crate::error::AppError;
use serde_json::Value;
use sqlx::{types::Json, Postgres, Transaction};
use uuid::Uuid;

pub(super) type LabelRow = (Option<Json<Value>>, i64);

pub(super) fn decode(rows: Vec<LabelRow>, budget: &mut i64) -> Result<Vec<Value>, AppError> {
    if rows.len() > 256 {
        return Err(AppError::BadRequest(
            "Concept designations exceed 256 entries".into(),
        ));
    }
    let bytes = rows.iter().try_fold(0_i64, |total, (_, bytes)| {
        (*bytes >= 0).then(|| total.checked_add(*bytes)).flatten()
    });
    let bytes = bytes
        .filter(|bytes| *bytes <= *budget)
        .ok_or_else(|| AppError::BadRequest("Designation metadata byte limit exceeded".into()))?;
    let values = rows
        .into_iter()
        .map(|(value, _)| {
            value.map(|value| value.0).ok_or_else(|| {
                AppError::BadRequest("Designation metadata byte limit exceeded".into())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    *budget -= bytes;
    Ok(values)
}

pub(super) async fn snomed(
    tx: &mut Transaction<'_, Postgres>,
    base: Uuid,
    code: &str,
    prefer_dialect: bool,
    budget: &mut i64,
) -> Result<Vec<Value>, AppError> {
    let rows: Vec<LabelRow> = sqlx::query_as("WITH labels AS MATERIALIZED (SELECT d.id,CASE WHEN $3 THEN EXISTS(SELECT 1 FROM snomed_language_members l JOIN snomed_releases r ON r.code_system_id=l.code_system_id AND r.dialect=l.refset_id WHERE l.code_system_id=d.code_system_id AND l.description_id=d.id AND l.active AND l.acceptability_id='900000000000548007') ELSE false END AS preferred,jsonb_build_object('language',d.language_code,'use',jsonb_build_object('system','http://snomed.info/sct','code',d.type_id),'value',d.term) AS label FROM snomed_descriptions d WHERE d.code_system_id=$1 AND d.concept_code=$2 AND d.active ORDER BY preferred DESC,d.id LIMIT 257), sized AS (SELECT id,preferred,label,octet_length(label::text)::bigint AS bytes FROM labels) SELECT CASE WHEN sum(bytes) OVER ()<=$4 THEN label END,bytes FROM sized ORDER BY preferred DESC,id")
        .bind(base).bind(code).bind(prefer_dialect).bind(*budget).fetch_all(&mut **tx).await?;
    decode(rows, budget)
}

/// Base and supplement labels share one per-concept count, including native sources.
pub(super) fn extend(target: &mut Vec<Value>, additional: Vec<Value>) -> Result<(), AppError> {
    if target.len().saturating_add(additional.len()) > 256 {
        return Err(AppError::BadRequest(
            "Concept designations exceed 256 entries".into(),
        ));
    }
    target.extend(additional);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    #[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
    async fn rich_projection_labels_share_bytes_and_keep_order_and_count_bounds() {
        let (admin, pool, schema) = crate::tests::hierarchy_test_database().await;
        let source = json!({"resourceType":"CodeSystem","url":"urn:label-budget","version":"1","status":"active","content":"complete","concept":[{"code":"A","display":"Alpha"}]});
        let inspection = term_squid_ingestion::inspect(&source).unwrap();
        let mut tx = pool.begin().await.unwrap();
        let base = term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
            .await
            .unwrap()
            .id;
        let source = json!({"resourceType":"CodeSystem","url":"urn:label-budget:supplement","version":"1","status":"active","content":"supplement","supplements":"urn:label-budget|1","concept":[{"code":"A","designation":[{"language":"en","value":"Alias one"},{"language":"ru","value":"Второй синоним"}]}]});
        let inspection = term_squid_ingestion::inspect(&source).unwrap();
        term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
            .await
            .unwrap();
        for (id, label) in [("1", "Native first"), ("2", "Native second")] {
            sqlx::query("INSERT INTO snomed_descriptions(code_system_id,id,effective_time,active,module_id,concept_code,language_code,type_id,term,case_significance_id) VALUES($1,$2,'2026-01-01',true,'module','A','en','900000000000013009',$3,'case')")
                .bind(base).bind(id).bind(label).execute(&mut *tx).await.unwrap();
        }
        tx.commit().await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await
            .unwrap();
        for native in [true, false] {
            let mut tiny = 1;
            let error = if native {
                snomed(&mut tx, base, "A", false, &mut tiny).await
            } else {
                super::super::supplements::designations(&mut tx, base, "A", None, &mut tiny).await
            };
            assert!(error.unwrap_err().to_string().contains("byte limit"));
            assert_eq!(
                tiny, 1,
                "A rejected read must not underflow its byte budget"
            );
            let mut remaining = 1024 * 1024;
            let labels = if native {
                snomed(&mut tx, base, "A", false, &mut remaining)
                    .await
                    .unwrap()
            } else {
                super::super::supplements::designations(&mut tx, base, "A", None, &mut remaining)
                    .await
                    .unwrap()
            };
            assert_eq!(labels.len(), 2);
            assert_eq!(
                labels[0]["value"],
                if native { "Native first" } else { "Alias one" }
            );
            assert_eq!(
                labels[1]["value"],
                if native {
                    "Native second"
                } else {
                    "Второй синоним"
                }
            );
            let mut exact = 1024 * 1024 - remaining;
            let repeated = if native {
                snomed(&mut tx, base, "A", true, &mut exact).await.unwrap()
            } else {
                super::super::supplements::designations(&mut tx, base, "A", None, &mut exact)
                    .await
                    .unwrap()
            };
            assert_eq!(labels, repeated);
            assert_eq!(exact, 0);
            assert!(if native {
                snomed(&mut tx, base, "A", false, &mut exact).await
            } else {
                super::super::supplements::designations(&mut tx, base, "A", None, &mut exact).await
            }
            .is_err());
            assert_eq!(exact, 0);
        }
        let mut combined = vec![json!({"value":"base"}); 256];
        let original = combined.clone();
        assert!(extend(&mut combined, vec![json!({"value":"supplement"})]).is_err());
        assert_eq!(
            combined, original,
            "An oversized merged concept must not be truncated or partially enriched"
        );
        tx.rollback().await.unwrap();
        pool.close().await;
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
            .execute(&admin)
            .await
            .unwrap();
        admin.close().await;
    }
}
