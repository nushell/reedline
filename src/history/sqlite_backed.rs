use super::{
    base::{CommandLineSearch, JsonFilterValue, SearchDirection, SearchQuery},
    History, HistoryItem, HistoryItemExtraInfo, HistoryItemId, HistorySessionId,
    IgnoreAllExtraInfo,
};
use crate::{
    result::{ReedlineError, ReedlineErrorVariants},
    Result,
};
use chrono::{TimeZone, Utc};
use rusqlite::{named_params, params, Connection, ToSql, TransactionBehavior};
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

/// Shared implementation of [`SqliteBackedHistory::load_with_extra`], generic over
/// anything that derefs to a [`Connection`] so it can run against either the history's
/// own connection or an in-flight [`rusqlite::Transaction`].
fn load_with_extra_conn<E: HistoryItemExtraInfo>(
    conn: &Connection,
    id: HistoryItemId,
) -> Result<HistoryItem<E>> {
    conn.prepare("select * from history where id = :id")
        .map_err(map_sqlite_err)?
        .query_row(named_params! { ":id": id.0 }, deserialize_history_item::<E>)
        .map_err(map_sqlite_err)
}

/// Shared implementation of [`SqliteBackedHistory::save_with_extra`], generic over
/// anything that derefs to a [`Connection`] so it can run against either the history's
/// own connection or an in-flight [`rusqlite::Transaction`].
fn save_with_extra_conn<E: HistoryItemExtraInfo>(
    conn: &Connection,
    mut entry: HistoryItem<E>,
) -> Result<HistoryItem<E>> {
    let more_info_serialized = entry
        .more_info
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(map_json_err)?;
    let ret: i64 = conn
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

impl History for SqliteBackedHistory {
    fn save(&mut self, entry: HistoryItem) -> Result<HistoryItem> {
        self.save_with_extra(entry)
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

    fn count(&self, mut query: SearchQuery) -> Result<i64> {
        // more_info_json is only evaluated by search_with_extra; ignore it here
        query.filter.more_info_json = None;
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

    fn search(&self, mut query: SearchQuery) -> Result<Vec<HistoryItem>> {
        // more_info_json is only evaluated by search_with_extra; ignore it here
        query.filter.more_info_json = None;
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
        // `HistoryItem` here is always `HistoryItem<IgnoreAllExtraInfo>`, which cannot
        // carry a meaningful `more_info` value (it serializes to `null` regardless of
        // what was loaded). So this type-erased path updates every column except
        // `more_info`, leaving whatever a typed caller wrote via `save_with_extra`
        // intact. Callers that need to read/modify typed `more_info` should use
        // [`Self::update_with_extra`] instead.
        self.update_preserving_more_info(updater(item))
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

type BoxedNamedParams<'a> = Vec<(&'static str, Box<dyn ToSql + 'a>)>;
type OwnedNamedParams<'a> = Vec<(String, Box<dyn ToSql + 'a>)>;

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
    ) -> (String, OwnedNamedParams<'a>) {
        // TODO: this whole function could be done with less allocs
        let (is_asc, asc) = match query.direction {
            SearchDirection::Forward => (true, "asc"),
            SearchDirection::Backward => (false, "desc"),
        };
        let mut wheres = Vec::new();
        let mut params: BoxedNamedParams = Vec::new();
        if let Some(start) = query.start_time {
            wheres.push(if is_asc {
                "timestamp_start > :start_time"
            } else {
                "timestamp_start < :start_time"
            });
            params.push((":start_time", Box::new(start.timestamp_millis())));
        }
        if let Some(end) = query.end_time {
            wheres.push(if is_asc {
                ":end_time >= timestamp_start"
            } else {
                ":end_time <= timestamp_start"
            });
            params.push((":end_time", Box::new(end.timestamp_millis())));
        }
        if let Some(start) = query.start_id {
            wheres.push(if is_asc {
                "id > :start_id"
            } else {
                "id < :start_id"
            });
            params.push((":start_id", Box::new(start.0)));
        }
        if let Some(end) = query.end_id {
            wheres.push(if is_asc {
                ":end_id >= id"
            } else {
                ":end_id <= id"
            });
            params.push((":end_id", Box::new(end.0)));
        }
        let limit = match query.limit {
            Some(l) => {
                params.push((":limit", Box::new(l)));
                "limit :limit"
            }
            None => "",
        };
        if let Some(command_line) = &query.filter.command_line {
            match command_line {
                CommandLineSearch::Exact(e) => {
                    wheres.push("command_line == :command_line");
                    params.push((":command_line", Box::new(e)));
                }
                CommandLineSearch::Prefix(prefix) => {
                    wheres.push("instr(command_line, :command_line) == 1");
                    params.push((":command_line", Box::new(prefix)));
                }
                CommandLineSearch::Substring(cont) => {
                    wheres.push("instr(command_line, :command_line) >= 1");
                    params.push((":command_line", Box::new(cont)));
                }
            };
        }

        if let Some(str) = &query.filter.not_command_line {
            wheres.push("command_line != :not_cmd");
            params.push((":not_cmd", Box::new(str)));
        }
        if let Some(hostname) = &query.filter.hostname {
            wheres.push("hostname = :hostname");
            params.push((":hostname", Box::new(hostname)));
        }
        if let Some(cwd_exact) = &query.filter.cwd_exact {
            wheres.push("cwd = :cwd");
            params.push((":cwd", Box::new(cwd_exact)));
        }
        if let Some(cwd_prefix) = &query.filter.cwd_prefix {
            wheres.push("cwd like :cwd_like");
            let cwd_like = format!("{cwd_prefix}%");
            params.push((":cwd_like", Box::new(cwd_like)));
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
            params.push((":session_id", Box::new(session_id)));
            params.push((
                ":session_timestamp",
                Box::new(session_timestamp.timestamp_millis()),
            ));
        }

        // Build WHERE string, appending dynamic json_type/json_extract conditions last.
        // Each JsonFilterValue variant generates type-precise SQL so that JSON booleans,
        // integers, and strings cannot collide with each other.
        // Dynamic parameter names (":json_path_N", ":json_val_N") are stored separately
        // since they cannot be &'static str.
        let mut where_string = wheres.join(" and ");
        let mut json_params: Vec<(String, Box<dyn ToSql + 'a>)> = Vec::new();
        if let Some(filters) = &query.filter.more_info_json {
            for (i, (path, value)) in filters.iter().enumerate() {
                if !where_string.is_empty() {
                    where_string.push_str(" and ");
                }
                match value {
                    JsonFilterValue::Null => {
                        write!(
                            where_string,
                            "json_type(more_info, :json_path_{i}) = 'null'"
                        )
                        .unwrap();
                    }
                    JsonFilterValue::Bool(b) => {
                        let type_str = if *b { "true" } else { "false" };
                        write!(
                            where_string,
                            "json_type(more_info, :json_path_{i}) = '{type_str}'"
                        )
                        .unwrap();
                    }
                    JsonFilterValue::Integer(n) => {
                        write!(
                            where_string,
                            "json_type(more_info, :json_path_{i}) = 'integer' \
                             AND json_extract(more_info, :json_path_{i}) = :json_val_{i}"
                        )
                        .unwrap();
                        json_params.push((format!(":json_val_{i}"), Box::new(*n)));
                    }
                    JsonFilterValue::Real(f) => {
                        write!(
                            where_string,
                            "json_type(more_info, :json_path_{i}) = 'real' \
                             AND json_extract(more_info, :json_path_{i}) = :json_val_{i}"
                        )
                        .unwrap();
                        json_params.push((format!(":json_val_{i}"), Box::new(*f)));
                    }
                    JsonFilterValue::Text(s) => {
                        write!(
                            where_string,
                            "json_type(more_info, :json_path_{i}) = 'text' \
                             AND json_extract(more_info, :json_path_{i}) = :json_val_{i}"
                        )
                        .unwrap();
                        json_params.push((format!(":json_val_{i}"), Box::new(s.clone())));
                    }
                }
                json_params.push((format!(":json_path_{i}"), Box::new(path.clone())));
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
        let mut all_params: OwnedNamedParams = params
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        all_params.extend(json_params);
        (query, all_params)
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
        save_with_extra_conn(&self.db, entry)
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
        load_with_extra_conn(&self.db, id)
    }

    /// Update every column of an existing row except `more_info`, leaving it as-is.
    ///
    /// Used by the type-erased [`History::update`], which cannot express a meaningful
    /// change to `more_info` (see its implementation for why).
    fn update_preserving_more_info(&mut self, entry: HistoryItem) -> Result<()> {
        let changed = self
            .db
            .prepare(
                "update history set
                    start_timestamp = :start_timestamp,
                    command_line = :command_line,
                    session_id = :session_id,
                    hostname = :hostname,
                    cwd = :cwd,
                    duration_ms = :duration_ms,
                    exit_status = :exit_status
                 where id = :id",
            )
            .map_err(map_sqlite_err)?
            .execute(named_params! {
                ":id": entry.id.map(|id| id.0),
                ":start_timestamp": entry.start_timestamp.map(|e| e.timestamp_millis()),
                ":command_line": entry.command_line,
                ":session_id": entry.session_id.map(|e| e.0),
                ":hostname": entry.hostname,
                ":cwd": entry.cwd,
                ":duration_ms": entry.duration.map(|e| e.as_millis() as i64),
                ":exit_status": entry.exit_status,
            })
            .map_err(map_sqlite_err)?;
        if changed == 0 {
            return Err(ReedlineError(ReedlineErrorVariants::HistoryDatabaseError(
                "Could not find item".to_string(),
            )));
        }
        Ok(())
    }

    /// Update a history item atomically while preserving typed `more_info`.
    ///
    /// Unlike [`History::update`], this loads and saves through [`Self::load_with_extra`]
    /// and [`Self::save_with_extra`], so `updater` can read and change the typed
    /// `more_info` and have the change persisted.
    ///
    /// Note: this method is specific to [`SqliteBackedHistory`]. It is not part of the
    /// [`History`] trait and so is unavailable through `&dyn History`.
    pub fn update_with_extra<E: HistoryItemExtraInfo>(
        &mut self,
        id: HistoryItemId,
        updater: &dyn Fn(HistoryItem<E>) -> HistoryItem<E>,
    ) -> Result<()> {
        // `Immediate` acquires the write lock up front, so no other connection can
        // interleave a write between the load and the save below.
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_err)?;
        let item = load_with_extra_conn::<E>(&tx, id)?;
        save_with_extra_conn(&tx, updater(item))?;
        tx.commit().map_err(map_sqlite_err)?;
        Ok(())
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
    use crate::history::base::{JsonFilterValue, SearchDirection, SearchFilter};
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    struct TestExtra {
        meta_command: bool,
        tag: String,
        count: i64,
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
                ..Default::default()
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

        let saved_meta = db.save_with_extra(item_with_extra(
            ":help",
            TestExtra {
                meta_command: true,
                tag: "meta".into(),
                ..Default::default()
            },
        ))?;
        let saved_normal = db.save_with_extra(item_with_extra(
            "ls",
            TestExtra {
                meta_command: false,
                tag: "normal".into(),
                ..Default::default()
            },
        ))?;
        db.save_with_extra(item_no_extra("pwd"))?;

        let filter = SearchFilter {
            more_info_json: Some(vec![(
                "$.meta_command".to_string(),
                JsonFilterValue::Bool(true),
            )]),
            ..SearchFilter::anything(None)
        };
        let results = db.search_with_extra::<TestExtra>(SearchQuery {
            filter,
            ..SearchQuery::everything(SearchDirection::Forward, None)
        })?;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, saved_meta.id);

        let filter2 = SearchFilter {
            more_info_json: Some(vec![(
                "$.meta_command".to_string(),
                JsonFilterValue::Bool(false),
            )]),
            ..SearchFilter::anything(None)
        };
        let results2 = db.search_with_extra::<TestExtra>(SearchQuery {
            filter: filter2,
            ..SearchQuery::everything(SearchDirection::Forward, None)
        })?;

        // Only "ls" matches (meta_command: false); "pwd" with NULL more_info does not match
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
            more_info_json: Some(vec![(
                "$.meta_command".to_string(),
                JsonFilterValue::Bool(true),
            )]),
            ..SearchFilter::anything(None)
        };
        let results = db.search_with_extra::<TestExtra>(SearchQuery {
            filter,
            ..SearchQuery::everything(SearchDirection::Forward, None)
        })?;

        assert_eq!(results, vec![]);
        Ok(())
    }

    /// Verify that Bool, Integer, and Text filters do not collide with each other
    /// even when the raw JSON bytes look similar (e.g. `true` vs `1` vs `"1"`).
    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    #[test]
    fn json_filter_value_no_type_collisions() -> crate::Result<()> {
        let mut db = SqliteBackedHistory::in_memory()?;

        // Row A: meta_command=true (JSON boolean), count=1 (JSON integer), tag="1" (JSON string)
        db.save_with_extra(item_with_extra(
            "cmd_a",
            TestExtra {
                meta_command: true,
                tag: "1".into(),
                count: 1,
            },
        ))?;
        // Row B: meta_command=false, count=0, tag="true"
        db.save_with_extra(item_with_extra(
            "cmd_b",
            TestExtra {
                meta_command: false,
                tag: "true".into(),
                count: 0,
            },
        ))?;

        let search = |val: JsonFilterValue| -> crate::Result<usize> {
            let filter = SearchFilter {
                more_info_json: Some(vec![("$.meta_command".to_string(), val)]),
                ..SearchFilter::anything(None)
            };
            Ok(db
                .search_with_extra::<TestExtra>(SearchQuery {
                    filter,
                    ..SearchQuery::everything(SearchDirection::Forward, None)
                })?
                .len())
        };

        // Bool(true) must match only row A (meta_command: true), not integer 1 or string "1"
        assert_eq!(search(JsonFilterValue::Bool(true))?, 1);
        // Bool(false) must match only row B (meta_command: false)
        assert_eq!(search(JsonFilterValue::Bool(false))?, 1);
        // Integer(1) must not match the boolean true in meta_command
        assert_eq!(
            search(JsonFilterValue::Integer(1))?,
            0,
            "Integer(1) must not match JSON boolean true"
        );
        // Text("true") must not match the boolean true in meta_command
        assert_eq!(
            search(JsonFilterValue::Text("true".into()))?,
            0,
            r#"Text("true") must not match JSON boolean true"#
        );

        let search_count = |val: JsonFilterValue| -> crate::Result<usize> {
            let filter = SearchFilter {
                more_info_json: Some(vec![("$.count".to_string(), val)]),
                ..SearchFilter::anything(None)
            };
            Ok(db
                .search_with_extra::<TestExtra>(SearchQuery {
                    filter,
                    ..SearchQuery::everything(SearchDirection::Forward, None)
                })?
                .len())
        };

        // Integer(1) must match row A (count: 1) but not row B (count: 0)
        assert_eq!(search_count(JsonFilterValue::Integer(1))?, 1);
        // Bool(true) must not match integer 1 in count
        assert_eq!(
            search_count(JsonFilterValue::Bool(true))?,
            0,
            "Bool(true) must not match JSON integer 1"
        );
        // Text("1") must not match JSON integer 1
        assert_eq!(
            search_count(JsonFilterValue::Text("1".into()))?,
            0,
            r#"Text("1") must not match JSON integer 1"#
        );

        let search_tag = |val: JsonFilterValue| -> crate::Result<usize> {
            let filter = SearchFilter {
                more_info_json: Some(vec![("$.tag".to_string(), val)]),
                ..SearchFilter::anything(None)
            };
            Ok(db
                .search_with_extra::<TestExtra>(SearchQuery {
                    filter,
                    ..SearchQuery::everything(SearchDirection::Forward, None)
                })?
                .len())
        };

        // Text("1") must match row A (tag: "1") but not row B (tag: "true")
        assert_eq!(search_tag(JsonFilterValue::Text("1".into()))?, 1);
        // Text("true") must match row B (tag: "true") but not row A
        assert_eq!(search_tag(JsonFilterValue::Text("true".into()))?, 1);
        // Bool(true) must not match the string "true" in tag
        assert_eq!(
            search_tag(JsonFilterValue::Bool(true))?,
            0,
            r#"Bool(true) must not match JSON string "true""#
        );
        // Integer(1) must not match the string "1" in tag
        assert_eq!(
            search_tag(JsonFilterValue::Integer(1))?,
            0,
            r#"Integer(1) must not match JSON string "1""#
        );

        Ok(())
    }

    /// Verify that Real(f) does not collide with Integer values stored at the same path.
    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    #[test]
    fn json_filter_value_real_vs_integer() -> crate::Result<()> {
        // Two structs so we can store the same path ($.score) as either real or integer.
        #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
        struct WithRealScore {
            score: f64,
        }
        impl HistoryItemExtraInfo for WithRealScore {}

        #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
        struct WithIntScore {
            score: i64,
        }
        impl HistoryItemExtraInfo for WithIntScore {}

        let mut db = SqliteBackedHistory::in_memory()?;
        let make_real = |cmd: &str, score: f64| HistoryItem {
            id: None,
            start_timestamp: None,
            command_line: cmd.to_string(),
            session_id: None,
            hostname: None,
            cwd: None,
            duration: None,
            exit_status: None,
            more_info: Some(WithRealScore { score }),
        };
        let make_int = |cmd: &str, score: i64| HistoryItem {
            id: None,
            start_timestamp: None,
            command_line: cmd.to_string(),
            session_id: None,
            hostname: None,
            cwd: None,
            duration: None,
            exit_status: None,
            more_info: Some(WithIntScore { score }),
        };
        // Row A: $.score = 1.5 (JSON real)
        db.save_with_extra(make_real("real_a", 1.5))?;
        // Row B: $.score = 1.0 (JSON real — serde_json always emits decimal point for f64)
        db.save_with_extra(make_real("real_b", 1.0))?;
        // Row C: $.score = 1 (JSON integer — stored via i64 struct)
        db.save_with_extra(make_int("int_c", 1))?;

        let search_score = |val: JsonFilterValue| -> crate::Result<Vec<String>> {
            let filter = SearchFilter {
                more_info_json: Some(vec![("$.score".to_string(), val)]),
                ..SearchFilter::anything(None)
            };
            Ok(db
                .search_with_extra::<WithRealScore>(SearchQuery {
                    filter,
                    ..SearchQuery::everything(SearchDirection::Forward, None)
                })?
                .into_iter()
                .map(|h| h.command_line)
                .collect())
        };

        // Real(1.5) matches only real_a
        assert_eq!(search_score(JsonFilterValue::Real(1.5))?, vec!["real_a"]);
        // Real(1.0) matches real_b but NOT int_c (json_type for int_c is 'integer', not 'real')
        assert_eq!(search_score(JsonFilterValue::Real(1.0))?, vec!["real_b"]);
        // Integer(1) matches int_c but NOT real_a or real_b
        assert_eq!(
            search_score(JsonFilterValue::Integer(1))?,
            vec!["int_c"],
            "Integer(1) must match JSON integer 1 but not JSON reals"
        );

        Ok(())
    }

    /// Verify that Null matches JSON null at a path but not SQL-NULL more_info
    /// or a path that is simply absent from the JSON object.
    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    #[test]
    fn json_filter_value_null_vs_missing_path() -> crate::Result<()> {
        #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
        struct WithOptVal {
            val: Option<i64>,
        }
        impl HistoryItemExtraInfo for WithOptVal {}

        // A separate struct without a `val` field to produce JSON that omits $.val entirely.
        #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
        struct WithoutVal {
            other: String,
        }
        impl HistoryItemExtraInfo for WithoutVal {}

        let mut db = SqliteBackedHistory::in_memory()?;
        let make_opt = |cmd: &str, val: Option<i64>| HistoryItem {
            id: None,
            start_timestamp: None,
            command_line: cmd.to_string(),
            session_id: None,
            hostname: None,
            cwd: None,
            duration: None,
            exit_status: None,
            more_info: Some(WithOptVal { val }),
        };
        // Row A: val=null in JSON  (serde serializes None as JSON null)
        db.save_with_extra(make_opt("null_row", None))?;
        // Row B: val=42 in JSON
        db.save_with_extra(make_opt("int_row", Some(42)))?;
        // Row C: SQL NULL more_info — $.val is absent because more_info IS NULL
        db.save_with_extra::<WithOptVal>(HistoryItem {
            id: None,
            start_timestamp: None,
            command_line: "sql_null_row".to_string(),
            session_id: None,
            hostname: None,
            cwd: None,
            duration: None,
            exit_status: None,
            more_info: None,
        })?;
        // Row D: non-null JSON that simply has no `val` key — $.val path is missing
        db.save_with_extra(HistoryItem {
            id: None,
            start_timestamp: None,
            command_line: "missing_path_row".to_string(),
            session_id: None,
            hostname: None,
            cwd: None,
            duration: None,
            exit_status: None,
            more_info: Some(WithoutVal { other: "x".into() }),
        })?;

        let search_val = |val: JsonFilterValue| -> crate::Result<Vec<String>> {
            let filter = SearchFilter {
                more_info_json: Some(vec![("$.val".to_string(), val)]),
                ..SearchFilter::anything(None)
            };
            Ok(db
                .search_with_extra::<WithOptVal>(SearchQuery {
                    filter,
                    ..SearchQuery::everything(SearchDirection::Forward, None)
                })?
                .into_iter()
                .map(|h| h.command_line)
                .collect())
        };

        // Null matches row A (json_type = 'null') only; rows B, C, and D are excluded
        assert_eq!(search_val(JsonFilterValue::Null)?, vec!["null_row"]);
        // Integer(42) matches row B only
        assert_eq!(search_val(JsonFilterValue::Integer(42))?, vec!["int_row"]);

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
                ..Default::default()
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

    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    #[test]
    fn trait_update_preserves_typed_more_info() -> crate::Result<()> {
        let mut db = SqliteBackedHistory::in_memory()?;
        let saved = db.save_with_extra(item_with_extra(
            "cmd",
            TestExtra {
                meta_command: true,
                tag: "keep-me".into(),
                count: 7,
            },
        ))?;
        let id = saved.id.unwrap();

        // Go through the type-erased `History::update`, e.g. as
        // `Reedline::update_last_command_context` does for post-command bookkeeping.
        History::update(&mut db, id, &|mut e| {
            e.exit_status = Some(0);
            e
        })?;

        let reloaded = db.load_with_extra::<TestExtra>(id)?;
        assert_eq!(reloaded.exit_status, Some(0));
        assert_eq!(
            reloaded.more_info,
            Some(TestExtra {
                meta_command: true,
                tag: "keep-me".into(),
                count: 7,
            })
        );
        Ok(())
    }

    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    #[test]
    fn trait_update_missing_id_errors() {
        let mut db = SqliteBackedHistory::in_memory().unwrap();
        let result = History::update(&mut db, HistoryItemId::new(999), &|e| e);
        assert!(result.is_err());
    }

    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    #[test]
    fn update_with_extra_modifies_typed_more_info() -> crate::Result<()> {
        let mut db = SqliteBackedHistory::in_memory()?;
        let saved = db.save_with_extra(item_with_extra(
            "cmd",
            TestExtra {
                meta_command: false,
                tag: "before".into(),
                count: 1,
            },
        ))?;
        let id = saved.id.unwrap();

        db.update_with_extra::<TestExtra>(id, &|mut e| {
            if let Some(extra) = e.more_info.as_mut() {
                extra.tag = "after".into();
                extra.count += 1;
            }
            e
        })?;

        let reloaded = db.load_with_extra::<TestExtra>(id)?;
        assert_eq!(
            reloaded.more_info,
            Some(TestExtra {
                meta_command: false,
                tag: "after".into(),
                count: 2,
            })
        );
        Ok(())
    }

    #[test]
    fn update_with_extra_blocks_concurrent_writer() -> crate::Result<()> {
        use std::cell::{Cell, RefCell};
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let histfile = tmp.path().join("history.sqlite3");

        let mut writer = SqliteBackedHistory::with_file(histfile.clone(), None, None)?;
        let saved = writer.save_with_extra(item_with_extra(
            "cmd",
            TestExtra {
                meta_command: false,
                tag: "before".into(),
                count: 1,
            },
        ))?;
        let id = saved.id.unwrap();

        // A second connection to the same file. Its busy timeout is set to 0 so a
        // lock conflict surfaces immediately as an error instead of blocking for
        // rusqlite's default 5-second busy handler, keeping this test fast.
        let other = SqliteBackedHistory::with_file(histfile, None, None)?;
        other
            .db
            .busy_timeout(Duration::from_millis(0))
            .map_err(map_sqlite_err)?;
        let other = RefCell::new(other);

        let concurrent_write_blocked = Cell::new(false);
        writer.update_with_extra::<TestExtra>(id, &|mut e| {
            // Runs while `writer`'s immediate transaction still holds the write
            // lock. Without it, this competing write would succeed.
            let result = other.borrow_mut().save_with_extra(item_with_extra(
                "competing",
                TestExtra {
                    meta_command: false,
                    tag: "concurrent".into(),
                    count: 0,
                },
            ));
            let is_locked_error =
                matches!(&result, Err(e) if e.to_string().contains("database is locked"));
            concurrent_write_blocked.set(is_locked_error);

            if let Some(extra) = e.more_info.as_mut() {
                extra.tag = "after".into();
                extra.count += 1;
            }
            e
        })?;

        assert!(
            concurrent_write_blocked.get(),
            "expected the competing writer to fail with a database-locked error while update_with_extra's immediate transaction was held"
        );

        let reloaded = writer.load_with_extra::<TestExtra>(id)?;
        assert_eq!(
            reloaded.more_info,
            Some(TestExtra {
                meta_command: false,
                tag: "after".into(),
                count: 2,
            })
        );
        Ok(())
    }
}
