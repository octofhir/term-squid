//! Complete/partial code knowledge retained independently of an expanded membership subset.
use crate::{Canonical, PreparedSystem};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug)]
enum Codes {
    Sensitive(BTreeSet<String>),
    Folded(BTreeMap<String, String>),
}
#[derive(Debug)]
pub struct CodeSystemIndex {
    pub canonical: Canonical,
    complete: bool,
    codes: Codes,
}
impl PreparedSystem {
    /// Retain compact knowledge without consuming a shared immutable preparation.
    pub fn code_index(&self) -> CodeSystemIndex {
        CodeSystemIndex {
            canonical: self.canonical.clone(),
            complete: self.content_mode == "complete",
            codes: if self.case_sensitive {
                Codes::Sensitive(self.concepts.keys().cloned().collect())
            } else {
                Codes::Folded(self.folded.clone())
            },
        }
    }
    /// Consume preparation after composition; retain only membership knowledge for diagnostics.
    pub fn into_code_index(self) -> CodeSystemIndex {
        CodeSystemIndex {
            canonical: self.canonical,
            complete: self.content_mode == "complete",
            codes: if self.case_sensitive {
                Codes::Sensitive(self.concepts.into_keys().collect())
            } else {
                Codes::Folded(self.folded)
            },
        }
    }
}
impl CodeSystemIndex {
    pub fn complete(&self) -> bool {
        self.complete
    }
    pub fn resolve(&self, code: &str) -> Option<&str> {
        match &self.codes {
            Codes::Sensitive(codes) => codes.get(code).map(String::as_str),
            Codes::Folded(codes) => codes.get(&code.to_lowercase()).map(String::as_str),
        }
    }
    pub fn memory_charge(&self) -> usize {
        let keys = match &self.codes {
            Codes::Sensitive(codes) => codes
                .iter()
                .map(|code| 128 + code.capacity())
                .sum::<usize>(),
            Codes::Folded(codes) => codes
                .iter()
                .map(|(key, code)| 256 + key.capacity() + code.capacity())
                .sum::<usize>(),
        };
        keys + 1024
            + 4 * (self.canonical.url.capacity()
                + self.canonical.version.as_ref().map_or(0, String::capacity))
    }
}
