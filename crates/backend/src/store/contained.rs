//! Local canonical fragments belong to one container, never to the global registry.
use crate::error::AppError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use term_squid_core::{Canonical, MAX_RESOURCES};
use uuid::Uuid;

pub(super) struct ContainedSets {
    namespace: Uuid,
    containers: usize,
    keys: BTreeSet<Canonical>,
    pending: BTreeMap<Canonical, Value>,
}
impl Default for ContainedSets {
    fn default() -> Self {
        Self {
            namespace: Uuid::new_v4(),
            containers: 0,
            keys: BTreeSet::new(),
            pending: BTreeMap::new(),
        }
    }
}
impl ContainedSets {
    pub fn is_local(&self, key: &Canonical) -> bool {
        self.keys.contains(key)
    }
    pub fn take(&mut self, key: &Canonical) -> Option<Value> {
        self.pending.remove(key)
    }
    /// Rewrite only compose references in a private evaluation copy. Original FHIR JSON
    /// and public provenance never acquire these unobservable internal identifiers.
    pub fn register(&mut self, key: &Canonical, resource: &mut Value) -> Result<(), AppError> {
        let contained = resource
            .get("contained")
            .map(|value| {
                value.as_array().ok_or_else(|| {
                    AppError::BadRequest("ValueSet.contained must be an array".into())
                })
            })
            .transpose()?;
        let mut references = BTreeMap::from([("#".to_owned(), key.url.clone())]);
        if key.version.is_some() {
            references.insert(
                "#".into(),
                format!("{}|{}", key.url, key.version.as_deref().unwrap()),
            );
        }
        let mut children = Vec::new();
        if let Some(contained) = contained {
            if self.keys.len().saturating_add(contained.len()) > MAX_RESOURCES {
                return Err(AppError::BadRequest(
                    "Contained ValueSet resource limit exceeded".into(),
                ));
            }
            for child in contained {
                if child["resourceType"] != "ValueSet" {
                    return Err(AppError::BadRequest(
                        "Only contained ValueSet definitions are supported in compose".into(),
                    ));
                }
                let id = child["id"]
                    .as_str()
                    .filter(|id| {
                        !id.is_empty()
                            && id.len() <= 64
                            && id
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
                    })
                    .ok_or_else(|| {
                        AppError::BadRequest("Contained ValueSet requires a valid R4 id".into())
                    })?;
                if child
                    .get("contained")
                    .is_some_and(|nested| nested.as_array().is_none_or(|nested| !nested.is_empty()))
                    || ["versionId", "lastUpdated", "security"]
                        .iter()
                        .any(|field| child.get("meta").and_then(|meta| meta.get(field)).is_some())
                {
                    return Err(AppError::BadRequest(
                        "Contained resources cannot contain nested resources or independent version/security metadata".into(),
                    ));
                }
                let internal = format!(
                    "urn:term-squid:contained:{}:{}:{id}",
                    self.namespace, self.containers,
                );
                if references
                    .insert(format!("#{id}"), internal.clone())
                    .is_some()
                {
                    return Err(AppError::BadRequest(
                        "Duplicate contained ValueSet id".into(),
                    ));
                }
                children.push((Canonical::parse(&internal, None)?, child.clone()));
            }
        }
        self.containers += 1;
        rewrite(resource, &references)?;
        for (key, mut child) in children {
            rewrite(&mut child, &references)?;
            self.keys.insert(key.clone());
            self.pending.insert(key, child);
        }
        Ok(())
    }
}

fn rewrite(resource: &mut Value, references: &BTreeMap<String, String>) -> Result<(), AppError> {
    for name in ["include", "exclude"] {
        if let Some(selections) = resource
            .get_mut("compose")
            .and_then(|compose| compose.get_mut(name))
            .and_then(Value::as_array_mut)
        {
            for selection in selections {
                if let Some(sets) = selection.get_mut("valueSet").and_then(Value::as_array_mut) {
                    for value in sets {
                        if let Some(reference) = value.as_str().filter(|s| s.starts_with('#')) {
                            *value = references
                                .get(reference)
                                .ok_or_else(|| {
                                    AppError::BadRequest(format!(
                                        "Unresolved local ValueSet reference {reference}"
                                    ))
                                })?
                                .clone()
                                .into();
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn malformed_containment_is_not_treated_as_an_external_reference() {
        let root = Canonical::parse("urn:parent", None).unwrap();
        let valid = json!({"resourceType":"ValueSet","id":"child","compose":{"include":[{"system":"urn:codes"}]}});
        for children in [
            json!({}),
            json!([valid, valid]),
            json!([{"resourceType":"ValueSet","id":"bad/id"}]),
            json!([{"resourceType":"CodeSystem","id":"child"}]),
            json!([{"resourceType":"ValueSet","id":"child","contained":[valid]}]),
            json!([{"resourceType":"ValueSet","id":"child","meta":{"versionId":"1"}}]),
            json!([{"resourceType":"ValueSet","id":"child","meta":{"security":[{}]}}]),
        ] {
            let mut resource =
                json!({"contained":children,"compose":{"include":[{"valueSet":["#child"]}]}});
            assert!(ContainedSets::default()
                .register(&root, &mut resource)
                .is_err());
        }
    }

    #[test]
    fn unused_contained_definitions_still_consume_the_resource_budget() {
        let root = Canonical::parse("urn:parent", None).unwrap();
        let children: Vec<_> = (0..=MAX_RESOURCES).map(|i| {
            json!({"resourceType":"ValueSet","id":format!("child-{i}"),"compose":{"include":[{"system":"urn:codes"}]}})
        }).collect();
        let mut resource =
            json!({"contained":children,"compose":{"include":[{"system":"urn:codes"}]}});
        assert!(ContainedSets::default()
            .register(&root, &mut resource)
            .is_err());
    }
}
