use super::*;
use flate2::{write::GzEncoder, Compression};
use serde_json::json;
use std::{
    fs::File,
    io::{self, Read},
    path::PathBuf,
};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn archive(entries: Vec<(&str, Vec<u8>, tar::EntryType)>) -> Fixture {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.benchmarks/package-upload-tests");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("{}.tgz", uuid::Uuid::new_v4()));
    let mut archive = tar::Builder::new(GzEncoder::new(
        File::create(&path).unwrap(),
        Compression::fast(),
    ));
    for (name, bytes, kind) in entries {
        let mut header = tar::Header::new_gnu();
        // Raw header permits deliberately unsafe path fixtures that Builder rejects.
        header.as_mut_bytes()[..100].fill(0);
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        header.set_entry_type(kind);
        header.set_mode(0o600);
        header.set_size(bytes.len() as u64);
        if kind.is_symlink() {
            header.set_link_name("/tmp/outside").unwrap();
        }
        header.set_cksum();
        archive.append(&header, bytes.as_slice()).unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap();
    Fixture(path)
}
fn metadata() -> Vec<u8> {
    serde_json::to_vec(
        &json!({"name":"org.example.fixture","version":"1.0.0","fhirVersions":["4.0.1"]}),
    )
    .unwrap()
}
fn resource() -> Value {
    json!({"resourceType":"CodeSystem","url":"urn:package:fixture","version":"1","status":"active","content":"complete","concept":[{"code":"a"}]})
}
fn entries() -> Vec<(&'static str, Vec<u8>, tar::EntryType)> {
    vec![
        ("package/package.json", metadata(), tar::EntryType::Regular),
        (
            "package/CodeSystem.json",
            serde_json::to_vec(&resource()).unwrap(),
            tar::EntryType::Regular,
        ),
    ]
}

#[test]
fn package_inspection_requires_explicit_r4_and_records_real_inventory() {
    let fixture = archive(entries());
    let parsed = inspect_upload(&fixture.0, UploadFormat::Package).unwrap();
    assert_eq!(parsed.inspection["resources"].as_array().unwrap().len(), 1);
    assert_eq!(parsed.inspection["records"], 1);
    assert_eq!(parsed.resources[0].0, resource());
    for versions in [json!(["5.0.0"]), json!(["4.0.1", "5.0.0"]), Value::Null] {
        let mut data = entries();
        data[0].1 =
            serde_json::to_vec(&json!({"name":"fixture","version":"1","fhirVersions":versions}))
                .unwrap();
        assert!(inspect_upload(&archive(data).0, UploadFormat::Package).is_err());
    }
}
#[test]
fn archive_rejects_traversal_absolute_paths_links_and_duplicate_members() {
    for (name, kind) in [
        ("../outside.json", tar::EntryType::Regular),
        ("/package/outside.json", tar::EntryType::Regular),
        ("package/../outside.json", tar::EntryType::Regular),
        ("package/link", tar::EntryType::Symlink),
        ("package/link", tar::EntryType::Link),
        ("other/resource.json", tar::EntryType::Regular),
    ] {
        let mut data = entries();
        data.push((name, Vec::new(), kind));
        assert!(
            inspect_upload(&archive(data).0, UploadFormat::Package).is_err(),
            "{name} {kind:?}"
        );
    }
    let mut data = entries();
    data.push(("package/package.json", metadata(), tar::EntryType::Regular));
    assert!(inspect_upload(&archive(data).0, UploadFormat::Package).is_err());
}
#[test]
fn malformed_empty_bundle_and_duplicate_canonicals_are_rejected() {
    for bytes in [
        b"malformed".to_vec(),
        serde_json::to_vec(&json!({"resourceType":"Bundle","entry":[]})).unwrap(),
        serde_json::to_vec(&json!({"resourceType":"CodeSystem","url":"urn:broken"})).unwrap(),
    ] {
        let mut data = entries();
        data[1].1 = bytes;
        assert!(inspect_upload(&archive(data).0, UploadFormat::Package).is_err());
    }
    let mut data = entries();
    data.push((
        "package/duplicate.json",
        serde_json::to_vec(&resource()).unwrap(),
        tar::EntryType::Regular,
    ));
    assert!(inspect_upload(&archive(data).0, UploadFormat::Package).is_err());
}
#[test]
fn archive_enforces_member_and_resource_limits() {
    let mut data = entries();
    data[1].1 = vec![b' '; MAX_JSON_BYTES + 1];
    assert!(inspect_upload(&archive(data).0, UploadFormat::Package)
        .unwrap_err_message()
        .contains("member limit"));
    let mut data = entries();
    let names: Vec<String> = (0..package::MAX_PACKAGE_RESOURCES)
        .map(|n| format!("package/resource-{n}.json"))
        .collect();
    for name in &names {
        data.push((
            name,
            serde_json::to_vec(&resource()).unwrap(),
            tar::EntryType::Regular,
        ));
    }
    assert!(inspect_upload(&archive(data).0, UploadFormat::Package)
        .unwrap_err_message()
        .contains("resource limit"));
}
#[test]
fn ignored_archive_bytes_and_gzip_trailers_are_budgeted_and_verified() {
    let fixture = archive(entries());
    let file = std::fs::OpenOptions::new()
        .append(true)
        .open(&fixture.0)
        .unwrap();
    let mut encoder = GzEncoder::new(file, Compression::fast());
    io::copy(
        &mut io::repeat(0).take(package::MAX_PACKAGE_DECOMPRESSED),
        &mut encoder,
    )
    .unwrap();
    encoder.finish().unwrap();
    assert!(inspect_upload(&fixture.0, UploadFormat::Package)
        .unwrap_err_message()
        .contains("decompression limit"));
    let fixture = archive(entries());
    let mut bytes = std::fs::read(&fixture.0).unwrap();
    let last = bytes.len() - 5;
    bytes[last] ^= 0xff;
    std::fs::write(&fixture.0, bytes).unwrap();
    assert!(inspect_upload(&fixture.0, UploadFormat::Package).is_err());
}
#[test]
fn package_aggregate_record_limit_prevents_many_small_resources_bypassing_budget() {
    let names: Vec<String> = (0..3).map(|n| format!("package/codes-{n}.json")).collect();
    let mut data = vec![("package/package.json", metadata(), tar::EntryType::Regular)];
    for (index, name) in names.iter().enumerate() {
        let mut content = resource();
        content["url"] = format!("urn:package:{index}").into();
        content["concept"] = Value::Array(
            (0..20_000)
                .map(|code| json!({"code":code.to_string()}))
                .collect(),
        );
        data.push((
            name,
            serde_json::to_vec(&content).unwrap(),
            tar::EntryType::Regular,
        ));
    }
    assert!(inspect_upload(&archive(data).0, UploadFormat::Package)
        .unwrap_err_message()
        .contains("Aggregate package record limit"));
}
// Avoid requiring Debug for potentially large successfully parsed payloads.
trait ErrorMessage {
    fn unwrap_err_message(self) -> String;
}
impl ErrorMessage for anyhow::Result<InspectedUpload> {
    fn unwrap_err_message(self) -> String {
        match self {
            Ok(_) => panic!("unexpected valid package"),
            Err(e) => e.to_string(),
        }
    }
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn supplemental_packages_publish_dependencies_atomically_despite_canonical_sort_order() {
    let url = std::env::var("TERM_SQUID_TEST_DATABASE_URL").unwrap();
    let admin = sqlx::PgPool::connect(&url).await.unwrap();
    let schema = format!("test_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let search = format!("SET search_path TO {schema}, public");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(move |c, _| {
            let search = search.clone();
            Box::pin(async move {
                sqlx::query(sqlx::AssertSqlSafe(search.as_str()))
                    .execute(c)
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
    for invalid in [false, true] {
        let base = json!({"resourceType":"CodeSystem","url":"urn:z-base","version":"1","status":"active","content":"complete","concept":[{"code":"a"}]});
        let supplement = json!({"resourceType":"CodeSystem","url":"urn:a-supplement","version":"1","status":"active","content":"supplement","supplements":"urn:z-base|1","concept":[{"code":if invalid {"unknown"}else{"a"},"designation":[{"language":"de","value":"Begriff"}]}]});
        let fixture = archive(vec![
            ("package/package.json", metadata(), tar::EntryType::Regular),
            (
                "package/a-supplement.json",
                serde_json::to_vec(&supplement).unwrap(),
                tar::EntryType::Regular,
            ),
            (
                "package/z-base.json",
                serde_json::to_vec(&base).unwrap(),
                tar::EntryType::Regular,
            ),
        ]);
        let inspected = inspect_upload(&fixture.0, UploadFormat::Package).unwrap();
        assert_eq!(inspected.resources[0].1.url, "urn:a-supplement");
        let mut tx = pool.begin().await.unwrap();
        let result = inspected.publish(&mut tx).await;
        if invalid {
            assert!(result.is_err());
            tx.rollback().await.unwrap();
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM code_systems")
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                0
            );
        } else {
            let result = result.unwrap();
            assert_eq!(result["imported"], 2);
            tx.commit().await.unwrap();
            let designations:sqlx::types::Json<Value>=sqlx::query_scalar("SELECT c.designations FROM concepts c JOIN code_systems s ON s.id=c.code_system_id WHERE s.url='urn:a-supplement'").fetch_one(&pool).await.unwrap();
            assert_eq!(designations.0[0]["value"], "Begriff");
            // Remove this disposable successful fixture so the failure path tests rollback of both new resources.
            sqlx::query("DELETE FROM code_systems")
                .execute(&pool)
                .await
                .unwrap();
        }
    }
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
