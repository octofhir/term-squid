//! Original generated fixtures: no licensed clinical data or upstream benchmark pools.
use super::*;
use serde_json::{json, Value};

struct Fixture {
    root: PathBuf,
    manifest: Value,
}
impl Fixture {
    fn new(release: &str) -> Self {
        let root = std::env::temp_dir().join(format!("term-squid-rf2-test-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let concepts = format!(
            "id\teffectiveTime\tactive\tmoduleId\tdefinitionStatusId\r\n{}",
            (1..=5)
                .map(|n| format!(
                    "10000{n}\t20260101\t{}\t900000000000207008\t900000000000074008\r\n",
                    if n == 5 { 0 } else { 1 }
                ))
                .collect::<String>()
        );
        let descriptions=format!("id\teffectiveTime\tactive\tmoduleId\tconceptId\tlanguageCode\ttypeId\tterm\tcaseSignificanceId\n{}{}",
            (1..=5).map(|n|format!("20000{n}\t20260101\t{}\t900000000000207008\t10000{n}\ten\t900000000000003001\tFixture FSN {n}\t900000000000448009\n",if n==5{0}else{1})).collect::<String>(),
            [("210002","Preferred \\ \"fixture\" μ"),("220002","Other dialect fixture"),("230002","Acceptable fixture")].iter().map(|(id,term)|format!("{id}\t20260101\t1\t900000000000207008\t100002\ten\t900000000000013009\t{term}\t900000000000448009\n")).collect::<String>());
        let relationships=format!("id\teffectiveTime\tactive\tmoduleId\tsourceId\tdestinationId\trelationshipGroup\ttypeId\tcharacteristicTypeId\tmodifierId\n{}",
            [("300001","1","100002","100001","0","116680003"),("300002","1","100003","100002","0","116680003"),("300003","1","100004","100001","1","363698007"),("300004","0","100005","100001","0","116680003")].iter().map(|(id,active,source,dest,group,kind)|format!("{id}\t20260101\t{active}\t900000000000207008\t{source}\t{dest}\t{group}\t{kind}\t900000000000011006\t900000000000451002\n")).collect::<String>());
        let language=format!("id\teffectiveTime\tactive\tmoduleId\trefsetId\treferencedComponentId\tacceptabilityId\n{}",
            [(1,"900000000000509007","210002","900000000000548007"),(2,"900000000000508004","220002","900000000000548007"),(3,"900000000000509007","230002","900000000000549004")].iter().map(|(n,dialect,description,accept)|format!("{}\t20260101\t1\t900000000000207008\t{dialect}\t{description}\t{accept}\n",Uuid::from_u128(*n))).collect::<String>());
        let mut sources = serde_json::Map::new();
        for (name, content) in [
            ("concepts", concepts),
            ("descriptions", descriptions),
            ("relationships", relationships),
            ("language", language),
        ] {
            let file = format!("{name}_Snapshot_{release}.txt");
            std::fs::write(root.join(&file), &content).unwrap();
            sources.insert(
                name.into(),
                json!({"path":file,"sha256":format!("{:x}",Sha256::digest(content.as_bytes()))}),
            );
        }
        let fixture = Self {
            root,
            manifest: json!({"format":"snomed-rf2-snapshot-v1","completeSnapshot":true,"editionModule":"900000000000207008","release":release,"dialect":"900000000000509007","language":"en","sources":sources}),
        };
        fixture.save();
        fixture
    }
    fn path(&self) -> PathBuf {
        self.root.join("manifest.json")
    }
    fn source(&self, kind: &str) -> PathBuf {
        self.root
            .join(self.manifest["sources"][kind]["path"].as_str().unwrap())
    }
    fn save(&self) {
        std::fs::write(
            self.path(),
            serde_json::to_vec_pretty(&self.manifest).unwrap(),
        )
        .unwrap();
    }
    fn alter(&mut self, kind: &str, f: impl FnOnce(String) -> String) {
        let file = self.source(kind);
        let content = f(std::fs::read_to_string(&file).unwrap());
        std::fs::write(file, &content).unwrap();
        self.manifest["sources"][kind]["sha256"] =
            format!("{:x}", Sha256::digest(content.as_bytes())).into();
        self.save();
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
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    let path = format!("SET search_path TO {schema},public");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .after_connect(move |c, _| {
            let path = path.clone();
            Box::pin(async move {
                sqlx::query(&path).execute(c).await?;
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
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
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
async fn rf2_manifest_rejects_unattested_or_invalid_identity_before_database() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://unused:unused@127.0.0.1/unused")
        .unwrap();
    for change in [0, 1, 2, 3, 4, 5, 6] {
        let mut fixture = Fixture::new("20260201");
        match change {
            0 => fixture.manifest["completeSnapshot"] = false.into(),
            1 => fixture.manifest["format"] = "other".into(),
            2 => fixture.manifest["release"] = "20260231".into(),
            3 => fixture.manifest["sources"]["concepts"]["sha256"] = "wrong".into(),
            4 => fixture.manifest["editionModule"] = "bad".into(),
            5 => {
                fixture.manifest["sources"]["concepts"]["path"] =
                    "Concept_Delta_20260201.txt".into()
            }
            _ => {
                fixture.manifest["sources"]["concepts"]["path"] =
                    "Concept_Snapshot_20250101.txt".into()
            }
        }
        fixture.save();
        assert!(import_manifest(&pool, &fixture.path(), false)
            .await
            .is_err());
    }
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rf2_import_preserves_projection_terms_roles_provenance_and_versions() {
    let (admin, pool, schema) = database().await;
    let mut fixture = Fixture::new("20260201");
    let initial = generation(&pool).await;
    let dry = import_manifest(&pool, &fixture.path(), true).await.unwrap();
    assert!(dry.code_system_id.is_none());
    assert_eq!(generation(&pool).await, initial);
    let result = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    let id = result.code_system_id.unwrap();
    assert_eq!(result.sources["concepts"].rows, 5);
    assert_eq!(result.sources["descriptions"].rows, 8);
    let display: String = sqlx::query_scalar(
        "SELECT display FROM concepts WHERE code_system_id=$1 AND code='100002'",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(display, "Preferred \\ \"fixture\" μ");
    let props: sqlx::types::Json<Value> = sqlx::query_scalar(
        "SELECT properties FROM concepts WHERE code_system_id=$1 AND code='100005'",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(props[0]["valueBoolean"], true);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM snomed_descriptions WHERE code_system_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 8);
    let role:(i32,String)=sqlx::query_as("SELECT relationship_group,type_id FROM snomed_relationships WHERE code_system_id=$1 AND id='300003'").bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(role, (1, "363698007".into()));
    let content: sqlx::types::Json<Value> =
        sqlx::query_scalar("SELECT content FROM code_systems WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(content["content"], "not-present");
    assert!(content.get("concept").is_none());
    assert_eq!(
        content["version"],
        "http://snomed.info/sct/900000000000207008/version/20260201"
    );
    let provenance: sqlx::types::Json<Value> =
        sqlx::query_scalar("SELECT provenance FROM snomed_releases WHERE code_system_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        provenance["sources"]["concepts"]["sha256"],
        fixture.manifest["sources"]["concepts"]["sha256"]
    );
    let current = generation(&pool).await;
    let again = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    assert!(again.unchanged);
    assert_eq!(again.code_system_id, Some(id));
    assert_eq!(generation(&pool).await, current);
    fixture.manifest["dialect"] = "900000000000508004".into();
    fixture.save();
    let conflict = import_manifest(&pool, &fixture.path(), false)
        .await
        .err()
        .unwrap();
    assert!(conflict.to_string().contains("Conflicting immutable"));
    assert_eq!(generation(&pool).await, current);
    let second = Fixture::new("20260301");
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
    assert!(
        sqlx::raw_sql(include_str!(
            "../../../backend/migrations/20261103000001_snomed_rf2.down.sql"
        ))
        .execute(&pool)
        .await
        .is_err(),
        "Downgrade must not silently remove native hierarchy data"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM snomed_releases")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rf2_malformed_sources_and_inconsistent_graphs_roll_back() {
    let (admin, pool, schema) = database().await;
    let initial = generation(&pool).await;
    for variant in 0..16 {
        let mut fixture = Fixture::new("20260201");
        match variant {
            0=>{std::fs::write(fixture.source("concepts"),"wrong header\n").unwrap();},
            1=>{let mut bytes=std::fs::read(fixture.source("descriptions")).unwrap();bytes.push(0xff);std::fs::write(fixture.source("descriptions"),bytes).unwrap();},
            2=>fixture.alter("concepts",|s|s.replace("20260101","20260301")),
            3=>fixture.alter("concepts",|s|s.replace("20260101","")),
            4=>fixture.alter("concepts",|s|{let row=s.lines().nth(1).unwrap().to_string();s+&row+"\n"}),
            5=>fixture.alter("descriptions",|s|s.replace("\t100003\t","\t999999\t")),
            6=>fixture.alter("relationships",|s|s.replace("\t100001\t0\t","\t999999\t0\t")),
            7=>fixture.alter("relationships",|s|s+"300099\t20260101\t1\t900000000000207008\t100001\t100003\t0\t116680003\t900000000000011006\t900000000000451002\n"),
            8=>fixture.alter("language",|s|s.replace("\t900000000000549004","\t900000000000548007")),
            9=>fixture.alter("language",|s|s.replace("\t210002\t","\t999999\t")),
            10=>fixture.alter("descriptions",|s|s.replace("Fixture FSN 1",&"x".repeat(70_000))),
            11=>fixture.alter("relationships",|s|s.replace("900000000000011006","900000000000010007")),
            12=>fixture.alter("concepts",|s|s.replace("\t1\t","\t3\t")),
            14=>{
                fixture.alter("concepts", |s|s+&(6..=75).map(|n|format!("{}\t20260101\t1\t900000000000207008\t900000000000074008\n",100000+n)).collect::<String>());
                fixture.alter("descriptions", |s|s+&(6..=75).map(|n|format!("{}\t20260101\t1\t900000000000207008\t{}\ten\t900000000000003001\tDeep fixture {n}\t900000000000448009\n",200000+n,100000+n)).collect::<String>());
                fixture.alter("relationships", |s|s+&(6..=75).map(|n|format!("{}\t20260101\t1\t900000000000207008\t{}\t{}\t0\t116680003\t900000000000011006\t900000000000451002\n",300000+n,100000+n,if n==6{100001}else{100000+n-1})).collect::<String>());
            },
            15=>{fixture.manifest["editionModule"]="999999".into();fixture.save();},
            _=>{fixture.manifest["sources"]["concepts"]["sha256"]="0".repeat(64).into();fixture.save();},
        }
        let error = import_manifest(&pool, &fixture.path(), false)
            .await
            .expect_err("Invalid release must fail");
        if variant == 14 {
            assert!(error.to_string().contains("64"), "{error:#}");
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM code_systems")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0,
            "variant {variant}"
        );
        assert_eq!(generation(&pool).await, initial);
    }
    // Fail after metadata, concepts, descriptions and relationships were inserted.
    sqlx::query("CREATE FUNCTION refuse_rf2_member() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected publication failure'; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER refuse_rf2_member BEFORE INSERT ON snomed_language_members FOR EACH STATEMENT EXECUTE FUNCTION refuse_rf2_member()").execute(&pool).await.unwrap();
    let fixture = Fixture::new("20260201");
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
    assert_eq!(generation(&pool).await, initial);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rf2_cancelled_import_does_not_activate_partial_release() {
    let (admin, pool, schema) = database().await;
    let fixture = Fixture::new("20260201");
    let initial = generation(&pool).await;
    let mut locker = pool.begin().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *locker)
        .await
        .unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('term-squid/rf2:'||current_schema()||':'||$1,0))").bind("http://snomed.info/sct/900000000000207008/version/20260201").execute(&mut *locker).await.unwrap();
    let other = pool.clone();
    let path = fixture.path();
    let importing = tokio::spawn(async move { import_manifest(&other, &path, false).await });
    tokio::time::timeout(std::time::Duration::from_secs(3),async{
        loop{
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks blocked JOIN pg_locks held ON blocked.locktype=held.locktype AND blocked.database=held.database AND blocked.classid=held.classid AND blocked.objid=held.objid AND blocked.objsubid=held.objsubid WHERE held.pid=$1 AND held.granted AND NOT blocked.granted)").bind(pid).fetch_one(&admin).await.unwrap();
            if waiting{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;
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
    let retry = import_manifest(&pool, &fixture.path(), false)
        .await
        .unwrap();
    assert!(retry.code_system_id.is_some());
    cleanup(admin, pool, schema).await;
}
