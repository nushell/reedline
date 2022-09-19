use chrono::Utc;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{fmt::Display, time::Duration};

/// Unique ID for the [`HistoryItem`]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct HistoryItemId(pub(crate) i64);
impl HistoryItemId {
    pub(crate) fn new(i: i64) -> HistoryItemId {
        HistoryItemId(i)
    }
}

impl Display for HistoryItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique ID for the session in which reedline was run to disambiguate different sessions
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct HistorySessionId(pub(crate) i64);
impl HistorySessionId {
    pub(crate) fn new(i: i64) -> HistorySessionId {
        HistorySessionId(i)
    }
}

impl Display for HistorySessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<HistorySessionId> for i64 {
    fn from(id: HistorySessionId) -> Self {
        id.0
    }
}

/// This trait represents additional arbitrary context to be added to a history (optional, see [`HistoryItem`])
pub trait HistoryItemExtraInfo: Serialize + DeserializeOwned + Default + Send {}

#[derive(Default, Debug, PartialEq, Eq)]
/// something that is serialized as null and deserialized by ignoring everything
pub struct IgnoreAllExtraInfo;

impl Serialize for IgnoreAllExtraInfo {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        Option::<IgnoreAllExtraInfo>::None.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for IgnoreAllExtraInfo {
    fn deserialize<D>(d: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        serde::de::IgnoredAny::deserialize(d).map(|_| IgnoreAllExtraInfo)
    }
}

impl HistoryItemExtraInfo for IgnoreAllExtraInfo {}

/// Represents one run command with some optional additional context
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryItem<ExtraInfo: HistoryItemExtraInfo = IgnoreAllExtraInfo> {
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
