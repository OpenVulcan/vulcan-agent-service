use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Channel;

use crate::pb_lancedb::{
    lance_db_service_client::LanceDbServiceClient,
    CreateTableRequest, ColumnDef, InputFormat, OutputFormat,
    DeleteRequest, DropTableRequest, SearchRequest, UpsertRequest,
};
use crate::pb_sqlite::{
    sqlite_service_client::SqliteServiceClient,
    ExecuteBatchItem, ExecuteBatchRequest, ExecuteRequest, QueryRequest, SqliteValue,
    sqlite_value,
};
use crate::pb_vmm::{
    vmm_service_client::VmmServiceClient,
    ResolveProjectRequest, EnsureProjectRequest, DeleteProjectRequest, MigrateProjectRequest,
    ResolveUserRequest, DeleteUserRequest, GetProfileNodesRequest, GetProfileBundleRequest,
    ApplyProfileInstructionRequest, SearchMemoryEventsRequest, GetTurnDetailsRequest,
    WriteMemoriesRequest, WriteMemoryItem, ScratchpadUpsertRequest, ScratchpadItem as VmmScratchpadItem,
    ScratchpadDeleteRequest, ScratchpadGetRequest, ScratchpadListKeysRequest,
    ScratchpadCleanRequest, ChatCompactRequest, PreCheckRequest, PostActionRequest,
    PostActionTimelineItem,
};

// ============================================================
// LanceDb gRPC client
// ============================================================

#[derive(Clone)]
pub struct LanceDbClient {
    client: Arc<Mutex<LanceDbServiceClient<Channel>>>,
    pub endpoint: String,
}

impl LanceDbClient {
    pub async fn connect(endpoint: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let client = LanceDbServiceClient::connect(endpoint.to_string()).await?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
            endpoint: endpoint.to_string(),
        })
    }

    pub async fn create_table(
        &self,
        table_name: &str,
        columns: Vec<ColumnDef>,
        overwrite: bool,
    ) -> Result<String, String> {
        let req = tonic::Request::new(CreateTableRequest {
            table_name: table_name.to_string(),
            columns,
            overwrite_if_exists: overwrite,
        });
        let mut client = self.client.lock().await;
        let resp = client.create_table(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        if inner.success {
            Ok(inner.message)
        } else {
            Err(inner.message)
        }
    }

    pub async fn vector_upsert(
        &self,
        table_name: &str,
        input_format: InputFormat,
        data: Vec<u8>,
        key_columns: Vec<String>,
    ) -> Result<String, String> {
        let req = tonic::Request::new(UpsertRequest {
            table_name: table_name.to_string(),
            input_format: input_format.into(),
            data,
            key_columns,
        });
        let mut client = self.client.lock().await;
        let resp = client.vector_upsert(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        if inner.success {
            Ok(format!(
                "version={}, rows={}, inserted={}, updated={}",
                inner.version, inner.input_rows, inner.inserted_rows, inner.updated_rows
            ))
        } else {
            Err(inner.message)
        }
    }

    pub async fn vector_search(
        &self,
        table_name: &str,
        vector: Vec<f32>,
        limit: u32,
        filter: String,
        vector_column: String,
        output_format: OutputFormat,
    ) -> Result<Vec<u8>, String> {
        let req = tonic::Request::new(SearchRequest {
            table_name: table_name.to_string(),
            vector,
            limit,
            filter,
            vector_column,
            output_format: output_format.into(),
        });
        let mut client = self.client.lock().await;
        let resp = client.vector_search(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        if inner.success {
            Ok(inner.data)
        } else {
            Err(inner.message)
        }
    }

    pub async fn delete(&self, table_name: &str, condition: String) -> Result<String, String> {
        let req = tonic::Request::new(DeleteRequest {
            table_name: table_name.to_string(),
            condition,
        });
        let mut client = self.client.lock().await;
        let resp = client.delete(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        if inner.success {
            Ok(format!("version={}, deleted={}", inner.version, inner.deleted_rows))
        } else {
            Err(inner.message)
        }
    }

    pub async fn drop_table(&self, table_name: &str) -> Result<String, String> {
        let req = tonic::Request::new(DropTableRequest {
            table_name: table_name.to_string(),
        });
        let mut client = self.client.lock().await;
        let resp = client.drop_table(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        if inner.success {
            Ok(inner.message)
        } else {
            Err(inner.message)
        }
    }
}

// ============================================================
// Sqlite gRPC client
// ============================================================

#[derive(Clone)]
pub struct SqliteClient {
    client: Arc<Mutex<SqliteServiceClient<Channel>>>,
    pub endpoint: String,
}

impl SqliteClient {
    pub async fn connect(endpoint: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let client = SqliteServiceClient::connect(endpoint.to_string()).await?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
            endpoint: endpoint.to_string(),
        })
    }

    pub async fn execute_script(
        &self,
        sql: &str,
        params: Vec<SqliteValue>,
    ) -> Result<String, String> {
        let req = tonic::Request::new(ExecuteRequest {
            sql: sql.to_string(),
            params_json: String::new(),
            params,
        });
        let mut client = self.client.lock().await;
        let resp = client.execute_script(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        if inner.success {
            Ok(format!(
                "rows_changed={}, last_insert_rowid={}",
                inner.rows_changed, inner.last_insert_rowid
            ))
        } else {
            Err(inner.message)
        }
    }

    pub async fn execute_batch(
        &self,
        sql: &str,
        items: Vec<Vec<SqliteValue>>,
    ) -> Result<String, String> {
        let items = items
            .into_iter()
            .map(|params| ExecuteBatchItem { params })
            .collect();
        let req = tonic::Request::new(ExecuteBatchRequest {
            sql: sql.to_string(),
            items,
        });
        let mut client = self.client.lock().await;
        let resp = client.execute_batch(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        if inner.success {
            Ok(format!(
                "rows_changed={}, statements={}",
                inner.rows_changed, inner.statements_executed
            ))
        } else {
            Err(inner.message)
        }
    }

    pub async fn query_json(&self, sql: &str, params: Vec<SqliteValue>) -> Result<String, String> {
        let req = tonic::Request::new(QueryRequest {
            sql: sql.to_string(),
            params_json: String::new(),
            params,
        });
        let mut client = self.client.lock().await;
        let resp = client.query_json(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(inner.json_data)
    }

    pub async fn query_stream(
        &self,
        sql: &str,
        params: Vec<SqliteValue>,
    ) -> Result<Vec<u8>, String> {
        let req = tonic::Request::new(QueryRequest {
            sql: sql.to_string(),
            params_json: String::new(),
            params,
        });
        let mut client = self.client.lock().await;
        let mut stream = client
            .query_stream(req)
            .await
            .map_err(|e| e.to_string())?
            .into_inner();

        let mut data = Vec::new();
        while let Some(chunk) = stream.message().await.map_err(|e| e.to_string())? {
            data.extend(chunk.arrow_ipc_chunk);
        }
        Ok(data)
    }
}

// ============================================================
// Helper functions for SqliteValue construction
// ============================================================

pub fn sqlite_int64(v: i64) -> SqliteValue {
    SqliteValue {
        kind: Some(sqlite_value::Kind::Int64Value(v)),
    }
}

pub fn sqlite_float64(v: f64) -> SqliteValue {
    SqliteValue {
        kind: Some(sqlite_value::Kind::Float64Value(v)),
    }
}

pub fn sqlite_string(v: &str) -> SqliteValue {
    SqliteValue {
        kind: Some(sqlite_value::Kind::StringValue(v.to_string())),
    }
}

pub fn sqlite_bool(v: bool) -> SqliteValue {
    SqliteValue {
        kind: Some(sqlite_value::Kind::BoolValue(v)),
    }
}

pub fn sqlite_null() -> SqliteValue {
    SqliteValue {
        kind: Some(sqlite_value::Kind::NullValue(crate::pb_sqlite::NullValue {})),
    }
}

pub fn sqlite_bytes(v: Vec<u8>) -> SqliteValue {
    SqliteValue {
        kind: Some(sqlite_value::Kind::BytesValue(v)),
    }
}

// ============================================================
// VMCP Scratchpad Store (uses vldb_sqlite gRPC with vmcp_ tables)
// ============================================================

use serde_json::json;

// Table DDL (created lazily via CREATE TABLE IF NOT EXISTS)
const SCRATCHPAD_PLAN_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS vmcp_scratchpad_plans (
  id INTEGER PRIMARY KEY,
  project_id INTEGER NOT NULL,
  user_id INTEGER NOT NULL,
  session_key TEXT NOT NULL,
  plan_name TEXT NOT NULL,
  plan_name_norm TEXT NOT NULL,
  created_timestamp INTEGER NOT NULL,
  updated_timestamp INTEGER NOT NULL,
  UNIQUE(project_id, user_id, session_key)
)"#;

const SCRATCHPAD_NODE_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS vmcp_scratchpad_nodes (
  id INTEGER PRIMARY KEY,
  plan_id INTEGER NOT NULL,
  item_key TEXT NOT NULL,
  item_value TEXT NOT NULL,
  created_timestamp INTEGER NOT NULL,
  updated_timestamp INTEGER NOT NULL,
  UNIQUE(plan_id, item_key)
)"#;

// Validation constants
const SCRATCHPAD_PLAN_NAME_MAX_LEN: usize = 128;
const SCRATCHPAD_ITEM_KEY_MAX_LEN: usize = 128;
const SCRATCHPAD_ITEM_VALUE_MAX_LEN: usize = 16000;
const SCRATCHPAD_BATCH_ITEM_LIMIT: usize = 32;
const SCRATCHPAD_SESSION_KEY_MAX_LEN: usize = 128;

#[derive(Clone, Debug)]
pub struct ScratchpadItem {
    pub key: String,
    pub value: String,
}

fn now_unix_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn sql_escape(s: &str) -> String {
    format!("'{}'", s.replace("'", "''"))
}

fn validate_scope(project_id: u64, user_id: u64, session_id: &str) -> Result<(), String> {
    if project_id == 0 {
        return Err("project_id must be a numeric id".into());
    }
    if user_id == 0 {
        return Err("user_id must be a numeric id".into());
    }
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Err("session_id is required".into());
    }
    if trimmed.len() > SCRATCHPAD_SESSION_KEY_MAX_LEN {
        return Err(format!("session_id must be <= {} characters", SCRATCHPAD_SESSION_KEY_MAX_LEN));
    }
    Ok(())
}

fn validate_plan_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("plan_name is required".into());
    }
    if trimmed.len() > SCRATCHPAD_PLAN_NAME_MAX_LEN {
        return Err(format!("plan_name must be <= {} characters", SCRATCHPAD_PLAN_NAME_MAX_LEN));
    }
    Ok(())
}

fn normalize_items(items: &[ScratchpadItem]) -> Result<Vec<ScratchpadItem>, String> {
    if items.is_empty() {
        return Err("items must contain at least one item".into());
    }
    if items.len() > SCRATCHPAD_BATCH_ITEM_LIMIT {
        return Err(format!("items must contain <= {} items", SCRATCHPAD_BATCH_ITEM_LIMIT));
    }
    // Deduplicate: keep last value for same key
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (idx, item) in items.iter().enumerate() {
        let key = item.key.trim().to_string();
        let value = item.value.trim().to_string();
        if key.is_empty() {
            return Err(format!("items[{}].key is required", idx));
        }
        if key.len() > SCRATCHPAD_ITEM_KEY_MAX_LEN {
            return Err(format!("items[{}].key must be <= {} characters", idx, SCRATCHPAD_ITEM_KEY_MAX_LEN));
        }
        if value.is_empty() {
            return Err(format!("items[{}].value is required", idx));
        }
        if value.len() > SCRATCHPAD_ITEM_VALUE_MAX_LEN {
            return Err(format!("items[{}].value must be <= {} characters", idx, SCRATCHPAD_ITEM_VALUE_MAX_LEN));
        }
        map.insert(key, value);
    }
    let mut out: Vec<ScratchpadItem> = map.into_iter().map(|(k, v)| ScratchpadItem { key: k, value: v }).collect();
    out.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(out)
}

fn normalize_keys(keys: &[String]) -> Result<Vec<String>, String> {
    if keys.is_empty() {
        return Err("keys must contain at least one key".into());
    }
    let mut set = std::collections::HashSet::new();
    for (idx, key) in keys.iter().enumerate() {
        let trimmed = key.trim().to_string();
        if trimmed.is_empty() {
            return Err(format!("keys[{}] is required", idx));
        }
        if trimmed.len() > SCRATCHPAD_ITEM_KEY_MAX_LEN {
            return Err(format!("keys[{}] must be <= {} characters", idx, SCRATCHPAD_ITEM_KEY_MAX_LEN));
        }
        set.insert(trimmed);
    }
    let mut out: Vec<String> = set.into_iter().collect();
    out.sort();
    Ok(out)
}

/// Ensure scratchpad tables exist (idempotent, called before every operation)
async fn ensure_tables(client: &SqliteClient) -> Result<(), String> {
    client.execute_script(SCRATCHPAD_PLAN_DDL, vec![]).await?;
    client.execute_script(SCRATCHPAD_NODE_DDL, vec![]).await?;
    Ok(())
}

#[derive(Clone)]
pub struct ScratchpadStore {
    sqlite: SqliteClient,
}

impl ScratchpadStore {
    pub fn new(sqlite: SqliteClient) -> Self {
        Self { sqlite }
    }

    pub fn sqlite_client(&self) -> &SqliteClient {
        &self.sqlite
    }

    async fn load_plan(&self, project_id: u64, user_id: u64, session_key: &str) -> Result<Option<serde_json::Value>, String> {
        let sql = "SELECT id, project_id, user_id, session_key, plan_name, plan_name_norm, created_timestamp, updated_timestamp FROM vmcp_scratchpad_plans WHERE project_id = ? AND user_id = ? AND session_key = ? LIMIT 1";
        let json_str = self.sqlite.query_json(sql, vec![
            sqlite_int64(project_id as i64),
            sqlite_int64(user_id as i64),
            sqlite_string(session_key.trim()),
        ]).await?;
        let arr: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap_or_default();
        Ok(arr.into_iter().next())
    }

    async fn create_plan(&self, project_id: u64, user_id: u64, session_key: &str, plan_name: &str, now_ms: i64) -> Result<serde_json::Value, String> {
        // Re-check under lock: get next id
        let id_json = self.sqlite.query_json("SELECT COALESCE(MAX(id), 0) + 1 AS next_id FROM vmcp_scratchpad_plans", vec![]).await?;
        let id_arr: Vec<serde_json::Value> = serde_json::from_str(&id_json).unwrap_or_default();
        let next_id = id_arr.first().and_then(|v| v.get("next_id")).and_then(|v| v.as_i64()).unwrap_or(1);

        let plan_name_trimmed = plan_name.trim();
        let sql = format!(
            "INSERT INTO vmcp_scratchpad_plans (id, project_id, user_id, session_key, plan_name, plan_name_norm, created_timestamp, updated_timestamp) VALUES ({}, {}, {}, {}, {}, {}, {}, {})",
            next_id, project_id, user_id, sql_escape(session_key.trim()), sql_escape(plan_name_trimmed),
            sql_escape(&plan_name_trimmed.to_lowercase()), now_ms, now_ms
        );
        self.sqlite.execute_script(&sql, vec![]).await?;
        Ok(json!({
            "id": next_id,
            "project_id": project_id,
            "user_id": user_id,
            "session_key": session_key.trim(),
            "plan_name": plan_name_trimmed,
            "plan_name_norm": plan_name_trimmed.to_lowercase(),
            "created_timestamp": now_ms,
            "updated_timestamp": now_ms,
        }))
    }

    /// Upsert items into scratchpad
    pub async fn upsert(
        &self,
        project_id: u64,
        user_id: u64,
        session_id: &str,
        plan_name: &str,
        items: Vec<ScratchpadItem>,
    ) -> Result<String, String> {
        validate_scope(project_id, user_id, session_id)?;
        validate_plan_name(plan_name)?;
        let items = normalize_items(&items)?;
        ensure_tables(&self.sqlite).await?;

        let plan = self.load_plan(project_id, user_id, session_id).await?;
        if let Some(ref p) = plan {
            let canonical = p.get("plan_name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let input = plan_name.trim();
            if canonical.to_lowercase() != input.to_lowercase() {
                return Ok(format!("status=failed, msg=The input plan_name does not match the current scratchpad plan. Check whether the plan_name is misspelled or call Clean before switching to a new plan. Current plan: {}. Input plan: {}", canonical, input));
            }
        }

        let now_ms = now_unix_millis();
        let plan = if let Some(p) = plan {
            p
        } else {
            // Concurrent re-check: try loading again (simplified, no mutex needed since unique constraint handles it)
            self.create_plan(project_id, user_id, session_id, plan_name, now_ms).await?
        };

        let plan_id = plan.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        // Load existing nodes for insert/update counting
        let existing_keys: Vec<String> = items.iter().map(|i| i.key.clone()).collect();
        let existing_json = if !existing_keys.is_empty() {
            let placeholders = existing_keys.iter().map(|k| sql_escape(k)).collect::<Vec<_>>().join(", ");
            let sql = format!("SELECT item_key FROM vmcp_scratchpad_nodes WHERE plan_id = {} AND item_key IN ({})", plan_id, placeholders);
            let j = self.sqlite.query_json(&sql, vec![]).await?;
            serde_json::from_str::<Vec<serde_json::Value>>(&j).unwrap_or_default()
        } else {
            vec![]
        };
        let existing_set: std::collections::HashSet<String> = existing_json.iter()
            .filter_map(|v| v.get("item_key").and_then(|v| v.as_str()).map(String::from))
            .collect();

        let mut inserted = 0i64;
        let mut updated = 0i64;
        let mut statements = String::from("BEGIN IMMEDIATE;\n");
        for item in &items {
            if existing_set.contains(&item.key) {
                updated += 1;
                statements.push_str(&format!(
                    "INSERT INTO vmcp_scratchpad_nodes (plan_id, item_key, item_value, created_timestamp, updated_timestamp) VALUES ({}, {}, {}, {}, {}) ON CONFLICT(plan_id, item_key) DO UPDATE SET item_value = excluded.item_value, updated_timestamp = excluded.updated_timestamp;\n",
                    plan_id, sql_escape(&item.key), sql_escape(&item.value), now_ms, now_ms
                ));
            } else {
                inserted += 1;
                statements.push_str(&format!(
                    "INSERT INTO vmcp_scratchpad_nodes (plan_id, item_key, item_value, created_timestamp, updated_timestamp) VALUES ({}, {}, {}, {}, {});\n",
                    plan_id, sql_escape(&item.key), sql_escape(&item.value), now_ms, now_ms
                ));
            }
        }
        statements.push_str(&format!("UPDATE vmcp_scratchpad_plans SET updated_timestamp = {} WHERE id = {};\n", now_ms, plan_id));
        statements.push_str("COMMIT;\n");

        self.sqlite.execute_script(&statements, vec![]).await?;
        let total = inserted + updated;
        Ok(format!("status=success, msg=Upserted {} scratchpad record(s)., plan_name={}, affected={}, inserted={}, updated={}",
            total, plan_name.trim(), total, inserted, updated))
    }

    /// Delete keys from scratchpad
    pub async fn delete(
        &self,
        project_id: u64,
        user_id: u64,
        session_id: &str,
        plan_name: &str,
        keys: Vec<String>,
    ) -> Result<String, String> {
        validate_scope(project_id, user_id, session_id)?;
        validate_plan_name(plan_name)?;
        let keys = normalize_keys(&keys)?;
        ensure_tables(&self.sqlite).await?;

        let plan = self.load_plan(project_id, user_id, session_id).await?;
        let Some(p) = plan else {
            return Ok("status=success, msg=No scratchpad plan exists for the current session. Create records first., affected=0".into());
        };

        let plan_id = p.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let canonical = p.get("plan_name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let input = plan_name.trim();
        if canonical.to_lowercase() != input.to_lowercase() {
            return Ok(format!("status=failed, msg=The input plan_name does not match the current scratchpad plan. Current plan: {}. Input plan: {}", canonical, input));
        }

        let now_ms = now_unix_millis();
        let key_list = keys.iter().map(|k| sql_escape(k)).collect::<Vec<_>>().join(", ");
        let delete_sql = format!(
            "BEGIN IMMEDIATE;\nDELETE FROM vmcp_scratchpad_nodes WHERE plan_id = {} AND item_key IN ({});\nUPDATE vmcp_scratchpad_plans SET updated_timestamp = {} WHERE id = {};\nCOMMIT;\n",
            plan_id, key_list, now_ms, plan_id
        );
        self.sqlite.execute_script(&delete_sql, vec![]).await?;

        Ok(format!("status=success, msg=Deleted {} scratchpad record(s)., plan_name={}, affected={}",
            keys.len(), canonical, keys.len()))
    }

    /// Get scratchpad items
    pub async fn get(
        &self,
        project_id: u64,
        user_id: u64,
        session_id: &str,
        keys: Vec<String>,
    ) -> Result<String, String> {
        validate_scope(project_id, user_id, session_id)?;
        ensure_tables(&self.sqlite).await?;

        let plan = self.load_plan(project_id, user_id, session_id).await?;
        let Some(p) = plan else {
            return Ok(r#"{"status":"success","msg":"No scratchpad records found for the current session.","items":[],"item_count":0}"#.into());
        };

        let plan_id = p.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let plan_name = p.get("plan_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let updated = p.get("updated_timestamp").and_then(|v| v.as_i64()).unwrap_or(0);

        let items_json = if keys.is_empty() {
            let sql = format!("SELECT item_key, item_value FROM vmcp_scratchpad_nodes WHERE plan_id = {} ORDER BY item_key ASC, id ASC", plan_id);
            self.sqlite.query_json(&sql, vec![]).await.unwrap_or_else(|_| "[]".into())
        } else {
            let validated_keys = normalize_keys(&keys)?;
            let key_list = validated_keys.iter().map(|k| sql_escape(k)).collect::<Vec<_>>().join(", ");
            let sql = format!("SELECT item_key, item_value FROM vmcp_scratchpad_nodes WHERE plan_id = {} AND item_key IN ({}) ORDER BY item_key ASC, id ASC", plan_id, key_list);
            self.sqlite.query_json(&sql, vec![]).await.unwrap_or_else(|_| "[]".into())
        };

        let items: Vec<serde_json::Value> = serde_json::from_str(&items_json).unwrap_or_default();
        Ok(serde_json::to_string(&json!({
            "status": "success",
            "msg": format!("Retrieved {} scratchpad record(s).", items.len()),
            "plan_name": plan_name,
            "updated_timestamp": updated,
            "item_count": items.len(),
            "items": items,
        })).unwrap_or_default())
    }

    /// List scratchpad keys
    pub async fn list_keys(
        &self,
        project_id: u64,
        user_id: u64,
        session_id: &str,
    ) -> Result<String, String> {
        validate_scope(project_id, user_id, session_id)?;
        ensure_tables(&self.sqlite).await?;

        let plan = self.load_plan(project_id, user_id, session_id).await?;
        let Some(p) = plan else {
            return Ok(r#"{"status":"success","msg":"No scratchpad records found for the current session.","keys":[],"key_count":0}"#.into());
        };

        let plan_id = p.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let plan_name = p.get("plan_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let updated = p.get("updated_timestamp").and_then(|v| v.as_i64()).unwrap_or(0);

        let keys_json = self.sqlite.query_json(
            &format!("SELECT item_key FROM vmcp_scratchpad_nodes WHERE plan_id = {} ORDER BY item_key ASC, id ASC", plan_id),
            vec![],
        ).await.unwrap_or_else(|_| "[]".into());
        let rows: Vec<serde_json::Value> = serde_json::from_str(&keys_json).unwrap_or_default();
        let keys: Vec<String> = rows.iter()
            .filter_map(|r| r.get("item_key").and_then(|v| v.as_str()).map(String::from))
            .collect();

        Ok(serde_json::to_string(&json!({
            "status": "success",
            "msg": format!("Listed {} scratchpad key(s).", keys.len()),
            "plan_name": plan_name,
            "updated_timestamp": updated,
            "key_count": keys.len(),
            "keys": keys,
        })).unwrap_or_default())
    }

    /// Clean entire scratchpad scope
    pub async fn clean(
        &self,
        project_id: u64,
        user_id: u64,
        session_id: &str,
    ) -> Result<String, String> {
        validate_scope(project_id, user_id, session_id)?;
        ensure_tables(&self.sqlite).await?;

        let plan = self.load_plan(project_id, user_id, session_id).await?;
        let Some(p) = plan else {
            return Ok("status=success, msg=The current scratchpad is already empty., affected=0".into());
        };

        let plan_id = p.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let clean_sql = format!(
            "BEGIN IMMEDIATE;\nDELETE FROM vmcp_scratchpad_nodes WHERE plan_id = {};\nDELETE FROM vmcp_scratchpad_plans WHERE id = {};\nCOMMIT;\n",
            plan_id, plan_id
        );
        self.sqlite.execute_script(&clean_sql, vec![]).await?;

        Ok("status=success, msg=Scratchpad history has been cleared.".into())
    }
}

// ============================================================
// Vmm gRPC client (wraps tonic-generated VmmServiceClient)
// ============================================================

#[derive(Clone)]
pub struct VmmClient {
    client: Arc<Mutex<VmmServiceClient<Channel>>>,
    pub endpoint: String,
}

impl VmmClient {
    pub async fn connect(endpoint: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let client = VmmServiceClient::connect(endpoint.to_string()).await?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
            endpoint: endpoint.to_string(),
        })
    }

    // 1. Healthz
    pub async fn healthz(&self) -> Result<String, String> {
        let req = tonic::Request::new(());
        let mut client = self.client.lock().await;
        let resp = client.healthz(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("status={}, trace_id={}", inner.status, inner.trace_id))
    }

    // 2. ListProjects
    pub async fn list_projects(&self) -> Result<String, String> {
        let req = tonic::Request::new(());
        let mut client = self.client.lock().await;
        let resp = client.list_projects(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let projects: Vec<String> = inner.projects.iter().map(|p| p.display_path.clone()).collect();
        Ok(format!("projects={}, trace_id={}", projects.join(", "), inner.trace_id))
    }

    // 3. ResolveProject
    pub async fn resolve_project(&self, project_ref: &str) -> Result<String, String> {
        let req = tonic::Request::new(ResolveProjectRequest {
            project_ref: project_ref.to_string(),
        });
        let mut client = self.client.lock().await;
        let resp = client.resolve_project(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let path = inner.project.as_ref().map(|p| p.display_path.clone()).unwrap_or_default();
        Ok(format!("message={}, project={}, trace_id={}", inner.message, path, inner.trace_id))
    }

    // 4. EnsureProject
    pub async fn ensure_project(&self, project_path: &str, confirm_create: bool) -> Result<String, String> {
        let req = tonic::Request::new(EnsureProjectRequest {
            project_path: project_path.to_string(),
            confirm_create,
        });
        let mut client = self.client.lock().await;
        let resp = client.ensure_project(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let path = inner.project.as_ref().map(|p| p.display_path.clone()).unwrap_or_default();
        Ok(format!("message={}, exists={}, project={}, trace_id={}", inner.message, inner.exists, path, inner.trace_id))
    }

    // 5. DeleteProject
    pub async fn delete_project(&self, project_path: &str, confirm_delete: bool) -> Result<String, String> {
        let req = tonic::Request::new(DeleteProjectRequest {
            project_path: project_path.to_string(),
            confirm_delete,
        });
        let mut client = self.client.lock().await;
        let resp = client.delete_project(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("message={}, needs_confirm={}, deleted_sessions={}, deleted_messages={}, deleted_memories={}, deleted_vector={}, trace_id={}",
            inner.message, inner.needs_confirm, inner.deleted_sessions, inner.deleted_messages, inner.deleted_memories, inner.deleted_vector_rows, inner.trace_id))
    }

    // 6. MigrateProject
    pub async fn migrate_project(&self, source: &str, target: &str, confirm: bool) -> Result<String, String> {
        let req = tonic::Request::new(MigrateProjectRequest {
            source_project_path: source.to_string(),
            target_project_path: target.to_string(),
            confirm_migrate: confirm,
        });
        let mut client = self.client.lock().await;
        let resp = client.migrate_project(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("message={}, needs_confirm={}, migrated_sessions={}, migrated_messages={}, migrated_memories={}, rebuilt_vector={}, trace_id={}",
            inner.message, inner.needs_confirm, inner.migrated_sessions, inner.migrated_messages, inner.migrated_memories, inner.rebuilt_vector_rows, inner.trace_id))
    }

    // 7. ResolveUser
    pub async fn resolve_user(&self, user_ref: &str, confirm_create: bool) -> Result<String, String> {
        let req = tonic::Request::new(ResolveUserRequest {
            user_ref: user_ref.to_string(),
            confirm_create,
        });
        let mut client = self.client.lock().await;
        let resp = client.resolve_user(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let user_info = inner.user.as_ref().map(|u| format!("{}({})", u.user_name, u.user_id)).unwrap_or_default();
        Ok(format!("message={}, user={}, created={}, exists={}, trace_id={}", inner.message, user_info, inner.created, inner.exists, inner.trace_id))
    }

    // 8. ListUsers
    pub async fn list_users(&self) -> Result<String, String> {
        let req = tonic::Request::new(());
        let mut client = self.client.lock().await;
        let resp = client.list_users(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let users: Vec<String> = inner.users.iter().map(|u| format!("{}({})", u.user_name, u.user_id)).collect();
        Ok(format!("users={}, trace_id={}", users.join(", "), inner.trace_id))
    }

    // 9. DeleteUser
    pub async fn delete_user(&self, user_ref: &str, confirmation_code: &str) -> Result<String, String> {
        let req = tonic::Request::new(DeleteUserRequest {
            user_ref: user_ref.to_string(),
            confirmation_code: confirmation_code.to_string(),
        });
        let mut client = self.client.lock().await;
        let resp = client.delete_user(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let user_info = inner.user.as_ref().map(|u| format!("{}({})", u.user_name, u.user_id)).unwrap_or_default();
        Ok(format!("message={}, requires_confirmation={}, user={}, deleted_sessions={}, deleted_messages={}, deleted_memories={}, deleted_vector={}, trace_id={}",
            inner.message, inner.requires_confirmation, user_info, inner.deleted_sessions, inner.deleted_messages, inner.deleted_memories, inner.deleted_vector_rows, inner.trace_id))
    }

    // 10. GetProfileNodes
    pub async fn get_profile_nodes(
        &self,
        target: i32,
        user_id: u64,
        project_id: u64,
        limit: u32,
    ) -> Result<String, String> {
        let req = tonic::Request::new(GetProfileNodesRequest {
            target,
            user_id,
            project_id,
            limit,
        });
        let mut client = self.client.lock().await;
        let resp = client.get_profile_nodes(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("node_count={}, trace_id={}", inner.nodes.len(), inner.trace_id))
    }

    // 11. GetProfileBundle
    pub async fn get_profile_bundle(
        &self,
        user_id: u64,
        project_id: u64,
        mode: i32,
        include_exp: Option<bool>,
    ) -> Result<String, String> {
        let req = tonic::Request::new(GetProfileBundleRequest {
            user_id,
            project_id,
            mode,
            include_explanation: include_exp,
        });
        let mut client = self.client.lock().await;
        let resp = client.get_profile_bundle(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("combined_text_len={}, trace_id={}", inner.combined_text.len(), inner.trace_id))
    }

    // 12. ApplyProfileInstruction
    pub async fn apply_profile_instruction(
        &self,
        target: i32,
        user_id: u64,
        project_id: u64,
        instruction: &str,
    ) -> Result<String, String> {
        let req = tonic::Request::new(ApplyProfileInstructionRequest {
            target,
            user_id,
            project_id,
            instruction: instruction.to_string(),
        });
        let mut client = self.client.lock().await;
        let resp = client.apply_profile_instruction(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("instruction_id={}, accepted_nodes={}, retired_nodes={}, trace_id={}",
            inner.instruction_id, inner.accepted_nodes.len(), inner.retired_nodes.len(), inner.trace_id))
    }

    // 13. SearchMemoryEvents
    pub async fn search_memory_events(
        &self,
        user_id: u64,
        project_id: u64,
        queries: Vec<String>,
        top_k: u32,
    ) -> Result<String, String> {
        let req = tonic::Request::new(SearchMemoryEventsRequest {
            user_id,
            project_id,
            queries,
            top_k,
        });
        let mut client = self.client.lock().await;
        let resp = client.search_memory_events(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let total: usize = inner.results.iter().map(|r| r.hits.len()).sum();
        Ok(format!("total_hits={}, query_groups={}, trace_id={}", total, inner.results.len(), inner.trace_id))
    }

    // 14. GetTurnDetails
    pub async fn get_turn_details(&self, turn_ids: Vec<u64>) -> Result<String, String> {
        let req = tonic::Request::new(GetTurnDetailsRequest {
            turn_ids,
        });
        let mut client = self.client.lock().await;
        let resp = client.get_turn_details(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("turns_loaded={}, trace_id={}", inner.turns.len(), inner.trace_id))
    }

    // 15. WriteMemories
    pub async fn write_memories(
        &self,
        session_id: &str,
        user_id: u64,
        project_id: u64,
        items: Vec<WriteMemoryItem>,
    ) -> Result<String, String> {
        let req = tonic::Request::new(WriteMemoriesRequest {
            session_id: session_id.to_string(),
            user_id,
            project_id,
            items,
        });
        let mut client = self.client.lock().await;
        let resp = client.write_memories(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        let deduped: usize = inner.items.iter().filter(|i| i.deduped).count();
        Ok(format!("written={}, deduped={}, trace_id={}", inner.items.len(), deduped, inner.trace_id))
    }

    // 16. ScratchpadUpsert
    pub async fn scratchpad_upsert(
        &self,
        session_id: &str,
        user_id: u64,
        project_id: u64,
        plan_name: &str,
        key: Option<String>,
        value: Option<String>,
        items: Vec<crate::grpc_client::ScratchpadItem>,
    ) -> Result<String, String> {
        let req = tonic::Request::new(ScratchpadUpsertRequest {
            session_id: session_id.to_string(),
            user_id,
            project_id,
            plan_name: plan_name.to_string(),
            key,
            value,
            items: items.into_iter().map(|i| VmmScratchpadItem {
                key: i.key,
                value: i.value,
            }).collect(),
        });
        let mut client = self.client.lock().await;
        let resp = client.scratchpad_upsert(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("status={:?}, msg={}, affected={}, inserted={}, updated={}",
            inner.status, inner.msg, inner.affected_count, inner.inserted_count, inner.updated_count))
    }

    // 17. ScratchpadDelete
    pub async fn scratchpad_delete(
        &self,
        session_id: &str,
        user_id: u64,
        project_id: u64,
        plan_name: &str,
        key: Option<String>,
        keys: Vec<String>,
    ) -> Result<String, String> {
        let req = tonic::Request::new(ScratchpadDeleteRequest {
            session_id: session_id.to_string(),
            user_id,
            project_id,
            plan_name: plan_name.to_string(),
            key,
            keys,
        });
        let mut client = self.client.lock().await;
        let resp = client.scratchpad_delete(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("status={:?}, msg={}, affected={}", inner.status, inner.msg, inner.affected_count))
    }

    // 18. ScratchpadGet
    pub async fn scratchpad_get(
        &self,
        session_id: &str,
        user_id: u64,
        project_id: u64,
        keys: Vec<String>,
    ) -> Result<String, String> {
        let req = tonic::Request::new(ScratchpadGetRequest {
            session_id: session_id.to_string(),
            user_id,
            project_id,
            keys,
        });
        let mut client = self.client.lock().await;
        let resp = client.scratchpad_get(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("status={:?}, msg={}, plan_name={}, item_count={}",
            inner.status, inner.msg, inner.plan_name, inner.item_count))
    }

    // 19. ScratchpadListKeys
    pub async fn scratchpad_list_keys(
        &self,
        session_id: &str,
        user_id: u64,
        project_id: u64,
    ) -> Result<String, String> {
        let req = tonic::Request::new(ScratchpadListKeysRequest {
            session_id: session_id.to_string(),
            user_id,
            project_id,
        });
        let mut client = self.client.lock().await;
        let resp = client.scratchpad_list_keys(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("status={:?}, msg={}, plan_name={}, key_count={}",
            inner.status, inner.msg, inner.plan_name, inner.key_count))
    }

    // 20. ScratchpadClean
    pub async fn scratchpad_clean(
        &self,
        session_id: &str,
        user_id: u64,
        project_id: u64,
    ) -> Result<String, String> {
        let req = tonic::Request::new(ScratchpadCleanRequest {
            session_id: session_id.to_string(),
            user_id,
            project_id,
        });
        let mut client = self.client.lock().await;
        let resp = client.scratchpad_clean(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("status={:?}, msg={}", inner.status, inner.msg))
    }

    // 21. ChatCompact
    pub async fn chat_compact(
        &self,
        session_id: &str,
        user_id: u64,
        project_id: u64,
    ) -> Result<String, String> {
        let req = tonic::Request::new(ChatCompactRequest {
            session_id: session_id.to_string(),
            user_id,
            project_id,
        });
        let mut client = self.client.lock().await;
        let resp = client.chat_compact(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("accepted={}, updated={}, compacted_turn_id={}, trace_id={}",
            inner.accepted, inner.updated, inner.compacted_turn_id, inner.trace_id))
    }

    // 22. PreCheck
    pub async fn pre_check(
        &self,
        session_id: &str,
        user_id: u64,
        project_id: u64,
        user_content: &str,
        recall_mode: i32,
    ) -> Result<String, String> {
        let req = tonic::Request::new(PreCheckRequest {
            session_id: session_id.to_string(),
            user_id,
            project_id,
            user_content: user_content.to_string(),
            recall_mode,
        });
        let mut client = self.client.lock().await;
        let resp = client.pre_check(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("should_inject={}, context_items={}, degraded={}, trace_id={}",
            inner.should_inject, inner.context_items.len(), inner.degraded, inner.trace_id))
    }

    // 23. PostAction
    pub async fn post_action(
        &self,
        session_id: &str,
        user_id: u64,
        project_id: u64,
        user_content: &str,
        assistant_content: &str,
        timeline: Vec<PostActionTimelineItem>,
    ) -> Result<String, String> {
        let req = tonic::Request::new(PostActionRequest {
            session_id: session_id.to_string(),
            user_id,
            project_id,
            user_content: user_content.to_string(),
            assistant_content: assistant_content.to_string(),
            timeline,
        });
        let mut client = self.client.lock().await;
        let resp = client.post_action(req).await.map_err(|e| e.to_string())?;
        let inner = resp.into_inner();
        Ok(format!("accepted={}, trace_id={}", inner.accepted, inner.trace_id))
    }
}
