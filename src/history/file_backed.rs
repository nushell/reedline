use super::{
    base::CommandLineSearch, History, HistoryItem, HistoryItemId, SearchDirection, SearchQuery,
};
use crate::{
    result::{ReedlineError, ReedlineErrorVariants},
    HistorySessionId, Result,
};

use std::{
    collections::VecDeque,
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
    entries: VecDeque<String>,
    file: Option<PathBuf>,
    len_on_disk: usize, // Keep track what was previously written to disk
    session: Option<HistorySessionId>,
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

fn encode_entry(s: &str) -> String {
    s.replace('\n', NEWLINE_ESCAPE)
}

fn decode_entry(s: &str) -> String {
    s.replace(NEWLINE_ESCAPE, "\n")
}

impl History for FileBackedHistory {
    /// only saves a value if it's different than the last value
    fn save(&mut self, h: HistoryItem) -> Result<HistoryItem> {
        let entry = h.command_line;
        // Don't append if the preceding value is identical or the string empty
        let entry_id =
            if (self.entries.back() != Some(&entry)) && !entry.is_empty() && self.capacity > 0 {
                if self.entries.len() == self.capacity {
                    // History is "full", so we delete the oldest entry first,
                    // before adding a new one.
                    self.entries.pop_front();
                    self.len_on_disk = self.len_on_disk.saturating_sub(1);
                }
                self.entries.push_back(entry.to_string());
                Some(HistoryItemId::new((self.entries.len() - 1) as i64))
            } else {
                None
            };
        Ok(FileBackedHistory::construct_entry(entry_id, entry))
    }

    fn load(&self, id: HistoryItemId) -> Result<super::HistoryItem> {
        Ok(FileBackedHistory::construct_entry(
            Some(id),
            self.entries
                .get(id.0 as usize)
                .ok_or(ReedlineError(ReedlineErrorVariants::OtherHistoryError(
                    "Item does not exist",
                )))?
                .clone(),
        ))
    }

    fn count(&self, query: SearchQuery) -> Result<i64> {
        // todo: this could be done cheaper
        Ok(self.search(query)?.len() as i64)
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
        let (min_id, max_id) = {
            let start = query.start_id.map(|e| e.0);
            let end = query.end_id.map(|e| e.0);
            if let SearchDirection::Backward = query.direction {
                (end, start)
            } else {
                (start, end)
            }
        };
        // add one to make it inclusive
        let min_id = min_id.map(|e| e + 1).unwrap_or(0);
        // subtract one to make it inclusive
        let max_id = max_id
            .map(|e| e - 1)
            .unwrap_or(self.entries.len() as i64 - 1);
        if max_id < 0 || min_id > self.entries.len() as i64 - 1 {
            return Ok(vec![]);
        }
        let intrinsic_limit = max_id - min_id + 1;
        let limit = if let Some(given_limit) = query.limit {
            std::cmp::min(intrinsic_limit, given_limit) as usize
        } else {
            intrinsic_limit as usize
        };
        let filter = |(idx, cmd): (usize, &String)| {
            if !match &query.filter.command_line {
                Some(CommandLineSearch::Prefix(p)) => cmd.starts_with(p),
                Some(CommandLineSearch::Substring(p)) => cmd.contains(p),
                Some(CommandLineSearch::Exact(p)) => cmd == p,
                None => true,
            } {
                return None;
            }
            if let Some(str) = &query.filter.not_command_line {
                if cmd == str {
                    return None;
                }
            }
            Some(FileBackedHistory::construct_entry(
                Some(HistoryItemId::new(idx as i64)),
                cmd.to_string(), // todo: this copy might be a perf bottleneck
            ))
        };

        let iter = self
            .entries
            .iter()
            .enumerate()
            .skip(min_id as usize)
            .take(intrinsic_limit as usize);
        if let SearchDirection::Backward = query.direction {
            Ok(iter.rev().filter_map(filter).take(limit).collect())
        } else {
            Ok(iter.filter_map(filter).take(limit).collect())
        }
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
        self.len_on_disk = 0;

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
        if let Some(fname) = &self.file {
            // The unwritten entries
            let own_entries = self.entries.range(self.len_on_disk..);

            if let Some(base_dir) = fname.parent() {
                std::fs::create_dir_all(base_dir)?;
            }

            let mut f_lock = fd_lock::RwLock::new(
                OpenOptions::new()
                    .create(true)
                    .write(true)
                    .read(true)
                    .truncate(false)
                    .open(fname)?,
            );
            let mut writer_guard = f_lock.write()?;
            let (mut foreign_entries, truncate) = {
                let reader = BufReader::new(writer_guard.deref());
                let mut from_file = reader
                    .lines()
                    .map(|o| o.map(|i| decode_entry(&i)))
                    .collect::<std::io::Result<VecDeque<_>>>()?;
                if from_file.len() + own_entries.len() > self.capacity {
                    (
                        from_file.split_off(
                            from_file.len() - (self.capacity.saturating_sub(own_entries.len())),
                        ),
                        true,
                    )
                } else {
                    (from_file, false)
                }
            };

            {
                let mut writer = BufWriter::new(writer_guard.deref_mut());
                if truncate {
                    writer.rewind()?;

                    for line in &foreign_entries {
                        writer.write_all(encode_entry(line).as_bytes())?;
                        writer.write_all("\n".as_bytes())?;
                    }
                } else {
                    writer.seek(SeekFrom::End(0))?;
                }
                for line in own_entries {
                    writer.write_all(encode_entry(line).as_bytes())?;
                    writer.write_all("\n".as_bytes())?;
                }
                writer.flush()?;
            }
            if truncate {
                let file = writer_guard.deref_mut();
                let file_len = file.stream_position()?;
                file.set_len(file_len)?;
            }

            let own_entries = self.entries.drain(self.len_on_disk..);
            foreign_entries.extend(own_entries);
            self.entries = foreign_entries;

            self.len_on_disk = self.entries.len();
        }
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
            entries: VecDeque::new(),
            file: None,
            len_on_disk: 0,
            session: None,
        })
    }

    /// Creates a new history with an associated history file.
    ///
    /// History file format: commands separated by new lines.
    /// If file exists file will be read otherwise empty file will be created.
    ///
    ///
    /// **Side effects:** creates all nested directories to the file
    ///
    pub fn with_file(capacity: usize, file: PathBuf) -> Result<Self> {
        let mut hist = Self::new(capacity)?;
        if let Some(base_dir) = file.parent() {
            std::fs::create_dir_all(base_dir)?;
        }
        hist.file = Some(file);
        hist.sync()?;
        Ok(hist)
    }

    // this history doesn't store any info except command line
    fn construct_entry(id: Option<HistoryItemId>, command_line: String) -> HistoryItem {
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
