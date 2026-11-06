//! Complete same-edition primary links; raw accessory rows never invent Part concepts.
use super::{reader, SourceStats};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Postgres, Transaction};
use std::path::{Path, PathBuf};
use term_squid_core::loinc::{normalize_part_code, PrimaryAxis, SYSTEM};
use uuid::Uuid;

const FILE: &str = "LoincPartLink_Primary.csv";
const MAX_ROWS: u64 = 20_000_000;
const MAX_FILE: u64 = 4 * 1024 * 1024 * 1024;
const COLUMNS: &[&str] = &[
    "LoincNumber",
    "LongCommonName",
    "PartNumber",
    "PartName",
    "PartCodeSystem",
    "PartTypeName",
    "LinkTypeName",
    "Property",
];

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Source {
    pub path: PathBuf,
    pub sha256: String,
    pub expected_rows: u64,
    pub version: String,
    pub complete_file: bool,
}
pub(super) fn validate(source: &Source, version: &str) -> Result<()> {
    ensure!(
        source.version == version && source.complete_file,
        "Primary links require the same explicitly attested complete LOINC edition"
    );
    ensure!(
        source.path.file_name().and_then(|file| file.to_str()) == Some(FILE),
        "Expected official LoincPartLink_Primary.csv filename"
    );
    ensure!(
        (1..=MAX_ROWS).contains(&source.expected_rows),
        "Primary-link expectedRows must be between 1 and 20000000"
    );
    ensure!(
        source.sha256.len() == 64
            && source
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "Expected lowercase primary-link source SHA-256"
    );
    Ok(())
}
pub(super) fn identity(source: &Source) -> Value {
    json!({"file":FILE,"sha256":source.sha256,"expectedRows":source.expected_rows,
        "version":source.version,"completeFile":true})
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
            sqlx::query("INSERT INTO loinc_primary_links(code_system_id,source_row,loinc_number,long_common_name,part_number,part_name,part_code_system,part_type_name,link_type_name,property,fields) SELECT $1,source_row,loinc_number,long_common_name,part_number,part_name,part_code_system,part_type_name,link_type_name,property,fields FROM loinc_link_input").bind(id).execute(&mut **tx).await?;
            // Coverage/uniqueness and the combined conservative byte charge were
            // checked before either the base or coded property arrays are allocated.
            sqlx::query("UPDATE concepts c SET properties=c.properties || (SELECT jsonb_agg(jsonb_build_object('code',a.code,'valueCode',l.part_number) ORDER BY a.code COLLATE \"C\") FROM loinc_link_input l JOIN loinc_link_axes a ON a.property=l.property WHERE l.loinc_number=c.code AND l.link_type_name='Primary' AND l.part_code_system='http://loinc.org') WHERE c.code_system_id=$1").bind(id).execute(&mut **tx).await?;
        }
        Ok(())
    }
}

pub(super) async fn stage(
    tx: &mut Transaction<'_, Postgres>,
    directory: &Path,
    source: Option<&Source>,
) -> Result<Loaded> {
    let Some(source) = source else {
        return Ok(Loaded {
            loaded: false,
            source: json!({}),
        });
    };
    sqlx::query("CREATE TEMP TABLE loinc_link_input(source_row bigint PRIMARY KEY,loinc_number text NOT NULL,long_common_name text NOT NULL,part_number text NOT NULL,part_name text NOT NULL,part_code_system text NOT NULL,part_type_name text NOT NULL,link_type_name text NOT NULL,property text NOT NULL,fields jsonb NOT NULL) ON COMMIT DROP").execute(&mut **tx).await?;
    let path = if source.path.is_absolute() {
        source.path.clone()
    } else {
        directory.join(&source.path)
    };
    let stats = copy_source(tx, source.clone(), path).await?;
    ensure!(
        stats.rows == source.expected_rows,
        "Primary-link row count does not match expectedRows"
    );
    sqlx::query("CREATE INDEX ON loinc_link_input(loinc_number)")
        .execute(&mut **tx)
        .await?;
    sqlx::query("CREATE INDEX ON loinc_link_input(loinc_number,property) WHERE link_type_name='Primary' AND part_code_system='http://loinc.org'").execute(&mut **tx).await?;
    let unknown: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM loinc_link_input l WHERE NOT EXISTS(SELECT 1 FROM loinc_input t WHERE t.code=l.loinc_number))").fetch_one(&mut **tx).await?;
    ensure!(
        !unknown,
        "Primary-link source references a term outside this Loinc.csv cohort"
    );
    sqlx::query("CREATE TEMP TABLE loinc_link_axes(code text PRIMARY KEY,property text UNIQUE NOT NULL) ON COMMIT DROP").execute(&mut **tx).await?;
    let axes: Vec<_> = PrimaryAxis::all()
        .into_iter()
        .map(|axis| json!({"code":axis.code(),"property":axis.uri()}))
        .collect();
    sqlx::query("INSERT INTO loinc_link_axes SELECT code,property FROM jsonb_to_recordset($1::jsonb) AS a(code text,property text)").bind(sqlx::types::Json(axes)).execute(&mut **tx).await?;
    let invalid: bool = sqlx::query_scalar("WITH counts AS (SELECT loinc_number,property,count(*) AS n FROM loinc_link_input WHERE link_type_name='Primary' AND part_code_system='http://loinc.org' GROUP BY loinc_number,property) SELECT EXISTS(SELECT 1 FROM loinc_input t CROSS JOIN loinc_link_axes a LEFT JOIN counts l ON l.loinc_number=t.code AND l.property=a.property WHERE COALESCE(l.n,0)<>CASE WHEN a.code='METHOD_TYP' AND t.fields->>'METHOD_TYP'='' THEN 0 ELSE 1 END)").fetch_one(&mut **tx).await?;
    ensure!(!invalid, "Primary links require exactly five axes and Method if and only if specified for every main-table term; duplicates are invalid");
    let mismatch: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM loinc_link_input l JOIN loinc_input t ON t.code=l.loinc_number JOIN loinc_link_axes a ON a.property=l.property WHERE l.link_type_name='Primary' AND l.part_code_system='http://loinc.org' AND l.part_name IS DISTINCT FROM t.fields->>a.code)").fetch_one(&mut **tx).await?;
    ensure!(
        !mismatch,
        "Primary PartName must exactly match its same-edition Loinc.csv axis label"
    );
    let inconsistent: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM loinc_link_input WHERE link_type_name='Primary' AND part_code_system='http://loinc.org' GROUP BY part_number HAVING count(DISTINCT part_name)>1)").fetch_one(&mut **tx).await?;
    ensure!(
        !inconsistent,
        "A primary Part identifier has conflicting names in the same source cohort"
    );
    // Eightfold UTF-8 charge covers JSON escaping; fixed space covers all property
    // wrappers/keys. Only the projected columns are charged, not opaque raw content.
    let oversized: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM loinc_input t WHERE 8192+8::bigint*(COALESCE((SELECT sum(octet_length(key)::bigint+octet_length(value)) FROM jsonb_each_text(t.fields) WHERE key=ANY(ARRAY['COMPONENT','PROPERTY','TIME_ASPCT','SYSTEM','SCALE_TYP','METHOD_TYP','EXTERNAL_COPYRIGHT_NOTICE','STATUS','CLASS','CLASSTYPE','ORDER_OBS','CONSUMER_NAME'])),0)+COALESCE((SELECT sum(octet_length(l.part_number)::bigint+octet_length(l.property)) FROM loinc_link_input l WHERE l.loinc_number=t.code AND l.link_type_name='Primary' AND l.part_code_system='http://loinc.org'),0))>4194304)").fetch_one(&mut **tx).await?;
    ensure!(
        !oversized,
        "Combined LOINC primary and coded properties exceed 4 MiB per concept"
    );
    Ok(Loaded {
        loaded: true,
        source: json!({"file":FILE,"sha256":stats.sha256,"rows":stats.rows,
        "bytes":stats.bytes,"version":source.version,"completeFile":true}),
    })
}

fn read_source(
    source: Source,
    path: PathBuf,
    send: tokio::sync::mpsc::Sender<Vec<u8>>,
) -> Result<SourceStats> {
    let mut input = reader::open_csv(&path, MAX_FILE)?;
    let mut record = csv::StringRecord::new();
    ensure!(
        reader::read_record(&mut input, &mut record)?,
        "Empty primary-link source"
    );
    ensure!(
        record.iter().eq(COLUMNS.iter().copied()),
        "Primary-link CSV requires exactly the eight official columns in order"
    );
    let mut rows = 0_u64;
    let mut chunk = Vec::with_capacity(64 * 1024);
    while reader::read_record(&mut input, &mut record)? {
        ensure!(!send.is_closed(), "LOINC primary-link import cancelled");
        rows += 1;
        ensure!(
            rows <= MAX_ROWS,
            "Primary-link source exceeds 20000000 rows"
        );
        ensure!(
            record.len() == COLUMNS.len(),
            "Primary-link CSV requires exactly eight fields"
        );
        ensure!(
            record.iter().all(|value| !value.contains('\0')),
            "NUL in primary-link source"
        );
        for index in [0, 2, 4, 5, 6, 7] {
            ensure!(
                record[index].len() <= 255 && !record[index].chars().any(char::is_control),
                "Primary-link identifiers exceed 255 bytes or contain control characters"
            );
        }
        term_squid_core::loinc::require_primary_code(&record[0])
            .context("Invalid primary-link term identifier")?;
        if &record[6] == "Primary" && &record[4] == SYSTEM {
            let axis = PrimaryAxis::all()
                .into_iter()
                .find(|axis| axis.uri() == &record[7])
                .context("Unsupported primary-link property URI")?;
            ensure!(
                axis.part_type() == &record[5],
                "Primary-link PartTypeName does not match its property URI"
            );
            ensure!(
                normalize_part_code(&record[2])? == record[2],
                "Primary Part identifiers require the canonical LP prefix"
            );
        }
        reader::append_copy_field(&mut chunk, &rows.to_string());
        for value in &record {
            chunk.push(b'\t');
            reader::append_copy_field(&mut chunk, value);
        }
        let fields = COLUMNS
            .iter()
            .zip(record.iter())
            .map(|(name, value)| ((*name).to_owned(), Value::String(value.to_owned())))
            .collect::<serde_json::Map<_, _>>();
        chunk.push(b'\t');
        reader::append_copy_field(&mut chunk, &serde_json::to_string(&fields)?);
        chunk.push(b'\n');
        if chunk.len() >= 64 * 1024 {
            send.blocking_send(std::mem::replace(&mut chunk, Vec::with_capacity(64 * 1024)))
                .map_err(|_| anyhow::anyhow!("LOINC primary-link import cancelled"))?;
        }
    }
    ensure!(rows > 0, "Primary-link source has no rows");
    let stats = input.into_inner().stats(rows, FILE);
    ensure!(
        stats.sha256 == source.sha256,
        "Primary-link source checksum mismatch"
    );
    if !chunk.is_empty() {
        send.blocking_send(chunk)
            .map_err(|_| anyhow::anyhow!("LOINC primary-link import cancelled"))?;
    }
    Ok(stats)
}
async fn copy_source(
    tx: &mut Transaction<'_, Postgres>,
    source: Source,
    path: PathBuf,
) -> Result<SourceStats> {
    let (send, mut receive) = tokio::sync::mpsc::channel::<Vec<u8>>(2);
    let worker = tokio::task::spawn_blocking(move || read_source(source, path, send));
    let mut copy=tx.copy_in_raw("COPY loinc_link_input(source_row,loinc_number,long_common_name,part_number,part_name,part_code_system,part_type_name,link_type_name,property,fields) FROM STDIN WITH(FORMAT CSV,DELIMITER E'\\t')").await?;
    while let Some(chunk) = receive.recv().await {
        if let Err(error) = copy.send(chunk).await {
            drop(receive);
            let _ = copy.abort("LOINC primary-link transport failed").await;
            let _ = worker.await;
            return Err(error.into());
        }
    }
    let result = match worker.await {
        Ok(result) => result,
        Err(error) => {
            let _ = copy.abort("LOINC primary-link reader failed").await;
            return Err(error).context("LOINC primary-link reader task failed");
        }
    };
    match result {
        Ok(stats) => {
            let copied = copy.finish().await?;
            ensure!(
                copied == stats.rows,
                "LOINC primary-link COPY row count mismatch"
            );
            Ok(stats)
        }
        Err(error) => {
            let _ = copy.abort("LOINC primary-link validation failed").await;
            Err(error)
        }
    }
}
