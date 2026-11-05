//! Coding diagnostics use the same snapshot as the membership decision.
use super::validation::{ValidationAnswer, ValidationCandidate, ValidationCoding};
use crate::models::ExpansionSources;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use term_squid_core::{Canonical, CodeSystemIndex, Member};

fn issue(severity: &str, code: &str, kind: &str, text: &str, path: &str) -> Value {
    json!({"severity":severity,"code":code,"details":{"coding":[{
        "system":"http://hl7.org/fhir/tools/CodeSystem/tx-issue-type","code":kind
    }],"text":text},"location":[path],"expression":[path]})
}
fn canonical(url: &str, version: Option<&str>) -> String {
    version.map_or_else(|| url.to_owned(), |version| format!("{url}|{version}"))
}

pub(super) fn member(
    answer: &mut ValidationAnswer,
    member: &Member,
    input: &ValidationCandidate<'_>,
) {
    coding(
        answer,
        &member.system,
        member.version.as_deref(),
        &member.code,
        member.case_sensitive,
        input,
    );
}
pub(super) fn coding(
    answer: &mut ValidationAnswer,
    system: &str,
    version: Option<&str>,
    code: &str,
    case_sensitive: bool,
    input: &ValidationCandidate<'_>,
) {
    let normalized = (code != input.code).then(|| code.to_owned());
    let mut issues = Vec::new();
    if normalized.is_some() && !case_sensitive {
        let text = format!("The code '{}' differs from the correct code '{}' by case. Although the code system '{}' is case insensitive, implementers are strongly encouraged to use the correct case anyway",
            input.code, code, canonical(system, version));
        issues.push(issue(
            "information",
            "business-rule",
            "code-rule",
            &text,
            &input.origin.code_path(),
        ));
    }
    match (answer.failure, input.display) {
        (Some(super::validation::ValidationFailure::Display), Some(expected)) => {
            let text = answer.display.as_deref().map_or_else(
                || {
                    format!(
                        "Display '{expected}' cannot be validated for '{}#{}'",
                        system, code
                    )
                },
                |display| {
                    format!(
                        "Display '{expected}' does not match '{display}' for '{}#{}'",
                        system, code
                    )
                },
            );
            issues.push(issue(
                if answer.valid { "warning" } else { "error" },
                "invalid",
                "invalid-display",
                &text,
                &input.origin.display_path(),
            ));
            answer.message = Some(text);
        }
        (Some(super::validation::ValidationFailure::Abstract), _) => {
            let text = format!(
                "Code '{}#{}' is abstract, and not allowed in this context",
                system, code
            );
            issues.push(issue(
                "error",
                "business-rule",
                "code-rule",
                &text,
                &input.origin.code_path(),
            ));
            answer.message = Some(text);
        }
        _ => {}
    }
    answer.coding = Some(ValidationCoding {
        system: system.to_owned(),
        code: input.code.into(),
        normalized,
        issues,
    });
}

pub(super) fn missing_member(
    value_set: &Value,
    system: &str,
    input: &ValidationCandidate<'_>,
    sources: &ExpansionSources,
    knowledge: Option<&BTreeMap<Canonical, CodeSystemIndex>>,
) -> ValidationAnswer {
    let identity = canonical(system, input.version);
    let set = canonical(
        value_set["url"].as_str().unwrap_or("urn:term-squid:inline"),
        value_set["version"].as_str(),
    );
    let message = format!(
        "The provided code '{}#{}' was not found in the value set '{}'",
        identity, input.code, set
    );
    let mut messages = vec![message.clone()];
    let mut issues = vec![issue(
        "error",
        "code-invalid",
        "not-in-vs",
        &message,
        &input.origin.code_path(),
    )];
    let mut version = input.version.map(str::to_owned);
    // Only editions selected by this exact operation can establish a definitive unknown code.
    let mut editions = sources.systems.iter().filter(|key| {
        key.url == system
            && input
                .version
                .is_none_or(|version| key.version.as_deref() == Some(version))
    });
    let first = editions.next();
    if let Some(selected) = first.filter(|_| editions.next().is_none()) {
        version = selected.version.clone();
        if knowledge
            .and_then(|knowledge| knowledge.get(selected))
            .is_some_and(|index| index.complete() && index.resolve(input.code).is_none())
        {
            let message = if let Some(version) = &version {
                format!(
                    "Unknown code '{}' in the CodeSystem '{}' version '{}'",
                    input.code, system, version
                )
            } else {
                format!(
                    "Unknown code '{}' in the CodeSystem '{}'",
                    input.code, system
                )
            };
            issues.push(issue(
                "error",
                "code-invalid",
                "invalid-code",
                &message,
                &input.origin.code_path(),
            ));
            messages.push(message);
        }
    }
    ValidationAnswer {
        valid: false,
        failure: Some(super::validation::ValidationFailure::NotInSet),
        display: None,
        version,
        message: Some(messages.join("; ")),
        coding: Some(ValidationCoding {
            system: system.into(),
            code: input.code.into(),
            normalized: None,
            issues,
        }),
    }
}

pub(super) fn unknown_code(
    system: &str,
    version: Option<&str>,
    input: &ValidationCandidate<'_>,
) -> ValidationAnswer {
    let message = version.map_or_else(
        || format!("Unknown code '{}' in the CodeSystem '{system}'", input.code),
        |version| {
            format!(
                "Unknown code '{}' in the CodeSystem '{system}' version '{version}'",
                input.code
            )
        },
    );
    ValidationAnswer {
        valid: false,
        display: None,
        version: version.map(str::to_owned),
        message: Some(message.clone()),
        failure: Some(super::validation::ValidationFailure::UnknownCode),
        coding: Some(ValidationCoding {
            system: system.into(),
            code: input.code.into(),
            normalized: None,
            issues: vec![issue(
                "error",
                "code-invalid",
                "invalid-code",
                &message,
                &input.origin.code_path(),
            )],
        }),
    }
}
