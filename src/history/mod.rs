mod base;
mod file_backed;

pub use base::{FormatTimeType, History, HistoryNavigationQuery, InnerEntry};
pub use file_backed::{FileBackedHistory, HISTORY_SIZE};
