use super::{
    base::{CommandLineSearch, SearchDirection, SearchQuery},
    History, HistoryItem, HistoryItemExtraInfo, HistoryItemId, HistorySessionId,
    IgnoreAllExtraInfo,
};
use crate::{
    result::{ReedlineError, ReedlineErrorVariants},
    Result,
};
use chrono::{TimeZone, Utc};
use rusqlite::{named_params, params, Connection, ToSql};
use std::{fmt::Write, path::PathBuf, time::Duration};
const SQLITE_APPLICATION_ID: i32 = 1151497937;

/// A history that stores the values to an SQLite database.
/// In addition to storing the command, the history can store an additional arbitrary HistoryEntryContext,
/// to add information such as a timestamp, running directory, result...
///
/// ## Required feature:
/// `sqlite` or `sqlite-dynlib`
pub struct SqliteBackedHistory {
    db: rusqlite::Connection,
    session: Option<HistorySessionId>,
    session_timestamp: Option<chrono::DateTime<Utc>>,
}

fn deserialize_history_item<E: HistoryItemExtraInfo>(
    row: &rusqlite::Row,
) -> rusqlite::Result<HistoryItem<E>> {
    let x: Option<String> = row.get("more_info")?;
    Ok(HistoryItem {
        id: Some(HistoryItemId::new(row.get("id")?)),
        start_timestamp: row.get::<&str, Option<i64>>("start_timestamp")?.map(|e| {
            match Utc.timestamp_millis_opt(e) {
                chrono::LocalResult::Single(e) => e,
                _ => chrono::Utc::now(),
            }
        }),
        command_line: row.get("command_line")?,
        session_id: row
            .get::<&str, Option<i64>>("session_id")?
            .map(HistorySessionId::new),
        hostname: row.get("hostname")?,
        cwd: row.get("cwd")?,
        duration: row
            .get::<&str, Option<i64>>("duration_ms")?
            .map(|e| Duration::from_millis(e as u64)),
        exit_status: row.get("exit_status")?,
        more_info: x
            .map(|x| {
                serde_json::from_str::<E>(&x).map_err(|e| {
                    // hack
                    rusqlite::Error::InvalidColumnType(
                        0,
                        format!("could not deserialize more_info: {e}"),
                        rusqlite::types::Type::Text,
                    )
                })
            })
            .transpose()?,
    })
}

impl History for SqliteBackedHistory {
    fn save(&mut self, entry: HistoryItem) -> Result<HistoryItem> {
        self.save_impl(entry)
    }

    fn load(&self, id: HistoryItemId) -> Result<HistoryItem> {
        let entry = self
            .db
            .prepare("select * from history where id = :id")
            .map_err(map_sqlite_err)?
            .query_row(
                named_params! { ":id": id.0 },
                deserialize_history_item::<IgnoreAllExtraInfo>,
            )
            .map_err(map_sqlite_err)?;
        Ok(entry)
    }

    fn count(&self, query: SearchQuery) -> Result<i64> {
        let (query, params) = self.construct_query(&query, "coalesce(count(*), 0)");
        let params_borrow: Vec<(&str, &dyn ToSql)> =
            params.iter().map(|e| (e.0.as_str(), &*e.1)).collect();
        let result: i64 = self
            .db
            .prepare(&query)
            .unwrap()
            .query_row(&params_borrow[..], |r| r.get(0))
            .map_err(map_sqlite_err)?;
        Ok(result)
    }

    fn search(&self, query: SearchQuery) -> Result<Vec<HistoryItem>> {
        let (query, params) = self.construct_query(&query, "*");
        let params_borrow: Vec<(&str, &dyn ToSql)> =
            params.iter().map(|e| (e.0.as_str(), &*e.1)).collect();
        let results: Vec<HistoryItem> = self
            .db
            .prepare(&query)
            .unwrap()
            .query_map(
                &params_borrow[..],
                deserialize_history_item::<IgnoreAllExtraInfo>,
            )
            .map_err(map_sqlite_err)?
            .collect::<rusqlite::Result<Vec<HistoryItem>>>()
            .map_err(map_sqlite_err)?;
        Ok(results)
    }

    fn update(
        &mut self,
        id: HistoryItemId,
        updater: &dyn Fn(HistoryItem) -> HistoryItem,
    ) -> Result<()> {
        // in theory this should run in a transaction
        let item = self.load(id)?;
        self.save(updater(item))?;
        Ok(())
    }

    fn clear(&mut self) -> Result<()> {
        self.db
            .execute("delete from history", params![])
            .map_err(map_sqlite_err)?;

        // VACUUM to ensure that sensitive data is completely erased
        // instead of being marked as available for reuse
        self.db
            .execute("VACUUM", params![])
            .map_err(map_sqlite_err)?;

        Ok(())
    }

    fn delete(&mut self, h: HistoryItemId) -> Result<()> {
        let changed = self
            .db
            .execute("delete from history where id = ?", params![h.0])
            .map_err(map_sqlite_err)?;
        if changed == 0 {
            return Err(ReedlineError(ReedlineErrorVariants::HistoryDatabaseError(
                "Could not find item".to_string(),
            )));
        }
        Ok(())
    }

    fn sync(&mut self) -> std::io::Result<()> {
        // no-op (todo?)
        Ok(())
    }

    fn session(&self) -> Option<HistorySessionId> {
        self.session
    }
}
fn map_sqlite_err(err: rusqlite::Error) -> ReedlineError {
    // TODO: better error mapping
    ReedlineError(ReedlineErrorVariants::HistoryDatabaseError(format!(
        "{err:?}"
    )))
}

fn map_json_err(err: serde_json::Error) -> ReedlineError {
    ReedlineError(ReedlineErrorVariants::HistoryDatabaseError(format!(
        "could not serialize more_info: {err}"
    )))
}

type BoxedNamedParams<'a> = Vec<(String, Box<dyn ToSql + 'a>)>;

impl SqliteBackedHistory {
    /// Creates a new history with an associated history file.
    ///
    ///
    /// **Side effects:** creates all nested directories to the file
    ///
    pub fn with_file(
        file: PathBuf,
        session: Option<HistorySessionId>,
        session_timestamp: Option<chrono::DateTime<Utc>>,
    ) -> Result<Self> {
        if let Some(base_dir) = file.parent() {
            std::fs::create_dir_all(base_dir).map_err(|e| {
                ReedlineError(ReedlineErrorVariants::HistoryDatabaseError(format!("{e}")))
            })?;
        }
        let db = Connection::open(&file).map_err(map_sqlite_err)?;
        Self::from_connection(db, session, session_timestamp)
    }
    /// Creates a new history in memory
    pub fn in_memory() -> Result<Self> {
        Self::from_connection(
            Connection::open_in_memory().map_err(map_sqlite_err)?,
            None,
            None,
        )
    }
    /// initialize a new database / migrate an existing one
    fn from_connection(
        db: Connection,
        session: Option<HistorySessionId>,
        session_timestamp: Option<chrono::DateTime<Utc>>,
    ) -> Result<Self> {
        // https://phiresky.github.io/blog/2020/sqlite-performance-tuning/
        db.pragma_update(None, "journal_mode", "wal")
            .map_err(map_sqlite_err)?;
        db.pragma_update(None, "synchronous", "normal")
            .map_err(map_sqlite_err)?;
        db.pragma_update(None, "mmap_size", "1000000000")
            .map_err(map_sqlite_err)?;
        db.pragma_update(None, "foreign_keys", "on")
            .map_err(map_sqlite_err)?;
        db.pragma_update(None, "application_id", SQLITE_APPLICATION_ID)
            .map_err(map_sqlite_err)?;
        let db_version: i32 = db
            .query_row(
                "SELECT user_version FROM pragma_user_version",
                params![],
                |r| r.get(0),
            )
            .map_err(map_sqlite_err)?;
        if db_version != 0 {
            return Err(ReedlineError(ReedlineErrorVariants::HistoryDatabaseError(
                format!("Unknown database version {db_version}"),
            )));
        }
        db.execute_batch(
            "
        create table if not exists history (
            id integer primary key autoincrement,
            command_line text not null,
            start_timestamp integer,
            session_id integer,
            hostname text,
            cwd text,
            duration_ms integer,
            exit_status integer,
            more_info text
        ) strict;
        create index if not exists idx_history_time on history(start_timestamp);
        create index if not exists idx_history_cwd on history(cwd); -- suboptimal for many hosts
        create index if not exists idx_history_exit_status on history(exit_status);
        create index if not exists idx_history_cmd on history(command_line);
        create index if not exists idx_history_cmd on history(session_id);
        -- todo: better indexes
        ",
        )
        .map_err(map_sqlite_err)?;
        Ok(SqliteBackedHistory {
            db,
            session,
            session_timestamp,
        })
    }

    fn construct_query<'a>(
        &self,
        query: &'a SearchQuery,
        select_expression: &str,
    ) -> (String, BoxedNamedParams<'a>) {
        // TODO: this whole function could be done with less allocs
        let (is_asc, asc) = match query.direction {
            SearchDirection::Forward => (true, "asc"),
            SearchDirection::Backward => (false, "desc"),
        };
        let mut wheres: Vec<&str> = Vec::new();
        let mut params: BoxedNamedParams = Vec::new();
        if let Some(start) = query.start_time {
            wheres.push(if is_asc {
                "timestamp_start > :start_time"
            } else {
                "timestamp_start < :start_time"
            });
            params.push((
                ":start_time".to_string(),
                Box::new(start.timestamp_millis()),
            ));
        }
        if let Some(end) = query.end_time {
            wheres.push(if is_asc {
                ":end_time >= timestamp_start"
            } else {
                ":end_time <= timestamp_start"
            });
            params.push((":end_time".to_string(), Box::new(end.timestamp_millis())));
        }
        if let Some(start) = query.start_id {
            wheres.push(if is_asc {
                "id > :start_id"
            } else {
                "id < :start_id"
            });
            params.push((":start_id".to_string(), Box::new(start.0)));
        }
        if let Some(end) = query.end_id {
            wheres.push(if is_asc {
                ":end_id >= id"
            } else {
                ":end_id <= id"
            });
            params.push((":end_id".to_string(), Box::new(end.0)));
        }
        let limit = match query.limit {
            Some(l) => {
                params.push((":limit".to_string(), Box::new(l)));
                "limit :limit"
            }
            None => "",
        };
        if let Some(command_line) = &query.filter.command_line {
            match command_line {
                CommandLineSearch::Exact(e) => {
                    wheres.push("command_line == :command_line");
                    params.push((":command_line".to_string(), Box::new(e)));
                }
                CommandLineSearch::Prefix(prefix) => {
                    wheres.push("instr(command_line, :command_line) == 1");
                    params.push((":command_line".to_string(), Box::new(prefix)));
                }
                CommandLineSearch::Substring(cont) => {
                    wheres.push("instr(command_line, :command_line) >= 1");
                    params.push((":command_line".to_string(), Box::new(cont)));
                }
            };
        }

        if let Some(str) = &query.filter.not_command_line {
            wheres.push("command_line != :not_cmd");
            params.push((":not_cmd".to_string(), Box::new(str)));
        }
        if let Some(hostname) = &query.filter.hostname {
            wheres.push("hostname = :hostname");
            params.push((":hostname".to_string(), Box::new(hostname)));
        }
        if let Some(cwd_exact) = &query.filter.cwd_exact {
            wheres.push("cwd = :cwd");
            params.push((":cwd".to_string(), Box::new(cwd_exact)));
        }
        if let Some(cwd_prefix) = &query.filter.cwd_prefix {
            wheres.push("cwd like :cwd_like");
            let cwd_like = format!("{cwd_prefix}%");
            params.push((":cwd_like".to_string(), Box::new(cwd_like)));
        }
        if let Some(exit_successful) = query.filter.exit_successful {
            if exit_successful {
                wheres.push("exit_status = 0");
            } else {
                wheres.push("exit_status != 0");
            }
        }
        if let (Some(session_id), Some(session_timestamp)) =
            (query.filter.session, self.session_timestamp)
        {
            // Filter so that we get rows:
            // - that have the same session_id, or
            // - were executed before our session started
            wheres.push("(session_id = :session_id OR start_timestamp < :session_timestamp)");
            params.push((":session_id".to_string(), Box::new(session_id)));
            params.push((
                ":session_timestamp".to_string(),
                Box::new(session_timestamp.timestamp_millis()),
            ));
        }

        // Build WHERE string, appending dynamic json_extract conditions last
        let mut where_string = wheres.join(" and ");
        if let Some(filters) = &query.filter.more_info_json {
            for (i, (path, value)) in filters.iter().enumerate() {
                if !where_string.is_empty() {
                    where_string.push_str(" and ");
                }
                // CAST to TEXT so that json_extract's type-dependent output (e.g. integer 1 for
                // JSON `true`) is consistently comparable to the string expected value.
                write!(
                    where_string,
                    "CAST(json_extract(more_info, :json_path_{i}) AS TEXT) = :json_val_{i}"
                )
                .unwrap();
                params.push((format!(":json_path_{i}"), Box::new(path.clone())));
                params.push((format!(":json_val_{i}"), Box::new(value.clone())));
            }
        }
        if where_string.is_empty() {
            where_string = "true".to_string();
        }
        let query = format!(
            "SELECT {select_expression} \
             FROM history \
             WHERE ({where_string}) \
             ORDER BY id {asc} \
             {limit}"
        );
        (query, params)
    }

    fn save_impl<E: HistoryItemExtraInfo>(
        &mut self,
        mut entry: HistoryItem<E>,
    ) -> Result<HistoryItem<E>> {
        let more_info_serialized = entry
            .more_info
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(map_json_err)?;
        let ret: i64 = self
            .db
            .prepare(
                "insert into history
                               (id,  start_timestamp,  command_line,  session_id,  hostname,  cwd,  duration_ms,  exit_status,  more_info)
                        values (:id, :start_timestamp, :command_line, :session_id, :hostname, :cwd, :duration_ms, :exit_status, :more_info)
                    on conflict (history.id) do update set
                        start_timestamp = excluded.start_timestamp,
                        command_line = excluded.command_line,
                        session_id = excluded.session_id,
                        hostname = excluded.hostname,
                        cwd = excluded.cwd,
                        duration_ms = excluded.duration_ms,
                        exit_status = excluded.exit_status,
                        more_info = excluded.more_info
                    returning id",
            )
            .map_err(map_sqlite_err)?
            .query_row(
                named_params! {
                    ":id": entry.id.map(|id| id.0),
                    ":start_timestamp": entry.start_timestamp.map(|e| e.timestamp_millis()),
                    ":command_line": entry.command_line,
                    ":session_id": entry.session_id.map(|e| e.0),
                    ":hostname": entry.hostname,
                    ":cwd": entry.cwd,
                    ":duration_ms": entry.duration.map(|e| e.as_millis() as i64),
                    ":exit_status": entry.exit_status,
                    ":more_info": more_info_serialized,
                },
                |row| row.get(0),
            )
            .map_err(map_sqlite_err)?;
        entry.id = Some(HistoryItemId::new(ret));
        Ok(entry)
    }

    /// Save a history item with typed `more_info`.
    ///
    /// Unlike [`History::save`], this method preserves the full `more_info` type `E`
    /// by serializing it to JSON and storing it in the `more_info` column.
    ///
    /// Note: this method is specific to [`SqliteBackedHistory`]. The [`History`] trait
    /// methods use [`IgnoreAllExtraInfo`] and do not roundtrip custom `more_info`.
    pub fn save_with_extra<E: HistoryItemExtraInfo>(
        &mut self,
        entry: HistoryItem<E>,
    ) -> Result<HistoryItem<E>> {
        self.save_impl(entry)
    }

    /// Load a history item by ID with typed `more_info`.
    ///
    /// Unlike [`History::load`], this method deserializes `more_info` into the concrete
    /// type `E` instead of discarding it.
    ///
    /// Note: this method is specific to [`SqliteBackedHistory`]. The [`History`] trait
    /// methods use [`IgnoreAllExtraInfo`] and do not roundtrip custom `more_info`.
    pub fn load_with_extra<E: HistoryItemExtraInfo>(
        &self,
        id: HistoryItemId,
    ) -> Result<HistoryItem<E>> {
        self.db
            .prepare("select * from history where id = :id")
            .map_err(map_sqlite_err)?
            .query_row(named_params! { ":id": id.0 }, deserialize_history_item::<E>)
            .map_err(map_sqlite_err)
    }

    /// Search history items with typed `more_info`.
    ///
    /// Unlike [`History::search`], this method deserializes `more_info` into the concrete
    /// type `E`. It also evaluates [`SearchFilter::more_info_json`] conditions using
    /// SQLite's `json_extract()`.
    ///
    /// Note: this method is specific to [`SqliteBackedHistory`]. The [`History`] trait
    /// methods use [`IgnoreAllExtraInfo`] and ignore [`SearchFilter::more_info_json`].
    pub fn search_with_extra<E: HistoryItemExtraInfo>(
        &self,
        query: SearchQuery,
    ) -> Result<Vec<HistoryItem<E>>> {
        let (sql, params) = self.construct_query(&query, "*");
        let params_borrow: Vec<(&str, &dyn ToSql)> =
            params.iter().map(|e| (e.0.as_str(), &*e.1)).collect();
        self.db
            .prepare(&sql)
            .map_err(map_sqlite_err)?
            .query_map(&params_borrow[..], deserialize_history_item::<E>)
            .map_err(map_sqlite_err)?
            .collect::<rusqlite::Result<Vec<HistoryItem<E>>>>()
            .map_err(map_sqlite_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::base::{SearchDirection, SearchFilter};
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    struct TestExtra {
        meta_command: bool,
        tag: String,
    }
    impl HistoryItemExtraInfo for TestExtra {}

    fn item_with_extra(cmd: &str, extra: TestExtra) -> HistoryItem<TestExtra> {
        HistoryItem {
            id: None,
            start_timestamp: None,
            command_line: cmd.to_string(),
            session_id: None,
            hostname: None,
            cwd: None,
            duration: None,
            exit_status: None,
            more_info: Some(extra),
        }
    }

    fn item_no_extra(cmd: &str) -> HistoryItem<TestExtra> {
        HistoryItem {
            id: None,
            start_timestamp: None,
            command_line: cmd.to_string(),
            session_id: None,
            hostname: None,
            cwd: None,
            duration: None,
            exit_status: None,
            more_info: None,
        }
    }

    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    #[test]
    fn save_and_load_with_extra() -> crate::Result<()> {
        let mut db = SqliteBackedHistory::in_memory()?;
        let item = item_with_extra(
            "ls -la",
            TestExtra {
                meta_command: false,
                tag: "test".into(),
            },
        );
        let saved = db.save_with_extra(item.clone())?;
        assert!(saved.id.is_some());
        assert_eq!(saved.more_info, item.more_info);

        let loaded = db.load_with_extra::<TestExtra>(saved.id.unwrap())?;
        assert_eq!(loaded, saved);
        Ok(())
    }

    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    #[test]
    fn save_with_extra_null_more_info_roundtrips() -> crate::Result<()> {
        let mut db = SqliteBackedHistory::in_memory()?;
        let item = item_no_extra("pwd");
        let saved = db.save_with_extra(item)?;
        assert!(saved.id.is_some());

        let loaded = db.load_with_extra::<TestExtra>(saved.id.unwrap())?;
        assert_eq!(loaded.more_info, None);
        Ok(())
    }

    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    #[test]
    fn search_with_extra_more_info_json_matches() -> crate::Result<()> {
        let mut db = SqliteBackedHistory::in_memory()?;

        // SQLite's json_extract() returns booleans as integers (1/0), not strings ("true"/"false").
        // Use "1" to match JSON `true` and "0" to match JSON `false`.
        let saved_meta = db.save_with_extra(item_with_extra(
            ":help",
            TestExtra {
                meta_command: true,
                tag: "meta".into(),
            },
        ))?;
        let saved_normal = db.save_with_extra(item_with_extra(
            "ls",
            TestExtra {
                meta_command: false,
                tag: "normal".into(),
            },
        ))?;
        db.save_with_extra(item_no_extra("pwd"))?;

        // Filter for meta commands (meta_command = true → SQLite integer 1)
        let filter = SearchFilter {
            more_info_json: Some(vec![("$.meta_command".to_string(), "1".to_string())]),
            ..SearchFilter::anything(None)
        };
        let results = db.search_with_extra::<TestExtra>(SearchQuery {
            filter,
            ..SearchQuery::everything(SearchDirection::Forward, None)
        })?;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, saved_meta.id);

        // Filter for non-meta commands (meta_command = false → SQLite integer 0)
        let filter2 = SearchFilter {
            more_info_json: Some(vec![("$.meta_command".to_string(), "0".to_string())]),
            ..SearchFilter::anything(None)
        };
        let results2 = db.search_with_extra::<TestExtra>(SearchQuery {
            filter: filter2,
            ..SearchQuery::everything(SearchDirection::Forward, None)
        })?;

        // Only "ls" matches (meta_command: false = integer 0); "pwd" with NULL more_info does not match
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0].id, saved_normal.id);
        Ok(())
    }

    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    #[test]
    fn search_with_extra_null_more_info_not_matched() -> crate::Result<()> {
        let mut db = SqliteBackedHistory::in_memory()?;
        db.save_with_extra(item_no_extra("pwd"))?;

        let filter = SearchFilter {
            more_info_json: Some(vec![("$.meta_command".to_string(), "1".to_string())]),
            ..SearchFilter::anything(None)
        };
        let results = db.search_with_extra::<TestExtra>(SearchQuery {
            filter,
            ..SearchQuery::everything(SearchDirection::Forward, None)
        })?;

        assert_eq!(results, vec![]);
        Ok(())
    }

    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    #[test]
    fn typed_and_untyped_interop() -> crate::Result<()> {
        let mut db = SqliteBackedHistory::in_memory()?;
        let item = item_with_extra(
            ":cd /tmp",
            TestExtra {
                meta_command: true,
                tag: String::new(),
            },
        );
        let saved = db.save_with_extra(item)?;

        // Load as untyped (IgnoreAllExtraInfo): more_info column is non-NULL but deserialized
        // as Some(IgnoreAllExtraInfo) since IgnoreAllExtraInfo accepts any JSON value
        let untyped = db.load(saved.id.unwrap())?;
        assert_eq!(untyped.command_line, ":cd /tmp");
        assert_eq!(untyped.more_info, Some(IgnoreAllExtraInfo));
        Ok(())
    }
}
