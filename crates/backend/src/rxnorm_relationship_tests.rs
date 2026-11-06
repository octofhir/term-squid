//! Generated relationship rows verify R4 endpoint direction, not clinical mappings.
use super::*;

#[allow(clippy::too_many_arguments)]
async fn row(
    pool: &PgPool,
    id: Uuid,
    ordinal: i64,
    cui1: &str,
    aui1: &str,
    rel: &str,
    cui2: &str,
    aui2: &str,
    rela: &str,
    sab: &str,
    suppress: &str,
) {
    let stype1 = if aui1.is_empty() { "CUI" } else { "AUI" };
    let stype2 = if aui2.is_empty() { "CUI" } else { "AUI" };
    let fields = json!({"RXCUI1":cui1,"RXAUI1":aui1,"STYPE1":stype1,"REL":rel,"RXCUI2":cui2,"RXAUI2":aui2,"STYPE2":stype2,"RELA":rela,"RUI":format!("r{ordinal}"),"SRUI":"","SAB":sab,"SL":"","RG":"","DIR":"","SUPPRESS":suppress,"CVF":""});
    sqlx::query("INSERT INTO rxnorm_relationships(code_system_id,source_row,rxcui1,rxaui1,stype1,rel,rxcui2,rxaui2,stype2,rela,rui,srui,sab,sl,rg,dir,suppress,cvf,fields) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'',$12,'','','',$13,'',$14)")
        .bind(id).bind(ordinal).bind(cui1).bind(aui1).bind(stype1).bind(rel).bind(cui2).bind(aui2).bind(stype2).bind(rela).bind(format!("r{ordinal}")).bind(sab).bind(suppress).bind(sqlx::types::Json(fields)).execute(pool).await.unwrap();
}

async fn loaded(pool: &PgPool, id: Uuid, version: &str, rows: i64) {
    let descriptor = json!({"file":"RXNREL.RRF","sha256":"c".repeat(64),"rows":rows,"bytes":rows*100,"version":version,"fullMonthly":true});
    sqlx::query("UPDATE rxnorm_releases SET relationships_loaded=true,relationship_source=$2 WHERE code_system_id=$1")
        .bind(id).bind(sqlx::types::Json(descriptor)).execute(pool).await.unwrap();
}

async fn seed(pool: &PgPool, id: Uuid, version: &str) {
    atom(
        pool,
        id,
        "444",
        "400",
        "VANDF",
        "SCD",
        "Source-only anchor",
        "N",
    )
    .await;
    for (ordinal, cui1, aui1, rel, cui2, aui2, rela, sab, suppress) in [
        (
            1,
            "111",
            "",
            "RO",
            "222",
            "",
            "has_ingredient",
            "RXNORM",
            "",
        ),
        (
            2,
            "222",
            "",
            "RO",
            "111",
            "",
            "ingredient_of",
            "RXNORM",
            "N",
        ),
        (
            3,
            "111",
            "",
            "RO",
            "222",
            "",
            "has_ingredient",
            "RXNORM",
            "N",
        ),
        (4, "", "305", "RO", "", "307", "has_ingredient", "VANDF", ""),
        (5, "111", "", "RO", "400", "", "has_dose_form", "VANDF", "N"),
        (
            6,
            "111",
            "",
            "RO",
            "222",
            "",
            "has_doseformgroup",
            "RXNORM",
            "Y",
        ),
        (
            7,
            "222",
            "",
            "RO",
            "222",
            "",
            "has_tradename",
            "RXNORM",
            "E",
        ),
        (
            8,
            "300",
            "",
            "RO",
            "222",
            "",
            "has_ingredient",
            "RXNORM",
            "N",
        ),
        (9, "111", "", "SY", "222", "", "", "RXNORM", "N"),
        (
            10,
            "222",
            "",
            "RQ",
            "111",
            "",
            "unsupported_raw",
            "VANDF",
            "",
        ),
        (11, "", "111", "RO", "", "306", "has_form", "RXNORM", "N"),
        (12, "C000001", "A000001", "SY", "111", "", "", "RXNORM", ""),
    ] {
        row(
            pool, id, ordinal, cui1, aui1, rel, cui2, aui2, rela, sab, suppress,
        )
        .await;
    }
    loaded(pool, id, version, 12).await;
}

fn selected(property: &str, op: &str, value: &str) -> Value {
    let mut selection = active();
    selection["filter"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":property,"op":op,"value":value}));
    selection
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationships_use_official_endpoint_direction_for_cui_and_aui() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    seed(&pool, id, VERSION).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (property, target, expected) in [
        ("has_ingredient", "CUI:222", vec!["111"]),
        ("ingredient_of", "CUI:111", vec!["222"]),
        ("has_ingredient", "CUI:111", vec![]),
        ("ingredient_of", "CUI:222", vec![]),
        ("has_ingredient", "AUI:307", vec!["111"]),
        ("has_ingredient", "AUI:305", vec![]),
        ("SY", "CUI:222", vec!["111"]),
        ("RO", "CUI:111", vec!["222"]),
    ] {
        let (status, body) = expand(&app, set(selected(property, "=", target)), vec![]).await;
        assert_eq!(status, 200, "{property} {target}: {body}");
        assert_eq!(body["expansion"]["total"], expected.len());
        assert_eq!(codes_or_empty(&body), expected);
    }
    cleanup(admin, pool, schema).await;
}

fn codes_or_empty(body: &Value) -> Vec<&str> {
    body["expansion"]["contains"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| c["code"].as_str().unwrap())
        .collect()
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationship_targets_use_raw_source_and_do_not_infer_transitive_paths() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    seed(&pool, id, VERSION).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (property, target) in [("has_dose_form", "CUI:400"), ("has_form", "AUI:306")] {
        let (status, body) = expand(&app, set(selected(property, "=", target)), vec![]).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(codes(&body), ["111"]);
    }
    // A known source-only CUI is a valid anchor, but not an active candidate.
    let (status, body) = member(&app, set(selected("has_ingredient", "=", "CUI:222")), "300").await;
    assert_eq!(status, 422, "{body}");
    row(
        &pool,
        id,
        13,
        "222",
        "",
        "RO",
        "400",
        "",
        "consists_of",
        "RXNORM",
        "N",
    )
    .await;
    row(
        &pool,
        id,
        14,
        "400",
        "",
        "RO",
        "111",
        "",
        "has_ingredient",
        "RXNORM",
        "N",
    )
    .await;
    let (status, body) = expand(
        &app,
        set(selected("has_ingredient", "=", "CUI:111")),
        vec![],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 0);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationship_lists_preserve_distinct_totals_pages_and_known_negative_membership() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    seed(&pool, id, VERSION).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let resource = set(selected("RO", "in", "CUI:111,222,CUI:222"));
    for (offset, count, total, expected) in [
        (0, 1, 2, vec!["111"]),
        (1, 1, 2, vec!["222"]),
        (2, 1, 2, vec![]),
        (0, 0, 2, vec![]),
        (i64::MAX, 1, 2, vec![]),
    ] {
        let (status, body) = expand(
            &app,
            resource.clone(),
            vec![
                json!({"name":"offset","valueInteger":offset}),
                json!({"name":"count","valueInteger":count}),
            ],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], total);
        assert_eq!(codes_or_empty(&body), expected);
    }
    for (code, expected) in [("111", true), ("222", false)] {
        let (status, body) =
            member(&app, set(selected("has_ingredient", "=", "CUI:222")), code).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], expected);
    }
    let (status, body) = member(&app, resource, "999999").await;
    assert_eq!(status, 422, "{body}");
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationship_missing_source_is_not_loaded_empty_or_empty_predicate_success() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let mut selection = selected("has_ingredient", "=", "CUI:222");
    selection["filter"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"TTY","op":"=","value":"none"}));
    let combined = json!({"resourceType":"ValueSet","compose":{"include":[{"system":SYSTEM,"concept":[{"code":"111"}]},selection]}});
    for resource in [set(selected("has_ingredient", "=", "CUI:222")), combined] {
        let (status, body) = expand(
            &app,
            resource.clone(),
            vec![json!({"name":"count","valueInteger":0})],
        )
        .await;
        assert_eq!(status, 422, "{body}");
        let (status, body) = member(&app, resource, "111").await;
        assert_eq!(status, 422, "{body}");
    }
    loaded(&pool, id, VERSION, 0).await;
    let (status, body) = expand(
        &app,
        set(selected("has_ingredient", "=", "CUI:222")),
        vec![],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 0);
    let (status, body) = member(&app, set(selected("has_ingredient", "=", "CUI:222")), "111").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationship_malformed_unknown_targets_and_missing_active_scope_are_explicit() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    seed(&pool, id, VERSION).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (property, op, value) in [
        ("RO", "=", "222"),
        ("RO", "=", "CUI:999999"),
        ("RO", "in", "CUI:222,999999"),
        ("RO", "=", "AUI:999999"),
        ("RO", "=", "AUI:A123"),
        ("RO", "in", "CUI:222,AUI:307"),
        ("RO", "in", "CUI:222,"),
        ("RO", "regex", ".*"),
        ("has_ingredient", "is-a", "CUI:222"),
        ("RQ", "=", "CUI:111"),
        ("unsupported_raw", "=", "CUI:111"),
        ("has_ingredient';DROP", "=", "CUI:222"),
    ] {
        let (status, body) = expand(
            &app,
            set(selected(property, op, value)),
            vec![json!({"name":"count","valueInteger":0})],
        )
        .await;
        assert_eq!(status, 422, "{property} {value}: {body}");
    }
    let (status,body)=expand(&app,set(json!({"system":SYSTEM,"filter":[{"property":"has_ingredient","op":"=","value":"CUI:222"}]})),vec![]).await;
    assert_eq!(status, 422, "{body}");
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationship_current_safe_suppression_and_all_closed_properties_use_source_rows() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    seed(&pool, id, VERSION).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for property in ["has_doseformgroup", "has_tradename"] {
        let (status, body) = expand(&app, set(selected(property, "=", "CUI:222")), vec![]).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], 0);
    }
    for (index, property) in term_squid_core::rxnorm::RELATION_TYPES
        .iter()
        .chain(term_squid_core::rxnorm::RELATION_LABELS)
        .enumerate()
    {
        let (rel, rela) = if term_squid_core::rxnorm::RELATION_TYPES.contains(property) {
            (*property, "")
        } else {
            ("RO", *property)
        };
        row(
            &pool,
            id,
            100 + index as i64,
            "111",
            "",
            rel,
            "222",
            "",
            rela,
            "VANDF",
            "N",
        )
        .await;
        let (status, body) = expand(&app, set(selected(property, "=", "CUI:222")), vec![]).await;
        assert_eq!(status, 200, "{property}: {body}");
        assert_eq!(codes(&body), ["111"]);
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationship_versions_instance_capabilities_and_loaded_empty_stay_edition_scoped() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let old = release(&pool, VERSION, "2026-03-02").await;
    let new_version = "04062026";
    let new = release(&pool, new_version, "2026-04-06").await;
    seed(&pool, new, new_version).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, body) = expand(
        &app,
        set(selected("has_ingredient", "=", "CUI:222")),
        vec![],
    )
    .await;
    assert_eq!(status, 422, "{body}");
    let mut newer = selected("has_ingredient", "=", "CUI:222");
    newer["version"] = new_version.into();
    let (status, body) = expand(&app, set(newer), vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["111"]);
    let (status, body) = request(&app, "GET", "/r4/metadata?mode=terminology", Value::Null).await;
    assert_eq!(status, 200, "{body}");
    let system = body["codeSystem"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["uri"] == SYSTEM)
        .unwrap();
    for (version, available) in [(VERSION, false), (new_version, true)] {
        let version = system["version"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["code"] == version)
            .unwrap();
        let text = version["filter"].to_string();
        assert_eq!(text.contains("has_ingredient"), available, "{version}");
        let scope = version["extension"].to_string();
        assert_eq!(
            scope.contains("RXNREL relationships: loaded"),
            available,
            "{scope}"
        );
        assert!(!scope.contains("relationships and mappings are unavailable"));
    }
    // A stored instance cannot acquire another edition's source facts.
    let instance = Uuid::new_v4();
    let mut source = set(selected("has_ingredient", "=", "CUI:222"));
    source["url"] = "urn:rxnorm:relation:old".into();
    sqlx::query("INSERT INTO value_sets(id,url,status,content) VALUES($1,'urn:rxnorm:relation:old','active',$2)")
        .bind(instance).bind(sqlx::types::Json(source)).execute(&pool).await.unwrap();
    let (status, response) = request(
        &app,
        "GET",
        &format!("/r4/ValueSet/{instance}/$expand"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 422, "{response}");
    loaded(&pool, old, VERSION, 0).await;
    let (status, body) = expand(
        &app,
        set(selected("has_ingredient", "=", "CUI:222")),
        vec![],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 0);
    let (status, response) = request(
        &app,
        "GET",
        &format!("/r4/ValueSet/{instance}/$expand"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{response}");
    assert_eq!(response["expansion"]["total"], 0);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationship_freshness_observes_only_committed_selected_release_rows() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    seed(&pool, id, VERSION).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let query = set(selected("has_ingredient", "=", "CUI:222"));
    let (status, body) = expand(&app, query.clone(), vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["111"]);
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "DELETE FROM rxnorm_relationships WHERE code_system_id=$1 AND rela='has_ingredient'",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .unwrap();
    let (status, body) = expand(&app, query.clone(), vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["111"]);
    tx.rollback().await.unwrap();
    let (status, body) = expand(&app, query.clone(), vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["111"]);
    sqlx::query(
        "DELETE FROM rxnorm_relationships WHERE code_system_id=$1 AND rela='has_ingredient'",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    for enabled in [true, false] {
        let store = if enabled {
            PostgresStore::new(pool.clone())
        } else {
            PostgresStore::without_prepared_cache(pool.clone())
        };
        let parity = api::create_router(Arc::new(store));
        let (status, body) = expand(&parity, query.clone(), vec![]).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], 0);
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_relationships_compose_with_auxiliary_finite_and_imported_exclusions() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    seed(&pool, id, VERSION).await;
    super::auxiliary::seed_aux(&pool, id, VERSION, true, true, false).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let mut selection = selected("has_ingredient", "=", "CUI:222");
    selection["filter"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"STY","op":"=","value":"T109"}));
    let mut imported = set(selection);
    imported["url"] = "urn:rxnorm:relation:imported".into();
    sqlx::query("INSERT INTO value_sets(url,status,content) VALUES('urn:rxnorm:relation:imported','active',$1)")
        .bind(sqlx::types::Json(imported)).execute(&pool).await.unwrap();
    let resource = json!({"resourceType":"ValueSet","compose":{"include":[{"valueSet":["urn:rxnorm:relation:imported"]},{"system":"urn:rxnorm:relation:finite"},{"system":SYSTEM,"concept":[{"code":"222"}]}],"exclude":[{"system":SYSTEM,"concept":[{"code":"111"}]}]}});
    let extra = vec![
        json!({"name":"tx-resource","resource":{"resourceType":"CodeSystem","url":"urn:rxnorm:relation:finite","content":"complete","caseSensitive":true,"concept":[{"code":"A","display":"Local finite entry"}]}}),
    ];
    let (status, body) = expand(&app, resource, extra).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["222", "A"]);
    cleanup(admin, pool, schema).await;
}
