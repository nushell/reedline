use super::{
    base::CommandLineSearch, History, HistoryItem, HistoryItemId, SearchDirection, SearchQuery,
};
use crate::{
    result::{ReedlineError, ReedlineErrorVariants},
    HistorySessionId, Result,
};

#[cfg(target_os = "android")]
use std::ops::{Deref, DerefMut};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write},
    path::PathBuf,
};

/// Default size of the [`FileBackedHistory`] used when calling [`FileBackedHistory::default()`]
pub const HISTORY_SIZE: usize = 1000;
pub const NEWLINE_ESCAPE: &str = "<\\n>";

/// Handler for File Backed history archival
pub trait FileBackedArchivalHandler: Send + 'static {
    /// Archival callback, use when the history passes the max capacity, `input`
    /// will contain all history entries, the handler is responsible to remove
    /// entries in such a way that fewer than `max_capacity` entries are left.
    /// The parameter `all` indicates that all entries should be removed instead,
    /// this is usually set when there are no file backing the history and
    /// the session is closing.
    fn archive(&mut self, input: &mut VecDeque<String>) -> Result<()>;
    /// Similar to archive, but it's responsible to consume all entries.
    fn archive_all(&mut self, input: &mut VecDeque<String>) -> Result<()>;
}
impl<F: FnMut(&mut VecDeque<String>, bool) -> Result<()> + Send + 'static> FileBackedArchivalHandler
    for F
{
    fn archive(&mut self, input: &mut VecDeque<String>) -> Result<()> {
        self(input, false)
    }

    fn archive_all(&mut self, input: &mut VecDeque<String>) -> Result<()> {
        self(input, true)
    }
}

/// Stateful history that allows up/down-arrow browsing with an internal cursor.
///
/// Can optionally be associated with a newline separated history file using the [`FileBackedHistory::with_file()`] constructor.
/// Similar to bash's behavior without HISTTIMEFORMAT.
/// (See <https://www.gnu.org/software/bash/manual/html_node/Bash-History-Facilities.html>)
/// If the history is associated to a file all new changes within a given history capacity will be written to disk when History is dropped.
pub struct FileBackedHistory {
    capacity: usize,
    entries: VecDeque<String>,
    file: Option<PathBuf>,
    len_on_disk: usize, // Keep track what was previously written to disk
    session: Option<HistorySessionId>,
    archival_handler: Option<Box<dyn FileBackedArchivalHandler>>,
}

impl Default for FileBackedHistory {
    /// Creates an in-memory [`History`] with a maximal capacity of [`HISTORY_SIZE`].
    ///
    /// To create a [`History`] that is synchronized with a file use [`FileBackedHistory::with_file()`]
    fn default() -> Self {
        Self::new(HISTORY_SIZE).expect("HISTORY_SIZE is a valid capacity")
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
        let identical = self.entries.back() == Some(&entry);
        let is_empty = entry.is_empty();
        let entry_id = match (identical, is_empty, self.capacity) {
            // Don't append if the preceding value is identical or the string empty
            (true, _, _) | (_, true, _) => None,
            // empty capacity, call archiver for all new entries
            (_, _, 0) => {
                if let Some(archiver) = &mut self.archival_handler {
                    let mut new_entry = [entry.clone()].into_iter().collect();
                    archiver.archive_all(&mut new_entry)?;
                }
                None
            }
            (false, false, 1..) => {
                if self.entries.len() >= self.capacity {
                    self.remove_one_old_entry()?;
                }
                self.entries.push_back(entry.to_string());
                Some(HistoryItemId::new((self.entries.len() - 1) as i64))
            }
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
            if let Some(base_dir) = fname.parent() {
                std::fs::create_dir_all(base_dir)?;
            }

            let mut opts = OpenOptions::new();
            opts.create(true).write(true).read(true).truncate(false);

            cfg_select! {
                target_os = "android" => {
                    let mut f_lock = fd_lock::RwLock::new(opts.open(fname)?);
                    let mut writer_guard = f_lock.write()?;
                    let file = writer_guard.deref_mut();
                },
                _ => {
                    let mut file = opts.open(fname)?;
                    file.lock()?;
                    let file = &mut file;
                }
            }

            // read the contents of the file
            self.read_history_file(file)?;
            let disk_is_empty = self.len_on_disk == 0;
            let mut truncate_file = false;

            // if more entries than capacity, call the archiver (if any) to
            // free space
            if self.entries.len() > self.capacity {
                truncate_file |= self.try_archive_old_entries()?;
            }
            // if the archiver didn't remove enough entries, just discard them
            if self.entries.len() > self.capacity {
                let skip_entries = self.entries.len() - self.capacity;
                truncate_file |= self.discard_old_entries(skip_entries)?;
            }

            let truncate = truncate_file && !disk_is_empty;
            self.write_history_file(file, truncate)?;
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
            archival_handler: None,
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

    /// Set the archive handler that will be executed each time the file
    /// exceeds its capacity, or in case of inevitable lost of history
    /// entries, such as when closing a in memory database.
    pub fn set_archival_handler(&mut self, handler: impl FileBackedArchivalHandler) {
        self.archival_handler = Some(Box::new(handler));
    }

    // read history entries from file
    fn read_history_file(&mut self, file: &File) -> std::io::Result<()> {
        let foreign_entries = BufReader::new(file)
            .lines()
            .map(|o| o.map(|i| decode_entry(&i)))
            .collect::<std::io::Result<VecDeque<_>>>()?;
        let foreign_entries_len = foreign_entries.len();

        // TODO use VecDeque::extend_front once available
        self.entries = foreign_entries
            .into_iter()
            .chain(self.entries.drain(..).skip(self.len_on_disk))
            .collect();
        self.len_on_disk = foreign_entries_len;
        Ok(())
    }

    // remove entries, so it will have space to add at least one entry
    fn remove_one_old_entry(&mut self) -> Result<()> {
        if self.archival_handler.is_none() {
            // if no archive handler, just remove the old entry
            self.entries.pop_front();
            self.len_on_disk = self.len_on_disk.saturating_sub(1);
            return Ok(());
        }
        // have a archive handler and file, means we need to read the file
        // before archiving entries
        #[cfg(target_os = "android")]
        let mut _file_lock = None;
        let mut file = None;
        if let Some(fname) = self.file.clone() {
            let mut opts = OpenOptions::new();
            opts.create(true).write(true).read(true).truncate(false);
            cfg_select! {
                target_os = "android" => {
                    _file_lock = Some(fd_lock::RwLock::new(opts.open(fname)?));
                    let writer_guard = _file_lock.as_mut().unwrap().write()?;
                    self.read_history_file(writer_guard.deref())?;
                    file = Some(writer_guard);
                },
                _ => {
                    let new_file = opts.open(fname)?;
                    new_file.lock()?;
                    self.read_history_file(&new_file)?;
                    file = Some(new_file);
                }
            }
        }
        let mut truncate_file = false;

        if self.entries.len() >= self.capacity {
            truncate_file |= self.try_archive_old_entries()?;
        }

        // if the archiver didn't remove enough entries, just discard them
        if self.entries.len() >= self.capacity {
            let skip_entries = self.entries.len() - self.capacity;
            truncate_file |= self.discard_old_entries(skip_entries)?;
        }

        if let Some(mut file) = file {
            // if the archival process truncated the file, we need to update it
            // so it doesn't get archived again by this or other session
            if truncate_file {
                cfg_select! {
                    target_os = "android" => {
                        self.overwrite_file(BufWriter::new(file.deref_mut()))?;
                    },
                    _ => {
                        self.overwrite_file(BufWriter::new(&mut file))?;
                    }
                }
            }
        }

        Ok(())
    }

    // if archive is available, archive old entries, return true if the
    // file (in disk) needs to be truncated
    fn try_archive_old_entries(&mut self) -> Result<bool> {
        if self.entries.is_empty() {
            return Ok(false);
        }
        let Some(archiver) = &mut self.archival_handler else {
            return Ok(false);
        };
        let len_on_disk_before = self.len_on_disk;
        let len_before = self.entries.len();
        archiver.archive(&mut self.entries)?;
        if len_before > self.entries.len() {
            self.len_on_disk = self
                .len_on_disk
                .saturating_sub(len_before - self.entries.len());
        }
        Ok(len_on_disk_before != self.len_on_disk)
    }

    // discard old entries, return true if the file (in disk) needs to be truncated
    fn discard_old_entries(&mut self, num: usize) -> Result<bool> {
        // TODO replace this with VecDeque::retain_back once available
        let len_on_disk_before = self.len_on_disk;
        self.entries = self.entries.drain(num..).collect();
        self.len_on_disk = self.len_on_disk.saturating_sub(num);
        Ok(len_on_disk_before != self.len_on_disk)
    }

    // write history entries to file
    fn write_history_file(&mut self, file: &mut File, truncate: bool) -> std::io::Result<()> {
        if truncate {
            // truncation happen, overwrite the whole file
            self.overwrite_file(BufWriter::new(file))?;
        } else {
            // just add new entries to the end
            self.append_file(BufWriter::new(file))?;
        }
        Ok(())
    }

    // rewrite the whole file
    fn overwrite_file(&mut self, mut writer: BufWriter<&mut File>) -> std::io::Result<()> {
        writer.rewind()?;

        for line in &self.entries {
            writer.write_all(encode_entry(line).as_bytes())?;
            writer.write_all("\n".as_bytes())?;
        }
        writer.flush()?;

        let file_len = writer.get_mut().stream_position()?;
        writer.get_ref().set_len(file_len)?;
        self.len_on_disk = self.entries.len();
        Ok(())
    }

    // add entries to the end of the file
    fn append_file(&mut self, mut writer: impl Write + Seek) -> std::io::Result<()> {
        writer.seek(SeekFrom::End(0))?;
        for line in self.entries.iter().skip(self.len_on_disk) {
            writer.write_all(encode_entry(line).as_bytes())?;
            writer.write_all("\n".as_bytes())?;
        }
        writer.flush()?;
        self.len_on_disk = self.entries.len();
        Ok(())
    }

    /// Close the history file, sync/archive all pending entries. Using the Drop
    /// implementation also calls this function, but doing so will silently
    /// ignore any errors that may happen
    pub fn close(self) -> Result<()> {
        // NOTE not using ManuallyDrop will trigger close_inner twice
        std::mem::ManuallyDrop::new(self).close_inner()
    }

    fn close_inner(&mut self) -> Result<()> {
        match (&mut self.archival_handler, &self.file) {
            // no file to store entries, just call archiver to store all of it
            (Some(archiver), None) => {
                if !self.entries.is_empty() {
                    archiver.archive_all(&mut self.entries)?;
                }
                Ok(())
            }
            // just sync to disk
            (None, _) | (Some(_), Some(_)) => Ok(self.sync()?),
        }
    }
}

impl Drop for FileBackedHistory {
    /// On drop the content of the [`History`] will be written to the file if specified via [`FileBackedHistory::with_file()`].
    fn drop(&mut self) {
        // NOTE if you change this function, please also change FileBackedHistory::close
        let _res = self.close_inner();
    }
}

#[cfg(test)]
mod test {
    use crate::{FileBackedHistory, History, HistoryTimestamp, SearchQuery};
    use std::{
        collections::VecDeque,
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    fn insert_and_test(file: &mut FileBackedHistory, id: i64, result: &[String]) {
        let mut item = crate::HistoryItem::from_command_line(&format!("line{id}"));
        item.start_timestamp = Some(HistoryTimestamp::from_millis(id));
        file.save(item).unwrap();
        assert_eq!(
            file.entries
                .iter()
                .map(|x| x.to_owned())
                .collect::<Vec<_>>(),
            result
        );
    }

    fn reopen_and_recheck_file(file: FileBackedHistory, len: usize, expected: &[String]) {
        let Some(file_path) = file.file.clone() else {
            return;
        };
        // this will force file to sync
        file.close().unwrap();
        // if file backed, reopen/reload the file and check the content
        let entries: Vec<_> = FileBackedHistory::with_file(len, file_path)
            .unwrap()
            .search(SearchQuery::everything(
                crate::SearchDirection::Forward,
                None,
            ))
            .unwrap()
            .into_iter()
            .map(|entry| entry.command_line)
            .collect();
        assert_eq!(&entries, expected);
    }

    // test adding exactly the capacity, then closing the session
    // if have file, it will just write this file
    // if doesn't have file, it will archive the entries.
    fn test_archive_full_capacity_and_close(capacity: usize, file_path: Option<PathBuf>) {
        let mut file = if let Some(path) = file_path {
            FileBackedHistory::with_file(capacity, path).unwrap()
        } else {
            FileBackedHistory::new(capacity).unwrap()
        };
        let archived = Arc::new(Mutex::new(vec![]));
        let moved_archived = Arc::clone(&archived);
        let has_file = file.file.is_some();
        file.set_archival_handler(move |entries: &mut VecDeque<_>, all: bool| {
            if has_file {
                // if have a file, it should just write the file not call archiver
                unreachable!()
            }
            // it should only be called when closing/dropping the history
            assert!(all);
            moved_archived.lock().unwrap().extend(entries.drain(..));
            Ok(())
        });

        let expected_results: Vec<String> = (0..capacity).map(|i| format!("line{i}")).collect();
        for i in 0..capacity {
            insert_and_test(&mut file, i as i64, &expected_results[0..i + 1]);
        }

        if file.file.is_some() {
            // check that closing the file will write the file to disk
            reopen_and_recheck_file(file, capacity, &expected_results);
        } else {
            // check that dropping file will trigger the archival
            file.close().unwrap();
            assert_eq!(&*archived.lock().unwrap(), &expected_results);
        }
    }

    // test the archival process by triggering it multiple times
    fn test_archive_multiple_calls(capacity: usize, file_path: Option<PathBuf>) {
        if capacity < 3 {
            panic!("test don't work with very small capacity")
        }
        let mut file = if let Some(path) = file_path {
            FileBackedHistory::with_file(capacity, path).unwrap()
        } else {
            FileBackedHistory::new(capacity).unwrap()
        };
        let archived = Arc::new(Mutex::new(vec![]));
        let moved_archived = Arc::clone(&archived);
        file.set_archival_handler(move |entries: &mut VecDeque<_>, all| {
            if entries.is_empty() {
                return Ok(());
            }
            // consume all leaving only 1 behind
            moved_archived
                .lock()
                .unwrap()
                .extend(entries.drain(..entries.len() - if all { 0 } else { 1 }));
            Ok(())
        });

        let mut expected_iter = (0..).map(|i| (i, format!("line{i}")));

        let mut expected_file: Vec<_> = vec![];
        let mut expected_archived: Vec<_> = vec![];

        // first fill the history to capacity
        for (i, expected) in (&mut expected_iter).take(capacity) {
            expected_file.push(expected.clone());
            insert_and_test(&mut file, i, &expected_file);
            assert_eq!(&*archived.lock().unwrap(), &expected_archived);
        }

        // insert one more, this needs to call archiver
        expected_archived.extend(expected_file.drain(0..expected_file.len() - 1));
        let (next_idx, next_entry) = expected_iter.next().unwrap();
        expected_file.push(next_entry);
        insert_and_test(&mut file, next_idx, &expected_file);
        // now the database have 2 entries, the one left behind by the archiver
        // and the other that was added.
        assert_eq!(&*archived.lock().unwrap(), &expected_archived);

        // sync to test file write/load
        file.sync().unwrap();

        // Add more entries, so we go to capacity again
        for (i, expected) in (&mut expected_iter).take(capacity - 2) {
            expected_file.push(expected.clone());
            insert_and_test(&mut file, i, &expected_file);
        }

        // add one more entry to trigger the archiver again
        expected_archived.extend(expected_file.drain(0..expected_file.len() - 1));
        let (next_idx, next_entry) = expected_iter.next().unwrap();
        expected_file.push(next_entry);
        insert_and_test(&mut file, next_idx, &expected_file);
        assert_eq!(&*archived.lock().unwrap(), &expected_archived);

        if file.file.is_some() {
            // check that closing the file will write the file to disk
            reopen_and_recheck_file(file, capacity, &expected_file);
        } else {
            // check that closing the file will trigger the archival
            expected_archived.extend(expected_file);
            file.close().unwrap();
            assert_eq!(&*archived.lock().unwrap(), &expected_archived);
        }
    }

    fn test_archive_empty_capacity(file_path: Option<PathBuf>) {
        // if the file have zero capacity, then all new entries should call
        // the archiver
        let mut file = if let Some(path) = file_path {
            FileBackedHistory::with_file(0, path).unwrap()
        } else {
            FileBackedHistory::new(0).unwrap()
        };
        let archived = Arc::new(Mutex::new(vec![]));
        let moved_archived = Arc::clone(&archived);
        file.set_archival_handler(move |entries: &mut VecDeque<_>, all: bool| {
            assert!(all);
            assert_eq!(entries.len(), 1);
            moved_archived.lock().unwrap().extend(entries.drain(..));
            Ok(())
        });

        let expected_results: Vec<String> = (0..4).map(|i| format!("line{i}")).collect();
        for i in 0..expected_results.len() {
            insert_and_test(&mut file, i.try_into().unwrap(), &[]);
            assert_eq!(&*archived.lock().unwrap(), &expected_results[0..i + 1]);
        }
    }

    #[test]
    fn test_archive_full_capacity_and_close_in_memory() {
        test_archive_full_capacity_and_close(4, None);
    }

    #[test]
    fn test_archive_full_capacity_and_close_in_file() {
        let tmp_file = tempfile::Builder::new()
            .make(|path| std::fs::File::create(path))
            .unwrap();
        test_archive_full_capacity_and_close(4, Some(tmp_file.path().to_path_buf()));
    }

    #[test]
    fn archive_hook_in_memory() {
        test_archive_multiple_calls(4, None);
    }

    #[test]
    fn archive_hook_in_file() {
        let tmp_file = tempfile::Builder::new()
            .make(|path| std::fs::File::create(path))
            .unwrap();
        test_archive_multiple_calls(4, Some(tmp_file.path().to_path_buf()));
    }

    #[test]
    fn test_archive_empty_capacity_in_memory() {
        test_archive_empty_capacity(None);
    }

    #[test]
    fn test_archive_empty_capacity_in_file() {
        let tmp_file = tempfile::Builder::new()
            .make(|path| std::fs::File::create(path))
            .unwrap();
        test_archive_empty_capacity(Some(tmp_file.path().to_path_buf()));
    }
}
