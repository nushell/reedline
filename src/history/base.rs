use std::time::Duration;

use chrono::Utc;
use serde::{de::DeserializeOwned, Serialize};

use crate::core_editor::LineBuffer;

// todo: better error type
pub type Result<T> = std::result::Result<T, String>;

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

// todo: merge with [HistoryNavigationQuery]
pub enum CommandLineSearch {
    Prefix(String),
    Substring(String),
    Exact(String),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct HistoryItemId(pub(crate) i64);
impl HistoryItemId {
    pub(crate) fn new(i: i64) -> HistoryItemId {
        HistoryItemId(i)
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct HistorySessionId(pub(crate) i64);
impl HistorySessionId {
    pub(crate) fn new(i: i64) -> HistorySessionId {
        HistorySessionId(i)
    }
}
/// This trait represents additional context to be added to a history (see [HistoryItem])
pub trait HistoryItemExtraInfo: Serialize + DeserializeOwned + Default + Send {}
impl HistoryItemExtraInfo for () {}
/// Represents one run command with some optional historical context
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryItem<ExtraInfo: HistoryItemExtraInfo = ()> {
    /// primary key, unique across one history
    pub id: Option<HistoryItemId>,
    /// date-time when this command was started
    pub start_timestamp: Option<chrono::DateTime<Utc>>,
    /// the full command line as text
    pub command_line: String,
    /// a unique id for one shell session.
    /// used so the history can be filtered to a single session
    pub session_id: Option<HistorySessionId>,
    /// the hostname the commands were run in
    pub hostname: Option<String>,
    /// the current working directory
    pub cwd: Option<String>,
    /// the duration the command took to complete
    pub duration: Option<Duration>,
    /// the exit status of the command
    pub exit_status: Option<i64>,
    /// arbitrary additional information that might be interesting
    pub more_info: Option<ExtraInfo>,
}

impl HistoryItem {
    /// create a history item purely from the command line with everything else set to None
    pub fn from_command_line(cmd: impl Into<String>) -> HistoryItem {
        HistoryItem {
            id: None,
            start_timestamp: None,
            command_line: cmd.into(),
            session_id: None,
            hostname: None,
            cwd: None,
            duration: None,
            exit_status: None,
            more_info: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Backward,
    Forward,
}

pub struct SearchFilter {
    pub command_line: Option<CommandLineSearch>,
    pub not_command_line: Option<String>, // to skip duplicates
    pub hostname: Option<String>,
    pub cwd_exact: Option<String>,
    pub cwd_prefix: Option<String>,
    pub exit_successful: Option<bool>,
}
impl SearchFilter {
    pub fn from_text_search(cmd: CommandLineSearch) -> SearchFilter {
        let mut s = SearchFilter::anything();
        s.command_line = Some(cmd);
        s
    }
    pub fn anything() -> SearchFilter {
        SearchFilter {
            command_line: None,
            not_command_line: None,
            hostname: None,
            cwd_exact: None,
            cwd_prefix: None,
            exit_successful: None,
        }
    }
}

pub struct SearchQuery {
    pub direction: SearchDirection,
    /// if given, only get results after/before this time (depending on direction)
    pub start_time: Option<chrono::DateTime<Utc>>,
    pub end_time: Option<chrono::DateTime<Utc>>,
    /// if given, only get results after/before this id (depending on direction)
    pub start_id: Option<HistoryItemId>,
    pub end_id: Option<HistoryItemId>,
    pub limit: Option<i64>,
    pub filter: SearchFilter,
}
/// some utility functions
impl SearchQuery {
    /// all that contain string in reverse chronological order
    pub fn all_that_contain_rev(contains: String) -> SearchQuery {
        return SearchQuery {
            direction: SearchDirection::Backward,
            start_time: None,
            end_time: None,
            start_id: None,
            end_id: None,
            limit: None,
            filter: SearchFilter::from_text_search(CommandLineSearch::Substring(contains)),
        };
    }
    pub fn last_with_search(filter: SearchFilter) -> SearchQuery {
        return SearchQuery {
            direction: SearchDirection::Backward,
            start_time: None,
            end_time: None,
            start_id: None,
            end_id: None,
            limit: Some(1),
            filter,
        };
    }
    pub fn last_with_prefix(prefix: String) -> SearchQuery {
        SearchQuery::last_with_search(SearchFilter::from_text_search(CommandLineSearch::Prefix(
            prefix,
        )))
    }
    pub fn everything(direction: SearchDirection) -> SearchQuery {
        SearchQuery {
            direction,
            start_time: None,
            end_time: None,
            start_id: None,
            end_id: None,
            limit: None,
            filter: SearchFilter::anything(),
        }
    }
}

/// Represents a history file or database
/// Data could be stored e.g. in a plain text file, in a JSONL file, in a SQLite database
pub trait History: Send {
    /// save a history item to the database
    /// if given id is None, a new id is created and set in the return value
    /// if given id is Some, the existing entry is updated
    fn save(&mut self, h: HistoryItem) -> Result<HistoryItem>;
    /// load a history item by its id
    fn load(&self, id: HistoryItemId) -> Result<HistoryItem>;
    /// gets the newest item id in this history
    fn newest(&self) -> Result<Option<HistoryItemId>> {
        let res = self.search(SearchQuery::last_with_search(SearchFilter::anything()))?;
        Ok(res.get(0).and_then(|e| e.id))
    }

    /// creates a new unique session id
    fn new_session_id(&mut self) -> Result<HistorySessionId>;

    /// count the results of a query
    fn count(&self, query: SearchQuery) -> Result<i64>;
    /// return the total number of history items
    fn count_all(&self) -> Result<i64> {
        self.count(SearchQuery::everything(SearchDirection::Forward))
    }
    /// return the results of a query
    fn search(&self, query: SearchQuery) -> Result<Vec<HistoryItem>>;

    /// update an item atomically
    fn update(
        &mut self,
        id: HistoryItemId,
        updater: &dyn Fn(HistoryItem) -> HistoryItem,
    ) -> Result<()>;
    /// remove an item from this history
    fn delete(&mut self, h: HistoryItemId) -> Result<()>;
    /// ensure that this history is written to disk
    fn sync(&mut self) -> std::io::Result<()>;
}

#[cfg(test)]
mod test {
    #[cfg(feature = "sqlite")]
    const IS_FILE_BASED: bool = false;
    #[cfg(not(feature = "sqlite"))]
    const IS_FILE_BASED: bool = true;

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
    use super::*;
    fn create_filled_example_history() -> Result<Box<dyn History>> {
        #[cfg(feature = "sqlite")]
        let mut history = crate::SqliteBackedHistory::in_memory()?;
        #[cfg(not(feature = "sqlite"))]
        let mut history = crate::FileBackedHistory::default();
        #[cfg(not(feature = "sqlite"))]
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

    fn search_returned(
        history: &dyn History,
        res: Vec<HistoryItem>,
        wanted: Vec<i64>,
    ) -> Result<()> {
        let wanted = wanted
            .iter()
            .map(|id| Ok(history.load(HistoryItemId::new(*id))?))
            .collect::<Result<Vec<HistoryItem>>>()?;
        assert_eq!(res, wanted);
        Ok(())
    }

    #[test]
    fn count_all() -> Result<()> {
        let history = create_filled_example_history()?;
        println!(
            "{:#?}",
            history.search(SearchQuery::everything(SearchDirection::Forward))
        );

        assert_eq!(history.count_all()?, if IS_FILE_BASED { 13 } else { 12 });
        Ok(())
    }

    #[test]
    fn get_latest() -> Result<()> {
        let history = create_filled_example_history()?;
        let res = history.search(SearchQuery::last_with_search(SearchFilter::anything()))?;

        search_returned(&*history, res, vec![12])?;
        Ok(())
    }

    #[test]
    fn get_earliest() -> Result<()> {
        let history = create_filled_example_history()?;
        let res = history.search(SearchQuery {
            limit: Some(1),
            ..SearchQuery::everything(SearchDirection::Forward)
        })?;
        search_returned(&*history, res, vec![if IS_FILE_BASED { 0 } else { 1 }])?;
        Ok(())
    }

    #[test]
    fn search_prefix() -> Result<()> {
        let history = create_filled_example_history()?;
        let res = history.search(SearchQuery {
            filter: SearchFilter::from_text_search(CommandLineSearch::Prefix("ls ".to_string())),
            ..SearchQuery::everything(SearchDirection::Backward)
        })?;
        search_returned(&*history, res, vec![9, 6])?;

        Ok(())
    }

    #[test]
    fn search_includes() -> Result<()> {
        let history = create_filled_example_history()?;
        let res = history.search(SearchQuery {
            filter: SearchFilter::from_text_search(CommandLineSearch::Substring(
                "foo.zip".to_string(),
            )),
            ..SearchQuery::everything(SearchDirection::Forward)
        })?;
        search_returned(&*history, res, vec![2, 3])?;
        Ok(())
    }

    #[test]
    fn search_includes_limit() -> Result<()> {
        let history = create_filled_example_history()?;
        let res = history.search(SearchQuery {
            filter: SearchFilter::from_text_search(CommandLineSearch::Substring("c".to_string())),
            limit: Some(2),
            ..SearchQuery::everything(SearchDirection::Forward)
        })?;
        search_returned(&*history, res, vec![1, 4])?;

        Ok(())
    }
}
