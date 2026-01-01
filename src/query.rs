use serde::{Deserialize, Serialize};
use crate::storage::Value;
use crate::table::ColumnType;

#[derive(Debug, Deserialize)]
pub struct CreateTableRequest {
    pub name: String,
    pub columns: Vec<ColumnDef>,
    pub capacity: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ColumnDef {
    pub name: String,
    #[serde(rename = "type")]
    pub col_type: String,
}

impl ColumnDef {
    pub fn to_column_type(&self) -> Option<ColumnType> {
        match self.col_type.to_lowercase().as_str() {
            "int" | "integer" | "i64" => Some(ColumnType::Int),
            "float" | "double" | "f64" => Some(ColumnType::Float),
            "string" | "text" | "varchar" => Some(ColumnType::String),
            "bytes" | "blob" | "binary" => Some(ColumnType::Bytes),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct InsertRequest {
    pub table: String,
    pub rows: Vec<Vec<JsonValue>>,
}

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub table: String,
    pub column: String,
    #[serde(rename = "type")]
    pub search_type: String,
    pub value: Option<JsonValue>,
    pub prefix: Option<String>,
    pub query: Option<String>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct GetRequest {
    pub table: String,
    pub ids: Vec<u64>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
    pub table: String,
    pub ids: Vec<u64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRequest {
    pub table: String,
    pub id: u64,
    pub values: Vec<JsonValue>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum JsonValue {
    Null,
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
}

impl JsonValue {
    pub fn to_value(&self) -> Value {
        match self {
            JsonValue::Null => Value::Null,
            JsonValue::Int(i) => Value::Int(*i),
            JsonValue::Float(f) => Value::Float(*f),
            JsonValue::String(s) => Value::String(s.clone().into_boxed_str()),
            JsonValue::Bytes(b) => Value::Bytes(b.clone().into_boxed_slice()),
        }
    }
}

impl From<&Value> for JsonValue {
    fn from(v: &Value) -> Self {
        match v {
            Value::Null => JsonValue::Null,
            Value::Int(i) => JsonValue::Int(*i),
            Value::Float(f) => JsonValue::Float(*f),
            Value::String(s) => JsonValue::String(s.to_string()),
            Value::Bytes(b) => JsonValue::Bytes(b.to_vec()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(msg: &str) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct InsertResponse {
    pub ids: Vec<u64>,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub rows: Vec<RowResponse>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct RowResponse {
    pub id: u64,
    pub values: Vec<JsonValue>,
}

#[derive(Debug, Serialize)]
pub struct TableInfo {
    pub name: String,
    pub row_count: usize,
    pub column_count: usize,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub tables: Vec<TableInfo>,
}

// sync-related request/response types
#[derive(Debug, Deserialize)]
pub struct SyncConfigRequest {
    pub source_type: Option<String>,        // "clickhouse"
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
    pub interval_secs: Option<u64>,
    pub tables: Vec<SyncTableRequest>,
}

#[derive(Debug, Deserialize)]
pub struct SyncTableRequest {
    pub source_table: String,
    pub target_table: String,
    pub columns: Vec<SyncColumnRequest>,
    pub query: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SyncColumnRequest {
    pub source: String,
    pub target: String,
    #[serde(rename = "type")]
    pub col_type: String,
}

#[derive(Debug, Deserialize)]
pub struct SyncTriggerRequest {
    pub table: Option<String>,  // if none, sync all
}

#[derive(Debug, Serialize)]
pub struct SyncStatusResponse {
    pub tables: Vec<SyncTableStatus>,
    pub running: bool,
    pub total_syncs: u64,
}

#[derive(Debug, Serialize)]
pub struct SyncTableStatus {
    pub table: String,
    pub last_sync_ago_secs: Option<u64>,
    pub last_row_count: usize,
    pub last_duration_ms: u64,
    pub error: Option<String>,
    pub syncing: bool,
}

#[derive(Debug, Serialize)]
pub struct SyncResultResponse {
    pub results: Vec<SyncTableResult>,
}

#[derive(Debug, Serialize)]
pub struct SyncTableResult {
    pub table: String,
    pub success: bool,
    pub rows_synced: usize,
    pub duration_ms: u64,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_value_conversion() {
        let json_int = JsonValue::Int(42);
        let value = json_int.to_value();
        assert_eq!(value, Value::Int(42));

        let json_str = JsonValue::String("test".to_string());
        let value = json_str.to_value();
        assert_eq!(value, Value::String("test".into()));
    }

    #[test]
    fn test_column_type_parsing() {
        let col = ColumnDef {
            name: "test".to_string(),
            col_type: "STRING".to_string(),
        };
        assert_eq!(col.to_column_type(), Some(ColumnType::String));

        let col = ColumnDef {
            name: "test".to_string(),
            col_type: "int".to_string(),
        };
        assert_eq!(col.to_column_type(), Some(ColumnType::Int));
    }

    #[test]
    fn test_api_response() {
        let resp: ApiResponse<i32> = ApiResponse::ok(42);
        assert!(resp.success);
        assert_eq!(resp.data, Some(42));

        let resp: ApiResponse<i32> = ApiResponse::err("test error");
        assert!(!resp.success);
        assert_eq!(resp.error, Some("test error".to_string()));
    }
}
