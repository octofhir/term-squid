//! Validate the common shallow DAG without repeating every ancestor walk.
use crate::{Error, PreparedSystem, Result, MAX_DEPTH, MAX_HIERARCHY_RELATIONSHIPS};
use std::collections::{BTreeMap, VecDeque};

pub(super) const MAX_PREPARATION_WORK: usize = 10_000_000;
impl PreparedSystem {
    pub(super) fn validate_graph(&self, complete: bool) -> Result<()> {
        if self.parents.is_empty() {
            return Ok(());
        }
        // Assign local integer identities once. Repeated queue/depth updates then
        // avoid comparing source strings; public code ordering and indexes stay intact.
        let identities: BTreeMap<&str, usize> = self
            .concepts
            .keys()
            .enumerate()
            .map(|(index, code)| (code.as_str(), index))
            .collect();
        let mut remaining = vec![0_usize; identities.len()];
        let mut depths = vec![0_usize; identities.len()];
        let mut children = vec![Vec::new(); identities.len()];
        let mut relationships = 0;
        for (child, parents) in &self.parents {
            let Some(&child_index) = identities.get(child.as_str()) else {
                if complete {
                    return Err(Error(format!("Unknown hierarchy child {child}")));
                }
                continue;
            };
            for parent in parents {
                let &parent_index = identities
                    .get(parent.as_str())
                    .ok_or_else(|| Error(format!("Unknown parent {parent}")))?;
                relationships += 1;
                if relationships > MAX_HIERARCHY_RELATIONSHIPS {
                    return Err(Error("Hierarchy relationship limit exceeded".into()));
                }
                remaining[child_index] += 1;
                children[parent_index].push(child_index);
            }
        }
        let mut ready: VecDeque<_> = remaining
            .iter()
            .enumerate()
            .filter(|(_, count)| **count == 0)
            .map(|(index, _)| index)
            .collect();
        let mut visited = 0;
        let mut longest = 0;
        while let Some(parent) = ready.pop_front() {
            visited += 1;
            let depth = depths[parent];
            longest = longest.max(depth);
            for &child in &children[parent] {
                remaining[child] -= 1;
                depths[child] = depths[child].max(depth + 1);
                if remaining[child] == 0 {
                    ready.push_back(child);
                }
            }
        }
        if visited != remaining.len() {
            return Err(Error("Hierarchy cycle".into()));
        }
        if longest > MAX_DEPTH {
            // Preserve the existing shortest-path distance bound in DAGs with shortcuts.
            // A longest-path test alone would reject otherwise supported hierarchies.
            let mut work = 0;
            for code in self.concepts.keys() {
                self.ancestor_depths_work(code, &mut work)?;
            }
        }
        Ok(())
    }
}
