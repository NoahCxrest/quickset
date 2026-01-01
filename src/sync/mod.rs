// sync module - pulls data from external sources of truth
//
// architecture:
//   Source (trait) -> defines how to connect and fetch data
//   SyncManager    -> coordinates syncing, handles scheduling
//   clickhouse.rs  -> clickhouse implementation
//
// to add a new source: implement the Source trait

mod source;
mod manager;
mod clickhouse;

pub use source::{Source, SourceConfig, SyncTable, ColumnMapping};
pub use manager::{SyncManager, SyncStatus, SyncResult, SyncConfig};
pub use clickhouse::ClickHouseSource;
