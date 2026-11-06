//! Original diagnostic fixtures verify complete coded axes without loading a Part corpus.
use super::*;
use term_squid_core::loinc::PrimaryAxis;

const LINK_COLUMNS: &[&str] = &[
    "LoincNumber",
    "LongCommonName",
    "PartNumber",
    "PartName",
    "PartCodeSystem",
    "PartTypeName",
    "LinkTypeName",
    "Property",
];
struct LinkFixture {
    base: Fixture,
    links: Vec<Vec<String>>,
}
impl LinkFixture {
    fn new(version: &str) -> Self {
        let mut base = Fixture::new(version);
        base.rows[0][6] = "Generated method".into();
        base.save_source();
        let mut links = Vec::new();
        for row in &base.rows {
            for (index, axis) in PrimaryAxis::all().into_iter().enumerate() {
                let name = &row[index + 1];
                if name.is_empty() {
                    continue;
                }
                links.push(vec![
                    row[0].clone(),
                    row[11].clone(),
                    format!("LP{}-1", 1000 + index),
                    name.clone(),
                    "http://loinc.org".into(),
                    axis.part_type().into(),
                    "Primary".into(),
                    axis.uri().into(),
                ]);
            }
        }
        let radiology = vec![
            base.rows[0][0].clone(),
            base.rows[0][11].clone(),
            "LP4000-1".into(),
            "Raw, \"μ\"\r\nsecond line \\".into(),
            "http://loinc.org".into(),
            "Rad.View.View type".into(),
            "Radiology".into(),
            "http://loinc.org/property/rad-view-view-type".into(),
        ];
        links.push(radiology.clone());
        links.push(radiology);
        links.push(vec![
            base.rows[0][0].clone(),
            "Raw document".into(),
            "LP5000-1".into(),
            "Raw document kind".into(),
            "http://loinc.org".into(),
            "Document.Kind".into(),
            "DocumentOntology".into(),
            "http://loinc.org/property/document-kind".into(),
        ]);
        links.push(vec![
            base.rows[0][0].clone(),
            "Raw foreign".into(),
            "opaque-source-code".into(),
            "Raw foreign name".into(),
            "https://example.org/source".into(),
            "Foreign".into(),
            "Primary".into(),
            "http://loinc.org/property/COMPONENT".into(),
        ]);
        let mut result = Self { base, links };
        result.save();
        result
    }
    fn source(&self) -> PathBuf {
        self.base.root.join("LoincPartLink_Primary.csv")
    }
    fn content(&self) -> Vec<u8> {
        let mut csv = csv::WriterBuilder::new()
            .terminator(csv::Terminator::CRLF)
            .from_writer(Vec::new());
        csv.write_record(LINK_COLUMNS).unwrap();
        for row in &self.links {
            csv.write_record(row).unwrap();
        }
        csv.into_inner().unwrap()
    }
    fn replace(&mut self, bytes: impl AsRef<[u8]>) {
        std::fs::write(self.source(), bytes.as_ref()).unwrap();
        self.base.manifest["primaryLinks"] = json!({"path":"LoincPartLink_Primary.csv",
            "sha256":hex::encode(Sha256::digest(bytes.as_ref())),"expectedRows":self.links.len(),
            "version":self.base.manifest["version"],"completeFile":true});
        self.base.save_manifest();
    }
    fn save(&mut self) {
        self.replace(self.content());
    }
    fn absent(&mut self) {
        self.base
            .manifest
            .as_object_mut()
            .unwrap()
            .remove("primaryLinks");
        self.base.save_manifest();
    }
}
async fn inventory(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT (SELECT count(*) FROM code_systems)+(SELECT count(*) FROM concepts)+(SELECT count(*) FROM loinc_terms)+(SELECT count(*) FROM loinc_releases)+(SELECT count(*) FROM loinc_primary_links)").fetch_one(pool).await.unwrap()
}

#[tokio::test]
async fn loinc_primary_link_manifest_requires_complete_same_edition_bounded_official_source() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://unused:unused@127.0.0.1/unused")
        .unwrap();
    for variant in 0..9 {
        let mut fixture = LinkFixture::new("2.82");
        match variant {
            0 => fixture.base.manifest["primaryLinks"]["version"] = "2.81".into(),
            1 => fixture.base.manifest["primaryLinks"]["completeFile"] = false.into(),
            2 => fixture.base.manifest["primaryLinks"]["expectedRows"] = 0.into(),
            3 => fixture.base.manifest["primaryLinks"]["expectedRows"] = 20_000_001.into(),
            4 => {
                fixture.base.manifest["primaryLinks"]["path"] =
                    "LoincPartLink_Supplementary.csv".into()
            }
            5 => fixture.base.manifest["primaryLinks"]["sha256"] = "A".repeat(64).into(),
            6 => fixture.base.manifest["primaryLinks"]["expectedRows"] = (-1).into(),
            7 => fixture.base.manifest["primaryLinks"]["unexpected"] = true.into(),
            _ => fixture.base.manifest["primaryLinks"]["completeFile"] = "true".into(),
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
async fn loinc_primary_links_preserve_raw_rows_and_project_typed_axes_for_all_lifecycle_states() {
    let (admin, pool, schema) = database().await;
    let legacy = Fixture::new("2.81");
    let old = import_manifest(&pool, &legacy.path(), false).await.unwrap();
    assert!(old.primary_link_source.is_none());
    let fixture = LinkFixture::new("2.82");
    let imported = import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap();
    let id = imported.code_system_id.unwrap();
    let (loaded,source):(bool,sqlx::types::Json<Value>)=sqlx::query_as("SELECT primary_links_loaded,primary_link_source FROM loinc_releases WHERE code_system_id=$1").bind(id).fetch_one(&pool).await.unwrap();
    assert!(loaded);
    assert_eq!(source.as_object().unwrap().len(), 6);
    assert_eq!(source["file"], "LoincPartLink_Primary.csv");
    assert_eq!(source["rows"], fixture.links.len());
    assert_eq!(
        source["bytes"],
        std::fs::metadata(fixture.source()).unwrap().len()
    );
    assert_eq!(
        source["sha256"],
        fixture.base.manifest["primaryLinks"]["sha256"]
    );
    assert_eq!(source["version"], "2.82");
    assert_eq!(source["completeFile"], true);
    let metadata: sqlx::types::Json<Value> =
        sqlx::query_scalar("SELECT content FROM code_systems WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(metadata["content"], "not-present");
    for axis in PrimaryAxis::all() {
        let property = metadata["property"]
            .as_array()
            .unwrap()
            .iter()
            .find(|property| property["code"] == axis.code())
            .unwrap();
        assert_eq!(property["type"], "code");
        assert_eq!(property["uri"], axis.uri());
    }
    let raw:Vec<(i64,sqlx::types::Json<Value>)>=sqlx::query_as("SELECT source_row,fields FROM loinc_primary_links WHERE code_system_id=$1 ORDER BY source_row").bind(id).fetch_all(&pool).await.unwrap();
    assert_eq!(raw.len(), fixture.links.len());
    for (index, ((ordinal, fields), row)) in raw.iter().zip(&fixture.links).enumerate() {
        assert_eq!(*ordinal, index as i64 + 1);
        for (name, value) in LINK_COLUMNS.iter().zip(row) {
            assert_eq!(fields[*name], *value);
        }
    }
    assert_eq!(raw[21].1, raw[22].1);
    for (index, row) in fixture.base.rows.iter().enumerate() {
        let properties: sqlx::types::Json<Value> = sqlx::query_scalar(
            "SELECT properties FROM concepts WHERE code_system_id=$1 AND code=$2",
        )
        .bind(id)
        .bind(&row[0])
        .fetch_one(&pool)
        .await
        .unwrap();
        let properties = properties.as_array().unwrap();
        for (axis_index, axis) in PrimaryAxis::all().into_iter().enumerate() {
            let coded = properties
                .iter()
                .find(|property| property["code"] == axis.code());
            if axis == PrimaryAxis::Method && index != 0 {
                assert!(coded.is_none());
            } else {
                assert_eq!(
                    coded.unwrap()["valueCode"],
                    format!("LP{}-1", 1000 + axis_index)
                );
            }
        }
        assert_eq!(
            properties
                .iter()
                .find(|p| p["code"] == "raw-COMPONENT")
                .unwrap()["valueString"],
            row[1]
        );
        assert_eq!(
            properties.iter().find(|p| p["code"] == "inactive").unwrap()["valueBoolean"],
            index == 3
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM concepts WHERE code LIKE 'LP%' OR code LIKE 'LA%'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM closure_table")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let absent:(bool,sqlx::types::Json<Value>)=sqlx::query_as("SELECT primary_links_loaded,primary_link_source FROM loinc_releases WHERE code_system_id=$1").bind(old.code_system_id).fetch_one(&pool).await.unwrap();
    assert!(!absent.0);
    assert_eq!(absent.1 .0, json!({}));
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_primary_link_legacy_identity_nulls_paths_and_immutable_cohorts_are_preserved() {
    let (admin, pool, schema) = database().await;
    let mut fixture = LinkFixture::new("2.82");
    let descriptor = fixture.base.manifest["primaryLinks"].clone();
    fixture.absent();
    let expected = json!({"format":"loinc-csv-v1","projection":"primary-table-v1","version":"2.82","sourceSha256":fixture.base.manifest["source"]["sha256"],"expectedConcepts":4});
    let expected = hex::encode(Sha256::digest(serde_json::to_vec(&expected).unwrap()));
    let legacy = import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap();
    assert_eq!(legacy.fingerprint, expected);
    let provenance: sqlx::types::Json<Value> =
        sqlx::query_scalar("SELECT provenance FROM loinc_releases WHERE code_system_id=$1")
            .bind(legacy.code_system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(provenance.as_object().unwrap().len(), 5);
    assert!(provenance.get("primaryLinkSource").is_none());
    fixture.base.manifest["primaryLinks"] = Value::Null;
    fixture.base.save_manifest();
    let unchanged = import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap();
    assert!(unchanged.unchanged);
    assert_eq!(unchanged.fingerprint, expected);
    fixture.base.manifest["primaryLinks"] = descriptor;
    fixture.base.save_manifest();
    assert!(import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap_err()
        .to_string()
        .contains("Conflicting immutable"));
    let mut next = LinkFixture::new("2.83");
    let imported = import_manifest(&pool, &next.base.path(), false)
        .await
        .unwrap();
    let (stamp,generation_before):(String,i64)=sqlx::query_as("SELECT updated_at::text,(SELECT generation FROM terminology_generation WHERE singleton) FROM code_systems WHERE id=$1").bind(imported.code_system_id).fetch_one(&pool).await.unwrap();
    std::fs::create_dir(next.base.root.join("moved")).unwrap();
    std::fs::copy(
        next.source(),
        next.base.root.join("moved/LoincPartLink_Primary.csv"),
    )
    .unwrap();
    next.base.manifest["primaryLinks"]["path"] = "moved/LoincPartLink_Primary.csv".into();
    next.base.save_manifest();
    let unchanged = import_manifest(&pool, &next.base.path(), false)
        .await
        .unwrap();
    assert!(unchanged.unchanged);
    assert_eq!(unchanged.fingerprint, imported.fingerprint);
    assert_eq!(generation(&pool).await, generation_before);
    let current: String =
        sqlx::query_scalar("SELECT updated_at::text FROM code_systems WHERE id=$1")
            .bind(imported.code_system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stamp, current);
    next.links.push(next.links[21].clone());
    next.save();
    assert!(import_manifest(&pool, &next.base.path(), false)
        .await
        .unwrap_err()
        .to_string()
        .contains("Conflicting immutable"));
    next.absent();
    assert!(import_manifest(&pool, &next.base.path(), false)
        .await
        .unwrap_err()
        .to_string()
        .contains("Conflicting immutable"));
    assert_eq!(generation(&pool).await, generation_before);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_primary_link_invalid_references_axes_labels_and_coverage_publish_nothing() {
    let (admin, pool, schema) = database().await;
    let initial = generation(&pool).await;
    for variant in 0..15 {
        let mut fixture = LinkFixture::new("2.82");
        match variant {
            0 => fixture.links[0][0] = "99999-1".into(),
            1 => fixture.links[21][0] = "99999-1".into(),
            2 => {
                fixture.links.remove(0);
            }
            3 => {
                fixture.links.push(fixture.links[0].clone());
            }
            4 => {
                fixture.links.remove(5);
            }
            5 => {
                let mut method = fixture.links[5].clone();
                method[0] = fixture.base.rows[1][0].clone();
                fixture.links.push(method);
            }
            6 => fixture.links[0][3] = "Wrong axis label".into(),
            7 => fixture.links[0][5] = "TIME".into(),
            8 => fixture.links[0][7] = "http://loinc.org/property/analyte-core".into(),
            9 => fixture.links[0][2] = "LP-not-a-code".into(),
            10 => fixture.links[0][2] = "lp1000-1".into(),
            11 => fixture.links[0][4] = "https://example.org/foreign".into(),
            12 => fixture.links[0][2] = "LP1001-1".into(),
            13 => {
                fixture
                    .links
                    .retain(|row| row[0] != fixture.base.rows[3][0]);
            }
            _ => {
                fixture.base.rows[1][6] = "Unlinked method".into();
                fixture.base.save_source();
            }
        }
        fixture.save();
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
async fn loinc_primary_link_strict_csv_hash_counts_record_and_combined_property_budgets_are_atomic()
{
    let (admin, pool, schema) = database().await;
    let initial = generation(&pool).await;
    for variant in 0..13 {
        let mut fixture = LinkFixture::new("2.82");
        match variant {
            0 => {
                fixture.base.manifest["primaryLinks"]["sha256"] = "0".repeat(64).into();
                fixture.base.save_manifest();
            }
            1 => {
                fixture.base.manifest["primaryLinks"]["expectedRows"] = 999.into();
                fixture.base.save_manifest();
            }
            2 => fixture.replace(b"\xff,broken\n"),
            3 => fixture.replace("name,\"unterminated"),
            4 => fixture.replace("\"name\"extra,broken\n"),
            5 => fixture.replace("x".repeat(1024 * 1024 + 1)),
            6 => {
                let mut content = String::from_utf8(fixture.content()).unwrap();
                content = content.replacen("LoincNumber", "WrongHeader", 1);
                fixture.replace(content);
            }
            7 => {
                fixture.links[0][0] = "bad\0code".into();
                fixture.save();
            }
            8 => {
                fixture.links[21][7] = "x".repeat(256);
                fixture.save();
            }
            9 => {
                fixture.links[0].push("extra".into());
                let mut csv = csv::WriterBuilder::new()
                    .flexible(true)
                    .from_writer(Vec::new());
                csv.write_record(LINK_COLUMNS).unwrap();
                for row in &fixture.links {
                    csv.write_record(row).unwrap();
                }
                fixture.replace(csv.into_inner().unwrap());
            }
            10 => {
                fixture.replace(LINK_COLUMNS.join(",") + "\n");
            }
            11 => {
                fixture.base.manifest["source"]["sha256"] = "0".repeat(64).into();
                fixture.base.save_manifest();
            }
            _ => {
                let label = "x".repeat(530_000);
                for row in &mut fixture.base.rows {
                    row[1] = label.clone();
                }
                fixture.base.save_source();
                for row in &mut fixture.links {
                    if row[6] == "Primary" && row[4] == "http://loinc.org" && row[5] == "COMPONENT"
                    {
                        row[3] = label.clone();
                    }
                }
                fixture.save();
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
    let mut fixture = LinkFixture::new("2.82");
    let content = String::from_utf8(fixture.content()).unwrap();
    fixture.replace(format!(
        "\u{feff}{}",
        content.trim_end_matches(['\r', '\n'])
    ));
    let imported = import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap();
    assert_eq!(
        imported.primary_link_source.unwrap()["bytes"],
        std::fs::metadata(fixture.source()).unwrap().len()
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_primary_link_dry_run_publication_failure_and_cancellation_leave_no_partial_cohort() {
    let (admin, pool, schema) = database().await;
    let fixture = LinkFixture::new("2.82");
    let initial = generation(&pool).await;
    let dry = import_manifest(&pool, &fixture.base.path(), true)
        .await
        .unwrap();
    assert!(dry.code_system_id.is_none());
    assert!(dry.primary_link_source.is_some());
    assert_eq!(inventory(&pool).await, 0);
    assert_eq!(generation(&pool).await, initial);
    sqlx::raw_sql("CREATE FUNCTION fail_primary_link() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected final publication failure'; END $$; CREATE TRIGGER fail_primary_link BEFORE INSERT ON loinc_releases FOR EACH ROW EXECUTE FUNCTION fail_primary_link()").execute(&pool).await.unwrap();
    assert!(import_manifest(&pool, &fixture.base.path(), false)
        .await
        .is_err());
    assert_eq!(inventory(&pool).await, 0);
    assert_eq!(generation(&pool).await, initial);
    sqlx::raw_sql(
        "DROP TRIGGER fail_primary_link ON loinc_releases; DROP FUNCTION fail_primary_link()",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut locker = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE code_systems IN SHARE MODE")
        .execute(&mut *locker)
        .await
        .unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *locker)
        .await
        .unwrap();
    let path = fixture.base.path();
    let cloned = pool.clone();
    let importing = tokio::spawn(async move { import_manifest(&cloned, &path, false).await });
    tokio::time::timeout(std::time::Duration::from_secs(5),async{loop{let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks blocked JOIN pg_locks held ON blocked.locktype=held.locktype AND blocked.database=held.database AND blocked.relation=held.relation WHERE held.pid=$1 AND held.granted AND NOT blocked.granted)").bind(pid).fetch_one(&admin).await.unwrap();if waiting{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();
    importing.abort();
    assert!(importing.await.unwrap_err().is_cancelled());
    locker.rollback().await.unwrap();
    assert_eq!(inventory(&pool).await, 0);
    assert_eq!(generation(&pool).await, initial);
    assert!(import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap()
        .code_system_id
        .is_some());
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_primary_link_concurrent_activation_and_source_generation_are_transactional() {
    let (admin, pool, schema) = database().await;
    let fixture = LinkFixture::new("2.82");
    let path = fixture.base.path();
    let (left, right) = tokio::join!(
        import_manifest(&pool, &path, false),
        import_manifest(&pool, &path, false)
    );
    let left = left.unwrap();
    let right = right.unwrap();
    assert_eq!(left.code_system_id, right.code_system_id);
    assert_ne!(left.unchanged, right.unchanged);
    let id = left.code_system_id.unwrap();
    let current = generation(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE loinc_primary_links SET part_name=part_name||' changed' WHERE code_system_id=$1 AND source_row=22").bind(id).execute(&mut *tx).await.unwrap();
    let in_tx: i64 =
        sqlx::query_scalar("SELECT generation FROM terminology_generation WHERE singleton")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(in_tx > current);
    tx.rollback().await.unwrap();
    assert_eq!(generation(&pool).await, current);
    sqlx::query("UPDATE loinc_primary_links SET part_name=part_name||' committed' WHERE code_system_id=$1 AND source_row=22").bind(id).execute(&pool).await.unwrap();
    assert!(generation(&pool).await > current);
    for invalid in [
        json!({}),
        json!({"file":"wrong.csv","sha256":"0".repeat(64),"rows":1,"bytes":1,"version":"2.82","completeFile":true}),
        json!({"file":"LoincPartLink_Primary.csv","sha256":"0".repeat(64),"rows":0,"bytes":1,"version":"2.82","completeFile":true}),
        json!({"file":"LoincPartLink_Primary.csv","sha256":"0".repeat(64),"rows":1,"bytes":1,"version":"2.81","completeFile":true}),
    ] {
        assert!(sqlx::query(
            "UPDATE loinc_releases SET primary_link_source=$2 WHERE code_system_id=$1"
        )
        .bind(id)
        .bind(sqlx::types::Json(invalid))
        .execute(&pool)
        .await
        .is_err());
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_primary_link_downgrade_preserves_legacy_and_refuses_loaded_or_raw_content() {
    let (admin, pool, schema) = database().await;
    let fixture = LinkFixture::new("2.82");
    let imported = import_manifest(&pool, &fixture.base.path(), false)
        .await
        .unwrap();
    let down =
        include_str!("../../../backend/migrations/20261106000003_loinc_primary_links.down.sql");
    let mut tx = pool.begin().await.unwrap();
    assert!(sqlx::raw_sql(down).execute(&mut *tx).await.is_err());
    tx.rollback().await.unwrap();
    sqlx::query("DELETE FROM loinc_primary_links")
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(sqlx::raw_sql(down).execute(&mut *tx).await.is_err());
    tx.rollback().await.unwrap();
    sqlx::query("DELETE FROM code_systems WHERE id=$1")
        .bind(imported.code_system_id)
        .execute(&pool)
        .await
        .unwrap();
    let legacy = Fixture::new("2.81");
    let old = import_manifest(&pool, &legacy.path(), false).await.unwrap();
    sqlx::query("INSERT INTO loinc_primary_links(code_system_id,source_row,loinc_number,long_common_name,part_number,part_name,part_code_system,part_type_name,link_type_name,property,fields) VALUES($1,1,'1000-1','','external','','external','','Other','', '{}'::jsonb)").bind(old.code_system_id).execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(sqlx::raw_sql(down).execute(&mut *tx).await.is_err());
    tx.rollback().await.unwrap();
    sqlx::query("DELETE FROM loinc_primary_links")
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(down).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM code_systems")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    let raw: bool = sqlx::query_scalar("SELECT to_regclass('loinc_primary_links') IS NOT NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!raw);
    let columns:i64=sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='loinc_releases' AND column_name IN ('primary_links_loaded','primary_link_source')").fetch_one(&pool).await.unwrap();
    assert_eq!(columns, 0);
    cleanup(admin, pool, schema).await;
}
