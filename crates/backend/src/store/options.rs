//! Common R4 temporal and language options, independent of HTTP and authentication.
use crate::error::AppError;
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};

pub fn date(value: &str) -> Result<DateTime<Utc>, AppError> {
    let invalid = || {
        AppError::BadRequest("date requires an R4 dateTime with a timezone for time values".into())
    };
    if value.len() > 64
        || !value
            .get(..4)
            .is_some_and(|s| s.bytes().all(|b| b.is_ascii_digit()) && s != "0000")
    {
        return Err(invalid());
    }
    let end = match value.len() {
        4 => NaiveDate::from_ymd_opt(value.parse::<i32>().map_err(|_| invalid())? + 1, 1, 1),
        7 => {
            let d = NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d")
                .map_err(|_| invalid())?;
            if d.month() == 12 {
                NaiveDate::from_ymd_opt(d.year() + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1)
            }
        }
        10 => NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.succ_opt()),
        _ => {
            return DateTime::parse_from_rfc3339(value)
                .map_err(|_| invalid())
                .and_then(|d| {
                    if d.offset().local_minus_utc().abs() > 14 * 3600 {
                        Err(invalid())
                    } else {
                        Ok(d.with_timezone(&Utc))
                    }
                })
        }
    }
    .ok_or_else(invalid)?;
    Ok(
        Utc.from_utc_datetime(&end.and_hms_opt(0, 0, 0).expect("Midnight"))
            - chrono::Duration::nanoseconds(1),
    )
}
pub fn language(value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 255
        || !value.split('-').all(|part| {
            !part.is_empty() && part.len() <= 8 && part.bytes().all(|b| b.is_ascii_alphanumeric())
        })
        || !value
            .split('-')
            .next()
            .is_some_and(|part| part.bytes().all(|b| b.is_ascii_alphabetic()))
    {
        return Err(AppError::BadRequest(
            "displayLanguage requires a language tag".into(),
        ));
    }
    Ok(())
}
pub fn language_matches(requested: &str, actual: &str) -> bool {
    requested.eq_ignore_ascii_case(actual)
        || actual
            .get(..requested.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(requested))
            && actual.as_bytes().get(requested.len()) == Some(&b'-')
}

/// A caller-supplied definition cannot claim a historical publication before its own date.
pub(super) fn check_resource_date(
    resource: &serde_json::Value,
    requested: Option<&str>,
) -> Result<(), AppError> {
    if let Some((requested, published)) = requested.zip(resource["date"].as_str()) {
        if date(published)? > date(requested)? {
            return Err(AppError::BadRequest(
                "Definition was published after the requested date".into(),
            ));
        }
    }
    Ok(())
}
