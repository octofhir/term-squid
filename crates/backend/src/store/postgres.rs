use crate::error::AppError;
use crate::models::{
    CodeSystem, Concept, ConceptMap, ExpandedValueSet, ResolvedConcept, SearchParams, ValueSet,
};
use crate::store::TerminologyStore;
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

/// PostgreSQL implementation of TerminologyStore
pub struct PostgresStore {
    pool: PgPool,
    compose_slots: std::sync::Arc<tokio::sync::Semaphore>,
}

impl PostgresStore {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            compose_slots: std::sync::Arc::new(tokio::sync::Semaphore::new(2)),
        }
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
            SELECT s.id, s.hierarchy_meaning, s.name, s.version, s.content_mode, c.code, c.display, c.definition, c.properties
            FROM (SELECT id, hierarchy_meaning, name, version, content_mode, case_sensitive FROM code_systems
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
    async fn compose_snapshot(
        &self,
        url: &str,
        version: Option<&str>,
        id: Option<Uuid>,
        system_context: Option<(&str, &str)>,
    ) -> Result<ExpandedValueSet, AppError> {
        use std::collections::BTreeSet;
        use term_squid_core::{Canonical, Compose, PreparedSystem, Snapshot, MAX_RESOURCES};
        let slot = self
            .compose_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let root = Canonical::parse(url, version)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
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
        tokio::task::spawn_blocking(move || -> Result<ExpandedValueSet, AppError> {
            let _slot = slot;
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
            Ok(ExpandedValueSet {
                resource: base_resource.expect("Root is resolved"),
                members: snapshot.expand(&root)?,
            })
        })
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    }
}

#[async_trait]
impl TerminologyStore for PostgresStore {
    async fn resolve_concept(
        &self,
        url: &str,
        version: Option<&str>,
        code: &str,
    ) -> Result<Option<ResolvedConcept>, AppError> {
        let canonical = term_squid_core::Canonical::parse(url, version)?;
        self.resolve_row(&canonical.url, canonical.version.as_deref(), code, None)
            .await
    }
    async fn resolve_concept_by_id(
        &self,
        id: &Uuid,
        code: &str,
    ) -> Result<Option<ResolvedConcept>, AppError> {
        self.resolve_row("", None, code, Some(*id)).await
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

    async fn check_subsumption(
        &self,
        code_system_id: &uuid::Uuid,
        code_a: &str,
        code_b: &str,
    ) -> Result<Option<bool>, AppError> {
        // Check if code_a subsumes code_b (A is ancestor of B)
        let subsumes: Option<(bool,)> = sqlx::query_as(
            "SELECT TRUE FROM closure_table
             WHERE code_system_id = $1 AND ancestor_code = $2 AND descendant_code = $3",
        )
        .bind(code_system_id)
        .bind(code_a)
        .bind(code_b)
        .fetch_optional(&self.pool)
        .await?;

        if subsumes.is_some() {
            return Ok(Some(true));
        }

        // Check if code_b subsumes code_a (B is ancestor of A)
        let subsumed_by: Option<(bool,)> = sqlx::query_as(
            "SELECT TRUE FROM closure_table
             WHERE code_system_id = $1 AND ancestor_code = $2 AND descendant_code = $3",
        )
        .bind(code_system_id)
        .bind(code_b)
        .bind(code_a)
        .fetch_optional(&self.pool)
        .await?;

        if subsumed_by.is_some() {
            return Ok(Some(false));
        }

        // No relationship
        Ok(None)
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
