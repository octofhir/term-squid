//! Generated complete-file fixtures exercise the loader without licensed drug content.
#[path = "auxiliary_tests.rs"]
mod auxiliary;
#[path = "relationship_tests.rs"]
mod relationships;
use super::*;

struct Fixture {
    root: PathBuf,
    rows: Vec<Vec<String>>,
    manifest: Value,
}
impl Fixture {
    fn atom(cui: &str, aui: &str, sab: &str, tty: &str, term: &str, suppress: &str) -> Vec<String> {
        [
            cui,
            "ENG",
            "",
            "",
            "",
            "",
            "",
            aui,
            "source-atom",
            "source-concept",
            "",
            sab,
            tty,
            "not-the-public-code",
            term,
            "",
            suppress,
            "",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }
    fn new(version: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.benchmarks/rxnorm-tests")
            .join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&root).unwrap();
        let rows = vec![
            Self::atom(
                "1000",
                "1",
                "RXNORM",
                "SCD",
                "Generated clinical \"μ\" \\ drug",
                "N",
            ),
            Self::atom("1000", "2", "RXNORM", "PSN", "Generated display alias", "N"),
            Self::atom("1000", "3", "RXNORM", "SY", "Generated synonym", "N"),
            Self::atom("1000", "4", "VANDF", "PT", "Foreign source name", "N"),
            Self::atom("1001", "5", "RXNORM", "IN", "Generated ingredient", "N"),
            Self::atom(
                "1002",
                "6",
                "RXNORM",
                "SCD",
                "Obsolete normalized name",
                "O",
            ),
            Self::atom(
                "1003",
                "7",
                "RXNORM",
                "SCD",
                "Quantified normalized name",
                "E",
            ),
            Self::atom(
                "1004",
                "8",
                "RXNORM",
                "SCD",
                "Suppressed normalized name",
                "Y",
            ),
            Self::atom(
                "2000",
                "9",
                "VANDF",
                "PT",
                "Not a normalized RxNorm code",
                "N",
            ),
            Self::atom("1000", "10", "RXNORM", "OLD", "Older atom type", "O"),
        ];
        let mut result = Self {
            root,
            rows,
            manifest: json!({"format":"rxnorm-rrf-v1","version":version,"fullMonthly":true,"expectedRows":10,"expectedConcepts":2,"source":{"path":"RXNCONSO.RRF","sha256":""}}),
        };
        result.save_source();
        result
    }
    fn path(&self) -> PathBuf {
        self.root.join("manifest.json")
    }
    fn source(&self) -> PathBuf {
        self.root.join(SOURCE_TABLE)
    }
    fn save_manifest(&self) {
        std::fs::write(
            self.path(),
            serde_json::to_vec_pretty(&self.manifest).unwrap(),
        )
        .unwrap();
    }
    fn save_source(&mut self) {
        self.manifest["expectedRows"] = self.rows.len().into();
        self.replace_source(
            self.rows
                .iter()
                .map(|row| format!("{}|\n", row.join("|")))
                .collect::<String>(),
        );
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
    let search = format!("SET search_path TO {schema},public");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .after_connect(move |connection, _| {
            let search = search.clone();
            Box::pin(async move {
                sqlx::query(sqlx::AssertSqlSafe(search.as_str()))
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
async fn inventory(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM code_systems")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn rxnorm_manifest_rejects_subsets_bad_versions_and_bounds_before_database() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://unused:unused@127.0.0.1/unused")
        .unwrap();
    for variant in 0..10 {
        let mut fixture = Fixture::new("03022026");
        match variant {
            0 => fixture.manifest["fullMonthly"] = false.into(),
            1 => fixture.manifest["format"] = "other".into(),
            2 => fixture.manifest["version"] = "02292025".into(),
            3 => fixture.manifest["source"]["sha256"] = "F".repeat(64).into(),
            4 => fixture.manifest["expectedRows"] = 0.into(),
            5 => fixture.manifest["expectedRows"] = 20_000_001.into(),
            6 => fixture.manifest["expectedConcepts"] = 0.into(),
            7 => fixture.manifest["expectedConcepts"] = 11.into(),
            8 => fixture.manifest["source"]["path"] = "prescribe.RRF".into(),
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
async fn rxnorm_import_preserves_all_atoms_source_identity_and_active_projection() {
    let (admin, pool, schema) = database().await;
    let fixture = Fixture::new("03022026");
    let result = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    let id = result.code_system_id.unwrap();
    assert_eq!(result.source.rows, 10);
    assert_eq!(result.concepts, 2);
    let metadata: sqlx::types::Json<Value> =
        sqlx::query_scalar("SELECT content FROM code_systems WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(metadata["content"], "not-present");
    assert_eq!(metadata["date"], "2026-03-02");
    assert_eq!(metadata["caseSensitive"], true);
    assert!(metadata.get("concept").is_none());
    for row in &fixture.rows {
        let fields: sqlx::types::Json<Value> = sqlx::query_scalar(
            "SELECT fields FROM rxnorm_atoms WHERE code_system_id=$1 AND rxaui=$2",
        )
        .bind(id)
        .bind(&row[7])
        .fetch_one(&pool)
        .await
        .unwrap();
        for (column, value) in [
            "RXCUI", "LAT", "TS", "LUI", "STT", "SUI", "ISPREF", "RXAUI", "SAUI", "SCUI", "SDUI",
            "SAB", "TTY", "CODE", "STR", "SRL", "SUPPRESS", "CVF",
        ]
        .into_iter()
        .zip(row)
        {
            assert_eq!(fields[column], *value);
        }
    }
    let projected:Vec<(String,String,sqlx::types::Json<Value>,sqlx::types::Json<Value>)>=sqlx::query_as("SELECT code,display,properties,designations FROM concepts WHERE code_system_id=$1 ORDER BY code").bind(id).fetch_all(&pool).await.unwrap();
    assert_eq!(projected.len(), 2);
    assert_eq!(projected[0].0, "1000");
    assert_eq!(projected[0].1, fixture.rows[0][14]);
    let props = projected[0].2.as_array().unwrap();
    for expected in [
        json!({"code":"inactive","valueBoolean":false}),
        json!({"code":"preferredTTY","valueCode":"SCD"}),
        json!({"code":"SAB","valueCode":"VANDF"}),
        json!({"code":"TTY","valueCode":"OLD"}),
        json!({"code":"TTY","valueCode":"PSN"}),
    ] {
        assert!(props.contains(&expected), "{props:?}");
    }
    assert!(!props.contains(&json!({"code":"TTY","valueCode":"SY"})));
    assert_eq!(projected[0].3.as_array().unwrap().len(), 2);
    assert_eq!(
        projected[0].3[0],
        json!({"language":"en","use":{"system":SYSTEM,"code":"PSN"},"value":"Generated display alias"})
    );
    let provenance: sqlx::types::Json<Value> =
        sqlx::query_scalar("SELECT provenance FROM rxnorm_releases WHERE code_system_id=$1")
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
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM closure_table")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert!(sqlx::raw_sql(include_str!(
        "../../../backend/migrations/20261106000000_rxnorm_rrf.down.sql"
    ))
    .execute(&pool)
    .await
    .is_err());
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_dry_run_idempotence_immutable_conflicts_and_versions_are_atomic() {
    let (admin, pool, schema) = database().await;
    let mut fixture = Fixture::new("03022026");
    let initial = generation(&pool).await;
    assert!(import_manifest(&pool, &fixture.path(), true)
        .await
        .unwrap()
        .code_system_id
        .is_none());
    assert_eq!(inventory(&pool).await, 0);
    assert_eq!(generation(&pool).await, initial);
    let first = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    let committed = generation(&pool).await;
    let timestamp: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT updated_at FROM code_systems WHERE id=$1")
            .bind(first.code_system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let repeated = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    assert!(repeated.unchanged);
    assert_eq!(repeated.code_system_id, first.code_system_id);
    assert_eq!(generation(&pool).await, committed);
    assert_eq!(
        timestamp,
        sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
            "SELECT updated_at FROM code_systems WHERE id=$1"
        )
        .bind(first.code_system_id)
        .fetch_one(&pool)
        .await
        .unwrap()
    );
    fixture.rows[0][14] = "Changed immutable display".into();
    fixture.save_source();
    assert!(import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap_err()
        .to_string()
        .contains("Conflicting immutable"));
    assert_eq!(generation(&pool).await, committed);
    let second = Fixture::new("04062026");
    let second = import_manifest(&pool, &second.path(), false).await.unwrap();
    assert_ne!(second.code_system_id, first.code_system_id);
    assert_eq!(inventory(&pool).await, 2);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_malformed_rrf_checksum_duplicate_atoms_and_projection_errors_publish_nothing() {
    let (admin, pool, schema) = database().await;
    let initial = generation(&pool).await;
    for variant in 0..18 {
        let mut fixture = Fixture::new("03022026");
        match variant {
            0 => {
                fixture.manifest["source"]["sha256"] = "0".repeat(64).into();
                fixture.save_manifest();
            }
            1 => {
                fixture.rows[0][0] = "01000".into();
                fixture.save_source();
            }
            2 => {
                fixture.rows[0][7] = "0".into();
                fixture.save_source();
            }
            3 => {
                fixture.rows[0][16] = "".into();
                fixture.save_source();
            }
            4 => {
                fixture.rows.push(fixture.rows[0].clone());
                fixture.save_source();
            }
            5 => {
                fixture.rows[0][14] = "bad\0term".into();
                fixture.save_source();
            }
            6 => {
                fixture.rows[0].push("extra".into());
                fixture.save_source();
            }
            7 => {
                fixture.rows[0][14] = "".into();
                fixture.save_source();
            }
            8 => {
                fixture.replace_source(b"1000|ENG|incomplete|\n");
            }
            9 => {
                fixture.replace_source(b"\xff|\n");
            }
            10 => {
                fixture.replace_source("x".repeat(1024 * 1024 + 1));
            }
            11 => {
                fixture.rows.push(Fixture::atom(
                    "1000",
                    "11",
                    "RXNORM",
                    "SBD",
                    "Ambiguous primary",
                    "N",
                ));
                fixture.save_source();
            }
            12 => {
                fixture.rows[0][1] = "SPA".into();
                fixture.save_source();
            }
            13 => {
                fixture.rows[0][12] = "SY".into();
                fixture.save_source();
            }
            14 => {
                fixture.manifest["expectedRows"] = 11.into();
                fixture.save_manifest();
            }
            15 => {
                fixture.manifest["expectedConcepts"] = 3.into();
                fixture.save_manifest();
            }
            16 => {
                let mut content = std::fs::read_to_string(fixture.source()).unwrap();
                content = content.replacen("|\n", "\n", 1);
                fixture.replace_source(content);
            }
            _ => {
                fixture.rows[0][12] = "UNKNOWN".into();
                fixture.save_source();
            }
        }
        assert!(
            import_manifest(&pool, &fixture.path(), false)
                .await
                .is_err(),
            "variant {variant}"
        );
        assert_eq!(inventory(&pool).await, 0, "variant {variant}");
        assert_eq!(generation(&pool).await, initial, "variant {variant}");
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_equivalent_primary_atoms_crlf_and_no_final_newline_preserve_identity() {
    let (admin, pool, schema) = database().await;
    let mut fixture = Fixture::new("03022026");
    fixture.rows[4][12] = "ET".into();
    let mut duplicate = fixture.rows[0].clone();
    duplicate[7] = "11".into();
    fixture.rows.push(duplicate);
    fixture.save_source();
    let content = std::fs::read_to_string(fixture.source())
        .unwrap()
        .replace('\n', "\r\n");
    fixture.replace_source(format!(
        "\u{feff}{}",
        content.trim_end_matches(['\r', '\n'])
    ));
    let result = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    assert_eq!(result.source.rows, 11);
    assert_eq!(result.concepts, 2);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_publication_failure_rolls_back_all_atoms_and_generations() {
    let (admin, pool, schema) = database().await;
    let fixture = Fixture::new("03022026");
    let initial = generation(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION fail_rxnorm() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected publication failure'; END $$; CREATE TRIGGER fail_rxnorm BEFORE INSERT ON rxnorm_releases FOR EACH ROW EXECUTE FUNCTION fail_rxnorm()").execute(&pool).await.unwrap();
    assert!(import_manifest(&pool, &fixture.path(), false)
        .await
        .is_err());
    assert_eq!(inventory(&pool).await, 0);
    assert_eq!(generation(&pool).await, initial);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM rxnorm_atoms")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_concurrent_identical_activation_serializes_without_replacement() {
    let (admin, pool, schema) = database().await;
    let fixture = Fixture::new("03022026");
    let path = fixture.path();
    let (left, right) = tokio::join!(
        import_manifest(&pool, &path, false),
        import_manifest(&pool, &path, false)
    );
    let (left, right) = (left.unwrap(), right.unwrap());
    assert_eq!(left.code_system_id, right.code_system_id);
    assert_ne!(left.unchanged, right.unchanged);
    assert_eq!(inventory(&pool).await, 1);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_cancelled_activation_and_oversized_file_publish_nothing() {
    let (admin, pool, schema) = database().await;
    let fixture = Fixture::new("03022026");
    let initial = generation(&pool).await;
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE code_systems IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let path = fixture.path();
    let other = pool.clone();
    let task = tokio::spawn(async move { import_manifest(&other, &path, false).await });
    let waiting = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity a JOIN pg_locks l ON l.pid=a.pid JOIN pg_class c ON c.oid=l.relation JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname='code_systems' AND NOT l.granted AND a.query LIKE 'SELECT c.id,c.native_provider,r.fingerprint FROM code_systems c LEFT JOIN rxnorm_releases%')")
                .bind(&schema).fetch_one(&admin).await.unwrap();
            if blocked { break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await;
    assert!(waiting.is_ok());
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    blocker.rollback().await.unwrap();
    assert_eq!(inventory(&pool).await, 0);
    assert_eq!(generation(&pool).await, initial);
    std::fs::OpenOptions::new()
        .write(true)
        .open(fixture.source())
        .unwrap()
        .set_len(reader::MAX_FILE + 1)
        .unwrap();
    assert!(import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap_err()
        .to_string()
        .contains("4 GiB"));
    assert_eq!(inventory(&pool).await, 0);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_aliases_are_not_truncated_and_projected_bytes_are_bounded() {
    let (admin, pool, schema) = database().await;
    let mut fixture = Fixture::new("03022026");
    for index in 0..257 {
        fixture.rows.push(Fixture::atom(
            "1000",
            &(100 + index).to_string(),
            "RXNORM",
            "SY",
            &format!("Generated complete alias {index}"),
            "N",
        ));
    }
    fixture.save_source();
    let imported = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    let count: i32 = sqlx::query_scalar("SELECT jsonb_array_length(designations) FROM concepts WHERE code_system_id=$1 AND code='1000'")
        .bind(imported.code_system_id).fetch_one(&pool).await.unwrap();
    assert_eq!(
        count, 259,
        "All aliases must survive import; rich reads enforce their own separate bounds"
    );
    let initial = generation(&pool).await;
    let mut oversized = Fixture::new("04062026");
    oversized.rows[1][14] = "x".repeat(600_000);
    oversized.save_source();
    assert!(import_manifest(&pool, &oversized.path(), false)
        .await
        .unwrap_err()
        .to_string()
        .contains("4 MiB"));
    assert_eq!(inventory(&pool).await, 1);
    assert_eq!(generation(&pool).await, initial);
    cleanup(admin, pool, schema).await;
}
