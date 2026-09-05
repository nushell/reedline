mod base;
mod cursor;
mod file_backed;
mod item;
#[cfg(feature = "_sqlite")]
mod sqlite_backed;
#[cfg(feature = "_sqlite")]
pub use sqlite_backed::SqliteBackedHistory;

pub use base::{
    CommandLineSearch, History, HistoryNavigationQuery, JsonFilterValue, SearchDirection,
    SearchFilter, SearchQuery,
};
pub use cursor::HistoryCursor;
pub use item::{
    HistoryItem, HistoryItemExtraInfo, HistoryItemId, HistorySessionId, IgnoreAllExtraInfo,
};

pub use file_backed::{FileBackedHistory, HISTORY_SIZE};
