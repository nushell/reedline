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
    HistoryItem, HistoryItemExtraInfo, HistoryItemId, HistorySessionId, HistoryTimestamp,
    IgnoreAllExtraInfo,
};

pub use file_backed::{FileBackedHistory, HISTORY_SIZE};

/// The history backends compiled into this build. Tests that should hold for
/// any `History` take one as an rstest case, so enabling `sqlite` adds cases
/// instead of swapping the file-backed ones out.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TestBackend {
    File,
    #[cfg(feature = "_sqlite")]
    Sqlite,
}

#[cfg(test)]
impl TestBackend {
    pub(crate) fn in_memory(self) -> Box<dyn History> {
        match self {
            Self::File => Box::<FileBackedHistory>::default(),
            #[cfg(feature = "_sqlite")]
            Self::Sqlite => Box::new(SqliteBackedHistory::in_memory().unwrap()),
        }
    }

    /// The id a backend gives its first entry: file-backed ids are indices
    /// from 0, sqlite rowids start at 1.
    pub(crate) fn first_id(self) -> i64 {
        match self {
            Self::File => 0,
            #[cfg(feature = "_sqlite")]
            Self::Sqlite => 1,
        }
    }

    /// Opens the same files as [`Self::in_dir`], through the read-only constructors.
    pub(crate) fn read_only_in_dir(self, dir: &std::path::Path) -> Box<dyn History> {
        match self {
            Self::File => Box::new(
                FileBackedHistory::with_file_read_only(100, dir.join("history.txt")).unwrap(),
            ),
            #[cfg(feature = "_sqlite")]
            Self::Sqlite => Box::new(
                SqliteBackedHistory::with_file_read_only(dir.join("history.db"), None, None)
                    .unwrap(),
            ),
        }
    }

    pub(crate) fn in_dir(self, dir: &std::path::Path) -> Box<dyn History> {
        match self {
            Self::File => {
                Box::new(FileBackedHistory::with_file(100, dir.join("history.txt")).unwrap())
            }
            #[cfg(feature = "_sqlite")]
            Self::Sqlite => Box::new(
                SqliteBackedHistory::with_file(dir.join("history.db"), None, None).unwrap(),
            ),
        }
    }
}
