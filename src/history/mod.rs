mod base;
mod file_backed;

pub use base::{History, HistoryNavigationQuery};
pub use file_backed::{FileBackedHistory, HISTORY_SIZE};
