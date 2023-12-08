use super::{
    base::{CommandLineSearch, SearchDirection, SearchQuery},
    History, HistoryItem, HistoryItemId, HistorySessionId,
};
use crate::{
    result::{ReedlineError, ReedlineErrorVariants},
    Result,
};
use chrono::{TimeZone, Utc};
use rand::{rngs::SmallRng, RngCore, SeedableRng};
use rusqlite::{named_params, params, Connection, ToSql};
use std::{path::PathBuf, time::Duration};
const SQLITE_APPLICATION_ID: i32 = 1151497937;

/// A history that stores the values to an SQLite database.
/// In addition to storing the command, the history can store an additional arbitrary HistoryEntryContext,
/// to add information such as a timestamp, running directory, result...
pub struct SqliteBackedHistory {
    db: rusqlite::Connection,
    session: Option<HistorySessionId>,
    session_timestamp: Option<chrono::DateTime<Utc>>,
    rng: SmallRng,
}

fn deserialize_history_item(row: &rusqlite::Row) -> rusqlite::Result<HistoryItem> {
    let x: Option<String> = row.get("more_info")?;

    Ok(HistoryItem {
        id: HistoryItemId::new(row.get("id")?),
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
                serde_json::from_str(&x).map_err(|e| {
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
    fn generate_id(&mut self) -> HistoryItemId {
        HistoryItemId(self.rng.next_u64() as i64)
    }

    fn save(&mut self, entry: &HistoryItem) -> Result<()> {
        self
            .db
            .prepare(
                "insert into history
                               (id, start_timestamp,  command_line,  session_id,  hostname,  cwd,  duration_ms,  exit_status,  more_info)
                        values (:id, :start_timestamp, :command_line, :session_id, :hostname, :cwd, :duration_ms, :exit_status, :more_info)
                    on conflict (history.id) do update set
                        start_timestamp = excluded.start_timestamp,
                        command_line = excluded.command_line,
                        session_id = excluded.session_id,
                        hostname = excluded.hostname,
                        cwd = excluded.cwd,
                        duration_ms = excluded.duration_ms,
                        exit_status = excluded.exit_status,
                        more_info = excluded.more_info",
            )
            .map_err(map_sqlite_err)?
            .execute(
                named_params! {
                    ":id": entry.id.0,
                    ":start_timestamp": entry.start_timestamp.map(|e| e.timestamp_millis()),
                    ":command_line": entry.command_line,
                    ":session_id": entry.session_id.map(|e| e.0),
                    ":hostname": entry.hostname,
                    ":cwd": entry.cwd,
                    ":duration_ms": entry.duration.map(|e| e.as_millis() as i64),
                    ":exit_status": entry.exit_status,
                    ":more_info": entry.more_info.as_ref().map(|e| serde_json::to_string(e).unwrap())
                },
            )
            .map(|_| ())
            .map_err(map_sqlite_err)
    }

    /// this history doesn't replace entries
    fn replace(&mut self, h: &HistoryItem) -> Result<()> {
        self.save(h)
    }

    fn load(&self, id: HistoryItemId) -> Result<HistoryItem> {
        let entry = self
            .db
            .prepare("select * from history where id = :id")
            .map_err(map_sqlite_err)?
            .query_row(named_params! { ":id": id.0 }, deserialize_history_item)
            .map_err(map_sqlite_err)?;

        Ok(entry)
    }

    fn count(&self, query: SearchQuery) -> Result<u64> {
        let (query, params) = self.construct_query(&query, "coalesce(count(*), 0)");

        let params_borrow: Vec<(&str, &dyn ToSql)> = params.iter().map(|e| (e.0, &*e.1)).collect();

        let result: u64 = self
            .db
            .prepare(&query)
            .unwrap()
            .query_row(&params_borrow[..], |r| r.get(0))
            .map_err(map_sqlite_err)?;

        Ok(result)
    }

    fn search(&self, query: SearchQuery) -> Result<Vec<HistoryItem>> {
        let (query, params) = self.construct_query(&query, "*");

        let params_borrow: Vec<(&str, &dyn ToSql)> = params.iter().map(|e| (e.0, &*e.1)).collect();

        let results: Vec<HistoryItem> = self
            .db
            .prepare(&query)
            .unwrap()
            .query_map(&params_borrow[..], deserialize_history_item)
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
        self.save(&updater(item))?;
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

type BoxedNamedParams<'a> = Vec<(&'static str, Box<dyn ToSql + 'a>)>;

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
        mut db: Connection,
        session: Option<HistorySessionId>,
        session_timestamp: Option<chrono::DateTime<Utc>>,
    ) -> Result<Self> {
        let inner = || -> rusqlite::Result<(i32, Self)> {
            // https://phiresky.github.io/blog/2020/sqlite-performance-tuning/
            db.pragma_update(None, "journal_mode", "wal")?;
            db.pragma_update(None, "synchronous", "normal")?;
            db.pragma_update(None, "mmap_size", "1000000000")?;
            db.pragma_update(None, "foreign_keys", "on")?;
            db.pragma_update(None, "application_id", SQLITE_APPLICATION_ID)?;

            // Get the user version
            // By default, it is set to 0
            let db_version: i32 = db.query_row(
                "SELECT user_version FROM pragma_user_version",
                params![],
                |r| r.get(0),
            )?;

            // An up-to-date database should have its version set to the latest number (currently 1)
            // 0 means the database is either uninitialized or it is using the old history format
            if db_version == 0 {
                // Check if an history already exists
                let existing_history = db.query_row(
                    "select count(*) from pragma_table_list() where name = 'history';",
                    (),
                    |result| Ok(result.get::<_, usize>("count(*)")? > 0),
                )?;

                let mut statements = vec![];

                // If so, rename it and delete related indexes
                if existing_history {
                    statements.push(
                        "
                        alter table history rename to history_old;

                        drop index if exists idx_history_cwd;
                        drop index if exists idx_history_exit_status;
                        drop index if exists idx_history_cmd;
                        drop index if exists idx_history_cmd;
                        ",
                    );
                }

                // Create the history table using the v1 schema
                statements.push(
                    "
                    create table history (
                        idx integer primary key autoincrement,
                        id integer unique not null,
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
                    ",
                );

                // If there was an history previously, migrate it to the new table
                // Then delete it
                if existing_history {
                    statements.push(
                        "
                        insert into history (id, command_line, start_timestamp, session_id, hostname, cwd, duration_ms, exit_status, more_info)
                        select id as idx, command_line, start_timestamp, session_id, hostname, cwd, duration_ms, exit_status, more_info
                        from history_old;

                        drop table history_old;
                        ",
                    );
                }

                // Update the version to indicate the DB is up-to-date
                statements.push("pragma user_version = 1;");

                // We use a transaction to ensure consistency, given we're doing multiple operations
                let transaction = db.transaction()?;
                transaction.execute_batch(&statements.join("\n"))?;
                transaction.commit()?;
            }

            Ok((
                db_version,
                SqliteBackedHistory {
                    db,
                    session,
                    session_timestamp,
                    rng: SmallRng::from_entropy(),
                },
            ))
        };

        // Connect to the database, check it and (if required) initialize it
        let (db_version, history) = inner().map_err(map_sqlite_err)?;

        // Ensure the database version is the currently supported one
        // If this isn't the case, then something is wrong
        // (either the previous versions migration is buggy, or the database is using a format deployed on a
        //  later reedline version than this one)
        if db_version != 1 {
            return Err(ReedlineError(ReedlineErrorVariants::HistoryDatabaseError(
                format!("Unknown database version {db_version}"),
            )));
        }

        Ok(history)
    }

    fn construct_query<'a>(
        &self,
        query: &'a SearchQuery,
        select_expression: &str,
    ) -> (String, BoxedNamedParams<'a>) {
        // Destructure the query - this ensures that if another element is added to this type later on,
        // we won't forget to update this function as the destructuring will then be incomplete.
        let SearchQuery {
            direction,
            start_time,
            end_time,
            start_id,
            end_id,
            limit,
            filter,
        } = query;

        let (is_asc, asc) = match direction {
            SearchDirection::Forward => (true, "asc"),
            SearchDirection::Backward => (false, "desc"),
        };

        // TODO: find a way to avoid too many allocations
        // Current version is an acceptable compromise given most of the performance
        // will be eaten by SQLite anyway
        let mut wheres = Vec::new();
        let mut params: BoxedNamedParams = Vec::new();

        if let Some(start) = start_time {
            let cmp_op = if is_asc { '>' } else { '<' };
            wheres.push(format!("timestamp_start {cmp_op} :start_time"));
            params.push((":start_time", Box::new(start.timestamp_millis())));
        }

        if let Some(end) = end_time {
            let cmp_op = if is_asc { ">=" } else { "<=" };
            wheres.push(format!(":end_time {cmp_op} timestamp_start"));
            params.push((":end_time", Box::new(end.timestamp_millis())));
        }

        if let Some(start) = start_id {
            let cmp_op = if is_asc { '>' } else { '<' };
            wheres.push(format!(
                "idx {cmp_op} (SELECT idx FROM history WHERE id = :start_id)"
            ));
            params.push((":start_id", Box::new(start.0)));
        }

        if let Some(end) = end_id {
            let cmp_op = if is_asc { ">=" } else { "<=" };
            wheres.push(format!(
                "idx {cmp_op} (SELECT idx FROM history WHERE id = :start_id)"
            ));
            params.push((":end_id", Box::new(end.0)));
        }

        let limit = match limit {
            Some(l) => {
                params.push((":limit", Box::new(l)));
                "limit :limit"
            }
            None => "",
        };

        if let Some(command_line) = &filter.command_line {
            let command_line_like = match command_line {
                CommandLineSearch::Exact(e) => escape_like_with_backslashes(e, ESCAPE_CHAR),
                CommandLineSearch::Prefix(prefix) => {
                    format!("{}%", escape_like_with_backslashes(prefix, ESCAPE_CHAR))
                }
                CommandLineSearch::Substring(cont) => {
                    format!("%{}%", escape_like_with_backslashes(cont, ESCAPE_CHAR))
                }
            };

            wheres.push(format!(
                "command_line like :command_line escape '{ESCAPE_CHAR}'"
            ));
            params.push((":command_line", Box::new(command_line_like)));
        }

        if let Some(str) = &filter.not_command_line {
            wheres.push("command_line != :not_cmd".to_owned());
            params.push((":not_cmd", Box::new(str)));
        }

        if let Some(hostname) = &filter.hostname {
            wheres.push("hostname = :hostname".to_owned());
            params.push((":hostname", Box::new(hostname)));
        }

        if let Some(cwd_exact) = &filter.cwd_exact {
            wheres.push("cwd = :cwd".to_owned());
            params.push((":cwd", Box::new(cwd_exact)));
        }

        if let Some(cwd_prefix) = &filter.cwd_prefix {
            wheres.push("cwd like :cwd_like".to_owned());
            let cwd_like = format!("{cwd_prefix}%");
            params.push((":cwd_like", Box::new(cwd_like)));
        }

        if let Some(exit_successful) = filter.exit_successful {
            let cmp_op = if exit_successful { "=" } else { "!=" };
            wheres.push(format!("exit_status {cmp_op} 0"));
        }

        if let (Some(session_id), Some(session_timestamp)) =
            (filter.session, self.session_timestamp)
        {
            // Filter so that we get rows:
            // - that have the same session_id, or
            // - were executed before our session started
            wheres.push(
                "(session_id = :session_id OR start_timestamp < :session_timestamp)".to_owned(),
            );
            params.push((":session_id", Box::new(session_id)));
            params.push((
                ":session_timestamp",
                Box::new(session_timestamp.timestamp_millis()),
            ));
        }
        let mut wheres = wheres.join(" and ");
        if wheres.is_empty() {
            wheres = "true".to_string();
        }
        let query = format!(
            "SELECT {select_expression} \
             FROM history \
             WHERE ({wheres}) \
             ORDER BY idx {asc} \
             {limit}"
        );
        (query, params)
    }
}

/// Escape special symbols for an SQL LIKE clause
/// (!) Requires LIKE clause to specify an `ESCAPE '<char>'` clause
fn escape_like_with_backslashes(str: &str, escape_char: char) -> String {
    let mut str = str.replace(escape_char, &format!("{escape_char}{escape_char}"));

    for forbidden in ['%', '\'', '\n'] {
        str = str.replace(forbidden, &format!("{escape_char}{forbidden}"));
    }

    str
}

static ESCAPE_CHAR: char = '\\';
