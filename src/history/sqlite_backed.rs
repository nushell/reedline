use super::{
    base::{HistoryAppender, HistoryView},
    Database, HistoryItem, HistoryNavigationQuery, Sqlite,
};
use crate::{line_buffer::LineBuffer, History};
use async_std::task::block_on;
use std::{collections::VecDeque, path::Path};

// most of the sqlitebackedhistory items are just hacks to emulate
// the filebackedhistory. the traits may need to be changed to be
// a little more flexible with types like returning results and
// returning HistoryItem struct instead of just a string
// also, all the database is async/await, which is beyond me, but
// if someone wants to change that, feel free. i had to hack on
// these changes to get it to compiled. this sqlitebackedhistory
// has not been tested but you want to play with the self-containted
// prototype, it's here https://github.com/fdncred/hiztery
pub struct SqliteBackedHistory {
    sqlite: Sqlite,
    entries: VecDeque<HistoryItem>,
    commands: VecDeque<String>,
    query: HistoryNavigationQuery,
    cursor: usize,
}

impl SqliteBackedHistory {
    async fn new(path: &Path) -> Result<SqliteBackedHistory, sqlx::Error> {
        match Sqlite::new(path).await {
            Ok(s) => {
                // Assume the db file exists now and load the entries
                // I don't really like this but it emulates a history file
                // so, let's run with it and change it later
                let result = s.query_history("select * from history_items");
                let r = block_on(result).unwrap();
                let mut commands = VecDeque::new();
                for x in r.iter() {
                    commands.push_back(x.command.clone())
                }

                Ok(SqliteBackedHistory {
                    sqlite: s,
                    entries: VecDeque::from(r),
                    commands,
                    cursor: 0,
                    query: HistoryNavigationQuery::Normal(LineBuffer::default()),
                })
            }
            Err(e) => return Err(e),
        }
    }

    fn back_with_criteria(&mut self, criteria: &dyn Fn(&str) -> bool) {
        if !self.commands.is_empty() {
            let previous_match = self.commands.get(self.cursor);
            if let Some((next_cursor, _)) = self
                .commands
                .iter()
                .take(self.cursor)
                .enumerate()
                .rev()
                .find(|(_, entry)| criteria(entry) && previous_match != Some(entry))
            {
                // set to entry
                self.cursor = next_cursor
            }
        }
    }

    fn forward_with_criteria(&mut self, criteria: &dyn Fn(&str) -> bool) {
        let previous_match = self.commands.get(self.cursor);
        if let Some((next_cursor, _)) = self
            .commands
            .iter()
            .enumerate()
            .skip(self.cursor + 1)
            .find(|(_, entry)| criteria(entry) && previous_match != Some(entry))
        {
            // set to entry
            self.cursor = next_cursor
        } else {
            self.reset_cursor()
        }
    }

    /// Reset the internal browsing cursor
    fn reset_cursor(&mut self) {
        self.cursor = self.entries.len();
    }
}

impl Default for SqliteBackedHistory {
    // probably shouldn't use this - new() is the way to go
    fn default() -> SqliteBackedHistory {
        SqliteBackedHistory {
            sqlite: Sqlite::default(),
            entries: VecDeque::new(),
            commands: VecDeque::new(),
            cursor: 0,
            query: HistoryNavigationQuery::Normal(LineBuffer::default()),
        }
    }
}

impl History for SqliteBackedHistory {}

impl HistoryAppender for SqliteBackedHistory {
    // why can't we return a Result
    // I also need the cwd, duration, exit_status, and run_count
    fn append(&mut self, entry: String) {
        let hi = HistoryItem::new(
            None,
            entry,
            "cwd".to_string(),
            0,
            0,
            Some(self.sqlite.pid),
            chrono::Utc::now(),
        );
        let result = self.sqlite.save(&hi);
        block_on(result);
    }

    fn iter_chronologic(&self) -> std::collections::vec_deque::Iter<'_, String> {
        // Why are we forced to return a vec_deque
        // I'd rather return a HistoryItem here and not just a String
        self.commands.iter()
    }
}

impl HistoryView for SqliteBackedHistory {
    fn back(&mut self) {
        match self.query.clone() {
            HistoryNavigationQuery::Normal(_) => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                self.back_with_criteria(&|entry| entry.starts_with(&prefix))
            }
            HistoryNavigationQuery::SubstringSearch(substring) => {
                self.back_with_criteria(&|entry| entry.contains(&substring))
            }
        }
    }

    fn forward(&mut self) {
        match self.query.clone() {
            HistoryNavigationQuery::Normal(_) => {
                if self.cursor < self.entries.len() {
                    self.cursor += 1;
                }
            }
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                self.forward_with_criteria(&|entry| entry.starts_with(&prefix))
            }
            HistoryNavigationQuery::SubstringSearch(substring) => {
                self.forward_with_criteria(&|entry| entry.contains(&substring))
            }
        }
    }

    fn string_at_cursor(&self) -> Option<String> {
        self.commands.get(self.cursor).cloned()
    }

    fn set_navigation(&mut self, navigation: HistoryNavigationQuery) {
        self.query = navigation;
        self.reset_cursor();
    }

    fn get_navigation(&self) -> HistoryNavigationQuery {
        self.query.clone()
    }
}
