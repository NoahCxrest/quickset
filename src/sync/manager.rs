// sync manager - coordinates syncing data from sources to quickset

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::table::{Column, Database};
use crate::{log_debug, log_error, log_info, log_warn};

use super::source::{Source, SyncTable};

// status of a sync operation
#[derive(Clone, Debug)]
pub struct SyncStatus {
    pub table: String,
    pub last_sync: Option<Instant>,
    pub last_row_count: usize,
    pub last_duration_ms: u64,
    pub error: Option<String>,
    pub syncing: bool,
}

// result of a sync operation
#[derive(Debug)]
pub struct SyncResult {
    pub table: String,
    pub success: bool,
    pub rows_synced: usize,
    pub duration_ms: u64,
    pub error: Option<String>,
}

// configuration for the sync manager
#[derive(Clone)]
pub struct SyncConfig {
    pub enabled: bool,
    pub interval_secs: u64,         // how often to sync (0 = manual only)
    pub tables: Vec<SyncTable>,     // tables to sync
    pub clear_before_sync: bool,    // drop and recreate table before sync
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 0,
            tables: Vec::new(),
            clear_before_sync: true,
        }
    }
}

impl SyncConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_interval(mut self, secs: u64) -> Self {
        self.interval_secs = secs;
        self.enabled = secs > 0;
        self
    }

    pub fn with_table(mut self, table: SyncTable) -> Self {
        self.tables.push(table);
        self
    }

    pub fn clear_before_sync(mut self, clear: bool) -> Self {
        self.clear_before_sync = clear;
        self
    }
}

pub struct SyncManager {
    source: Box<dyn Source>,
    config: SyncConfig,
    status: RwLock<HashMap<String, SyncStatus>>,
    running: AtomicBool,
    sync_count: AtomicU64,
}

impl SyncManager {
    pub fn new(source: Box<dyn Source>, config: SyncConfig) -> Self {
        let mut status = HashMap::new();
        
        // initialize status for each table
        for table in &config.tables {
            status.insert(table.target_table.clone(), SyncStatus {
                table: table.target_table.clone(),
                last_sync: None,
                last_row_count: 0,
                last_duration_ms: 0,
                error: None,
                syncing: false,
            });
        }

        Self {
            source,
            config,
            status: RwLock::new(status),
            running: AtomicBool::new(false),
            sync_count: AtomicU64::new(0),
        }
    }

    // sync a single table
    pub fn sync_table(&self, table: &SyncTable, db: &Arc<RwLock<Database>>) -> SyncResult {
        let start = Instant::now();
        let target = &table.target_table;

        log_info!("sync", "starting sync for table: {}", target);

        // mark as syncing
        if let Ok(mut status) = self.status.write() {
            if let Some(s) = status.get_mut(target) {
                s.syncing = true;
                s.error = None;
            }
        }

        // fetch from source
        let fetch_result = match self.source.fetch_table(table) {
            Ok(r) => r,
            Err(e) => {
                let error_msg = e.to_string();
                log_error!("sync", "failed to fetch {}: {}", target, error_msg);
                
                self.update_status(target, 0, start.elapsed(), Some(error_msg.clone()));
                
                return SyncResult {
                    table: target.clone(),
                    success: false,
                    rows_synced: 0,
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(error_msg),
                };
            }
        };

        log_debug!("sync", "fetched {} rows from source for {}", fetch_result.row_count, target);

        // update quickset
        let mut db = db.write().unwrap();

        // optionally clear and recreate table
        if self.config.clear_before_sync {
            let _ = db.drop_table(target);
            
            let columns: Vec<Column> = table.columns.iter()
                .map(|c| Column {
                    name: c.target_name.clone().into_boxed_str(),
                    col_type: c.col_type,
                })
                .collect();

            if let Err(e) = db.create_table_with_capacity(target, columns, fetch_result.row_count) {
                let error_msg = format!("failed to create table: {}", e);
                log_error!("sync", "{}", error_msg);
                
                self.update_status(target, 0, start.elapsed(), Some(error_msg.clone()));
                
                return SyncResult {
                    table: target.clone(),
                    success: false,
                    rows_synced: 0,
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(error_msg),
                };
            }
        }

        // insert rows
        let table_ref = match db.get_table_mut(target) {
            Some(t) => t,
            None => {
                let error_msg = "table not found after creation".to_string();
                log_error!("sync", "{}", error_msg);
                
                self.update_status(target, 0, start.elapsed(), Some(error_msg.clone()));
                
                return SyncResult {
                    table: target.clone(),
                    success: false,
                    rows_synced: 0,
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(error_msg),
                };
            }
        };

        let mut inserted = 0;
        for row in fetch_result.rows {
            if table_ref.insert(row).is_ok() {
                inserted += 1;
            }
        }

        let duration = start.elapsed();
        log_info!("sync", "synced {} rows to {} in {}ms", inserted, target, duration.as_millis());

        self.update_status(target, inserted, duration, None);
        self.sync_count.fetch_add(1, Ordering::Relaxed);

        SyncResult {
            table: target.clone(),
            success: true,
            rows_synced: inserted,
            duration_ms: duration.as_millis() as u64,
            error: None,
        }
    }

    // sync all configured tables
    pub fn sync_all(&self, db: &Arc<RwLock<Database>>) -> Vec<SyncResult> {
        self.config.tables.iter()
            .map(|table| self.sync_table(table, db))
            .collect()
    }

    // start background sync thread
    pub fn start_background_sync(self: Arc<Self>, db: Arc<RwLock<Database>>) {
        if self.config.interval_secs == 0 {
            log_info!("sync", "background sync disabled (interval = 0)");
            return;
        }

        if self.running.swap(true, Ordering::SeqCst) {
            log_warn!("sync", "background sync already running");
            return;
        }

        let interval = Duration::from_secs(self.config.interval_secs);
        log_info!("sync", "starting background sync every {}s", self.config.interval_secs);

        thread::spawn(move || {
            // initial sync
            self.sync_all(&db);

            while self.running.load(Ordering::Relaxed) {
                thread::sleep(interval);
                
                if !self.running.load(Ordering::Relaxed) {
                    break;
                }

                self.sync_all(&db);
            }

            log_info!("sync", "background sync stopped");
        });
    }

    // stop background sync
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    // get sync status for all tables
    pub fn status(&self) -> Vec<SyncStatus> {
        self.status.read()
            .map(|s| s.values().cloned().collect())
            .unwrap_or_default()
    }

    // get sync status for a specific table
    pub fn table_status(&self, table: &str) -> Option<SyncStatus> {
        self.status.read()
            .ok()
            .and_then(|s| s.get(table).cloned())
    }

    // get total sync count
    pub fn sync_count(&self) -> u64 {
        self.sync_count.load(Ordering::Relaxed)
    }

    // check if background sync is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    // helper to update status
    fn update_status(&self, table: &str, rows: usize, duration: Duration, error: Option<String>) {
        if let Ok(mut status) = self.status.write() {
            if let Some(s) = status.get_mut(table) {
                s.last_sync = Some(Instant::now());
                s.last_row_count = rows;
                s.last_duration_ms = duration.as_millis() as u64;
                s.error = error;
                s.syncing = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::ColumnType;

    #[test]
    fn test_sync_config_builder() {
        let table = SyncTable::new("source_users", "users")
            .with_column("id", "id", ColumnType::Int);

        let config = SyncConfig::new()
            .with_interval(60)
            .with_table(table);

        assert!(config.enabled);
        assert_eq!(config.interval_secs, 60);
        assert_eq!(config.tables.len(), 1);
    }

    #[test]
    fn test_sync_config_manual_only() {
        let config = SyncConfig::new()
            .with_interval(0);

        assert!(!config.enabled);
    }
}
