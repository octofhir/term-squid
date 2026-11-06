//! Finite callers borrow immutable plans; native callers receive one bounded page.
use crate::{error::AppError, models::ExpandedValueSet, store::TerminologyStore};
use serde_json::Value;
use term_squid_core::Member;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub struct ValueSetRequest<'a> {
    pub url: &'a str,
    pub version: Option<&'a str>,
    pub id: Option<Uuid>,
    pub inline: Option<&'a Value>,
    pub system_context: Option<(&'a str, &'a str)>,
}
#[derive(Clone, Copy)]
pub enum ValueSetOperation<'a> {
    Expand {
        filter: Option<&'a str>,
        offset: i64,
        count: i64,
    },
    Find {
        system: &'a str,
        version: Option<&'a str>,
        code: &'a str,
    },
}
#[derive(Debug)]
pub struct ValueSetPage {
    pub sources: crate::models::ExpansionSources,
    pub resource: Value,
    pub total: i64,
    pub members: Vec<Member>,
    /// Page-local labels and compact source knowledge for finite leaves of a mixed plan.
    pub(crate) finite: Option<super::finite_bridge::Page>,
}
pub enum ValueSetResult {
    Finite(ExpandedValueSet),
    Native(ValueSetPage),
}
impl std::fmt::Debug for ValueSetResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Finite(_) => f.write_str("FiniteValueSet"),
            Self::Native(p) => f
                .debug_struct("NativeValueSet")
                .field("total", &p.total)
                .field("page_length", &p.members.len())
                .finish(),
        }
    }
}
pub(super) async fn finite<S: TerminologyStore + ?Sized>(
    store: &S,
    request: ValueSetRequest<'_>,
    operation: ValueSetOperation<'_>,
) -> Result<ValueSetResult, AppError> {
    if let ValueSetOperation::Expand { offset, count, .. } = operation {
        check_page(offset, count)?;
    }
    let expanded = if let Some(id) = request.id {
        store.expand_compose_by_id(&id).await?
    } else if let Some(resource) = request.inline {
        store
            .compose_inline(resource, request.version, request.system_context)
            .await?
    } else if let Some((system, version)) = request.system_context {
        store
            .compose_for_validation(request.url, request.version, None, system, Some(version))
            .await?
    } else {
        store.expand_compose(request.url, request.version).await?
    };
    Ok(ValueSetResult::Finite(expanded))
}
pub(super) fn check_page(offset: i64, count: i64) -> Result<(), AppError> {
    if offset < 0 || !(0..=10_000).contains(&count) {
        return Err(AppError::BadRequest(
            "offset must be nonnegative and count must be between 0 and 10000".into(),
        ));
    }
    Ok(())
}
