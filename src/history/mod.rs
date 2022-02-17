mod base;
mod file_backed;

pub use base::{History, HistoryNavigationQuery,InnerEntry,FormatTimeType};
pub use file_backed::{ FileBackedHistory, HISTORY_SIZE};
