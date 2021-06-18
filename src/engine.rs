use crate::clip_buffer::{get_default_clipboard, Clipboard};
use crate::EditCommand;
use crate::{history::History, line_buffer::LineBuffer};
use crate::{
    history_search::{BasicSearch, BasicSearchCommand},
    line_buffer::InsertionPoint,
};

pub struct EditEngine {
    line_buffer: LineBuffer,
    cut_buffer: Box<dyn Clipboard>,
    // History
    history: History,
    history_search: Option<BasicSearch>, // This could be have more features in the future (fzf, configurable?)
}

impl Default for EditEngine {
    fn default() -> Self {
        EditEngine {
            line_buffer: LineBuffer::new(),
            cut_buffer: Box::new(get_default_clipboard()),
            history: History::default(),
            history_search: None,
        }
    }
}

impl EditEngine {
    pub fn set_history(&mut self, history: History) {
        self.history = history;
    }

    /// Get the current line of a multi-line edit [`LineBuffer`]
    pub fn insertion_line(&self) -> &str {
        self.line_buffer.get_buffer()
    }

    pub fn is_line_buffer_empty(&self) -> bool {
        self.line_buffer.is_empty()
    }

    /// Get the cursor position as understood by the underlying [`LineBuffer`]
    pub fn insertion_point(&self) -> InsertionPoint {
        self.line_buffer.insertion_point()
    }

    // TODO: Not sure if this should be part of the public interface
    pub fn history(&self) -> &History {
        &self.history
    }

    // TODO: Not sure if this should be part of the public interface
    pub fn history_search(&self) -> Option<BasicSearch> {
        self.history_search.clone()
    }

    pub fn clear_history_search(&mut self) {
        self.history_search = None;
    }

    // History interface
    // Note: Can this be a interface rather than a concrete struct
    pub fn numbered_chronological_history(&self) -> Vec<(usize, String)> {
        self.history
            .iter_chronologic()
            .cloned()
            .enumerate()
            .collect()
    }

    // HACK: Exposing this method for now, Will need to figure out a proper way of handling
    // history related stuff later down the road. Then this will become hidden once again
    pub fn update_buffer_with_history(&mut self) {
        if let Some(Some((history_index, _))) = self.history_search().map(|hs| hs.result) {
            self.line_buffer.set_buffer(
                self.history()
                    .get_nth_newest(history_index)
                    .unwrap()
                    .clone(),
            );
        }
    }

    pub fn run_edit_commands(&mut self, commands: &[EditCommand]) {
        // Handle command for history inputs
        if self.has_history() {
            self.run_history_commands(commands);
            return;
        }

        // // Vim mode transformations
        // let commands = match self.edit_mode {
        //     EditMode::ViNormal => self.vi_engine.handle(commands),
        //     _ => commands.into(),
        // };

        // Run the commands over the edit buffer
        for command in commands {
            match command {
                EditCommand::MoveToStart => self.move_to_start(),
                EditCommand::MoveToEnd => {
                    self.move_to_end();
                }
                EditCommand::MoveLeft => self.move_left(),
                EditCommand::MoveRight => self.move_right(),
                EditCommand::MoveWordLeft => {
                    self.move_word_left();
                }
                EditCommand::MoveWordRight => {
                    self.move_word_right();
                }
                EditCommand::InsertChar(c) => {
                    self.insert_char(*c);
                }
                EditCommand::Backspace => {
                    self.backspace();
                }
                EditCommand::Delete => {
                    self.delete();
                }
                EditCommand::BackspaceWord => {
                    self.backspace_word();
                }
                EditCommand::DeleteWord => {
                    self.delete_word();
                }
                EditCommand::Clear => {
                    self.clear();
                }
                EditCommand::AppendToHistory => {
                    self.append_to_history();
                }
                EditCommand::PreviousHistory => {
                    self.previous_history();
                }
                EditCommand::NextHistory => {
                    self.next_history();
                }
                EditCommand::SearchHistory => {
                    self.search_history();
                }
                EditCommand::CutFromStart => {
                    self.cut_from_start();
                }
                EditCommand::CutToEnd => {
                    self.cut_from_end();
                }
                EditCommand::CutWordLeft => {
                    self.cut_word_left();
                }
                EditCommand::CutWordRight => {
                    self.cut_word_right();
                }
                EditCommand::PasteCutBuffer => {
                    self.insert_cut_buffer();
                }
                EditCommand::UppercaseWord => {
                    self.uppercase_word();
                }
                EditCommand::LowercaseWord => {
                    self.lowercase_word();
                }
                EditCommand::CapitalizeChar => {
                    self.capitalize_char();
                }
                EditCommand::SwapWords => {
                    self.swap_words();
                }
                EditCommand::SwapGraphemes => {
                    self.swap_graphemes();
                }
                EditCommand::EnterViInsert => {
                    panic!("Should not have happened");
                }
                EditCommand::EnterViNormal => {
                    panic!("Should not have happened");
                }
                _ => {}
            }

            // TODO: This seems a bit hacky, probabaly think of another approach
            // Clean-up after commands run
            for command in commands {
                match command {
                    EditCommand::PreviousHistory => {}
                    EditCommand::NextHistory => {}
                    _ => {
                        // Clean up the old prefix used for history search
                        if self.history.history_prefix.is_some() {
                            self.history.history_prefix = None;
                        }
                    }
                }
            }
        }
    }

    fn move_to_start(&mut self) {
        self.line_buffer.move_to_start()
    }

    fn move_to_end(&mut self) {
        self.line_buffer.move_to_end()
    }

    fn move_left(&mut self) {
        self.line_buffer.move_left()
    }

    fn move_right(&mut self) {
        self.line_buffer.move_right()
    }

    fn move_word_left(&mut self) {
        self.line_buffer.move_word_left();
    }

    fn move_word_right(&mut self) {
        self.line_buffer.move_right()
    }

    fn clear_to_end(&mut self) {
        self.line_buffer.clear_to_end()
    }

    fn clear_to_insertion_point(&mut self) {
        self.line_buffer.clear_to_insertion_point()
    }

    /// Set the cursor position as understood by the underlying [`LineBuffer`] for the current line
    fn set_insertion_point(&mut self, pos: usize) {
        let mut insertion_point = self.line_buffer.insertion_point();
        insertion_point.offset = pos;

        self.line_buffer.set_insertion_point(insertion_point)
    }

    fn clear_range<R>(&mut self, range: R)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.line_buffer.clear_range(range)
    }

    fn insert_char(&mut self, c: char) {
        let insertion_point = self.line_buffer.insertion_point();
        self.line_buffer.insert_char(insertion_point, c);
    }

    /// Reset the [`LineBuffer`] to be a line specified by `buffer`
    fn set_buffer(&mut self, buffer: String) {
        self.line_buffer.set_buffer(buffer)
    }

    fn backspace(&mut self) {
        let left_index = self.line_buffer.grapheme_left_index();
        let insertion_offset = self.insertion_point().offset;
        if left_index < insertion_offset {
            self.clear_range(left_index..insertion_offset);
            self.set_insertion_point(left_index);
        }
    }

    fn delete(&mut self) {
        let right_index = self.line_buffer.grapheme_right_index();
        let insertion_offset = self.insertion_point().offset;
        if right_index > insertion_offset {
            self.clear_range(insertion_offset..right_index);
        }
    }

    fn backspace_word(&mut self) {
        let left_word_index = self.line_buffer.word_left_index();
        self.clear_range(left_word_index..self.insertion_point().offset);
        self.set_insertion_point(left_word_index);
    }

    fn delete_word(&mut self) {
        let right_word_index = self.line_buffer.word_right_index();
        self.clear_range(self.insertion_point().offset..right_word_index);
    }

    fn clear(&mut self) {
        self.line_buffer.clear();
        self.set_insertion_point(0);
    }
    fn cut_from_start(&mut self) {
        let insertion_offset = self.insertion_point().offset;
        if insertion_offset > 0 {
            self.cut_buffer
                .set(&self.line_buffer.get_buffer()[..insertion_offset]);
            self.clear_to_insertion_point();
        }
    }

    fn cut_from_end(&mut self) {
        let cut_slice = &self.line_buffer.get_buffer()[self.insertion_point().offset..];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice);
            self.clear_to_end();
        }
    }

    fn cut_word_left(&mut self) {
        let insertion_offset = self.insertion_point().offset;
        let left_index = self.line_buffer.word_left_index();
        if left_index < insertion_offset {
            let cut_range = left_index..insertion_offset;
            self.cut_buffer
                .set(&self.line_buffer.get_buffer()[cut_range.clone()]);
            self.clear_range(cut_range);
            self.set_insertion_point(left_index);
        }
    }

    fn cut_word_right(&mut self) {
        let insertion_offset = self.insertion_point().offset;
        let right_index = self.line_buffer.word_right_index();
        if right_index > insertion_offset {
            let cut_range = insertion_offset..right_index;
            self.cut_buffer
                .set(&self.line_buffer.get_buffer()[cut_range.clone()]);
            self.clear_range(cut_range);
        }
    }

    fn insert_cut_buffer(&mut self) {
        let insertion_offset = self.insertion_point().offset;
        let cut_buffer = self.cut_buffer.get();
        self.line_buffer.insert_str(insertion_offset, &cut_buffer);
        self.set_insertion_point(insertion_offset + cut_buffer.len());
    }

    fn uppercase_word(&mut self) {
        let insertion_offset = self.insertion_point().offset;
        let right_index = self.line_buffer.word_right_index();
        if right_index > insertion_offset {
            let change_range = insertion_offset..right_index;
            let uppercased = self.insertion_line()[change_range.clone()].to_uppercase();
            self.line_buffer.replace_range(change_range, &uppercased);
            self.line_buffer.move_word_right();
        }
    }

    fn lowercase_word(&mut self) {
        let insertion_offset = self.insertion_point().offset;
        let right_index = self.line_buffer.word_right_index();
        if right_index > insertion_offset {
            let change_range = insertion_offset..right_index;
            let lowercased = self.insertion_line()[change_range.clone()].to_lowercase();
            self.line_buffer.replace_range(change_range, &lowercased);
            self.line_buffer.move_word_right();
        }
    }

    fn capitalize_char(&mut self) {
        if self.line_buffer.on_whitespace() {
            self.line_buffer.move_word_right();
            self.line_buffer.move_word_left();
        }
        let insertion_offset = self.insertion_point().offset;
        let right_index = self.line_buffer.grapheme_right_index();
        if right_index > insertion_offset {
            let change_range = insertion_offset..right_index;
            let uppercased = self.insertion_line()[change_range.clone()].to_uppercase();
            self.line_buffer.replace_range(change_range, &uppercased);
            self.line_buffer.move_word_right();
        }
    }

    fn swap_words(&mut self) {
        let old_insertion_point = self.insertion_point().offset;
        self.line_buffer.move_word_right();
        let word_2_end = self.insertion_point().offset;
        self.line_buffer.move_word_left();
        let word_2_start = self.insertion_point().offset;
        self.line_buffer.move_word_left();
        let word_1_start = self.insertion_point().offset;
        let word_1_end = self.line_buffer.word_right_index();

        if word_1_start < word_1_end && word_1_end < word_2_start && word_2_start < word_2_end {
            let insertion_line = self.insertion_line();
            let word_1 = insertion_line[word_1_start..word_1_end].to_string();
            let word_2 = insertion_line[word_2_start..word_2_end].to_string();
            self.line_buffer
                .replace_range(word_2_start..word_2_end, &word_1);
            self.line_buffer
                .replace_range(word_1_start..word_1_end, &word_2);
            self.set_insertion_point(word_2_end);
        } else {
            self.set_insertion_point(old_insertion_point);
        }
    }

    fn swap_graphemes(&mut self) {
        let insertion_offset = self.insertion_point().offset;

        if insertion_offset == 0 {
            self.line_buffer.move_right()
        } else if insertion_offset == self.line_buffer.get_buffer().len() {
            self.line_buffer.move_left()
        }
        let grapheme_1_start = self.line_buffer.grapheme_left_index();
        let grapheme_2_end = self.line_buffer.grapheme_right_index();

        if grapheme_1_start < insertion_offset && grapheme_2_end > insertion_offset {
            let grapheme_1 = self.insertion_line()[grapheme_1_start..insertion_offset].to_string();
            let grapheme_2 = self.insertion_line()[insertion_offset..grapheme_2_end].to_string();
            self.line_buffer
                .replace_range(insertion_offset..grapheme_2_end, &grapheme_1);
            self.line_buffer
                .replace_range(grapheme_1_start..insertion_offset, &grapheme_2);
            self.set_insertion_point(grapheme_2_end);
        } else {
            self.set_insertion_point(insertion_offset);
        }
    }

    /// Dispatches the applicable [`EditCommand`] actions for editing the history search string.
    ///
    /// Only modifies internal state, does not perform regular output!
    fn run_history_commands(&mut self, commands: &[EditCommand]) {
        for command in commands {
            match command {
                EditCommand::InsertChar(c) => {
                    let search = self
                        .history_search
                        .as_mut()
                        .expect("couldn't get history_search as mutable"); // We checked it is some
                    search.step(BasicSearchCommand::InsertChar(*c), &self.history);
                }
                EditCommand::Backspace => {
                    let search = self
                        .history_search
                        .as_mut()
                        .expect("couldn't get history_search as mutable"); // We checked it is some
                    search.step(BasicSearchCommand::Backspace, &self.history);
                }
                EditCommand::SearchHistory => {
                    let search = self
                        .history_search
                        .as_mut()
                        .expect("couldn't get history_search as mutable"); // We checked it is some
                    search.step(BasicSearchCommand::Next, &self.history);
                }
                EditCommand::MoveRight => {
                    // Ignore move right, it is currently emited with InsertChar
                }
                // Leave history search otherwise
                _ => self.history_search = None,
            }
        }
    }

    fn append_to_history(&mut self) {
        self.history.append(self.insertion_line().to_string());
    }

    fn previous_history(&mut self) {
        if self.history.history_prefix.is_none() {
            let buffer = self.line_buffer.get_buffer();
            self.history.history_prefix = Some(buffer.to_owned());
        }

        if let Some(history_entry) = self.history.go_back_with_prefix() {
            let new_buffer = history_entry.to_string();
            self.set_buffer(new_buffer);
            self.move_to_end();
        }
    }

    fn next_history(&mut self) {
        if self.history.history_prefix.is_none() {
            let buffer = self.line_buffer.get_buffer();
            self.history.history_prefix = Some(buffer.to_owned());
        }

        if let Some(history_entry) = self.history.go_forward_with_prefix() {
            let new_buffer = history_entry.to_string();
            self.set_buffer(new_buffer);
            self.move_to_end();
        }
    }

    fn search_history(&mut self) {
        self.history_search = Some(BasicSearch::new(self.insertion_line().to_string()));
    }

    fn has_history(&self) -> bool {
        self.history_search.is_some()
    }
}
