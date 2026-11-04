//! Projection-backed native sets. Rust retains definitions and one response page only.
//! Every dependency, count and page is read inside one fresh repeatable-read snapshot.
use super::{
    inline::InlineValueSet, ValueSetOperation, ValueSetPage, ValueSetRequest, ValueSetResult,
};
use crate::error::AppError;
use serde_json::Value;
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use term_squid_core::{
    loinc::{self, PrimaryFilter},
    snomed::{self, NativeFilter},
    Canonical, Compose, Member, Selection,
};
use uuid::Uuid;

const INPUT_BYTES: i64 = 4 * 1024 * 1024;
pub(super) const PAGE_BYTES: i64 = 32 * 1024 * 1024;
const MAX_TERMS: usize = 4096;
const MAX_SELECTIONS: usize = 128;
const MAX_HIERARCHY_FILTERS: usize = 16;
const EXPANSION_WALK: i64 = 1_000_000;
pub(super) const MEMBERSHIP_WALK: i64 = 8192;
const INACTIVE: &str = "COALESCE(c.properties,'[]'::jsonb) @> '[{\"code\":\"inactive\",\"valueBoolean\":true}]'::jsonb";
type Tx<'a> = Transaction<'a, Postgres>;
type Sql = QueryBuilder<'static, Postgres>;
pub(super) fn unsupported(message: impl Into<String>) -> AppError {
    AppError::Terminology(term_squid_core::Error(message.into()))
}
#[derive(Clone)]
struct SelectionPlan {
    system: Option<Uuid>,
    system_url: Option<String>,
    versioned: bool,
    whole_unversioned: bool,
    concepts: Vec<term_squid_core::ExplicitConcept>,
    filters: Vec<NativeFilter>,
    primary_filters: Vec<PrimaryFilter>,
    imports: Vec<Canonical>,
}
struct Node {
    compose: Compose,
    include: Vec<SelectionPlan>,
    exclude: Vec<SelectionPlan>,
}
struct Compiler {
    sql: Sql,
    serial: usize,
    guards: Vec<(String, i64)>,
    membership: Option<String>,
}
impl Compiler {
    fn name(&mut self) -> String {
        self.serial += 1;
        format!("p{}", self.serial)
    }
    fn hierarchy(&mut self, system: Uuid, start: &str, ancestors: bool) -> String {
        let name = self.name();
        let (source, destination) = if ancestors {
            ("source_code", "destination_code")
        } else {
            ("destination_code", "source_code")
        };
        let limit = if self.membership.is_some() {
            MEMBERSHIP_WALK
        } else {
            EXPANSION_WALK
        };
        // Parameterized edge probes keep deep chains from rescanning the entire RF2 table.
        self.sql.push(format!("{name}_walk(code) AS (SELECT ")).push_bind(start.to_owned())
            .push(format!("::text UNION SELECT r.{destination}::text FROM {name}_walk w CROSS JOIN LATERAL (SELECT {destination} FROM snomed_relationships r WHERE r.{source}=w.code AND r.code_system_id="))
            .push_bind(system).push(" AND r.active AND r.type_id='116680003' AND r.characteristic_type_id='900000000000011006' OFFSET 0) r), ")
            .push(format!("{name} AS MATERIALIZED (SELECT code FROM {name}_walk LIMIT {}), {name}_size AS MATERIALIZED (SELECT count(*)::bigint AS n FROM {name}), ",limit+1));
        self.guards.push((name.clone(), limit));
        name
    }
    fn selection(
        &mut self,
        selection: &SelectionPlan,
        names: &BTreeMap<Canonical, String>,
    ) -> String {
        // Walk up from a single requested code for membership; never enumerate its descendants.
        let mut hierarchies = Vec::new();
        for filter in &selection.filters {
            if let NativeFilter::Hierarchy {
                anchor, ancestors, ..
            } = filter
            {
                let start = if !ancestors {
                    self.membership.as_deref().unwrap_or(anchor)
                } else {
                    anchor
                }
                .to_owned();
                let walk = self.hierarchy(
                    selection.system.unwrap(),
                    &start,
                    *ancestors || self.membership.is_some(),
                );
                hierarchies.push(walk);
            }
        }
        let name = self.name();
        let driver = if self.membership.is_none() && selection.concepts.is_empty() {
            selection
                .filters
                .iter()
                .filter(|f| matches!(f, NativeFilter::Hierarchy { .. }))
                .zip(&hierarchies)
                .find(|(f, _)| {
                    matches!(
                        f,
                        NativeFilter::Hierarchy {
                            ancestors: false,
                            negate: false,
                            ..
                        }
                    )
                })
                .map(|(_, name)| name.as_str())
        } else {
            None
        };
        self.sql.push(format!("{name} AS ("));
        self.selection_rows(selection, names, &hierarchies, false, driver);
        if driver.is_some() {
            self.sql.push(" UNION ALL ");
            self.selection_rows(selection, names, &hierarchies, true, driver);
        }
        self.sql.push("), ");
        name
    }
    fn selection_rows(
        &mut self,
        selection: &SelectionPlan,
        names: &BTreeMap<Canonical, String>,
        hierarchies: &[String],
        hash: bool,
        driver: Option<&str>,
    ) {
        self.sql.push("SELECT c.code_system_id,c.code,");
        // Resolve duplicates before emitting CASE: a later absent display restores the projection.
        let overrides: BTreeMap<_, _> = selection
            .concepts
            .iter()
            .map(|c| (&c.code, &c.display))
            .collect();
        if overrides.values().any(|display| display.is_some()) {
            self.sql.push("CASE c.code ");
            for (code, display) in overrides {
                if let Some(display) = display {
                    self.sql
                        .push(" WHEN ")
                        .push_bind(code.clone())
                        .push(" THEN ")
                        .push_bind(display.clone());
                }
            }
            self.sql.push(" ELSE c.display END");
        } else {
            self.sql.push("c.display");
        }
        self.sql.push(" AS display,");
        if selection.system.is_some() {
            self.sql.push(format!("({INACTIVE}) AS inactive FROM "));
        } else {
            self.sql.push("c.inactive FROM ");
        }
        if let Some(system) = selection.system {
            self.sql
                .push("concepts c WHERE c.code_system_id=")
                .push_bind(system);
        } else {
            let first = &names[&selection.imports[0]];
            self.sql.push(format!("{first} c WHERE true"));
        }
        if let Some(code) = &self.membership {
            self.sql.push(" AND c.code=").push_bind(code.clone());
        }
        if !selection.concepts.is_empty() {
            self.sql
                .push(" AND c.code=ANY(")
                .push_bind(
                    selection
                        .concepts
                        .iter()
                        .map(|c| c.code.clone())
                        .collect::<Vec<_>>(),
                )
                .push(")");
        }
        if let Some(driver) = driver {
            let comparison = if hash { ">256" } else { "<=256" };
            // Only one branch executes. Use indexed probes for small sets and a hashed
            // predicate for broad sets, based on cardinality in this same snapshot.
            self.sql
                .push(format!(" AND (SELECT n FROM {driver}_size){comparison}"));
        }
        let mut hierarchies = hierarchies.iter();
        for filter in &selection.filters {
            self.sql.push(" AND (");
            match filter {
                NativeFilter::Hierarchy {
                    anchor,
                    ancestors,
                    strict,
                    negate,
                } => {
                    let walk = hierarchies.next().unwrap();
                    if *negate {
                        self.sql.push("NOT (");
                    }
                    if self.membership.is_some() && !ancestors {
                        self.sql
                            .push(format!("EXISTS(SELECT 1 FROM {walk} WHERE code="))
                            .push_bind(anchor.clone())
                            .push(")");
                    } else {
                        if hash {
                            self.sql
                                .push(format!("(c.code IN (SELECT code FROM {walk})) IS TRUE"));
                        } else {
                            self.sql
                                .push(format!("c.code IN (SELECT code FROM {walk})"));
                        }
                    }
                    if *strict {
                        self.sql.push(" AND c.code<>").push_bind(anchor.clone());
                    }
                    if *negate {
                        self.sql.push(")");
                    }
                }
                NativeFilter::Code(code) => {
                    self.sql.push("c.code=").push_bind(code.clone());
                }
                NativeFilter::Property { code, value }
                    if code.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    self.sql.push("EXISTS(SELECT 1 FROM snomed_relationships r WHERE r.code_system_id=c.code_system_id AND r.source_code=c.code AND r.active AND r.characteristic_type_id='900000000000011006' AND r.type_id=")
                        .push_bind(code.clone()).push(" AND r.destination_code=").push_bind(value.clone()).push(")");
                }
                NativeFilter::Property { code, value } => {
                    if code == "inactive" {
                        self.sql
                            .push(format!("({INACTIVE}) = "))
                            .push_bind(value == "true");
                    } else {
                        self.sql.push("EXISTS(SELECT 1 FROM jsonb_array_elements(COALESCE(c.properties,'[]'::jsonb)) prop WHERE prop->>'code'=")
                            .push_bind(code.clone()).push(" AND COALESCE(prop->>'valueCode',prop->>'valueString')=").push_bind(value.clone()).push(")");
                    }
                }
            }
            self.sql.push(")");
        }
        for filter in &selection.primary_filters {
            self.sql.push(" AND (");
            match filter {
                PrimaryFilter::Code(code) => {
                    self.sql.push("c.code=").push_bind(code.clone());
                }
                PrimaryFilter::Equality { property, value } => {
                    let typed = if property == "inactive" {
                        serde_json::json!([{"code":property,"valueBoolean":value=="true"}])
                    } else {
                        serde_json::json!([{"code":property,"valueString":value}])
                    };
                    self.sql
                        .push("COALESCE(c.properties,'[]'::jsonb) @> ")
                        .push_bind(sqlx::types::Json(typed));
                }
                PrimaryFilter::Exists { property, present } => {
                    if !present {
                        self.sql.push("NOT ");
                    }
                    self.sql.push("EXISTS(SELECT 1 FROM jsonb_array_elements(COALESCE(c.properties,'[]'::jsonb)) prop WHERE prop->>'code'=")
                        .push_bind(property.clone()).push(")");
                }
            }
            self.sql.push(")");
        }
        for imported in &selection.imports {
            self.sql.push(format!(" AND EXISTS(SELECT 1 FROM {} i WHERE i.code_system_id=c.code_system_id AND i.code=c.code)",names[imported]));
        }
    }
    fn node(&mut self, node: &Node, names: &BTreeMap<Canonical, String>) -> String {
        let includes: Vec<_> = node
            .include
            .iter()
            .map(|s| self.selection(s, names))
            .collect();
        let excludes: Vec<_> = node
            .exclude
            .iter()
            .map(|s| self.selection(s, names))
            .collect();
        if includes.len() == 1 && excludes.is_empty() && node.compose.inactive != Some(false) {
            return includes[0].clone();
        }
        let name = self.name();
        self.sql.push(format!("{name} AS (SELECT DISTINCT ON(c.code_system_id,c.code) c.code_system_id,c.code,c.display,c.inactive FROM ("));
        for (index, include) in includes.iter().enumerate() {
            if index != 0 {
                self.sql.push(" UNION ALL ");
            }
            self.sql
                .push(format!("SELECT *,{index} AS ordinal FROM {include}"));
        }
        self.sql.push(") c WHERE true");
        for (selection, exclude) in node.exclude.iter().zip(excludes) {
            if selection.whole_unversioned {
                self.sql.push(" AND NOT EXISTS(SELECT 1 FROM code_systems cs WHERE cs.id=c.code_system_id AND cs.url=").push_bind(selection.system_url.clone().unwrap()).push(")");
                continue;
            }
            self.sql.push(format!(" AND NOT EXISTS(SELECT 1 FROM {exclude} e JOIN code_systems es ON es.id=e.code_system_id JOIN code_systems cs ON cs.id=c.code_system_id WHERE es.url=cs.url"));
            self.sql.push(" AND e.code=c.code");
            if selection.versioned {
                self.sql.push(" AND e.code_system_id=c.code_system_id");
            }
            self.sql.push(")");
        }
        if node.compose.inactive == Some(false) {
            self.sql.push(" AND NOT c.inactive");
        }
        self.sql
            .push(" ORDER BY c.code_system_id,c.code,c.ordinal DESC), ");
        name
    }
}

pub(super) async fn query(
    pool: &PgPool,
    request: ValueSetRequest<'_>,
    operation: ValueSetOperation<'_>,
) -> Result<ValueSetResult, AppError> {
    let mut tx = pool.begin().await?;
    sqlx::Executor::execute(&mut *tx,sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY; SET LOCAL statement_timeout='10s'; SET LOCAL work_mem='4MB'")).await?;
    let result = query_in_transaction(&mut tx, request, operation).await?;
    tx.commit().await?;
    Ok(result)
}
pub(super) async fn query_in_transaction(
    tx: &mut Tx<'_>,
    request: ValueSetRequest<'_>,
    operation: ValueSetOperation<'_>,
) -> Result<ValueSetResult, AppError> {
    query_options(tx, request, operation, None).await
}
pub(super) async fn query_options(
    tx: &mut Tx<'_>,
    request: ValueSetRequest<'_>,
    operation: ValueSetOperation<'_>,
    options: Option<&super::ExpansionOptions>,
) -> Result<ValueSetResult, AppError> {
    match operation {
        ValueSetOperation::Expand { offset, count, .. } => {
            super::value_set::check_page(offset, count)?
        }
        ValueSetOperation::Find { system, code, .. } if system == snomed::SYSTEM => {
            snomed::reject_expression(code)?
        }
        _ => {}
    }
    let inline = request
        .inline
        .map(|r| InlineValueSet::new(r, request.version))
        .transpose()?;
    let root = inline
        .as_ref()
        .map(|i| i.canonical.clone())
        .map(Ok)
        .unwrap_or_else(|| Canonical::parse(request.url, request.version))?;
    if inline.is_some() && (snomed::implicit(&root)?.is_some() || loinc::implicit(&root)?.is_some())
    {
        return Err(AppError::BadRequest(
            "Inline ValueSets with an implicit terminology URL must use the canonical URL operation"
                .into(),
        ));
    }
    if inline.is_none() && request.id.is_none() && options.is_none() {
        if let (
            Some(definition),
            ValueSetOperation::Find {
                system,
                version,
                code,
            },
        ) = (snomed::implicit_definition(&root)?, operation)
        {
            let result = super::implicit_membership::find(
                tx,
                &root,
                &definition,
                request.system_context,
                system,
                version,
                code,
            )
            .await?;
            return Ok(result);
        }
    }
    let mut remaining = INPUT_BYTES;
    let mut pending = vec![root.clone()];
    let mut nodes = BTreeMap::new();
    let mut resource = Value::Null;
    let mut terms = 0;
    let mut selection_count = 0;
    let mut hierarchy_filters = 0;
    while let Some(key) = pending.pop() {
        if nodes.contains_key(&key) {
            continue;
        }
        if nodes.len() >= term_squid_core::MAX_RESOURCES {
            return Err(unsupported("Native compose resource limit exceeded"));
        }
        let implicit = snomed::implicit(&key)?.or(loinc::implicit(&key)?);
        let implicit_root = implicit.is_some();
        let mut definition = if let Some(i) = inline.as_ref().filter(|_| key == root) {
            super::options::check_resource_date(
                &i.resource,
                options.and_then(|o| o.date.as_deref()),
            )?;
            remaining -= i.bytes as i64;
            i.resource.clone()
        } else if let Some(implicit) = implicit {
            implicit
        } else if let Some(options) = options.filter(|o| o.date.is_some()) {
            super::selection::set(
                tx,
                &key,
                if key == root { request.id } else { None },
                options.date.as_deref(),
                &mut remaining,
            )
            .await?
        } else {
            let row: Option<(Option<sqlx::types::Json<Value>>,i64)> = sqlx::query_as("SELECT CASE WHEN octet_length(content::text)<=$4 THEN content END,octet_length(content::text)::bigint FROM value_sets WHERE ($3::uuid IS NOT NULL AND id=$3) OR ($3::uuid IS NULL AND url=$1 AND ($2::text IS NULL OR version=$2)) ORDER BY updated_at DESC,id LIMIT 1")
                .bind(&key.url).bind(&key.version).bind(if key == root {request.id} else {None}).bind(remaining).fetch_optional(&mut **tx).await?;
            let (content, bytes) =
                row.ok_or_else(|| AppError::NotFound(format!("ValueSet {} not found", key.url)))?;
            remaining -= bytes;
            content
                .ok_or_else(|| unsupported("Native compose input exceeds 4 MiB"))?
                .0
        };
        if remaining < 0 {
            return Err(unsupported("Native compose input exceeds 4 MiB"));
        }
        if key == root {
            resource = definition.clone();
        }
        if let Some(options) = options {
            options.compose(tx, &mut definition).await?;
        }
        let compose: Compose = serde_json::from_value(
            definition
                .get("compose")
                .cloned()
                .ok_or_else(|| unsupported("ValueSet requires compose"))?,
        )
        .map_err(|e| AppError::BadRequest(format!("Invalid compose: {e}")))?;
        let (_, imports) = compose.references()?;
        pending.extend(imports);
        let mut include = Vec::new();
        let mut exclude = Vec::new();
        for (selections, target, including) in [
            (&compose.include, &mut include, true),
            (&compose.exclude, &mut exclude, false),
        ] {
            for selection in selections {
                terms += 1
                    + selection.concept.len()
                    + selection.filter.len()
                    + selection.value_set.len();
                if terms > MAX_TERMS {
                    return Err(unsupported("Native compose exceeds 4096 terms"));
                }
                selection_count += 1;
                let plan = resolve_selection(
                    tx,
                    selection,
                    request.system_context,
                    implicit_root,
                    including && matches!(operation, ValueSetOperation::Expand { .. }),
                    match operation {
                        ValueSetOperation::Find { system, code, .. } => Some((system, code)),
                        _ => None,
                    },
                )
                .await?;
                hierarchy_filters += plan
                    .filters
                    .iter()
                    .filter(|f| matches!(f, NativeFilter::Hierarchy { .. }))
                    .count();
                if selection_count > MAX_SELECTIONS || hierarchy_filters > MAX_HIERARCHY_FILTERS {
                    return Err(unsupported(
                        "Native compose selection/hierarchy work limit exceeded",
                    ));
                }
                target.push(plan);
            }
        }
        nodes.insert(
            key,
            Node {
                compose,
                include,
                exclude,
            },
        );
    }
    let supplement_ids = if let Some(options) = options.filter(|o| o.display_language.is_some()) {
        let bases: Vec<_> = nodes
            .values()
            .flat_map(|n| n.include.iter().chain(&n.exclude))
            .filter_map(|p| p.system)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        super::supplements::ids(tx, &bases, options.date.as_deref()).await?
    } else {
        Vec::new()
    };
    let mut order = Vec::new();
    topological(
        &root,
        &nodes,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut order,
    )?;
    let mut compiler = Compiler {
        sql: Sql::new("WITH RECURSIVE "),
        serial: 0,
        guards: Vec::new(),
        membership: match operation {
            ValueSetOperation::Find { code, .. } => Some(code.into()),
            _ => None,
        },
    };
    let mut names = BTreeMap::new();
    for key in order {
        let name = compiler.node(&nodes[&key], &names);
        names.insert(key, name);
    }
    let root_name = &names[&root];
    compiler
        .sql
        .push("matched AS NOT MATERIALIZED (SELECT s.url AS system,s.version,n.code,");
    push_display(
        &mut compiler.sql,
        options.and_then(|o| o.display_language.as_deref()),
        &supplement_ids,
    );
    compiler.sql.push(format!(" AS display,n.inactive,s.case_sensitive FROM {root_name} n JOIN code_systems s ON s.id=n.code_system_id WHERE true"));
    if let Some(options) = options {
        if options.active_only == Some(true) {
            compiler.sql.push(" AND NOT n.inactive");
        }
        for system in &options.exclude_system {
            let canonical = Canonical::parse(system, None)?;
            compiler
                .sql
                .push(" AND NOT(s.url=")
                .push_bind(canonical.url)
                .push(" AND (")
                .push_bind(canonical.version.clone())
                .push("::text IS NULL OR s.version IS NOT DISTINCT FROM ")
                .push_bind(canonical.version)
                .push("))");
        }
    }
    match operation {
        ValueSetOperation::Find {
            system,
            version,
            code,
        } => {
            compiler
                .sql
                .push(" AND s.url=")
                .push_bind(system.to_owned())
                .push(" AND n.code=")
                .push_bind(code.to_owned());
            if let Some(version) = version {
                compiler
                    .sql
                    .push(" AND s.version=")
                    .push_bind(version.to_owned());
            }
        }
        ValueSetOperation::Expand {
            filter: Some(filter),
            ..
        } => {
            let filter = filter.to_lowercase();
            compiler
                .sql
                .push(" AND (strpos(lower(n.code),")
                .push_bind(filter.clone())
                .push(")>0 OR strpos(lower(");
            push_display(
                &mut compiler.sql,
                options.and_then(|o| o.display_language.as_deref()),
                &supplement_ids,
            );
            compiler.sql.push("),").push_bind(filter).push(")>0)");
        }
        _ => {}
    }
    let windowed = matches!(operation, ValueSetOperation::Expand { .. });
    if let ValueSetOperation::Expand { count, offset, .. } = operation {
        // One ordered stream feeds both the total and page. The running charge gates
        // JSON aggregation, so an oversized page never allocates the complete payload.
        const ORDER: &str =
            "system COLLATE \"C\",version COLLATE \"C\" NULLS FIRST,code COLLATE \"C\"";
        compiler.sql.push("), bounds AS (SELECT ").push_bind(offset).push("::bigint AS skip,").push_bind(count).push("::bigint AS take), ")
            .push(format!("ordered AS (SELECT *,row_number() OVER(ORDER BY {ORDER}) AS ordinal FROM matched), charged AS (SELECT ordered.*,sum(CASE WHEN ordinal>skip AND ordinal-skip<=take THEN 768+8::bigint*(octet_length(system)+COALESCE(octet_length(version),0)+octet_length(code)+COALESCE(octet_length(display),0)) ELSE 0 END) OVER(ORDER BY {ORDER} ROWS UNBOUNDED PRECEDING) AS page_charge FROM ordered CROSS JOIN bounds) SELECT count(*)::bigint,COALESCE(jsonb_agg(to_jsonb(charged)-'ordinal'-'page_charge' ORDER BY ordinal) FILTER(WHERE ordinal>(SELECT skip FROM bounds) AND ordinal-(SELECT skip FROM bounds)<=(SELECT take FROM bounds) AND page_charge<={PAGE_BYTES}),'[]'::jsonb), "));
    } else {
        compiler.sql.push("), page AS MATERIALIZED (SELECT * FROM matched ORDER BY system COLLATE \"C\",version COLLATE \"C\" NULLS FIRST,code COLLATE \"C\" LIMIT 1), page_budget AS (SELECT COALESCE(sum(768 + 8::bigint * (octet_length(system) + COALESCE(octet_length(version),0) + octet_length(code) + COALESCE(octet_length(display),0))),0) > ")
            .push(PAGE_BYTES.to_string()).push(" AS exceeded FROM page) SELECT (SELECT count(*) FROM matched)::bigint,CASE WHEN (SELECT exceeded FROM page_budget) THEN '[]'::jsonb ELSE COALESCE((SELECT jsonb_agg(to_jsonb(page)) FROM page),'[]'::jsonb) END, ");
    }
    if compiler.guards.is_empty() {
        compiler.sql.push("false");
    } else {
        for (index, (name, limit)) in compiler.guards.iter().enumerate() {
            if index != 0 {
                compiler.sql.push(" OR ");
            }
            compiler
                .sql
                .push(format!("(SELECT n FROM {name}_size)>{limit}"));
        }
    }
    if windowed {
        compiler.sql.push(format!(
            ",COALESCE(max(page_charge)>{PAGE_BYTES},false) FROM charged"
        ));
    } else {
        compiler.sql.push(", (SELECT exceeded FROM page_budget)");
    }
    let (total, page, exceeded, page_exceeded): (i64, sqlx::types::Json<Value>, bool, bool) =
        compiler.sql.build_query_as().fetch_one(&mut **tx).await?;
    if page_exceeded {
        return Err(unsupported(
            "Native response page exceeds 32 MiB budget; request fewer entries",
        ));
    }
    if exceeded {
        return Err(unsupported(
            "Native hierarchy traversal limit exceeded; no partial result returned",
        ));
    }
    #[derive(serde::Deserialize)]
    struct Row {
        system: String,
        version: Option<String>,
        code: String,
        display: Option<String>,
        inactive: bool,
        case_sensitive: bool,
    }
    let rows: Vec<Row> =
        serde_json::from_value(page.0).map_err(|e| AppError::Internal(e.into()))?;
    let members = rows
        .into_iter()
        .map(|r| Member {
            system: r.system,
            version: r.version,
            code: r.code,
            display: r.display,
            inactive: r.inactive,
            case_sensitive: r.case_sensitive,
            abstract_code: false,
        })
        .collect();
    Ok(ValueSetResult::Native(ValueSetPage {
        resource,
        total,
        members,
    }))
}
fn topological(
    key: &Canonical,
    nodes: &BTreeMap<Canonical, Node>,
    path: &mut BTreeSet<Canonical>,
    done: &mut BTreeSet<Canonical>,
    order: &mut Vec<Canonical>,
) -> Result<(), AppError> {
    if path.len() >= term_squid_core::MAX_DEPTH || path.contains(key) {
        return Err(unsupported("ValueSet import cycle or depth limit"));
    }
    if done.contains(key) {
        return Ok(());
    }
    path.insert(key.clone());
    for selection in nodes[key].include.iter().chain(&nodes[key].exclude) {
        for imported in &selection.imports {
            topological(imported, nodes, path, done, order)?;
        }
    }
    path.remove(key);
    done.insert(key.clone());
    order.push(key.clone());
    Ok(())
}
/// One release ordering policy shared by generic and specialized native operations.
pub(super) fn selected_release_sql(
    sql: &mut QueryBuilder<'static, Postgres>,
    version: Option<&str>,
    implicit: bool,
) {
    if implicit && version.is_none() {
        sql.push("SELECT c.id,c.version,c.native_provider FROM code_systems c JOIN snomed_releases r ON r.code_system_id=c.id WHERE c.url=").push_bind(snomed::SYSTEM.to_owned())
            .push(" AND r.edition_module=").push_bind(snomed::INTERNATIONAL.to_owned()).push(" ORDER BY r.release_date DESC,c.id LIMIT 1");
    } else if let Some(edition) = version.filter(|v| snomed::is_edition_uri(v)) {
        sql.push("SELECT c.id,c.version,c.native_provider FROM code_systems c JOIN snomed_releases r ON r.code_system_id=c.id WHERE c.url=").push_bind(snomed::SYSTEM.to_owned())
            .push(" AND c.version LIKE ").push_bind(format!("{edition}/version/%")).push(" ORDER BY r.release_date DESC,c.id LIMIT 1");
    } else {
        sql.push("SELECT id,version,native_provider FROM code_systems WHERE url=")
            .push_bind(snomed::SYSTEM.to_owned());
        if let Some(version) = version {
            sql.push(" AND version=").push_bind(version.to_owned());
        }
        sql.push(" ORDER BY updated_at DESC,id LIMIT 1");
    }
}

async fn resolve_selection(
    tx: &mut Tx<'_>,
    selection: &Selection,
    context: Option<(&str, &str)>,
    implicit: bool,
    require_primary_scope: bool,
    membership: Option<(&str, &str)>,
) -> Result<SelectionPlan, AppError> {
    let imports = selection
        .value_set
        .iter()
        .map(|s| Canonical::parse(s, None))
        .collect::<Result<Vec<_>, _>>()?;
    let mut plan = SelectionPlan {
        system: None,
        system_url: None,
        versioned: false,
        whole_unversioned: false,
        concepts: selection.concept.clone(),
        filters: Vec::new(),
        primary_filters: Vec::new(),
        imports,
    };
    let Some(system) = &selection.system else {
        return Ok(plan);
    };
    let system = Canonical::parse(system, selection.version.as_deref())?;
    if system.url != snomed::SYSTEM && system.url != loinc::SYSTEM {
        return Err(unsupported(
            "Mixed native/finite CodeSystem composition requires a provider",
        ));
    }
    plan.system_url = Some(system.url.clone());
    let version = system.version.as_deref().or_else(|| {
        context
            .filter(|(url, _)| *url == system.url)
            .map(|(_, v)| v)
    });
    plan.versioned = system.version.is_some();
    plan.whole_unversioned = !plan.versioned
        && selection.concept.is_empty()
        && selection.filter.is_empty()
        && selection.value_set.is_empty();
    let mut query = Sql::new("");
    if system.url == snomed::SYSTEM {
        selected_release_sql(&mut query, version, implicit);
    } else {
        query
            .push("SELECT id,version,native_provider FROM code_systems WHERE url=")
            .push_bind(loinc::SYSTEM.to_owned());
        if let Some(version) = version {
            query.push(" AND version=").push_bind(version.to_owned());
        }
        query.push(" ORDER BY updated_at DESC,id LIMIT 1");
    }
    let row: Option<(Uuid, Option<String>, Option<String>)> =
        query.build_query_as().fetch_optional(&mut **tx).await?;
    let (id, _, provider) = row.ok_or_else(|| {
        AppError::NotFound(if system.url == snomed::SYSTEM {
            format!("SNOMED release {version:?} not found (implicit default: International)")
        } else {
            format!("CodeSystem {} release {version:?} not found", system.url)
        })
    })?;
    if system.url == snomed::SYSTEM {
        if provider.as_deref() != Some("snomed-rf2") {
            return Err(unsupported("Native composition requires an RF2 projection"));
        }
        plan.filters = selection
            .filter
            .iter()
            .map(NativeFilter::parse)
            .collect::<Result<_, _>>()?;
    } else {
        if provider.as_deref() != Some("loinc-csv") {
            return Err(unsupported(
                "Native LOINC composition requires a primary-table projection",
            ));
        }
        plan.primary_filters = selection
            .filter
            .iter()
            .map(PrimaryFilter::parse)
            .collect::<Result<_, _>>()?;
        if let Some((loinc::SYSTEM, code)) = membership {
            loinc::require_primary_code(code)?;
        }
        if require_primary_scope
            && selection.concept.is_empty()
            && !plan
                .primary_filters
                .iter()
                .any(PrimaryFilter::scopes_primary_table)
        {
            return Err(unsupported("LOINC expansion requires explicit primary concepts or source-table=Loinc.csv; all-code enumeration requires accessory source files"));
        }
    }
    plan.system = Some(id);
    let explicit: BTreeSet<_> = selection.concept.iter().map(|c| c.code.clone()).collect();
    for code in &explicit {
        if system.url == snomed::SYSTEM {
            snomed::reject_expression(code)?;
        } else {
            loinc::require_primary_code(code)?;
        }
    }
    if !explicit.is_empty() {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM concepts WHERE code_system_id=$1 AND code=ANY($2)",
        )
        .bind(id)
        .bind(explicit.iter().cloned().collect::<Vec<_>>())
        .fetch_one(&mut **tx)
        .await?;
        if count != explicit.len() as i64 {
            return Err(unsupported("Unknown explicit native terminology code"));
        }
    }
    for filter in &plan.filters {
        let anchor = match filter {
            NativeFilter::Hierarchy { anchor, .. } => Some(anchor),
            NativeFilter::Property { code, .. } if code.bytes().all(|b| b.is_ascii_digit()) => {
                Some(code)
            }
            _ => None,
        };
        if let Some(anchor) = anchor {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM concepts WHERE code_system_id=$1 AND code=$2)",
            )
            .bind(id)
            .bind(anchor)
            .fetch_one(&mut **tx)
            .await?;
            if !exists {
                return Err(unsupported(format!(
                    "Unknown native hierarchy/property code {anchor}"
                )));
            }
        }
        if let NativeFilter::Property { code, .. } = filter {
            if code.bytes().all(|b| b.is_ascii_digit()) {
                let (count,attribute):(i64,bool) = sqlx::query_as("WITH RECURSIVE walk(code) AS (SELECT $2::text UNION SELECT r.destination_code::text FROM walk w CROSS JOIN LATERAL (SELECT destination_code FROM snomed_relationships r WHERE r.source_code=w.code AND r.code_system_id=$1 AND r.active AND r.type_id='116680003' AND r.characteristic_type_id='900000000000011006' OFFSET 0) r), bounded AS MATERIALIZED (SELECT code FROM walk LIMIT 8193) SELECT count(*)::bigint,COALESCE(bool_or(code='410662002'),false) FROM bounded")
                    .bind(id).bind(code).fetch_one(&mut **tx).await?;
                if count > MEMBERSHIP_WALK || !attribute {
                    return Err(unsupported(
                        "Relationship property is not a bounded concept model attribute",
                    ));
                }
            }
        }
    }
    Ok(plan)
}

fn push_display(sql: &mut Sql, language: Option<&str>, supplements: &[Uuid]) {
    if let Some(language) = language {
        let mut ranges = vec![language.to_lowercase()];
        while let Some((prefix, _)) = ranges.last().and_then(|s| s.rsplit_once('-')) {
            ranges.push(prefix.to_owned());
        }
        sql.push("COALESCE((SELECT d.term FROM (SELECT d.term,d.language_code,EXISTS(SELECT 1 FROM snomed_language_members l JOIN snomed_releases r ON r.code_system_id=l.code_system_id AND r.dialect=l.refset_id WHERE l.code_system_id=d.code_system_id AND l.description_id=d.id AND l.active AND l.acceptability_id='900000000000548007') AS preferred,d.id::text AS identity FROM snomed_descriptions d WHERE d.code_system_id=s.id AND d.concept_code=n.code AND d.active UNION ALL SELECT designation->>'value',designation->>'language',designation->'use'->>'code'='display',c.id::text FROM code_systems supplement JOIN concepts c ON c.code_system_id=supplement.id CROSS JOIN LATERAL jsonb_array_elements(COALESCE(c.designations,'[]')) designation WHERE supplement.id=ANY(").push_bind(supplements.to_vec()).push("::uuid[]) AND split_part(supplement.content->>'supplements','|',1)=s.url AND (split_part(supplement.content->>'supplements','|',2)='' OR split_part(supplement.content->>'supplements','|',2)=s.version) AND (c.code=n.code OR NOT s.case_sensitive AND lower(c.code)=lower(n.code))) d WHERE (lower(d.language_code)=ANY(").push_bind(ranges.clone()).push("::text[]) OR strpos(lower(d.language_code),").push_bind(format!("{}-",language.to_lowercase())).push(")=1) ORDER BY COALESCE(array_position(").push_bind(ranges).push("::text[],lower(d.language_code)),0),d.preferred DESC NULLS LAST,d.identity LIMIT 1),n.display)");
    } else {
        sql.push("n.display");
    }
}
