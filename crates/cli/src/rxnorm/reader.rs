//! Strict bounded RRF decoding and COPY transport, without release-sized Rust maps.
use super::{Source, SourceStats};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::PathBuf,
};

pub(super) const MAX_ROWS: u64 = 20_000_000;
pub(super) const MAX_FILE: u64 = 4 * 1024 * 1024 * 1024;
const MAX_LINE: u64 = 1024 * 1024;
const CHUNK: usize = 64 * 1024;
const COLUMNS: &[&str] = &[
    "RXCUI", "LAT", "TS", "LUI", "STT", "SUI", "ISPREF", "RXAUI", "SAUI", "SCUI", "SDUI", "SAB",
    "TTY", "CODE", "STR", "SRL", "SUPPRESS", "CVF",
];

pub(super) fn field(chunk: &mut Vec<u8>, value: &str) {
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
    send: tokio::sync::mpsc::Sender<Vec<u8>>,
) -> Result<SourceStats> {
    ensure!(
        std::fs::metadata(&path)?.is_file(),
        "RxNorm source must be a regular file"
    );
    let file = File::open(&path).context("Cannot open RxNorm source")?;
    ensure!(
        file.metadata()?.is_file(),
        "RxNorm source must be a regular file"
    );
    ensure!(
        file.metadata()?.len() <= MAX_FILE,
        "RxNorm source exceeds 4 GiB"
    );
    let mut input = BufReader::new(file);
    let mut sha = Sha256::new();
    let mut bytes = 0_u64;
    let mut rows = 0_u64;
    let mut line = Vec::new();
    let mut chunk = Vec::with_capacity(CHUNK);
    loop {
        ensure!(!send.is_closed(), "RxNorm import cancelled");
        line.clear();
        let read = (&mut input)
            .take(MAX_LINE + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }
        ensure!(read as u64 <= MAX_LINE, "RxNorm source line exceeds 1 MiB");
        bytes += read as u64;
        ensure!(bytes <= MAX_FILE, "RxNorm source exceeds 4 GiB");
        sha.update(&line);
        let text = std::str::from_utf8(&line).context("RxNorm RRF must be UTF-8")?;
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
            .context("RxNorm RRF rows require a trailing pipe")?;
        let values: Vec<_> = text.split('|').collect();
        ensure!(
            values.len() == COLUMNS.len(),
            "RxNorm RRF requires exactly 18 fields on row {}",
            rows + 1
        );
        ensure!(
            !values
                .iter()
                .any(|value| value.chars().any(char::is_control)),
            "Control character in RxNorm field on row {}",
            rows + 1
        );
        term_squid_core::rxnorm::require_code(values[0]).context("Invalid RXCUI")?;
        term_squid_core::rxnorm::require_code(values[7]).context("Invalid RXAUI")?;
        ensure!(
            values[1].len() == 3 && values[1].bytes().all(|byte| byte.is_ascii_uppercase()),
            "RxNorm LAT requires a three-letter upper-case language code"
        );
        ensure!(
            [11, 12]
                .into_iter()
                .all(|index| !values[index].is_empty() && values[index].len() <= 255),
            "RxNorm SAB/TTY must be nonempty and at most 255 bytes"
        );
        ensure!(!values[14].is_empty(), "RxNorm term must be nonempty");
        ensure!(
            matches!(values[16], "N" | "O" | "Y" | "E"),
            "Unknown RxNorm SUPPRESS value"
        );
        if values[11] == "RXNORM"
            && values[16] == "N"
            && !matches!(values[12], "SY" | "PSN" | "TMSY")
        {
            ensure!(
                term_squid_core::rxnorm::PRIMARY_TYPES.contains(&values[12]),
                "Unsupported active RxNorm primary TTY"
            );
        }
        rows += 1;
        ensure!(rows <= MAX_ROWS, "RxNorm source exceeds 20000000 atoms");
        for index in [7, 0, 1, 11, 12, 14, 16] {
            if index != 7 {
                chunk.push(b'\t');
            }
            field(&mut chunk, values[index]);
        }
        let fields = COLUMNS
            .iter()
            .zip(&values)
            .map(|(column, value)| {
                (
                    (*column).to_owned(),
                    serde_json::Value::String((*value).to_owned()),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        chunk.push(b'\t');
        field(&mut chunk, &serde_json::to_string(&fields)?);
        chunk.push(b'\n');
        if chunk.len() >= CHUNK {
            send.blocking_send(std::mem::replace(&mut chunk, Vec::with_capacity(CHUNK)))
                .map_err(|_| anyhow::anyhow!("RxNorm import cancelled"))?;
        }
    }
    ensure!(rows > 0, "RxNorm source has no atoms");
    let sha256 = hex::encode(sha.finalize());
    ensure!(sha256 == source.sha256, "RxNorm source checksum mismatch");
    if !chunk.is_empty() {
        send.blocking_send(chunk)
            .map_err(|_| anyhow::anyhow!("RxNorm import cancelled"))?;
    }
    Ok(SourceStats {
        sha256,
        rows,
        bytes,
        file: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
    })
}

pub(super) async fn copy_source(
    tx: &mut Transaction<'_, Postgres>,
    source: Source,
    path: PathBuf,
) -> Result<SourceStats> {
    sqlx::query("CREATE TEMP TABLE rxnorm_input(rxaui text PRIMARY KEY,rxcui text NOT NULL,lat text NOT NULL,sab text NOT NULL,tty text NOT NULL,term text NOT NULL,suppress text NOT NULL,fields jsonb NOT NULL) ON COMMIT DROP").execute(&mut **tx).await?;
    let (send, mut receive) = tokio::sync::mpsc::channel::<Vec<u8>>(2);
    let worker = tokio::task::spawn_blocking(move || read_source(source, path, send));
    let mut copy = tx.copy_in_raw("COPY rxnorm_input(rxaui,rxcui,lat,sab,tty,term,suppress,fields) FROM STDIN WITH (FORMAT CSV, DELIMITER E'\\t')").await?;
    while let Some(chunk) = receive.recv().await {
        if let Err(error) = copy.send(chunk).await {
            drop(receive);
            let _ = copy.abort("RxNorm transport failed").await;
            let _ = worker.await;
            return Err(error.into());
        }
    }
    let parsed = match worker.await {
        Ok(result) => result,
        Err(error) => {
            let _ = copy.abort("RxNorm reader failed").await;
            return Err(error).context("RxNorm reader task failed");
        }
    };
    match parsed {
        Ok(stats) => {
            let copied = copy.finish().await?;
            ensure!(copied == stats.rows, "RxNorm COPY row count mismatch");
            Ok(stats)
        }
        Err(error) => {
            let _ = copy.abort("RxNorm validation failed").await;
            Err(error)
        }
    }
}
