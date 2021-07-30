use chrono::Utc;
use core::hash::{Hash, Hasher};
use std::process;

#[derive(Debug, Clone, Ord, PartialOrd, sqlx::FromRow)]
pub struct HistoryItem {
    pub history_id: Option<i64>,
    pub command: String,
    pub cwd: String,
    pub duration: i64,
    pub exit_status: i64,
    pub session_id: i64,
    pub timestamp: chrono::DateTime<Utc>,
}

impl HistoryItem {
    pub fn new(
        history_id: Option<i64>,
        command: String,
        cwd: String,
        duration: i64,
        exit_status: i64,
        session_id: Option<i64>,
        timestamp: chrono::DateTime<Utc>,
    ) -> Self {
        let session_id = session_id.unwrap_or_else(|| process::id().into());

        Self {
            history_id,
            command,
            cwd,
            duration,
            exit_status,
            session_id,
            timestamp,
        }
    }
}

impl PartialEq for HistoryItem {
    // for the sakes of listing unique history only, we do not care about
    // anything else
    // obviously this does not refer to the *same* item of history, but when
    // we only render the command, it looks the same
    fn eq(&self, other: &Self) -> bool {
        self.command == other.command
    }
}

impl Eq for HistoryItem {}

impl Hash for HistoryItem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.command.hash(state);
    }
}
