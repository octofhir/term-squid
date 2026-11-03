//! Keep unsupported SNOMED expression handling separate from scalar code membership.
use crate::{Error, Result};

/// A complete RF2 identifier projection is not a post-coordination evaluator.
pub fn reject_expression(code: &str) -> Result<()> {
    if code.bytes().any(|b| b":+|{}=(),".contains(&b)) {
        return Err(Error(
            "SNOMED expression/post-coordination evaluation is unsupported".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expressions_are_not_classified_as_invalid_scalar_codes() {
        for code in [
            "100001:100002=100003",
            "100001+100002",
            "100001 |fixture|",
            "100001:{100002=100003}",
        ] {
            assert!(reject_expression(code).is_err());
        }
        for code in ["100001", "999999999999", "missing"] {
            assert!(reject_expression(code).is_ok()); // Existence is the projection's job.
        }
    }
}
