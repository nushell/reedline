mod base;
mod cursor;
mod file_backed;
mod item;
#[cfg(feature = "sqlite")]
mod sqlite_backed;
#[cfg(feature = "sqlite")]
pub use sqlite_backed::SqliteBackedHistory;

pub use base::{History, HistoryNavigationQuery, Result, SearchDirection, SearchQuery};
pub use cursor::HistoryCursor;
pub use item::{HistoryItem, HistoryItemId, HistorySessionId};

pub use file_backed::{FileBackedHistory, HISTORY_SIZE};
