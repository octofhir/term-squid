//! Generated mixed-provider fixtures preserve FHIR set semantics, not clinical coverage.
use super::*;
use crate::store::{ValueSetOperation, ValueSetRequest, ValueSetResult};
use term_squid_core::snomed::{INTERNATIONAL, SYSTEM};

const FINITE: &str = "urn:mixed:finite";
const NATIVE_VERSION: &str = "http://snomed.info/sct/900000000000207008/version/20260201";
const LOINC: &str = "http://loinc.org";
const UCUM: &str = "http://unitsofmeasure.org";

async fn publish(pool: &PgPool, source: &Value) -> Uuid {
    let inspection = term_squid_ingestion::inspect(source).unwrap();
    let mut tx = pool.begin().await.unwrap();
    let result = term_squid_ingestion::import_resource(&mut tx, source, &inspection)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    result.id
}

async fn snomed(pool: &PgPool) -> Uuid {
    let id = native_test_system(pool, NATIVE_VERSION).await;
    sqlx::query("INSERT INTO snomed_releases VALUES($1,$2,'2026-02-01','900000000000509007','en',repeat('a',64),'{\"generatedFixture\":true}')")
        .bind(id).bind(INTERNATIONAL).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,'1','Native root','[]'),($1,'2','Native child','[]'),($1,'3','Native inactive','[{\"code\":\"inactive\",\"valueBoolean\":true}]')")
        .bind(id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO snomed_relationships VALUES($1,'parent','2026-02-01',true,'1','2','1',0,'116680003','900000000000011006','900000000000451002')")
        .bind(id).execute(pool).await.unwrap();
    id
}

async fn loinc(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider) VALUES($1,$2,'2.82','active','Generated LOINC mixed fixture',$3,'loinc-csv')")
        .bind(id).bind(LOINC).bind(sqlx::types::Json(json!({"resourceType":"CodeSystem","url":LOINC,"version":"2.82","status":"active","content":"not-present","caseSensitive":false})))
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO loinc_releases VALUES($1,'2.82',repeat('a',64),'{\"generatedFixture\":true,\"scope\":\"Loinc.csv\"}')")
        .bind(id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,'1000-1','Primary LOINC','[{\"code\":\"source-table\",\"valueString\":\"Loinc.csv\"}]')")
        .bind(id).execute(pool).await.unwrap();
    id
}

fn system(version: &str, concepts: Value) -> Value {
    json!({"resourceType":"CodeSystem","url":FINITE,"version":version,"status":"active","content":"complete","caseSensitive":false,"concept":concepts})
}

fn native() -> Value {
    json!({"system":SYSTEM,"version":NATIVE_VERSION,"concept":[{"code":"2"}]})
}

fn mixed(selection: Value) -> Value {
    json!({"resourceType":"ValueSet","compose":{"include":[native(),selection]}})
}

fn parameters(mut fields: Vec<Value>, resources: &[Value]) -> Value {
    fields.extend(
        resources
            .iter()
            .map(|resource| json!({"name":"tx-resource","resource":resource})),
    );
    json!({"resourceType":"Parameters","parameter":fields})
}

async fn expand(
    app: &axum::Router,
    set: &Value,
    extra: Vec<Value>,
    resources: &[Value],
) -> (u16, Value) {
    let mut fields = vec![json!({"name":"valueSet","resource":set})];
    fields.extend(extra);
    request(
        app,
        "POST",
        "/r4/ValueSet/$expand",
        parameters(fields, resources),
    )
    .await
}

async fn validate(
    app: &axum::Router,
    set: &Value,
    system: &str,
    code: &str,
    resources: &[Value],
) -> (u16, Value) {
    request(
        app,
        "POST",
        "/r4/ValueSet/$validate-code",
        parameters(
            vec![
                json!({"name":"valueSet","resource":set}),
                json!({"name":"system","valueUri":system}),
                json!({"name":"code","valueCode":code}),
            ],
            resources,
        ),
    )
    .await
}

fn identities(body: &Value) -> Vec<(String, String, String)> {
    body["expansion"]["contains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["system"].as_str().unwrap().to_owned(),
                m["version"].as_str().unwrap_or("").to_owned(),
                m["code"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn assert_members(body: &Value, expected: &[(&str, &str, &str)], total: usize) {
    assert_eq!(body["expansion"]["total"], total, "{body}");
    assert_eq!(
        identities(body),
        expected
            .iter()
            .map(|(s, v, c)| (s.to_string(), v.to_string(), c.to_string()))
            .collect::<Vec<_>>(),
        "{body}"
    );
}

fn member<'a>(body: &'a Value, system: &str, version: &str, code: &str) -> &'a Value {
    body["expansion"]["contains"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["system"] == system && m["version"] == version && m["code"] == code)
        .unwrap()
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
async fn mixed_union_native_finite_loinc_and_explicit_ucum_preserves_identity_and_pages() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    snomed(&pool).await;
    loinc(&pool).await;
    publish(&pool, &system("1",json!([{"code":"Alpha","display":"Finite Alpha"},{"code":"Beta","display":"Finite Beta"}]))).await;
    let set = json!({"resourceType":"ValueSet","compose":{"include":[native(),{"system":FINITE,"version":"1"},{"system":LOINC,"version":"2.82","concept":[{"code":"1000-1"}]},{"system":UCUM,"concept":[{"code":"mg"}]}]}});
    let expected = [
        (LOINC, "2.82", "1000-1"),
        (SYSTEM, NATIVE_VERSION, "2"),
        (UCUM, "2.2", "mg"),
        (FINITE, "1", "Alpha"),
        (FINITE, "1", "Beta"),
    ];
    for cache in [true, false] {
        let store = Arc::new(if cache {
            PostgresStore::new(pool.clone())
        } else {
            PostgresStore::without_prepared_cache(pool.clone())
        });
        let app = api::create_router(store.clone());
        let (status, body) = expand(&app, &set, vec![], &[]).await;
        assert_eq!(status, 200, "{body}");
        assert_members(&body, &expected, 5);
        assert_eq!(member(&body, UCUM, "2.2", "mg")["display"], "milligram");
        for (offset, count, page) in [
            (0, 0, vec![]),
            (1, 2, expected[1..3].to_vec()),
            (5, 2, vec![]),
            (i64::MAX, 1, vec![]),
        ] {
            let (status, body) = expand(
                &app,
                &set,
                vec![
                    json!({"name":"offset","valueInteger":offset}),
                    json!({"name":"count","valueInteger":count}),
                ],
                &[],
            )
            .await;
            assert_eq!(status, 200, "{body}");
            assert_members(&body, &page, 5);
        }
        for (system, code, valid) in [
            (FINITE, "alpha", true),
            (FINITE, "missing", false),
            (SYSTEM, "2", true),
            (SYSTEM, "1", false),
            (LOINC, "1000-1", true),
            (UCUM, "mg", true),
            (UCUM, "kg", false),
        ] {
            let (status, body) = validate(&app, &set, system, code, &[]).await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(parameter(&body, "result")["valueBoolean"], valid, "{body}");
        }
        let result = store
            .query_value_set(
                ValueSetRequest {
                    url: "urn:mixed:inline",
                    version: None,
                    id: None,
                    inline: Some(&set),
                    system_context: None,
                },
                ValueSetOperation::Expand {
                    filter: None,
                    offset: 0,
                    count: 10,
                },
            )
            .await
            .unwrap();
        assert!(
            matches!(result, ValueSetResult::Native(_)),
            "Mixed sets must keep the bounded native relation path"
        );
    }
    let grammar_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM code_systems WHERE url=$1")
        .bind(UCUM)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        grammar_rows, 0,
        "UCUM projections must not become persisted resources"
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn mixed_actual_editions_aliases_and_duplicate_displays_match_finite_identity() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    snomed(&pool).await;
    let old = publish(
        &pool,
        &system(
            "1",
            json!([{"code":"Alpha","display":"Old Alpha"},{"code":"Gone"},{"code":"Common"}]),
        ),
    )
    .await;
    let current = publish(
        &pool,
        &system(
            "2",
            json!([{"code":"Alpha","display":"Current Alpha"},{"code":"Beta"},{"code":"Common"}]),
        ),
    )
    .await;
    sqlx::query("UPDATE code_systems SET updated_at=CASE WHEN id=$1 THEN '2026-01-01'::timestamptz ELSE '2026-02-01'::timestamptz END WHERE id=ANY($2)")
        .bind(old).bind(vec![old,current]).execute(&pool).await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let mut set = mixed(json!({"system":FINITE,"version":"1"}));
    set["compose"]["include"].as_array_mut().unwrap().extend([
        json!({"system":FINITE}),
        json!({"system":format!("{FINITE}|2"),"concept":[{"code":"alpha","display":"Last override"}]}),
    ]);
    let (status, body) = expand(&app, &set, vec![], &[]).await;
    assert_eq!(status, 200, "{body}");
    assert_members(
        &body,
        &[
            (SYSTEM, NATIVE_VERSION, "2"),
            (FINITE, "1", "Alpha"),
            (FINITE, "1", "Common"),
            (FINITE, "1", "Gone"),
            (FINITE, "2", "Alpha"),
            (FINITE, "2", "Beta"),
            (FINITE, "2", "Common"),
        ],
        7,
    );
    assert_eq!(
        member(&body, FINITE, "2", "Alpha")["display"],
        "Last override"
    );
    set["compose"]["include"]
        .as_array_mut()
        .unwrap()
        .push(json!({"system":FINITE,"version":"2","concept":[{"code":"ALPHA"}]}));
    let (status, body) = expand(&app, &set, vec![], &[]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 7);
    assert_eq!(
        member(&body, FINITE, "2", "Alpha")["display"],
        "Current Alpha"
    );
    let used = body["expansion"]["parameter"].as_array().unwrap();
    for canonical in [
        format!("{FINITE}|1"),
        format!("{FINITE}|2"),
        format!("{SYSTEM}|{NATIVE_VERSION}"),
    ] {
        assert!(
            used.iter()
                .any(|p| p["name"] == "used-codesystem" && p["valueUri"] == canonical),
            "{body}"
        );
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn mixed_excludes_preserve_versions_source_spelling_and_unversioned_whole_system_scope() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    snomed(&pool).await;
    let old = publish(
        &pool,
        &system(
            "1",
            json!([{"code":"Alpha"},{"code":"Common"},{"code":"Gone"}]),
        ),
    )
    .await;
    let mut latest = system(
        "2",
        json!([{"code":"ALPHA"},{"code":"Common"},{"code":"Beta"}]),
    );
    latest["caseSensitive"] = true.into();
    let current = publish(&pool, &latest).await;
    sqlx::query("UPDATE code_systems SET updated_at=CASE WHEN id=$1 THEN '2026-01-01'::timestamptz ELSE '2026-02-01'::timestamptz END WHERE id=ANY($2)")
        .bind(old).bind(vec![old,current]).execute(&pool).await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let base = json!({"resourceType":"ValueSet","compose":{"include":[native(),{"system":FINITE,"version":"1"},{"system":FINITE,"version":"2"}]}});
    for (exclude, expected) in [
        (
            json!({"system":FINITE,"version":"1","concept":[{"code":"common"}]}),
            vec![
                (SYSTEM, NATIVE_VERSION, "2"),
                (FINITE, "1", "Alpha"),
                (FINITE, "1", "Gone"),
                (FINITE, "2", "ALPHA"),
                (FINITE, "2", "Beta"),
                (FINITE, "2", "Common"),
            ],
        ),
        (
            json!({"system":FINITE,"concept":[{"code":"Common"}]}),
            vec![
                (SYSTEM, NATIVE_VERSION, "2"),
                (FINITE, "1", "Alpha"),
                (FINITE, "1", "Gone"),
                (FINITE, "2", "ALPHA"),
                (FINITE, "2", "Beta"),
            ],
        ),
        (
            json!({"system":FINITE,"concept":[{"code":"ALPHA"}]}),
            vec![
                (SYSTEM, NATIVE_VERSION, "2"),
                (FINITE, "1", "Alpha"),
                (FINITE, "1", "Common"),
                (FINITE, "1", "Gone"),
                (FINITE, "2", "Beta"),
                (FINITE, "2", "Common"),
            ],
        ),
        (
            json!({"system":FINITE}),
            vec![(SYSTEM, NATIVE_VERSION, "2")],
        ),
    ] {
        let mut set = base.clone();
        set["compose"]["exclude"] = json!([exclude]);
        let (status, body) = expand(&app, &set, vec![], &[]).await;
        assert_eq!(status, 200, "{body}");
        assert_members(&body, &expected, expected.len());
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn mixed_imports_intersect_complete_identities_and_system_constraints() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    snomed(&pool).await;
    publish(
        &pool,
        &system("1", json!([{"code":"A"},{"code":"B"},{"code":"C"}])),
    )
    .await;
    publish(&pool,&json!({"resourceType":"ValueSet","url":"urn:mixed:left","version":"1","status":"active","compose":{"include":[native(),{"system":FINITE,"version":"1","concept":[{"code":"A"},{"code":"B"}]}]}})).await;
    publish(&pool,&json!({"resourceType":"ValueSet","url":"urn:mixed:right","version":"1","status":"active","compose":{"include":[native(),{"system":FINITE,"version":"1","concept":[{"code":"B"},{"code":"C"}]}]}})).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let set = json!({"resourceType":"ValueSet","compose":{"include":[{"valueSet":["urn:mixed:left|1","urn:mixed:right|1"]}]}});
    let (status, body) = expand(&app, &set, vec![], &[]).await;
    assert_eq!(status, 200, "{body}");
    assert_members(
        &body,
        &[(SYSTEM, NATIVE_VERSION, "2"), (FINITE, "1", "B")],
        2,
    );
    let set = json!({"resourceType":"ValueSet","compose":{"include":[{"system":FINITE,"version":"1","valueSet":["urn:mixed:left|1","urn:mixed:right|1"]}]}});
    let (status, body) = expand(&app, &set, vec![], &[]).await;
    assert_eq!(status, 200, "{body}");
    assert_members(&body, &[(FINITE, "1", "B")], 1);
    let set = json!({"resourceType":"ValueSet","compose":{"include":[{"valueSet":["urn:mixed:left|1"]}],"exclude":[{"valueSet":["urn:mixed:right|1"]}]}});
    let (status, body) = expand(&app, &set, vec![], &[]).await;
    assert_eq!(status, 200, "{body}");
    assert_members(&body, &[(FINITE, "1", "A")], 1);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn mixed_request_local_overrides_contained_sets_and_release_leave_no_persisted_or_cached_rows(
) {
    let (admin, pool, schema) = hierarchy_test_database().await;
    snomed(&pool).await;
    let stored = system("1", json!([{"code":"A","display":"Stored A"}]));
    publish(&pool, &stored).await;
    let temporary = system("1", json!([{"code":"B","display":"Temporary B"}]));
    let container = json!({"resourceType":"ValueSet","url":"urn:mixed:container","version":"1","status":"active","contained":[{"resourceType":"ValueSet","id":"local","compose":{"include":[{"system":FINITE,"version":"1"}]}}],"compose":{"include":[native(),{"valueSet":["#local"]}]}});
    let temporary_set = json!({"resourceType":"ValueSet","url":"urn:mixed:temporary-set","version":"1","status":"active","compose":{"include":[{"valueSet":["urn:mixed:container|1"]}]}});
    let before:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM code_systems),(SELECT count(*) FROM value_sets),(SELECT count(*) FROM concepts)").fetch_one(&pool).await.unwrap();
    for cache in [true, false] {
        let store = Arc::new(if cache {
            PostgresStore::new(pool.clone())
        } else {
            PostgresStore::without_prepared_cache(pool.clone())
        });
        let app = api::create_router(store.clone());
        let set = mixed(json!({"system":FINITE,"version":"1"}));
        let (status, body) = expand(&app, &set, vec![], std::slice::from_ref(&temporary)).await;
        assert_eq!(status, 200, "{body}");
        assert_members(
            &body,
            &[(SYSTEM, NATIVE_VERSION, "2"), (FINITE, "1", "B")],
            2,
        );
        assert_eq!(member(&body, FINITE, "1", "B")["display"], "Temporary B");
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            parameters(
                vec![json!({"name":"url","valueUri":"urn:mixed:temporary-set"})],
                &[temporary.clone(), container.clone(), temporary_set.clone()],
            ),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_members(
            &body,
            &[(SYSTEM, NATIVE_VERSION, "2"), (FINITE, "1", "B")],
            2,
        );
        let (status, body) = expand(&app, &set, vec![], &[]).await;
        assert_eq!(status, 200, "{body}");
        assert_members(
            &body,
            &[(SYSTEM, NATIVE_VERSION, "2"), (FINITE, "1", "A")],
            2,
        );
        assert_eq!(member(&body, FINITE, "1", "A")["display"], "Stored A");
        let (status, body) = request(
            &app,
            "GET",
            "/r4/ValueSet/$expand?url=urn:mixed:temporary-set",
            Value::Null,
        )
        .await;
        assert_eq!(status, 404, "{body}");
    }
    let after:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM code_systems),(SELECT count(*) FROM value_sets),(SELECT count(*) FROM concepts)").fetch_one(&pool).await.unwrap();
    assert_eq!(
        before, after,
        "Request-local bridge rows must never be published"
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn mixed_finite_filters_use_r4_properties_hierarchies_and_regex_before_union() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    snomed(&pool).await;
    let mut source = system(
        "1",
        json!([{"code":"Root","concept":[{"code":"Alpha","property":[{"code":"tag","valueCode":"selected"},{"code":"rank","valueInteger":2}]},{"code":"Beta","property":[{"code":"tag","valueCode":"other"},{"code":"rank","valueInteger":3}]}]}]),
    );
    source["hierarchyMeaning"] = "is-a".into();
    source["property"] = json!([{"code":"tag","type":"code"},{"code":"rank","type":"integer"}]);
    publish(&pool, &source).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (filters, expected) in [
        (
            json!([{"property":"code","op":"regex","value":"alpha|beta"}]),
            vec![
                (SYSTEM, NATIVE_VERSION, "2"),
                (FINITE, "1", "Alpha"),
                (FINITE, "1", "Beta"),
            ],
        ),
        (
            json!([{"property":"concept","op":"child-of","value":"ROOT"},{"property":"tag","op":"=","value":"selected"},{"property":"rank","op":"in","value":"1,2"}]),
            vec![(SYSTEM, NATIVE_VERSION, "2"), (FINITE, "1", "Alpha")],
        ),
    ] {
        for temporary in [false, true] {
            let set = mixed(json!({"system":FINITE,"version":"1","filter":filters}));
            let resources = if temporary {
                vec![source.clone()]
            } else {
                vec![]
            };
            let (status, body) = expand(&app, &set, vec![], &resources).await;
            assert_eq!(status, 200, "{body}");
            assert_members(&body, &expected, expected.len());
        }
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn mixed_finite_comparison_uses_engine_unicode_policy_not_postgres_lowercase() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    snomed(&pool).await;
    let source = system("1", json!([{"code":"İ","display":"Dotted capital I"}]));
    publish(&pool, &source).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let set = mixed(json!({"system":FINITE,"version":"1"}));
    for temporary in [false, true] {
        let resources = if temporary {
            vec![source.clone()]
        } else {
            vec![]
        };
        for (code, valid) in [("İ", true), ("i\u{307}", true), ("i", false)] {
            let (status, body) = validate(&app, &set, FINITE, code, &resources).await;
            assert_eq!(status, 200, "{code}: {body}");
            assert_eq!(
                parameter(&body, "result")["valueBoolean"],
                valid,
                "{code}: {body}"
            );
        }
        let set = mixed(
            json!({"system":FINITE,"version":"1","concept":[{"code":"i\u{307}","display":"Canonical override"}]}),
        );
        let (status, body) = expand(&app, &set, vec![], &resources).await;
        assert_eq!(status, 200, "{body}");
        assert_members(
            &body,
            &[(SYSTEM, NATIVE_VERSION, "2"), (FINITE, "1", "İ")],
            2,
        );
        assert_eq!(
            member(&body, FINITE, "1", "İ")["display"],
            "Canonical override"
        );
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn mixed_rich_finite_and_native_supplements_enrich_labels_without_changing_identity() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    snomed(&pool).await;
    publish(&pool,&system("1",json!([{"code":"Alpha","display":"Original Alpha","designation":[{"language":"ru","use":{"system":"urn:use","code":"display"},"value":"Альфа"}]}]))).await;
    let finite_supplement = json!({"resourceType":"CodeSystem","url":"urn:mixed:finite-supplement","version":"1","status":"active","content":"supplement","supplements":format!("{FINITE}|1"),"property":[{"code":"group","type":"code"}],"concept":[{"code":"alpha","designation":[{"language":"de","value":"Zusatz"}],"property":[{"code":"group","valueCode":"selected"}]}]});
    publish(&pool, &finite_supplement).await;
    publish(&pool,&json!({"resourceType":"CodeSystem","url":"urn:mixed:native-supplement","version":"1","status":"active","content":"supplement","supplements":format!("{SYSTEM}|{NATIVE_VERSION}"),"concept":[{"code":"2","designation":[{"language":"de","value":"Kind"}]}]})).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let set = mixed(
        json!({"system":FINITE,"version":"1","filter":[{"property":"group","op":"=","value":"selected"}]}),
    );
    for (language, filter, system, version, code, display) in [
        ("ru", "Альф", FINITE, "1", "Alpha", "Альфа"),
        ("de", "Zusatz", FINITE, "1", "Alpha", "Zusatz"),
        ("de", "Kind", SYSTEM, NATIVE_VERSION, "2", "Kind"),
    ] {
        let (status, body) = expand(
            &app,
            &set,
            vec![
                json!({"name":"displayLanguage","valueCode":language}),
                json!({"name":"filter","valueString":filter}),
                json!({"name":"includeDesignations","valueBoolean":true}),
                json!({"name":"designation","valueString":language}),
            ],
            &[],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_members(&body, &[(system, version, code)], 1);
        let row = member(&body, system, version, code);
        assert_eq!(row["display"], display);
        assert!(
            row["designation"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["language"] == language && d["value"] == display),
            "{body}"
        );
    }
    let mut required = set.clone();
    required["extension"] = json!([{"url":"http://hl7.org/fhir/StructureDefinition/valueset-supplement","valueCanonical":"urn:mixed:finite-supplement|1"}]);
    let (status, body) = expand(&app, &required, vec![], &[]).await;
    assert_eq!(status, 200, "{body}");
    assert_members(
        &body,
        &[(SYSTEM, NATIVE_VERSION, "2"), (FINITE, "1", "Alpha")],
        2,
    );
    assert!(
        body["expansion"]["parameter"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "used-supplement"
                && p["valueUri"] == "urn:mixed:finite-supplement|1"),
        "{body}"
    );
    required["extension"][0]["valueCanonical"] = "urn:mixed:missing-supplement".into();
    let (status, body) = expand(&app, &required, vec![], &[]).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["resourceType"], "OperationOutcome");
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn mixed_rich_options_filter_finite_flags_before_count_pages_and_nest_both_hierarchies() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    snomed(&pool).await;
    let mut source = system(
        "1",
        json!([{"code":"Root","display":"Finite root","concept":[{"code":"Active","display":"Finite active"},{"code":"Inactive","property":[{"code":"inactive","valueBoolean":true}]},{"code":"Logical","property":[{"code":"notSelectable","valueBoolean":true}]},{"code":"Expression","property":[{"code":"postCoordinated","valueBoolean":true}]}]}]),
    );
    source["hierarchyMeaning"] = "is-a".into();
    publish(&pool, &source).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let set = json!({"resourceType":"ValueSet","compose":{"include":[{"system":SYSTEM,"version":NATIVE_VERSION},{"system":FINITE,"version":"1"}]}});
    let flags = vec![
        json!({"name":"activeOnly","valueBoolean":true}),
        json!({"name":"excludeNotForUI","valueBoolean":true}),
        json!({"name":"excludePostCoordinated","valueBoolean":true}),
    ];
    let (status, body) = expand(&app, &set, flags.clone(), &[]).await;
    assert_eq!(status, 200, "{body}");
    assert_members(
        &body,
        &[
            (SYSTEM, NATIVE_VERSION, "1"),
            (SYSTEM, NATIVE_VERSION, "2"),
            (FINITE, "1", "Active"),
            (FINITE, "1", "Root"),
        ],
        4,
    );
    let mut paged = flags.clone();
    paged.extend([
        json!({"name":"offset","valueInteger":2}),
        json!({"name":"count","valueInteger":1}),
    ]);
    let (status, body) = expand(&app, &set, paged, &[]).await;
    assert_eq!(status, 200, "{body}");
    assert_members(&body, &[(FINITE, "1", "Active")], 4);
    let mut nested = flags.clone();
    nested.push(json!({"name":"excludeNested","valueBoolean":false}));
    let (status, body) = expand(&app, &set, nested, &[]).await;
    assert_eq!(status, 200, "{body}");
    assert_members(
        &body,
        &[(SYSTEM, NATIVE_VERSION, "1"), (FINITE, "1", "Root")],
        4,
    );
    assert_eq!(
        member(&body, SYSTEM, NATIVE_VERSION, "1")["contains"][0]["code"],
        "2"
    );
    assert_eq!(
        member(&body, FINITE, "1", "Root")["contains"][0]["code"],
        "Active"
    );
    let mut incomplete = flags;
    incomplete.extend([
        json!({"name":"excludeNested","valueBoolean":false}),
        json!({"name":"count","valueInteger":1}),
    ]);
    let (status, body) = expand(&app, &set, incomplete, &[]).await;
    assert_eq!(status, 400, "{body}");
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn mixed_codeable_concept_validation_uses_selected_finite_designations_and_field_paths() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    snomed(&pool).await;
    publish(&pool,&system("1",json!([{"code":"Alpha","display":"Authoritative Alpha","designation":[{"language":"ru","value":"Альфа"}]}]))).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let set = mixed(json!({"system":FINITE,"version":"1"}));
    for (display, valid) in [("Альфа", true), ("Wrong display", false)] {
        let(status,body)=request(&app,"POST","/r4/ValueSet/$validate-code",parameters(vec![json!({"name":"valueSet","resource":set}),json!({"name":"codeableConcept","valueCodeableConcept":{"coding":[{"system":"urn:outside","code":"unrelated"},{"system":FINITE,"version":"1","code":"alpha","display":display}]}}),json!({"name":"displayLanguage","valueCode":"ru"})],&[])).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], valid, "{body}");
        assert_eq!(parameter(&body, "system")["valueUri"], FINITE);
        assert_eq!(parameter(&body, "code")["valueCode"], "alpha");
        assert_eq!(parameter(&body, "normalized-code")["valueCode"], "Alpha");
        let issues = parameter(&body, "issues")["resource"]["issue"]
            .as_array()
            .unwrap();
        assert!(
            issues
                .iter()
                .any(|i| i["details"]["coding"][0]["code"] == "code-rule"
                    && i["expression"] == json!(["CodeableConcept.coding[1].code"])),
            "{body}"
        );
        if !valid {
            assert!(
                issues
                    .iter()
                    .any(|i| i["details"]["coding"][0]["code"] == "invalid-display"
                        && i["expression"] == json!(["CodeableConcept.coding[1].display"])),
                "{body}"
            );
        }
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn mixed_unknown_incomplete_ucum_and_unsupported_filters_fail_whole_operations() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    snomed(&pool).await;
    publish(&pool, &system("1", json!([{"code":"Known"}]))).await;
    let mut fragment = system("fragment", json!([{"code":"Known"}]));
    fragment["content"] = "fragment".into();
    publish(&pool, &fragment).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (selection, status) in [
        (json!({"system":"urn:mixed:unknown"}), 404),
        (json!({"system":FINITE,"version":"missing"}), 404),
        (json!({"system":FINITE,"version":"fragment"}), 422),
        (
            json!({"system":FINITE,"version":"1","concept":[{"code":"Missing"}]}),
            422,
        ),
        (
            json!({"system":FINITE,"version":"1","filter":[{"property":"code","op":"regex","value":"(?<=x)y"}]}),
            422,
        ),
        (json!({"system":UCUM}), 422),
        (
            json!({"system":UCUM,"concept":[{"code":"not-a-unit"}]}),
            422,
        ),
        (
            json!({"system":UCUM,"concept":[{"code":"mg"}],"filter":[{"property":"code","op":"=","value":"mg"}]}),
            422,
        ),
    ] {
        let set = mixed(selection);
        for count in [0, 10] {
            let (actual, body) = expand(
                &app,
                &set,
                vec![json!({"name":"count","valueInteger":count})],
                &[],
            )
            .await;
            assert_eq!(actual, status, "{body}");
            assert_eq!(
                body["resourceType"], "OperationOutcome",
                "No early native positive can hide an unresolved finite source: {body}"
            );
        }
        let (actual, body) = validate(&app, &set, SYSTEM, "2", &[]).await;
        assert_eq!(
            actual, status,
            "Membership must verify every dependency even when native code is included: {body}"
        );
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn mixed_new_editions_commits_rollbacks_and_concurrent_reads_use_one_source_snapshot() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let native_id = snomed(&pool).await;
    publish(
        &pool,
        &system("1", json!([{"code":"A","display":"Before"}])),
    )
    .await;
    sqlx::query("UPDATE concepts SET display='Before' WHERE code_system_id=$1 AND code='2'")
        .bind(native_id)
        .execute(&pool)
        .await
        .unwrap();
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let app = api::create_router(store);
    let set = mixed(json!({"system":FINITE}));
    let (status, body) = expand(&app, &set, vec![], &[]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(member(&body, FINITE, "1", "A")["display"], "Before");
    let next = system(
        "2",
        json!([{"code":"A","display":"After"},{"code":"B","display":"New code"}]),
    );
    let inspection = term_squid_ingestion::inspect(&next).unwrap();
    let mut tx = pool.begin().await.unwrap();
    term_squid_ingestion::import_resource(&mut tx, &next, &inspection)
        .await
        .unwrap();
    sqlx::query("UPDATE concepts SET display='After' WHERE code_system_id=$1 AND code='2'")
        .bind(native_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let (status, body) = expand(&app, &set, vec![], &[]).await;
    assert_eq!(status, 200, "{body}");
    assert_members(
        &body,
        &[(SYSTEM, NATIVE_VERSION, "2"), (FINITE, "1", "A")],
        2,
    );
    assert_eq!(
        member(&body, SYSTEM, NATIVE_VERSION, "2")["display"],
        "Before"
    );
    tx.rollback().await.unwrap();
    let (status, body) = validate(&app, &set, FINITE, "B", &[]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    let mut readers = Vec::new();
    for _ in 0..8 {
        let app = app.clone();
        let set = set.clone();
        readers.push(tokio::spawn(async move {
            for _ in 0..8 {
                let (status, body) = expand(&app, &set, vec![], &[]).await;
                assert_eq!(status, 200, "{body}");
                let finite = body["expansion"]["contains"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|m| m["system"] == FINITE && m["code"] == "A")
                    .unwrap();
                let native = member(&body, SYSTEM, NATIVE_VERSION, "2");
                let pair = (
                    finite["version"].as_str().unwrap(),
                    finite["display"].as_str().unwrap(),
                    native["display"].as_str().unwrap(),
                );
                assert!(
                    matches!(pair, ("1", "Before", "Before") | ("2", "After", "After")),
                    "Mixed snapshot: {body}"
                );
            }
        }));
    }
    let mut tx = pool.begin().await.unwrap();
    term_squid_ingestion::import_resource(&mut tx, &next, &inspection)
        .await
        .unwrap();
    sqlx::query("UPDATE concepts SET display='After' WHERE code_system_id=$1 AND code='2'")
        .bind(native_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    for reader in readers {
        reader.await.unwrap();
    }
    let (status, body) = expand(&app, &set, vec![], &[]).await;
    assert_eq!(status, 200, "{body}");
    assert_members(
        &body,
        &[
            (SYSTEM, NATIVE_VERSION, "2"),
            (FINITE, "2", "A"),
            (FINITE, "2", "B"),
        ],
        3,
    );
    assert_eq!(member(&body, FINITE, "2", "A")["display"], "After");
    assert_eq!(
        member(&body, SYSTEM, NATIVE_VERSION, "2")["display"],
        "After"
    );
    let (status, body) = validate(&app, &set, FINITE, "B", &[]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn mixed_source_and_bridge_admission_rejects_complete_work_even_for_zero_count_or_early_matches(
) {
    let (admin, pool, schema) = hierarchy_test_database().await;
    snomed(&pool).await;
    // Raw JSON fixtures intentionally bypass importer admission to exercise the
    // provider's independent read bounds without manufacturing partial projections.
    for (version, count, padding) in [
        ("concept-overflow", 100_001, 0),
        ("aggregate", 50_001, 0),
        ("bridge-overflow", 20_000, 0),
        ("source-overflow", 1, 33 * 1024 * 1024),
    ] {
        let mut source = system(
            version,
            Value::Array(
                (0..count)
                    .map(|n| json!({"code":format!("c{n:06}")}))
                    .collect(),
            ),
        );
        if padding > 0 {
            source["copyright"] = "x".repeat(padding).into();
        }
        sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content) VALUES($1,$2,$3,'active','Generated bounded source fixture',$4)")
            .bind(Uuid::new_v4()).bind(FINITE).bind(version).bind(sqlx::types::Json(source)).execute(&pool).await.unwrap();
    }
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let aggregate = json!({"resourceType":"ValueSet","compose":{"include":[native(),{"system":FINITE,"version":"aggregate","concept":[{"code":"c000000"}]},{"system":FINITE,"version":"aggregate","concept":[{"code":"c000000"}]}]}});
    let mut selections =
        mixed(json!({"system":FINITE,"version":"aggregate","concept":[{"code":"c000000"}]}));
    selections["compose"]["include"]
        .as_array_mut()
        .unwrap()
        .extend((0..128).map(|_| native()));
    for (set, expected_status) in [
        (
            mixed(
                json!({"system":FINITE,"version":"concept-overflow","concept":[{"code":"c000000"}]}),
            ),
            422,
        ),
        (aggregate, 422),
        (
            mixed(json!({"system":FINITE,"version":"bridge-overflow"})),
            422,
        ),
        // Raw source-size admission preserves compose_loader's BadRequest contract.
        (
            mixed(
                json!({"system":FINITE,"version":"source-overflow","concept":[{"code":"c000000"}]}),
            ),
            400,
        ),
        (selections, 422),
    ] {
        let (status, body) = expand(
            &app,
            &set,
            vec![json!({"name":"count","valueInteger":0})],
            &[],
        )
        .await;
        assert_eq!(status, expected_status, "{body}");
        assert_eq!(body["resourceType"], "OperationOutcome");
        assert!(
            body.get("expansion").is_none(),
            "Admission cannot publish a truncated success: {body}"
        );
        let (status, body) = validate(&app, &set, SYSTEM, "2", &[]).await;
        assert_eq!(
            status, expected_status,
            "An included native code cannot bypass incomplete finite work: {body}"
        );
    }
    // A rejected large source must not poison later small mixed operations.
    let small = system("small", json!([{"code":"A"}]));
    let set = mixed(json!({"system":FINITE,"version":"small"}));
    let (status, body) = expand(&app, &set, vec![], &[small]).await;
    assert_eq!(status, 200, "{body}");
    assert_members(
        &body,
        &[(SYSTEM, NATIVE_VERSION, "2"), (FINITE, "small", "A")],
        2,
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn mixed_cancellation_keeps_cpu_admission_until_the_blocking_leaf_finishes() {
    use crate::store::finite_bridge;
    use std::time::Duration;

    let (admin, pool, schema) = hierarchy_test_database().await;
    snomed(&pool).await;
    publish(&pool, &system("1", json!([{"code":"A"}]))).await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let set = mixed(json!({"system":FINITE,"version":"1"}));
    assert_eq!(store.native_available_permits(), 4);
    let (pause, started, release) = finite_bridge::Pause::new();
    let worker = store.clone();
    let operation = tokio::spawn(async move {
        finite_bridge::paused(
            pause,
            worker.query_value_set(
                ValueSetRequest {
                    url: "urn:mixed:cancel",
                    version: None,
                    id: None,
                    inline: Some(&set),
                    system_context: None,
                },
                ValueSetOperation::Expand {
                    filter: None,
                    offset: 0,
                    count: 10,
                },
            ),
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(5), started)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        store.native_available_permits(),
        3,
        "The public operation must pass its existing CPU admission into the leaf"
    );
    operation.abort();
    assert!(operation.await.unwrap_err().is_cancelled());
    assert_eq!(store.native_available_permits(),3,"Aborting HTTP/application work must not admit another CPU worker while its blocking leaf remains live");
    drop(release);
    tokio::time::timeout(Duration::from_secs(5), async {
        while store.native_available_permits() != 4 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let set = mixed(json!({"system":FINITE,"version":"1"}));
    let result = store
        .query_value_set(
            ValueSetRequest {
                url: "urn:mixed:after-cancel",
                version: None,
                id: None,
                inline: Some(&set),
                system_context: None,
            },
            ValueSetOperation::Expand {
                filter: None,
                offset: 0,
                count: 10,
            },
        )
        .await
        .unwrap();
    let ValueSetResult::Native(page) = result else {
        panic!("Mixed recovery must keep native dispatch")
    };
    assert_eq!(page.total, 2);
    assert_eq!(
        page.members
            .iter()
            .map(|m| m.code.as_str())
            .collect::<Vec<_>>(),
        ["2", "A"]
    );
    assert_eq!(store.native_available_permits(), 4);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn mixed_designation_admission_preserves_256_labels_and_rejects_overflow_before_early_results(
) {
    let (admin, pool, schema) = hierarchy_test_database().await;
    snomed(&pool).await;
    let labels: Vec<_> = (0..256)
        .map(|n| json!({"language":if n%2==0 {"en"}else{"de"},"value":format!("Label {n}")}))
        .collect();
    let valid = system(
        "1",
        json!([{"code":"A","display":"Primary","designation":labels}]),
    );
    publish(&pool, &valid).await;
    let mut overflow = valid.clone();
    overflow["version"] = "overflow".into();
    overflow["concept"][0]["designation"]
        .as_array_mut()
        .unwrap()
        .push(json!({"language":"en","value":"Label 256"}));
    publish(&pool, &overflow).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let set = mixed(json!({"system":FINITE,"version":"1"}));
    let (status, body) = expand(
        &app,
        &set,
        vec![json!({"name":"includeDesignations","valueBoolean":true})],
        &[],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_members(
        &body,
        &[(SYSTEM, NATIVE_VERSION, "2"), (FINITE, "1", "A")],
        2,
    );
    assert_eq!(
        member(&body, FINITE, "1", "A")["designation"]
            .as_array()
            .unwrap()
            .len(),
        256
    );
    let (status, body) = expand(
        &app,
        &set,
        vec![
            json!({"name":"includeDesignations","valueBoolean":true}),
            json!({"name":"designation","valueString":"en"}),
            json!({"name":"displayLanguage","valueCode":"de"}),
            json!({"name":"filter","valueString":"Label 1"}),
        ],
        &[],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_members(&body, &[(FINITE, "1", "A")], 1);
    let row = member(&body, FINITE, "1", "A");
    assert_eq!(row["display"], "Label 1");
    let filtered = row["designation"].as_array().unwrap();
    assert_eq!(filtered.len(), 128);
    assert!(filtered.iter().all(|d| d["language"] == "en"));
    assert_eq!(filtered[0]["value"], "Label 0");
    for temporary in [false, true] {
        let mut temporary_source = overflow.clone();
        temporary_source["version"] = "1".into();
        let resources = if temporary {
            vec![temporary_source]
        } else {
            vec![]
        };
        let set = mixed(json!({"system":FINITE,"version":if temporary {"1"}else{"overflow"}}));
        for count in [0, 1] {
            let (status, body) = expand(
                &app,
                &set,
                vec![json!({"name":"count","valueInteger":count})],
                &resources,
            )
            .await;
            assert_eq!(status, 422, "{body}");
            assert!(
                body["issue"][0]["diagnostics"]
                    .as_str()
                    .unwrap()
                    .contains("256"),
                "{body}"
            );
            assert!(body.get("expansion").is_none());
        }
        let (status, body) = validate(&app, &set, SYSTEM, "2", &resources).await;
        assert_eq!(
            status, 422,
            "Native early positives must not bypass finite label admission: {body}"
        );
    }
    let (status, body) = expand(&app, &set, vec![], &[]).await;
    assert_eq!(
        status, 200,
        "Overflow must not poison the admitted stored edition: {body}"
    );
    assert_members(
        &body,
        &[(SYSTEM, NATIVE_VERSION, "2"), (FINITE, "1", "A")],
        2,
    );
    cleanup(admin, pool, schema).await;
}
