//! Generated auxiliary cohorts exercise availability, not clinical source authenticity.
use super::*;

const FIRST_NDC: &str = "00012345678";
const SECOND_NDC: &str = "00012345679";

pub(super) async fn seed_aux(
    pool: &PgPool,
    id: Uuid,
    version: &str,
    sty: bool,
    sat: bool,
    empty_sat: bool,
) {
    let mut tx = pool.begin().await.unwrap();
    let mut sources = json!({});
    if sty {
        sources["RXNSTY.RRF"] = descriptor("RXNSTY.RRF", version, 4, 400);
        for (row, code, tui, stn, name) in [
            (1, "111", "T109", "A1.4", "Organic Chemical"),
            (2, "222", "T121", "A1.4.1", "Pharmacologic Substance"),
            (3, "222", "T200", "B2", "Second Semantic Type"),
            (4, "300", "T109", "A1.4", "Organic Chemical"),
        ] {
            let fields = json!({"RXCUI":code,"TUI":tui,"STN":stn,"STY":name,"ATUI":"","CVF":""});
            sqlx::query("INSERT INTO rxnorm_semantic_types(code_system_id,source_row,rxcui,tui,stn,sty,fields) VALUES($1,$2,$3,$4,$5,$6,$7)")
                .bind(id).bind(i64::from(row)).bind(code).bind(tui).bind(stn).bind(name).bind(sqlx::types::Json(fields)).execute(&mut *tx).await.unwrap();
        }
        sqlx::query("UPDATE concepts SET properties=properties||CASE code WHEN '111' THEN '[{\"code\":\"TUI\",\"valueCode\":\"T109\"},{\"code\":\"STN\",\"valueString\":\"A1.4\"},{\"code\":\"STY\",\"valueString\":\"Organic Chemical\"}]'::jsonb ELSE '[{\"code\":\"TUI\",\"valueCode\":\"T121\"},{\"code\":\"TUI\",\"valueCode\":\"T200\"},{\"code\":\"STN\",\"valueString\":\"A1.4.1\"},{\"code\":\"STN\",\"valueString\":\"B2\"},{\"code\":\"STY\",\"valueString\":\"Pharmacologic Substance\"},{\"code\":\"STY\",\"valueString\":\"Second Semantic Type\"}]'::jsonb END WHERE code_system_id=$1")
            .bind(id).execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE code_systems SET content=jsonb_set(content,'{property}',content->'property'||'[{\"code\":\"TUI\",\"type\":\"code\"},{\"code\":\"STN\",\"type\":\"string\"},{\"code\":\"STY\",\"type\":\"string\"}]'::jsonb) WHERE id=$1")
            .bind(id).execute(&mut *tx).await.unwrap();
    }
    if sat {
        sources["RXNSAT.RRF"] = descriptor(
            "RXNSAT.RRF",
            version,
            if empty_sat { 0 } else { 7 },
            if empty_sat { 0 } else { 700 },
        );
        if !empty_sat {
            for (row, code, aui, stype, name, sab, value, suppress) in [
                (1, "222", "222", "AUI", "NDC", "RXNORM", FIRST_NDC, "N"),
                (2, "222", "222", "AUI", "NDC", "RXNORM", SECOND_NDC, "N"),
                (3, "222", "222", "AUI", "NDC", "RXNORM", "11111111111", "O"),
                (4, "111", "305", "AUI", "NDC", "VANDF", "00111111111", "N"),
                (
                    5,
                    "222",
                    "r-123",
                    "RUI",
                    "OTHER",
                    "RXNORM",
                    "Unsupported relation attribute",
                    "N",
                ),
                (6, "300", "306", "AUI", "NDC", "RXNORM", "00999999999", "N"),
                (7, "222", "222", "AUI", "NDC", "RXNORM", FIRST_NDC, "N"),
            ] {
                let fields = json!({"RXCUI":code,"LUI":"","SUI":"","RXAUI":aui,"STYPE":stype,"CODE":"not-public-code","ATUI":"","SATUI":"","ATN":name,"SAB":sab,"ATV":value,"SUPPRESS":suppress,"CVF":""});
                sqlx::query("INSERT INTO rxnorm_attributes(code_system_id,source_row,rxcui,rxaui,stype,code,atui,atn,sab,atv,suppress,fields) VALUES($1,$2,$3,$4,$5,'not-public-code','',$6,$7,$8,$9,$10)")
                    .bind(id).bind(i64::from(row)).bind(code).bind(aui).bind(stype).bind(name).bind(sab).bind(value).bind(suppress).bind(sqlx::types::Json(fields)).execute(&mut *tx).await.unwrap();
            }
            sqlx::query("UPDATE concepts SET properties=properties||$2 WHERE code_system_id=$1 AND code='222'")
                .bind(id).bind(sqlx::types::Json(json!([{"code":"NDC","valueString":FIRST_NDC},{"code":"NDC","valueString":SECOND_NDC}]))).execute(&mut *tx).await.unwrap();
        }
        sqlx::query("UPDATE code_systems SET content=jsonb_set(content,'{property}',content->'property'||'[{\"code\":\"NDC\",\"type\":\"string\"}]'::jsonb) WHERE id=$1")
            .bind(id).execute(&mut *tx).await.unwrap();
    }
    sqlx::query("UPDATE rxnorm_releases SET semantic_types_loaded=$2,attributes_loaded=$3,auxiliary_sources=$4 WHERE code_system_id=$1")
        .bind(id).bind(sty).bind(sat).bind(sqlx::types::Json(sources)).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

fn descriptor(file: &str, version: &str, rows: u64, bytes: u64) -> Value {
    json!({"file":file,"sha256":"b".repeat(64),"rows":rows,"bytes":bytes,"version":version,"fullMonthly":true})
}

fn selection(property: &str, op: &str, value: &str) -> Value {
    let mut selection = active();
    selection["filter"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":property,"op":op,"value":value}));
    selection
}

fn values(body: &Value, property: &str) -> Vec<String> {
    body["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"] == "property")
        .filter_map(|p| {
            let parts = p["part"].as_array()?;
            if !parts
                .iter()
                .any(|part| part["name"] == "code" && part["valueCode"] == property)
            {
                return None;
            }
            parts
                .iter()
                .find_map(|part| {
                    part.get("valueCode")
                        .filter(|_| part["name"] != "code")
                        .or_else(|| part.get("valueString"))
                })
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

async fn lookup(app: &axum::Router, code: &str, properties: &str) -> (u16, Value) {
    request(
        app,
        "GET",
        &format!("/r4/CodeSystem/$lookup?system={SYSTEM}&code={code}{properties}"),
        Value::Null,
    )
    .await
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_auxiliary_availability_is_not_inferred_from_matching_rows() {
    for (sty, sat) in [(false, false), (true, false), (false, true), (true, true)] {
        let (admin, pool, schema) = hierarchy_test_database().await;
        let id = release(&pool, VERSION, "2026-03-02").await;
        seed_aux(&pool, id, VERSION, sty, sat, false).await;
        let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
        for (property, value, loaded) in [("STY", "T121", sty), ("NDC", FIRST_NDC, sat)] {
            let (status, body) = expand(
                &app,
                set(selection(property, "=", value)),
                vec![json!({"name":"count","valueInteger":0})],
            )
            .await;
            assert_eq!(status, if loaded { 200 } else { 422 }, "{sty}/{sat} {body}");
            if loaded {
                assert_eq!(body["expansion"]["total"], 1);
            } else {
                assert_eq!(body["resourceType"], "OperationOutcome");
            }
        }
        for (property, loaded) in [("STY", sty), ("TUI", sty), ("STN", sty), ("NDC", sat)] {
            let (status, body) = lookup(&app, "222", &format!("&property={property}")).await;
            assert_eq!(status, if loaded { 200 } else { 422 }, "{property}: {body}");
        }
        for properties in ["", "&property=*"] {
            let (status, body) = lookup(&app, "222", properties).await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(
                parameter(&body, "display")["valueString"],
                "Generated tablet"
            );
        }
        let (status, body) =
            request(&app, "GET", "/r4/metadata?mode=terminology", Value::Null).await;
        assert_eq!(status, 200, "{body}");
        let version = &body["codeSystem"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["uri"] == SYSTEM)
            .unwrap()["version"][0];
        for (property, loaded) in [("STY", sty), ("NDC", sat)] {
            assert_eq!(
                version["filter"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|f| f["code"] == property),
                loaded,
                "{version}"
            );
        }
        cleanup(admin, pool, schema).await;
    }
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_semantic_type_qualifiers_lists_and_pages_use_exact_selected_source() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    seed_aux(&pool, id, VERSION, true, true, false).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (op, value, expected) in [
        ("=", "T109", vec!["111"]),
        ("=", "TUI:T121", vec!["222"]),
        ("=", "STN:A1.4.1", vec!["222"]),
        ("=", "STY:Organic Chemical", vec!["111"]),
        ("in", "T109,T121", vec!["111", "222"]),
        ("in", "TUI:T109,T121", vec!["111", "222"]),
        ("in", "TUI:T109,TUI:T200", vec!["111", "222"]),
        ("=", "T999", vec![]),
    ] {
        let (status, body) = expand(&app, set(selection("STY", op, value)), vec![]).await;
        assert_eq!(status, 200, "{value}: {body}");
        assert_eq!(codes(&body), expected);
        assert_eq!(body["expansion"]["total"], expected.len());
    }
    let resource = set(selection("STY", "in", "T109,T121,T200"));
    for (offset, count, expected) in [
        (0, 1, vec!["111"]),
        (1, 1, vec!["222"]),
        (2, 1, vec![]),
        (0, 0, vec![]),
        (i64::MAX, 10, vec![]),
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
        assert_eq!(codes(&body), expected);
        assert_eq!(body["expansion"]["total"], 2);
    }
    let (status, body) = lookup(&app, "222", "&property=TUI&property=STN&property=STY").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(values(&body, "TUI"), ["T121", "T200"]);
    assert_eq!(values(&body, "STN"), ["A1.4.1", "B2"]);
    assert_eq!(
        values(&body, "STY"),
        ["Pharmacologic Substance", "Second Semantic Type"]
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_normalized_ndc_preserves_leading_zeros_and_rejects_source_or_suppressed_leakage() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    seed_aux(&pool, id, VERSION, true, true, false).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (value, expected) in [
        (FIRST_NDC, vec!["222"]),
        (SECOND_NDC, vec!["222"]),
        ("11111111111", vec![]),
        ("00111111111", vec![]),
        ("00999999999", vec![]),
    ] {
        let (status, body) = expand(&app, set(selection("NDC", "=", value)), vec![]).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(codes(&body), expected);
        assert_eq!(body["expansion"]["total"], expected.len());
    }
    let (status, body) = expand(
        &app,
        set(selection("NDC", "in", &format!("{FIRST_NDC},{SECOND_NDC}"))),
        vec![],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["222"]);
    assert_eq!(body["expansion"]["total"], 1);
    let (status, body) = lookup(&app, "222", "&property=NDC").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(values(&body, "NDC"), [FIRST_NDC, SECOND_NDC]);
    let (status, body) = lookup(&app, "111", "&property=NDC").await;
    assert_eq!(status, 200, "{body}");
    assert!(values(&body, "NDC").is_empty());
    let (status, body) = member(&app, set(selection("NDC", "=", FIRST_NDC)), "111").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    let (status, body) = member(&app, set(selection("NDC", "=", FIRST_NDC)), "999999").await;
    assert_eq!(status, 422, "{body}");
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_loaded_empty_attributes_are_known_absence_not_unavailable_source() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    seed_aux(&pool, id, VERSION, false, true, true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, body) = lookup(&app, "222", "&property=NDC").await;
    assert_eq!(status, 200, "{body}");
    assert!(values(&body, "NDC").is_empty());
    let (status, body) = expand(&app, set(selection("NDC", "=", FIRST_NDC)), vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 0);
    let (status, body) = member(&app, set(selection("NDC", "=", FIRST_NDC)), "222").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_auxiliary_bad_qualifiers_lists_and_ndc_formats_do_not_become_empty_successes() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    seed_aux(&pool, id, VERSION, true, true, false).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (property, op, value) in [
        ("STY", "=", "RXCUI:111"),
        ("STY", "in", "T109,STY:Organic Chemical"),
        ("STY", "in", "T109,,T121"),
        ("STY", "=", "TUI:T109') OR true --"),
        ("STY", "regex", ".*"),
        ("NDC", "=", "00123-4567-89"),
        ("NDC", "=", "1234567890"),
        ("NDC", "in", "00012345678,"),
        ("NDC", "regex", ".*"),
    ] {
        let (status, body) = expand(&app, set(selection(property, op, value)), vec![]).await;
        assert_eq!(status, 422, "{property} {value}: {body}");
        assert_eq!(body["resourceType"], "OperationOutcome");
    }
    let (status, body) = expand(
        &app,
        set(selection("STY", "in", &vec!["T109"; 257].join(","))),
        vec![],
    )
    .await;
    assert_eq!(status, 422, "{body}");
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_unavailable_auxiliary_sources_fail_before_empty_or_early_positive_paths() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    release(&pool, VERSION, "2026-03-02").await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (property, value) in [("STY", "T121"), ("NDC", FIRST_NDC)] {
        let mut selected = selection(property, "=", value);
        selected["filter"]
            .as_array_mut()
            .unwrap()
            .push(json!({"property":"TTY","op":"=","value":"NoSuchType"}));
        let resource = json!({"resourceType":"ValueSet","compose":{"include":[{"system":SYSTEM,"concept":[{"code":"111"}]},selected]}});
        let (status, body) = expand(
            &app,
            resource.clone(),
            vec![json!({"name":"count","valueInteger":0})],
        )
        .await;
        assert_eq!(status, 422, "{body}");
        assert!(body.get("expansion").is_none());
        let (status, body) = member(&app, resource, "111").await;
        assert_eq!(status, 422, "{body}");
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_auxiliary_predicates_mix_with_finite_context_and_exact_exclusions() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    seed_aux(&pool, id, VERSION, true, true, false).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let finite = json!({"resourceType":"CodeSystem","url":"urn:rxnorm:aux-finite","version":"1","status":"active","content":"complete","concept":[{"code":"A","display":"Finite A"}]});
    let resource = json!({"resourceType":"ValueSet","compose":{"include":[selection("STY","in","T109,T121"),selection("NDC","=",FIRST_NDC),{"system":"urn:rxnorm:aux-finite","version":"1"}],"exclude":[{"system":SYSTEM,"version":VERSION,"concept":[{"code":"111"}]}]}});
    let(status,body)=request(&app,"POST","/r4/ValueSet/$expand",json!({"resourceType":"Parameters","parameter":[{"name":"valueSet","resource":resource},{"name":"tx-resource","resource":finite}]})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["222", "A"]);
    assert_eq!(body["expansion"]["total"], 2);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_auxiliary_edition_flags_and_commits_are_fresh_and_snapshot_scoped() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let old = release(&pool, "02022026", "2026-02-02").await;
    let current = release(&pool, VERSION, "2026-03-02").await;
    seed_aux(&pool, current, VERSION, true, true, false).await;
    sqlx::query("UPDATE code_systems SET updated_at='2099-01-01' WHERE id=$1")
        .bind(current)
        .execute(&pool)
        .await
        .unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let mut selected = selection("STY", "=", "T121");
    selected["version"] = "02022026".into();
    let (status, body) = expand(&app, set(selected), vec![]).await;
    assert_eq!(status, 422, "{body}");
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/{old}/$lookup?code=222&property=STY"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 422, "{body}");
    let resource = set(selection("STY", "=", "T121"));
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE rxnorm_semantic_types SET tui='T122' WHERE code_system_id=$1 AND tui='T121'",
    )
    .bind(current)
    .execute(&mut *tx)
    .await
    .unwrap();
    let (status, body) = expand(&app, resource.clone(), vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["222"]);
    tx.rollback().await.unwrap();
    sqlx::query(
        "UPDATE rxnorm_semantic_types SET tui='T122' WHERE code_system_id=$1 AND tui='T121'",
    )
    .bind(current)
    .execute(&pool)
    .await
    .unwrap();
    let (status, body) = expand(&app, resource, vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 0);
    let uncached = api::create_router(Arc::new(PostgresStore::without_prepared_cache(
        pool.clone(),
    )));
    for app in [&app, &uncached] {
        let (status, body) = expand(app, set(selection("STY", "=", "T122")), vec![]).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(codes(&body), ["222"]);
    }
    cleanup(admin, pool, schema).await;
}
