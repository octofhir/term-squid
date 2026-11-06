//! Generated RF2 source rows test native text search, not clinical release coverage.
use super::*;
use crate::store::{ValueSetOperation, ValueSetRequest, ValueSetResult};
use term_squid_core::snomed::{INTERNATIONAL, SYSTEM};

const VERSION: &str = "http://snomed.info/sct/900000000000207008/version/20260201";
const OLD: &str = "http://snomed.info/sct/900000000000207008/version/20260101";
const DIALECT: &str = "900000000000509007";
const OTHER_DIALECT: &str = "900000000000508004";

async fn release(pool: &PgPool, version: &str, date: &str, dialect: &str) -> Uuid {
    let id = native_test_system(pool, version).await;
    sqlx::query("INSERT INTO snomed_releases VALUES($1,$2,$3,$4,'en',repeat('a',64),'{\"generatedFixture\":true}')")
        .bind(id).bind(INTERNATIONAL)
        .bind(chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap())
        .bind(dialect).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) SELECT $1,n::text,'Projection '||n,'[]'::jsonb FROM generate_series(1,12)n")
        .bind(id).execute(pool).await.unwrap();
    id
}

async fn synonym(pool: &PgPool, system: Uuid, code: &str, id: &str, term: &str) {
    sqlx::query("INSERT INTO snomed_descriptions VALUES($1,$2,'2026-02-01',true,'1',$3,'en','900000000000013009',$4,'900000000000448009')")
        .bind(system).bind(id).bind(code).bind(term).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO snomed_language_members VALUES($1,$2,'2026-02-01',true,'1',$3,$4,'900000000000549004')")
        .bind(system).bind(Uuid::new_v4()).bind(DIALECT).bind(id).execute(pool).await.unwrap();
}

fn whole(version: Option<&str>) -> Value {
    json!({"resourceType":"ValueSet","compose":{"include":[{"system":SYSTEM,"version":version}]}})
}

async fn expand(app: &axum::Router, resource: Value, filter: &str, extra: Vec<Value>) -> Value {
    let mut parameters = vec![json!({"name":"filter","valueString":filter})];
    parameters.extend(extra);
    let (status, body) = request(
        app,
        "POST",
        "/r4/ValueSet/$expand",
        inline_parameters(resource, parameters),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    body
}

fn assert_codes(body: &Value, expected: &[&str], total: usize) {
    let members = body["expansion"]["contains"].as_array().unwrap();
    assert_eq!(body["expansion"]["total"], total, "{body}");
    assert_eq!(
        members
            .iter()
            .map(|member| member["code"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected,
        "{body}"
    );
}

async fn cleanup(admin: PgPool, pool: PgPool, schema: String) {
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_search_uses_only_active_accepted_synonyms_in_release_context() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-02-01", DIALECT).await;
    for (code, description, term) in [
        ("1", "preferred", "Needle preferred synonym"),
        ("1", "duplicate", "Needle second synonym"),
        ("2", "acceptable", "Needle acceptable synonym"),
        ("3", "inactive-description", "Needle inactive-description"),
        ("4", "inactive-member", "Needle inactive-member"),
        ("5", "foreign-dialect", "Needle foreign-dialect"),
        ("6", "foreign-language", "Needle foreign-language"),
        ("7", "fsn", "Needle fsn-only"),
        ("8", "unknown-acceptability", "Needle unknown-acceptability"),
        ("9", "unaccepted", "Needle unaccepted"),
    ] {
        synonym(&pool, id, code, description, term).await;
    }
    sqlx::query("UPDATE snomed_language_members SET acceptability_id='900000000000548007' WHERE description_id='preferred'")
        .execute(&pool).await.unwrap();
    sqlx::query("UPDATE snomed_descriptions SET active=false WHERE id='inactive-description'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE snomed_language_members SET active=false WHERE description_id='inactive-member'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE snomed_language_members SET refset_id=$1 WHERE description_id='foreign-dialect'",
    )
    .bind(OTHER_DIALECT)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE snomed_descriptions SET language_code='es' WHERE id='foreign-language'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE snomed_descriptions SET type_id='900000000000003001' WHERE id='fsn'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE snomed_language_members SET acceptability_id='unknown' WHERE description_id='unknown-acceptability'")
        .execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM snomed_language_members WHERE description_id='unaccepted'")
        .execute(&pool)
        .await
        .unwrap();
    // Duplicate active language memberships must not multiply the matching concept.
    sqlx::query("INSERT INTO snomed_language_members SELECT code_system_id,$1,effective_time,active,module_id,refset_id,description_id,acceptability_id FROM snomed_language_members WHERE description_id='preferred'")
        .bind(Uuid::new_v4()).execute(&pool).await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let body = expand(&app, whole(Some(VERSION)), "NeEdLe", vec![]).await;
    assert_codes(&body, &["1", "2"], 2);
    for member in body["expansion"]["contains"].as_array().unwrap() {
        assert_eq!(member["system"], SYSTEM);
        assert_eq!(member["version"], VERSION);
        assert_eq!(
            member["display"],
            format!("Projection {}", member["code"].as_str().unwrap())
        );
    }
    for query in [
        "inactive-description",
        "inactive-member",
        "foreign-dialect",
        "foreign-language",
        "fsn-only",
        "unknown-acceptability",
        "unaccepted",
    ] {
        assert_codes(
            &expand(&app, whole(Some(VERSION)), query, vec![]).await,
            &[],
            0,
        );
    }
    // Source synonyms extend filtering without replacing existing code/display matching.
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "Projection 10", vec![]).await,
        &["10"],
        1,
    );
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "12", vec![]).await,
        &["12"],
        1,
    );
    let (status, unfiltered) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        inline_parameters(whole(Some(VERSION)), vec![]),
    )
    .await;
    assert_eq!(status, 200, "{unfiltered}");
    assert_codes(
        &unfiltered,
        &[
            "1", "10", "11", "12", "2", "3", "4", "5", "6", "7", "8", "9",
        ],
        12,
    );
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        inline_parameters(
            whole(Some(VERSION)),
            vec![json!({"name":"filter","valueString":""})],
        ),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    let uncached = api::create_router(Arc::new(PostgresStore::without_prepared_cache(
        pool.clone(),
    )));
    for filter in [
        "needle",
        "inactive-member",
        "foreign-dialect",
        "Projection 10",
    ] {
        let retained = expand(&app, whole(Some(VERSION)), filter, vec![]).await;
        let unretained = expand(&uncached, whole(Some(VERSION)), filter, vec![]).await;
        assert_eq!(
            retained["expansion"]["contains"],
            unretained["expansion"]["contains"]
        );
        assert_eq!(
            retained["expansion"]["total"],
            unretained["expansion"]["total"]
        );
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_search_treats_wildcards_backslashes_and_sql_text_literally() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-02-01", DIALECT).await;
    for (code, description, term) in [
        ("1", "percent", "Dose 20% literal"),
        ("2", "underscore", "Part_a literal"),
        ("3", "backslash", "Path\\a literal"),
        ("4", "sql", "' OR true -- literal"),
        ("5", "decoy", "Dose 200 PartXa Path/a literal"),
    ] {
        synonym(&pool, id, code, description, term).await;
    }
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (filter, code) in [("%", "1"), ("_", "2"), ("\\", "3"), ("' OR true --", "4")] {
        assert_codes(
            &expand(&app, whole(Some(VERSION)), filter, vec![]).await,
            &[code],
            1,
        );
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_search_preserves_composition_displays_totals_and_pages() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-02-01", DIALECT).await;
    for code in ["1", "2", "3", "4", "5"] {
        synonym(
            &pool,
            id,
            code,
            &format!("search-{code}"),
            "Searchable alternate",
        )
        .await;
    }
    synonym(&pool, id, "2", "duplicate", "Another searchable alternate").await;
    insert_set(&pool, Some("a"), json!({"include":[{"system":SYSTEM,"version":VERSION,"concept":[{"code":"1"},{"code":"2"},{"code":"3"},{"code":"4"}]}]})).await;
    insert_set(&pool, Some("b"), json!({"include":[{"system":SYSTEM,"version":VERSION,"concept":[{"code":"2"},{"code":"3"},{"code":"5"}]}]})).await;
    let root = insert_set(
        &pool,
        Some("search-root"),
        json!({"include":[{"valueSet":["urn:set|a","urn:set|b"]}]}),
    )
    .await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let direct = store
        .query_value_set(
            ValueSetRequest {
                url: "urn:set",
                version: Some("search-root"),
                id: None,
                inline: None,
                system_context: None,
            },
            ValueSetOperation::Expand {
                filter: Some("searchable"),
                offset: 0,
                count: 10,
            },
        )
        .await
        .unwrap();
    let ValueSetResult::Native(page) = direct else {
        panic!("RF2 search must retain native dispatch")
    };
    assert_eq!(page.total, 2);
    assert_eq!(
        page.members
            .iter()
            .map(|m| m.code.as_str())
            .collect::<Vec<_>>(),
        ["2", "3"]
    );
    let app = api::create_router(store);
    for path in [
        format!("/r4/ValueSet/{root}/$expand?filter=searchable"),
        "/r4/ValueSet/$expand?url=urn:set%7Csearch-root&filter=searchable".into(),
    ] {
        let (status, body) = request(&app, "GET", &path, Value::Null).await;
        assert_eq!(status, 200, "{body}");
        assert_codes(&body, &["2", "3"], 2);
    }
    let resource = json!({"resourceType":"ValueSet","compose":{"include":[{"valueSet":["urn:set|a","urn:set|b"]},{"system":SYSTEM,"version":VERSION,"concept":[{"code":"5","display":"Explicit override"}]}],"exclude":[{"system":SYSTEM,"version":VERSION,"concept":[{"code":"2"}]}]}});
    let body = expand(&app, resource.clone(), "searchable", vec![]).await;
    assert_codes(&body, &["3", "5"], 2);
    assert_eq!(
        body["expansion"]["contains"][1]["display"],
        "Explicit override"
    );
    for (offset, count, expected) in [
        (0, 1, vec!["3"]),
        (1, 1, vec!["5"]),
        (2, 1, vec![]),
        (0, 0, vec![]),
        (i64::MAX, 10, vec![]),
    ] {
        let body = expand(
            &app,
            resource.clone(),
            "searchable",
            vec![
                json!({"name":"offset","valueInteger":offset}),
                json!({"name":"count","valueInteger":count}),
            ],
        )
        .await;
        assert_codes(&body, &expected, 2);
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_search_uses_selected_edition_language_and_dialect_without_cross_release_terms() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let old = release(&pool, OLD, "2026-01-01", OTHER_DIALECT).await;
    let current = release(&pool, VERSION, "2026-02-01", DIALECT).await;
    synonym(&pool, old, "2", "shared-description", "Legacy search term").await;
    sqlx::query("UPDATE snomed_language_members SET refset_id=$1 WHERE code_system_id=$2")
        .bind(OTHER_DIALECT)
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
    synonym(
        &pool,
        current,
        "2",
        "shared-description",
        "Fresh search term",
    )
    .await;
    // Keep latest ordinary selection explicit and independent of insertion clock resolution.
    sqlx::query("UPDATE code_systems SET updated_at='2099-01-01' WHERE id=$1")
        .bind(current)
        .execute(&pool)
        .await
        .unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (version, filter, expected) in [
        (Some(OLD), "legacy", vec!["2"]),
        (Some(OLD), "fresh", vec![]),
        (Some(VERSION), "legacy", vec![]),
        (Some(VERSION), "fresh", vec!["2"]),
        (None, "legacy", vec![]),
        (None, "fresh", vec!["2"]),
    ] {
        let body = expand(&app, whole(version), filter, vec![]).await;
        assert_codes(&body, &expected, expected.len());
        if !expected.is_empty() {
            assert_eq!(
                body["expansion"]["contains"][0]["version"],
                version.unwrap_or(VERSION)
            );
        }
    }
    let (status, body) = request(&app, "POST", "/r4/ValueSet/$expand", json!({"resourceType":"Parameters","parameter":[{"name":"url","valueUri":format!("{OLD}?fhir_vs")},{"name":"filter","valueString":"legacy"}]})).await;
    assert_eq!(status, 200, "{body}");
    assert_codes(&body, &["2"], 1);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_search_observes_commits_but_not_uncommitted_or_rolled_back_source_changes() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-02-01", DIALECT).await;
    synonym(&pool, id, "1", "mutable-description", "Before mutation").await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "before", vec![]).await,
        &["1"],
        1,
    );
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "after", vec![]).await,
        &[],
        0,
    );
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE snomed_descriptions SET term='After mutation' WHERE id='mutable-description'",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "before", vec![]).await,
        &["1"],
        1,
    );
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "after", vec![]).await,
        &[],
        0,
    );
    tx.rollback().await.unwrap();
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "before", vec![]).await,
        &["1"],
        1,
    );
    sqlx::query(
        "UPDATE snomed_descriptions SET term='After mutation' WHERE id='mutable-description'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "before", vec![]).await,
        &[],
        0,
    );
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "after", vec![]).await,
        &["1"],
        1,
    );
    sqlx::query("UPDATE snomed_language_members SET active=false WHERE description_id='mutable-description'")
        .execute(&pool).await.unwrap();
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "after", vec![]).await,
        &[],
        0,
    );
    sqlx::query(
        "UPDATE snomed_language_members SET active=true WHERE description_id='mutable-description'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "after", vec![]).await,
        &["1"],
        1,
    );
    sqlx::query("UPDATE snomed_descriptions SET active=false WHERE id='mutable-description'")
        .execute(&pool)
        .await
        .unwrap();
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "after", vec![]).await,
        &[],
        0,
    );
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE snomed_descriptions SET active=true WHERE id='mutable-description'")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "after", vec![]).await,
        &[],
        0,
    );
    // Projection display is unaffected by alternate-source edits and remains searchable.
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "Projection 1", vec![]).await,
        &["1", "10", "11", "12"],
        4,
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_search_keeps_active_and_display_language_options_separate_from_synonym_scope() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-02-01", DIALECT).await;
    synonym(&pool, id, "1", "english", "Searchable English alternate").await;
    synonym(
        &pool,
        id,
        "2",
        "inactive-concept",
        "Searchable inactive concept alternate",
    )
    .await;
    synonym(
        &pool,
        id,
        "1",
        "00-translated-display",
        "Translated display",
    )
    .await;
    synonym(
        &pool,
        id,
        "1",
        "zz-translated-alternate",
        "Spanish-only alternate",
    )
    .await;
    synonym(&pool, id, "10", "unicode", "İSTANBUL alternate").await;
    sqlx::query("UPDATE snomed_descriptions SET language_code='es' WHERE id IN ('00-translated-display','zz-translated-alternate')")
        .execute(&pool).await.unwrap();
    sqlx::query("UPDATE concepts SET properties='[{\"code\":\"inactive\",\"valueBoolean\":true}]'::jsonb WHERE code_system_id=$1 AND code='2'")
        .bind(id).execute(&pool).await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "searchable", vec![]).await,
        &["1", "2"],
        2,
    );
    assert_codes(
        &expand(
            &app,
            whole(Some(VERSION)),
            "searchable",
            vec![json!({"name":"activeOnly","valueBoolean":true})],
        )
        .await,
        &["1"],
        1,
    );
    let spanish = vec![json!({"name":"displayLanguage","valueCode":"es"})];
    let body = expand(
        &app,
        whole(Some(VERSION)),
        "English alternate",
        spanish.clone(),
    )
    .await;
    assert_codes(&body, &["1"], 1);
    assert_eq!(
        body["expansion"]["contains"][0]["display"],
        "Translated display"
    );
    assert_codes(
        &expand(
            &app,
            whole(Some(VERSION)),
            "Spanish-only alternate",
            spanish.clone(),
        )
        .await,
        &[],
        0,
    );
    // The resolved display still participates independently of the selected-dialect synonyms.
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "Translated display", spanish).await,
        &["1"],
        1,
    );
    // Both source text and query use the same database case mapping, even for dotted I.
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "İSTANBUL", vec![]).await,
        &["10"],
        1,
    );
    let (status, metadata) =
        request(&app, "GET", "/r4/metadata?mode=terminology", Value::Null).await;
    assert_eq!(status, 200, "{metadata}");
    assert_eq!(metadata["resourceType"], "TerminologyCapabilities");
    let policy = metadata["expansion"]["textFilter"].as_str().unwrap();
    for documented in [
        "active preferred/acceptable synonyms",
        "selected release language and dialect",
        "displayLanguage does not change that synonym scope",
        "Punctuation is significant and wildcards have no special meaning",
        "Native text comparisons use PostgreSQL case mapping",
        "finite text comparisons use Rust Unicode lowercase",
    ] {
        assert!(
            policy.contains(documented),
            "Missing {documented}: {policy}"
        );
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_search_reads_fresh_release_language_and_dialect_without_rollback_leakage() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-02-01", DIALECT).await;
    for (code, description, term) in [
        ("1", "en-us", "Context probe English US"),
        ("2", "es-us", "Context probe Spanish US"),
        ("3", "en-gb", "Context probe English GB"),
        ("4", "es-gb", "Context probe Spanish GB"),
    ] {
        synonym(&pool, id, code, description, term).await;
    }
    sqlx::query("UPDATE snomed_descriptions SET language_code='es' WHERE id IN ('es-us','es-gb')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE snomed_language_members SET refset_id=$1 WHERE description_id IN ('en-gb','es-gb')",
    )
    .bind(OTHER_DIALECT)
    .execute(&pool)
    .await
    .unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "context probe", vec![]).await,
        &["1"],
        1,
    );
    // Direct fixture mutations exercise source-snapshot freshness; ordinary imports
    // retain immutable release context and do not mutate these fields in place.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE snomed_releases SET language='es',dialect=$1 WHERE code_system_id=$2")
        .bind(OTHER_DIALECT)
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "context probe", vec![]).await,
        &["1"],
        1,
    );
    tx.rollback().await.unwrap();
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "context probe", vec![]).await,
        &["1"],
        1,
    );
    sqlx::query("UPDATE snomed_releases SET language='es' WHERE code_system_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "context probe", vec![]).await,
        &["2"],
        1,
    );
    sqlx::query("UPDATE snomed_releases SET dialect=$1 WHERE code_system_id=$2")
        .bind(OTHER_DIALECT)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "context probe", vec![]).await,
        &["4"],
        1,
    );
    sqlx::query("UPDATE snomed_releases SET language='en' WHERE code_system_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_codes(
        &expand(&app, whole(Some(VERSION)), "context probe", vec![]).await,
        &["3"],
        1,
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_search_loinc_uses_literal_database_case_mapping_without_rf2_synonym_bleed() {
    const LOINC: &str = "http://loinc.org";
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider) VALUES($1,$2,'2.82','active','Generated LOINC search fixture',$3,'loinc-csv')")
        .bind(id).bind(LOINC).bind(sqlx::types::Json(json!({"resourceType":"CodeSystem","url":LOINC,"version":"2.82","content":"not-present","caseSensitive":false})))
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO loinc_releases VALUES($1,'2.82',repeat('a',64),'{\"generatedFixture\":true,\"scope\":\"Loinc.csv\"}')")
        .bind(id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,'1000-1','İSTANBUL 20%_\\ primary','[]'),($1,'1001-9','Ordinary primary','[]')")
        .bind(id).execute(&pool).await.unwrap();
    // Inject foreign-provider rows to test the explicit native_provider guard,
    // beyond the source-shape invariants enforced by the production importers.
    sqlx::query("INSERT INTO snomed_releases VALUES($1,$2,'2026-02-01',$3,'en',repeat('a',64),'{\"generatedFixture\":true}')")
        .bind(id).bind(INTERNATIONAL).bind(DIALECT).execute(&pool).await.unwrap();
    synonym(
        &pool,
        id,
        "1001-9",
        "foreign-provider",
        "RF2-only alternate",
    )
    .await;
    let snomed = release(&pool, VERSION, "2026-02-01", DIALECT).await;
    synonym(&pool, snomed, "1", "real-rf2", "RF2-only alternate").await;
    let resource = json!({"resourceType":"ValueSet","compose":{"include":[{"system":LOINC,"version":"2.82","concept":[{"code":"1000-1"},{"code":"1001-9"}]}]}});
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for filter in ["İSTANBUL", "%", "_", "\\", "1000-1"] {
        let body = expand(&app, resource.clone(), filter, vec![]).await;
        assert_codes(&body, &["1000-1"], 1);
        assert_eq!(body["expansion"]["contains"][0]["system"], LOINC);
        assert_eq!(body["expansion"]["contains"][0]["version"], "2.82");
        assert_eq!(
            body["expansion"]["contains"][0]["display"],
            "İSTANBUL 20%_\\ primary"
        );
    }
    assert_codes(
        &expand(&app, resource.clone(), "Ordinary primary", vec![]).await,
        &["1001-9"],
        1,
    );
    assert_codes(
        &expand(&app, resource.clone(), "RF2-only alternate", vec![]).await,
        &[],
        0,
    );
    let mut mixed = resource;
    mixed["compose"]["include"]
        .as_array_mut()
        .unwrap()
        .push(json!({"system":SYSTEM,"version":VERSION,"concept":[{"code":"1"}]}));
    let body = expand(&app, mixed, "RF2-only alternate", vec![]).await;
    assert_codes(&body, &["1"], 1);
    assert_eq!(body["expansion"]["contains"][0]["system"], SYSTEM);
    cleanup(admin, pool, schema).await;
}
