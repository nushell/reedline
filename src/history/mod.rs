mod base;
mod file_backed;
#[cfg(feature="sqlite")]
mod sqlite_backed;
#[cfg(feature="sqlite")]
pub use sqlite_backed::SqliteBackedHistory;

pub use base::{History, HistoryNavigationQuery};
pub use file_backed::{FileBackedHistory, HISTORY_SIZE};