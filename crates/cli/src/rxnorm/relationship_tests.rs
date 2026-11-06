//! Generated RXNREL fixtures verify raw source direction and immutable cohort boundaries.
use super::*;

const COLUMNS: &[&str] = &[
    "RXCUI1", "RXAUI1", "STYPE1", "REL", "RXCUI2", "RXAUI2", "STYPE2", "RELA", "RUI", "SRUI",
    "SAB", "SL", "RG", "DIR", "SUPPRESS", "CVF",
];
struct RelationshipFixture {
    base: Fixture,
    rows: Vec<Vec<String>>,
}
impl RelationshipFixture {
    #[allow(clippy::too_many_arguments)]
    fn row(
        cui1: &str,
        aui1: &str,
        kind1: &str,
        rel: &str,
        cui2: &str,
        aui2: &str,
        kind2: &str,
        rela: &str,
        sab: &str,
        suppress: &str,
    ) -> Vec<String> {
        [
            cui1,
            aui1,
            kind1,
            rel,
            cui2,
            aui2,
            kind2,
            rela,
            "shared-rui",
            "",
            sab,
            "",
            "12",
            "Y",
            suppress,
            "",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }
    fn new(version: &str) -> Self {
        let mut base = Fixture::new(version);
        base.rows[0][12] = "SCDC".into();
        base.save_source();
        let edge = Self::row(
            "1001",
            "",
            "CUI",
            "RO",
            "1000",
            "",
            "CUI",
            "has_ingredient",
            "RXNORM",
            "N",
        );
        let rows = vec![
            edge.clone(),
            Self::row(
                "1000",
                "",
                "CUI",
                "RO",
                "1001",
                "",
                "CUI",
                "ingredient_of",
                "RXNORM",
                "N",
            ),
            edge,
            Self::row(
                "",
                "4",
                "AUI",
                "SY",
                "",
                "5",
                "AUI",
                "source_alias",
                "VANDF",
                "",
            ),
            Self::row(
                "1000",
                "1",
                "AUI",
                "SY",
                "C000001",
                "A000001",
                "AUI",
                "external_mapping",
                "RXNORM",
                "N",
            ),
            Self::row(
                "1001", "", "CUI", "RN", "1000", "", "CUI", "isa", "RXNORM", "Y",
            ),
            Self::row(
                "opaque-source-id",
                "",
                "SOURCE-CODE",
                "RQ",
                "other-source-id",
                "",
                "SCUI",
                "unknown_relation",
                "OTHER",
                "E",
            ),
            Self::row(
                "1002",
                "",
                "CUI",
                "RO",
                "1000",
                "",
                "CUI",
                "historical_fact",
                "OTHER",
                "O",
            ),
        ];
        let mut fixture = Self { base, rows };
        fixture.save();
        fixture
    }
    fn save(&mut self) {
        let content = self
            .rows
            .iter()
            .map(|row| format!("{}|\n", row.join("|")))
            .collect::<String>();
        self.replace(content);
        self.base.manifest["relationships"]["expectedRows"] = self.rows.len().into();
        self.base.save_manifest();
    }
    fn replace(&mut self, content: impl AsRef<[u8]>) {
        std::fs::write(self.base.root.join("RXNREL.RRF"), content.as_ref()).unwrap();
        self.base.manifest["relationships"] = json!({"path":"RXNREL.RRF","sha256":hex::encode(Sha256::digest(content.as_ref())),"expectedRows":self.rows.len(),"version":self.base.manifest["version"],"fullMonthly":true});
        self.base.save_manifest();
    }
    fn auxiliary(&mut self) {
        let sty = ["1000", "1001", "1002", "1003", "1004", "2000"]
            .into_iter()
            .map(|cui| format!("{cui}|T121|A1.4.1.1|Generated substance|||\n"))
            .collect::<String>();
        for (name, file, content, rows) in [
            ("semanticTypes", "RXNSTY.RRF", sty, 6),
            ("attributes", "RXNSAT.RRF", String::new(), 0),
        ] {
            std::fs::write(self.base.root.join(file), content.as_bytes()).unwrap();
            self.base.manifest[name] = json!({"path":file,"sha256":hex::encode(Sha256::digest(content.as_bytes())),"expectedRows":rows,"version":self.base.manifest["version"],"fullMonthly":true});
        }
        self.base.save_manifest();
    }
    fn without_relationships(&mut self) {
        self.base
            .manifest
            .as_object_mut()
            .unwrap()
            .remove("relationships");
        self.base.save_manifest();
    }
}

#[tokio::test]
async fn rxnorm_relationship_manifest_rejects_foreign_editions_bounds_and_source_formats_before_database(
) {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://unused:unused@127.0.0.1/unused")
        .unwrap();
    for variant in 0..7 {
        let mut fixture = RelationshipFixture::new("03022026");
        match variant {
            0 => fixture.base.manifest["relationships"]["version"] = "04062026".into(),
            1 => fixture.base.manifest["relationships"]["fullMonthly"] = false.into(),
            2 => fixture.base.manifest["relationships"]["path"] = "other.RRF".into(),
            3 => fixture.base.manifest["relationships"]["sha256"] = "F".repeat(64).into(),
            4 => fixture.base.manifest["relationships"]["expectedRows"] = 20_000_001.into(),
            5 => fixture.base.manifest["relationships"]["unknown"] = true.into(),
            _ => fixture.base.manifest["relationships"]["expectedRows"] = (-1_i64).into(),
        }
        fixture.base.save_manifest();
        assert!(
            import_manifest(&pool, &fixture.base.path(), false)
                .await
                .is_err(),
            "variant {variant}"
        );
    }
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationship_four_cohorts_preserve_all_fields_duplicates_and_source_context() {
    let (admin, pool, schema) = database().await;
    for (index, version) in ["03022026", "04062026", "05042026", "06012026"]
        .into_iter()
        .enumerate()
    {
        let mut fixture = RelationshipFixture::new(version);
        let aux = index & 1 != 0;
        let rel = index & 2 != 0;
        if aux {
            fixture.auxiliary();
        }
        if !rel {
            fixture.without_relationships();
        }
        let imported = import_manifest(&pool, &fixture.base.path(), false)
            .await
            .unwrap();
        let id = imported.code_system_id.unwrap();
        let(sty,sat,relations,source):(bool,bool,bool,sqlx::types::Json<Value>)=sqlx::query_as("SELECT semantic_types_loaded,attributes_loaded,relationships_loaded,relationship_source FROM rxnorm_releases WHERE code_system_id=$1").bind(id).fetch_one(&pool).await.unwrap();
        assert_eq!((sty, sat, relations), (aux, aux, rel));
        let raw: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(
            "SELECT fields FROM rxnorm_relationships WHERE code_system_id=$1 ORDER BY source_row",
        )
        .bind(id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(raw.len(), if rel { fixture.rows.len() } else { 0 });
        if rel {
            for (actual, row) in raw.iter().zip(&fixture.rows) {
                for (column, value) in COLUMNS.iter().zip(row) {
                    assert_eq!(actual[*column], *value);
                }
            }
            assert_eq!(
                raw[0], raw[2],
                "Exact duplicate assertions retain distinct source ordinals"
            );
            assert_eq!(raw[0]["RG"], "12");
            assert_eq!(raw[0]["DIR"], "Y");
            assert_eq!(source["rows"], fixture.rows.len());
            assert_eq!(
                source["bytes"],
                std::fs::metadata(fixture.base.root.join("RXNREL.RRF"))
                    .unwrap()
                    .len()
            );
            assert_eq!(source["version"], version);
            assert_eq!(
                source["sha256"],
                fixture.base.manifest["relationships"]["sha256"]
            );
            assert!(imported.relationship_source.is_some());
        } else {
            assert_eq!(*source, json!({}));
            assert!(imported.relationship_source.is_none());
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM closure_table")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM concept_maps")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationship_legacy_fingerprints_nulls_idempotence_and_path_independence_are_preserved(
) {
    let (admin, pool, schema) = database().await;
    for (index, version) in ["03022026", "04062026"].into_iter().enumerate() {
        let mut fixture = RelationshipFixture::new(version);
        if index == 1 {
            fixture.auxiliary();
        }
        fixture.without_relationships();
        let mut identity = json!({"format":"rxnorm-rrf-v1","projection":"current-active-v1","version":version,"fullMonthly":true,"sourceSha256":fixture.base.manifest["source"]["sha256"],"expectedRows":fixture.base.rows.len(),"expectedConcepts":2});
        if index == 1 {
            identity["auxiliaryProjection"] = "semantic-ndc-v1".into();
            for (key, file) in [
                ("semanticTypes", "RXNSTY.RRF"),
                ("attributes", "RXNSAT.RRF"),
            ] {
                let source = &fixture.base.manifest[key];
                identity[key] = json!({"file":file,"sha256":source["sha256"],"expectedRows":source["expectedRows"],"version":version,"fullMonthly":true});
            }
        }
        let imported = import_manifest(&pool, &fixture.base.path(), false)
            .await
            .unwrap();
        assert_eq!(
            imported.fingerprint,
            hex::encode(Sha256::digest(serde_json::to_vec(&identity).unwrap()))
        );
        fixture.base.manifest["relationships"] = Value::Null;
        fixture.base.save_manifest();
        assert!(
            import_manifest(&pool, &fixture.base.path(), false)
                .await
                .unwrap()
                .unchanged
        );
        fixture.save();
        assert!(import_manifest(&pool, &fixture.base.path(), false)
            .await
            .unwrap_err()
            .to_string()
            .contains("Conflicting immutable"));
    }
    let mut fixture = RelationshipFixture::new("05042026");
    fixture.auxiliary();
    let imported = import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap();
    let initial = generation(&pool).await;
    std::fs::create_dir(fixture.base.root.join("moved")).unwrap();
    std::fs::copy(
        fixture.base.root.join("RXNREL.RRF"),
        fixture.base.root.join("moved/RXNREL.RRF"),
    )
    .unwrap();
    fixture.base.manifest["relationships"]["path"] = "moved/RXNREL.RRF".into();
    fixture.base.save_manifest();
    let repeated = import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap();
    assert!(repeated.unchanged);
    assert_eq!(repeated.fingerprint, imported.fingerprint);
    assert_eq!(generation(&pool).await, initial);
    fixture.rows[0][13] = "N".into();
    fixture.save();
    assert!(import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap_err()
        .to_string()
        .contains("Conflicting immutable"));
    assert_eq!(generation(&pool).await, initial);
    fixture.without_relationships();
    assert!(import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap_err()
        .to_string()
        .contains("Conflicting immutable"));
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationship_malformed_sources_and_numeric_reference_errors_publish_nothing() {
    let (admin, pool, schema) = database().await;
    let initial = generation(&pool).await;
    for variant in 0..17 {
        let mut fixture = RelationshipFixture::new("03022026");
        fixture.auxiliary();
        match variant {
            0 => {
                fixture.base.manifest["relationships"]["sha256"] = "0".repeat(64).into();
                fixture.base.save_manifest();
            }
            1 => {
                fixture.base.manifest["relationships"]["expectedRows"] = 99.into();
                fixture.base.save_manifest();
            }
            2 => {
                fixture.rows[0][0] = "9999".into();
                fixture.save();
            }
            3 => {
                fixture.rows[3][1] = "9999".into();
                fixture.save();
            }
            4 => {
                fixture.rows[3][0] = "1001".into();
                fixture.save();
            }
            5 => {
                fixture.rows[0][0] = "01001".into();
                fixture.save();
            }
            6 => {
                fixture.rows[3][1] = "0".into();
                fixture.save();
            }
            7 => {
                fixture.rows[0][0] = "".into();
                fixture.save();
            }
            8 => {
                fixture.rows[3][1] = "".into();
                fixture.save();
            }
            9 => {
                fixture.rows[0][14] = "Z".into();
                fixture.save();
            }
            10 => {
                fixture.rows[0].push("extra".into());
                fixture.save();
            }
            11 => {
                fixture.rows[0][7] = "bad\0label".into();
                fixture.save();
            }
            12 => {
                fixture.rows[0][7] = "x".repeat(256);
                fixture.save();
            }
            13 => fixture.replace(b"\xff|\n"),
            14 => fixture.replace("x".repeat(1024 * 1024 + 1)),
            15 => {
                let content = std::fs::read_to_string(fixture.base.root.join("RXNREL.RRF"))
                    .unwrap()
                    .replacen("|\n", "\n", 1);
                fixture.replace(content);
            }
            _ => {
                fixture.base.manifest["attributes"]["sha256"] = "0".repeat(64).into();
                fixture.base.save_manifest();
            }
        }
        assert!(
            import_manifest(&pool, &fixture.base.path(), false)
                .await
                .is_err(),
            "variant {variant}"
        );
        assert_eq!(inventory(&pool).await, 0, "variant {variant}");
        assert_eq!(generation(&pool).await, initial, "variant {variant}");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT (SELECT count(*) FROM rxnorm_relationships)+(SELECT count(*) FROM rxnorm_atoms)+(SELECT count(*) FROM rxnorm_semantic_types)+(SELECT count(*) FROM rxnorm_attributes)").fetch_one(&pool).await.unwrap(),0);
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationship_empty_sources_crlf_bom_and_unterminated_final_record_are_lossless() {
    let (admin, pool, schema) = database().await;
    let mut empty = RelationshipFixture::new("03022026");
    empty.rows.clear();
    empty.save();
    let imported = import_manifest(&pool, &empty.base.path(), false)
        .await
        .unwrap();
    let id = imported.code_system_id.unwrap();
    let(loaded,source):(bool,sqlx::types::Json<Value>)=sqlx::query_as("SELECT relationships_loaded,relationship_source FROM rxnorm_releases WHERE code_system_id=$1").bind(id).fetch_one(&pool).await.unwrap();
    assert!(loaded);
    assert_eq!(source["rows"], 0);
    assert_eq!(source["bytes"], 0);
    let mut fixture = RelationshipFixture::new("04062026");
    let content = std::fs::read_to_string(fixture.base.root.join("RXNREL.RRF"))
        .unwrap()
        .replace('\n', "\r\n");
    fixture.replace(format!(
        "\u{feff}{}",
        content.trim_end_matches(['\r', '\n'])
    ));
    let imported = import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap();
    assert_eq!(
        imported.relationship_source.unwrap()["rows"],
        fixture.rows.len()
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationship_dry_run_publication_failure_and_cancellation_are_atomic() {
    let (admin, pool, schema) = database().await;
    let mut fixture = RelationshipFixture::new("03022026");
    fixture.auxiliary();
    let initial = generation(&pool).await;
    let dry = import_manifest(&pool, &fixture.base.path(), true)
        .await
        .unwrap();
    assert!(dry.code_system_id.is_none());
    assert!(dry.relationship_source.is_some());
    assert_eq!(inventory(&pool).await, 0);
    assert_eq!(generation(&pool).await, initial);
    sqlx::raw_sql("CREATE FUNCTION fail_relationship() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected relationship publication failure'; END $$; CREATE TRIGGER fail_relationship BEFORE INSERT ON rxnorm_releases FOR EACH ROW EXECUTE FUNCTION fail_relationship()").execute(&pool).await.unwrap();
    assert!(import_manifest(&pool, &fixture.base.path(), false)
        .await
        .is_err());
    assert_eq!(inventory(&pool).await, 0);
    assert_eq!(generation(&pool).await, initial);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT (SELECT count(*) FROM rxnorm_relationships)+(SELECT count(*) FROM rxnorm_atoms)+(SELECT count(*) FROM rxnorm_semantic_types)+(SELECT count(*) FROM rxnorm_attributes)").fetch_one(&pool).await.unwrap(),0);
    sqlx::query("DROP TRIGGER fail_relationship ON rxnorm_releases")
        .execute(&pool)
        .await
        .unwrap();
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE code_systems IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let path = fixture.base.path();
    let other = pool.clone();
    let task = tokio::spawn(async move { import_manifest(&other, &path, false).await });
    tokio::time::timeout(std::time::Duration::from_secs(5),async{loop{let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity a JOIN pg_locks l ON l.pid=a.pid JOIN pg_class c ON c.oid=l.relation JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname='code_systems' AND NOT l.granted AND a.query LIKE 'SELECT c.id,c.native_provider,r.fingerprint FROM code_systems c LEFT JOIN rxnorm_releases%')").bind(&schema).fetch_one(&admin).await.unwrap();if blocked{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    blocker.rollback().await.unwrap();
    assert_eq!(inventory(&pool).await, 0);
    assert_eq!(generation(&pool).await, initial);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationship_concurrent_activation_is_idempotent_and_source_trigger_tracks_changes()
{
    let (admin, pool, schema) = database().await;
    let fixture = RelationshipFixture::new("03022026");
    let path = fixture.base.path();
    let (left, right) = tokio::join!(
        import_manifest(&pool, &path, false),
        import_manifest(&pool, &path, false)
    );
    let (left, right) = (left.unwrap(), right.unwrap());
    assert_eq!(left.code_system_id, right.code_system_id);
    assert_ne!(left.unchanged, right.unchanged);
    assert_eq!(inventory(&pool).await, 1);
    let initial = generation(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE rxnorm_relationships SET suppress='Y' WHERE code_system_id=$1 AND source_row=1",
    )
    .bind(left.code_system_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(generation(&pool).await, initial);
    sqlx::query(
        "UPDATE rxnorm_relationships SET suppress='Y' WHERE code_system_id=$1 AND source_row=1",
    )
    .bind(left.code_system_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(generation(&pool).await > initial);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationship_downgrade_preserves_legacy_and_refuses_even_empty_loaded_cohorts() {
    let (admin, pool, schema) = database().await;
    let mut fixture = RelationshipFixture::new("03022026");
    fixture.without_relationships();
    let legacy = import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!(
        "../../../backend/migrations/20261106000002_rxnorm_relationships.down.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let fingerprint: String =
        sqlx::query_scalar("SELECT fingerprint FROM rxnorm_releases WHERE code_system_id=$1")
            .bind(legacy.code_system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(fingerprint, legacy.fingerprint);
    cleanup(admin, pool, schema).await;
    let (admin, pool, schema) = database().await;
    let mut fixture = RelationshipFixture::new("03022026");
    fixture.rows.clear();
    fixture.save();
    import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap();
    assert!(sqlx::raw_sql(include_str!(
        "../../../backend/migrations/20261106000002_rxnorm_relationships.down.sql"
    ))
    .execute(&pool)
    .await
    .is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM rxnorm_releases WHERE relationships_loaded"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    cleanup(admin, pool, schema).await;
}
