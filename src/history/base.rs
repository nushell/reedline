use super::HistoryItemId;
use crate::{core_editor::LineBuffer, HistoryItem, HistorySessionId, Result};
use chrono::Utc;

/// Browsing modes for a [`History`]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryNavigationQuery {
    /// `bash` style browsing through the history. Contained `LineBuffer` is used to store the state of manual entry before browsing through the history
    Normal(LineBuffer),
    /// Search for entries starting with a particular string.
    PrefixSearch(String),
    /// Full exact search for all entries containing a string.
    SubstringSearch(String),
    // Suffix Search
    // Fuzzy Search
}

/// Ways to search for a particular command line in the [`History`]
// todo: merge with [HistoryNavigationQuery]
pub enum CommandLineSearch {
    /// Command line starts with the same string
    Prefix(String),
    /// Command line contains the string
    Substring(String),
    /// Command line is the string.
    ///
    /// Useful to gather statistics
    Exact(String),
}

/// Defines how to traverse the history when executing a [`SearchQuery`]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    /// From the most recent entry backward
    Backward,
    /// From the least recent entry forward
    Forward,
}

/// Defines additional filters for querying the [`History`]
pub struct SearchFilter {
    /// Query for the command line content
    pub command_line: Option<CommandLineSearch>,
    /// Considered implementation detail for now
    pub(crate) not_command_line: Option<String>, // to skip the currently shown value in up-arrow navigation
    /// Filter based on the executing systems hostname
    pub hostname: Option<String>,
    /// Exact filter for the working directory
    pub cwd_exact: Option<String>,
    /// Prefix filter for the working directory
    pub cwd_prefix: Option<String>,
    /// Filter whether the command completed
    pub exit_successful: Option<bool>,
    /// Filter on the session id
    pub session: Option<HistorySessionId>,
}

impl SearchFilter {
    /// Create a search filter with a [`CommandLineSearch`]
    pub fn from_text_search(
        cmd: CommandLineSearch,
        session: Option<HistorySessionId>,
    ) -> SearchFilter {
        let mut s = SearchFilter::anything(session);
        s.command_line = Some(cmd);
        s
    }

    /// Create a search filter with a [`CommandLineSearch`] and `cwd`
    pub fn from_text_search_cwd(
        cwd: String,
        cmd: CommandLineSearch,
        session: Option<HistorySessionId>,
    ) -> SearchFilter {
        let mut s = SearchFilter::anything(session);
        s.command_line = Some(cmd);
        s.cwd_exact = Some(cwd);
        s
    }

    /// anything within this session
    pub fn anything(session: Option<HistorySessionId>) -> SearchFilter {
        SearchFilter {
            command_line: None,
            not_command_line: None,
            hostname: None,
            cwd_exact: None,
            cwd_prefix: None,
            exit_successful: None,
            session,
        }
    }
}

/// Query for search in the potentially rich [`History`]
pub struct SearchQuery {
    /// Direction to search in
    pub direction: SearchDirection,
    /// if given, only get results after/before this time (depending on direction)
    pub start_time: Option<chrono::DateTime<Utc>>,
    /// if given, only get results after/before this time (depending on direction)
    pub end_time: Option<chrono::DateTime<Utc>>,
    /// if given, only get results after/before this id (depending on direction)
    pub start_id: Option<HistoryItemId>,
    /// if given, only get results after/before this id (depending on direction)
    pub end_id: Option<HistoryItemId>,
    /// How many results to get
    pub limit: Option<i64>,
    /// Additional filters defined with [`SearchFilter`]
    pub filter: SearchFilter,
}

/// Currently `pub` ways to construct a query
impl SearchQuery {
    /// all that contain string in reverse chronological order
    pub fn all_that_contain_rev(contains: String) -> SearchQuery {
        SearchQuery {
            direction: SearchDirection::Backward,
            start_time: None,
            end_time: None,
            start_id: None,
            end_id: None,
            limit: None,
            filter: SearchFilter::from_text_search(CommandLineSearch::Substring(contains), None),
        }
    }

    /// Get the most recent entry matching [`SearchFilter`]
    pub const fn last_with_search(filter: SearchFilter) -> SearchQuery {
        SearchQuery {
            direction: SearchDirection::Backward,
            start_time: None,
            end_time: None,
            start_id: None,
            end_id: None,
            limit: Some(1),
            filter,
        }
    }

    /// Get the most recent entry starting with the `prefix`
    pub fn last_with_prefix(prefix: String, session: Option<HistorySessionId>) -> SearchQuery {
        SearchQuery::last_with_search(SearchFilter::from_text_search(
            CommandLineSearch::Prefix(prefix),
            session,
        ))
    }

    /// Get the most recent entry starting with the `prefix` and `cwd`
    pub fn last_with_prefix_and_cwd(
        prefix: String,
        cwd: String,
        session: Option<HistorySessionId>,
    ) -> SearchQuery {
        let prefix = CommandLineSearch::Prefix(prefix);
        SearchQuery::last_with_search(SearchFilter::from_text_search_cwd(cwd, prefix, session))
    }

    /// Query to get all entries in the given [`SearchDirection`]
    pub fn everything(
        direction: SearchDirection,
        session: Option<HistorySessionId>,
    ) -> SearchQuery {
        SearchQuery {
            direction,
            start_time: None,
            end_time: None,
            start_id: None,
            end_id: None,
            limit: None,
            filter: SearchFilter::anything(session),
        }
    }
}

/// Represents a history file or database
/// Data could be stored e.g. in a plain text file, in a `JSONL` file, in a `SQLite` database
pub trait History: Send {
    /// save a history item to the database
    /// if given id is None, a new id is created and set in the return value
    /// if given id is Some, the existing entry is updated
    fn save(&mut self, h: HistoryItem) -> Result<HistoryItem>;
    /// load a history item by its id
    fn load(&self, id: HistoryItemId) -> Result<HistoryItem>;

    /// count the results of a query
    fn count(&self, query: SearchQuery) -> Result<i64>;
    /// return the total number of history items
    fn count_all(&self) -> Result<i64> {
        self.count(SearchQuery::everything(SearchDirection::Forward, None))
    }
    /// return the results of a query
    fn search(&self, query: SearchQuery) -> Result<Vec<HistoryItem>>;

    /// update an item atomically
    fn update(
        &mut self,
        id: HistoryItemId,
        updater: &dyn Fn(HistoryItem) -> HistoryItem,
    ) -> Result<()>;
    /// delete all history items
    fn clear(&mut self) -> Result<()>;
    /// remove an item from this history
    fn delete(&mut self, h: HistoryItemId) -> Result<()>;
    /// ensure that this history is written to disk
    fn sync(&mut self) -> std::io::Result<()>;
    /// get the history session id
    fn session(&self) -> Option<HistorySessionId>;
}

#[cfg(test)]
mod test {
    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    const IS_FILE_BASED: bool = false;
    #[cfg(not(any(feature = "sqlite", feature = "sqlite-dynlib")))]
    const IS_FILE_BASED: bool = true;

    use crate::HistorySessionId;

    fn create_item(session: i64, cwd: &str, cmd: &str, exit_status: i64) -> HistoryItem {
        HistoryItem {
            id: None,
            start_timestamp: None,
            command_line: cmd.to_string(),
            session_id: Some(HistorySessionId::new(session)),
            hostname: Some("foohost".to_string()),
            cwd: Some(cwd.to_string()),
            duration: Some(Duration::from_millis(1000)),
            exit_status: Some(exit_status),
            more_info: None,
        }
    }
    use std::time::Duration;

    use super::*;
    fn create_filled_example_history() -> Result<Box<dyn History>> {
        #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
        let mut history = crate::SqliteBackedHistory::in_memory()?;
        #[cfg(not(any(feature = "sqlite", feature = "sqlite-dynlib")))]
        let mut history = crate::FileBackedHistory::default();
        #[cfg(not(any(feature = "sqlite", feature = "sqlite-dynlib")))]
        history.save(create_item(1, "/", "dummy", 0))?; // add dummy item so ids start with 1
        history.save(create_item(1, "/home/me", "cd ~/Downloads", 0))?; // 1
        history.save(create_item(1, "/home/me/Downloads", "unzp foo.zip", 1))?; // 2
        history.save(create_item(1, "/home/me/Downloads", "unzip foo.zip", 0))?; // 3
        history.save(create_item(1, "/home/me/Downloads", "cd foo", 0))?; // 4
        history.save(create_item(1, "/home/me/Downloads/foo", "ls", 0))?; // 5
        history.save(create_item(1, "/home/me/Downloads/foo", "ls -alh", 0))?; // 6
        history.save(create_item(1, "/home/me/Downloads/foo", "cat x.txt", 0))?; // 7

        history.save(create_item(1, "/home/me", "cd /etc/nginx", 0))?; // 8
        history.save(create_item(1, "/etc/nginx", "ls -l", 0))?; // 9
        history.save(create_item(1, "/etc/nginx", "vim nginx.conf", 0))?; // 10
        history.save(create_item(1, "/etc/nginx", "vim htpasswd", 0))?; // 11
        history.save(create_item(1, "/etc/nginx", "cat nginx.conf", 0))?; // 12
        Ok(Box::new(history))
    }

    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    #[test]
    fn update_item() -> Result<()> {
        let mut history = create_filled_example_history()?;
        let id = HistoryItemId::new(2);
        let before = history.load(id)?;
        history.update(id, &|mut e| {
            e.exit_status = Some(1);
            e
        })?;
        let after = history.load(id)?;
        assert_eq!(
            after,
            HistoryItem {
                exit_status: Some(1),
                ..before
            }
        );
        Ok(())
    }

    fn search_returned(
        history: &dyn History,
        res: Vec<HistoryItem>,
        wanted: Vec<i64>,
    ) -> Result<()> {
        let wanted = wanted
            .iter()
            .map(|id| history.load(HistoryItemId::new(*id)))
            .collect::<Result<Vec<HistoryItem>>>()?;
        assert_eq!(res, wanted);
        Ok(())
    }

    #[test]
    fn count_all() -> Result<()> {
        let history = create_filled_example_history()?;
        println!(
            "{:#?}",
            history.search(SearchQuery::everything(SearchDirection::Forward, None))
        );

        assert_eq!(history.count_all()?, if IS_FILE_BASED { 13 } else { 12 });
        Ok(())
    }

    #[test]
    fn get_latest() -> Result<()> {
        let history = create_filled_example_history()?;
        let res = history.search(SearchQuery::last_with_search(SearchFilter::anything(None)))?;

        search_returned(&*history, res, vec![12])?;
        Ok(())
    }

    #[test]
    fn get_earliest() -> Result<()> {
        let history = create_filled_example_history()?;
        let res = history.search(SearchQuery {
            limit: Some(1),
            ..SearchQuery::everything(SearchDirection::Forward, None)
        })?;
        search_returned(&*history, res, vec![if IS_FILE_BASED { 0 } else { 1 }])?;
        Ok(())
    }

    #[test]
    fn search_prefix() -> Result<()> {
        let history = create_filled_example_history()?;
        let res = history.search(SearchQuery {
            filter: SearchFilter::from_text_search(
                CommandLineSearch::Prefix("ls ".to_string()),
                None,
            ),
            ..SearchQuery::everything(SearchDirection::Backward, None)
        })?;
        search_returned(&*history, res, vec![9, 6])?;

        Ok(())
    }

    #[test]
    fn search_prefix_is_case_sensitive() -> Result<()> {
        // Basic prefix search should preserve case
        //
        // https://github.com/nushell/nushell/issues/10131
        let history = create_filled_example_history()?;
        let res = history.search(SearchQuery {
            filter: SearchFilter::from_text_search(
                CommandLineSearch::Prefix("LS ".to_string()),
                None,
            ),
            ..SearchQuery::everything(SearchDirection::Backward, None)
        })?;
        search_returned(&*history, res, vec![])?;

        Ok(())
    }

    #[test]
    fn search_includes() -> Result<()> {
        let history = create_filled_example_history()?;
        let res = history.search(SearchQuery {
            filter: SearchFilter::from_text_search(
                CommandLineSearch::Substring("foo.zip".to_string()),
                None,
            ),
            ..SearchQuery::everything(SearchDirection::Forward, None)
        })?;
        search_returned(&*history, res, vec![2, 3])?;
        Ok(())
    }

    #[test]
    fn search_includes_limit() -> Result<()> {
        let history = create_filled_example_history()?;
        let res = history.search(SearchQuery {
            filter: SearchFilter::from_text_search(
                CommandLineSearch::Substring("c".to_string()),
                None,
            ),
            limit: Some(2),
            ..SearchQuery::everything(SearchDirection::Forward, None)
        })?;
        search_returned(&*history, res, vec![1, 4])?;

        Ok(())
    }

    #[test]
    fn clear_history() -> Result<()> {
        let mut history = create_filled_example_history()?;
        assert_ne!(history.count_all()?, 0);
        history.clear().unwrap();
        assert_eq!(history.count_all()?, 0);

        Ok(())
    }

    // test that clear() works as expected across multiple instances of History
    #[test]
    fn clear_history_with_backing_file() -> Result<()> {
        #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
        fn open_history() -> Box<dyn History> {
            Box::new(
                crate::SqliteBackedHistory::with_file("target/test-history.db".into(), None, None)
                    .unwrap(),
            )
        }

        #[cfg(not(any(feature = "sqlite", feature = "sqlite-dynlib")))]
        fn open_history() -> Box<dyn History> {
            Box::new(
                crate::FileBackedHistory::with_file(100, "target/test-history.txt".into()).unwrap(),
            )
        }

        // create history, add a few entries
        let mut history = open_history();
        history.save(create_item(1, "/home/me", "cd ~/Downloads", 0))?; // 1
        history.save(create_item(1, "/home/me/Downloads", "unzp foo.zip", 1))?; // 2
        assert_eq!(history.count_all()?, 2);
        drop(history);

        // open it again and clear it
        let mut history = open_history();
        assert_eq!(history.count_all()?, 2);
        history.clear().unwrap();
        assert_eq!(history.count_all()?, 0);
        drop(history);

        // open it once more and confirm that the cleared data is gone forever
        let history = open_history();
        assert_eq!(history.count_all()?, 0);

        Ok(())
    }

    #[cfg(not(any(feature = "sqlite", feature = "sqlite-dynlib")))]
    #[test]
    fn history_size_zero() -> Result<()> {
        let mut history = crate::FileBackedHistory::new(0)?;
        history.save(create_item(1, "/home/me", "cd ~/Downloads", 0))?;
        assert_eq!(history.count_all()?, 0);
        let _ = history.sync();
        history.clear()?;
        drop(history);

        Ok(())
    }

    #[test]
    fn create_file_backed_history() {
        use crate::HISTORY_SIZE;

        assert!(crate::FileBackedHistory::new(usize::MAX).is_err());
        assert!(crate::FileBackedHistory::new(HISTORY_SIZE).is_ok());
    }
}
