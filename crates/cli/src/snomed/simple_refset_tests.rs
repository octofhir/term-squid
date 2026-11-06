//! Generated concept-only cohorts; no licensed release files are required.
use super::*;

const HEADER: &str = "id\teffectiveTime\tactive\tmoduleId\trefsetId\treferencedComponentId\n";
const DOWN: &str =
    include_str!("../../../backend/migrations/20261106000004_snomed_simple_refsets.down.sql");

fn member(id: u128, active: bool, refset: &str, concept: &str) -> String {
    format!(
        "{}\t20260101\t{}\t900000000000207008\t{refset}\t{concept}\n",
        Uuid::from_u128(id),
        u8::from(active)
    )
}

fn rows(fixture: &mut Fixture, content: &str) {
    let file = fixture
        .root
        .join(fixture.manifest["simpleRefsets"]["path"].as_str().unwrap());
    std::fs::write(file, content).unwrap();
    fixture.manifest["simpleRefsets"]["sha256"] =
        hex::encode(Sha256::digest(content.as_bytes())).into();
    fixture.manifest["simpleRefsets"]["expectedRows"] =
        (content.lines().count().saturating_sub(1) as u64).into();
    fixture.save();
}

fn cohort(release: &str, content: &str) -> Fixture {
    let mut fixture = Fixture::new(release);
    fixture.alter("concepts", |s| {
        s + &["446609009", "700001", "700002", "700003"]
            .iter()
            .map(|id| format!("{id}\t20260101\t1\t900000000000207008\t900000000000074008\n"))
            .collect::<String>()
    });
    fixture.alter("descriptions", |s| s + &[
        ("800001", "446609009"), ("800002", "700001"), ("800003", "700002"), ("800004", "700003")
    ].iter().map(|(id,concept)| format!("{id}\t20260101\t1\t900000000000207008\t{concept}\ten\t900000000000003001\tGenerated refset {concept}\t900000000000448009\n")).collect::<String>());
    fixture.alter("relationships", |s| s + &(1..=3).map(|n|format!("90000{n}\t20260101\t1\t900000000000207008\t70000{n}\t446609009\t0\t116680003\t900000000000011006\t900000000000451002\n")).collect::<String>());
    fixture.manifest["simpleRefsets"] = json!({"path":format!("der2_Refset_SimpleSnapshot_INT_{release}.txt"),
        "sha256":"", "expectedRows":0,"release":release,"completeSnapshot":true,"conceptOnly":true,
        "refsetIds":["700001","700002"]});
    rows(&mut fixture, &(HEADER.to_owned() + content));
    fixture
}

async fn assert_unpublished(pool: &PgPool, initial: i64) {
    let counts: (i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM code_systems),(SELECT count(*) FROM snomed_releases),(SELECT count(*) FROM snomed_simple_refsets),(SELECT count(*) FROM snomed_simple_members)")
        .fetch_one(pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0, 0));
    assert_eq!(generation(pool).await, initial);
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn simple_refsets_preserve_raw_members_inactive_references_and_duplicate_concepts() {
    let (admin, pool, schema) = database().await;
    let content = member(0xabcdef, true, "700001", "100002")
        + &member(2, true, "700001", "100002")
        + &member(3, false, "700001", "100003")
        + &member(4, true, "700001", "100005");
    let mut fixture = cohort("20260201", "");
    let content = (HEADER.to_owned() + &content)
        .replace("abcdef", "ABCDEF")
        .replace('\n', "\r\n");
    rows(&mut fixture, &("\u{feff}".to_owned() + &content));
    let result = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    let id = result.code_system_id.unwrap();
    assert_eq!(result.sources["simpleRefsets"].rows, 4);
    let raw: Vec<(Uuid, bool, String, sqlx::types::Json<Value>)> = sqlx::query_as("SELECT id,active,referenced_component_id,fields FROM snomed_simple_members WHERE code_system_id=$1 ORDER BY id")
        .bind(id).fetch_all(&pool).await.unwrap();
    assert_eq!(raw.len(), 4);
    assert!(raw[0].1);
    assert!(!raw[1].1);
    assert_eq!(raw[2].2, "100005");
    assert_eq!(raw[3].0, Uuid::from_u128(0xabcdef));
    assert_eq!(
        raw[3].3 .0,
        json!({"id":"00000000-0000-0000-0000-000000ABCDEF","effectiveTime":"20260101","active":"1","moduleId":"900000000000207008","refsetId":"700001","referencedComponentId":"100002"})
    );
    let metadata: (bool, sqlx::types::Json<Value>) = sqlx::query_as("SELECT simple_refsets_loaded,simple_refset_source FROM snomed_releases WHERE code_system_id=$1")
        .bind(id).fetch_one(&pool).await.unwrap();
    assert!(metadata.0);
    assert_eq!(metadata.1["refsetIds"], json!(["700001", "700002"]));
    assert_eq!(metadata.1["rows"], 4);
    assert_eq!(metadata.1["bytes"], result.sources["simpleRefsets"].bytes);
    assert_eq!(
        metadata.1["sha256"],
        fixture.manifest["simpleRefsets"]["sha256"]
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM snomed_simple_refsets WHERE code_system_id=$1"
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
    assert!(sqlx::query_scalar::<_, bool>("SELECT snomed_simple_refset_source_valid(simple_refset_source,to_char(release_date,'YYYYMMDD')) FROM snomed_releases WHERE code_system_id=$1").bind(id).fetch_one(&pool).await.unwrap());
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn simple_refsets_loaded_empty_and_inactive_only_remain_declared() {
    let (admin, pool, schema) = database().await;
    for (release, content, expected) in [
        ("20260201", String::new(), 0),
        ("20260301", member(1, false, "700001", "100005"), 1),
    ] {
        let fixture = cohort(release, &content);
        let result = import_manifest(&pool, &fixture.path(), false)
            .await
            .unwrap();
        let id = result.code_system_id.unwrap();
        let actual: (i64, i64, i64, bool) = sqlx::query_as("SELECT (SELECT count(*) FROM snomed_simple_refsets WHERE code_system_id=$1),(SELECT count(*) FROM snomed_simple_members WHERE code_system_id=$1),(SELECT count(*) FROM snomed_simple_members WHERE code_system_id=$1 AND active),simple_refsets_loaded FROM snomed_releases WHERE code_system_id=$1")
            .bind(id).fetch_one(&pool).await.unwrap();
        assert_eq!(actual, (2, expected, 0, true));
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn simple_refsets_reject_inconsistent_source_descriptors_and_versions() {
    let (admin, pool, schema) = database().await;
    let initial = generation(&pool).await;
    for variant in 0..14 {
        let mut fixture = cohort("20260201", "");
        let descriptor = &mut fixture.manifest["simpleRefsets"];
        match variant {
            0 => descriptor["completeSnapshot"] = false.into(),
            1 => descriptor["conceptOnly"] = false.into(),
            2 => descriptor["release"] = "20260301".into(),
            3 => descriptor["path"] = "der2_Refset_SimpleSnapshot_INT_20250101.txt".into(),
            4 => descriptor["path"] = "der2_Refset_SimpleDelta_INT_20260201.txt".into(),
            5 => descriptor["sha256"] = "A".repeat(64).into(),
            6 => descriptor["expectedRows"] = 20_000_001.into(),
            7 => descriptor["refsetIds"] = json!([]),
            8 => descriptor["refsetIds"] = json!(["700002", "700001"]),
            9 => descriptor["refsetIds"] = json!(["700001", "700001"]),
            10 => descriptor["refsetIds"] = json!(["070001"]),
            11 => descriptor["refsetIds"] = json!(["12345"]),
            12 => {
                descriptor["refsetIds"] =
                    json!((700001..700066).map(|n| n.to_string()).collect::<Vec<_>>())
            }
            _ => descriptor["unexpected"] = true.into(),
        }
        fixture.save();
        assert!(
            import_manifest(&pool, &fixture.path(), false)
                .await
                .is_err(),
            "variant {variant}"
        );
        assert_unpublished(&pool, initial).await;
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn simple_refsets_reject_bad_rows_references_duplicates_and_checksums() {
    let (admin, pool, schema) = database().await;
    let initial = generation(&pool).await;
    for variant in 0..18 {
        let mut fixture = cohort("20260201", "");
        let valid = HEADER.to_owned() + &member(0xabcdef, true, "700001", "100002");
        let content = match variant {
            0 => valid.replace("referencedComponentId", "descriptionId"),
            1 => valid.replace("20260101", "20260301"),
            2 => valid.replace("20260101", "20260231"),
            3 => valid.replace("\t1\t", "\t2\t"),
            4 => valid.replace("\t100002", "\t999999"),
            5 => valid.replace("\t100002", "\t210002"),
            6 => valid.replace("\t100002", "\t300001"),
            7 => valid.replace("\t700001", "\t700003"),
            8 => {
                valid.clone()
                    + &member(0xabcdef, false, "700001", "100003").replace("abcdef", "ABCDEF")
            }
            9 => valid.replace("00000000-0000-0000-0000-000000abcdef", "not-a-uuid"),
            10 => valid.replace("\t100002", "\t010002"),
            11 => valid.replace("\t100002", "\t0"),
            12 => valid.replace("\t100002", &format!("\t{}", "1".repeat(65_536))),
            16 => String::new(),
            17 => valid.replace("\t100002\n", "\t100002\textra\n"),
            _ => valid,
        };
        rows(&mut fixture, &content);
        if variant == 13 {
            fixture.manifest["simpleRefsets"]["sha256"] = "0".repeat(64).into();
        }
        if variant == 14 {
            fixture.manifest["simpleRefsets"]["expectedRows"] = 2.into();
        }
        if variant == 15 {
            std::fs::OpenOptions::new()
                .write(true)
                .open(
                    fixture
                        .root
                        .join(fixture.manifest["simpleRefsets"]["path"].as_str().unwrap()),
                )
                .unwrap()
                .set_len(2 * 1024 * 1024 * 1024 + 1)
                .unwrap();
        }
        fixture.save();
        assert!(
            import_manifest(&pool, &fixture.path(), false)
                .await
                .is_err(),
            "variant {variant}"
        );
        assert_unpublished(&pool, initial).await;
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn simple_refsets_require_active_inferred_simple_type_proof_even_when_empty() {
    let (admin, pool, schema) = database().await;
    let initial = generation(&pool).await;
    for variant in 0..7 {
        let mut fixture = cohort("20260201", "");
        match variant {
            0 => fixture.manifest["simpleRefsets"]["refsetIds"] = json!(["799999"]),
            1 => {
                fixture.alter("relationships", |s| {
                    s.lines()
                        .filter(|line| !line.contains("\t700001\t"))
                        .collect::<Vec<_>>()
                        .join("\n")
                        + "\n"
                });
                fixture.alter("concepts", |s| {
                    s.replace("700001\t20260101\t1", "700001\t20260101\t0")
                });
            }
            2 => fixture.alter("relationships", |s| {
                s.replace("700001\t446609009", "700001\t100001")
            }),
            3 => fixture.alter("relationships", |s| {
                s.replace("900001\t20260101\t1", "900001\t20260101\t0")
            }),
            4 => fixture.alter("relationships", |s| {
                s.replace(
                    "700001\t446609009\t0\t116680003",
                    "700001\t446609009\t0\t363698007",
                )
            }),
            5 => {
                // An early path to the Simple type root must not skip the full walk guard.
                fixture.alter("concepts", |s| {
                    s + &(0..8191)
                        .map(|n| {
                            format!(
                                "{}\t20260101\t1\t900000000000207008\t900000000000074008\n",
                                1_000_000 + n
                            )
                        })
                        .collect::<String>()
                });
                fixture.alter("descriptions", |s| s + &(0..8191).map(|n|format!("{}\t20260101\t1\t900000000000207008\t{}\ten\t900000000000003001\tGenerated parent {n}\t900000000000448009\n", 2_000_000 + n, 1_000_000 + n)).collect::<String>());
                fixture.alter("relationships", |s| s + &(0..8191).map(|n|format!("{}\t20260101\t1\t900000000000207008\t700001\t{}\t0\t116680003\t900000000000011006\t900000000000451002\n", 3_000_000 + n, 1_000_000 + n)).collect::<String>());
            }
            _ => fixture.manifest["simpleRefsets"]["refsetIds"] = json!(["446609009"]),
        }
        fixture.save();
        let error = import_manifest(&pool, &fixture.path(), false)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains(match variant {
                0..=1 => "active concept",
                5 => "8192",
                6 => "proper descendant",
                _ => "inferred subtype",
            }),
            "variant {variant}: {error:#}"
        );
        assert_unpublished(&pool, initial).await;
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn simple_refsets_preserve_legacy_absent_null_fingerprint_and_immutable_first_cohort() {
    let (admin, pool, schema) = database().await;
    let mut fixture = cohort("20260201", &member(1, true, "700001", "100002"));
    let descriptor = fixture
        .manifest
        .as_object_mut()
        .unwrap()
        .remove("simpleRefsets")
        .unwrap();
    fixture.save();
    let legacy = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    let id = legacy.code_system_id.unwrap();
    let stored: (String, sqlx::types::Json<Value>, bool, sqlx::types::Json<Value>) = sqlx::query_as("SELECT fingerprint,provenance,simple_refsets_loaded,simple_refset_source FROM snomed_releases WHERE code_system_id=$1")
        .bind(id).fetch_one(&pool).await.unwrap();
    let mut identities = BTreeMap::new();
    for kind in ["concepts", "descriptions", "relationships", "language"] {
        identities.insert(kind, fixture.manifest["sources"][kind]["sha256"].clone());
    }
    let identity = json!({"format":"snomed-rf2-snapshot-v1","editionModule":"900000000000207008","release":"20260201","dialect":"900000000000509007","language":"en","sources":identities});
    assert_eq!(
        stored.0,
        hex::encode(Sha256::digest(serde_json::to_vec(&identity).unwrap()))
    );
    assert_eq!(stored.1["identity"], identity);
    assert_eq!(stored.1["scope"], "Concept, Description, inferred Relationship and Language Snapshot; no Full, Delta, OWL, concrete relationships or other refsets");
    assert!(!stored.2);
    assert_eq!(stored.3 .0, json!({}));
    let initial = generation(&pool).await;
    fixture.manifest["simpleRefsets"] = Value::Null;
    fixture.save();
    let null = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    assert!(null.unchanged);
    assert_eq!(null.fingerprint, legacy.fingerprint);
    assert_eq!(null.code_system_id, Some(id));
    fixture.manifest["simpleRefsets"] = descriptor;
    fixture.save();
    let error = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Conflicting immutable"));
    assert_eq!(generation(&pool).await, initial);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM snomed_simple_refsets")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn simple_refsets_dry_run_idempotence_new_versions_and_changed_cohorts() {
    let (admin, pool, schema) = database().await;
    let mut fixture = cohort("20260201", &member(1, true, "700001", "100002"));
    let initial = generation(&pool).await;
    let dry = import_manifest(&pool, &fixture.path(), true).await.unwrap();
    assert!(dry.dry_run && !dry.unchanged && dry.code_system_id.is_none());
    assert_eq!(dry.sources["simpleRefsets"].rows, 1);
    assert_unpublished(&pool, initial).await;
    let first = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    assert_eq!(first.fingerprint, dry.fingerprint);
    let published = generation(&pool).await;
    let same = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    assert!(same.unchanged);
    assert_eq!(same.code_system_id, first.code_system_id);
    assert_eq!(generation(&pool).await, published);
    rows(
        &mut fixture,
        &(HEADER.to_owned() + &member(1, true, "700001", "100003")),
    );
    assert!(import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap_err()
        .to_string()
        .contains("Conflicting immutable"));
    fixture
        .manifest
        .as_object_mut()
        .unwrap()
        .remove("simpleRefsets");
    fixture.save();
    assert!(import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap_err()
        .to_string()
        .contains("Conflicting immutable"));
    assert_eq!(generation(&pool).await, published);
    let next_fixture = cohort("20260301", &member(1, true, "700001", "100003"));
    let next = import_manifest(&pool, &next_fixture.path(), false)
        .await
        .unwrap();
    assert_ne!(next.code_system_id, first.code_system_id);
    assert_ne!(next.fingerprint, first.fingerprint);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM snomed_simple_members")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn simple_refsets_failure_after_raw_publication_rolls_back_everything() {
    let (admin, pool, schema) = database().await;
    let initial = generation(&pool).await;
    sqlx::query("CREATE FUNCTION refuse_simple_member() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected Simple member failure'; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER refuse_simple_member AFTER INSERT ON snomed_simple_members FOR EACH STATEMENT EXECUTE FUNCTION refuse_simple_member()").execute(&pool).await.unwrap();
    let fixture = cohort("20260201", &member(1, true, "700001", "100002"));
    let error = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("injected Simple member failure"),
        "{error:#}"
    );
    assert_unpublished(&pool, initial).await;
    sqlx::query("DROP TRIGGER refuse_simple_member ON snomed_simple_members")
        .execute(&pool)
        .await
        .unwrap();
    assert!(import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap()
        .code_system_id
        .is_some());
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn simple_refsets_cancelled_and_concurrent_imports_have_one_atomic_activation() {
    let (admin, pool, schema) = database().await;
    let fixture = cohort("20260201", &member(1, true, "700001", "100002"));
    let initial = generation(&pool).await;
    let mut lock = pool.begin().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('term-squid/rf2:'||current_schema()||':'||$1,0))").bind("http://snomed.info/sct/900000000000207008/version/20260201").execute(&mut *lock).await.unwrap();
    let task_pool = pool.clone();
    let path = fixture.path();
    let cancelled = tokio::spawn(async move { import_manifest(&task_pool, &path, false).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks blocked JOIN pg_locks held ON blocked.locktype=held.locktype AND blocked.database=held.database AND blocked.classid=held.classid AND blocked.objid=held.objid AND blocked.objsubid=held.objsubid WHERE held.pid=$1 AND held.granted AND NOT blocked.granted)").bind(pid).fetch_one(&admin).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    cancelled.abort();
    assert!(cancelled.await.unwrap_err().is_cancelled());
    lock.rollback().await.unwrap();
    assert_unpublished(&pool, initial).await;
    let path = fixture.path();
    let (first, second) = tokio::join!(
        import_manifest(&pool, &path, false),
        import_manifest(&pool, &path, false)
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.code_system_id, second.code_system_id);
    assert_ne!(first.unchanged, second.unchanged);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM code_systems")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM snomed_simple_refsets")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM snomed_simple_members")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn simple_refsets_metadata_constraints_and_safe_down_preserve_legacy_release() {
    let (admin, pool, schema) = database().await;
    let fixture = cohort("20260201", "");
    let result = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    let id = result.code_system_id.unwrap();
    let source: sqlx::types::Json<Value> = sqlx::query_scalar(
        "SELECT simple_refset_source FROM snomed_releases WHERE code_system_id=$1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    for variant in 0..10 {
        let mut invalid = source.0.clone();
        match variant {
            0 => invalid["release"] = "20260301".into(),
            1 => invalid["rows"] = (-1).into(),
            2 => invalid["bytes"] = 2_147_483_649u64.into(),
            3 => invalid["completeSnapshot"] = false.into(),
            4 => invalid["refsetIds"] = json!(["700001", "700001"]),
            5 => invalid["refsetIds"] = json!(["700002", "700001"]),
            6 => invalid["conceptOnly"] = Value::Null,
            7 => invalid["other"] = true.into(),
            8 => invalid["file"] = "nested/der2_Refset_SimpleSnapshot_INT_20260201.txt".into(),
            _ => invalid["refsetIds"] = json!(["446609009"]),
        }
        assert!(
            !sqlx::query_scalar::<_, bool>(
                "SELECT snomed_simple_refset_source_valid($1,'20260201')"
            )
            .bind(sqlx::types::Json(&invalid))
            .fetch_one(&pool)
            .await
            .unwrap(),
            "variant {variant}"
        );
        assert!(
            sqlx::query(
                "UPDATE snomed_releases SET simple_refset_source=$2 WHERE code_system_id=$1"
            )
            .bind(id)
            .bind(sqlx::types::Json(&invalid))
            .execute(&pool)
            .await
            .is_err(),
            "variant {variant}"
        );
    }
    assert!(
        sqlx::raw_sql(DOWN).execute(&pool).await.is_err(),
        "Loaded-empty cohorts cannot be silently downgraded"
    );
    sqlx::query("UPDATE snomed_releases SET simple_refsets_loaded=false,simple_refset_source='{}' WHERE code_system_id=$1").bind(id).execute(&pool).await.unwrap();
    assert!(
        sqlx::raw_sql(DOWN).execute(&pool).await.is_err(),
        "Registry rows protect against corrupted availability flags"
    );
    sqlx::query("DELETE FROM code_systems WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let legacy = Fixture::new("20260301");
    let legacy_result = import_manifest(&pool, &legacy.path(), false).await.unwrap();
    let legacy_id = legacy_result.code_system_id.unwrap();
    let before: (String, sqlx::types::Json<Value>) = sqlx::query_as(
        "SELECT fingerprint,provenance FROM snomed_releases WHERE code_system_id=$1",
    )
    .bind(legacy_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(DOWN).execute(&mut *tx).await.unwrap();
    assert!(sqlx::query_scalar::<_, Option<String>>(
        "SELECT to_regclass(current_schema()||'.snomed_simple_refsets')::text"
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap()
    .is_none());
    let after: (String, sqlx::types::Json<Value>) = sqlx::query_as(
        "SELECT fingerprint,provenance FROM snomed_releases WHERE code_system_id=$1",
    )
    .bind(legacy_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(after, before);
    tx.rollback().await.unwrap();
    cleanup(admin, pool, schema).await;
}
