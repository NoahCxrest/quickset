// clickhouse source implementation
// uses native http interface for simplicity (no extra deps)

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::storage::Value;
use crate::table::ColumnType;

use super::source::{FetchResult, Source, SourceConfig, SourceError, SyncTable};

pub struct ClickHouseSource {
    config: SourceConfig,
    connected: bool,
}

impl ClickHouseSource {
    pub fn new(config: SourceConfig) -> Self {
        Self {
            config,
            connected: false,
        }
    }

    // build the select query for a table
    fn build_query(&self, table: &SyncTable) -> String {
        if let Some(ref query) = table.query_override {
            return query.clone();
        }

        let columns: Vec<&str> = table.columns.iter()
            .map(|c| c.source_name.as_str())
            .collect();

        if columns.is_empty() {
            format!("SELECT * FROM {}", table.source_table)
        } else {
            format!("SELECT {} FROM {}", columns.join(", "), table.source_table)
        }
    }

    // execute a query via clickhouse http interface
    fn execute_query(&self, query: &str) -> Result<String, SourceError> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        
        let mut stream = TcpStream::connect(&addr)
            .map_err(|e| SourceError::Connection(format!("failed to connect to {}: {}", addr, e)))?;
        
        stream.set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| SourceError::Connection(e.to_string()))?;
        stream.set_write_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| SourceError::Connection(e.to_string()))?;

        // build http request
        let db = self.config.database.as_deref().unwrap_or("default");
        let user = self.config.user.as_deref().unwrap_or("default");
        let pass = self.config.password.as_deref().unwrap_or("");
        
        // use tsv format for easier parsing
        let full_query = format!("{} FORMAT TabSeparated", query);
        let body = full_query.as_bytes();
        
        let request = format!(
            "POST /?database={}&user={}&password={} HTTP/1.1\r\n\
             Host: {}\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n",
            db, user, pass, self.config.host, body.len()
        );

        stream.write_all(request.as_bytes())
            .map_err(|e| SourceError::Query(format!("failed to send request: {}", e)))?;
        stream.write_all(body)
            .map_err(|e| SourceError::Query(format!("failed to send query: {}", e)))?;
        stream.flush()
            .map_err(|e| SourceError::Query(e.to_string()))?;

        // read response
        let mut reader = BufReader::new(stream);
        let mut response = String::new();
        
        // read status line
        let mut status_line = String::new();
        reader.read_line(&mut status_line)
            .map_err(|e| SourceError::Query(format!("failed to read response: {}", e)))?;
        
        if !status_line.contains("200") {
            // read error body
            let mut error_body = String::new();
            let _ = reader.read_line(&mut error_body);
            return Err(SourceError::Query(format!("clickhouse error: {} {}", status_line.trim(), error_body.trim())));
        }

        // skip headers until empty line
        loop {
            let mut line = String::new();
            reader.read_line(&mut line)
                .map_err(|e| SourceError::Query(e.to_string()))?;
            if line.trim().is_empty() {
                break;
            }
        }

        // read body
        reader.read_line(&mut response)
            .map_err(|e| SourceError::Query(format!("failed to read body: {}", e)))?;
        
        // read remaining lines
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => response.push_str(&line),
                Err(_) => break,
            }
        }

        Ok(response)
    }

    // parse a tsv value into our Value type
    fn parse_value(s: &str, col_type: ColumnType) -> Value {
        let s = s.trim();
        
        if s.is_empty() || s == "\\N" || s == "NULL" {
            return Value::Null;
        }

        match col_type {
            ColumnType::Int => {
                s.parse::<i64>()
                    .map(Value::Int)
                    .unwrap_or(Value::Null)
            }
            ColumnType::Float => {
                s.parse::<f64>()
                    .map(Value::Float)
                    .unwrap_or(Value::Null)
            }
            ColumnType::String => {
                // unescape common clickhouse escapes
                let unescaped = s
                    .replace("\\t", "\t")
                    .replace("\\n", "\n")
                    .replace("\\\\", "\\");
                Value::String(unescaped.into_boxed_str())
            }
            ColumnType::Bytes => {
                Value::Bytes(s.as_bytes().to_vec().into_boxed_slice())
            }
        }
    }

    // parse tsv response into rows
    fn parse_response(&self, response: &str, table: &SyncTable) -> Result<Vec<Vec<Value>>, SourceError> {
        let mut rows = Vec::new();
        
        for line in response.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let fields: Vec<&str> = line.split('\t').collect();
            
            if fields.len() != table.columns.len() && !table.columns.is_empty() {
                // if no columns specified, try to parse anyway
                if table.columns.is_empty() {
                    let row: Vec<Value> = fields.iter()
                        .map(|f| Value::String((*f).to_string().into_boxed_str()))
                        .collect();
                    rows.push(row);
                    continue;
                }
                return Err(SourceError::Parse(format!(
                    "column count mismatch: expected {}, got {}",
                    table.columns.len(), fields.len()
                )));
            }

            let row: Vec<Value> = fields.iter()
                .zip(table.columns.iter())
                .map(|(field, col)| Self::parse_value(field, col.col_type))
                .collect();
            
            rows.push(row);
        }

        Ok(rows)
    }
}

impl Source for ClickHouseSource {
    fn connect(&mut self) -> Result<(), SourceError> {
        // test connection with a simple query
        self.execute_query("SELECT 1")?;
        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) {
        self.connected = false;
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn fetch_table(&self, table: &SyncTable) -> Result<FetchResult, SourceError> {
        let query = self.build_query(table);
        let response = self.execute_query(&query)?;
        let rows = self.parse_response(&response, table)?;
        let row_count = rows.len();
        
        Ok(FetchResult { rows, row_count })
    }

    fn name(&self) -> &str {
        "clickhouse"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_query() {
        let config = SourceConfig::new("localhost", 8123);
        let source = ClickHouseSource::new(config);

        let table = SyncTable::new("users", "users")
            .with_column("id", "id", ColumnType::Int)
            .with_column("name", "name", ColumnType::String);

        let query = source.build_query(&table);
        assert_eq!(query, "SELECT id, name FROM users");
    }

    #[test]
    fn test_build_query_with_override() {
        let config = SourceConfig::new("localhost", 8123);
        let source = ClickHouseSource::new(config);

        let table = SyncTable::new("users", "users")
            .with_query("SELECT * FROM users WHERE active = 1");

        let query = source.build_query(&table);
        assert_eq!(query, "SELECT * FROM users WHERE active = 1");
    }

    #[test]
    fn test_parse_value() {
        assert_eq!(
            ClickHouseSource::parse_value("123", ColumnType::Int),
            Value::Int(123)
        );
        assert_eq!(
            ClickHouseSource::parse_value("45.67", ColumnType::Float),
            Value::Float(45.67)
        );
        assert_eq!(
            ClickHouseSource::parse_value("hello", ColumnType::String),
            Value::String("hello".into())
        );
        assert_eq!(
            ClickHouseSource::parse_value("\\N", ColumnType::Int),
            Value::Null
        );
    }
}
