//! Bounded CodeSystem payload snapshots over stored concept projections.
use super::generation::{Family, GenerationReads, Observation};
use crate::{error::AppError, models::ResolvedConcept};
use sqlx::PgPool;
use std::{collections::BTreeMap, sync::Arc};
use term_squid_core::{Canonical, PreparedConcepts, ProjectedConcept, MAX_CONCEPTS};
use tokio::sync::Mutex;
use uuid::Uuid;

const CACHE_BYTES: usize = 32 * 1024 * 1024;
const INPUT_BYTES: i64 = 32 * 1024 * 1024;
const PROPERTY_BYTES: i64 = 64 * 1024;
type Key = (Canonical, Option<Uuid>);

#[derive(sqlx::FromRow)]
struct Metadata {
    url: String,
    native_provider: Option<String>,
    id: Uuid,
    name: Option<String>,
    version: Option<String>,
    content_mode: Option<String>,
    case_sensitive: bool,
    bytes: i64,
}
#[derive(sqlx::FromRow)]
struct RawConcept {
    code: String,
    comparison_key: String,
    display: Option<String>,
    definition: Option<String>,
    properties: Option<String>,
}
enum Snapshot {
    Prepared {
        metadata: Box<Metadata>,
        concepts: PreparedConcepts,
    },
    Sql,
}
impl Snapshot {
    fn exact(&self) -> bool {
        matches!(self, Self::Prepared { metadata, .. } if metadata.case_sensitive)
    }
    fn resolve(&self, code: &str, observed: &Observation) -> Read {
        match self {
            Self::Sql => Read::Sql,
            Self::Prepared { metadata, concepts } => {
                let found = concepts.resolve(code, &observed.key_a);
                Read::Resolved(ResolvedConcept {
                    url: metadata.url.clone(),
                    native_provider: metadata.native_provider.clone(),
                    name: metadata.name.clone(),
                    version: metadata.version.clone(),
                    content_mode: metadata.content_mode.clone(),
                    code: found.map(|c| c.code.clone()),
                    display: found.and_then(|c| c.display.clone()),
                    definition: found.and_then(|c| c.definition.clone()),
                    properties: found
                        .and_then(|c| c.properties.clone())
                        .map(sqlx::types::Json),
                })
            }
        }
    }
}
// Keep the bounded payload inline: boxing it would allocate on every warm read.
#[allow(clippy::large_enum_variant)]
pub(super) enum Read {
    Missing,
    Resolved(ResolvedConcept),
    Sql,
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
            self.generation = generation;
            self.entries.clear();
            self.bytes = 0;
        }
    }
    fn publish(&mut self, generation: i64, key: Key, value: Arc<Snapshot>, charge: usize) {
        self.advance(generation);
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
pub(super) struct ConceptPlans {
    cache: Mutex<Cache>,
    preparation: Arc<Mutex<()>>,
}
impl ConceptPlans {
    async fn cached(&self, generation: i64, key: &Key) -> Option<Arc<Snapshot>> {
        let mut cache = self.cache.lock().await;
        cache.advance(generation);
        if generation == cache.generation {
            cache.entries.get(key).map(|(plan, _)| plan.clone())
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
        code: &str,
    ) -> Result<Read, AppError> {
        let key_bytes = root.url.len() + root.version.as_ref().map_or(0, String::len);
        if code.len() > 4096 || key_bytes > 4096 {
            return Ok(Read::Sql);
        }
        let key = (root.clone(), id);
        let retained = {
            let cache = self.cache.lock().await;
            cache
                .entries
                .get(&key)
                .map(|(plan, _)| (cache.generation, Arc::downgrade(plan), plan.exact()))
        };
        let exact = retained.as_ref().is_some_and(|(_, _, exact)| *exact);
        // A retained payload is only a hint until a new SQL observation confirms its
        // generation. Case-sensitive plans need no PostgreSQL comparison-key folding.
        // Weak ownership cannot retain retired payloads while SQL checkout is waiting.
        let observed = if exact {
            Observation {
                generation: generations.current(Family::Concept).await?,
                ..Observation::default()
            }
        } else {
            generations.concept(code).await?
        };
        if let Some(plan) = retained
            .filter(|(generation, _, _)| *generation == observed.generation)
            .and_then(|(_, plan, _)| plan.upgrade())
        {
            return Ok(plan.resolve(code, &observed));
        }
        if let Some(plan) = self.cached(observed.generation, &key).await {
            // Another request may have installed a folded plan after our exact hint.
            // In that case obtain its comparison key in the preparation transaction.
            if !exact || plan.exact() {
                return Ok(plan.resolve(code, &observed));
            }
        }
        let admission = self.preparation.clone().lock_owned().await;
        let mut tx = pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let observed: Observation = sqlx::query_as("SELECT generation,lower($1::text) AS key_a,''::text AS key_b FROM terminology_generation WHERE singleton AND hierarchies_tracked")
            .bind(code).fetch_one(&mut *tx).await?;
        if let Some(plan) = self.cached(observed.generation, &key).await {
            tx.commit().await?;
            return Ok(plan.resolve(code, &observed));
        }
        let metadata = sqlx::query_as::<_,Metadata>("SELECT id,url,name,version,native_provider,CASE WHEN native_provider IS NOT NULL THEN 'complete' WHEN octet_length(content_mode)<=65536 THEN content_mode END AS content_mode,case_sensitive,(octet_length(url)+COALESCE(octet_length(name),0)+COALESCE(octet_length(version),0)+COALESCE(octet_length(content_mode),0)+COALESCE(octet_length(native_provider),0))::bigint AS bytes FROM code_systems WHERE ($3::uuid IS NOT NULL AND id=$3) OR ($3::uuid IS NULL AND url=$1 AND ($2::text IS NULL OR version=$2)) ORDER BY updated_at DESC,id LIMIT 1")
            .bind(&root.url).bind(&root.version).bind(id).fetch_optional(&mut *tx).await?;
        let Some(metadata) = metadata else {
            tx.commit().await?;
            return Ok(Read::Missing);
        };
        let rows = if metadata.bytes > PROPERTY_BYTES {
            None
        } else {
            let (count,bytes,max_property): (i64,i64,i64) = sqlx::query_as("SELECT count(*)::bigint,COALESCE(sum(octet_length(code)::bigint+octet_length(comparison_key)+COALESCE(octet_length(display),0)+COALESCE(octet_length(definition),0)+COALESCE(octet_length(properties),0)),0)::bigint,COALESCE(max(octet_length(properties)),0)::bigint FROM (SELECT code,CASE WHEN $3::boolean THEN code::text ELSE lower(code) END AS comparison_key,display,definition,properties::text FROM concepts WHERE code_system_id=$1 LIMIT $2) bounded")
                .bind(metadata.id).bind(MAX_CONCEPTS as i64+1).bind(metadata.case_sensitive).fetch_one(&mut *tx).await?;
            if count > MAX_CONCEPTS as i64 || bytes > INPUT_BYTES || max_property > PROPERTY_BYTES {
                None
            } else {
                Some(sqlx::query_as::<_,RawConcept>("SELECT code,CASE WHEN $2::boolean THEN code::text ELSE lower(code) END AS comparison_key,display,definition,properties::text FROM concepts WHERE code_system_id=$1")
                    .bind(metadata.id).bind(metadata.case_sensitive).fetch_all(&mut *tx).await?)
            }
        };
        tx.commit().await?;
        let (_admission, snapshot, charge) =
            tokio::task::spawn_blocking(move || -> Result<_, AppError> {
                let base_charge = 8192
                    + metadata.url.capacity()
                    + key_bytes * 8
                    + metadata.name.as_ref().map_or(0, String::capacity)
                    + metadata.version.as_ref().map_or(0, String::capacity)
                    + metadata.content_mode.as_ref().map_or(0, String::capacity);
                let (snapshot, charge) = if let Some(rows) = rows {
                    // Account each decoded payload before accumulating a complete index.
                    // At most one <=64 KiB property document can exceed this estimate at once.
                    let mut charge = base_charge
                        + rows.len() * (std::mem::size_of::<ProjectedConcept>() + 64) * 2;
                    let mut projected = Vec::with_capacity(rows.len());
                    for row in rows {
                        let concept = ProjectedConcept {
                            code: row.code,
                            display: row.display,
                            definition: row.definition,
                            properties: row
                                .properties
                                .map(|value| serde_json::from_str(&value))
                                .transpose()
                                .map_err(|e| AppError::Internal(e.into()))?,
                        };
                        charge += row.comparison_key.capacity() + concept.memory_charge();
                        if charge > CACHE_BYTES {
                            return Ok((admission, Arc::new(Snapshot::Sql), base_charge));
                        }
                        projected.push((row.comparison_key, concept));
                    }
                    let concepts = PreparedConcepts::from_rows(metadata.case_sensitive, projected)?;
                    let charge = base_charge + concepts.memory_charge();
                    if charge > CACHE_BYTES {
                        (Snapshot::Sql, base_charge)
                    } else {
                        (
                            Snapshot::Prepared {
                                metadata: Box::new(metadata),
                                concepts,
                            },
                            charge,
                        )
                    }
                } else {
                    (Snapshot::Sql, base_charge)
                };
                Ok((admission, Arc::new(snapshot), charge))
            })
            .await
            .map_err(|e| AppError::Internal(e.into()))??;
        self.cache
            .lock()
            .await
            .publish(observed.generation, key, snapshot.clone(), charge);
        Ok(snapshot.resolve(code, &observed))
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
