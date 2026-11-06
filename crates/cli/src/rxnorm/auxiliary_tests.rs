//! Original same-edition auxiliary fixtures; all checks enter through the public importer.
use super::*;

struct AuxiliaryFixture {
    base: Fixture,
    semantic_types: Vec<Vec<String>>,
    attributes: Vec<Vec<String>>,
}
impl AuxiliaryFixture {
    fn attribute(
        cui: &str,
        aui: &str,
        stype: &str,
        atn: &str,
        sab: &str,
        value: &str,
        suppress: &str,
    ) -> Vec<String> {
        [
            cui,
            "",
            "",
            aui,
            stype,
            "source-code",
            "",
            "",
            atn,
            sab,
            value,
            suppress,
            "",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }
    fn new(version: &str) -> Self {
        let base = Fixture::new(version);
        let mut semantic_types = ["1000", "1001", "1002", "1003", "1004", "2000"]
            .into_iter()
            .map(|cui| {
                [
                    cui,
                    "T121",
                    "A1.4.1.1",
                    "Generated pharmacologic substance",
                    "",
                    "",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect()
            })
            .collect::<Vec<_>>();
        semantic_types.push(
            [
                "1000",
                "T200",
                "A1.2.3",
                "Generated clinical drug \"μ\" \\",
                "AT-generated",
                "",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        );
        let curated = Self::attribute("1000", "1", "AUI", "NDC", "RXNORM", "00012345678", "N");
        let attributes = vec![
            curated.clone(),
            curated,
            Self::attribute(
                "1000",
                "4",
                "AUI",
                "NDC",
                "VANDF",
                "not-normalized-source-code",
                "N",
            ),
            Self::attribute(
                "1000",
                "1",
                "AUI",
                "NDC",
                "RXNORM",
                "obsolete-source-code",
                "O",
            ),
            Self::attribute("1000", "4", "AUI", "AMBIGUITY_FLAG", "RXNORM", "Base", "N"),
            Self::attribute("1001", "", "CUI", "RXN_HUMAN_DRUG", "RXNORM", "US", "N"),
            Self::attribute(
                "",
                "relationship-id",
                "RUI",
                "opaque",
                "OTHER",
                "preserve",
                "Y",
            ),
            Self::attribute(
                "1002",
                "6",
                "AUI",
                "NDC",
                "RXNORM",
                "historical-raw-code",
                "N",
            ),
        ];
        let mut fixture = Self {
            base,
            semantic_types,
            attributes,
        };
        fixture.save(true, true);
        fixture
    }
    fn save(&mut self, semantic: bool, attributes: bool) {
        self.base
            .manifest
            .as_object_mut()
            .unwrap()
            .remove("semanticTypes");
        self.base
            .manifest
            .as_object_mut()
            .unwrap()
            .remove("attributes");
        for (enabled, key, file, rows) in [
            (
                semantic,
                "semanticTypes",
                "RXNSTY.RRF",
                &self.semantic_types,
            ),
            (attributes, "attributes", "RXNSAT.RRF", &self.attributes),
        ] {
            if enabled {
                let content = rows
                    .iter()
                    .map(|row| format!("{}|\n", row.join("|")))
                    .collect::<String>();
                std::fs::write(self.base.root.join(file), content.as_bytes()).unwrap();
                self.base.manifest[key] = json!({"path":file,"sha256":hex::encode(Sha256::digest(content.as_bytes())),"expectedRows":rows.len(),"version":self.base.manifest["version"],"fullMonthly":true});
            }
        }
        self.base.save_manifest();
    }
    fn replace(&mut self, key: &str, content: impl AsRef<[u8]>) {
        let file = self.base.manifest[key]["path"].as_str().unwrap();
        std::fs::write(self.base.root.join(file), content.as_ref()).unwrap();
        self.base.manifest[key]["sha256"] = hex::encode(Sha256::digest(content.as_ref())).into();
        self.base.save_manifest();
    }
}

#[tokio::test]
async fn rxnorm_auxiliary_manifest_rejects_wrong_editions_bounds_and_source_attestations_before_database(
) {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://unused:unused@127.0.0.1/unused")
        .unwrap();
    for variant in 0..10 {
        let mut fixture = AuxiliaryFixture::new("03022026");
        match variant {
            0 => fixture.base.manifest["semanticTypes"]["version"] = "04062026".into(),
            1 => fixture.base.manifest["attributes"]["fullMonthly"] = false.into(),
            2 => fixture.base.manifest["semanticTypes"]["expectedRows"] = 0.into(),
            3 => fixture.base.manifest["attributes"]["expectedRows"] = 20_000_001.into(),
            4 => fixture.base.manifest["semanticTypes"]["path"] = "OTHER.RRF".into(),
            5 => fixture.base.manifest["attributes"]["sha256"] = "F".repeat(64).into(),
            6 => fixture.base.manifest["semanticTypes"]["unknown"] = true.into(),
            7 => fixture.base.manifest["semanticTypes"]["fullMonthly"] = false.into(),
            8 => fixture.base.manifest["attributes"]["version"] = "03022026|wrong".into(),
            _ => fixture.base.manifest["semanticTypes"]["expectedRows"] = (-1_i64).into(),
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
async fn rxnorm_auxiliary_four_cohorts_preserve_raw_rows_flags_and_typed_properties() {
    let (admin, pool, schema) = database().await;
    for (index, version) in ["03022026", "04062026", "05042026", "06012026"]
        .into_iter()
        .enumerate()
    {
        let mut fixture = AuxiliaryFixture::new(version);
        let sty = index & 1 != 0;
        let sat = index & 2 != 0;
        fixture.save(sty, sat);
        let imported = import_manifest(&pool, &fixture.base.path(), false)
            .await
            .unwrap();
        let id = imported.code_system_id.unwrap();
        let (sty_loaded,sat_loaded,sources):(bool,bool,sqlx::types::Json<Value>)=sqlx::query_as("SELECT semantic_types_loaded,attributes_loaded,auxiliary_sources FROM rxnorm_releases WHERE code_system_id=$1").bind(id).fetch_one(&pool).await.unwrap();
        assert_eq!((sty_loaded, sat_loaded), (sty, sat));
        assert_eq!(
            sources.as_object().unwrap().len(),
            usize::from(sty) + usize::from(sat)
        );
        let props: sqlx::types::Json<Value> = sqlx::query_scalar(
            "SELECT properties FROM concepts WHERE code_system_id=$1 AND code='1000'",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let props = props.as_array().unwrap();
        assert_eq!(
            props.contains(&json!({"code":"TUI","valueCode":"T200"})),
            sty
        );
        assert_eq!(
            props.contains(&json!({"code":"STN","valueString":"A1.2.3"})),
            sty
        );
        assert_eq!(
            props.iter().filter(|p| p["code"] == "NDC").count(),
            usize::from(sat)
        );
        if sat {
            assert!(props.contains(&json!({"code":"NDC","valueString":"00012345678"})));
        }
        let metadata: sqlx::types::Json<Value> =
            sqlx::query_scalar("SELECT content FROM code_systems WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(metadata["content"], "not-present");
        assert_eq!(
            metadata["property"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["code"] == "TUI"),
            sty
        );
        assert_eq!(
            metadata["property"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["code"] == "NDC"),
            sat
        );
        if sty {
            let fields:Vec<sqlx::types::Json<Value>>=sqlx::query_scalar("SELECT fields FROM rxnorm_semantic_types WHERE code_system_id=$1 ORDER BY source_row").bind(id).fetch_all(&pool).await.unwrap();
            assert_eq!(fields.len(), fixture.semantic_types.len());
            for (actual, row) in fields.iter().zip(&fixture.semantic_types) {
                for (column, value) in ["RXCUI", "TUI", "STN", "STY", "ATUI", "CVF"]
                    .into_iter()
                    .zip(row)
                {
                    assert_eq!(actual[column], *value);
                }
            }
        }
        if sat {
            let fields: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(
                "SELECT fields FROM rxnorm_attributes WHERE code_system_id=$1 ORDER BY source_row",
            )
            .bind(id)
            .fetch_all(&pool)
            .await
            .unwrap();
            assert_eq!(
                fields.len(),
                fixture.attributes.len(),
                "Duplicate SAT assertions are preserved as source rows"
            );
            for (actual, row) in fields.iter().zip(&fixture.attributes) {
                for (column, value) in [
                    "RXCUI", "LUI", "SUI", "RXAUI", "STYPE", "CODE", "ATUI", "SATUI", "ATN", "SAB",
                    "ATV", "SUPPRESS", "CVF",
                ]
                .into_iter()
                .zip(row)
                {
                    assert_eq!(actual[column], *value);
                }
            }
            for code in ["1001", "1002"] {
                assert!(!sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM concepts c CROSS JOIN LATERAL jsonb_array_elements(c.properties) p WHERE c.code_system_id=$1 AND c.code=$2 AND p->>'code'='NDC')").bind(id).bind(code).fetch_one(&pool).await.unwrap());
            }
        }
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_auxiliary_identity_preserves_legacy_and_forbids_same_edition_enrichment() {
    let (admin, pool, schema) = database().await;
    let mut fixture = AuxiliaryFixture::new("03022026");
    fixture.save(false, false);
    let expected = json!({"format":"rxnorm-rrf-v1","projection":"current-active-v1","version":"03022026","fullMonthly":true,"sourceSha256":fixture.base.manifest["source"]["sha256"],"expectedRows":fixture.base.rows.len(),"expectedConcepts":2});
    let legacy = import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap();
    assert_eq!(
        legacy.fingerprint,
        hex::encode(Sha256::digest(serde_json::to_vec(&expected).unwrap()))
    );
    assert!(legacy.auxiliary_sources.is_none());
    fixture.base.manifest["semanticTypes"] = Value::Null;
    fixture.base.manifest["attributes"] = Value::Null;
    fixture.base.save_manifest();
    assert!(
        import_manifest(&pool, &fixture.base.path(), false)
            .await
            .unwrap()
            .unchanged
    );
    fixture.save(true, true);
    let initial = generation(&pool).await;
    assert!(import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap_err()
        .to_string()
        .contains("Conflicting immutable"));
    assert_eq!(generation(&pool).await, initial);
    let mut next = AuxiliaryFixture::new("04062026");
    let cohort = import_manifest(&pool, &next.base.path(), false)
        .await
        .unwrap();
    assert!(
        import_manifest(&pool, &next.base.path(), false)
            .await
            .unwrap()
            .unchanged
    );
    std::fs::create_dir(next.base.root.join("moved")).unwrap();
    for (key, file) in [
        ("semanticTypes", "RXNSTY.RRF"),
        ("attributes", "RXNSAT.RRF"),
    ] {
        std::fs::copy(
            next.base.root.join(file),
            next.base.root.join("moved").join(file),
        )
        .unwrap();
        next.base.manifest[key]["path"] = format!("moved/{file}").into();
    }
    next.base.save_manifest();
    assert_eq!(
        import_manifest(&pool, &next.base.path(), false)
            .await
            .unwrap()
            .fingerprint,
        cohort.fingerprint
    );
    next.save(false, true);
    assert!(import_manifest(&pool, &next.base.path(), false)
        .await
        .unwrap_err()
        .to_string()
        .contains("Conflicting immutable"));
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_auxiliary_bad_sources_attachments_and_curated_ndcs_publish_nothing() {
    let (admin, pool, schema) = database().await;
    let initial = generation(&pool).await;
    for variant in 0..25 {
        let mut fixture = AuxiliaryFixture::new("03022026");
        match variant {
            0 => {
                fixture.base.manifest["semanticTypes"]["sha256"] = "0".repeat(64).into();
                fixture.base.save_manifest();
            }
            1 => {
                fixture.base.manifest["attributes"]["expectedRows"] = 99.into();
                fixture.base.save_manifest();
            }
            2 => {
                fixture
                    .semantic_types
                    .push(fixture.semantic_types[0].clone());
                fixture.save(true, true);
            }
            3 => {
                fixture.semantic_types[1][3] = "Conflicting TUI label".into();
                fixture.save(true, true);
            }
            4 => {
                fixture.semantic_types.pop();
                fixture.semantic_types.remove(5);
                fixture.save(true, true);
            }
            5 => {
                fixture.semantic_types[0][0] = "9999".into();
                fixture.save(true, true);
            }
            6 => {
                fixture.attributes[0][3] = "9999".into();
                fixture.save(true, true);
            }
            7 => {
                fixture.attributes[0][0] = "1001".into();
                fixture.save(true, true);
            }
            8 => {
                fixture.attributes[5][0] = "9999".into();
                fixture.save(true, true);
            }
            9 => {
                fixture.attributes[0][10] = "1234567890".into();
                fixture.save(true, true);
            }
            10 => {
                fixture.attributes[0][3] = "4".into();
                fixture.save(true, true);
            }
            11 => {
                fixture.attributes[0][4] = "RUI".into();
                fixture.save(true, true);
            }
            12 => {
                fixture.attributes[0][11] = "E".into();
                fixture.save(true, true);
            }
            13 => {
                fixture.attributes[0][0] = "1001".into();
                fixture.attributes[0][3] = "5".into();
                fixture.save(true, true);
            }
            14 => {
                fixture.base.rows[0][16] = "O".into();
                fixture.base.rows[1][12] = "SCD".into();
                fixture.base.rows[1][14] = "Replacement active product name".into();
                fixture.base.save_source();
                fixture.save(true, true);
            }
            15 => fixture.replace("semanticTypes", b"\xff|\n"),
            16 => fixture.replace("attributes", "x".repeat(1024 * 1024 + 1)),
            17 => fixture.replace("semanticTypes", "1000|T121|A1|Label||\n"),
            18 => {
                fixture.semantic_types[0][2] = "x".repeat(256);
                fixture.save(true, true);
            }
            19 => {
                fixture.attributes[0][10] = "123\0bad".into();
                fixture.save(true, true);
            }
            20 => {
                fixture.base.rows[4][12] = "SCD".into();
                fixture.base.save_source();
                fixture.attributes.push(AuxiliaryFixture::attribute(
                    "1001",
                    "5",
                    "AUI",
                    "NDC",
                    "RXNORM",
                    "00012345678",
                    "N",
                ));
                fixture.save(true, true);
            }
            21 => {
                fixture.attributes[0][4] = "CUI".into();
                fixture.attributes[0][3] = "".into();
                fixture.save(true, true);
            }
            22 => {
                fixture.attributes[0].push("extra".into());
                fixture.save(true, true);
            }
            23 => {
                fixture.semantic_types[0][1] = "not-a-tui".into();
                fixture.save(true, true);
            }
            _ => {
                for row in &mut fixture.semantic_types {
                    if row[1] == "T121" {
                        row[2] = "A1..2".into();
                    }
                }
                fixture.save(true, true);
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
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_auxiliary_empty_sat_and_lossless_crlf_sources_remain_distinct_from_missing() {
    let (admin, pool, schema) = database().await;
    let mut fixture = AuxiliaryFixture::new("03022026");
    fixture.attributes.clear();
    fixture.save(true, true);
    let content = std::fs::read_to_string(fixture.base.root.join("RXNSTY.RRF"))
        .unwrap()
        .replace('\n', "\r\n");
    fixture.replace(
        "semanticTypes",
        format!("\u{feff}{}", content.trim_end_matches(['\r', '\n'])),
    );
    let imported = import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap();
    let id = imported.code_system_id.unwrap();
    assert_eq!(
        imported.auxiliary_sources.as_ref().unwrap()["RXNSAT.RRF"]["rows"],
        0
    );
    let present: bool =
        sqlx::query_scalar("SELECT attributes_loaded FROM rxnorm_releases WHERE code_system_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(present);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM rxnorm_attributes WHERE code_system_id=$1"
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_auxiliary_dry_run_publication_failure_and_cancellation_roll_back_every_source() {
    let (admin, pool, schema) = database().await;
    let fixture = AuxiliaryFixture::new("03022026");
    let initial = generation(&pool).await;
    let dry = import_manifest(&pool, &fixture.base.path(), true)
        .await
        .unwrap();
    assert!(dry.code_system_id.is_none());
    assert!(dry.auxiliary_sources.is_some());
    assert_eq!(inventory(&pool).await, 0);
    assert_eq!(generation(&pool).await, initial);
    sqlx::raw_sql("CREATE FUNCTION fail_auxiliary() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected auxiliary publication failure'; END $$; CREATE TRIGGER fail_auxiliary BEFORE INSERT ON rxnorm_releases FOR EACH ROW EXECUTE FUNCTION fail_auxiliary()").execute(&pool).await.unwrap();
    assert!(import_manifest(&pool, &fixture.base.path(), false)
        .await
        .is_err());
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT (SELECT count(*) FROM rxnorm_atoms)+(SELECT count(*) FROM rxnorm_semantic_types)+(SELECT count(*) FROM rxnorm_attributes)").fetch_one(&pool).await.unwrap(),0);
    assert_eq!(generation(&pool).await, initial);
    sqlx::query("DROP TRIGGER fail_auxiliary ON rxnorm_releases")
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
async fn rxnorm_auxiliary_combined_projection_budget_rejects_before_publication() {
    let (admin, pool, schema) = database().await;
    let mut fixture = AuxiliaryFixture::new("03022026");
    for row in &mut fixture.semantic_types {
        if row[1] == "T121" {
            row[3] = "x".repeat(600_000);
        }
    }
    fixture.save(true, true);
    let initial = generation(&pool).await;
    assert!(import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap_err()
        .to_string()
        .contains("Combined RxNorm property projection"));
    assert_eq!(inventory(&pool).await, 0);
    assert_eq!(generation(&pool).await, initial);
    cleanup(admin, pool, schema).await;
}
