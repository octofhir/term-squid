//! Optional, same-release semantic/attribute files share the original atomic activation.
use super::{reader, SourceStats};
use anyhow::{ensure, Context, Result};
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

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuxiliarySource {
    pub path: PathBuf,
    pub sha256: String,
    pub expected_rows: u64,
    pub version: String,
    pub full_monthly: bool,
}

#[derive(Clone, Copy)]
pub(super) enum Kind {
    SemanticTypes,
    Attributes,
}
impl Kind {
    pub fn file(self) -> &'static str {
        match self {
            Self::SemanticTypes => "RXNSTY.RRF",
            Self::Attributes => "RXNSAT.RRF",
        }
    }
    fn columns(self) -> &'static [&'static str] {
        match self {
            Self::SemanticTypes => &["RXCUI", "TUI", "STN", "STY", "ATUI", "CVF"],
            Self::Attributes => &[
                "RXCUI", "LUI", "SUI", "RXAUI", "STYPE", "CODE", "ATUI", "SATUI", "ATN", "SAB",
                "ATV", "SUPPRESS", "CVF",
            ],
        }
    }
    fn copy(self) -> &'static str {
        match self {
        Self::SemanticTypes=>"COPY rxnorm_sty_input(source_row,rxcui,tui,stn,sty,fields) FROM STDIN WITH (FORMAT CSV, DELIMITER E'\\t')",
        Self::Attributes=>"COPY rxnorm_sat_input(source_row,rxcui,rxaui,stype,code,atui,atn,sab,atv,suppress,fields) FROM STDIN WITH (FORMAT CSV, DELIMITER E'\\t')",
    }
    }
    fn indices(self) -> &'static [usize] {
        match self {
            Self::SemanticTypes => &[0, 1, 2, 3],
            Self::Attributes => &[0, 3, 4, 5, 6, 8, 9, 10, 11],
        }
    }
    pub fn validate(self, source: &AuxiliarySource, version: &str) -> Result<()> {
        ensure!(
            source.version == version && source.full_monthly,
            "Auxiliary source requires the same attested full monthly RxNorm version"
        );
        ensure!(
            source.path.file_name().and_then(|file| file.to_str()) == Some(self.file()),
            "Unexpected RxNorm auxiliary filename; expected {}",
            self.file()
        );
        ensure!(
            source.sha256.len() == 64
                && source
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "Expected lowercase auxiliary source SHA-256"
        );
        ensure!(
            source.expected_rows <= reader::MAX_ROWS
                && (matches!(self, Self::Attributes) || source.expected_rows > 0),
            "RxNorm auxiliary row count is outside supported bounds"
        );
        Ok(())
    }
    pub fn identity(self, source: &AuxiliarySource) -> Value {
        json!({"file":self.file(),"sha256":source.sha256,"expectedRows":source.expected_rows,"version":source.version,"fullMonthly":true})
    }
}

pub(super) struct Loaded {
    pub semantic_types: bool,
    pub attributes: bool,
    pub sources: Value,
}
impl Loaded {
    pub fn enabled(&self) -> bool {
        self.semantic_types || self.attributes
    }
    pub fn result_sources(&self) -> Option<Value> {
        self.enabled().then(|| self.sources.clone())
    }
    pub fn metadata(&self, metadata: &mut Value) {
        let properties = metadata["property"]
            .as_array_mut()
            .expect("RxNorm metadata property inventory");
        if self.semantic_types {
            for (code, kind) in [("TUI", "code"), ("STN", "string"), ("STY", "string")] {
                properties.push(
                    json!({"code":code,"uri":format!("urn:term-squid:rxnorm:{code}"),"type":kind}),
                );
            }
        }
        if self.attributes {
            properties.push(json!({"code":"NDC","uri":"urn:term-squid:rxnorm:NDC","type":"string","description":"Exact current NLM-curated 11-digit NDC; no source-code normalization or inferred mapping"}));
        }
    }
    pub async fn publish(&self, tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<()> {
        if !self.enabled() {
            return Ok(());
        }
        sqlx::query("INSERT INTO rxnorm_semantic_types(code_system_id,source_row,rxcui,tui,stn,sty,fields) SELECT $1,source_row,rxcui,tui,stn,sty,fields FROM rxnorm_sty_input").bind(id).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO rxnorm_attributes(code_system_id,source_row,rxcui,rxaui,stype,code,atui,atn,sab,atv,suppress,fields) SELECT $1,source_row,rxcui,rxaui,stype,code,atui,atn,sab,atv,suppress,fields FROM rxnorm_sat_input").bind(id).execute(&mut **tx).await?;
        sqlx::query("WITH additions AS (SELECT rxcui,jsonb_agg(jsonb_build_object('code',property,kind,value) ORDER BY property,value) AS properties FROM rxnorm_aux_properties GROUP BY rxcui) UPDATE concepts c SET properties=c.properties||a.properties FROM additions a WHERE c.code_system_id=$1 AND c.code=a.rxcui").bind(id).execute(&mut **tx).await?;
        Ok(())
    }
}

pub(super) async fn stage(
    tx: &mut Transaction<'_, Postgres>,
    directory: &Path,
    semantic_types: Option<&AuxiliarySource>,
    attributes: Option<&AuxiliarySource>,
) -> Result<Loaded> {
    let mut loaded = Loaded {
        semantic_types: semantic_types.is_some(),
        attributes: attributes.is_some(),
        sources: json!({}),
    };
    if !loaded.enabled() {
        return Ok(loaded);
    }
    sqlx::raw_sql("CREATE TEMP TABLE rxnorm_sty_input(source_row bigint PRIMARY KEY,rxcui text NOT NULL,tui text NOT NULL,stn text NOT NULL,sty text NOT NULL,fields jsonb NOT NULL) ON COMMIT DROP; CREATE TEMP TABLE rxnorm_sat_input(source_row bigint PRIMARY KEY,rxcui text NOT NULL,rxaui text NOT NULL,stype text NOT NULL,code text NOT NULL,atui text NOT NULL,atn text NOT NULL,sab text NOT NULL,atv text NOT NULL,suppress text NOT NULL,fields jsonb NOT NULL) ON COMMIT DROP").execute(&mut **tx).await?;
    for (kind, source) in [
        (Kind::SemanticTypes, semantic_types),
        (Kind::Attributes, attributes),
    ] {
        if let Some(source) = source {
            let path = if source.path.is_absolute() {
                source.path.clone()
            } else {
                directory.join(&source.path)
            };
            let stats = copy_source(tx, kind, source.clone(), path).await?;
            ensure!(
                stats.rows == source.expected_rows,
                "{} row count does not match expectedRows",
                kind.file()
            );
            loaded.sources[kind.file()] = json!({"file":stats.file,"sha256":stats.sha256,"rows":stats.rows,"bytes":stats.bytes,"version":source.version,"fullMonthly":true});
        }
    }
    sqlx::raw_sql("CREATE INDEX rxnorm_sty_input_concept ON rxnorm_sty_input(rxcui); CREATE INDEX rxnorm_sat_input_concept ON rxnorm_sat_input(rxcui)").execute(&mut **tx).await?;
    if loaded.semantic_types {
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM rxnorm_sty_input GROUP BY rxcui,sty HAVING count(*)>1)",
        )
        .fetch_one(&mut **tx)
        .await?;
        ensure!(!duplicate, "Duplicate RxNorm semantic type for one RXCUI");
        let conflict:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rxnorm_sty_input GROUP BY tui HAVING count(DISTINCT (stn,sty))>1)").fetch_one(&mut **tx).await?;
        ensure!(
            !conflict,
            "Inconsistent RxNorm TUI semantic type labels/tree numbers"
        );
        let missing:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rxnorm_sty_input s WHERE NOT EXISTS(SELECT 1 FROM rxnorm_input a WHERE a.rxcui=s.rxcui)) OR EXISTS(SELECT 1 FROM rxnorm_input a WHERE NOT EXISTS(SELECT 1 FROM rxnorm_sty_input s WHERE s.rxcui=a.rxcui))").fetch_one(&mut **tx).await?;
        ensure!(
            !missing,
            "Full RXNSTY source must reference and cover every CONSO RXCUI"
        );
    }
    if loaded.attributes {
        let invalid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rxnorm_sat_input s WHERE (s.stype='AUI' AND NOT EXISTS(SELECT 1 FROM rxnorm_input a WHERE a.rxaui=s.rxaui AND a.rxcui=s.rxcui)) OR (s.stype='CUI' AND NOT EXISTS(SELECT 1 FROM rxnorm_input a WHERE a.rxcui=s.rxcui)))").fetch_one(&mut **tx).await?;
        ensure!(
            !invalid,
            "RxNorm attribute attachment references a missing or mismatched atom/concept"
        );
        let invalid_ndc:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rxnorm_sat_input s JOIN rxnorm_primary p USING(rxcui) WHERE s.sab='RXNORM' AND s.atn='NDC' AND s.suppress='N' AND (s.stype<>'AUI' OR p.tty NOT IN ('SCD','SBD','GPCK','BPCK') OR length(s.atv)<>11 OR s.atv~'[^0-9]' OR NOT EXISTS(SELECT 1 FROM rxnorm_input a WHERE a.rxaui=s.rxaui AND a.rxcui=s.rxcui AND a.sab='RXNORM' AND a.suppress='N' AND a.tty IN ('SCD','SBD','GPCK','BPCK'))))").fetch_one(&mut **tx).await?;
        ensure!(!invalid_ndc,"Current NLM-curated NDC requires a supported active normalized product atom and exactly 11 digits; no value guessed or silently omitted");
        let conflict:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rxnorm_sat_input s JOIN rxnorm_primary p USING(rxcui) WHERE s.sab='RXNORM' AND s.atn='NDC' AND s.suppress='N' GROUP BY s.atv HAVING count(DISTINCT s.rxcui)>1)").fetch_one(&mut **tx).await?;
        ensure!(!conflict, "Conflicting current NLM-curated NDC assignments");
    }
    sqlx::query("CREATE TEMP TABLE rxnorm_aux_properties ON COMMIT DROP AS SELECT DISTINCT s.rxcui,v.property,v.kind,v.value FROM rxnorm_sty_input s JOIN rxnorm_primary p USING(rxcui) CROSS JOIN LATERAL (VALUES ('TUI','valueCode',s.tui),('STN','valueString',s.stn),('STY','valueString',s.sty)) v(property,kind,value) WHERE v.value<>'' UNION SELECT DISTINCT s.rxcui,'NDC','valueString',s.atv FROM rxnorm_sat_input s JOIN rxnorm_primary p USING(rxcui) WHERE s.sab='RXNORM' AND s.atn='NDC' AND s.suppress='N'").execute(&mut **tx).await?;
    // Count original and optional properties together, before either JSON aggregation.
    let oversized:bool=sqlx::query_scalar("WITH property_values AS (SELECT DISTINCT a.rxcui,'SAB' AS property,a.sab AS value FROM rxnorm_input a JOIN rxnorm_primary p USING(rxcui) UNION ALL SELECT DISTINCT a.rxcui,'TTY',a.tty FROM rxnorm_input a JOIN rxnorm_primary p USING(rxcui) WHERE a.sab='RXNORM' AND a.tty<>'SY' UNION ALL SELECT rxcui,property,value FROM rxnorm_aux_properties UNION ALL SELECT rxcui,'preferredTTY',tty FROM rxnorm_primary UNION ALL SELECT rxcui,'inactive','false' FROM rxnorm_primary UNION ALL SELECT rxcui,'source-table','RXNCONSO.RRF' FROM rxnorm_primary UNION ALL SELECT rxcui,'rxnorm-status','active' FROM rxnorm_primary) SELECT EXISTS(SELECT 1 FROM property_values GROUP BY rxcui HAVING sum(512::bigint+16::bigint*(octet_length(property)+octet_length(value)))>4194304)").fetch_one(&mut **tx).await?;
    ensure!(
        !oversized,
        "Combined RxNorm property projection exceeds 4 MiB budget; no auxiliary values truncated"
    );
    Ok(loaded)
}

fn read_source(
    kind: Kind,
    source: AuxiliarySource,
    path: PathBuf,
    send: tokio::sync::mpsc::Sender<Vec<u8>>,
) -> Result<SourceStats> {
    ensure!(
        std::fs::metadata(&path)?.is_file(),
        "RxNorm auxiliary source must be a regular file"
    );
    let file = File::open(&path).context("Cannot open RxNorm auxiliary source")?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= reader::MAX_FILE,
        "RxNorm auxiliary source must be a regular file of at most 4 GiB"
    );
    let mut input = BufReader::new(file);
    let mut sha = Sha256::new();
    let mut rows = 0_u64;
    let mut bytes = 0_u64;
    let mut line = Vec::new();
    let mut chunk = Vec::with_capacity(64 * 1024);
    loop {
        ensure!(!send.is_closed(), "RxNorm auxiliary import cancelled");
        line.clear();
        let read = (&mut input)
            .take(1024 * 1024 + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }
        ensure!(
            read <= 1024 * 1024,
            "RxNorm auxiliary source line exceeds 1 MiB"
        );
        bytes += read as u64;
        ensure!(
            bytes <= reader::MAX_FILE,
            "RxNorm auxiliary source exceeds 4 GiB"
        );
        sha.update(&line);
        let text = std::str::from_utf8(&line).context("RxNorm auxiliary RRF must be UTF-8")?;
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
            .context("RxNorm auxiliary rows require a trailing pipe")?;
        let values: Vec<_> = text.split('|').take(kind.columns().len() + 1).collect();
        ensure!(
            values.len() == kind.columns().len(),
            "Wrong {} field count",
            kind.file()
        );
        ensure!(
            !values
                .iter()
                .any(|value| value.chars().any(char::is_control)),
            "Control character in RxNorm auxiliary field"
        );
        match kind {
            Kind::SemanticTypes => {
                term_squid_core::rxnorm::require_code(values[0])?;
                ensure!([1,2,3].into_iter().all(|index|!values[index].is_empty())&&values[1].len()<=255&&values[2].len()<=255,"Semantic type identifiers/name must be nonempty and identifiers at most 255 bytes");
                use term_squid_core::rxnorm::SemanticTypeField;
                ensure!(
                    SemanticTypeField::Tui.valid(values[1])
                        && SemanticTypeField::Stn.valid(values[2]),
                    "Invalid RxNorm semantic type identifier/tree number"
                );
            }
            Kind::Attributes => {
                if !values[0].is_empty() {
                    term_squid_core::rxnorm::require_code(values[0])?;
                }
                ensure!(
                    [4, 8, 9]
                        .into_iter()
                        .all(|index| !values[index].is_empty() && values[index].len() <= 255),
                    "Attribute STYPE/ATN/SAB must be nonempty and at most 255 bytes"
                );
                if values[4] == "AUI" {
                    term_squid_core::rxnorm::require_code(values[3])?;
                    term_squid_core::rxnorm::require_code(values[0])?;
                }
                if values[4] == "CUI" {
                    term_squid_core::rxnorm::require_code(values[0])?;
                }
                ensure!(
                    matches!(values[11], "N" | "O" | "Y"),
                    "Unknown RxNorm attribute SUPPRESS value"
                );
            }
        }
        rows += 1;
        ensure!(
            rows <= reader::MAX_ROWS,
            "RxNorm auxiliary source exceeds 20000000 rows"
        );
        reader::field(&mut chunk, &rows.to_string());
        for index in kind.indices() {
            chunk.push(b'\t');
            reader::field(&mut chunk, values[*index]);
        }
        let fields = kind
            .columns()
            .iter()
            .zip(&values)
            .map(|(column, value)| ((*column).to_owned(), Value::String((*value).to_owned())))
            .collect::<serde_json::Map<_, _>>();
        chunk.push(b'\t');
        reader::field(&mut chunk, &serde_json::to_string(&fields)?);
        chunk.push(b'\n');
        if chunk.len() >= 64 * 1024 {
            send.blocking_send(std::mem::replace(&mut chunk, Vec::with_capacity(64 * 1024)))
                .map_err(|_| anyhow::anyhow!("RxNorm auxiliary import cancelled"))?;
        }
    }
    let sha256 = hex::encode(sha.finalize());
    ensure!(
        sha256 == source.sha256,
        "RxNorm auxiliary source checksum mismatch"
    );
    if !chunk.is_empty() {
        send.blocking_send(chunk)
            .map_err(|_| anyhow::anyhow!("RxNorm auxiliary import cancelled"))?;
    }
    Ok(SourceStats {
        sha256,
        rows,
        bytes,
        file: kind.file().into(),
    })
}

async fn copy_source(
    tx: &mut Transaction<'_, Postgres>,
    kind: Kind,
    source: AuxiliarySource,
    path: PathBuf,
) -> Result<SourceStats> {
    let (send, mut receive) = tokio::sync::mpsc::channel::<Vec<u8>>(2);
    let worker = tokio::task::spawn_blocking(move || read_source(kind, source, path, send));
    let mut copy = tx.copy_in_raw(kind.copy()).await?;
    while let Some(chunk) = receive.recv().await {
        if let Err(error) = copy.send(chunk).await {
            drop(receive);
            let _ = copy.abort("RxNorm auxiliary transport failed").await;
            let _ = worker.await;
            return Err(error.into());
        }
    }
    let parsed = match worker.await {
        Ok(result) => result,
        Err(error) => {
            let _ = copy.abort("RxNorm auxiliary reader failed").await;
            return Err(error).context("RxNorm auxiliary reader task failed");
        }
    };
    match parsed {
        Ok(stats) => {
            let count = copy.finish().await?;
            ensure!(
                count == stats.rows,
                "RxNorm auxiliary COPY row count mismatch"
            );
            Ok(stats)
        }
        Err(error) => {
            let _ = copy.abort("RxNorm auxiliary validation failed").await;
            Err(error)
        }
    }
}
