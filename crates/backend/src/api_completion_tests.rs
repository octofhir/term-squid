//! End-to-end R4 operation forms and shared-snapshot semantics.
use super::{hierarchy_test_database, insert_set, insert_system, parameter, request};
use crate::{api, store::PostgresStore};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
fn params(values: Vec<Value>) -> Value {
    json!({"resourceType":"Parameters","parameter":values})
}
fn cc(codings: Value) -> Value {
    json!({"name":"codeableConcept","valueCodeableConcept":{"coding":codings}})
}
#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_rich_expansion_filters_translated_displays_before_paging_and_preserves_projection_content(
) {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let version = "http://snomed.info/sct/900000000000207008/version/20200131";
    let id = super::native_test_system(&pool, version).await;
    sqlx::query("INSERT INTO snomed_releases(code_system_id,edition_module,release_date,dialect,language,fingerprint,provenance) VALUES ($1,'900000000000207008','2020-01-31','900000000000509007','en',repeat('a',64),'{}')").bind(id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) VALUES ($1,'1','Root','[]'),($1,'2','Child','[]'),($1,'3','Inactive','[{\"code\":\"inactive\",\"valueBoolean\":true}]')").bind(id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO snomed_relationships(code_system_id,id,effective_time,active,module_id,source_code,destination_code,relationship_group,type_id,characteristic_type_id,modifier_id) VALUES ($1,'11','2020-01-31',true,'1','2','1',0,'116680003','900000000000011006','900000000000451002')").bind(id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO snomed_descriptions(code_system_id,id,effective_time,active,module_id,concept_code,language_code,type_id,term,case_significance_id) VALUES ($1,'21','2020-01-31',true,'1','1','ru','900000000000013009','Корень','900000000000448009'),($1,'22','2020-01-31',true,'1','2','ru','900000000000013009','Дочерний','900000000000448009')").bind(id).execute(&pool).await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let resource = json!({"resourceType":"ValueSet","compose":{"include":[{"system":"http://snomed.info/sct","version":version}]}});
    for count in [0, 1] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            params(vec![
                json!({"name":"valueSet","resource":resource}),
                json!({"name":"displayLanguage","valueCode":"ru"}),
                json!({"name":"filter","valueString":"Доч"}),
                json!({"name":"includeDesignations","valueBoolean":true}),
                json!({"name":"designation","valueString":"ru"}),
                json!({"name":"activeOnly","valueBoolean":true}),
                json!({"name":"count","valueInteger":count}),
            ]),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], 1);
        assert_eq!(
            body["expansion"]["contains"].as_array().unwrap().len(),
            count as usize
        );
        if count == 1 {
            assert_eq!(body["expansion"]["contains"][0]["display"], "Дочерний");
            assert_eq!(
                body["expansion"]["contains"][0]["designation"][0]["use"]["code"],
                "900000000000013009"
            );
        }
    }
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        params(vec![
            json!({"name":"valueSet","resource":resource}),
            json!({"name":"excludeNested","valueBoolean":false}),
            json!({"name":"activeOnly","valueBoolean":true}),
            json!({"name":"excludePostCoordinated","valueBoolean":true}),
            json!({"name":"excludeNotForUI","valueBoolean":true}),
        ]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 2);
    assert_eq!(body["expansion"]["contains"][0]["code"], "1");
    assert_eq!(body["expansion"]["contains"][0]["contains"][0]["code"], "2");
    let(status,body)=request(&app,"GET",&format!("/r4/CodeSystem/{id}/$lookup?code=2&displayLanguage=ru&property=parent&property=designation&date=2021"),Value::Null).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "display")["valueString"], "Дочерний");
    assert!(body["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "property"
            && p["part"][0]["valueCode"] == "parent"
            && p["part"][1]["valueCode"] == "1"));
    let(status,body)=request(&app,"POST","/r4/ValueSet/$validate-code",params(vec![json!({"name":"valueSet","resource":resource}),cc(json!([{"system":"http://snomed.info/sct","code":"2:3"},{"system":"http://snomed.info/sct","code":"2","display":"Дочерний"}])),json!({"name":"displayLanguage","valueCode":"ru"}),json!({"name":"abstract","valueBoolean":false})])).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    assert_eq!(parameter(&body, "display")["valueString"], "Дочерний");
    publish_system(&pool,json!({"url":"urn:native-supplement","version":"1","date":"2020-02-01","content":"supplement","supplements":format!("http://snomed.info/sct|{version}"),"concept":[{"code":"2","designation":[{"language":"de","value":"Kind"}]}]})).await;
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        params(vec![
            json!({"name":"valueSet","resource":resource}),
            json!({"name":"displayLanguage","valueCode":"de"}),
            json!({"name":"filter","valueString":"Kind"}),
            json!({"name":"includeDesignations","valueBoolean":true}),
        ]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 1);
    assert_eq!(body["expansion"]["contains"][0]["display"], "Kind");
    assert!(body["expansion"]["contains"][0]["designation"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["language"] == "de" && d["value"] == "Kind"));
    sqlx::query("UPDATE code_systems SET version=NULL,content=content-'version' WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let(status,body)=request(&app,"POST","/r4/ValueSet/$expand",params(vec![json!({"name":"valueSet","resource":{"resourceType":"ValueSet","compose":{"include":[{"system":"http://snomed.info/sct"}]}}}),json!({"name":"exclude-system","valueCanonical":"http://snomed.info/sct|different-edition"})])).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["expansion"]["total"], 3,
        "Version-specific exclusions must preserve versionless members: {body}"
    );
    cleanup(admin, pool, schema, app).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn metadata_inventory_is_fresh_versioned_and_operation_definitions_preserve_the_pinned_r4_contract(
) {
    let (admin, pool, schema) = hierarchy_test_database().await;
    publish_system(&pool,json!({"url":"urn:inventory","version":"1","content":"complete","hierarchyMeaning":"is-a","property":[{"code":"rank","type":"integer"}],"concept":[{"code":"a"}]})).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for path in [
        "/r4/metadata?mode=terminology",
        "/r4/TerminologyCapabilities",
    ] {
        let (status, body) = request(&app, "GET", path, Value::Null).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["resourceType"], "TerminologyCapabilities");
        let ucum = body["codeSystem"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["uri"] == "http://unitsofmeasure.org")
            .unwrap();
        assert_eq!(ucum["uri"], "http://unitsofmeasure.org");
        assert_eq!(ucum["version"][0]["code"], "2.2");
        assert_eq!(ucum["version"][0]["compositional"], true);
        assert!(body["implementation"].is_object());
        let system = body["codeSystem"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["uri"] == "urn:inventory")
            .unwrap();
        assert_eq!(system["version"][0]["code"], "1");
        assert_eq!(system["version"][0]["isDefault"], true);
        assert_eq!(system["subsumption"], true);
        assert!(system["version"][0]["filter"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["code"] == "rank" && f["op"].as_array().unwrap().contains(&json!("="))));
        assert_eq!(body["expansion"]["parameter"].as_array().unwrap().len(), 21);
    }
    publish_system(
        &pool,
        json!({"url":"urn:inventory","version":"2","content":"complete","concept":[{"code":"b"}]}),
    )
    .await;
    let (status, body) = request(&app, "GET", "/r4/metadata?mode=terminology", Value::Null).await;
    assert_eq!(status, 200, "{body}");
    let system = body["codeSystem"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["uri"] == "urn:inventory")
        .unwrap();
    assert_eq!(system["version"].as_array().unwrap().len(), 2);
    assert_eq!(system["version"][0]["code"], "2");
    assert_eq!(system["version"][0]["isDefault"], true);
    assert_eq!(system["version"][1]["isDefault"], false);
    let contract: Value =
        serde_json::from_str(include_str!("../../../docs/R4_API_CONTRACT.json")).unwrap();
    for operation in contract["operations"].as_array().unwrap() {
        let (status, body) = request(
            &app,
            "GET",
            &format!(
                "/r4/OperationDefinition/{}",
                operation["id"].as_str().unwrap()
            ),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["url"], operation["canonical"]);
        assert_eq!(body["affectsState"], false);
        let inputs: Vec<_> = body["parameter"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["use"] == "in")
            .collect();
        assert_eq!(
            inputs.len(),
            operation["parameters"].as_array().unwrap().len()
        );
        assert!(body["parameter"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["use"] == "out"));
        assert!(!body.to_string().contains("acceptanceTests"));
    }
    cleanup(admin, pool, schema, app).await;
}

fn dependency(element: &str, code: &str) -> Value {
    json!({"name":"dependency","part":[{"name":"element","valueUri":element},{"name":"concept","valueCodeableConcept":{"coding":[{"system":"urn:kind","code":code}]}}]})
}

#[tokio::test]
async fn translate_dependencies_products_and_unmapped_rules_are_real_public_operations() {
    let pool = PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool)));
    let map = json!({"resourceType":"ConceptMap","group":[{"source":"urn:s","target":"urn:t","element":[{"code":"a","target":[{"code":"b","equivalence":"wider","dependsOn":[{"property":"urn:context","system":"urn:kind","value":"in"}],"product":[{"property":"urn:result","system":"urn:kind","value":"out"}]}]},{"code":"negative","target":[{"code":"n","equivalence":"disjoint"}]}],"unmapped":{"mode":"provided"}}]});
    for (reverse, input_system, input_code, extra, expected, status) in [
        (false, "urn:s", "a", vec![], None, 422),
        (
            false,
            "urn:s",
            "a",
            vec![dependency("urn:context", "wrong")],
            None,
            200,
        ),
        (
            false,
            "urn:s",
            "a",
            vec![dependency("urn:context", "in")],
            Some("b"),
            200,
        ),
        (
            true,
            "urn:t",
            "b",
            vec![dependency("urn:result", "out")],
            Some("a"),
            200,
        ),
        (false, "urn:s", "unknown", vec![], Some("unknown"), 200),
        (false, "urn:s", "negative", vec![], None, 200),
    ] {
        let mut fields = vec![
            json!({"name":"conceptMap","resource":map}),
            json!({"name":"coding","valueCoding":{"system":input_system,"code":input_code}}),
            json!({"name":"reverse","valueBoolean":reverse}),
        ];
        fields.extend(extra);
        let (actual, body) =
            request(&app, "POST", "/r4/ConceptMap/$translate", params(fields)).await;
        assert_eq!(actual, status, "{body}");
        if status != 200 {
            assert_eq!(body["resourceType"], "OperationOutcome");
            continue;
        }
        assert_eq!(
            parameter(&body, "result")["valueBoolean"],
            expected.is_some(),
            "{body}"
        );
        if let Some(code) = expected {
            let matches: Vec<_> = body["parameter"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["name"] == "match")
                .collect();
            assert!(
                matches.iter().any(|p| p["part"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|p| p["name"] == "concept" && p["valueCoding"]["code"] == code)),
                "{body}"
            );
            if input_code == "a" || input_code == "b" {
                let product = &parameter(&body, "match")["part"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["name"] == "product")
                    .unwrap()["part"];
                assert_eq!(
                    product[0]["valueUri"],
                    if reverse { "urn:context" } else { "urn:result" }
                );
                assert_eq!(
                    product[1]["valueCoding"]["code"],
                    if reverse { "in" } else { "out" }
                );
            }
        }
    }
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn translate_discovery_scope_filtering_chains_and_fixed_reverse_share_real_content() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    insert_system(&pool,json!({"url":"urn:s","version":"1","content":"complete","concept":[{"code":"a"},{"code":"unmapped"},{"code":"negative"}]})).await;
    insert_system(
        &pool,
        json!({"url":"urn:t","content":"complete","concept":[{"code":"b"},{"code":"fixed"}]}),
    )
    .await;
    insert_set(
        &pool,
        None,
        json!({"include":[{"system":"urn:t","concept":[{"code":"b"}]}]}),
    )
    .await;
    for (url, map) in [
        (
            "urn:base",
            json!({"group":[{"source":"urn:s","target":"urn:t","element":[{"code":"a","target":[{"code":"b","equivalence":"equivalent"}]},{"code":"negative","target":[{"code":"b","equivalence":"disjoint"}]}],"unmapped":{"mode":"fixed","code":"fixed"}}]}),
        ),
        (
            "urn:chain",
            json!({"group":[{"source":"urn:s","target":"urn:t","unmapped":{"mode":"other-map","url":"urn:base|1"}}]}),
        ),
        (
            "urn:cycle",
            json!({"group":[{"source":"urn:s","target":"urn:t","unmapped":{"mode":"other-map","url":"urn:cycle|2"}}]}),
        ),
    ] {
        let mut content = map;
        content["resourceType"] = "ConceptMap".into();
        content["url"] = url.into();
        let version = if url == "urn:cycle" { "2" } else { "1" };
        content["version"] = version.into();
        sqlx::query(
            "INSERT INTO concept_maps (url,version,status,content) VALUES ($1,$3,'active',$2)",
        )
        .bind(url)
        .bind(sqlx::types::Json(content))
        .bind(version)
        .execute(&pool)
        .await
        .unwrap();
    }
    let source = json!({"resourceType":"ValueSet","url":"urn:source-set","version":"1","status":"active","compose":{"include":[{"system":"urn:s"}]}});
    let inspection = term_squid_ingestion::inspect(&source).unwrap();
    let mut tx = pool.begin().await.unwrap();
    term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (query, expected, status) in [
        (
            "code=a&source=urn:source-set&conceptMapVersion=1&targetsystem=urn:t",
            Some("b"),
            200,
        ),
        ("url=urn:chain&code=a&system=urn:s", Some("b"), 200),
        (
            "url=urn:chain&code=unmapped&system=urn:s",
            Some("fixed"),
            200,
        ),
        (
            "url=urn:base&code=fixed&system=urn:t&reverse=true",
            Some("unmapped"),
            200,
        ),
        (
            "url=urn:base&code=a&system=urn:s&target=urn:set",
            Some("b"),
            200,
        ),
        (
            "url=urn:base&code=unmapped&system=urn:s&target=urn:set",
            None,
            200,
        ),
        ("url=urn:cycle&code=a&system=urn:s", None, 400),
        // Restrict discovery by the independently selected map business version.
        ("code=a&system=urn:s&conceptMapVersion=missing", None, 200),
    ] {
        let (actual, body) = request(
            &app,
            "GET",
            &format!("/r4/ConceptMap/$translate?{query}"),
            Value::Null,
        )
        .await;
        assert_eq!(actual, status, "{query}: {body}");
        if status != 200 {
            continue;
        }
        assert_eq!(
            parameter(&body, "result")["valueBoolean"],
            expected.is_some(),
            "{query}: {body}"
        );
        if let Some(code) = expected {
            assert!(
                body["parameter"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|p| p["name"] == "match")
                    .any(|m| m["part"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|p| p["valueCoding"]["code"] == code)),
                "{body}"
            );
        }
        if query.contains("reverse=true") {
            assert!(
                !body.to_string().contains("negative"),
                "Explicit disjoint source must not enter fixed fallback: {body}"
            );
        }
    }
    cleanup(admin, pool, schema, app).await;
}
async fn publish_system(pool: &PgPool, mut resource: Value) -> uuid::Uuid {
    resource["resourceType"] = "CodeSystem".into();
    resource["status"] = "active".into();
    let meta = term_squid_ingestion::inspect(&resource).unwrap();
    let mut tx = pool.begin().await.unwrap();
    let result = term_squid_ingestion::import_resource(&mut tx, &resource, &meta)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    result.id
}
async fn cleanup(admin: PgPool, pool: PgPool, schema: String, app: Router) {
    drop(app);
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn lookup_rich_output_preserves_typed_properties_real_designations_language_date_and_hierarchy(
) {
    let (admin, pool, schema) = hierarchy_test_database().await;
    publish_system(&pool,json!({"url":"urn:rich","version":"1","date":"2020-01-01","content":"complete","hierarchyMeaning":"is-a","concept":[{"code":"parent","concept":[{"code":"a","display":"Alpha","definition":"A definition","designation":[{"language":"ru","use":{"system":"urn:use","code":"display"},"value":"Альфа"}],"property":[{"code":"rank","valueInteger":7}]}]}]})).await;
    publish_system(&pool,json!({"url":"urn:rich","version":"2","date":"2022-01-01","content":"complete","concept":[{"code":"a","display":"New"}]})).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (method,query,body) in [
        ("GET","?system=urn:rich&code=a&date=2021&displayLanguage=ru&property=designation&property=parent&property=definition",Value::Null),
        ("POST","",params(vec![json!({"name":"system","valueUri":"urn:rich"}),json!({"name":"code","valueCode":"a"}),json!({"name":"date","valueDateTime":"2021"}),json!({"name":"displayLanguage","valueCode":"ru"}),json!({"name":"property","valueCode":"designation"}),json!({"name":"property","valueCode":"parent"}),json!({"name":"property","valueCode":"definition"})])),
    ] {
        let (status,body)=request(&app,method,&format!("/r4/CodeSystem/$lookup{query}"),body).await;
        assert_eq!(status,200,"{body}");
        assert_eq!(parameter(&body,"display")["valueString"],"Альфа");
        assert_eq!(parameter(&body,"name")["valueString"],"urn:rich");
        assert_eq!(parameter(&body,"version")["valueString"],"1");
        let designation=&parameter(&body,"designation")["part"];
        assert_eq!(designation[0]["valueCode"],"ru");
        assert_eq!(designation[1]["valueCoding"]["code"],"display");
        assert_eq!(designation[2]["valueString"],"Альфа");
        let properties:Vec<_>=body["parameter"].as_array().unwrap().iter().filter(|p|p["name"]=="property").collect();
        assert_eq!(properties.len(),2,"Unrequested rank property must be omitted: {body}");
        assert!(properties.iter().any(|p|p["part"][0]["valueCode"]=="definition"&&p["part"][1]["valueString"]=="A definition"));
        assert!(properties.iter().any(|p|p["part"][0]["valueCode"]=="parent"&&p["part"][1]["valueCode"]=="parent"));
    }
    let (status, body) = request(
        &app,
        "GET",
        "/r4/CodeSystem/$lookup?system=urn:rich&code=a&property=rank&version=1",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "property")["part"][1]["valueInteger"], 7);
    assert!(!body["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "designation"));
    cleanup(admin, pool, schema, app).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn expansion_options_apply_before_counting_paging_and_transitive_version_resolution() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    for (version, date, display) in [("1", "2020-01-01", "Old"), ("2", "2022-01-01", "New")] {
        publish_system(&pool,json!({"url":"urn:rich","version":version,"date":date,"content":"complete","hierarchyMeaning":"is-a","concept":[{"code":"a","display":display,"designation":[{"language":"ru","use":{"system":"urn:use","code":"display"},"value":"Альфа"}],"concept":[{"code":"b","display":"Beta","property":[{"code":"inactive","valueBoolean":true}]},{"code":"logical","display":"Logical","property":[{"code":"notSelectable","valueBoolean":true}]}]}]})).await;
    }
    publish_system(
        &pool,
        json!({"url":"urn:other","version":"1","content":"complete","concept":[{"code":"other"}]}),
    )
    .await;
    let id = insert_set(
        &pool,
        Some("1"),
        json!({"include":[{"system":"urn:rich","version":"1"},{"system":"urn:other"}]}),
    )
    .await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (query, total, codes) in [
        (
            "activeOnly=true&excludeNotForUI=true&exclude-system=urn:other",
            1,
            vec!["a"],
        ),
        (
            "activeOnly=true&exclude-system=urn:other&offset=1&count=1",
            2,
            vec!["logical"],
        ),
        (
            "activeOnly=true&exclude-system=urn:other&count=0",
            2,
            vec![],
        ),
        (
            "exclude-system=urn:rich&exclude-system=urn:other",
            0,
            vec![],
        ),
    ] {
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/ValueSet/$expand?url=urn:set&{query}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], total, "{body}");
        let actual: Vec<_> = body["expansion"]["contains"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["code"].as_str().unwrap())
            .collect();
        assert_eq!(actual, codes);
    }
    let (status,body)=request(&app,"GET","/r4/ValueSet/$expand?url=urn:set&displayLanguage=ru&filter=%D0%B0%D0%BB%D1%8C&includeDesignations=true&designation=ru&includeDefinition=false&force-system-version=urn:rich%7C2",Value::Null).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 1);
    assert_eq!(body["expansion"]["contains"][0]["display"], "Альфа");
    assert_eq!(body["expansion"]["contains"][0]["version"], "2");
    assert_eq!(
        body["expansion"]["contains"][0]["designation"][0]["value"],
        "Альфа"
    );
    assert!(body.get("compose").is_none());
    assert!(body["expansion"]["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "force-system-version" && p["valueString"] == "urn:rich|2"));
    let (status, body) = request(
        &app,
        "GET",
        "/r4/ValueSet/$expand?url=urn:set&check-system-version=urn:rich%7C2",
        Value::Null,
    )
    .await;
    assert_eq!(status, 400, "{body}");
    let (status,body)=request(&app,"POST","/r4/ValueSet/$expand",params(vec![json!({"name":"valueSet","resource":{"resourceType":"ValueSet","compose":{"include":[{"system":"urn:rich"}]}}}),json!({"name":"system-version","valueCanonical":"urn:rich|1"}),json!({"name":"includeDefinition","valueBoolean":true}),json!({"name":"excludeNested","valueBoolean":false})])).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 3);
    assert_eq!(body["expansion"]["contains"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["expansion"]["contains"][0]["contains"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(body.get("compose").is_some());
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/ValueSet/$expand?context=ValueSet/{id}&contextDirection=incoming&count=0"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 4);
    let(status,body)=request(&app,"POST","/r4/ValueSet/$expand",params(vec![json!({"name":"valueSet","resource":{"resourceType":"ValueSet","compose":{"include":[{"system":"urn:rich"}]}}}),json!({"name":"date","valueDateTime":"2021"}),json!({"name":"includeDesignations","valueBoolean":false}),json!({"name":"designation","valueString":"ru"}),json!({"name":"excludeNested","valueBoolean":true})])).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["contains"][0]["version"], "1");
    assert_eq!(body["expansion"]["contains"][0]["display"], "Old");
    assert!(body["expansion"]["contains"][0]
        .get("designation")
        .is_none());
    let(status,body)=request(&app,"GET","/r4/ValueSet/$expand?url=urn:set&check-system-version=urn:rich%7C1&system-version=urn:rich%7C2&exclude-system=urn:other%7C1",Value::Null).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 3);
    assert!(body["expansion"]["contains"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["version"] == "1"));
    publish_system(&pool,json!({"url":"urn:expressions","content":"complete","concept":[{"code":"atomic"},{"code":"expression","property":[{"code":"postCoordinated","valueBoolean":true}]}]})).await;
    let(status,body)=request(&app,"POST","/r4/ValueSet/$expand",params(vec![json!({"name":"valueSet","resource":{"resourceType":"ValueSet","compose":{"include":[{"system":"urn:expressions"}]}}}),json!({"name":"excludePostCoordinated","valueBoolean":true})])).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 1);
    assert_eq!(body["expansion"]["contains"][0]["code"], "atomic");
    cleanup(admin, pool, schema, app).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn validation_date_language_context_and_system_inference_use_the_selected_definitions() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    for (version, date, display) in [("1", "2020-01-01", "Old"), ("2", "2022-01-01", "New")] {
        publish_system(&pool,json!({"url":"urn:rich","version":version,"date":date,"content":"complete","concept":[{"code":"a","display":display,"designation":[{"language":"ru","use":{"system":"urn:use","code":"display"},"value":"Альфа"},{"language":"ru","value":"Синоним"}]}]})).await;
    }
    let id = insert_set(&pool, Some("1"), json!({"include":[{"system":"urn:rich"}]})).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (path,version) in [
        ("/r4/CodeSystem/$validate-code?url=urn:rich&code=a&date=2021&displayLanguage=ru&display=%D0%90%D0%BB%D1%8C%D1%84%D0%B0".into(),"1"),
        ("/r4/ValueSet/$validate-code?url=urn:set%7C1&code=a&date=2021&displayLanguage=ru-RU&display=%D0%A1%D0%B8%D0%BD%D0%BE%D0%BD%D0%B8%D0%BC".into(),"1"),
        ("/r4/ValueSet/$validate-code?url=urn:set&code=a".into(),"2"),
        (format!("/r4/ValueSet/$validate-code?context=ValueSet/{id}&code=a"),"2"),
    ] {
        let(status,body)=request(&app,"GET",&path,Value::Null).await;assert_eq!(status,200,"{body}");assert_eq!(parameter(&body,"result")["valueBoolean"],true,"{body}");assert_eq!(parameter(&body,"version")["valueString"],version,"{body}");
    }
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$validate-code",
        params(vec![
            json!({"name":"url","valueUri":"urn:set|1"}),
            json!({"name":"code","valueCode":"a"}),
            json!({"name":"date","valueDateTime":"2021"}),
            json!({"name":"displayLanguage","valueCode":"ru"}),
            json!({"name":"display","valueString":"Альфа"}),
        ]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    assert_eq!(parameter(&body, "display")["valueString"], "Альфа");
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$validate-code",
        params(vec![
            json!({"name":"url","valueUri":"urn:set"}),
            json!({"name":"coding","valueCoding":{"code":"a"}}),
        ]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    publish_system(
        &pool,
        json!({"url":"urn:other","content":"complete","concept":[{"code":"a"}]}),
    )
    .await;
    sqlx::query("UPDATE value_sets SET content=jsonb_set(content,'{compose,include}',content->'compose'->'include'||'[ {\"system\":\"urn:other\"} ]'::jsonb) WHERE id=$1").bind(id).execute(&pool).await.unwrap();
    let (status, body) = request(
        &app,
        "GET",
        "/r4/ValueSet/$validate-code?url=urn:set&code=a",
        Value::Null,
    )
    .await;
    assert_eq!(status, 400, "Ambiguous systems must not be guessed: {body}");
    cleanup(admin, pool, schema, app).await;
}

#[tokio::test]
async fn inline_validation_localizes_real_designations_and_ucum_candidates_preserve_any_valid_semantics(
) {
    let pool = PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool)));
    let(status,body)=request(&app,"POST","/r4/CodeSystem/$validate-code",params(vec![json!({"name":"codeSystem","resource":{"resourceType":"CodeSystem","content":"complete","concept":[{"code":"a","display":"Alpha","designation":[{"language":"ru","value":"Альфа"}]}]}}),json!({"name":"code","valueCode":"a"}),json!({"name":"display","valueString":"Альфа"}),json!({"name":"displayLanguage","valueCode":"ru"})])).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    assert_eq!(parameter(&body, "display")["valueString"], "Альфа");
    let(status,body)=request(&app,"POST","/r4/CodeSystem/$validate-code",params(vec![json!({"name":"url","valueUri":"http://unitsofmeasure.org"}),cc(json!([{"system":"http://unitsofmeasure.org","version":"unavailable","code":"m"},{"system":"http://unitsofmeasure.org","code":"m"}]))])).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn subsumption_coding_pairs_preserve_all_outcomes_versions_and_instance_identity() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id=insert_system(&pool,json!({"url":"urn:hierarchy","version":"1","content":"complete","hierarchyMeaning":"is-a","concept":[{"code":"root","concept":[{"code":"child"}]},{"code":"other"}]})).await;
    insert_system(&pool,json!({"url":"urn:hierarchy","version":"2","content":"complete","hierarchyMeaning":"is-a","concept":[{"code":"child","concept":[{"code":"root"}]}]})).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for _ in 0..2 {
        for (a, b, outcome) in [
            ("root", "child", "subsumes"),
            ("child", "root", "subsumed-by"),
            ("root", "root", "equivalent"),
            ("child", "other", "not-subsumed"),
        ] {
            for instance in [false, true] {
                for mixed in [false, true] {
                    let mut fields = vec![
                        if mixed {
                            json!({"name":"codeA","valueCode":a})
                        } else {
                            json!({"name":"codingA","valueCoding":{"system":"urn:hierarchy","version":"1","code":a}})
                        },
                        json!({"name":"codingB","valueCoding":{"system":"urn:hierarchy","version":"1","code":b}}),
                    ];
                    if !instance && mixed {
                        fields.push(json!({"name":"system","valueUri":"urn:hierarchy"}));
                    }
                    let path = if instance {
                        format!("/r4/CodeSystem/{id}/$subsumes")
                    } else {
                        "/r4/CodeSystem/$subsumes".into()
                    };
                    let(status,body)=request(&app,"POST","/r4/CodeSystem/$validate-code",params(vec![json!({"name":"codeSystem","resource":{"resourceType":"CodeSystem","date":"2030-01-01","content":"complete","concept":[{"code":"a"}]}}),json!({"name":"code","valueCode":"a"}),json!({"name":"date","valueDateTime":"2021"})])).await;
                    assert_eq!(
                        status, 400,
                        "Historical validation cannot use a future definition: {body}"
                    );
                    let (status, body) = request(&app, "POST", &path, params(fields)).await;
                    assert_eq!(status, 200, "{body}");
                    assert_eq!(parameter(&body, "outcome")["valueCode"], outcome);
                }
            }
        }
    }
    for (system, version, expected) in [("urn:wrong", "1", 400), ("urn:hierarchy", "2", 200)] {
        let(status,body)=request(&app,"GET",&format!("/r4/CodeSystem/{id}/$subsumes?system={system}&version={version}&codeA=root&codeB=child"),Value::Null).await;
        assert_eq!(status, expected, "{body}");
    }
    let (status, body) = request(
        &app,
        "POST",
        "/r4/CodeSystem/$subsumes",
        params(vec![
            json!({"name":"system","valueUri":"urn:hierarchy"}),
            json!({"name":"codeA","valueCode":"root"}),
            json!({"name":"codingB","valueCoding":{"system":"urn:other","code":"child"}}),
        ]),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    cleanup(admin, pool, schema, app).await;
}

#[tokio::test]
async fn inline_translation_handles_coding_codeable_concept_versions_and_source_provenance_without_database(
) {
    let pool = PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool)));
    let map = json!({"resourceType":"ConceptMap","url":"urn:map","version":"map-v1","sourceUri":"urn:source-set","targetUri":"urn:target-set","group":[
        {"source":"urn:s","sourceVersion":"1","target":"urn:t","targetVersion":"A","element":[{"code":"x","target":[{"code":"a","equivalence":"wider"}]}]},
        {"source":"urn:s","sourceVersion":"2","target":"urn:t","targetVersion":"B","element":[{"code":"x","target":[{"code":"b","equivalence":"equivalent"}]}]}
    ]});
    for (version, code, output_version) in [("1", "a", "A"), ("2", "b", "B")] {
        let(status,body)=request(&app,"POST","/r4/ConceptMap/$translate",params(vec![json!({"name":"conceptMap","resource":map}),json!({"name":"coding","valueCoding":{"system":"urn:s","version":version,"code":"x"}}),json!({"name":"source","valueUri":"urn:source-set"}),json!({"name":"target","valueUri":"urn:target-set"})])).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], true);
        let parts = parameter(&body, "match")["part"].as_array().unwrap();
        assert!(parts.iter().any(|p| p["name"] == "concept"
            && p["valueCoding"]["code"] == code
            && p["valueCoding"]["version"] == output_version));
        assert!(parts
            .iter()
            .any(|p| p["name"] == "source" && p["valueUri"] == "urn:map|map-v1"));
    }
    let(status,body)=request(&app,"POST","/r4/ConceptMap/$translate",params(vec![json!({"name":"conceptMap","resource":map}),cc(json!([{"system":"urn:unknown","code":"x"},{"system":"urn:s","version":"1","code":"x"},{"system":"urn:s","version":"1","code":"x"}]))])).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["parameter"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["name"] == "match")
            .count(),
        1
    );
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ConceptMap/$translate",
        params(vec![
            json!({"name":"conceptMap","resource":map}),
            json!({"name":"code","valueCode":"x"}),
            json!({"name":"version","valueString":"1"}),
        ]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert!(parameter(&body, "match")["part"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["valueCoding"]["code"] == "a"));
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ConceptMap/$translate",
        params(vec![
            json!({"name":"conceptMap","resource":map}),
            json!({"name":"coding","valueCoding":{"system":"urn:t","version":"A","code":"a"}}),
            json!({"name":"reverse","valueBoolean":true}),
        ]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let parts = parameter(&body, "match")["part"].as_array().unwrap();
    assert!(parts
        .iter()
        .any(|p| p["valueCoding"]["code"] == "x" && p["valueCoding"]["version"] == "1"));
    assert!(parts.iter().any(|p| p["valueCode"] == "narrower"));
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn codeable_concept_validation_uses_any_valid_candidate_and_preserves_unknown_negatives() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id=insert_system(&pool,json!({"url":"urn:codes","version":"1","content":"complete","concept":[{"code":"A","display":"Alpha"},{"code":"logical","display":"Logical","property":[{"code":"notSelectable","valueBoolean":true}]}]})).await;
    sqlx::query("UPDATE concepts SET properties=$2 WHERE code_system_id=$1 AND code='logical'")
        .bind(id)
        .bind(sqlx::types::Json(
            json!([{"code":"notSelectable","valueBoolean":true}]),
        ))
        .execute(&pool)
        .await
        .unwrap();
    let set = insert_set(
        &pool,
        Some("set-v1"),
        json!({"include":[{"system":"urn:codes"}]}),
    )
    .await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (path, selectors) in [
        (
            "/r4/CodeSystem/$validate-code".to_string(),
            vec![json!({"name":"url","valueUri":"urn:codes"})],
        ),
        (format!("/r4/CodeSystem/{id}/$validate-code"), vec![]),
        (
            "/r4/ValueSet/$validate-code".to_string(),
            vec![json!({"name":"url","valueUri":"urn:set"})],
        ),
        (format!("/r4/ValueSet/{set}/$validate-code"), vec![]),
        (
            "/r4/ValueSet/$validate-code".to_string(),
            vec![
                json!({"name":"valueSet","resource":{"resourceType":"ValueSet","compose":{"include":[{"system":"urn:codes"}]}}}),
            ],
        ),
    ] {
        for (codings, valid) in [
            (
                json!([{"system":"urn:other","code":"missing"},{"system":"urn:codes","version":"1","code":"A","display":"Alpha"}]),
                true,
            ),
            (
                json!([{"system":"urn:codes","version":"1","code":"missing"},{"system":"urn:codes","version":"1","code":"A","display":"Wrong"}]),
                false,
            ),
        ] {
            let mut fields = selectors.clone();
            fields.push(cc(codings));
            let (status, body) = request(&app, "POST", &path, params(fields)).await;
            assert_eq!(status, 200, "{path}: {body}");
            assert_eq!(parameter(&body, "result")["valueBoolean"], valid);
        }
        for (allow, valid) in [(true, true), (false, false)] {
            let mut fields = selectors.clone();
            fields.extend([
                cc(json!([{"system":"urn:codes","code":"logical"}])),
                json!({"name":"abstract","valueBoolean":allow}),
            ]);
            let (status, body) = request(&app, "POST", &path, params(fields)).await;
            assert_eq!(status, 200, "{path}: {body}");
            assert_eq!(parameter(&body, "result")["valueBoolean"], valid);
        }
    }
    for allow in [true, false] {
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/CodeSystem/{id}/$validate-code?code=logical&abstract={allow}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], allow);
    }
    let(status,body)=request(&app,"POST","/r4/CodeSystem/$validate-code",params(vec![json!({"name":"url","valueUri":"urn:codes"}),cc(json!([{"system":"urn:codes","version":"unavailable","code":"A"},{"system":"urn:codes","version":"1","code":"missing"}]))])).await;
    assert_eq!(status, 404, "{body}");
    cleanup(admin, pool, schema, app).await;
}

#[tokio::test]
async fn inline_codesystem_validation_supports_complete_fragment_case_and_abstract_semantics() {
    let pool = PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool)));
    for mode in ["complete", "fragment"] {
        let system = json!({"resourceType":"CodeSystem","url":"urn:inline","content":mode,"caseSensitive":false,"concept":[{"code":"A","display":"Alpha","property":[{"code":"notSelectable","valueBoolean":true}]}]});
        for (code, allow, valid, status) in [
            ("a", true, true, 200),
            ("a", false, false, 200),
            (
                "missing",
                true,
                false,
                if mode == "complete" { 200 } else { 400 },
            ),
        ] {
            let (status_actual, body) = request(
                &app,
                "POST",
                "/r4/CodeSystem/$validate-code",
                params(vec![
                    json!({"name":"codeSystem","resource":system}),
                    json!({"name":"code","valueCode":code}),
                    json!({"name":"abstract","valueBoolean":allow}),
                ]),
            )
            .await;
            assert_eq!(status_actual, status, "{body}");
            if status == 200 {
                assert_eq!(parameter(&body, "result")["valueBoolean"], valid);
            }
        }
    }
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn codeable_concept_never_combines_two_concurrently_published_states() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id=insert_system(&pool,json!({"url":"urn:codes","content":"complete","concept":[{"code":"A","display":"OldA"},{"code":"B","display":"OldB"}]})).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let writer_pool = pool.clone();
    let writer = tokio::spawn(async move {
        for generation in 0..60 {
            let mut tx = writer_pool.begin().await.unwrap();
            let new = generation % 2 == 0;
            sqlx::query("UPDATE concepts SET display=CASE WHEN code='A' THEN $2 ELSE $3 END WHERE code_system_id=$1").bind(id).bind(if new {"NewA"}else{"OldA"}).bind(if new {"NewB"}else{"OldB"}).execute(&mut *tx).await.unwrap();
            tx.commit().await.unwrap();
            tokio::task::yield_now().await;
        }
    });
    for _ in 0..60 {
        let(status,body)=request(&app,"POST","/r4/CodeSystem/$validate-code",params(vec![json!({"name":"url","valueUri":"urn:codes"}),cc(json!([{"system":"urn:codes","code":"A","display":"NewA"},{"system":"urn:codes","code":"B","display":"OldB"}]))])).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body,"result")["valueBoolean"],true,"Each complete generation has a valid coding; a mixed snapshot can produce a false negative: {body}");
        assert!(matches!(
            parameter(&body, "display")["valueString"].as_str(),
            Some("NewA" | "OldB")
        ));
    }
    writer.await.unwrap();
    cleanup(admin, pool, schema, app).await;
}

#[tokio::test]
async fn fhir_http_negotiation_obeys_quality_format_versions_and_error_envelopes() {
    use axum::{
        body::Body,
        http::{header, Request},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let pool = PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool)));
    for (accept, path, status, content_type) in [
        (
            "application/fhir+json;q=0.9;extension=ignored",
            "/r4/metadata",
            200,
            "application/fhir+json",
        ),
        (
            "application/fhir+json;q=0.1234",
            "/r4/metadata",
            400,
            "application/fhir+json",
        ),
        (
            "application/fhir+json",
            "/r4/metadata",
            200,
            "application/fhir+json",
        ),
        ("application/json", "/r4/metadata", 200, "application/json"),
        ("text/json", "/r4/metadata", 200, "text/json"),
        (
            "application/xml,application/fhir+json;q=0.5",
            "/r4/metadata",
            200,
            "application/fhir+json",
        ),
        (
            "application/fhir+json;q=0,application/json;q=0.8",
            "/r4/metadata",
            200,
            "application/json",
        ),
        (
            "application/fhir+json;q=0,*/*;q=0.9",
            "/r4/metadata",
            200,
            "application/json",
        ),
        (
            "application/fhir+json;fhirVersion=4.0",
            "/r4/metadata",
            200,
            "application/fhir+json; fhirVersion=4.0",
        ),
        (
            "application/xml",
            "/r4/metadata",
            406,
            "application/fhir+json",
        ),
        (
            "application/fhir+json;fhirVersion=5.0",
            "/r4/metadata",
            406,
            "application/fhir+json",
        ),
        (
            "application/fhir+json;q=0",
            "/r4/metadata",
            406,
            "application/fhir+json",
        ),
        (
            "application/xml",
            "/r4/metadata?_format=json",
            200,
            "application/fhir+json",
        ),
        ("application/json", "/r4/unknown", 404, "application/json"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header(header::ACCEPT, accept)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status, "{accept} {path}");
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            content_type,
            "{accept} {path}"
        );
        let body: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        if status >= 400 {
            assert_eq!(body["resourceType"], "OperationOutcome");
        }
    }
    for media in [
        "application/fhir+json;fhirVersion=5.0",
        "application/fhir+xml",
        "application/json;charset=utf-16",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/r4/CodeSystem/$lookup")
                    .header(header::CONTENT_TYPE, media)
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 415, "{media}");
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/r4/CodeSystem/$lookup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 405);
    assert!(response.headers()[header::ALLOW]
        .to_str()
        .unwrap()
        .contains("POST"));
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["resourceType"], "OperationOutcome");
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn supplements_enrich_base_codes_languages_and_filters_without_independent_code_identities() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    publish_system(&pool,json!({"url":"urn:base","version":"1","date":"2020-01-01","caseSensitive":false,"content":"complete","concept":[{"code":"A","display":"Original"}]})).await;
    let set=insert_set(&pool,Some("1"),json!({"include":[{"system":"urn:base","filter":[{"property":"group","op":"=","value":"selected"}]}]})).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, _) = request(
        &app,
        "GET",
        &format!("/r4/ValueSet/{set}/$expand"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 422);
    let supplement = json!({"url":"urn:supplement","version":"1","date":"2020-02-01","content":"supplement","supplements":"urn:base|1","property":[{"code":"group","type":"code"}],"concept":[{"code":"a","designation":[{"language":"ru","value":"Альфа"}],"property":[{"code":"group","valueCode":"selected"}]}]});
    let mut invalid_display = supplement.clone();
    invalid_display["resourceType"] = "CodeSystem".into();
    invalid_display["status"] = "active".into();
    invalid_display["concept"][0]["display"] = "Replacement".into();
    assert!(
        term_squid_ingestion::inspect(&invalid_display).is_err(),
        "All providers must reject supplemental display overrides before activation"
    );
    publish_system(&pool, supplement.clone()).await;
    for _ in 0..2 {
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/ValueSet/{set}/$expand?displayLanguage=ru&includeDesignations=true"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], 1);
        assert_eq!(body["expansion"]["contains"][0]["system"], "urn:base");
        assert_eq!(body["expansion"]["contains"][0]["code"], "A");
        assert_eq!(body["expansion"]["contains"][0]["display"], "Альфа");
        let(status,body)=request(&app,"GET","/r4/CodeSystem/$lookup?system=urn:base&code=A&displayLanguage=ru&property=group&property=designation",Value::Null).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "display")["valueString"], "Альфа");
        assert_eq!(
            parameter(&body, "property")["part"][1]["valueCode"],
            "selected"
        );
        let(status,body)=request(&app,"GET","/r4/CodeSystem/$validate-code?url=urn:base&code=A&displayLanguage=ru&display=%D0%90%D0%BB%D1%8C%D1%84%D0%B0",Value::Null).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    }
    for operation in ["$lookup", "$validate-code"] {
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/CodeSystem/{operation}?system=urn:supplement&code=a"),
            Value::Null,
        )
        .await;
        assert_eq!(
            status, 400,
            "Supplement URL must never become Coding.system: {body}"
        );
    }
    let mut invalid = supplement.clone();
    invalid["url"] = "urn:bad-supplement".into();
    invalid["concept"][0]["code"] = "NEW".into();
    invalid["resourceType"] = "CodeSystem".into();
    invalid["status"] = "active".into();
    let meta = term_squid_ingestion::inspect(&invalid).unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(
        term_squid_ingestion::import_resource(&mut tx, &invalid, &meta)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM code_systems WHERE url='urn:bad-supplement'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    // A new immutable supplement version changes subsequent prepared reads across generations.
    let mut updated = supplement;
    updated["version"] = "2".into();
    updated["concept"][0]["property"][0]["valueCode"] = "other".into();
    publish_system(&pool, updated).await;
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/ValueSet/{set}/$expand"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 0);
    publish_system(&pool,json!({"url":"urn:ucum-supplement","version":"1","date":"2020-01-01","content":"supplement","supplements":"http://unitsofmeasure.org|2.2","concept":[{"code":"mg","designation":[{"language":"de","value":"Milligramm"}]}]})).await;
    for operation in ["$lookup", "$validate-code"] {
        let(status,body)=request(&app,"GET",&format!("/r4/CodeSystem/{operation}?system=http://unitsofmeasure.org&code=mg&displayLanguage=de{}",if operation=="$validate-code" {"&display=Milligramm"}else{""}),Value::Null).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "display")["valueString"], "Milligramm");
        if operation == "$validate-code" {
            assert_eq!(parameter(&body, "result")["valueBoolean"], true);
        }
    }
    cleanup(admin, pool, schema, app).await;
}
