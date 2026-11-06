//! Lossless optional RXNREL cohort; direct source facts never become hierarchy or maps.
use super::{auxiliary::AuxiliarySource, reader, SourceStats};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const FILE: &str = "RXNREL.RRF";
const COLUMNS: &[&str] = &[
    "RXCUI1", "RXAUI1", "STYPE1", "REL", "RXCUI2", "RXAUI2", "STYPE2", "RELA", "RUI", "SRUI",
    "SAB", "SL", "RG", "DIR", "SUPPRESS", "CVF",
];

pub(super) fn validate(source: &AuxiliarySource, version: &str) -> Result<()> {
    ensure!(
        source.version == version && source.full_monthly,
        "Relationships require the same attested full monthly RxNorm version"
    );
    ensure!(
        source.path.file_name().and_then(|file| file.to_str()) == Some(FILE),
        "Expected official RXNREL.RRF filename"
    );
    ensure!(
        source.expected_rows <= reader::MAX_ROWS,
        "RxNorm relationship row count exceeds 20000000"
    );
    ensure!(
        source.sha256.len() == 64
            && source
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "Expected lowercase relationship source SHA-256"
    );
    Ok(())
}
pub(super) fn identity(source: &AuxiliarySource) -> Value {
    json!({"file":FILE,"sha256":source.sha256,"expectedRows":source.expected_rows,"version":source.version,"fullMonthly":true})
}

pub(super) struct Loaded {
    pub loaded: bool,
    pub source: Value,
}
impl Loaded {
    pub fn result_source(&self) -> Option<Value> {
        self.loaded.then(|| self.source.clone())
    }
    pub async fn publish(&self, tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<()> {
        if self.loaded {
            sqlx::query("INSERT INTO rxnorm_relationships(code_system_id,source_row,rxcui1,rxaui1,stype1,rel,rxcui2,rxaui2,stype2,rela,rui,srui,sab,sl,rg,dir,suppress,cvf,fields) SELECT $1,source_row,rxcui1,rxaui1,stype1,rel,rxcui2,rxaui2,stype2,rela,rui,srui,sab,sl,rg,dir,suppress,cvf,fields FROM rxnorm_rel_input").bind(id).execute(&mut **tx).await?;
        }
        Ok(())
    }
}

pub(super) async fn stage(
    tx: &mut Transaction<'_, Postgres>,
    directory: &Path,
    source: Option<&AuxiliarySource>,
) -> Result<Loaded> {
    let Some(source) = source else {
        return Ok(Loaded {
            loaded: false,
            source: json!({}),
        });
    };
    sqlx::query("CREATE TEMP TABLE rxnorm_rel_input(source_row bigint PRIMARY KEY,rxcui1 text NOT NULL,rxaui1 text NOT NULL,stype1 text NOT NULL,rel text NOT NULL,rxcui2 text NOT NULL,rxaui2 text NOT NULL,stype2 text NOT NULL,rela text NOT NULL,rui text NOT NULL,srui text NOT NULL,sab text NOT NULL,sl text NOT NULL,rg text NOT NULL,dir text NOT NULL,suppress text NOT NULL,cvf text NOT NULL,fields jsonb NOT NULL) ON COMMIT DROP").execute(&mut **tx).await?;
    let path = if source.path.is_absolute() {
        source.path.clone()
    } else {
        directory.join(&source.path)
    };
    let stats = copy_source(tx, source.clone(), path).await?;
    ensure!(
        stats.rows == source.expected_rows,
        "RXNREL.RRF row count does not match expectedRows"
    );
    // External UMLS C.../A... endpoints and opaque source identities are retained.
    // Only numeric RxNorm identifiers assert a reference into this exact CONSO cohort.
    let invalid:bool=sqlx::query_scalar("WITH endpoints AS (SELECT rxcui1 AS cui,rxaui1 AS aui FROM rxnorm_rel_input UNION ALL SELECT rxcui2,rxaui2 FROM rxnorm_rel_input) SELECT EXISTS(SELECT 1 FROM endpoints e WHERE (e.cui~'^[0-9]+$' AND NOT EXISTS(SELECT 1 FROM rxnorm_input a WHERE a.rxcui=e.cui)) OR (e.aui~'^[0-9]+$' AND NOT EXISTS(SELECT 1 FROM rxnorm_input a WHERE a.rxaui=e.aui AND (e.cui='' OR a.rxcui=e.cui))))").fetch_one(&mut **tx).await?;
    ensure!(
        !invalid,
        "RxNorm relationship references a missing or mismatched numeric CONSO endpoint"
    );
    Ok(Loaded {
        loaded: true,
        source: json!({"file":FILE,"sha256":stats.sha256,"rows":stats.rows,"bytes":stats.bytes,"version":source.version,"fullMonthly":true}),
    })
}

fn read_source(
    source: AuxiliarySource,
    path: PathBuf,
    send: tokio::sync::mpsc::Sender<Vec<u8>>,
) -> Result<SourceStats> {
    ensure!(
        std::fs::metadata(&path)?.is_file(),
        "RxNorm relationship source must be a regular file"
    );
    let file = File::open(&path).context("Cannot open RxNorm relationship source")?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= reader::MAX_FILE,
        "RxNorm relationship source must be a regular file of at most 4 GiB"
    );
    let mut input = BufReader::new(file);
    let mut sha = Sha256::new();
    let mut rows = 0_u64;
    let mut bytes = 0_u64;
    let mut line = Vec::new();
    let mut chunk = Vec::with_capacity(64 * 1024);
    loop {
        ensure!(!send.is_closed(), "RxNorm relationship import cancelled");
        line.clear();
        let read = (&mut input)
            .take(1024 * 1024 + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }
        ensure!(
            read <= 1024 * 1024,
            "RxNorm relationship line exceeds 1 MiB"
        );
        bytes += read as u64;
        ensure!(
            bytes <= reader::MAX_FILE,
            "RxNorm relationship source exceeds 4 GiB"
        );
        sha.update(&line);
        let text = std::str::from_utf8(&line).context("RxNorm RXNREL must be UTF-8")?;
        let text = if let Some(text) = text.strip_suffix('\n') {
            text.strip_suffix('\r').unwrap_or(text)
        } else {
            text
        };
        let text = if rows == 0 {
            text.strip_prefix('\u{feff}').unwrap_or(text)
        } else {
            text
        };
        let text = text
            .strip_suffix('|')
            .context("RxNorm RXNREL requires a trailing pipe")?;
        let values: Vec<_> = text.split('|').take(COLUMNS.len() + 1).collect();
        ensure!(
            values.len() == COLUMNS.len(),
            "RXNREL.RRF requires exactly 16 fields"
        );
        ensure!(
            values
                .iter()
                .all(|value| value.len() <= 255 && !value.chars().any(char::is_control)),
            "RxNorm relationship fields must be at most 255 bytes without control characters"
        );
        ensure!(
            !values[3].is_empty() && !values[10].is_empty(),
            "RxNorm relationship REL and SAB must be nonempty"
        );
        for (cui, aui, kind) in [(0, 1, 2), (4, 5, 6)] {
            if values[kind] == "CUI" {
                ensure!(
                    !values[cui].is_empty(),
                    "A CUI relationship endpoint requires its identifier"
                );
            }
            if values[kind] == "AUI" {
                ensure!(
                    !values[aui].is_empty(),
                    "An AUI relationship endpoint requires its identifier"
                );
            }
            for index in [cui, aui] {
                if !values[index].is_empty()
                    && values[index].bytes().all(|byte| byte.is_ascii_digit())
                {
                    term_squid_core::rxnorm::require_code(values[index])
                        .context("Invalid numeric RxNorm relationship endpoint")?;
                }
            }
        }
        ensure!(
            matches!(values[14], "" | "N" | "Y" | "E" | "O"),
            "Unknown RxNorm relationship suppression marker"
        );
        rows += 1;
        ensure!(
            rows <= reader::MAX_ROWS,
            "RxNorm relationship source exceeds 20000000 rows"
        );
        reader::field(&mut chunk, &rows.to_string());
        for value in &values {
            chunk.push(b'\t');
            reader::field(&mut chunk, value);
        }
        let fields = COLUMNS
            .iter()
            .zip(&values)
            .map(|(column, value)| ((*column).to_owned(), Value::String((*value).to_owned())))
            .collect::<serde_json::Map<_, _>>();
        chunk.push(b'\t');
        reader::field(&mut chunk, &serde_json::to_string(&fields)?);
        chunk.push(b'\n');
        if chunk.len() >= 64 * 1024 {
            send.blocking_send(std::mem::replace(&mut chunk, Vec::with_capacity(64 * 1024)))
                .map_err(|_| anyhow::anyhow!("RxNorm relationship import cancelled"))?;
        }
    }
    let sha256 = hex::encode(sha.finalize());
    ensure!(
        sha256 == source.sha256,
        "RxNorm relationship source checksum mismatch"
    );
    if !chunk.is_empty() {
        send.blocking_send(chunk)
            .map_err(|_| anyhow::anyhow!("RxNorm relationship import cancelled"))?;
    }
    Ok(SourceStats {
        sha256,
        rows,
        bytes,
        file: FILE.into(),
    })
}

async fn copy_source(
    tx: &mut Transaction<'_, Postgres>,
    source: AuxiliarySource,
    path: PathBuf,
) -> Result<SourceStats> {
    let (send, mut receive) = tokio::sync::mpsc::channel::<Vec<u8>>(2);
    let worker = tokio::task::spawn_blocking(move || read_source(source, path, send));
    let mut copy=tx.copy_in_raw("COPY rxnorm_rel_input(source_row,rxcui1,rxaui1,stype1,rel,rxcui2,rxaui2,stype2,rela,rui,srui,sab,sl,rg,dir,suppress,cvf,fields) FROM STDIN WITH (FORMAT CSV, DELIMITER E'\\t')").await?;
    while let Some(chunk) = receive.recv().await {
        if let Err(error) = copy.send(chunk).await {
            drop(receive);
            let _ = copy.abort("RxNorm relationship transport failed").await;
            let _ = worker.await;
            return Err(error.into());
        }
    }
    let parsed = match worker.await {
        Ok(result) => result,
        Err(error) => {
            let _ = copy.abort("RxNorm relationship reader failed").await;
            return Err(error).context("RxNorm relationship reader task failed");
        }
    };
    match parsed {
        Ok(stats) => {
            let count = copy.finish().await?;
            ensure!(
                count == stats.rows,
                "RxNorm relationship COPY row count mismatch"
            );
            Ok(stats)
        }
        Err(error) => {
            let _ = copy.abort("RxNorm relationship validation failed").await;
            Err(error)
        }
    }
}
