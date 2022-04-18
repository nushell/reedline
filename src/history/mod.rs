mod base;
mod cursor;
mod file_backed;
#[cfg(feature = "sqlite")]
mod sqlite_backed;
#[cfg(feature = "sqlite")]
pub use sqlite_backed::SqliteBackedHistory;

pub use base::{
    History, HistoryItem, HistoryItemId, HistoryNavigationQuery, Result, SearchDirection,
    SearchQuery,
};
pub use cursor::HistoryCursor;

pub use file_backed::{FileBackedHistory, HISTORY_SIZE};
