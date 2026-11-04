use super::*;
use axum::extract::Path;
use futures_util::StreamExt;
use sha2::Sha256;
use sqlx::Row;
use term_squid_ingestion::{inspect_upload, InspectedUpload, UploadFormat};
use tokio::{
    io::AsyncWriteExt,
    time::{Duration, MissedTickBehavior},
};

fn file(state: &AdminState, id: Uuid) -> PathBuf {
    state
        .config
        .as_ref()
        .expect("Configured worker")
        .staging
        .join(format!("{id}.json"))
}
const ACTIVE: &str = "('uploading','inspecting','ready','queued','validating','publishing')";

pub async fn initialize(state: Shared) -> anyhow::Result<()> {
    let Some(config) = &state.config else {
        return Ok(());
    };
    tokio::fs::create_dir_all(&config.staging).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&config.staging, std::fs::Permissions::from_mode(0o700)).await?;
    }
    // A session advisory lock elects one job owner across configured backend processes.
    let mut lease = state.pool.acquire().await?;
    let acquired: bool =
        sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtext(current_schema()), 6400123)")
            .fetch_one(&mut *lease)
            .await?;
    anyhow::ensure!(
        acquired,
        "Another administration worker owns this database; run one configured admin backend"
    );
    // Publication and success commit together; unfinished jobs are never silently retried.
    recover(&state).await?;
    cleanup(&state).await?;
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_millis(500));
        timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut maintained_at = tokio::time::Instant::now();
        loop {
            timer.tick().await;
            // Losing the lease ends the worker; a new process may perform recovery safely.
            if sqlx::query("SELECT 1").execute(&mut *lease).await.is_err() {
                state.available.store(false, Ordering::SeqCst);
                tracing::error!("Administration lease lost; worker stopped");
                break;
            }
            match claim(&state).await {
                Ok(Some(id)) => process(state.clone(), id).await,
                Ok(None) => {}
                Err(e) => tracing::error!(%e,"Cannot claim import job"),
            }
            if maintained_at.elapsed() >= Duration::from_secs(60) {
                if let Err(e) = cleanup(&state).await {
                    tracing::error!(%e,"Staging cleanup failed");
                }
                maintained_at = tokio::time::Instant::now();
            }
        }
    });
    Ok(())
}

pub(super) async fn recover(state: &AdminState) -> anyhow::Result<()> {
    sqlx::query("UPDATE admin_import_jobs SET state='interrupted',error='Server restarted before activation; upload again',updated_at=NOW() WHERE state IN ('uploading','inspecting','queued','validating','publishing')")
        .execute(&state.pool).await?;
    Ok(())
}
async fn cleanup(state: &AdminState) -> anyhow::Result<()> {
    sqlx::query("UPDATE admin_import_jobs SET state='interrupted',error='Upload disconnected or inspection expired; upload again',updated_at=NOW() WHERE state IN ('uploading','inspecting') AND updated_at<NOW()-INTERVAL '10 minutes'").execute(&state.pool).await?;
    sqlx::query("UPDATE admin_import_jobs SET state='cancelled',error='Unstarted inspection expired after 24 hours',updated_at=NOW() WHERE state='ready' AND updated_at<NOW()-INTERVAL '24 hours'").execute(&state.pool).await?;
    let mut entries = tokio::fs::read_dir(&state.config.as_ref().unwrap().staging).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|s| s.strip_suffix(".json"))
            .and_then(|s| Uuid::parse_str(s).ok())
        else {
            continue;
        };
        let active: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS(SELECT 1 FROM admin_import_jobs WHERE id=$1 AND state IN {ACTIVE})"
        ))
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
        if !active {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
    sqlx::query(&format!("DELETE FROM admin_import_jobs WHERE state NOT IN {ACTIVE} AND updated_at<NOW()-INTERVAL '7 days'" )).execute(&state.pool).await?;
    sqlx::query("DELETE FROM admin_sessions WHERE expires_at<=NOW()")
        .execute(&state.pool)
        .await?;
    Ok(())
}

pub async fn list(State(state): State<Shared>) -> Result<Json<Value>> {
    let rows: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(j) FROM (SELECT id,state,format,bytes,sha256,inspection,result,error,cancel_requested,created_at,updated_at FROM admin_import_jobs ORDER BY created_at DESC LIMIT 100) j")
        .fetch_all(&state.pool).await?;
    Ok(Json(json!({"jobs":rows,"historyLimit":100})))
}
pub async fn detail(State(state): State<Shared>, Path(id): Path<Uuid>) -> Result<Json<Value>> {
    let row: Option<Value> =
        sqlx::query_scalar("SELECT to_jsonb(j) FROM admin_import_jobs j WHERE id=$1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    row.map(Json)
        .ok_or(ApiError(StatusCode::NOT_FOUND, "Import job not found"))
}

/// Only executed after session/origin/CSRF middleware; never buffers a full upload.
pub async fn upload(
    State(state): State<Shared>,
    headers: HeaderMap,
    request: Request,
) -> Result<Json<Value>> {
    let _permit = state.uploads.try_acquire().map_err(|_| {
        ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Two uploads are already in progress",
        )
    })?;
    let format = match headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()) {
        Some("application/fhir+json") => UploadFormat::Json,
        Some("application/gzip") => UploadFormat::Package,
        _ => return Err(ApiError(StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Use application/fhir+json for one R4 resource or application/gzip for a local R4 FHIR package")),
    };
    let byte_limit = format.byte_limit();
    if headers.get(header::CONTENT_ENCODING).is_some() {
        return Err(ApiError(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "HTTP Content-Encoding is unsupported; send the original file bytes",
        ));
    }
    if headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .is_some_and(|v| v > byte_limit)
    {
        return Err(ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Upload exceeds the selected format byte limit",
        ));
    }
    let id = Uuid::new_v4();
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(7382916400124)")
        .execute(&mut *tx)
        .await?;
    let count: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM admin_import_jobs WHERE state IN {ACTIVE}"
    ))
    .fetch_one(&mut *tx)
    .await?;
    if count >= 16 {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Staging capacity reached; start or cancel existing jobs",
        ));
    }
    sqlx::query("INSERT INTO admin_import_jobs(id,state,format) VALUES($1,'uploading',$2)")
        .bind(id)
        .bind(format.name())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let path = file(&state, id);
    let result = tokio::time::timeout(Duration::from_secs(120), async {
        let mut output = tokio::fs::OpenOptions::new().create_new(true).write(true).open(&path).await.map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR,"Cannot stage upload"))?;
        let mut stream = request.into_body().into_data_stream();
        let mut hash = Sha256::new();
        let mut bytes = 0usize;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| ApiError(StatusCode::BAD_REQUEST,"Upload disconnected"))?;
            bytes = bytes.checked_add(chunk.len()).ok_or(ApiError(StatusCode::PAYLOAD_TOO_LARGE,"Upload exceeds the selected format byte limit"))?;
            if bytes>byte_limit { return Err(ApiError(StatusCode::PAYLOAD_TOO_LARGE,"Upload exceeds the selected format byte limit")); }
            hash.update(&chunk);
            output.write_all(&chunk).await.map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR,"Cannot stage upload"))?;
        }
        output.flush().await.map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR,"Cannot stage upload"))?;
        output.sync_all().await.map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR,"Cannot persist upload"))?;
        let sha = format!("{:x}",hash.finalize());
        sqlx::query("UPDATE admin_import_jobs SET state='inspecting',bytes=$2,sha256=$3,updated_at=NOW() WHERE id=$1").bind(id).bind(bytes as i64).bind(&sha).execute(&state.pool).await?;
        let parsed = read_inspect(path.clone(), format).await.map_err(|error| { tracing::warn!(%id,%error,"Upload inspection rejected"); ApiError(StatusCode::UNPROCESSABLE_ENTITY,"Invalid or unsupported upload; check R4 metadata, resource definitions and archive limits") })?;
        sqlx::query("UPDATE admin_import_jobs SET state='ready',inspection=$2,updated_at=NOW() WHERE id=$1").bind(id).bind(sqlx::types::Json(&parsed.0.inspection)).execute(&state.pool).await?;
        Ok::<_,ApiError>(())
    }).await.unwrap_or(Err(ApiError(StatusCode::REQUEST_TIMEOUT,"Upload timed out")));
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(path).await;
        sqlx::query(
            "UPDATE admin_import_jobs SET state='failed',error=$2,updated_at=NOW() WHERE id=$1",
        )
        .bind(id)
        .bind(e.1)
        .execute(&state.pool)
        .await?;
        return Err(e);
    }
    detail(State(state.clone()), Path(id)).await
}
async fn read_inspect(
    path: PathBuf,
    format: UploadFormat,
) -> anyhow::Result<(InspectedUpload, String)> {
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut input = std::fs::File::open(&path)?.take((format.byte_limit() + 1) as u64);
        let mut hash = Sha256::new();
        let mut bytes = 0;
        let mut buffer = [0u8; 65536];
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes += count;
            anyhow::ensure!(bytes <= format.byte_limit(), "Input limit exceeded");
            hash.update(&buffer[..count]);
        }
        let parsed = inspect_upload(&path, format)?;
        Ok((parsed, format!("{:x}", hash.finalize())))
    })
    .await?
}
pub async fn start(State(state): State<Shared>, Path(id): Path<Uuid>) -> Result<Json<Value>> {
    // State compare-and-set makes repeat start requests idempotent.
    sqlx::query("UPDATE admin_import_jobs SET state='queued',updated_at=NOW() WHERE id=$1 AND state='ready'").bind(id).execute(&state.pool).await?;
    detail(State(state.clone()), Path(id)).await
}
pub async fn cancel(State(state): State<Shared>, Path(id): Path<Uuid>) -> Result<Json<Value>> {
    let changed = sqlx::query("UPDATE admin_import_jobs SET cancel_requested=true,state=CASE WHEN state IN ('ready','queued') THEN 'cancelled' ELSE state END,updated_at=NOW() WHERE id=$1 AND state IN ('ready','queued','validating','publishing')")
        .bind(id).execute(&state.pool).await?.rows_affected();
    if changed == 0 {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Job cannot be cancelled in its current state",
        ));
    }
    detail(State(state.clone()), Path(id)).await
}
pub(super) async fn claim(state: &AdminState) -> std::result::Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("UPDATE admin_import_jobs SET state='validating',updated_at=NOW() WHERE id=(SELECT id FROM admin_import_jobs WHERE state='queued' ORDER BY created_at LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING id")
        .fetch_optional(&state.pool).await
}
async fn wait_cancel(state: &AdminState, id: Uuid) -> std::result::Result<(), sqlx::Error> {
    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;
        match sqlx::query_scalar::<_, bool>(
            "SELECT cancel_requested FROM admin_import_jobs WHERE id=$1",
        )
        .bind(id)
        .fetch_one(&state.pool)
        .await
        {
            Ok(false) => {}
            Ok(true) => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}
async fn activate(state: &AdminState, id: Uuid) -> anyhow::Result<()> {
    let (expected, format, inspection): (String, String, Value) =
        sqlx::query_as("SELECT sha256,format,inspection FROM admin_import_jobs WHERE id=$1")
            .bind(id)
            .fetch_one(&state.pool)
            .await?;
    let (parsed, sha) = read_inspect(file(state, id), UploadFormat::from_name(&format)?).await?;
    anyhow::ensure!(sha == expected, "Source checksum mismatch");
    anyhow::ensure!(
        parsed.inspection == inspection,
        "Source inspection mismatch"
    );
    sqlx::query("UPDATE admin_import_jobs SET state='publishing',updated_at=NOW() WHERE id=$1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("SET LOCAL statement_timeout='30s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    let result = parsed.publish(&mut tx).await?;
    // Cancellation and final publication serialize here; a committed success is authoritative.
    let row = sqlx::query("SELECT cancel_requested FROM admin_import_jobs WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    anyhow::ensure!(!row.get::<bool, _>("cancel_requested"), "Import cancelled");
    sqlx::query(
        "UPDATE admin_import_jobs SET state='succeeded',result=$2,updated_at=NOW() WHERE id=$1",
    )
    .bind(id)
    .bind(sqlx::types::Json(result))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
pub(super) async fn process(state: Shared, id: Uuid) {
    let outcome = tokio::select! {
        biased;
        result = tokio::time::timeout(Duration::from_secs(120),activate(&state,id)) => {
            match result { Ok(Ok(())) => None, Ok(Err(error)) => { tracing::warn!(%id,%error,"Import rejected"); Some(("failed","Import rejected: invalid input, immutable version conflict, checksum or publication failure")) }, Err(_) => Some(("failed","Import exceeded the 120-second execution limit")) }
        },
        result = wait_cancel(&state,id) => Some(if result.is_ok() {
            ("cancelled","Import cancelled; no content activated")
        } else {
            ("failed","Cancellation monitor unavailable; import aborted before publication")
        }),
    };
    if let Some((status, error)) = outcome {
        if let Err(e) = sqlx::query("UPDATE admin_import_jobs SET state=CASE WHEN cancel_requested THEN 'cancelled' ELSE $2 END,error=CASE WHEN cancel_requested THEN 'Import cancelled; no content activated' ELSE $3 END,updated_at=NOW() WHERE id=$1 AND state<>'succeeded'").bind(id).bind(status).bind(error).execute(&state.pool).await { tracing::error!(%e,"Cannot finish import job"); }
    }
    let _ = tokio::fs::remove_file(file(&state, id)).await;
}
