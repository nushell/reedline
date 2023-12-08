use indexmap::IndexMap;
use rand::{rngs::SmallRng, Rng, SeedableRng};

use super::{
    base::CommandLineSearch, History, HistoryItem, HistoryItemId, SearchDirection, SearchQuery,
};
use crate::{
    result::{ReedlineError, ReedlineErrorVariants},
    HistorySessionId, Result,
};

use std::{
    fs::OpenOptions,
    io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write},
    ops::{Deref, DerefMut},
    path::PathBuf,
};

/// Default size of the [`FileBackedHistory`] used when calling [`FileBackedHistory::default()`]
pub const HISTORY_SIZE: usize = 1000;
pub const NEWLINE_ESCAPE: &str = "<\\n>";

/// Stateful history that allows up/down-arrow browsing with an internal cursor.
///
/// Can optionally be associated with a newline separated history file using the [`FileBackedHistory::with_file()`] constructor.
/// Similar to bash's behavior without HISTTIMEFORMAT.
/// (See <https://www.gnu.org/software/bash/manual/html_node/Bash-History-Facilities.html>)
/// If the history is associated to a file all new changes within a given history capacity will be written to disk when History is dropped.
#[derive(Debug)]
pub struct FileBackedHistory {
    capacity: usize,
    entries: IndexMap<HistoryItemId, String>,
    file: Option<PathBuf>,
    last_on_disk: Option<HistoryItemId>,
    session: Option<HistorySessionId>,
    rng: SmallRng,
}

impl Default for FileBackedHistory {
    /// Creates an in-memory [`History`] with a maximal capacity of [`HISTORY_SIZE`].
    ///
    /// To create a [`History`] that is synchronized with a file use [`FileBackedHistory::with_file()`]
    ///
    /// # Panics
    ///
    /// If `HISTORY_SIZE == usize::MAX`
    fn default() -> Self {
        match Self::new(HISTORY_SIZE) {
            Ok(history) => history,
            Err(e) => panic!("{}", e),
        }
    }
}

static ID_SEP: &str = "<id>:";

fn encode_entry(id: HistoryItemId, s: &str) -> String {
    format!("{id}{ID_SEP}{}", s.replace('\n', NEWLINE_ESCAPE))
}

/// Decode an entry
///
/// Legacy format: ls /
/// New format   : 182535<id>:ls /
///
/// If a line can't be parsed using the new format, it will fallback to the legacy one.
///
/// This allows this function to support decoding for both legacy and new histories,
/// as well as mixing both of them.
fn decode_entry(s: &str, counter: &mut i64) -> (HistoryItemId, String) {
    let mut parsed = None;

    if let Some(sep) = s.find(ID_SEP) {
        if let Ok(parsed_id) = s[..sep].parse() {
            parsed = Some((parsed_id, &s[sep + ID_SEP.len()..]));
        }
    }

    let (id, content) = parsed.unwrap_or_else(|| {
        *counter += 1;
        (*counter - 1, s)
    });

    (HistoryItemId(id), content.replace(NEWLINE_ESCAPE, "\n"))
}

impl History for FileBackedHistory {
    fn generate_id(&mut self) -> HistoryItemId {
        HistoryItemId(self.rng.gen())
    }

    /// only saves a value if it's different than the last value
    fn save(&mut self, h: &HistoryItem) -> Result<()> {
        let entry = h.command_line.clone();

        // Don't append if the preceding value is identical or the string empty
        if self
            .entries
            .last()
            .map_or(true, |(_, previous)| previous != &entry)
            && !entry.is_empty()
            && self.capacity > 0
        {
            if self.entries.len() >= self.capacity {
                // History is "full", so we delete the oldest entry first,
                // before adding a new one.
                let first_id = *(self.entries.first().unwrap().0);
                let prev = self.entries.shift_remove(&first_id);
                assert!(prev.is_some());
            }

            self.entries.insert(h.id, entry.to_string());
        }

        Ok(())
    }

    /// this history doesn't replace entries
    fn replace(&mut self, h: &HistoryItem) -> Result<()> {
        self.save(h)
    }

    fn load(&self, id: HistoryItemId) -> Result<HistoryItem> {
        println!("{:?}", self.entries);

        Ok(FileBackedHistory::construct_entry(
            id,
            self.entries
                .get(&id)
                .ok_or(ReedlineError(ReedlineErrorVariants::OtherHistoryError(
                    "Item does not exist",
                )))?
                .clone(),
        ))
    }

    fn count(&self, query: SearchQuery) -> Result<u64> {
        // todo: this could be done cheaper
        Ok(self.search(query)?.len() as u64)
    }

    fn search(&self, query: SearchQuery) -> Result<Vec<HistoryItem>> {
        if query.start_time.is_some() || query.end_time.is_some() {
            return Err(ReedlineError(
                ReedlineErrorVariants::HistoryFeatureUnsupported {
                    history: "FileBackedHistory",
                    feature: "filtering by time",
                },
            ));
        }

        if query.filter.hostname.is_some()
            || query.filter.cwd_exact.is_some()
            || query.filter.cwd_prefix.is_some()
            || query.filter.exit_successful.is_some()
        {
            return Err(ReedlineError(
                ReedlineErrorVariants::HistoryFeatureUnsupported {
                    history: "FileBackedHistory",
                    feature: "filtering by extra info",
                },
            ));
        }

        let (from_id, to_id) = {
            let start = query.start_id;
            let end = query.end_id;

            if let SearchDirection::Backward = query.direction {
                (end, start)
            } else {
                (start, end)
            }
        };

        let from_index = match from_id {
            Some(from_id) => self.entries.get_index_of(&from_id).expect("todo"),
            None => 0,
        };

        let to_index = match to_id {
            Some(to_id) => self.entries.get_index_of(&to_id).expect("todo"),
            None => self.entries.len().saturating_sub(1),
        };

        assert!(from_index <= to_index);

        let iter = self
            .entries
            .iter()
            .skip(from_index)
            .take(1 + to_index - from_index);

        let limit = match query.limit {
            Some(limit) => usize::try_from(limit).unwrap(),
            None => usize::MAX,
        };

        let filter = |(id, cmd): (&HistoryItemId, &String)| {
            let str_matches = match &query.filter.command_line {
                Some(CommandLineSearch::Prefix(p)) => cmd.starts_with(p),
                Some(CommandLineSearch::Substring(p)) => cmd.contains(p),
                Some(CommandLineSearch::Exact(p)) => cmd == p,
                None => true,
            };

            if !str_matches {
                return None;
            }

            if let Some(str) = &query.filter.not_command_line {
                if cmd == str {
                    return None;
                }
            }

            Some(FileBackedHistory::construct_entry(
                *id,
                cmd.clone(), // todo: this cloning might be a perf bottleneck
            ))
        };

        Ok(match query.direction {
            SearchDirection::Backward => iter.rev().filter_map(filter).take(limit).collect(),
            SearchDirection::Forward => iter.filter_map(filter).take(limit).collect(),
        })
    }

    fn update(
        &mut self,
        _id: super::HistoryItemId,
        _updater: &dyn Fn(super::HistoryItem) -> super::HistoryItem,
    ) -> Result<()> {
        Err(ReedlineError(
            ReedlineErrorVariants::HistoryFeatureUnsupported {
                history: "FileBackedHistory",
                feature: "updating entries",
            },
        ))
    }

    fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        self.last_on_disk = None;

        if let Some(file) = &self.file {
            if let Err(err) = std::fs::remove_file(file) {
                return Err(ReedlineError(ReedlineErrorVariants::IOError(err)));
            }
        }

        Ok(())
    }

    fn delete(&mut self, _h: super::HistoryItemId) -> Result<()> {
        Err(ReedlineError(
            ReedlineErrorVariants::HistoryFeatureUnsupported {
                history: "FileBackedHistory",
                feature: "removing entries",
            },
        ))
    }

    /// Writes unwritten history contents to disk.
    ///
    /// If file would exceed `capacity` truncates the oldest entries.
    fn sync(&mut self) -> std::io::Result<()> {
        let Some(fname) = &self.file else {
            return Ok(());
        };

        // The unwritten entries
        let last_index_on_disk = self
            .last_on_disk
            .map(|id| self.entries.get_index_of(&id).unwrap());

        let range_start = match last_index_on_disk {
            Some(index) => index + 1,
            None => 0,
        };

        let own_entries = self.entries.get_range(range_start..).unwrap();

        if let Some(base_dir) = fname.parent() {
            std::fs::create_dir_all(base_dir)?;
        }

        let mut f_lock = fd_lock::RwLock::new(
            OpenOptions::new()
                .create(true)
                .write(true)
                .read(true)
                .open(fname)?,
        );

        let mut writer_guard = f_lock.write()?;

        let (mut foreign_entries, truncate) = {
            let reader = BufReader::new(writer_guard.deref());

            let mut counter = 0;

            let mut from_file = reader
                .lines()
                .map(|o| o.map(|i| decode_entry(&i, &mut counter)))
                .collect::<std::io::Result<IndexMap<_, _>>>()?;

            if from_file.len() + own_entries.len() > self.capacity {
                let start = from_file.len() + own_entries.len() - self.capacity;

                (from_file.split_off(start), true)
            } else {
                (from_file, false)
            }
        };

        {
            let mut writer = BufWriter::new(writer_guard.deref_mut());

            // In case of truncation, we first write every foreign entry (replacing existing content)
            if truncate {
                writer.rewind()?;

                for (id, line) in &foreign_entries {
                    writer.write_all(encode_entry(*id, line).as_bytes())?;
                    writer.write_all("\n".as_bytes())?;
                }
            } else {
                // Otherwise we directly jump at the end of the file
                writer.seek(SeekFrom::End(0))?;
            }

            // Then we write new entries (that haven't been synced to the file yet)
            for (id, line) in own_entries {
                writer.write_all(encode_entry(*id, line).as_bytes())?;
                writer.write_all("\n".as_bytes())?;
            }

            writer.flush()?;
        }

        // If truncation is needed, we then remove everything after the cursor's current location
        if truncate {
            let file = writer_guard.deref_mut();
            let file_len = file.stream_position()?;
            file.set_len(file_len)?;
        }

        match last_index_on_disk {
            Some(last_index_on_disk) => {
                if last_index_on_disk + 1 < self.entries.len() {
                    foreign_entries.extend(self.entries.drain(last_index_on_disk + 1..));
                }
            }

            None => {
                foreign_entries.extend(self.entries.drain(..));
            }
        }

        self.entries = foreign_entries;

        self.last_on_disk = self.entries.last().map(|(id, _)| *id);

        Ok(())
    }

    fn session(&self) -> Option<HistorySessionId> {
        self.session
    }
}

impl FileBackedHistory {
    /// Creates a new in-memory history that remembers `n <= capacity` elements
    ///
    pub fn new(capacity: usize) -> Result<Self> {
        if capacity == usize::MAX {
            return Err(ReedlineError(ReedlineErrorVariants::OtherHistoryError(
                "History capacity too large to be addressed safely",
            )));
        }

        Ok(FileBackedHistory {
            capacity,
            entries: IndexMap::new(),
            file: None,
            last_on_disk: None,
            session: None,
            rng: SmallRng::from_entropy(),
        }
    }

    /// Creates a new history with an associated history file.
    ///
    /// History file format: commands separated by new lines.
    /// If file exists file will be read otherwise empty file will be created.
    ///
    ///
    /// **Side effects:** creates all nested directories to the file
    ///
    pub fn with_file(capacity: usize, file: PathBuf) -> std::io::Result<Self> {
        let mut hist = Self::new(capacity);

        if let Some(base_dir) = file.parent() {
            std::fs::create_dir_all(base_dir)?;
        }

        hist.file = Some(file);
        hist.sync()?;

        Ok(hist)
    }

    // this history doesn't store any info except command line
    fn construct_entry(id: HistoryItemId, command_line: String) -> HistoryItem {
        HistoryItem {
            id,
            start_timestamp: None,
            command_line,
            session_id: None,
            hostname: None,
            cwd: None,
            duration: None,
            exit_status: None,
            more_info: None,
        }
    }
}

impl Drop for FileBackedHistory {
    /// On drop the content of the [`History`] will be written to the file if specified via [`FileBackedHistory::with_file()`].
    fn drop(&mut self) {
        let _res = self.sync();
    }
}
