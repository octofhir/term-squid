//! Required mapping modifiers and group-relative dependency namespaces fail closed.
use super::*;

fn map() -> Value {
    json!({"resourceType":"ConceptMap","url":"urn:safety:map","version":"map-v1","status":"active","group":[{"source":"urn:s","sourceVersion":"1","target":"urn:t","targetVersion":"2","element":[{"code":"a","target":[{"code":"b","equivalence":"equivalent"}]}]}]})
}
fn malformed_maps() -> Vec<Value> {
    let mut result = Vec::new();
    let mut missing = map();
    missing["group"][0]["element"][0]["target"][0]
        .as_object_mut()
        .unwrap()
        .remove("equivalence");
    result.push(missing);
    for value in [
        Value::Null,
        json!(false),
        json!(1),
        json!([]),
        json!({}),
        json!(""),
        json!("Equivalent"),
        json!("invalid"),
    ] {
        let mut malformed = map();
        malformed["group"][0]["element"][0]["target"][0]["equivalence"] = value;
        result.push(malformed);
    }
    result
}
fn parameters(fields: Vec<Value>) -> Value {
    json!({"resourceType":"Parameters","parameter":fields})
}
fn input() -> Vec<Value> {
    vec![
        json!({"name":"system","valueUri":"urn:s"}),
        json!({"name":"version","valueString":"1"}),
        json!({"name":"code","valueCode":"a"}),
    ]
}
async fn inline(app: &axum::Router, map: Value, mut extra: Vec<Value>) -> (u16, Value) {
    let mut fields = vec![json!({"name":"conceptMap","resource":map})];
    fields.append(&mut extra);
    request(app, "POST", "/r4/ConceptMap/$translate", parameters(fields)).await
}
fn lazy() -> axum::Router {
    api::create_router(Arc::new(PostgresStore::new(
        PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap(),
    )))
}
async fn insert(pool: &PgPool, map: &Value) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO concept_maps(id,url,version,status,content) VALUES($1,$2,$3,'active',$4)",
    )
    .bind(id)
    .bind(map["url"].as_str())
    .bind(map["version"].as_str())
    .bind(sqlx::types::Json(map))
    .execute(pool)
    .await
    .unwrap();
    id
}
async fn cleanup(admin: PgPool, pool: PgPool, schema: String) {
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
fn dependency(element: &str, system: Option<&str>, code: &str) -> Value {
    let mut coding = json!({"code":code});
    if let Some(system) = system {
        coding["system"] = system.into();
    }
    json!({"name":"dependency","part":[{"name":"element","valueUri":element},{"name":"concept","valueCodeableConcept":{"coding":[coding]}}]})
}
fn match_parts(body: &Value) -> &Vec<Value> {
    parameter(body, "match")["part"].as_array().unwrap()
}

#[tokio::test]
async fn translation_inline_never_invents_a_missing_or_invalid_required_equivalence() {
    let app = lazy();
    for malformed in malformed_maps() {
        for mut fields in [
            input(),
            vec![
                json!({"name":"system","valueUri":"urn:unrelated"}),
                json!({"name":"code","valueCode":"missing"}),
            ],
        ] {
            let (status, body) = inline(&app, malformed.clone(), std::mem::take(&mut fields)).await;
            assert_eq!(status, 422, "{body}");
            assert_eq!(body["resourceType"], "OperationOutcome");
            assert!(body.to_string().contains("equivalence"), "{body}");
        }
    }
}

#[tokio::test]
async fn translation_temporary_maps_reject_bad_modifiers_before_any_database_access() {
    let app = lazy();
    for malformed in malformed_maps() {
        let mut fields = input();
        fields.extend([
            json!({"name":"url","valueUri":"urn:safety:map"}),
            json!({"name":"tx-resource","resource":malformed}),
        ]);
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ConceptMap/$translate",
            parameters(fields),
        )
        .await;
        assert_eq!(status, 422, "{body}");
        assert!(body.to_string().contains("equivalence"));
    }
}

#[tokio::test]
async fn translation_all_ten_explicit_equivalences_keep_versions_and_negative_codeless_targets() {
    let app = lazy();
    for equivalence in [
        "relatedto",
        "equivalent",
        "equal",
        "wider",
        "subsumes",
        "narrower",
        "specializes",
        "inexact",
        "unmatched",
        "disjoint",
    ] {
        let mut map = map();
        let target = &mut map["group"][0]["element"][0]["target"][0];
        target["equivalence"] = equivalence.into();
        let negative = matches!(equivalence, "unmatched" | "disjoint");
        if negative {
            target.as_object_mut().unwrap().remove("code");
        }
        map["group"][0]["unmapped"] = json!({"mode":"provided"});
        let (status, body) = inline(&app, map, input()).await;
        assert_eq!(status, 200, "{equivalence}: {body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], !negative);
        if !negative {
            let parts = match_parts(&body);
            assert!(parts
                .iter()
                .any(|p| p["name"] == "equivalence" && p["valueCode"] == equivalence));
            let coding = &parts.iter().find(|p| p["name"] == "concept").unwrap()["valueCoding"];
            assert_eq!(coding["system"], "urn:t");
            assert_eq!(coding["code"], "b");
            assert_eq!(coding["version"], "2");
        }
    }
}

#[tokio::test]
async fn translation_group_defaults_are_not_wildcards_and_products_are_r4_codings_in_both_directions(
) {
    let app = lazy();
    let mut map = map();
    let target = &mut map["group"][0]["element"][0]["target"][0];
    target["dependsOn"] = json!([{"property":"urn:axis","value":"X"}]);
    target["product"] = json!([{"property":"urn:result","value":"Y"}]);
    for (reverse, input_system, version, code, element, value, output_system, output_code) in [
        (false, "urn:s", "1", "a", "urn:axis", "X", "urn:t", "Y"),
        (true, "urn:t", "2", "b", "urn:result", "Y", "urn:s", "X"),
    ] {
        for system in [None, Some(input_system), Some("urn:evil")] {
            let fields = vec![
                json!({"name":"system","valueUri":input_system}),
                json!({"name":"version","valueString":version}),
                json!({"name":"code","valueCode":code}),
                json!({"name":"reverse","valueBoolean":reverse}),
                dependency(element, system, value),
            ];
            let (status, body) = inline(&app, map.clone(), fields).await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(
                parameter(&body, "result")["valueBoolean"],
                system != Some("urn:evil")
            );
            if system == Some("urn:evil") {
                continue;
            }
            let product = match_parts(&body)
                .iter()
                .find(|p| p["name"] == "product")
                .unwrap();
            let concept = product["part"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == "concept")
                .unwrap();
            assert!(
                concept.get("valueCodeableConcept").is_none(),
                "R4 product output is Coding, not CodeableConcept: {concept}"
            );
            assert_eq!(concept["valueCoding"]["system"], output_system);
            assert_eq!(concept["valueCoding"]["code"], output_code);
            assert!(concept["valueCoding"].get("version").is_none());
        }
    }
}

#[tokio::test]
async fn translation_product_canonical_versions_are_coding_versions_in_both_directions() {
    let app = lazy();
    for versioned in [false, true] {
        let mut map = map();
        let target = &mut map["group"][0]["element"][0]["target"][0];
        target["dependsOn"] = json!([{"property":"urn:axis","system":if versioned {"urn:axis-system|in-v1"} else {"urn:axis-system"},"value":"X"}]);
        target["product"] = json!([{"property":"urn:result","system":if versioned {"urn:product-system|out-v2"} else {"urn:product-system"},"value":"Y"}]);
        for (
            reverse,
            system,
            input_version,
            code,
            element,
            dependency_system,
            dependency_version,
            value,
            output_system,
            output_version,
            output_code,
        ) in [
            (
                false,
                "urn:s",
                "1",
                "a",
                "urn:axis",
                "urn:axis-system",
                "in-v1",
                "X",
                "urn:product-system",
                "out-v2",
                "Y",
            ),
            (
                true,
                "urn:t",
                "2",
                "b",
                "urn:result",
                "urn:product-system",
                "out-v2",
                "Y",
                "urn:axis-system",
                "in-v1",
                "X",
            ),
        ] {
            let mut dependent = dependency(element, Some(dependency_system), value);
            if versioned {
                dependent["part"][1]["valueCodeableConcept"]["coding"][0]["version"] =
                    dependency_version.into();
            }
            let fields = vec![
                json!({"name":"system","valueUri":system}),
                json!({"name":"version","valueString":input_version}),
                json!({"name":"code","valueCode":code}),
                json!({"name":"reverse","valueBoolean":reverse}),
                dependent,
            ];
            let (status, body) = inline(&app, map.clone(), fields).await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(parameter(&body, "result")["valueBoolean"], true);
            let product = match_parts(&body)
                .iter()
                .find(|p| p["name"] == "product")
                .unwrap();
            let concept = product["part"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == "concept")
                .unwrap();
            assert!(concept.get("valueCodeableConcept").is_none());
            assert_eq!(concept["valueCoding"]["system"], output_system);
            assert_eq!(concept["valueCoding"]["code"], output_code);
            assert_eq!(
                concept["valueCoding"]["version"].as_str(),
                versioned.then_some(output_version)
            );
            assert!(!concept["valueCoding"]["system"]
                .as_str()
                .unwrap()
                .contains('|'));
        }
    }
}

#[tokio::test]
async fn translation_explicit_dependency_namespace_wins_over_group_and_unqualified_request() {
    let app = lazy();
    let mut map = map();
    map["group"][0]["element"][0]["target"][0]["dependsOn"] =
        json!([{"property":"urn:axis","system":"urn:external","value":"X"}]);
    for system in [None, Some("urn:s"), Some("urn:external"), Some("urn:evil")] {
        let mut fields = input();
        fields.push(dependency("urn:axis", system, "X"));
        let (status, body) = inline(&app, map.clone(), fields).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(
            parameter(&body, "result")["valueBoolean"],
            system == Some("urn:external")
        );
    }
    for (key, group) in [("dependsOn", "source"), ("product", "target")] {
        let mut unresolved = map.clone();
        unresolved["group"][0]["element"][0]["target"][0][key] =
            json!([{"property":"urn:axis","value":"X"}]);
        unresolved["group"][0]
            .as_object_mut()
            .unwrap()
            .remove(group);
        let (status, body) = inline(&app, unresolved, input()).await;
        assert_eq!(status, 422, "{body}");
        assert!(body.to_string().contains("system"));
    }
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn translation_warm_and_cache_off_stored_reads_reject_malformed_sql_updates_and_recover() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let valid = map();
    let id = insert(&pool, &valid).await;
    let stores = [
        Arc::new(PostgresStore::new(pool.clone())),
        Arc::new(PostgresStore::without_prepared_cache(pool.clone())),
    ];
    let apps = stores
        .iter()
        .map(|s| api::create_router(s.clone()))
        .collect::<Vec<_>>();
    let path="/r4/ConceptMap/$translate?url=urn:safety:map&conceptMapVersion=map-v1&system=urn:s&version=1&code=a";
    for app in &apps {
        let (status, body) = request(app, "GET", path, Value::Null).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    }
    for malformed in malformed_maps() {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("UPDATE concept_maps SET content=$2 WHERE id=$1")
            .bind(id)
            .bind(sqlx::types::Json(&malformed))
            .execute(&mut *tx)
            .await
            .unwrap();
        for app in &apps {
            let (status, body) = request(app, "GET", path, Value::Null).await;
            assert_eq!(status, 200, "{body}");
        }
        tx.rollback().await.unwrap();
        sqlx::query("UPDATE concept_maps SET content=$2 WHERE id=$1")
            .bind(id)
            .bind(sqlx::types::Json(&malformed))
            .execute(&pool)
            .await
            .unwrap();
        for (store, app) in stores.iter().zip(&apps) {
            assert!(store
                .prepare_concept_map("urn:safety:map", Some("map-v1"), None)
                .await
                .is_err());
            for url in [
                path,
                &format!("/r4/ConceptMap/{id}/$translate?system=urn:s&version=1&code=a"),
            ] {
                let (status, body) = request(app, "GET", url, Value::Null).await;
                assert_eq!(status, 422, "{body}");
                assert!(body.to_string().contains("equivalence"));
            }
            // Discovery uses fresh contextual map preparation, not the primitive map cache.
            let (status, body) = request(
                app,
                "POST",
                "/r4/ConceptMap/$translate",
                parameters(input()),
            )
            .await;
            assert_eq!(status, 422, "{body}");
        }
        sqlx::query("UPDATE concept_maps SET content=$2 WHERE id=$1")
            .bind(id)
            .bind(sqlx::types::Json(&valid))
            .execute(&pool)
            .await
            .unwrap();
        for app in &apps {
            let (status, body) = request(app, "GET", path, Value::Null).await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(parameter(&body, "result")["valueBoolean"], true);
        }
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn translation_discovery_and_temporary_namespace_defaults_match_the_same_core_contract() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let mut map = map();
    let target = &mut map["group"][0]["element"][0]["target"][0];
    target["dependsOn"] = json!([{"property":"urn:axis","value":"X"}]);
    target["product"] = json!([{"property":"urn:result","value":"Y"}]);
    insert(&pool, &map).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for temporary in [false, true] {
        for system in [None, Some("urn:s"), Some("urn:evil")] {
            let mut fields = input();
            fields.push(dependency("urn:axis", system, "X"));
            if temporary {
                fields.extend([
                    json!({"name":"url","valueUri":"urn:safety:map"}),
                    json!({"name":"tx-resource","resource":map}),
                ]);
            }
            let (status, body) = request(
                &app,
                "POST",
                "/r4/ConceptMap/$translate",
                parameters(fields),
            )
            .await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(
                parameter(&body, "result")["valueBoolean"],
                system != Some("urn:evil")
            );
            if system != Some("urn:evil") {
                let product = match_parts(&body)
                    .iter()
                    .find(|p| p["name"] == "product")
                    .unwrap();
                assert_eq!(product["part"][1]["valueCoding"]["system"], "urn:t");
                assert!(product["part"][1].get("valueCodeableConcept").is_none());
            }
        }
    }
    cleanup(admin, pool, schema).await;
}
