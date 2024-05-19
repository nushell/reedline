mod base;
mod cursor;
mod file_backed;
mod item;
#[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
mod sqlite_backed;
#[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
pub use sqlite_backed::SqliteBackedHistory;

#[cfg(any(feature = "rqlite"))]
mod rqlite_backed;
#[cfg(any(feature = "rqlite"))]
pub use rqlite_backed::RqliteBackedHistory;

pub use base::{
    CommandLineSearch, History, HistoryNavigationQuery, HistoryStorageDest, SearchDirection,
    SearchFilter, SearchQuery,
};
pub use cursor::HistoryCursor;
pub use item::{HistoryItem, HistoryItemId, HistorySessionId};

pub use file_backed::{FileBackedHistory, HISTORY_SIZE};
