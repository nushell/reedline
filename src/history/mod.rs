mod base;
mod cursor;
mod file_backed;
#[cfg(feature="sqlite")]
mod sqlite_backed;
#[cfg(feature="sqlite")]
pub use sqlite_backed::SqliteBackedHistory;

pub use base::{HistoryItem, HistoryItemId, History, HistoryNavigationQuery, Result, SearchQuery, SearchDirection};
pub use cursor::HistoryCursor;

pub use file_backed::{FileBackedHistory, HISTORY_SIZE};