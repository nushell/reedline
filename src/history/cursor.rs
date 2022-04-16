
impl HistoryCursor for FileBackedHistory {
    fn iter_chronologic(&self) -> Box<(dyn DoubleEndedIterator<Item = String> + '_)> {
        Box::new(self.entries.iter().map(|e| e.to_string()))
    }

    fn back(&mut self) {
        match self.query.clone() {
            HistoryNavigationQuery::Normal(_) => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                self.back_with_criteria(&|entry| entry.starts_with(&prefix));
            }
            HistoryNavigationQuery::SubstringSearch(substring) => {
                self.back_with_criteria(&|entry| entry.contains(&substring));
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
                self.forward_with_criteria(&|entry| entry.starts_with(&prefix));
            }
            HistoryNavigationQuery::SubstringSearch(substring) => {
                self.forward_with_criteria(&|entry| entry.contains(&substring));
            }
        }
    }

    fn string_at_cursor(&self) -> Option<String> {
        self.entries.get(self.cursor).cloned()
    }

    fn set_navigation(&mut self, navigation: HistoryNavigationQuery) {
        self.query = navigation;
        self.reset_cursor();
    }

    fn get_navigation(&self) -> HistoryNavigationQuery {
        self.query.clone()
    }

    fn query_entries(&self, search: &str) -> Vec<String> {
        self.iter_chronologic()
            .rev()
            .filter(|entry| entry.contains(search))
            .collect::<Vec<String>>()
    }

    fn max_values(&self) -> usize {
        self.entries.len()
    }

    /// Writes unwritten history contents to disk.
    ///
    /// If file would exceed `capacity` truncates the oldest entries.
    fn sync(&mut self) -> std::io::Result<()> {
        if let Some(fname) = &self.file {
            // The unwritten entries
            let own_entries = self.entries.range(self.len_on_disk..);

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
                let mut from_file = reader
                    .lines()
                    .map(|o| o.map(|i| decode_entry(&i)))
                    .collect::<Result<VecDeque<_>, _>>()?;
                if from_file.len() + own_entries.len() > self.capacity {
                    (
                        from_file.split_off(from_file.len() - (self.capacity - own_entries.len())),
                        true,
                    )
                } else {
                    (from_file, false)
                }
            };

            {
                let mut writer = BufWriter::new(writer_guard.deref_mut());
                if truncate {
                    writer.seek(SeekFrom::Start(0))?;

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

        self.reset_cursor();

        Ok(())
    }

    /// Reset the internal browsing cursor
    fn reset_cursor(&mut self) {
        self.cursor = self.entries.len();
    }
}