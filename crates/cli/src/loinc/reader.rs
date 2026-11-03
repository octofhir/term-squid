//! Bounded streaming CSV decoding and COPY transport. No release-sized collections.
use super::{Source, SourceStats};
use anyhow::{ensure, Context, Result};
use csv::{ReaderBuilder, StringRecord};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, Read},
    path::PathBuf,
};

pub(super) const MAX_ROWS: u64 = 2_000_000;
const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_RECORD: usize = 1024 * 1024;
const CSV_BUFFER: usize = 8 * 1024;
const MAX_COLUMNS: usize = 128;
const CHUNK: usize = 64 * 1024;
const REQUIRED: &[&str] = &[
    "LOINC_NUM",
    "COMPONENT",
    "PROPERTY",
    "TIME_ASPCT",
    "SYSTEM",
    "SCALE_TYP",
    "METHOD_TYP",
    "CLASS",
    "STATUS",
    "CLASSTYPE",
    "ORDER_OBS",
    "LONG_COMMON_NAME",
    "SHORTNAME",
    "DefinitionDescription",
];

#[derive(Clone, Copy, Default)]
enum QuoteState {
    #[default]
    Start,
    Unquoted,
    Quoted,
    AfterQuote,
}

/// csv intentionally accepts some malformed quoting. This guard validates syntax
/// while leaving field decoding, escaped values and record handling to that crate.
#[derive(Default)]
struct QuoteGuard {
    state: QuoteState,
    prefix: [u8; 3],
    prefix_len: usize,
    prefix_done: bool,
}
impl QuoteGuard {
    fn byte(&mut self, byte: u8) -> io::Result<()> {
        use QuoteState::*;
        self.state = match (self.state, byte) {
            (Start, b'"') => Quoted,
            (Start | Unquoted | AfterQuote, b',' | b'\r' | b'\n') => Start,
            (Start, _) => Unquoted,
            (Unquoted, b'"') => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid LOINC CSV quote inside an unquoted field",
                ))
            }
            (Unquoted, _) => Unquoted,
            (Quoted, b'"') => AfterQuote,
            (Quoted, _) => Quoted,
            (AfterQuote, b'"') => Quoted,
            (AfterQuote, _) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid LOINC CSV characters after a closing quote",
                ))
            }
        };
        Ok(())
    }
    fn input(&mut self, bytes: &[u8]) -> io::Result<()> {
        for &byte in bytes {
            if self.prefix_done {
                self.byte(byte)?;
            } else {
                self.prefix[self.prefix_len] = byte;
                self.prefix_len += 1;
                if self.prefix_len == 3 {
                    self.prefix_done = true;
                    if self.prefix != [0xef, 0xbb, 0xbf] {
                        for value in self.prefix {
                            self.byte(value)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn finish(&mut self) -> io::Result<()> {
        if !self.prefix_done {
            self.prefix_done = true;
            for index in 0..self.prefix_len {
                self.byte(self.prefix[index])?;
            }
        }
        if matches!(self.state, QuoteState::Quoted) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unterminated quoted LOINC CSV field",
            ));
        }
        Ok(())
    }
}

/// CSV may retain at most CSV_BUFFER bytes between calls. This limits new raw input
/// per record before the CSV crate can grow a record without bound. Decoded records
/// are checked separately; a malformed quoted field cannot consume the whole file.
struct BudgetReader<R> {
    input: R,
    sha: Sha256,
    bytes: u64,
    record_bytes: usize,
    syntax: QuoteGuard,
}
impl<R: Read> Read for BudgetReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        let room = MAX_RECORD.saturating_sub(self.record_bytes);
        if room == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "LOINC CSV record raw-input budget exceeds 1 MiB plus 8 KiB prefetched input",
            ));
        }
        let length = out.len().min(room);
        let read = self.input.read(&mut out[..length])?;
        if read == 0 {
            self.syntax.finish()?;
        } else {
            self.syntax.input(&out[..read])?;
        }
        self.record_bytes += read;
        self.bytes += read as u64;
        if self.bytes > MAX_FILE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "LOINC source exceeds 2 GiB",
            ));
        }
        self.sha.update(&out[..read]);
        Ok(read)
    }
}

fn read_record<R: Read>(
    reader: &mut csv::Reader<BudgetReader<R>>,
    record: &mut StringRecord,
) -> Result<bool> {
    reader.get_mut().record_bytes = 0;
    let found = reader
        .read_record(record)
        .context("Invalid UTF-8 LOINC CSV")?;
    ensure!(record.len() <= MAX_COLUMNS, "LOINC CSV exceeds 128 columns");
    ensure!(
        record.as_slice().len() <= MAX_RECORD,
        "LOINC decoded CSV record exceeds 1 MiB"
    );
    Ok(found)
}

fn headers(record: &StringRecord) -> Result<Vec<String>> {
    let columns = record.iter().map(str::to_owned).collect::<Vec<_>>();
    ensure!(!columns.is_empty(), "Empty LOINC header");
    let unique = columns.iter().collect::<BTreeSet<_>>();
    ensure!(
        unique.len() == columns.len(),
        "Duplicate LOINC CSV header column"
    );
    ensure!(
        columns
            .iter()
            .all(|c| !c.is_empty() && !c.chars().any(char::is_control)),
        "Invalid LOINC CSV header"
    );
    ensure!(
        REQUIRED
            .iter()
            .all(|required| columns.iter().any(|column| column == required)),
        "Missing required Loinc.csv columns"
    );
    Ok(columns)
}

fn validate_fields(fields: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    let value = |name: &str| fields[name].as_str().unwrap();
    let code = value("LOINC_NUM");
    let (number, check) = code
        .split_once('-')
        .context("Expected a numeric primary LOINC term identifier")?;
    ensure!(
        (1..=8).contains(&number.len())
            && number.bytes().all(|b| b.is_ascii_digit())
            && check.len() == 1
            && check.bytes().all(|b| b.is_ascii_digit()),
        "Expected a numeric primary LOINC term identifier"
    );
    ensure!(
        matches!(
            value("STATUS"),
            "ACTIVE" | "TRIAL" | "DISCOURAGED" | "DEPRECATED"
        ),
        "Unknown LOINC STATUS"
    );
    ensure!(
        matches!(value("CLASSTYPE"), "" | "1" | "2" | "3" | "4"),
        "LOINC CLASSTYPE must be blank, 1, 2, 3 or 4"
    );
    ensure!(
        matches!(
            value("ORDER_OBS"),
            "" | "Order" | "Observation" | "Both" | "Subset"
        ),
        "Unknown LOINC ORDER_OBS"
    );
    ensure!(
        fields.values().all(|v| !v.as_str().unwrap().contains('\0')),
        "NUL in LOINC field"
    );
    Ok(())
}

fn append_copy_field(chunk: &mut Vec<u8>, value: &str) {
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
        "LOINC source must be a regular file"
    );
    let file = File::open(&path).context("Cannot open LOINC source")?;
    ensure!(
        file.metadata()?.is_file(),
        "LOINC source must be a regular file"
    );
    ensure!(
        file.metadata()?.len() <= MAX_FILE,
        "LOINC source exceeds 2 GiB"
    );
    let budget = BudgetReader {
        input: file,
        sha: Sha256::new(),
        bytes: 0,
        record_bytes: 0,
        syntax: QuoteGuard::default(),
    };
    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .buffer_capacity(CSV_BUFFER)
        .from_reader(budget);
    let mut record = StringRecord::new();
    ensure!(read_record(&mut reader, &mut record)?, "Empty LOINC source");
    let columns = headers(&record)?;
    let mut rows = 0;
    let mut chunk = Vec::with_capacity(CHUNK);
    while read_record(&mut reader, &mut record)? {
        rows += 1;
        ensure!(rows <= MAX_ROWS, "LOINC source exceeds 2000000 terms");
        ensure!(
            record.len() == columns.len(),
            "Wrong LOINC column count on record {}",
            rows + 1
        );
        let fields = columns
            .iter()
            .cloned()
            .zip(record.iter().map(|s| s.into()))
            .collect::<serde_json::Map<_, _>>();
        validate_fields(&fields).with_context(|| format!("Invalid LOINC record {}", rows + 1))?;
        append_copy_field(&mut chunk, fields["LOINC_NUM"].as_str().unwrap());
        chunk.push(b'\t');
        append_copy_field(&mut chunk, &serde_json::to_string(&fields)?);
        chunk.push(b'\n');
        if chunk.len() >= CHUNK {
            send.blocking_send(std::mem::replace(&mut chunk, Vec::with_capacity(CHUNK)))
                .map_err(|_| anyhow::anyhow!("LOINC import cancelled"))?;
        }
    }
    ensure!(rows > 0, "LOINC source has no terms");
    let input = reader.into_inner();
    let sha256 = format!("{:x}", input.sha.finalize());
    ensure!(sha256 == source.sha256, "LOINC source checksum mismatch");
    if !chunk.is_empty() {
        send.blocking_send(chunk)
            .map_err(|_| anyhow::anyhow!("LOINC import cancelled"))?;
    }
    Ok(SourceStats {
        sha256,
        rows,
        bytes: input.bytes,
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
    sqlx::query("CREATE TEMP TABLE loinc_input(code varchar(255) PRIMARY KEY,fields jsonb NOT NULL) ON COMMIT DROP")
        .execute(&mut **tx).await?;
    let (send, mut receive) = tokio::sync::mpsc::channel::<Vec<u8>>(2);
    let worker = tokio::task::spawn_blocking(move || read_source(source, path, send));
    let mut copy = tx
        .copy_in_raw("COPY loinc_input(code,fields) FROM STDIN WITH (FORMAT CSV, DELIMITER E'\\t')")
        .await?;
    while let Some(chunk) = receive.recv().await {
        if let Err(error) = copy.send(chunk).await {
            drop(receive);
            let _ = copy.abort("LOINC transport failed").await;
            let _ = worker.await;
            return Err(error.into());
        }
    }
    let parsed = match worker.await {
        Ok(result) => result,
        Err(error) => {
            let _ = copy.abort("LOINC reader failed").await;
            return Err(error).context("LOINC reader task failed");
        }
    };
    match parsed {
        Ok(stats) => {
            let copied = copy.finish().await?;
            ensure!(copied == stats.rows, "LOINC COPY row count mismatch");
            Ok(stats)
        }
        Err(error) => {
            let _ = copy.abort("LOINC validation failed").await;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quote_guard_rejects_short_malformed_fields_across_read_boundaries() {
        for input in [
            "name,\"unfinished",
            "name,bad\"quote\n",
            "name,\"closed\"junk\n",
        ] {
            let mut guard = QuoteGuard::default();
            let mut result = Ok(());
            for byte in input.bytes() {
                result = guard.input(&[byte]);
                if result.is_err() {
                    break;
                }
            }
            assert!(result.and_then(|_| guard.finish()).is_err(), "{input}");
        }
        let mut guard = QuoteGuard::default();
        for byte in "\u{feff}name,\"quoted,\r\n\"\"μ\"\"\"\r\n".bytes() {
            guard.input(&[byte]).unwrap();
        }
        guard.finish().unwrap();
    }

    #[test]
    fn csv_record_budget_bounds_unterminated_fields_before_full_file_allocation() {
        let input = std::io::Cursor::new([b"\"".as_slice(), &vec![b'x'; MAX_RECORD * 2]].concat());
        let budget = BudgetReader {
            input,
            sha: Sha256::new(),
            bytes: 0,
            record_bytes: 0,
            syntax: QuoteGuard::default(),
        };
        let mut reader = ReaderBuilder::new()
            .has_headers(false)
            .buffer_capacity(CSV_BUFFER)
            .from_reader(budget);
        let error = read_record(&mut reader, &mut StringRecord::new()).unwrap_err();
        assert!(format!("{error:#}").contains("budget"));
        assert_eq!(reader.get_ref().bytes, MAX_RECORD as u64);
    }

    #[test]
    fn csv_decoding_preserves_multiline_escaped_unicode_and_bom() {
        let input = std::io::Cursor::new(
            "\u{feff}name,value\r\n\"A, \'B\'\",\"line 1\r\nline 2 \"\"μ\"\" \\\"\r\n".as_bytes(),
        );
        let budget = BudgetReader {
            input,
            sha: Sha256::new(),
            bytes: 0,
            record_bytes: 0,
            syntax: QuoteGuard::default(),
        };
        let mut reader = ReaderBuilder::new()
            .has_headers(false)
            .buffer_capacity(CSV_BUFFER)
            .from_reader(budget);
        let mut record = StringRecord::new();
        assert!(read_record(&mut reader, &mut record).unwrap());
        assert_eq!(record.get(0), Some("name"));
        assert!(read_record(&mut reader, &mut record).unwrap());
        assert_eq!(record.get(0), Some("A, 'B'"));
        assert_eq!(record.get(1), Some("line 1\r\nline 2 \"μ\" \\"));
        assert!(!read_record(&mut reader, &mut record).unwrap());
    }
}
