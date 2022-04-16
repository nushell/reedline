mod base;
#[cfg(feature="file-history")]
mod file_backed;
#[cfg(feature="sqlite")]
mod sqlite_backed;
#[cfg(feature="sqlite")]
pub use sqlite_backed::SqliteBackedHistory;

pub use base::{HistoryCursor, HistoryItem, HistoryItemId, History, HistoryNavigationQuery, Result};

#[cfg(feature="file-history")]
pub use file_backed::{FileBackedHistory, HISTORY_SIZE};