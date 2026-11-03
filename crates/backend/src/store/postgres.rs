use crate::error::AppError;
use crate::models::{
    CodeSystem, Concept, ConceptMap, ExpandedValueSet, ResolvedConcept, ResolvedSubsumption,
    SearchParams, ValueSet,
};
use crate::store::TerminologyStore;
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

/// PostgreSQL implementation of TerminologyStore
pub struct PostgresStore {
    pool: PgPool,
    hierarchies: super::hierarchy::HierarchyPlans,
    concepts: super::concepts::ConceptPlans,
    generations: super::generation::GenerationReads,
    prepared: std::sync::Arc<tokio::sync::Mutex<PlanCache>>,
    preparation: std::sync::Arc<tokio::sync::Mutex<()>>,
    maps: tokio::sync::Mutex<MapCache>,
    map_preparation: std::sync::Arc<tokio::sync::Mutex<()>>,
}

// Cache locking never spans SQL or preparation. Generations only advance: an older
// in-flight reader cannot evict or publish over a newer immutable snapshot.
#[derive(Default)]
struct PlanCache {
    generation: i64,
    entries: std::collections::BTreeMap<PlanKey, (ExpandedValueSet, usize)>,
    bytes: usize,
}
type PlanKey = (
    term_squid_core::Canonical,
    Option<Uuid>,
    Option<(String, String)>,
);
const PLAN_CACHE_BYTES: usize = 64 * 1024 * 1024;

impl PlanCache {
    fn publish(&mut self, generation: i64, key: PlanKey, result: ExpandedValueSet, charge: usize) {
        self.advance(generation);
        if generation != self.generation || charge > PLAN_CACHE_BYTES {
            return;
        }
        if self.entries.len() >= 8 || self.bytes + charge > PLAN_CACHE_BYTES {
            self.entries.clear();
            self.bytes = 0;
        }
        if let Some((_, previous_charge)) = self.entries.insert(key, (result, charge)) {
            self.bytes -= previous_charge;
        }
        self.bytes += charge;
    }
    fn advance(&mut self, generation: i64) {
        if generation > self.generation {
            self.entries.clear();
            self.bytes = 0;
            self.generation = generation;
        }
    }
}

type MapKey = (term_squid_core::Canonical, Option<Uuid>);
const MAP_CACHE_BYTES: usize = 16 * 1024 * 1024;
#[derive(Default)]
struct MapCache {
    generation: i64,
    bytes: usize,
    entries: std::collections::BTreeMap<
        MapKey,
        (std::sync::Arc<term_squid_core::PreparedConceptMap>, usize),
    >,
}
impl MapCache {
    fn advance(&mut self, generation: i64) {
        if generation > self.generation {
            self.entries.clear();
            self.bytes = 0;
            self.generation = generation;
        }
    }
}

impl PostgresStore {
    pub fn new(pool: PgPool) -> Self {
        Self {
            generations: super::generation::GenerationReads::new(pool.clone()),
            pool,
            hierarchies: super::hierarchy::HierarchyPlans::default(),
            concepts: super::concepts::ConceptPlans::default(),
            prepared: std::sync::Arc::new(tokio::sync::Mutex::new(PlanCache::default())),
            preparation: std::sync::Arc::new(tokio::sync::Mutex::new(())),
            maps: tokio::sync::Mutex::new(MapCache::default()),
            map_preparation: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        }
    }
    #[cfg(test)]
    pub(crate) async fn pause_preparation(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.preparation.clone().lock_owned().await
    }
    #[cfg(test)]
    pub(crate) async fn pause_hierarchy_preparation(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.hierarchies.pause().await
    }
    #[cfg(test)]
    pub(crate) async fn hierarchy_cache_usage(&self) -> (usize, usize) {
        self.hierarchies.usage().await
    }
    #[cfg(test)]
    pub(crate) async fn pause_generation(
        &self,
        delivery: bool,
    ) -> tokio::sync::OwnedMutexGuard<()> {
        self.generations.pause(delivery).await
    }
    #[cfg(test)]
    pub(crate) fn generation_stats(&self) -> (usize, usize, usize) {
        self.generations.stats()
    }
    #[cfg(test)]
    pub(crate) async fn pause_concept_preparation(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.concepts.pause().await
    }
    #[cfg(test)]
    pub(crate) async fn concept_cache_usage(&self) -> (usize, usize) {
        self.concepts.usage().await
    }
    async fn prepared_concept(
        &self,
        root: term_squid_core::Canonical,
        id: Option<Uuid>,
        code: &str,
    ) -> Result<Option<ResolvedConcept>, AppError> {
        match self
            .concepts
            .resolve(&self.pool, &self.generations, &root, id, code)
            .await?
        {
            super::concepts::Read::Missing => Ok(None),
            super::concepts::Read::Resolved(row) => Ok(Some(row)),
            super::concepts::Read::Sql => {
                self.resolve_row(&root.url, root.version.as_deref(), code, id)
                    .await
            }
        }
    }
    async fn prepared_subsumption(
        &self,
        root: term_squid_core::Canonical,
        id: Option<Uuid>,
        a: &str,
        b: &str,
    ) -> Result<Option<ResolvedSubsumption>, AppError> {
        match self
            .hierarchies
            .resolve(&self.pool, &self.generations, &root, id, a, b)
            .await?
        {
            super::hierarchy::Read::Missing => Ok(None),
            super::hierarchy::Read::Resolved(row) => Ok(Some(row)),
            super::hierarchy::Read::Sql => {
                self.subsumption_row(&root.url, root.version.as_deref(), a, b, id)
                    .await
            }
        }
    }
    async fn cached_plan(&self, generation: i64, key: &PlanKey) -> Option<ExpandedValueSet> {
        let mut cache = self.prepared.lock().await;
        cache.advance(generation);
        (cache.generation == generation)
            .then(|| cache.entries.get(key).map(|(plan, _)| plan.clone()))
            .flatten()
    }
    async fn cached_map(
        &self,
        generation: i64,
        key: &MapKey,
    ) -> Option<std::sync::Arc<term_squid_core::PreparedConceptMap>> {
        let mut cache = self.maps.lock().await;
        cache.advance(generation);
        if cache.generation == generation {
            cache.entries.get(key).map(|(plan, _)| plan.clone())
        } else {
            None
        }
    }
    async fn map_snapshot(
        &self,
        url: &str,
        version: Option<&str>,
        id: Option<Uuid>,
    ) -> Result<std::sync::Arc<term_squid_core::PreparedConceptMap>, AppError> {
        let root = term_squid_core::Canonical::parse(url, version)?;
        let key = (root.clone(), id);
        let observed = self
            .generations
            .current(super::generation::Family::Map)
            .await?;
        if let Some(plan) = self.cached_map(observed, &key).await {
            return Ok(plan);
        }
        let admission = self.map_preparation.clone().lock_owned().await;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let generation: i64 = sqlx::query_scalar(
            "SELECT generation FROM terminology_generation WHERE singleton AND maps_tracked",
        )
        .fetch_one(&mut *tx)
        .await?;
        if let Some(plan) = self.cached_map(generation, &key).await {
            tx.commit().await?;
            return Ok(plan);
        }
        let row: Option<Option<sqlx::types::Json<serde_json::Value>>> = sqlx::query_scalar(
            "SELECT CASE WHEN octet_length(content::text) <= 33554432 THEN content END FROM concept_maps WHERE ($3::uuid IS NOT NULL AND id=$3) OR ($3::uuid IS NULL AND url=$1 AND ($2::text IS NULL OR version=$2)) ORDER BY updated_at DESC,id LIMIT 1")
            .bind(&root.url).bind(&root.version).bind(id).fetch_optional(&mut *tx).await?;
        let content = row
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "ConceptMap {} version {:?} not found",
                    root.url, root.version
                ))
            })?
            .ok_or_else(|| AppError::BadRequest("ConceptMap input exceeds 32 MiB".into()))?
            .0;
        tx.commit().await?;
        let (_admission, plan) = tokio::task::spawn_blocking(move || -> Result<_, AppError> {
            let plan = std::sync::Arc::new(term_squid_core::PreparedConceptMap::prepare(&content)?);
            Ok((admission, plan))
        })
        .await
        .map_err(|e| AppError::Internal(e.into()))??;
        let charge = plan.memory_charge()
            + 4096
            + (root.url.len() + root.version.as_ref().map_or(0, String::len)) * 8;
        let mut cache = self.maps.lock().await;
        cache.advance(generation);
        if generation == cache.generation && charge <= MAP_CACHE_BYTES {
            if cache.entries.len() >= 8 || cache.bytes + charge > MAP_CACHE_BYTES {
                cache.entries.clear();
                cache.bytes = 0;
            }
            if let Some((_, previous)) = cache.entries.insert(key, (plan.clone(), charge)) {
                cache.bytes -= previous;
            }
            cache.bytes += charge;
        }
        Ok(plan)
    }
    async fn resolve_row(
        &self,
        url: &str,
        version: Option<&str>,
        code: &str,
        id: Option<Uuid>,
    ) -> Result<Option<ResolvedConcept>, AppError> {
        // A missing concept still returns system metadata, distinguishing unknown content.
        Ok(sqlx::query_as::<_, ResolvedConcept>(r#"
            SELECT s.name, s.version, s.content_mode, c.code, c.display, c.definition, c.properties
            FROM (SELECT id, name, version, content_mode, case_sensitive FROM code_systems
                  WHERE ($4::uuid IS NOT NULL AND id = $4) OR ($4::uuid IS NULL AND url = $1 AND ($2::text IS NULL OR version = $2))
                  ORDER BY updated_at DESC, id LIMIT 1) s
            LEFT JOIN LATERAL (
                SELECT code, display, definition, properties FROM concepts
                WHERE code_system_id = s.id AND
                    ((s.case_sensitive AND code = $3) OR
                     (NOT s.case_sensitive AND lower(code) = lower($3)))
                LIMIT 1
            ) c ON true
        "#).bind(url).bind(version).bind(code).bind(id)
            .fetch_optional(&self.pool).await?)
    }
    async fn subsumption_row(
        &self,
        url: &str,
        version: Option<&str>,
        code_a: &str,
        code_b: &str,
        id: Option<Uuid>,
    ) -> Result<Option<ResolvedSubsumption>, AppError> {
        // One statement is the operation's snapshot boundary, including CLI commits.
        // Read only indexed projections: no full resource or concept JSON is decoded.
        // Separate case-policy branches keep exact code equality in index bounds;
        // a mixed OR can make even case-sensitive lookup scan the whole system.
        Ok(sqlx::query_as::<_, ResolvedSubsumption>(
            r#"
            SELECT s.hierarchy_meaning, a.code AS code_a, b.code AS code_b,
                   CASE WHEN s.hierarchy_meaning = 'is-a' AND a.code <> b.code THEN
                       CASE WHEN EXISTS (
                           SELECT 1 FROM closure_table
                           WHERE code_system_id = s.id
                             AND ancestor_code = a.code AND descendant_code = b.code
                       ) THEN true
                       WHEN EXISTS (
                           SELECT 1 FROM closure_table
                           WHERE code_system_id = s.id
                             AND ancestor_code = b.code AND descendant_code = a.code
                       ) THEN false END
                   END AS relationship
            FROM (SELECT id, hierarchy_meaning, case_sensitive FROM code_systems
                  WHERE ($5::uuid IS NOT NULL AND id = $5) OR
                        ($5::uuid IS NULL AND url = $1 AND ($2::text IS NULL OR version = $2))
                  ORDER BY updated_at DESC, id LIMIT 1) s
            LEFT JOIN LATERAL (
                SELECT code FROM concepts
                WHERE s.case_sensitive AND code_system_id = s.id AND code = $3
                UNION ALL
                SELECT code FROM concepts
                WHERE NOT s.case_sensitive AND code_system_id = s.id AND lower(code) = lower($3)
                LIMIT 1
            ) a ON true
            LEFT JOIN LATERAL (
                SELECT code FROM concepts
                WHERE s.case_sensitive AND code_system_id = s.id AND code = $4
                UNION ALL
                SELECT code FROM concepts
                WHERE NOT s.case_sensitive AND code_system_id = s.id AND lower(code) = lower($4)
                LIMIT 1
            ) b ON true
        "#,
        )
        .bind(url)
        .bind(version)
        .bind(code_a)
        .bind(code_b)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?)
    }
    async fn compose_snapshot(
        &self,
        url: &str,
        version: Option<&str>,
        id: Option<Uuid>,
        system_context: Option<(&str, &str)>,
    ) -> Result<ExpandedValueSet, AppError> {
        use std::collections::BTreeSet;
        use term_squid_core::{Canonical, Compose, PreparedSystem, Snapshot, MAX_RESOURCES};
        let root = Canonical::parse(url, version)?;
        let cache_key = (
            root.clone(),
            id,
            system_context.map(|(s, v)| (s.to_owned(), v.to_owned())),
        );
        // A fresh autocommit statement is the warm operation's snapshot boundary.
        // No time-based generation cache: a request starting after a CLI commit sees it.
        let observed = self
            .generations
            .current(super::generation::Family::ValueSet)
            .await?;
        if let Some(result) = self.cached_plan(observed, &cache_key).await {
            return Ok(result);
        }
        // Only cold work is serialized. Recheck in a new snapshot after waiting for admission.
        let preparation = self.preparation.clone().lock_owned().await;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let generation: i64 =
            sqlx::query_scalar("SELECT generation FROM terminology_generation WHERE singleton")
                .fetch_one(&mut *tx)
                .await?;
        if let Some(result) = self.cached_plan(generation, &cache_key).await {
            tx.commit().await?;
            return Ok(result);
        }
        let mut snapshot = Snapshot::default();
        let mut raw_systems = std::collections::BTreeMap::new();
        let mut bytes_remaining: i64 = 32 * 1024 * 1024;
        let mut pending = vec![root.clone()];
        let mut seen = BTreeSet::new();
        let mut base_resource = None;
        while let Some(key) = pending.pop() {
            if !seen.insert(key.clone()) {
                continue;
            }
            if seen.len() + raw_systems.len() > MAX_RESOURCES {
                return Err(AppError::BadRequest(
                    "Compose resource limit exceeded".into(),
                ));
            }
            let row: Option<(Option<sqlx::types::Json<serde_json::Value>>, i64)> = sqlx::query_as(
                "SELECT CASE WHEN octet_length(content::text) <= $4 THEN content END, octet_length(content::text)::bigint FROM value_sets WHERE ($3::uuid IS NOT NULL AND id = $3) OR ($3::uuid IS NULL AND url = $1 AND ($2::text IS NULL OR version = $2)) ORDER BY updated_at DESC, id LIMIT 1")
                .bind(&key.url).bind(&key.version).bind(if key == root { id } else { None }).bind(bytes_remaining).fetch_optional(&mut *tx).await?;
            let row = row.ok_or_else(|| {
                AppError::NotFound(format!(
                    "ValueSet {} version {:?} not found",
                    key.url, key.version
                ))
            })?;
            bytes_remaining -= row.1;
            let resource = row
                .0
                .ok_or_else(|| AppError::BadRequest("Compose input exceeds 32 MiB".into()))?
                .0;
            if key == root {
                base_resource = Some(resource.clone());
            }
            let compose: Compose =
                serde_json::from_value(resource.get("compose").cloned().ok_or_else(|| {
                    AppError::BadRequest("ValueSet without compose is not supported".into())
                })?)
                .map_err(|e| AppError::BadRequest(format!("Invalid compose: {e}")))?;
            let (systems, imports) = compose.dependencies()?;
            pending.extend(imports);
            for system in systems {
                if raw_systems.contains_key(&system) {
                    continue;
                }
                if seen.len() + raw_systems.len() >= MAX_RESOURCES {
                    return Err(AppError::BadRequest(
                        "Compose resource limit exceeded".into(),
                    ));
                }
                let row: Option<(Option<sqlx::types::Json<serde_json::Value>>, i64)> = sqlx::query_as(
                    "SELECT CASE WHEN octet_length(content::text) <= $3 THEN content END, octet_length(content::text)::bigint FROM code_systems WHERE url = $1 AND ($2::text IS NULL OR version = $2) ORDER BY updated_at DESC, id LIMIT 1")
                    .bind(&system.url).bind(system.version.as_deref().or_else(|| system_context.filter(|(url, _)| *url == system.url).map(|(_, version)| version))).bind(bytes_remaining).fetch_optional(&mut *tx).await?;
                let row = row.ok_or_else(|| {
                    AppError::NotFound(format!(
                        "CodeSystem {} version {:?} not found",
                        system.url, system.version
                    ))
                })?;
                bytes_remaining -= row.1;
                let resource = row
                    .0
                    .ok_or_else(|| AppError::BadRequest("Compose input exceeds 32 MiB".into()))?
                    .0;
                raw_systems.insert(system, resource);
            }
            snapshot.value_sets.insert(key, compose);
        }
        tx.commit().await?;
        let (_preparation, result) = tokio::task::spawn_blocking(move || -> Result<_, AppError> {
            let mut concept_count = 0;
            for (key, resource) in raw_systems {
                let definition = serde_json::from_value(resource)
                    .map_err(|e| AppError::BadRequest(format!("Invalid CodeSystem: {e}")))?;
                let prepared = PreparedSystem::prepare(definition)?;
                concept_count += prepared.concepts.len();
                if concept_count > term_squid_core::MAX_CONCEPTS {
                    return Err(AppError::BadRequest(
                        "Snapshot concept limit exceeded".into(),
                    ));
                }
                snapshot.systems.insert(key, prepared);
            }
            let result = ExpandedValueSet {
                resource: std::sync::Arc::new(base_resource.expect("Root is resolved")),
                members: std::sync::Arc::new(term_squid_core::PreparedValueSet::prepare(
                    &snapshot, &root,
                )?),
            };
            // Keep preparation single-flight even if the awaiting HTTP request is cancelled.
            Ok((preparation, result))
        })
        .await
        .map_err(|e| AppError::Internal(e.into()))??;
        let key_bytes = cache_key.0.url.len()
            + cache_key.0.version.as_ref().map_or(0, String::len)
            + cache_key.2.as_ref().map_or(0, |(s, v)| s.len() + v.len());
        let charge = key_bytes * 8
            + result.members.memory_charge()
            + serde_json::to_vec(result.resource.as_ref())
                .map_err(|e| AppError::Internal(e.into()))?
                .len()
                * 64
            + 4096;
        self.prepared
            .lock()
            .await
            .publish(generation, cache_key, result.clone(), charge);
        Ok(result)
    }
}

#[async_trait]
impl TerminologyStore for PostgresStore {
    async fn prepare_concept_map(
        &self,
        url: &str,
        version: Option<&str>,
        id: Option<Uuid>,
    ) -> Result<std::sync::Arc<term_squid_core::PreparedConceptMap>, AppError> {
        self.map_snapshot(url, version, id).await
    }
    async fn resolve_concept(
        &self,
        url: &str,
        version: Option<&str>,
        code: &str,
    ) -> Result<Option<ResolvedConcept>, AppError> {
        let canonical = term_squid_core::Canonical::parse(url, version)?;
        self.prepared_concept(canonical, None, code).await
    }
    async fn resolve_concept_by_id(
        &self,
        id: &Uuid,
        code: &str,
    ) -> Result<Option<ResolvedConcept>, AppError> {
        self.prepared_concept(
            term_squid_core::Canonical::parse("urn:term-squid:instance", None)?,
            Some(*id),
            code,
        )
        .await
    }

    async fn expand_compose(
        &self,
        url: &str,
        version: Option<&str>,
    ) -> Result<ExpandedValueSet, AppError> {
        self.compose_snapshot(url, version, None, None).await
    }
    async fn expand_compose_by_id(&self, id: &Uuid) -> Result<ExpandedValueSet, AppError> {
        self.compose_snapshot("urn:term-squid:instance", None, Some(*id), None)
            .await
    }

    async fn compose_for_validation(
        &self,
        url: &str,
        version: Option<&str>,
        id: Option<Uuid>,
        system: &str,
        system_version: Option<&str>,
    ) -> Result<ExpandedValueSet, AppError> {
        self.compose_snapshot(url, version, id, system_version.map(|v| (system, v)))
            .await
    }

    // ========== CodeSystem operations ==========

    async fn create_code_system(&self, cs: CodeSystem) -> Result<CodeSystem, AppError> {
        let result = sqlx::query_as::<_, CodeSystem>(
            r#"
            INSERT INTO code_systems (url, version, status, name, title, content, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, NOW())
            RETURNING *
            "#,
        )
        .bind(&cs.url)
        .bind(&cs.version)
        .bind(&cs.status)
        .bind(&cs.name)
        .bind(&cs.title)
        .bind(&cs.content)
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    async fn get_code_system(
        &self,
        url: &str,
        version: Option<&str>,
    ) -> Result<Option<CodeSystem>, AppError> {
        let canonical = term_squid_core::Canonical::parse(url, version)?;
        let url = canonical.url.as_str();
        let version = canonical.version.as_deref();
        let result = match version {
            Some(v) => {
                sqlx::query_as::<_, CodeSystem>(
                    "SELECT * FROM code_systems WHERE url = $1 AND version = $2",
                )
                .bind(url)
                .bind(v)
                .fetch_optional(&self.pool)
                .await?
            }
            None => {
                // Get the most recent version if no version specified
                sqlx::query_as::<_, CodeSystem>(
                    "SELECT * FROM code_systems WHERE url = $1 ORDER BY updated_at DESC LIMIT 1",
                )
                .bind(url)
                .fetch_optional(&self.pool)
                .await?
            }
        };

        Ok(result)
    }

    async fn get_code_system_by_id(&self, id: &Uuid) -> Result<Option<CodeSystem>, AppError> {
        let result = sqlx::query_as::<_, CodeSystem>("SELECT * FROM code_systems WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result)
    }

    async fn update_code_system(&self, cs: CodeSystem) -> Result<CodeSystem, AppError> {
        let result = sqlx::query_as::<_, CodeSystem>(
            r#"
            UPDATE code_systems
            SET status = $1, name = $2, title = $3, content = $4, updated_at = NOW()
            WHERE id = $5
            RETURNING *
            "#,
        )
        .bind(&cs.status)
        .bind(&cs.name)
        .bind(&cs.title)
        .bind(&cs.content)
        .bind(cs.id)
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    async fn delete_code_system(&self, url: &str, version: Option<&str>) -> Result<(), AppError> {
        match version {
            Some(v) => {
                sqlx::query("DELETE FROM code_systems WHERE url = $1 AND version = $2")
                    .bind(url)
                    .bind(v)
                    .execute(&self.pool)
                    .await?;
            }
            None => {
                sqlx::query("DELETE FROM code_systems WHERE url = $1")
                    .bind(url)
                    .execute(&self.pool)
                    .await?;
            }
        }

        Ok(())
    }

    async fn search_code_systems(
        &self,
        params: &SearchParams,
    ) -> Result<Vec<CodeSystem>, AppError> {
        let mut query_str = "SELECT * FROM code_systems WHERE 1=1".to_string();
        let mut param_count = 0;

        if params.url.is_some() {
            param_count += 1;
            query_str.push_str(&format!(" AND url = ${param_count}"));
        }
        if params.status.is_some() {
            param_count += 1;
            query_str.push_str(&format!(" AND status = ${param_count}"));
        }
        if params.name.is_some() {
            param_count += 1;
            query_str.push_str(&format!(" AND name ILIKE ${param_count}"));
        }
        if params.fhir_version.is_some() {
            param_count += 1;
            query_str.push_str(&format!(" AND fhir_version = ${param_count}"));
        }

        query_str.push_str(" ORDER BY updated_at DESC");

        if let Some(limit) = params.limit {
            query_str.push_str(&format!(" LIMIT {limit}"));
        }
        if let Some(offset) = params.offset {
            query_str.push_str(&format!(" OFFSET {offset}"));
        }

        // Build the query dynamically
        let mut query = sqlx::query_as::<_, CodeSystem>(&query_str);

        if let Some(ref url) = params.url {
            query = query.bind(url);
        }
        if let Some(ref status) = params.status {
            query = query.bind(status);
        }
        if let Some(ref name) = params.name {
            query = query.bind(format!("%{name}%"));
        }
        if let Some(ref fhir_version) = params.fhir_version {
            query = query.bind(fhir_version);
        }

        let results = query.fetch_all(&self.pool).await?;

        Ok(results)
    }

    // ========== ValueSet operations ==========

    async fn create_value_set(&self, vs: ValueSet) -> Result<ValueSet, AppError> {
        let result = sqlx::query_as::<_, ValueSet>(
            r#"
            INSERT INTO value_sets (url, version, status, name, title, content, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, NOW())
            RETURNING *
            "#,
        )
        .bind(&vs.url)
        .bind(&vs.version)
        .bind(&vs.status)
        .bind(&vs.name)
        .bind(&vs.title)
        .bind(&vs.content)
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    async fn get_value_set(
        &self,
        url: &str,
        version: Option<&str>,
    ) -> Result<Option<ValueSet>, AppError> {
        let canonical = term_squid_core::Canonical::parse(url, version)?;
        let url = canonical.url.as_str();
        let version = canonical.version.as_deref();
        let result = match version {
            Some(v) => {
                sqlx::query_as::<_, ValueSet>(
                    "SELECT * FROM value_sets WHERE url = $1 AND version = $2",
                )
                .bind(url)
                .bind(v)
                .fetch_optional(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, ValueSet>(
                    "SELECT * FROM value_sets WHERE url = $1 ORDER BY updated_at DESC LIMIT 1",
                )
                .bind(url)
                .fetch_optional(&self.pool)
                .await?
            }
        };

        Ok(result)
    }

    async fn get_value_set_by_id(&self, id: &Uuid) -> Result<Option<ValueSet>, AppError> {
        let result = sqlx::query_as::<_, ValueSet>("SELECT * FROM value_sets WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result)
    }

    async fn update_value_set(&self, vs: ValueSet) -> Result<ValueSet, AppError> {
        let result = sqlx::query_as::<_, ValueSet>(
            r#"
            UPDATE value_sets
            SET status = $1, name = $2, title = $3, content = $4, updated_at = NOW()
            WHERE id = $5
            RETURNING *
            "#,
        )
        .bind(&vs.status)
        .bind(&vs.name)
        .bind(&vs.title)
        .bind(&vs.content)
        .bind(vs.id)
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    async fn delete_value_set(&self, url: &str, version: Option<&str>) -> Result<(), AppError> {
        match version {
            Some(v) => {
                sqlx::query("DELETE FROM value_sets WHERE url = $1 AND version = $2")
                    .bind(url)
                    .bind(v)
                    .execute(&self.pool)
                    .await?;
            }
            None => {
                sqlx::query("DELETE FROM value_sets WHERE url = $1")
                    .bind(url)
                    .execute(&self.pool)
                    .await?;
            }
        }

        Ok(())
    }

    async fn search_value_sets(&self, params: &SearchParams) -> Result<Vec<ValueSet>, AppError> {
        let mut query_str = "SELECT * FROM value_sets WHERE 1=1".to_string();
        let mut param_count = 0;

        if params.url.is_some() {
            param_count += 1;
            query_str.push_str(&format!(" AND url = ${param_count}"));
        }
        if params.name.is_some() {
            param_count += 1;
            query_str.push_str(&format!(" AND name ILIKE ${param_count}"));
        }
        if params.status.is_some() {
            param_count += 1;
            query_str.push_str(&format!(" AND status = ${param_count}"));
        }
        if params.fhir_version.is_some() {
            param_count += 1;
            query_str.push_str(&format!(" AND fhir_version = ${param_count}"));
        }

        query_str.push_str(" ORDER BY updated_at DESC");

        if let Some(limit) = params.limit {
            query_str.push_str(&format!(" LIMIT {limit}"));
        }
        if let Some(offset) = params.offset {
            query_str.push_str(&format!(" OFFSET {offset}"));
        }

        let mut query = sqlx::query_as::<_, ValueSet>(&query_str);

        if let Some(ref url) = params.url {
            query = query.bind(url);
        }
        if let Some(ref name) = params.name {
            query = query.bind(format!("%{name}%"));
        }
        if let Some(ref status) = params.status {
            query = query.bind(status);
        }
        if let Some(ref fhir_version) = params.fhir_version {
            query = query.bind(fhir_version);
        }

        let results = query.fetch_all(&self.pool).await?;

        Ok(results)
    }

    // ========== ConceptMap operations ==========

    async fn create_concept_map(&self, cm: ConceptMap) -> Result<ConceptMap, AppError> {
        let result = sqlx::query_as::<_, ConceptMap>(
            r#"
            INSERT INTO concept_maps (url, version, status, name, title, source_uri, target_uri, content, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
            RETURNING *
            "#
        )
        .bind(&cm.url)
        .bind(&cm.version)
        .bind(&cm.status)
        .bind(&cm.name)
        .bind(&cm.title)
        .bind(&cm.source_uri)
        .bind(&cm.target_uri)
        .bind(&cm.content)
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    async fn get_concept_map(
        &self,
        url: &str,
        version: Option<&str>,
    ) -> Result<Option<ConceptMap>, AppError> {
        let canonical = term_squid_core::Canonical::parse(url, version)?;
        let url = canonical.url.as_str();
        let version = canonical.version.as_deref();
        let result =
            match version {
                Some(v) => {
                    sqlx::query_as::<_, ConceptMap>(
                        "SELECT * FROM concept_maps WHERE url = $1 AND version = $2",
                    )
                    .bind(url)
                    .bind(v)
                    .fetch_optional(&self.pool)
                    .await?
                }
                None => sqlx::query_as::<_, ConceptMap>(
                    "SELECT * FROM concept_maps WHERE url = $1 ORDER BY updated_at DESC LIMIT 1",
                )
                .bind(url)
                .fetch_optional(&self.pool)
                .await?,
            };

        Ok(result)
    }

    async fn get_concept_map_by_id(&self, id: &Uuid) -> Result<Option<ConceptMap>, AppError> {
        let result = sqlx::query_as::<_, ConceptMap>("SELECT * FROM concept_maps WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result)
    }

    async fn update_concept_map(&self, cm: ConceptMap) -> Result<ConceptMap, AppError> {
        let result = sqlx::query_as::<_, ConceptMap>(
            r#"
            UPDATE concept_maps
            SET status = $1, name = $2, title = $3, source_uri = $4, target_uri = $5, content = $6, updated_at = NOW()
            WHERE id = $7
            RETURNING *
            "#
        )
        .bind(&cm.status)
        .bind(&cm.name)
        .bind(&cm.title)
        .bind(&cm.source_uri)
        .bind(&cm.target_uri)
        .bind(&cm.content)
        .bind(cm.id)
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    async fn delete_concept_map(&self, url: &str, version: Option<&str>) -> Result<(), AppError> {
        match version {
            Some(v) => {
                sqlx::query("DELETE FROM concept_maps WHERE url = $1 AND version = $2")
                    .bind(url)
                    .bind(v)
                    .execute(&self.pool)
                    .await?;
            }
            None => {
                sqlx::query("DELETE FROM concept_maps WHERE url = $1")
                    .bind(url)
                    .execute(&self.pool)
                    .await?;
            }
        }

        Ok(())
    }

    async fn search_concept_maps(
        &self,
        params: &SearchParams,
    ) -> Result<Vec<ConceptMap>, AppError> {
        let mut query_str = "SELECT * FROM concept_maps WHERE 1=1".to_string();
        let mut param_count = 0;

        if params.url.is_some() {
            param_count += 1;
            query_str.push_str(&format!(" AND url = ${param_count}"));
        }
        if params.status.is_some() {
            param_count += 1;
            query_str.push_str(&format!(" AND status = ${param_count}"));
        }
        if params.fhir_version.is_some() {
            param_count += 1;
            query_str.push_str(&format!(" AND fhir_version = ${param_count}"));
        }

        query_str.push_str(" ORDER BY updated_at DESC");

        if let Some(limit) = params.limit {
            query_str.push_str(&format!(" LIMIT {limit}"));
        }
        if let Some(offset) = params.offset {
            query_str.push_str(&format!(" OFFSET {offset}"));
        }

        let mut query = sqlx::query_as::<_, ConceptMap>(&query_str);

        if let Some(ref url) = params.url {
            query = query.bind(url);
        }
        if let Some(ref status) = params.status {
            query = query.bind(status);
        }
        if let Some(ref fhir_version) = params.fhir_version {
            query = query.bind(fhir_version);
        }

        let results = query.fetch_all(&self.pool).await?;

        Ok(results)
    }

    // ========== Statistics ==========

    async fn count_code_systems(&self) -> Result<i64, AppError> {
        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM code_systems")
            .fetch_one(&self.pool)
            .await?;

        Ok(result.0)
    }

    async fn count_value_sets(&self) -> Result<i64, AppError> {
        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM value_sets")
            .fetch_one(&self.pool)
            .await?;

        Ok(result.0)
    }

    async fn count_concept_maps(&self) -> Result<i64, AppError> {
        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM concept_maps")
            .fetch_one(&self.pool)
            .await?;

        Ok(result.0)
    }

    async fn get_concept(
        &self,
        code_system_id: &uuid::Uuid,
        code: &str,
    ) -> Result<Option<Concept>, AppError> {
        let concept = sqlx::query_as::<_, Concept>(
            "SELECT * FROM concepts WHERE code_system_id = $1 AND code = $2",
        )
        .bind(code_system_id)
        .bind(code)
        .fetch_optional(&self.pool)
        .await?;

        Ok(concept)
    }

    async fn resolve_subsumption(
        &self,
        url: &str,
        version: Option<&str>,
        code_a: &str,
        code_b: &str,
    ) -> Result<Option<ResolvedSubsumption>, AppError> {
        let canonical = term_squid_core::Canonical::parse(url, version)?;
        self.prepared_subsumption(canonical, None, code_a, code_b)
            .await
    }

    async fn resolve_subsumption_by_id(
        &self,
        code_system_id: &uuid::Uuid,
        code_a: &str,
        code_b: &str,
    ) -> Result<Option<ResolvedSubsumption>, AppError> {
        self.prepared_subsumption(
            term_squid_core::Canonical::parse("urn:term-squid:instance", None)?,
            Some(*code_system_id),
            code_a,
            code_b,
        )
        .await
    }

    async fn get_value_set_expansion(
        &self,
        value_set_id: &uuid::Uuid,
    ) -> Result<Option<Vec<serde_json::Value>>, AppError> {
        let expansion = sqlx::query_as::<_, (sqlx::types::Json<serde_json::Value>,)>(
            "SELECT expansion_data FROM value_set_expansions WHERE value_set_id = $1 ORDER BY created_at DESC, id LIMIT 1",
        )
        .bind(value_set_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(expansion.map(|(json,)| {
            // Return the expansion as an array of contains elements
            if let Some(contains) = json.0.get("contains").and_then(|c| c.as_array()) {
                contains.clone()
            } else {
                vec![]
            }
        }))
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    #[tokio::test]
    async fn late_generation_observation_and_publication_cannot_replace_newer_plan() {
        use term_squid_core::{Canonical, Compose, PreparedSystem, PreparedValueSet, Snapshot};
        let root = Canonical::parse("urn:set", None).unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.systems.insert(Canonical::parse("urn:codes", None).unwrap(), PreparedSystem::prepare(
            serde_json::from_value(serde_json::json!({"url":"urn:codes","content":"complete","concept":[{"code":"new"}]})).unwrap()).unwrap());
        snapshot.value_sets.insert(
            root.clone(),
            serde_json::from_value::<Compose>(
                serde_json::json!({"include":[{"system":"urn:codes"}]}),
            )
            .unwrap(),
        );
        let plan = ExpandedValueSet {
            resource: std::sync::Arc::new(serde_json::json!({"generation":2})),
            members: std::sync::Arc::new(PreparedValueSet::prepare(&snapshot, &root).unwrap()),
        };
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgresql://unused:unused@127.0.0.1/unused")
            .unwrap();
        let store = PostgresStore::new(pool);
        let key = (root, None, None);
        store
            .prepared
            .lock()
            .await
            .publish(2, key.clone(), plan.clone(), 4096);
        // Model a generation-1 SELECT finishing after a generation-2 reader has published.
        assert!(store.cached_plan(1, &key).await.is_none());
        // Model generation-1 preparation finishing after the same publication.
        store
            .prepared
            .lock()
            .await
            .publish(1, key.clone(), plan.clone(), 1024);
        let retained = store.cached_plan(2, &key).await.unwrap();
        assert!(std::sync::Arc::ptr_eq(&retained.members, &plan.members));
        assert_eq!(store.prepared.lock().await.bytes, 4096);
    }
}
