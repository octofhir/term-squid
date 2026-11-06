//! The permissive CLI package reader must not bypass R4 modifier validation.
use super::*;
use flate2::{write::GzEncoder, Compression};
use serde_json::{json, Value};
use std::{fs::File, path::PathBuf};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn package(map: Value) -> Fixture {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.benchmarks/spec-speed-20261106/translation-packages");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("{}.tgz", Uuid::new_v4()));
    let mut archive = tar::Builder::new(GzEncoder::new(
        File::create(&path).unwrap(),
        Compression::fast(),
    ));
    for (name, content) in [
        (
            "package/package.json",
            json!({"name":"org.example.mapping-safety","version":"1","fhirVersions":["4.0.1"]}),
        ),
        (
            "package/CodeSystem.json",
            json!({"resourceType":"CodeSystem","url":"urn:a-safety:codes","status":"active","content":"complete","concept":[{"code":"A"}]}),
        ),
        ("package/ConceptMap.json", map),
    ] {
        let bytes = serde_json::to_vec(&content).unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        archive
            .append_data(&mut header, name, bytes.as_slice())
            .unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap();
    Fixture(path)
}
#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn cli_package_late_malformed_mapping_rolls_back_resources_and_generation() {
    let url = std::env::var("TERM_SQUID_TEST_DATABASE_URL").unwrap();
    let admin = PgPool::connect(&url).await.unwrap();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let search = format!("SET search_path TO {schema}, public");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(move |c, _| {
            let search = search.clone();
            Box::pin(async move {
                sqlx::query(sqlx::AssertSqlSafe(search)).execute(c).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("../backend/migrations")
        .run(&pool)
        .await
        .unwrap();
    let initial: i64 =
        sqlx::query_scalar("SELECT generation FROM terminology_generation WHERE singleton")
            .fetch_one(&pool)
            .await
            .unwrap();
    for modifier in [
        None,
        Some(Value::Null),
        Some(json!(true)),
        Some(json!("")),
        Some(json!("invalid")),
    ] {
        let mut map = json!({"resourceType":"ConceptMap","url":"urn:z-safety:map","status":"active","group":[{"source":"urn:s","target":"urn:t","element":[{"code":"a","target":[{"code":"b"}]}]}]});
        if let Some(modifier) = modifier {
            map["group"][0]["element"][0]["target"][0]["equivalence"] = modifier;
        }
        let fixture = package(map);
        let parsed = term_squid_ingestion::package::read_package(&fixture.0, false).unwrap();
        assert_eq!(parsed.resources.len(), 2);
        assert!(run(
            pool.clone(),
            fixture.0.to_string_lossy().into_owned(),
            None,
            "https://unused.invalid".into(),
            false,
            true
        )
        .await
        .is_err());
        let inventory:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM code_systems)+(SELECT count(*) FROM concepts)+(SELECT count(*) FROM concept_maps)").fetch_one(&pool).await.unwrap();
        assert_eq!(inventory, 0);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT generation FROM terminology_generation WHERE singleton"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            initial
        );
    }
    let valid = json!({"resourceType":"ConceptMap","url":"urn:z-safety:map","status":"active","group":[{"source":"urn:s","target":"urn:t","element":[{"code":"a","target":[{"code":"b","equivalence":"equivalent"}]}]}]});
    let fixture = package(valid);
    run(
        pool.clone(),
        fixture.0.to_string_lossy().into_owned(),
        None,
        "https://unused.invalid".into(),
        false,
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM concept_maps")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
