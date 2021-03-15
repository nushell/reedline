use std::collections::VecDeque;

use crate::line_buffer::LineBuffer;

const HISTORY_SIZE: usize = 100;

pub enum EditCommand {
    MoveToStart,
    MoveToEnd,
    MoveLeft,
    MoveRight,
    MoveWordLeft,
    MoveWordRight,
    InsertChar(char),
    Backspace,
    Delete,
    AppendToHistory,
    PreviousHistory,
    NextHistory,
    Clear,
    CutFromStart,
    CutToEnd,
    CutWordLeft,
    CutWordRight,
    InsertCutBuffer,
}

pub struct Engine {
    line_buffer: LineBuffer,

    // Cut buffer
    cut_buffer: String,

    // History
    history: VecDeque<String>,
    history_cursor: i64,
    has_history: bool,
}

impl Engine {
    pub fn new() -> Engine {
        let history = VecDeque::with_capacity(HISTORY_SIZE);
        let history_cursor = -1i64;
        let has_history = false;
        let cut_buffer = String::new();

        Engine {
            line_buffer: LineBuffer::new(),
            cut_buffer,
            history,
            history_cursor,
            has_history,
        }
    }

    pub fn run_edit_commands(&mut self, commands: &[EditCommand]) {
        for command in commands {
            match command {
                EditCommand::MoveToStart => self.line_buffer.set_insertion_point(0),
                EditCommand::MoveToEnd => {
                    self.line_buffer.move_to_end();
                }
                EditCommand::MoveLeft => self.line_buffer.dec_insertion_point(),
                EditCommand::MoveRight => self.line_buffer.inc_insertion_point(),
                EditCommand::MoveWordLeft => {
                    self.line_buffer.move_word_left();
                }
                EditCommand::MoveWordRight => {
                    self.line_buffer.move_word_right();
                }
                EditCommand::InsertChar(c) => {
                    let insertion_point = self.line_buffer.get_insertion_point();
                    self.line_buffer.insert_char(insertion_point, *c)
                }
                EditCommand::Backspace => {
                    let insertion_point = self.get_insertion_point();
                    if insertion_point == self.get_buffer_len() && !self.is_empty() {
                        // buffer.dec_insertion_point();
                        self.pop();
                    } else if insertion_point < self.get_buffer_len()
                        && insertion_point > 0
                        && !self.is_empty()
                    {
                        self.dec_insertion_point();
                        let insertion_point = self.get_insertion_point();
                        self.remove_char(insertion_point);
                    }
                }
                EditCommand::Delete => {
                    let insertion_point = self.get_insertion_point();
                    if insertion_point < self.get_buffer_len() && !self.is_empty() {
                        self.remove_char(insertion_point);
                    }
                }
                EditCommand::Clear => {
                    self.line_buffer.clear();
                    self.set_insertion_point(0);
                }
                EditCommand::AppendToHistory => {
                    if self.history.len() + 1 == HISTORY_SIZE {
                        // History is "full", so we delete the oldest entry first,
                        // before adding a new one.
                        self.history.pop_back();
                    }
                    self.history.push_front(String::from(self.get_buffer()));
                    self.has_history = true;
                    // reset the history cursor - we want to start at the bottom of the
                    // history again.
                    self.history_cursor = -1;
                }
                EditCommand::PreviousHistory => {
                    if self.has_history && self.history_cursor < (self.history.len() as i64 - 1) {
                        self.history_cursor += 1;
                        let history_entry = self
                            .history
                            .get(self.history_cursor as usize)
                            .unwrap()
                            .clone();
                        self.set_buffer(history_entry.clone());
                        self.move_to_end();
                    }
                }
                EditCommand::NextHistory => {
                    if self.history_cursor >= 0 {
                        self.history_cursor -= 1;
                    }
                    let new_buffer = if self.history_cursor < 0 {
                        String::new()
                    } else {
                        // We can be sure that we always have an entry on hand, that's why
                        // unwrap is fine.
                        self.history
                            .get(self.history_cursor as usize)
                            .unwrap()
                            .clone()
                    };

                    self.set_buffer(new_buffer.clone());
                    self.move_to_end();
                }
                EditCommand::CutFromStart => {
                    if self.get_insertion_point() > 0 {
                        let cut_slice = self.get_buffer()[..self.get_insertion_point()].to_string();

                        self.cut_buffer.replace_range(.., &cut_slice);
                        self.clear_to_insertion_point();
                    }
                }
                EditCommand::CutToEnd => {
                    let cut_slice = &self.get_buffer()[self.get_insertion_point()..].to_string();
                    if !cut_slice.is_empty() {
                        self.cut_buffer.replace_range(.., &cut_slice);
                        self.clear_to_end();
                    }
                }
                EditCommand::CutWordLeft => {
                    let old_insertion_point = self.get_insertion_point();

                    self.move_word_left();

                    let cut_slice = self.get_buffer()
                        [self.get_insertion_point()..old_insertion_point]
                        .to_string();

                    if self.get_insertion_point() < old_insertion_point {
                        self.cut_buffer.replace_range(.., &cut_slice);
                        self.clear_range(self.get_insertion_point()..old_insertion_point);
                    }
                }
                EditCommand::CutWordRight => {
                    let old_insertion_point = self.get_insertion_point();

                    self.move_word_right();

                    let cut_slice = self.get_buffer()
                        [old_insertion_point..self.get_insertion_point()]
                        .to_string();

                    if self.get_insertion_point() > old_insertion_point {
                        self.cut_buffer.replace_range(.., &cut_slice);
                        self.clear_range(old_insertion_point..self.get_insertion_point());
                        self.set_insertion_point(old_insertion_point);
                    }
                }
                EditCommand::InsertCutBuffer => {
                    let cut_buffer = self.cut_buffer.clone();
                    self.insert_str(self.get_insertion_point(), &cut_buffer);
                    self.set_insertion_point(self.get_insertion_point() + self.cut_buffer.len());
                }
            }
        }
    }

    pub fn set_insertion_point(&mut self, pos: usize) {
        self.line_buffer.set_insertion_point(pos)
    }

    pub fn get_insertion_point(&self) -> usize {
        self.line_buffer.get_insertion_point()
    }

    pub fn get_buffer(&self) -> &str {
        &self.line_buffer.get_buffer()
    }

    pub fn set_buffer(&mut self, buffer: String) {
        self.line_buffer.set_buffer(buffer)
    }

    pub fn move_to_end(&mut self) -> usize {
        self.line_buffer.move_to_end()
    }

    pub fn dec_insertion_point(&mut self) {
        self.line_buffer.dec_insertion_point()
    }

    pub fn get_buffer_len(&self) -> usize {
        self.line_buffer.get_buffer_len()
    }

    pub fn remove_char(&mut self, pos: usize) -> char {
        self.line_buffer.remove_char(pos)
    }

    pub fn insert_str(&mut self, idx: usize, string: &str) {
        self.line_buffer.insert_str(idx, string)
    }

    pub fn is_empty(&self) -> bool {
        self.line_buffer.is_empty()
    }

    pub fn pop(&mut self) -> Option<char> {
        self.line_buffer.pop()
    }

    pub fn clear_to_end(&mut self) {
        self.line_buffer.clear_to_end()
    }

    pub fn clear_to_insertion_point(&mut self) {
        self.line_buffer.clear_to_insertion_point()
    }

    pub fn clear_range<R>(&mut self, range: R)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.line_buffer.clear_range(range)
    }

    pub fn move_word_left(&mut self) -> usize {
        self.line_buffer.move_word_left()
    }

    pub fn move_word_right(&mut self) -> usize {
        self.line_buffer.move_word_right()
    }
}
