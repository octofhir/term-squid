//! Generation-checked hierarchy snapshots over the same projections as atomic SQL.
use super::generation::{GenerationReads, Observation};
use crate::{error::AppError, models::ResolvedSubsumption};
use sqlx::{Executor, PgPool, Postgres};
use std::{collections::BTreeMap, sync::Arc};
use term_squid_core::{
    Canonical, HierarchyConcept, PreparedHierarchy, MAX_CONCEPTS, MAX_HIERARCHY_RELATIONSHIPS,
};
use tokio::sync::Mutex;
use uuid::Uuid;

const CACHE_BYTES: usize = 16 * 1024 * 1024;
const INPUT_BYTES: i64 = 32 * 1024 * 1024;
type Key = (Canonical, Option<Uuid>);

enum Snapshot {
    Prepared {
        plan: PreparedHierarchy,
        url: String,
        version: Option<String>,
    },
    Unsupported {
        meaning: Option<String>,
        url: String,
        version: Option<String>,
    },
    Sql,
}

impl Snapshot {
    fn charge(&self) -> usize {
        match self {
            Self::Prepared { plan, url, version } => {
                plan.memory_charge() + url.capacity() + version.as_ref().map_or(0, String::capacity)
            }
            Self::Unsupported {
                meaning,
                url,
                version,
            } => {
                4096 + meaning.as_ref().map_or(0, String::capacity)
                    + url.capacity()
                    + version.as_ref().map_or(0, String::capacity)
            }
            Self::Sql => 4096,
        }
    }
    fn resolve(&self, a: &str, b: &str, observed: &Observation) -> Read {
        let (url, version, meaning, found) = match self {
            Self::Sql => return Read::Sql,
            Self::Unsupported {
                meaning,
                url,
                version,
            } => (url.clone(), version.clone(), meaning.clone(), None),
            Self::Prepared { plan, url, version } => (
                url.clone(),
                version.clone(),
                Some("is-a".into()),
                Some(plan.resolve(a, b, &observed.key_a, &observed.key_b)),
            ),
        };
        Read::Resolved(ResolvedSubsumption {
            url,
            version,
            hierarchy_meaning: meaning,
            code_a: found.as_ref().and_then(|r| r.code_a).map(str::to_owned),
            code_b: found.as_ref().and_then(|r| r.code_b).map(str::to_owned),
            relationship: found.and_then(|r| r.relationship),
        })
    }
}

pub(super) enum Read {
    Missing,
    Resolved(ResolvedSubsumption),
    Sql,
}

#[derive(sqlx::FromRow)]
struct Metadata {
    url: String,
    version: Option<String>,
    id: Uuid,
    hierarchy_meaning: Option<String>,
    case_sensitive: bool,
    native_provider: Option<String>,
}

#[derive(Default)]
struct Cache {
    generation: i64,
    bytes: usize,
    entries: BTreeMap<Key, (Arc<Snapshot>, usize)>,
}

impl Cache {
    fn advance(&mut self, generation: i64) {
        if generation > self.generation {
            self.entries.clear();
            self.bytes = 0;
            self.generation = generation;
        }
    }
    fn publish(&mut self, generation: i64, key: Key, value: Arc<Snapshot>) {
        self.advance(generation);
        let charge = value.charge()
            + 4096
            + (key.0.url.len() + key.0.version.as_ref().map_or(0, String::len)) * 8;
        if self.generation != generation || charge > CACHE_BYTES {
            return;
        }
        if self.entries.len() >= 8 || self.bytes + charge > CACHE_BYTES {
            self.entries.clear();
            self.bytes = 0;
        }
        if let Some((_, old)) = self.entries.insert(key, (value, charge)) {
            self.bytes -= old;
        }
        self.bytes += charge;
    }
}

#[derive(Default)]
pub(super) struct HierarchyPlans {
    cache: Mutex<Cache>,
    preparation: Arc<Mutex<()>>,
}

impl HierarchyPlans {
    async fn cached(&self, generation: i64, key: &Key) -> Option<Arc<Snapshot>> {
        let mut cache = self.cache.lock().await;
        cache.advance(generation);
        if cache.generation == generation {
            cache.entries.get(key).map(|(value, _)| value.clone())
        } else {
            None
        }
    }

    pub(super) async fn resolve(
        &self,
        pool: &PgPool,
        generations: &GenerationReads,
        root: &Canonical,
        id: Option<Uuid>,
        a: &str,
        b: &str,
    ) -> Result<Read, AppError> {
        // Large external probes bypass preparation, not validation, and never grow the cache.
        let key_bytes = root.url.len() + root.version.as_ref().map_or(0, String::len);
        if a.len() > 4096 || b.len() > 4096 || key_bytes > 4096 {
            return Ok(Read::Sql);
        }
        let key = (root.clone(), id);
        let observed = generations.hierarchy(a, b).await?;
        if let Some(plan) = self.cached(observed.generation, &key).await {
            return Ok(plan.resolve(a, b, &observed));
        }
        // Warm reads never wait for cold admission or hold a cache lock across SQL.
        let admission = self.preparation.clone().lock_owned().await;
        let mut tx = pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let observed = observation(&mut *tx, a, b).await?;
        if let Some(plan) = self.cached(observed.generation, &key).await {
            tx.commit().await?;
            return Ok(plan.resolve(a, b, &observed));
        }
        let metadata = sqlx::query_as::<_,Metadata>(
            "SELECT id,url,version,hierarchy_meaning,case_sensitive,native_provider FROM code_systems WHERE ($3::uuid IS NOT NULL AND id=$3) OR ($3::uuid IS NULL AND url=$1 AND ($2::text IS NULL OR version=$2)) ORDER BY updated_at DESC,id LIMIT 1")
            .bind(&root.url).bind(&root.version).bind(id).fetch_optional(&mut *tx).await?;
        let Some(metadata) = metadata else {
            tx.commit().await?;
            return Ok(Read::Missing);
        };
        let (snapshot, concepts, relationships) = if metadata.hierarchy_meaning.as_deref()
            != Some("is-a")
        {
            (
                Some(Snapshot::Unsupported {
                    meaning: metadata.hierarchy_meaning.clone(),
                    url: metadata.url.clone(),
                    version: metadata.version.clone(),
                }),
                Vec::new(),
                Vec::new(),
            )
        } else if metadata.native_provider.is_some() {
            (Some(Snapshot::Sql), Vec::new(), Vec::new())
        } else {
            let (count, bytes): (i64,i64) = sqlx::query_as(
                "SELECT count(*)::bigint,COALESCE(sum(octet_length(code)+octet_length(comparison_key)),0)::bigint FROM (SELECT code,CASE WHEN $3::boolean THEN code::text ELSE lower(code) END AS comparison_key FROM concepts WHERE code_system_id=$1 LIMIT $2) bounded")
                .bind(metadata.id).bind(MAX_CONCEPTS as i64+1).bind(metadata.case_sensitive).fetch_one(&mut *tx).await?;
            let (edge_count, edge_bytes): (i64, i64) = if count > MAX_CONCEPTS as i64
                || bytes > INPUT_BYTES
            {
                (0, 0)
            } else {
                sqlx::query_as("SELECT count(*)::bigint,COALESCE(sum(octet_length(ancestor_code)+octet_length(descendant_code)),0)::bigint FROM (SELECT ancestor_code,descendant_code FROM closure_table WHERE code_system_id=$1 LIMIT $2) bounded")
                    .bind(metadata.id).bind(MAX_HIERARCHY_RELATIONSHIPS as i64+1).fetch_one(&mut *tx).await?
            };
            if count > MAX_CONCEPTS as i64
                || edge_count > MAX_HIERARCHY_RELATIONSHIPS as i64
                || bytes + edge_bytes > INPUT_BYTES
            {
                (Some(Snapshot::Sql), Vec::new(), Vec::new())
            } else {
                let concepts: Vec<(String,String)> = sqlx::query_as("SELECT code,CASE WHEN $2::boolean THEN code::text ELSE lower(code) END FROM concepts WHERE code_system_id=$1")
                    .bind(metadata.id).bind(metadata.case_sensitive).fetch_all(&mut *tx).await?;
                let relationships: Vec<(String,String)> = sqlx::query_as("SELECT ancestor_code,descendant_code FROM closure_table WHERE code_system_id=$1")
                    .bind(metadata.id).fetch_all(&mut *tx).await?;
                (None, concepts, relationships)
            }
        };
        tx.commit().await?;
        let (_admission, snapshot) = tokio::task::spawn_blocking(move || -> Result<_, AppError> {
            let snapshot = if let Some(snapshot) = snapshot {
                snapshot
            } else {
                let concepts = concepts
                    .into_iter()
                    .map(|(code, comparison_key)| HierarchyConcept {
                        code,
                        comparison_key,
                    })
                    .collect();
                let plan = PreparedHierarchy::from_closure(
                    metadata.case_sensitive,
                    concepts,
                    relationships,
                )?;
                if plan.memory_charge() + 4096 + key_bytes * 8 > CACHE_BYTES {
                    Snapshot::Sql
                } else {
                    Snapshot::Prepared {
                        plan,
                        url: metadata.url,
                        version: metadata.version,
                    }
                }
            };
            Ok((admission, Arc::new(snapshot)))
        })
        .await
        .map_err(|e| AppError::Internal(e.into()))??;
        self.cache
            .lock()
            .await
            .publish(observed.generation, key, snapshot.clone());
        Ok(snapshot.resolve(a, b, &observed))
    }

    #[cfg(test)]
    pub(super) async fn pause(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.preparation.clone().lock_owned().await
    }

    #[cfg(test)]
    pub(super) async fn usage(&self) -> (usize, usize) {
        let cache = self.cache.lock().await;
        (cache.entries.len(), cache.bytes)
    }
}

async fn observation<'e, E: Executor<'e, Database = Postgres>>(
    executor: E,
    a: &str,
    b: &str,
) -> Result<Observation, AppError> {
    // This fresh statement is the warm snapshot boundary. Query keys use precisely
    // the same database lower() semantics as stored case-insensitive projection keys.
    Ok(sqlx::query_as("SELECT generation,lower($1::text) AS key_a,lower($2::text) AS key_b FROM terminology_generation WHERE singleton AND hierarchies_tracked")
        .bind(a).bind(b).fetch_one(executor).await?)
}
