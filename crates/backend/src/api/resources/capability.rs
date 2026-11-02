use axum::Json;
use serde_json::{json, Value};

pub async fn capability_statement() -> Json<Value> {
    Json(json!({
        "resourceType": "CapabilityStatement",
        "status": "active",
        "date": "2025-09-30",
        "kind": "instance",
        "software": {
            "name": "term-squid",
            "version": env!("CARGO_PKG_VERSION")
        },
        "implementation": {
            "description": "FHIR R4 terminology service backed by PostgreSQL; R5/R6 routes are experimental aliases"
        },
        "fhirVersion": "4.0.1",
        "format": ["json"],
        "rest": [{
            "mode": "server",
            "documentation": "Supported FHIR release: R4 4.0.1 at /r4. Legacy /r5 and /r6 aliases have no verified release adapter.",
            "resource": [
                {
                    "type": "CodeSystem",
                    "interaction": [
                        {"code": "read"},
                        {"code": "search-type"}
                    ],
                    "searchParam": [
                        {"name": "url", "type": "uri"},
                        {"name": "name", "type": "string"},
                        {"name": "status", "type": "token"}
                    ]
                },
                {
                    "type": "ValueSet",
                    "interaction": [
                        {"code": "read"},
                        {"code": "search-type"}
                    ],
                    "searchParam": [
                        {"name": "url", "type": "uri"},
                        {"name": "name", "type": "string"},
                        {"name": "status", "type": "token"}
                    ]
                },
                {
                    "type": "ConceptMap",
                    "interaction": [
                        {"code": "read"},
                        {"code": "search-type"}
                    ],
                    "searchParam": [
                        {"name": "url", "type": "uri"},
                        {"name": "status", "type": "token"}
                    ]
                }
            ]
        }]
    }))
}

pub async fn terminology_capabilities() -> Json<Value> {
    Json(json!({
        "resourceType": "TerminologyCapabilities",
        "status": "active",
        "date": "2025-09-30",
        "kind": "instance",
        "software": {
            "name": "term-squid",
            "version": env!("CARGO_PKG_VERSION")
        },
        "codeSystem": [],
        "expansion": {
            "hierarchical": false,
            "paging": true
        },
        "codeSearch": "all",
        "validateCode": {
            "translations": false
        }
    }))
}
