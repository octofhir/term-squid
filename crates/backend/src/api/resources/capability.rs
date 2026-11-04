use crate::{
    api::extractors::{FhirJson as Json, FhirPath as Path, FhirQuery as Query},
    error::AppError,
    store::TerminologyStore,
};
use axum::extract::State;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, LazyLock};
static CONTRACT: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../../../docs/R4_API_CONTRACT.json"))
        .expect("Checked R4 contract")
});
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataParams {
    mode: Option<String>,
    #[serde(rename = "_format")]
    format: Option<String>,
}
pub async fn capability_statement(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(params): Query<MetadataParams>,
) -> Result<Json<Value>, AppError> {
    crate::api::extractors::validate_json_format(params.format.as_deref())?;
    match params.mode.as_deref() {
        Some("terminology") => return terminology_capabilities(State(store)).await,
        None | Some("full") | Some("normative") => {}
        Some(_) => {
            return Err(AppError::BadRequest(
                "metadata mode must be full, normative or terminology".into(),
            ))
        }
    }
    let resources=vec![
        resource("CodeSystem",&[("lookup","Primitive or Coding lookup; date, requested properties, real designations and displayLanguage. Instance lookup is a compatibility extension."),("validate-code","Primitive, Coding or CodeableConcept validation; inline CodeSystem, date, language and abstract policy."),("subsumes","Primitive or Coding pairs in one selected is-a hierarchy; cross-system relationships remain indeterminate.")]),
        resource("ValueSet",&[("expand","Bounded stored, instance, inline and local-context expansion; version directives, date, language, designations, lifecycle filters, hierarchy and paging. Provider expression limits apply."),("validate-code","Primitive, Coding or CodeableConcept validation with independent system/ValueSet versions, date, language, abstract policy and unambiguous local context inference.")]),
        resource("ConceptMap",&[("translate","Stored, instance or inline maps and bounded discovery; Coding/CodeableConcept, source/target ValueSet context, dependency/product, versions, reverse and unmapped rules.")]),
    ];
    Ok(Json(
        json!({"resourceType":"CapabilityStatement","status":"active","date":"2026-11-04","kind":"instance","software":{"name":"term-squid","version":env!("CARGO_PKG_VERSION")},"implementation":{"description":"Standalone term-squid terminology service"},"fhirVersion":"4.0.1","format":["application/fhir+json"],"rest":[{"mode":"server","documentation":"Verified adapter: R4 4.0.1 at /r4. R5/R6 are experimental aliases. Public read operations include POST; administration is separately authenticated. Local context accepts ValueSet canonicals or ValueSet/{uuid}; StructureDefinition bindings require a conformance registry. Historical inference requires dated definitions or an explicit immutable edition.","resource":resources}]}),
    ))
}
fn resource(kind: &str, operations: &[(&str, &str)]) -> Value {
    let operations:Vec<_>=operations.iter().map(|(name,documentation)|json!({"name":name,"definition":format!("http://hl7.org/fhir/OperationDefinition/{kind}-{name}"),"documentation":documentation})).collect();
    json!({"type":kind,"interaction":[{"code":"read"},{"code":"search-type"}],"operation":operations,"searchParam":[{"name":"url","type":"uri"},{"name":"name","type":"string"},{"name":"status","type":"token"}]})
}
pub async fn terminology_capabilities(
    State(store): State<Arc<dyn TerminologyStore>>,
) -> Result<Json<Value>, AppError> {
    let inventory = store.code_system_capabilities().await?;
    let expansion: Vec<_> = CONTRACT["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == "ValueSet-expand")
        .unwrap()["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| json!({"name":p["name"]}))
        .collect();
    Ok(Json(
        json!({"resourceType":"TerminologyCapabilities","status":"active","date":"2026-11-04","kind":"instance","software":{"name":"term-squid","version":env!("CARGO_PKG_VERSION")},"implementation":{"description":"Current local terminology inventory"},"description":"R4 terminology operations support Coding and CodeableConcept, bounded inline resources, dates and display languages. Installed content and native provider expression limits determine clinical coverage. CodeableConcept validation does not implicitly invoke ConceptMap translation. UCUM uses the bundled immutable grammar.","codeSystem":inventory,"expansion":{"hierarchical":true,"paging":true,"incomplete":false,"parameter":expansion,"textFilter":"Case-insensitive substring matching on code and the selected display; punctuation is significant and wildcards have no special meaning."},"validateCode":{"translations":false},"translation":{"needsMap":false}}),
    ))
}
pub async fn operation_definition(Path(id): Path<String>) -> Result<Json<Value>, AppError> {
    let operation = CONTRACT["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == id)
        .ok_or_else(|| AppError::NotFound("OperationDefinition not found".into()))?;
    let mut parameters = operation["parameters"].as_array().unwrap().clone();
    parameters.extend(operation["outputs"].as_array().unwrap().iter().cloned());
    for parameter in &mut parameters {
        if let Some(object) = parameter.as_object_mut() {
            object.remove("status");
            object.remove("acceptanceTests");
        }
    }
    Ok(Json(
        json!({"resourceType":"OperationDefinition","id":id,"url":operation["canonical"],"version":"4.0.1","name":operation["name"],"status":"active","kind":"operation","date":"2019-11-01","code":operation["code"],"resource":operation["resource"],"system":false,"type":operation["type"],"instance":operation["instance"],"affectsState":false,"parameter":parameters}),
    ))
}
