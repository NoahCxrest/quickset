// source trait - implement this to add new data sources

use crate::storage::Value;
use crate::table::ColumnType;

// describes a column mapping from source to quickset
#[derive(Clone, Debug)]
pub struct ColumnMapping {
    pub source_name: String,    // column name in source
    pub target_name: String,    // column name in quickset
    pub col_type: ColumnType,   // quickset column type
}

// describes a table to sync
#[derive(Clone, Debug)]
pub struct SyncTable {
    pub source_table: String,       // table name in source (can include db like "db.table")
    pub target_table: String,       // table name in quickset
    pub columns: Vec<ColumnMapping>,
    pub query_override: Option<String>, // optional: custom query instead of SELECT *
}

impl SyncTable {
    pub fn new(source: &str, target: &str) -> Self {
        Self {
            source_table: source.to_string(),
            target_table: target.to_string(),
            columns: Vec::new(),
            query_override: None,
        }
    }

    pub fn with_column(mut self, source: &str, target: &str, col_type: ColumnType) -> Self {
        self.columns.push(ColumnMapping {
            source_name: source.to_string(),
            target_name: target.to_string(),
            col_type,
        });
        self
    }

    pub fn with_query(mut self, query: &str) -> Self {
        self.query_override = Some(query.to_string());
        self
    }
}

// configuration for connecting to a source
#[derive(Clone, Debug)]
pub struct SourceConfig {
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
}

impl SourceConfig {
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            host: host.to_string(),
            port,
            user: None,
            password: None,
            database: None,
        }
    }

    pub fn with_auth(mut self, user: &str, password: &str) -> Self {
        self.user = Some(user.to_string());
        self.password = Some(password.to_string());
        self
    }

    pub fn with_database(mut self, db: &str) -> Self {
        self.database = Some(db.to_string());
        self
    }
}

// result of fetching rows from source
pub struct FetchResult {
    pub rows: Vec<Vec<Value>>,
    pub row_count: usize,
}

// error type for source operations
#[derive(Debug)]
pub enum SourceError {
    Connection(String),
    Query(String),
    Parse(String),
    Config(String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connection(s) => write!(f, "connection error: {}", s),
            Self::Query(s) => write!(f, "query error: {}", s),
            Self::Parse(s) => write!(f, "parse error: {}", s),
            Self::Config(s) => write!(f, "config error: {}", s),
        }
    }
}

// the main trait - implement this to add a new source type
pub trait Source: Send + Sync {
    // connect to the source
    fn connect(&mut self) -> Result<(), SourceError>;
    
    // disconnect from the source
    fn disconnect(&mut self);
    
    // check if connected
    fn is_connected(&self) -> bool;
    
    // fetch all rows for a table
    fn fetch_table(&self, table: &SyncTable) -> Result<FetchResult, SourceError>;
    
    // get source name for logging
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_table_builder() {
        let table = SyncTable::new("source_users", "users")
            .with_column("id", "id", ColumnType::Int)
            .with_column("name", "name", ColumnType::String);
        
        assert_eq!(table.source_table, "source_users");
        assert_eq!(table.target_table, "users");
        assert_eq!(table.columns.len(), 2);
    }

    #[test]
    fn test_source_config_builder() {
        let config = SourceConfig::new("localhost", 9000)
            .with_auth("default", "password")
            .with_database("mydb");
        
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 9000);
        assert_eq!(config.user, Some("default".to_string()));
        assert_eq!(config.database, Some("mydb".to_string()));
    }
}
