use std::collections::HashMap;
use std::fmt::{Debug};
use std::marker::PhantomData;
use std::time::Duration;

use chrono::{LocalResult, TimeZone, Utc};
use itertools::Itertools;
use rqlite_client::Mapping;
use serde_json::{json, Value};

use crate::{CommandLineSearch, result::{ReedlineError, ReedlineErrorVariants}, Result, SearchDirection};
use crate::history::base::HistoryStorageDest;

use super::{
    base::SearchQuery,
    History, HistoryItem, HistoryItemId, HistorySessionId,
};

/// A history that stores the values to a Rqlite database.
/// In addition to storing the command, the history can store an additional arbitrary HistoryEntryContext,
/// to add information such as a timestamp, running directory, result...
///
/// ## Required feature:
/// `rqlite`
pub struct RqliteBackedHistory {
    db: rqlite_client::Connection,
    session: Option<HistorySessionId>,
    session_timestamp: Option<chrono::DateTime<Utc>>,
}

fn json_value_into_string(val: &Value) -> Option<String> {
    Some(val.as_str().map(|i| i.to_string()).unwrap_or("".into()))
}

fn deserialize_history_item(row: RqliteIterItem) -> Option<HistoryItem> {
    let more_info = row.get_col("more_info", |val| Some(serde_json::from_value(val.to_owned())))
        .transpose();
    Some(HistoryItem {
        id: row.get_col("id", |i| i.as_i64().map(|i| HistoryItemId::new(i))),
        start_timestamp: row.get_col("start_timestamp", |e| e.as_i64()
            .map(|i| match Utc.timestamp_millis_opt(i) {
                LocalResult::Single(e) => e,
                _ => chrono::Utc::now(),
            })),
        command_line: row.get_col("command_line", json_value_into_string).unwrap(),
        session_id: row.get_col("session_id", |val| val.as_i64().map(HistorySessionId::new)),
        hostname: row.get_col("hostname", json_value_into_string),
        cwd: row.get_col("cwd", json_value_into_string),
        duration: row.get_col("duration_ms", |val| val.as_u64().map(Duration::from_millis)),
        exit_status: row.get_col("exit_status", |val| val.as_i64()),
        more_info: more_info.unwrap_or(None),
    })
}

impl History for RqliteBackedHistory {
    fn save(&mut self, mut entry: HistoryItem) -> Result<HistoryItem> {
        let payload = json!([
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
            entry.id.map(|id| id.0),
            entry.start_timestamp.map(|e| e.timestamp_millis()),
            entry.command_line,
            entry.session_id.map(|e| e.0),
            entry.hostname,
            entry.cwd,
            entry.duration.map(|e| e.as_millis() as i64),
            entry.exit_status,
            entry.more_info.as_ref().map(|e| serde_json::to_string(e).unwrap_or("".into())),
        ]);
        let ret = self.db.execute()
            .push_sql(payload)
            .execute_run()
            .map_err(map_rqlite_err)?
            .next();
        entry.id = if let Some(Mapping::Execute(ret)) = ret {
            Some(HistoryItemId::new(ret.last_insert_id as i64))
        } else { None };

        Ok(entry)
    }

    fn load(&self, id: HistoryItemId) -> Result<HistoryItem> {
        let entry = self.db.query()
            .push_sql_str_slice(&["select * from history where id = :id", id.0.to_string().as_str()])
            .query_run()
            .map_err(map_rqlite_err)?
            .next().unwrap()
            .into_rqlite_iter(deserialize_history_item)
            .map_err(map_rqlite_err)?
            .next().unwrap();
        Ok(entry)
    }

    fn count(&self, query: SearchQuery) -> Result<i64> {
        let sql = self.construct_query(&query, "coalesce(count(*), 0)");
        let result = self.db.query()
            .set_sql(sql)
            .query_run()
            .map_err(map_rqlite_err)?
            .next().unwrap()
            .into_rqlite_iter(|row| row.get_idx(0, |val| val.as_i64()))
            .map_err(map_rqlite_err)?
            .next().unwrap();

        Ok(result)
    }

    fn search(&self, query: SearchQuery) -> Result<Vec<HistoryItem>> {
        let sql = self.construct_query(&query, "*");
        let mapping = self.db.query()
            .set_sql(sql)
            .query_run()
            .map_err(map_rqlite_err)?
            .next().unwrap()
            ;
        Ok(RqliteIter::new(mapping, deserialize_history_item)
            .map_err(map_rqlite_err)?
            .collect()
        )
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
        self.db.execute()
            .push_sql_str("delete from history")
            .execute_run()
            .map_err(map_rqlite_err)?;

        self.db.execute()
            .push_sql_str("VACUUM ")
            .execute_run()
            .map_err(map_rqlite_err)?;

        Ok(())
    }

    fn delete(&mut self, entry: HistoryItemId) -> Result<()> {
        self.db.execute()
            .push_sql_str_slice(&["delete from history where id = ?", &entry.0.to_string()])
            .execute_run()
            .map_err(map_rqlite_err)?;

        Ok(())
    }

    fn sync(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn session(&self) -> Option<HistorySessionId> {
        self.session
    }
}

impl RqliteBackedHistory {
    /// Create rqlite backed history with url
    ///
    /// # Arguments 
    ///
    /// * `url`: The url of reachable rqlite instance
    /// * `session`: Identify of history session
    /// * `session_timestamp`: Shell session started time
    ///
    /// returns: Result<RqliteBackedHistory, ReedlineError> 
    ///
    /// # Examples 
    ///
    /// ```
    /// use reedline::{Reedline, RqliteBackedHistory, HistoryStorageDest};
    /// let history = RqliteBackedHistory::with_url(
    ///   HistoryStorageDest::Url(url::Url::parse("http://localhost:4001").unwrap()),
    ///   Reedline::create_history_session_id(),
    ///   Some(chrono::Utc::now())
    /// );
    /// ```
    pub fn with_url(
        dest: HistoryStorageDest,
        session: Option<HistorySessionId>,
        session_timestamp: Option<chrono::DateTime<Utc>>,
    ) -> Result<Self> {
        match dest {
            HistoryStorageDest::Path(file) => Err(ReedlineError(
                ReedlineErrorVariants::HistoryDatabaseError(format!("Expect url, got file: {:?}", file))
            )),
            HistoryStorageDest::Url(url) => {
                let db = rqlite_client::Connection::new(url.as_str())
                    .map_err(map_rqlite_err)?;
                Self::from_connection(db, session, session_timestamp)
            }
        }
    }

    /// initialize a new database / migrate an existing one
    fn from_connection(
        db: rqlite_client::Connection,
        session: Option<HistorySessionId>,
        session_timestamp: Option<chrono::DateTime<Utc>>,
    ) -> Result<Self> {
        let db_version = db.query()
            .set_sql_str("SELECT user_version FROM pragma_user_version")
            .query_run()
            .map_err(map_rqlite_err)?
            .next().unwrap()
            .into_rqlite_iter(|row| row.get_idx(0, |val| val.as_i64()))
            .map_err(map_rqlite_err)?
            .next().unwrap();

        if db_version != 0 {
            return Err(ReedlineError(ReedlineErrorVariants::HistoryDatabaseError(
                format!("Unknown database version {db_version}"),
            )));
        }

        let query = db.execute().push_sql_str("
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
        ).execute_run().map_err(map_rqlite_err)?;
        if let Some(err) = query.first_error()
        {
            return Err(map_rqlite_err(err));
        }

        Ok(RqliteBackedHistory {
            db,
            session,
            session_timestamp,
        })
    }

    fn construct_query<'a>(
        &self,
        query: &'a SearchQuery,
        select_expression: &str,
    ) -> serde_json::Value {
        let mut wheres = vec![];
        let mut params = vec![];

        let (is_asc, asc) = match query.direction {
            SearchDirection::Forward => (true, "asc"),
            SearchDirection::Backward => (false, "desc"),
        };

        if let Some(start_time) = query.start_time {
            wheres.push(if is_asc { "timestamp_start > :start_time" } else { "timestamp_start < :start_time" });
            params.push(json!(start_time.timestamp_millis()));
        }
        if let Some(end_time) = query.end_time {
            wheres.push(if is_asc { ":end_time >= timestamp_start" } else { ":end_time <= timestamp_start" });
            params.push(json!(end_time.timestamp_millis()));
        }
        if let Some(start_id) = query.start_id {
            wheres.push(if is_asc { "id > :start_id" } else { "id < :start_id" });
            params.push(json!(start_id));
        }
        if let Some(end_id) = query.end_id {
            wheres.push(if is_asc { ":end_id >= id" } else { ":end_id <= id" });
            params.push(json!(end_id));
        }
        if let Some(command_line) = &query.filter.command_line {
            // TODO: escape %
            let command_line_like = match command_line {
                CommandLineSearch::Exact(e) => e.to_string(),
                CommandLineSearch::Prefix(prefix) => format!("{prefix}%"),
                CommandLineSearch::Substring(cont) => format!("%{cont}%"),
            };
            wheres.push("command_line like :command_line");
            params.push(json!(command_line_like));
        }
        if let Some(str) = &query.filter.not_command_line {
            wheres.push("command_line != :not_cmd");
            params.push(json!(str));
        }
        if let Some(hostname) = &query.filter.hostname {
            wheres.push("hostname = :hostname");
            params.push(json!(hostname));
        }
        if let Some(cwd_exact) = &query.filter.cwd_exact {
            wheres.push("cwd = :cwd");
            params.push(json!(cwd_exact));
        }
        if let Some(cwd_prefix) = &query.filter.cwd_prefix {
            wheres.push("cwd like :cwd_like");
            params.push(json!(format!("{cwd_prefix}%")));
        }
        if let Some(exit_successful) = query.filter.exit_successful {
            wheres.push(if exit_successful { " exit_status != 0" } else { " exit_status != 0" });
        }
        if let (Some(session_id), Some(session_timestamp)) = (query.filter.session, self.session_timestamp)
        {
            wheres.push("(session_id = :session_id OR start_timestamp < :session_timestamp)");
            params.push(json!(session_id));
            params.push(json!(session_timestamp.timestamp_millis()));
        }
        let limit = match query.limit {
            Some(l) => {
                params.push(json!(l));
                "limit :limit"
            }
            None => "",
        };

        let wheres = if wheres.is_empty() { "true".into() } else { wheres.join(" and ") };
        let query = format!(
            "SELECT {select_expression}\
             FROM history \
             WHERE ({wheres}) \
             ORDER BY id {asc} \
             {limit}"
        );
        params.insert(0, json!(query));
        json!(params)
    }
}

trait AsStrVec {
    fn as_str_vec(&self) -> Vec<&str>;
}

impl AsStrVec for Vec<String> {
    fn as_str_vec(&self) -> Vec<&str> {
        self.iter().map(|s| s.as_str()).collect()
    }
}

fn map_rqlite_err<E>(err: E) -> ReedlineError where E: Debug {
    // TODO: better error mapping
    ReedlineError(ReedlineErrorVariants::HistoryDatabaseError(format!(
        "{err:?}"
    )))
}

enum CarriedMapping {
    Associative(rqlite_client::response::mapping::Associative),
    Standard(rqlite_client::response::mapping::Standard),
    Execute(rqlite_client::response::mapping::Execute),
    Empty,
}

enum RqliteIterRow<'a> {
    HashMap(&'a HashMap<String, Value>),
    Table { column: &'a [String], values: &'a Vec<Value> },
}

struct RqliteIterItem<'a>(RqliteIterRow<'a>);


trait GetItem<T, F: Fn(&Value) -> Option<T>> {
    fn get_idx(&self, idx: usize, f: F) -> Option<T>;
    fn get_col(&self, col: &str, f: F) -> Option<T>;
}

impl<'a, T, F: Fn(&Value) -> Option<T>> GetItem<T, F> for RqliteIterItem<'a> {
    fn get_idx(&self, idx: usize, f: F) -> Option<T> {
        match &self.0 {
            RqliteIterRow::HashMap(map) => map.values().skip(idx).next(),
            RqliteIterRow::Table { values, .. } => values.get(idx)
        }.and_then(f)
    }

    fn get_col(&self, col: &str, f: F) -> Option<T> {
        match &self.0 {
            RqliteIterRow::HashMap(map) => map.get(col),
            RqliteIterRow::Table { column, values } => column.iter()
                .find_position(|e| ***e == *col)
                .and_then(|i| values.get(i.0))
        }.and_then(f)
    }
}

struct RqliteIter<T, F: Fn(RqliteIterItem) -> Option<T>> {
    carried: CarriedMapping,
    map_fn: F,
    cur_row: usize,
    row_count: usize,
    _marker: PhantomData<T>,
}

impl<T, F: Fn(RqliteIterItem) -> Option<T>> RqliteIter<T, F> {
    fn new(mapping: rqlite_client::Mapping, f: F) -> std::result::Result<RqliteIter<T, F>, rqlite_client::Error> {
        match mapping {
            Mapping::Associative(res) => Ok(RqliteIter {
                row_count: res.rows.len(),
                carried: CarriedMapping::Associative(res),
                map_fn: f,
                cur_row: 0,
                _marker: PhantomData,
            }),
            Mapping::Standard(std) => Ok(RqliteIter {
                row_count: std.values.as_ref().map(|i| i.len()).unwrap_or(0),
                carried: CarriedMapping::Standard(std),
                map_fn: f,
                cur_row: 0,
                _marker: PhantomData,
            }),
            Mapping::Execute(res) => Ok(RqliteIter {
                row_count: res.rows.as_ref().map(|i| i.len()).unwrap_or(0),
                carried: CarriedMapping::Execute(res),
                map_fn: f,
                cur_row: 0,
                _marker: PhantomData,
            }),
            Mapping::Empty(_) => Ok(RqliteIter {
                carried: CarriedMapping::Empty,
                map_fn: f,
                cur_row: 0,
                row_count: 0,
                _marker: PhantomData,
            }),
            Mapping::Error(e) => Err(rqlite_client::Error::ResultError(e.error.clone())),
        }
    }
}

trait IntoRqliteIter<T, F: Fn(RqliteIterItem) -> Option<T>> {
    fn into_rqlite_iter(self, f: F) -> std::result::Result<RqliteIter<T, F>, rqlite_client::Error>;
}

impl<T, F: Fn(RqliteIterItem) -> Option<T>> IntoRqliteIter<T, F> for rqlite_client::Mapping {
    fn into_rqlite_iter(self, f: F) -> std::result::Result<RqliteIter<T, F>, rqlite_client::Error> {
        RqliteIter::new(self, f)
    }
}

impl<T, F: Fn(RqliteIterItem) -> Option<T>> Iterator for RqliteIter<T, F> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur_row >= self.row_count { return None; }
        let item = match &self.carried {
            CarriedMapping::Associative(res) => RqliteIterItem(RqliteIterRow::HashMap(&res.rows[self.cur_row])),
            CarriedMapping::Execute(res) => {
                let map = &res.rows.as_ref()
                    .map(|i| &i[self.cur_row]);
                match map {
                    Some(map) => RqliteIterItem(RqliteIterRow::HashMap(map)),
                    None => return None,
                }
            }
            CarriedMapping::Standard(res) => RqliteIterItem(RqliteIterRow::Table {
                column: res.columns.as_slice(),
                values: res.values.as_ref().map(|i| &i[self.cur_row]).unwrap(),
            }),
            CarriedMapping::Empty => return None,
        };
        self.cur_row += 1;

        (self.map_fn)(item)
    }
}

trait FirstError {
    fn first_error(&self) -> Option<&rqlite_client::response::mapping::Error>;
}

impl FirstError for rqlite_client::response::Query {
    fn first_error(&self) -> Option<&rqlite_client::response::mapping::Error> {
        for map in self.results() {
            if let Mapping::Error(err) = map {
                return Some(err);
            }
        }
        None
    }
}

trait ExecuteRun {
    fn execute_run(self) -> std::result::Result<rqlite_client::response::query::Query, rqlite_client::Error>;
}

trait QueryRun {
    fn query_run(self) -> std::result::Result<rqlite_client::response::query::Query, rqlite_client::Error>;
}

impl<'a, T> ExecuteRun for rqlite_client::Query<'a, T>
    where T: rqlite_client::state::State,
{
    fn execute_run(self) -> std::result::Result<rqlite_client::response::query::Query, rqlite_client::Error> {
        let res = self.request_run()?;
        let query = rqlite_client::response::query::Query::try_from(res).unwrap();
        for map in query.results() {
            if let Mapping::Error(err) = map {
                return Err(rqlite_client::Error::ResultError(err.error.clone()));
            }
        }
        Ok(query)
    }
}

impl<'a, T> QueryRun for rqlite_client::Query<'a, T>
    where T: rqlite_client::state::State,
{
    fn query_run(self) -> std::result::Result<rqlite_client::response::query::Query, rqlite_client::Error> {
        let res = self.request_run()?;
        Ok(rqlite_client::response::query::Query::try_from(res).unwrap())
    }
}
