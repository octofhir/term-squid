//! R4 supplements enrich an existing identity; they never define independent codes.
use crate::{Canonical, Error, Result, MAX_CONCEPTS, MAX_DEPTH};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub fn target(resource: &Value) -> Result<Canonical> {
    if resource["content"] != "supplement" {
        return Err(Error("Expected CodeSystem supplement".into()));
    }
    let target = Canonical::parse(
        resource["supplements"]
            .as_str()
            .ok_or_else(|| Error("CodeSystem supplement requires supplements canonical".into()))?,
        None,
    )?;
    if resource["url"] == target.url {
        return Err(Error(
            "Supplement URL must differ from the supplemented CodeSystem".into(),
        ));
    }
    let mut pending: Vec<_> = resource["concept"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| (v, 0))
        .collect();
    let mut records = 0;
    while let Some((concept, depth)) = pending.pop() {
        records += 1;
        if records > MAX_CONCEPTS || depth > MAX_DEPTH {
            return Err(Error("Supplement concept limit exceeded".into()));
        }
        if concept.get("display").is_some() || concept.get("definition").is_some() {
            return Err(Error(
                "Supplement adds designations/properties; it cannot replace display or definition"
                    .into(),
            ));
        }
        pending.extend(
            concept["concept"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| (v, depth + 1)),
        );
    }
    Ok(target)
}
pub fn codes(resource: &Value) -> Result<Vec<&str>> {
    let mut pending: Vec<_> = resource["concept"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| (v, 0))
        .collect();
    let mut codes = Vec::new();
    while let Some((value, depth)) = pending.pop() {
        if depth > MAX_DEPTH || codes.len() >= MAX_CONCEPTS {
            return Err(Error("Supplement concept limit exceeded".into()));
        }
        codes.push(
            value["code"]
                .as_str()
                .ok_or_else(|| Error("Supplement concept code required".into()))?,
        );
        pending.extend(
            value["concept"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| (v, depth + 1)),
        );
    }
    Ok(codes)
}
/// Merge additional properties/designations while retaining the base URL, version and codes.
pub fn apply(base: &Value, supplements: &[Value]) -> Result<Value> {
    let mut base = base.clone();
    let case_sensitive = base["caseSensitive"].as_bool().unwrap_or(true);
    let normalize = |code: &str| {
        if case_sensitive {
            code.to_owned()
        } else {
            code.to_lowercase()
        }
    };
    let mut additions: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    let mut definitions = base["property"].as_array().cloned().unwrap_or_default();
    let mut count = 0;
    for supplement in supplements {
        let canonical = target(supplement)?;
        if base["url"] != canonical.url
            || canonical
                .version
                .as_deref()
                .is_some_and(|v| base["version"].as_str() != Some(v))
        {
            return Err(Error(
                "Supplement targets a different CodeSystem edition".into(),
            ));
        }
        for property in supplement["property"].as_array().into_iter().flatten() {
            let code = property["code"]
                .as_str()
                .ok_or_else(|| Error("Supplement property code required".into()))?;
            if let Some(existing) = definitions.iter().find(|p| p["code"] == code) {
                if existing["uri"] != property["uri"] || existing["type"] != property["type"] {
                    return Err(Error("Conflicting supplement property definition".into()));
                }
            } else {
                definitions.push(property.clone());
            }
        }
        let mut pending: Vec<_> = supplement["concept"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| (v, 0))
            .collect();
        while let Some((concept, depth)) = pending.pop() {
            count += 1;
            if count > MAX_CONCEPTS || depth > MAX_DEPTH {
                return Err(Error("Supplement concept limit exceeded".into()));
            }
            let code = concept["code"]
                .as_str()
                .ok_or_else(|| Error("Supplement concept code required".into()))?;
            if concept.get("display").is_some() || concept.get("definition").is_some() {
                return Err(Error("Supplement adds designations/properties; it cannot replace display or definition".into()));
            }
            additions.entry(normalize(code)).or_default().push(concept);
            pending.extend(
                concept["concept"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|v| (v, depth + 1)),
            );
        }
    }
    let mut found = BTreeSet::new();
    fn enrich(
        values: &mut [Value],
        additions: &BTreeMap<String, Vec<&Value>>,
        found: &mut BTreeSet<String>,
        case_sensitive: bool,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err(Error("Supplement base depth exceeded".into()));
        }
        for value in values {
            let code = value["code"]
                .as_str()
                .ok_or_else(|| Error("Base concept code required".into()))?;
            let code = if case_sensitive {
                code.to_owned()
            } else {
                code.to_lowercase()
            };
            if let Some(supplements) = additions.get(&code) {
                found.insert(code);
                for supplement in supplements {
                    for field in ["designation", "property"] {
                        if let Some(incoming) = supplement.get(field) {
                            let incoming = incoming.as_array().ok_or_else(|| {
                                Error("Supplement additions must be arrays".into())
                            })?;
                            let current = value
                                .as_object_mut()
                                .ok_or_else(|| Error("Concept object required".into()))?
                                .entry(field)
                                .or_insert_with(|| Value::Array(Vec::new()))
                                .as_array_mut()
                                .ok_or_else(|| Error("Invalid base concept additions".into()))?;
                            for addition in incoming {
                                if field == "property"
                                    && current
                                        .iter()
                                        .any(|p| p["code"] == addition["code"] && p != addition)
                                {
                                    return Err(Error(
                                        "Supplement cannot override an existing property value"
                                            .into(),
                                    ));
                                }
                                if !current.contains(addition) {
                                    current.push(addition.clone());
                                }
                            }
                            if field == "designation" && current.len() > 256 {
                                return Err(Error(
                                    "Supplement designations exceed 256 per concept".into(),
                                ));
                            }
                        }
                    }
                }
            }
            if let Some(children) = value.get_mut("concept").and_then(Value::as_array_mut) {
                enrich(children, additions, found, case_sensitive, depth + 1)?;
            }
        }
        Ok(())
    }
    if let Some(concepts) = base.get_mut("concept").and_then(Value::as_array_mut) {
        enrich(concepts, &additions, &mut found, case_sensitive, 0)?;
    }
    if additions.keys().any(|code| !found.contains(code)) {
        return Err(Error(
            "Supplement cannot introduce a code absent from the base CodeSystem".into(),
        ));
    }
    if !definitions.is_empty() {
        base["property"] = definitions.into();
    }
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn supplements_preserve_identity_add_designations_and_reject_new_codes_or_overrides() {
        let base = json!({"url":"urn:base","version":"1","caseSensitive":false,"concept":[{"code":"A","display":"Original","property":[{"code":"rank","valueInteger":1}]}]});
        let supplement = json!({"url":"urn:supplement","content":"supplement","supplements":"urn:base|1","property":[{"code":"group","type":"code"}],"concept":[{"code":"a","designation":[{"language":"ru","value":"Альфа"}],"property":[{"code":"group","valueCode":"test"}]}]});
        let merged = apply(&base, std::slice::from_ref(&supplement)).unwrap();
        assert_eq!(merged["url"], "urn:base");
        assert_eq!(merged["version"], "1");
        assert_eq!(merged["concept"][0]["display"], "Original");
        assert_eq!(merged["concept"][0]["designation"][0]["value"], "Альфа");
        assert_eq!(
            merged["concept"][0]["property"].as_array().unwrap().len(),
            2
        );
        for change in [
            json!({"code":"unknown"}),
            json!({"code":"a","display":"Changed"}),
            json!({"code":"a","property":[{"code":"rank","valueInteger":2}]}),
        ] {
            let mut invalid = supplement.clone();
            invalid["concept"] = json!([change]);
            assert!(apply(&base, &[invalid]).is_err());
        }
        let mut wrong = supplement;
        wrong["supplements"] = "urn:base|2".into();
        assert!(apply(&base, &[wrong]).is_err());
    }
}
