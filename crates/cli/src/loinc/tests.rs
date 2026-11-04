//! Original generated primary-table fixtures; no licensed content or benchmark pool rows.
use super::*;
use serde_json::json;

const COLUMNS: &[&str] = &[
    "LOINC_NUM",
    "COMPONENT",
    "PROPERTY",
    "TIME_ASPCT",
    "SYSTEM",
    "SCALE_TYP",
    "METHOD_TYP",
    "CLASS",
    "STATUS",
    "CLASSTYPE",
    "ORDER_OBS",
    "LONG_COMMON_NAME",
    "SHORTNAME",
    "DefinitionDescription",
    "CONSUMER_NAME",
    "EXTERNAL_COPYRIGHT_NOTICE",
    "FutureColumn",
];

struct Fixture {
    root: PathBuf,
    rows: Vec<Vec<String>>,
    manifest: Value,
}
impl Fixture {
    fn new(version: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.benchmarks/loinc-provider-20261103/test-fixtures")
            .join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&root).unwrap();
        let rows = ["ACTIVE", "TRIAL", "DISCOURAGED", "DEPRECATED"]
            .iter()
            .enumerate()
            .map(|(index, status)| {
                [
                    format!("{}-1", 1000 + index),
                    "Generated analyte".into(),
                    "MCnc".into(),
                    "Pt".into(),
                    "Ser/Plas".into(),
                    "Qn".into(),
                    "".into(),
                    "GENERATED".into(),
                    status.to_string(),
                    (index + 1).to_string(),
                    ["Order", "Observation", "Both", "Subset"][index].into(),
                    format!("Generated long name {index}, \"μ\" \\"),
                    format!("Generated short {index}"),
                    "Generated definition line 1\r\nline 2, with \"quotes\" and \\".into(),
                    "Generated consumer".into(),
                    "Generated external notice".into(),
                    "Preserve this column".into(),
                ]
                .to_vec()
            })
            .collect();
        let mut fixture = Self {
            root,
            rows,
            manifest: json!({"format":"loinc-csv-v1","version":version,"completeTable":true,"expectedConcepts":4,"source":{"path":"Loinc.csv","sha256":""}}),
        };
        fixture.save_source();
        fixture
    }
    fn path(&self) -> PathBuf {
        self.root.join("manifest.json")
    }
    fn source(&self) -> PathBuf {
        self.root.join("Loinc.csv")
    }
    fn save_manifest(&self) {
        std::fs::write(
            self.path(),
            serde_json::to_vec_pretty(&self.manifest).unwrap(),
        )
        .unwrap();
    }
    fn save_source(&mut self) {
        let mut writer = csv::WriterBuilder::new()
            .terminator(csv::Terminator::CRLF)
            .from_writer(Vec::new());
        writer.write_record(COLUMNS).unwrap();
        for row in &self.rows {
            writer.write_record(row).unwrap();
        }
        let content = writer.into_inner().unwrap();
        std::fs::write(self.source(), &content).unwrap();
        self.manifest["source"]["sha256"] = hex::encode(Sha256::digest(content)).into();
        self.save_manifest();
    }
    fn replace_source(&mut self, content: impl AsRef<[u8]>) {
        std::fs::write(self.source(), content.as_ref()).unwrap();
        self.manifest["source"]["sha256"] = hex::encode(Sha256::digest(content.as_ref())).into();
        self.save_manifest();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

async fn database() -> (PgPool, PgPool, String) {
    let url = std::env::var("TERM_SQUID_TEST_DATABASE_URL").unwrap();
    let admin = PgPool::connect(&url).await.unwrap();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let path = format!("SET search_path TO {schema},public");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .after_connect(move |connection, _| {
            let path = path.clone();
            Box::pin(async move {
                sqlx::query(sqlx::AssertSqlSafe(path.as_str()))
                    .execute(connection)
                    .await?;
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
    (admin, pool, schema)
}
async fn cleanup(admin: PgPool, pool: PgPool, schema: String) {
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
async fn generation(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT generation FROM terminology_generation WHERE singleton")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn loinc_manifest_rejects_unattested_or_ambiguous_input_before_database() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://unused:unused@127.0.0.1/unused")
        .unwrap();
    for variant in 0..8 {
        let mut fixture = Fixture::new("2.82");
        match variant {
            0 => fixture.manifest["completeTable"] = false.into(),
            1 => fixture.manifest["format"] = "other".into(),
            2 => fixture.manifest["version"] = "2.82|bad".into(),
            3 => fixture.manifest["source"]["sha256"] = "wrong".into(),
            4 => fixture.manifest["expectedConcepts"] = 0.into(),
            5 => fixture.manifest["expectedConcepts"] = 2_000_001.into(),
            6 => fixture.manifest["source"]["path"] = "LoincTableCore.csv".into(),
            _ => fixture.manifest["unknown"] = true.into(),
        }
        fixture.save_manifest();
        assert!(
            import_manifest(&pool, &fixture.path(), false)
                .await
                .is_err(),
            "variant {variant}"
        );
    }
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_import_preserves_raw_fields_lifecycle_types_versions_and_provenance() {
    let (admin, pool, schema) = database().await;
    let mut fixture = Fixture::new("2.82");
    let initial = generation(&pool).await;
    let dry = import_manifest(&pool, &fixture.path(), true).await.unwrap();
    assert!(dry.code_system_id.is_none());
    assert_eq!(dry.source.rows, 4);
    assert_eq!(generation(&pool).await, initial);
    let result = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    let id = result.code_system_id.unwrap();
    let metadata: sqlx::types::Json<Value> =
        sqlx::query_scalar("SELECT content FROM code_systems WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(metadata["content"], "not-present");
    assert_eq!(metadata["caseSensitive"], false);
    assert!(metadata.get("concept").is_none());
    assert!(metadata["copyright"]
        .as_str()
        .unwrap()
        .contains("http://loinc.org/terms-of-use"));
    assert!(metadata["property"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| p["code"] != "COMPONENT"));
    for index in 0..4 {
        let code = format!("{}-1", 1000 + index);
        let projected: (String, String, sqlx::types::Json<Value>) = sqlx::query_as("SELECT display,definition,properties FROM concepts WHERE code_system_id=$1 AND code=$2").bind(id).bind(&code).fetch_one(&pool).await.unwrap();
        assert_eq!(projected.0, fixture.rows[index][11]);
        assert_eq!(projected.1, fixture.rows[index][13]);
        let properties = projected.2.as_array().unwrap();
        assert_eq!(
            properties
                .iter()
                .find(|p| p["code"] == "source-table")
                .unwrap()["valueString"],
            "Loinc.csv"
        );
        assert_eq!(
            properties.iter().find(|p| p["code"] == "inactive").unwrap()["valueBoolean"],
            index == 3
        );
        assert_eq!(
            properties
                .iter()
                .find(|p| p["code"] == "CLASSTYPE")
                .unwrap()["valueString"],
            (index + 1).to_string()
        );
        assert!(properties.iter().all(|p| p["code"] != "raw-METHOD_TYP"));
        assert_eq!(
            properties
                .iter()
                .find(|p| p["code"] == "raw-COMPONENT")
                .unwrap()["valueString"],
            "Generated analyte"
        );
        let fields: sqlx::types::Json<Value> = sqlx::query_scalar(
            "SELECT fields FROM loinc_terms WHERE code_system_id=$1 AND code=$2",
        )
        .bind(id)
        .bind(&code)
        .fetch_one(&pool)
        .await
        .unwrap();
        for (column, value) in COLUMNS.iter().zip(&fixture.rows[index]) {
            assert_eq!(fields[*column], *value);
        }
    }
    let provenance: sqlx::types::Json<Value> =
        sqlx::query_scalar("SELECT provenance FROM loinc_releases WHERE code_system_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        provenance["source"]["sha256"],
        fixture.manifest["source"]["sha256"]
    );
    assert_eq!(
        provenance["source"]["bytes"],
        std::fs::metadata(fixture.source()).unwrap().len()
    );
    assert!(provenance["scope"].as_str().unwrap().contains("no Parts"));
    let current = generation(&pool).await;
    let again = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    assert!(again.unchanged);
    assert_eq!(again.code_system_id, Some(id));
    assert_eq!(generation(&pool).await, current);
    std::fs::create_dir(fixture.root.join("moved")).unwrap();
    std::fs::copy(fixture.source(), fixture.root.join("moved/Loinc.csv")).unwrap();
    fixture.manifest["source"]["path"] = "moved/Loinc.csv".into();
    fixture.save_manifest();
    assert_eq!(
        import_manifest(&pool, &fixture.path(), false)
            .await
            .unwrap()
            .fingerprint,
        result.fingerprint
    );
    fixture.manifest["source"]["path"] = "Loinc.csv".into();
    fixture.rows[0][11] = "Changed display".into();
    fixture.save_source();
    let error = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Conflicting immutable"));
    assert_eq!(generation(&pool).await, current);
    let second = Fixture::new("2.83");
    let next = import_manifest(&pool, &second.path(), false).await.unwrap();
    assert_ne!(next.code_system_id, Some(id));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM code_systems")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM closure_table")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert!(sqlx::raw_sql(include_str!(
        "../../../backend/migrations/20261103000003_loinc_csv.down.sql"
    ))
    .execute(&pool)
    .await
    .is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM loinc_releases")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_malformed_sources_and_publication_failure_publish_nothing() {
    let (admin, pool, schema) = database().await;
    let initial = generation(&pool).await;
    for variant in 0..15 {
        let mut fixture = Fixture::new("2.82");
        match variant {
            0 => {
                fixture.manifest["source"]["sha256"] = "0".repeat(64).into();
                fixture.save_manifest();
            }
            1 => {
                fixture.rows[0][0] = "LA123-1".into();
                fixture.save_source();
            }
            2 => {
                fixture.rows[0][8] = "UNKNOWN".into();
                fixture.save_source();
            }
            3 => {
                fixture.rows[0][9] = "5".into();
                fixture.save_source();
            }
            4 => {
                fixture.rows[0][10] = "Unknown".into();
                fixture.save_source();
            }
            5 => {
                fixture.rows[0][1] = "nul\0field".into();
                fixture.save_source();
            }
            6 => {
                fixture.rows[0][8] = "".into();
                fixture.save_source();
            }
            7 => {
                fixture.rows.push(fixture.rows[0].clone());
                fixture.manifest["expectedConcepts"] = 5.into();
                fixture.save_source();
            }
            8 => {
                fixture.manifest["expectedConcepts"] = 5.into();
                fixture.save_manifest();
            }
            9 => {
                fixture.replace_source(b"wrong,header\nvalue,value\n");
            }
            10 => {
                let mut bytes = std::fs::read(fixture.source()).unwrap();
                bytes.push(0xff);
                fixture.replace_source(bytes);
            }
            11 => {
                fixture.rows[0][13] = "x".repeat(1_100_000);
                fixture.save_source();
            }
            _ => {
                let mut content = std::fs::read_to_string(fixture.source()).unwrap();
                let index = content.rfind("Preserve this column").unwrap();
                content.replace_range(
                    index..,
                    match variant {
                        12 => "\"unterminated",
                        13 => "bad\"quote\r\n",
                        _ => "\"closed\"junk\r\n",
                    },
                );
                fixture.replace_source(content);
            }
        }
        assert!(
            import_manifest(&pool, &fixture.path(), false)
                .await
                .is_err(),
            "variant {variant}"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM code_systems")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0,
            "variant {variant}"
        );
        assert_eq!(generation(&pool).await, initial, "variant {variant}");
    }
    sqlx::query("CREATE FUNCTION refuse_loinc_publication() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected publication failure'; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER refuse_loinc_publication BEFORE INSERT ON loinc_releases FOR EACH STATEMENT EXECUTE FUNCTION refuse_loinc_publication()").execute(&pool).await.unwrap();
    let fixture = Fixture::new("2.82");
    assert!(import_manifest(&pool, &fixture.path(), false)
        .await
        .is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM concepts")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM loinc_terms")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(generation(&pool).await, initial);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_optional_blanks_are_preserved_without_inventing_properties_or_displays() {
    let (admin, pool, schema) = database().await;
    let mut fixture = Fixture::new("2.82");
    for index in [1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 14, 15] {
        fixture.rows[0][index] = "".into();
    }
    fixture.rows[1][11] = "".into();
    fixture.save_source();
    let result = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    let id = result.code_system_id.unwrap();
    let (display, properties): (Option<String>, sqlx::types::Json<Value>) = sqlx::query_as(
        "SELECT display,properties FROM concepts WHERE code_system_id=$1 AND code='1000-1'",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(display.is_none());
    assert!(properties.as_array().unwrap().iter().all(|property| ![
        "CLASS",
        "CLASSTYPE",
        "ORDER_OBS",
        "CONSUMER_NAME",
        "raw-COMPONENT",
        "raw-PROPERTY",
        "raw-TIME_ASPCT",
        "raw-SYSTEM",
        "raw-SCALE_TYP",
        "raw-METHOD_TYP",
        "raw-EXTERNAL_COPYRIGHT_NOTICE"
    ]
    .contains(&property["code"].as_str().unwrap())));
    assert_eq!(
        properties
            .as_array()
            .unwrap()
            .iter()
            .find(|property| property["code"] == "STATUS")
            .unwrap()["valueString"],
        "ACTIVE"
    );
    let fields: sqlx::types::Json<Value> = sqlx::query_scalar(
        "SELECT fields FROM loinc_terms WHERE code_system_id=$1 AND code='1000-1'",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    for column in [
        "COMPONENT",
        "CLASS",
        "CLASSTYPE",
        "ORDER_OBS",
        "LONG_COMMON_NAME",
        "SHORTNAME",
    ] {
        assert_eq!(fields[column], "");
    }
    let fallback: String = sqlx::query_scalar(
        "SELECT display FROM concepts WHERE code_system_id=$1 AND code='1001-1'",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(fallback, fixture.rows[1][12]);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_concurrent_identical_imports_activate_exactly_one_release() {
    let (admin, pool, schema) = database().await;
    let fixture = Fixture::new("2.82");
    let path = fixture.path();
    let (left, right) = tokio::join!(
        import_manifest(&pool, &path, false),
        import_manifest(&pool, &path, false)
    );
    let (left, right) = (left.unwrap(), right.unwrap());
    assert_ne!(left.unchanged, right.unchanged);
    assert_eq!(left.code_system_id, right.code_system_id);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM code_systems")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM concepts")
            .fetch_one(&pool)
            .await
            .unwrap(),
        4
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_cancelled_activation_retains_no_partial_projection() {
    let (admin, pool, schema) = database().await;
    let fixture = Fixture::new("2.82");
    let initial = generation(&pool).await;
    let mut locker = pool.begin().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *locker)
        .await
        .unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('term-squid/loinc:'||current_schema()||':'||$1,0))").bind("2.82").execute(&mut *locker).await.unwrap();
    let other = pool.clone();
    let path = fixture.path();
    let importing = tokio::spawn(async move { import_manifest(&other, &path, false).await });
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks blocked JOIN pg_locks held ON blocked.locktype=held.locktype AND blocked.database=held.database AND blocked.classid=held.classid AND blocked.objid=held.objid AND blocked.objsubid=held.objsubid WHERE held.pid=$1 AND held.granted AND NOT blocked.granted)").bind(pid).fetch_one(&admin).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    importing.abort();
    assert!(importing.await.unwrap_err().is_cancelled());
    locker.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM code_systems")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(generation(&pool).await, initial);
    assert!(import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap()
        .code_system_id
        .is_some());
    cleanup(admin, pool, schema).await;
}
