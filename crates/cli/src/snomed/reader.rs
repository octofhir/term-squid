//! Bounded RF2 decoding and CSV COPY transport; no release-sized Rust collections.
use super::{Source, SourceStats};
use anyhow::{bail, ensure, Context, Result};
use chrono::NaiveDate;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::PathBuf,
};

const MAX_LINE: u64 = 64 * 1024;
const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;
const CHUNK: usize = 64 * 1024;

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    Concepts,
    Descriptions,
    Relationships,
    Language,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Concepts => "concepts",
            Self::Descriptions => "descriptions",
            Self::Relationships => "relationships",
            Self::Language => "language",
        }
    }
    fn header(self) -> &'static str {
        match self {
            Self::Concepts=>"id\teffectiveTime\tactive\tmoduleId\tdefinitionStatusId",
            Self::Descriptions=>"id\teffectiveTime\tactive\tmoduleId\tconceptId\tlanguageCode\ttypeId\tterm\tcaseSignificanceId",
            Self::Relationships=>"id\teffectiveTime\tactive\tmoduleId\tsourceId\tdestinationId\trelationshipGroup\ttypeId\tcharacteristicTypeId\tmodifierId",
            Self::Language=>"id\teffectiveTime\tactive\tmoduleId\trefsetId\treferencedComponentId\tacceptabilityId",
        }
    }
    fn columns(self) -> &'static [&'static str] {
        match self {
            Self::Concepts => &[
                "id",
                "effective_time",
                "active",
                "module_id",
                "definition_status_id",
            ],
            Self::Descriptions => &[
                "id",
                "effective_time",
                "active",
                "module_id",
                "concept_id",
                "language_code",
                "type_id",
                "term",
                "case_significance_id",
            ],
            Self::Relationships => &[
                "id",
                "effective_time",
                "active",
                "module_id",
                "source_id",
                "destination_id",
                "relationship_group",
                "type_id",
                "characteristic_type_id",
                "modifier_id",
            ],
            Self::Language => &[
                "id",
                "effective_time",
                "active",
                "module_id",
                "refset_id",
                "description_id",
                "acceptability_id",
            ],
        }
    }
    fn max_rows(self) -> u64 {
        if matches!(self, Self::Concepts) {
            2_000_000
        } else {
            10_000_000
        }
    }
    fn validate(self, values: &[&str], release: NaiveDate) -> Result<()> {
        ensure!(
            values.len() == self.columns().len(),
            "Wrong RF2 column count"
        );
        ensure!(
            !values.iter().any(|v| v.chars().any(char::is_control)),
            "Control character in RF2 field"
        );
        if matches!(self, Self::Language) {
            uuid::Uuid::parse_str(values[0]).context("Invalid language member UUID")?;
        } else {
            sctid(values[0])?;
        }
        ensure!(
            date(values[1])? <= release,
            "RF2 effectiveTime is after the declared release"
        );
        ensure!(matches!(values[2], "0" | "1"), "RF2 active must be 0 or 1");
        sctid(values[3])?;
        match self {
            Self::Concepts => sctid(values[4])?,
            Self::Descriptions => {
                sctid(values[4])?;
                language(values[5])?;
                sctid(values[6])?;
                sctid(values[8])?;
                ensure!(!values[7].is_empty(), "Empty RF2 term");
            }
            Self::Relationships => {
                for i in [4, 5, 7, 8, 9] {
                    sctid(values[i])?;
                }
                ensure!(
                    !values[6].is_empty() && values[6].bytes().all(|b| b.is_ascii_digit()),
                    "Invalid relationshipGroup"
                );
                values[6]
                    .parse::<i32>()
                    .context("relationshipGroup exceeds integer range")?;
                if values[2] == "1" {
                    ensure!(
                        values[8] == "900000000000011006",
                        "Only active inferred relationships are supported"
                    );
                    ensure!(
                        values[9] == "900000000000451002",
                        "Only existential relationship modifiers are supported"
                    );
                }
            }
            Self::Language => {
                for i in [4, 5, 6] {
                    sctid(values[i])?;
                }
                ensure!(
                    matches!(values[6], "900000000000548007" | "900000000000549004"),
                    "Unknown RF2 language acceptability"
                );
            }
        }
        Ok(())
    }
}

pub(super) fn sctid(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 18
            && id.bytes().all(|b| b.is_ascii_digit())
            && !id.starts_with('0'),
        "Expected a positive numeric SCTID (at most 18 digits)"
    );
    Ok(())
}
pub(super) fn language(code: &str) -> Result<()> {
    ensure!(
        (2..=3).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_lowercase()),
        "Expected a lower-case ISO language code"
    );
    Ok(())
}
pub(super) fn date(value: &str) -> Result<NaiveDate> {
    ensure!(
        value.len() == 8 && value.bytes().all(|b| b.is_ascii_digit()),
        "Expected published YYYYMMDD effectiveTime"
    );
    NaiveDate::parse_from_str(value, "%Y%m%d").context("Invalid RF2 calendar date")
}

pub(super) async fn copy_source(
    tx: &mut Transaction<'_, Postgres>,
    kind: Kind,
    source: Source,
    path: PathBuf,
    release: NaiveDate,
) -> Result<SourceStats> {
    let columns = kind
        .columns()
        .iter()
        .map(|name| format!("{name} text NOT NULL"))
        .collect::<Vec<_>>()
        .join(",");
    sqlx::query(&format!(
        "CREATE TEMP TABLE rf2_{} ({columns}, PRIMARY KEY(id)) ON COMMIT DROP",
        kind.name()
    ))
    .execute(&mut **tx)
    .await?;
    let (send, mut receive) = tokio::sync::mpsc::channel::<Vec<u8>>(2);
    let worker =
        tokio::task::spawn_blocking(move || read_source(kind, source, path, release, send));
    let mut copy = tx
        .copy_in_raw(&format!(
            "COPY rf2_{} FROM STDIN WITH (FORMAT CSV, DELIMITER E'\\t')",
            kind.name()
        ))
        .await?;
    while let Some(chunk) = receive.recv().await {
        if let Err(error) = copy.send(chunk).await {
            drop(receive);
            let _ = copy.abort("RF2 transport failed").await;
            return Err(error.into());
        }
    }
    let parsed = match worker.await {
        Ok(result) => result,
        Err(error) => {
            let _ = copy.abort("RF2 reader task failed").await;
            return Err(error).context("RF2 reader task failed");
        }
    };
    match parsed {
        Ok(stats) => {
            let copied = copy.finish().await?;
            ensure!(copied == stats.rows, "RF2 COPY row count mismatch");
            Ok(stats)
        }
        Err(error) => {
            let _ = copy.abort("RF2 validation failed").await;
            Err(error)
        }
    }
}

fn read_source(
    kind: Kind,
    source: Source,
    path: PathBuf,
    release: NaiveDate,
    send: tokio::sync::mpsc::Sender<Vec<u8>>,
) -> Result<SourceStats> {
    ensure!(
        std::fs::metadata(&path)?.is_file(),
        "RF2 source is not a regular file"
    );
    let file = File::open(&path).with_context(|| format!("Cannot open {} source", kind.name()))?;
    ensure!(
        file.metadata()?.is_file(),
        "RF2 source is not a regular file"
    );
    ensure!(
        file.metadata()?.len() <= MAX_FILE,
        "RF2 source exceeds 2 GiB"
    );
    let mut reader = BufReader::new(file);
    let mut sha = Sha256::new();
    let mut bytes = 0u64;
    let mut rows = 0u64;
    let mut line = Vec::new();
    let mut chunk = Vec::with_capacity(CHUNK);
    loop {
        line.clear();
        let read = (&mut reader)
            .take(MAX_LINE + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }
        ensure!(read as u64 <= MAX_LINE, "RF2 line exceeds 64 KiB");
        sha.update(&line);
        bytes += read as u64;
        ensure!(bytes <= MAX_FILE, "RF2 source exceeds 2 GiB");
        let text = std::str::from_utf8(&line).context("RF2 must be UTF-8")?;
        let text = text.strip_suffix('\n').unwrap_or(text);
        let text = text.strip_suffix('\r').unwrap_or(text);
        if bytes == read as u64 {
            ensure!(
                text.strip_prefix('\u{feff}').unwrap_or(text) == kind.header(),
                "Unexpected {} RF2 header",
                kind.name()
            );
            continue;
        }
        rows += 1;
        ensure!(
            rows <= kind.max_rows(),
            "RF2 {} row limit exceeded",
            kind.name()
        );
        let values = text.split('\t').collect::<Vec<_>>();
        kind.validate(&values, release)
            .with_context(|| format!("Invalid {} RF2 row {}", kind.name(), rows + 1))?;
        for (i, value) in values.iter().enumerate() {
            if i > 0 {
                chunk.push(b'\t');
            }
            chunk.push(b'"');
            for byte in value.bytes() {
                if byte == b'"' {
                    chunk.push(b'"');
                }
                chunk.push(byte);
            }
            chunk.push(b'"');
        }
        chunk.push(b'\n');
        if chunk.len() >= CHUNK {
            send.blocking_send(std::mem::replace(&mut chunk, Vec::with_capacity(CHUNK)))
                .map_err(|_| anyhow::anyhow!("RF2 import cancelled"))?;
        }
    }
    ensure!(rows > 0, "RF2 source has no data rows");
    if !chunk.is_empty() {
        send.blocking_send(chunk)
            .map_err(|_| anyhow::anyhow!("RF2 import cancelled"))?;
    }
    let hash = format!("{:x}", sha.finalize());
    if hash != source.sha256 {
        bail!("RF2 {} checksum mismatch", kind.name());
    }
    Ok(SourceStats {
        sha256: hash,
        rows,
        bytes,
        file: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
    })
}
