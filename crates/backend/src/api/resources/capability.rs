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
            "description": "FHIR Terminology Service - PostgreSQL backed. Multi-version support: /r4, /r5, /r6"
        },
        "fhirVersion": "4.0.1",
        "format": ["json"],
        "rest": [{
            "mode": "server",
            "documentation": "All endpoints are available under version-specific base URLs: /r4, /r5, and /r6",
            "resource": [
                {
                    "type": "CodeSystem",
                    "interaction": [
                        {"code": "read"},
                        {"code": "create"},
                        {"code": "update"},
                        {"code": "delete"},
                        {"code": "search-type"}
                    ],
                    "searchParam": [
                        {"name": "url", "type": "uri"},
                        {"name": "version", "type": "string"},
                        {"name": "name", "type": "string"},
                        {"name": "status", "type": "token"}
                    ]
                },
                {
                    "type": "ValueSet",
                    "interaction": [
                        {"code": "read"},
                        {"code": "create"},
                        {"code": "update"},
                        {"code": "delete"},
                        {"code": "search-type"}
                    ],
                    "searchParam": [
                        {"name": "url", "type": "uri"},
                        {"name": "version", "type": "string"},
                        {"name": "name", "type": "string"},
                        {"name": "status", "type": "token"}
                    ]
                },
                {
                    "type": "ConceptMap",
                    "interaction": [
                        {"code": "read"},
                        {"code": "create"},
                        {"code": "update"},
                        {"code": "delete"},
                        {"code": "search-type"}
                    ],
                    "searchParam": [
                        {"name": "url", "type": "uri"},
                        {"name": "version", "type": "string"},
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
