//! Optional, explicitly attested concept-only RF2 Simple Snapshot cohorts.
use super::{reader, require_empty, SourceStats};
use anyhow::{ensure, Context, Result};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const HEADER: &str = "id\teffectiveTime\tactive\tmoduleId\trefsetId\treferencedComponentId";
const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_LINE: u64 = 64 * 1024;
const MAX_ROWS: u64 = 20_000_000;
const CHUNK: usize = 64 * 1024;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Source {
    path: PathBuf,
    sha256: String,
    expected_rows: u64,
    release: String,
    complete_snapshot: bool,
    concept_only: bool,
    refset_ids: Vec<String>,
}

impl Source {
    pub(super) fn validate(&self, release: &str) -> Result<()> {
        ensure!(
            self.release == release && self.complete_snapshot && self.concept_only,
            "Simple refsets require the same release and explicit completeSnapshot/conceptOnly attestations"
        );
        ensure!(
            self.sha256.len() == 64
                && self
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Expected lowercase Simple refset SHA-256"
        );
        ensure!(
            self.expected_rows <= MAX_ROWS,
            "Simple refset row limit exceeds 20 million"
        );
        ensure!(
            (1..=64).contains(&self.refset_ids.len()),
            "Declare 1 to 64 Simple refset IDs"
        );
        for id in &self.refset_ids {
            term_squid_core::snomed::require_refset_code(id)?;
            ensure!(
                id != "446609009",
                "Declared Simple refset must be a proper descendant of 446609009, not the type root itself"
            );
        }
        ensure!(
            self.refset_ids.windows(2).all(|ids| ids[0] < ids[1]),
            "Simple refset IDs must be sorted and unique"
        );
        let name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .context("Invalid Simple refset source filename")?;
        ensure!(
            name.contains("SimpleSnapshot")
                && !name.contains("Full")
                && !name.contains("Delta")
                && name.ends_with(&format!("_{release}.txt"))
                && !name.contains('\\'),
            "Simple refsets require a SimpleSnapshot filename with the declared release date"
        );
        Ok(())
    }

    pub(super) fn identity(&self) -> Value {
        json!({"sha256":self.sha256,"rows":self.expected_rows,"release":self.release,
            "completeSnapshot":self.complete_snapshot,"conceptOnly":self.concept_only,
            "refsetIds":self.refset_ids})
    }

    pub(super) async fn stage(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        manifest: &Path,
        release: NaiveDate,
    ) -> Result<Prepared> {
        sqlx::query("CREATE TEMP TABLE rf2_simple_members(id uuid PRIMARY KEY,effective_time text NOT NULL,active text NOT NULL,module_id text NOT NULL,refset_id text NOT NULL,referenced_component_id text NOT NULL,fields jsonb NOT NULL) ON COMMIT DROP")
            .execute(&mut **tx).await?;
        let path = if self.path.is_absolute() {
            self.path.clone()
        } else {
            manifest
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(&self.path)
        };
        let source = self.clone();
        let (send, mut receive) = tokio::sync::mpsc::channel::<Vec<u8>>(2);
        let worker = tokio::task::spawn_blocking(move || read_source(source, path, release, send));
        let mut copy = tx
            .copy_in_raw("COPY rf2_simple_members FROM STDIN WITH (FORMAT CSV, DELIMITER E'\\t')")
            .await?;
        while let Some(chunk) = receive.recv().await {
            if let Err(error) = copy.send(chunk).await {
                drop(receive);
                let _ = copy.abort("Simple refset transport failed").await;
                return Err(error.into());
            }
        }
        let stats = match worker.await {
            Ok(Ok(stats)) => stats,
            result => {
                let _ = copy.abort("Simple refset validation failed").await;
                return match result {
                    Ok(Err(error)) => Err(error),
                    Err(error) => Err(error).context("Simple refset reader task failed"),
                    Ok(Ok(_)) => unreachable!(),
                };
            }
        };
        ensure!(
            copy.finish().await? == stats.rows,
            "Simple refset COPY row count mismatch"
        );
        require_empty(tx, "SELECT EXISTS(SELECT 1 FROM rf2_simple_members m LEFT JOIN rf2_concepts c ON c.id=m.referenced_component_id WHERE c.id IS NULL)",
            "Simple refset member references a missing concept; description and relationship refsets are unsupported").await?;
        sqlx::query("CREATE INDEX rf2_simple_parent_source ON rf2_relationships(source_id,destination_id) WHERE active='1' AND type_id='116680003' AND characteristic_type_id='900000000000011006'")
            .execute(&mut **tx).await?;
        for id in &self.refset_ids {
            let active: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM rf2_concepts WHERE id=$1 AND active='1')",
            )
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
            ensure!(
                active,
                "Declared Simple refset must be an active concept in this release"
            );
            // LIMIT bounds the complete visited set, not only a successful path or early match.
            let ancestors: Vec<String> = sqlx::query_scalar("WITH RECURSIVE ancestors(code) AS (SELECT id FROM rf2_concepts WHERE id=$1 AND active='1' UNION SELECT r.destination_id FROM ancestors a JOIN rf2_relationships r ON r.source_id=a.code JOIN rf2_concepts c ON c.id=r.destination_id AND c.active='1' WHERE r.active='1' AND r.type_id='116680003' AND r.characteristic_type_id='900000000000011006') SELECT code FROM ancestors LIMIT 8193")
                .bind(id).fetch_all(&mut **tx).await?;
            ensure!(
                ancestors.len() <= 8192,
                "Simple refset type proof exceeds 8192 concepts"
            );
            ensure!(
                ancestors.iter().any(|code| code == "446609009"),
                "Declared refset is not an inferred subtype of Simple type reference set 446609009"
            );
        }
        let metadata = json!({"file":stats.file,"sha256":stats.sha256,"rows":stats.rows,
            "bytes":stats.bytes,"release":self.release,"completeSnapshot":true,"conceptOnly":true,
            "refsetIds":self.refset_ids});
        Ok(Prepared {
            stats,
            metadata,
            refset_ids: self.refset_ids.clone(),
        })
    }
}

pub(super) struct Prepared {
    pub(super) stats: SourceStats,
    metadata: Value,
    refset_ids: Vec<String>,
}
impl Prepared {
    pub(super) async fn publish(&self, tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE snomed_releases SET simple_refsets_loaded=true,simple_refset_source=$2 WHERE code_system_id=$1")
            .bind(id).bind(sqlx::types::Json(&self.metadata)).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO snomed_simple_refsets(code_system_id,refset_id) SELECT $1,unnest($2::text[])")
            .bind(id).bind(&self.refset_ids).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO snomed_simple_members(code_system_id,id,effective_time,active,module_id,refset_id,referenced_component_id,fields) SELECT $1,id,to_date(effective_time,'YYYYMMDD'),active='1',module_id,refset_id,referenced_component_id,fields FROM rf2_simple_members")
            .bind(id).execute(&mut **tx).await?;
        Ok(())
    }
}

fn csv_field(chunk: &mut Vec<u8>, value: &str) {
    chunk.push(b'"');
    for byte in value.bytes() {
        if byte == b'"' {
            chunk.push(b'"');
        }
        chunk.push(byte);
    }
    chunk.push(b'"');
}

fn read_source(
    source: Source,
    path: PathBuf,
    release: NaiveDate,
    send: tokio::sync::mpsc::Sender<Vec<u8>>,
) -> Result<SourceStats> {
    ensure!(
        std::fs::metadata(&path)?.is_file(),
        "Simple refset source must be a regular file"
    );
    let file = File::open(&path)?;
    ensure!(
        file.metadata()?.is_file(),
        "Simple refset source must be a regular file"
    );
    ensure!(
        file.metadata()?.len() <= MAX_FILE,
        "Simple refset source exceeds 2 GiB"
    );
    let mut reader = BufReader::new(file);
    let mut sha = Sha256::new();
    let (mut bytes, mut rows) = (0u64, 0u64);
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
        ensure!(read as u64 <= MAX_LINE, "Simple refset line exceeds 64 KiB");
        sha.update(&line);
        bytes += read as u64;
        ensure!(bytes <= MAX_FILE, "Simple refset source exceeds 2 GiB");
        let text = std::str::from_utf8(&line).context("Simple refset source must be UTF-8")?;
        let text = text.strip_suffix('\n').unwrap_or(text);
        let text = text.strip_suffix('\r').unwrap_or(text);
        if bytes == read as u64 {
            ensure!(
                text.strip_prefix('\u{feff}').unwrap_or(text) == HEADER,
                "Unexpected Simple refset RF2 header"
            );
            continue;
        }
        rows += 1;
        ensure!(rows <= MAX_ROWS, "Simple refset exceeds 20 million rows");
        let values: Vec<&str> = text.split('\t').collect();
        ensure!(
            values.len() == 6,
            "Simple refset must have exactly six columns"
        );
        ensure!(
            !values
                .iter()
                .any(|value| value.chars().any(char::is_control)),
            "Control character in Simple refset field"
        );
        Uuid::parse_str(values[0]).context("Invalid Simple refset member UUID")?;
        ensure!(
            reader::date(values[1])? <= release,
            "Simple refset effectiveTime is after the declared release"
        );
        ensure!(
            matches!(values[2], "0" | "1"),
            "Simple refset active must be 0 or 1"
        );
        reader::sctid(values[3])?;
        term_squid_core::snomed::require_refset_code(values[4])?;
        reader::sctid(values[5])?;
        ensure!(
            source.refset_ids.iter().any(|id| id == values[4]),
            "Simple refset row uses an undeclared refset ID"
        );
        for (i, value) in values.iter().enumerate() {
            if i > 0 {
                chunk.push(b'\t');
            }
            csv_field(&mut chunk, value);
        }
        let fields = json!({"id":values[0],"effectiveTime":values[1],"active":values[2],
            "moduleId":values[3],"refsetId":values[4],"referencedComponentId":values[5]});
        chunk.push(b'\t');
        csv_field(&mut chunk, &serde_json::to_string(&fields)?);
        chunk.push(b'\n');
        if chunk.len() >= CHUNK {
            send.blocking_send(std::mem::replace(&mut chunk, Vec::with_capacity(CHUNK)))
                .map_err(|_| anyhow::anyhow!("Simple refset import cancelled"))?;
        }
    }
    ensure!(bytes > 0, "Simple refset source has no header");
    ensure!(
        rows == source.expected_rows,
        "Simple refset expectedRows mismatch"
    );
    let hash = hex::encode(sha.finalize());
    ensure!(hash == source.sha256, "Simple refset checksum mismatch");
    if !chunk.is_empty() {
        send.blocking_send(chunk)
            .map_err(|_| anyhow::anyhow!("Simple refset import cancelled"))?;
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
