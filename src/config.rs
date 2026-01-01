use std::env;

// controls which operations require authentication
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AuthLevel {
    None,       // no auth required for anything
    Write,      // auth required for write operations only
    Read,       // auth required for read and write operations
    All,        // auth required for everything including health
}

impl AuthLevel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "none" | "off" | "false" | "0" => Some(Self::None),
            "write" | "writes" => Some(Self::Write),
            "read" | "reads" => Some(Self::Read),
            "all" | "full" | "true" | "1" => Some(Self::All),
            _ => None,
        }
    }

    pub fn requires_auth_for_read(&self) -> bool {
        matches!(self, Self::Read | Self::All)
    }

    pub fn requires_auth_for_write(&self) -> bool {
        matches!(self, Self::Write | Self::Read | Self::All)
    }

    pub fn requires_auth_for_health(&self) -> bool {
        matches!(self, Self::All)
    }
}

#[derive(Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub auth_level: AuthLevel,
    pub admin_user: String,
    pub admin_pass: String,
    pub log_level: String,
    pub max_connections: usize,
}

impl Config {
    pub fn from_env() -> Self {
        // support both old QUICKSET_AUTH and new QUICKSET_AUTH_LEVEL
        let auth_level = env::var("QUICKSET_AUTH_LEVEL")
            .ok()
            .and_then(|s| AuthLevel::from_str(&s))
            .or_else(|| {
                // backwards compatibility: treat old bool as all-or-nothing
                env::var("QUICKSET_AUTH").ok().and_then(|s| {
                    if s == "1" || s.to_lowercase() == "true" {
                        Some(AuthLevel::All)
                    } else {
                        Some(AuthLevel::None)
                    }
                })
            })
            .unwrap_or(AuthLevel::None);

        Self {
            host: env::var("QUICKSET_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("QUICKSET_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8080),
            auth_level,
            admin_user: env::var("QUICKSET_ADMIN_USER").unwrap_or_else(|_| "admin".to_string()),
            admin_pass: env::var("QUICKSET_ADMIN_PASS").unwrap_or_else(|_| "admin".to_string()),
            log_level: env::var("QUICKSET_LOG").unwrap_or_else(|_| "info".to_string()),
            max_connections: env::var("QUICKSET_MAX_CONN")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000),
        }
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    // helper for backwards compat
    pub fn auth_enabled(&self) -> bool {
        self.auth_level != AuthLevel::None
    }
}

// sync source configuration (parsed from env)
#[derive(Clone, Debug)]
pub struct SyncSourceConfig {
    pub enabled: bool,
    pub source_type: String,        // "clickhouse" for now
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
    pub interval_secs: u64,
    pub tables: Vec<String>,        // comma-separated table mappings
}

impl SyncSourceConfig {
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("QUICKSET_SYNC_ENABLED")
                .map(|s| s == "1" || s.to_lowercase() == "true")
                .unwrap_or(false),
            source_type: std::env::var("QUICKSET_SYNC_SOURCE")
                .unwrap_or_else(|_| "clickhouse".to_string()),
            host: std::env::var("QUICKSET_SYNC_HOST")
                .unwrap_or_else(|_| "localhost".to_string()),
            port: std::env::var("QUICKSET_SYNC_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8123),
            user: std::env::var("QUICKSET_SYNC_USER")
                .unwrap_or_else(|_| "default".to_string()),
            password: std::env::var("QUICKSET_SYNC_PASSWORD")
                .unwrap_or_default(),
            database: std::env::var("QUICKSET_SYNC_DATABASE")
                .unwrap_or_else(|_| "default".to_string()),
            interval_secs: std::env::var("QUICKSET_SYNC_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300), // default 5 minutes
            tables: std::env::var("QUICKSET_SYNC_TABLES")
                .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
                .unwrap_or_default(),
        }
    }
}

impl Default for SyncSourceConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config {
            host: "0.0.0.0".to_string(),
            port: 8080,
            auth_level: AuthLevel::None,
            admin_user: "admin".to_string(),
            admin_pass: "admin".to_string(),
            log_level: "info".to_string(),
            max_connections: 1000,
        };
        
        assert_eq!(config.address(), "0.0.0.0:8080");
    }

    #[test]
    fn test_auth_level_parsing() {
        assert_eq!(AuthLevel::from_str("none"), Some(AuthLevel::None));
        assert_eq!(AuthLevel::from_str("write"), Some(AuthLevel::Write));
        assert_eq!(AuthLevel::from_str("read"), Some(AuthLevel::Read));
        assert_eq!(AuthLevel::from_str("all"), Some(AuthLevel::All));
        assert_eq!(AuthLevel::from_str("true"), Some(AuthLevel::All));
        assert_eq!(AuthLevel::from_str("false"), Some(AuthLevel::None));
    }

    #[test]
    fn test_auth_level_permissions() {
        assert!(!AuthLevel::None.requires_auth_for_read());
        assert!(!AuthLevel::None.requires_auth_for_write());
        
        assert!(!AuthLevel::Write.requires_auth_for_read());
        assert!(AuthLevel::Write.requires_auth_for_write());
        
        assert!(AuthLevel::Read.requires_auth_for_read());
        assert!(AuthLevel::Read.requires_auth_for_write());
        
        assert!(AuthLevel::All.requires_auth_for_health());
    }

    #[test]
    fn test_sync_config_defaults() {
        let config = SyncSourceConfig {
            enabled: false,
            source_type: "clickhouse".to_string(),
            host: "localhost".to_string(),
            port: 8123,
            user: "default".to_string(),
            password: String::new(),
            database: "default".to_string(),
            interval_secs: 300,
            tables: vec![],
        };
        
        assert!(!config.enabled);
        assert_eq!(config.source_type, "clickhouse");
        assert_eq!(config.port, 8123);
    }
}
