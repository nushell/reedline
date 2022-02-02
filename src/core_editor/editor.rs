use super::{Clipboard, ClipboardMode, LineBuffer};
use crate::{core_editor::get_default_clipboard, EditCommand, UndoBehavior};

pub struct Editor {
    line_buffer: LineBuffer,
    cut_buffer: Box<dyn Clipboard>,

    edits: Vec<LineBuffer>,
    index_undo: usize,
}

impl Default for Editor {
    fn default() -> Self {
        Editor {
            line_buffer: LineBuffer::new(),
            cut_buffer: Box::new(get_default_clipboard()),

            // Note: Using list-zipper we can reduce these to one field
            edits: vec![LineBuffer::new()],
            index_undo: 2,
        }
    }
}

impl Editor {
    pub fn line_buffer(&mut self) -> &mut LineBuffer {
        &mut self.line_buffer
    }

    pub fn set_line_buffer(&mut self, line_buffer: LineBuffer) {
        self.line_buffer = line_buffer;
    }

    pub fn run_edit_command(&mut self, command: &EditCommand) {
        match command {
            EditCommand::MoveToStart => self.line_buffer.move_to_start(),
            EditCommand::MoveToLineStart => self.line_buffer.move_to_line_start(),
            EditCommand::MoveToEnd => self.line_buffer.move_to_end(),
            EditCommand::MoveToLineEnd => self.line_buffer.move_to_line_end(),
            EditCommand::MoveLeft => self.line_buffer.move_left(),
            EditCommand::MoveRight => self.line_buffer.move_right(),
            EditCommand::MoveWordLeft => self.line_buffer.move_word_left(),
            EditCommand::MoveWordRight => self.line_buffer.move_word_right(),
            EditCommand::InsertChar(c) => self.insert_char(*c),
            EditCommand::InsertString(str) => self.line_buffer.insert_str(str),
            EditCommand::Backspace => self.line_buffer.delete_left_grapheme(),
            EditCommand::Delete => self.line_buffer.delete_right_grapheme(),
            EditCommand::BackspaceWord => self.line_buffer.delete_word_left(),
            EditCommand::DeleteWord => self.line_buffer.delete_word_right(),
            EditCommand::Clear => self.line_buffer.clear(),
            EditCommand::ClearToLineEnd => self.line_buffer.clear_to_line_end(),
            EditCommand::CutCurrentLine => self.cut_current_line(),
            EditCommand::CutFromStart => self.cut_from_start(),
            EditCommand::CutFromLineStart => self.cut_from_line_start(),
            EditCommand::CutToEnd => self.cut_from_end(),
            EditCommand::CutToLineEnd => self.cut_to_line_end(),
            EditCommand::CutWordLeft => self.cut_word_left(),
            EditCommand::CutWordRight => self.cut_word_right(),
            EditCommand::PasteCutBufferBefore => self.insert_cut_buffer_before(),
            EditCommand::PasteCutBufferAfter => self.insert_cut_buffer_after(),
            EditCommand::UppercaseWord => self.line_buffer.uppercase_word(),
            EditCommand::LowercaseWord => self.line_buffer.lowercase_word(),
            EditCommand::CapitalizeChar => self.line_buffer.capitalize_char(),
            EditCommand::SwapWords => self.line_buffer.swap_words(),
            EditCommand::SwapGraphemes => self.line_buffer.swap_graphemes(),
            EditCommand::Undo => self.undo(),
            EditCommand::Redo => self.redo(),
            EditCommand::CutRightUntil(c) => self.cut_right_until_char(*c, false, true),
            EditCommand::CutRightBefore(c) => self.cut_right_until_char(*c, true, true),
            EditCommand::MoveRightUntil(c) => self.move_right_until_char(*c, false, true),
            EditCommand::MoveRightBefore(c) => self.move_right_until_char(*c, true, true),
            EditCommand::CutLeftUntil(c) => self.cut_left_until_char(*c, false, true),
            EditCommand::CutLeftBefore(c) => self.cut_left_until_char(*c, true, true),
            EditCommand::MoveLeftUntil(c) => self.move_left_until_char(*c, false, true),
            EditCommand::MoveLeftBefore(c) => self.move_left_until_char(*c, true, true),
        }
        match command.undo_behavior() {
            UndoBehavior::Ignore => {}
            UndoBehavior::Full => {
                self.remember_undo_state(true);
            }
            UndoBehavior::Coalesce => {
                self.remember_undo_state(false);
            }
        }
    }

    pub fn move_line_up(&mut self) {
        self.line_buffer.move_line_up();
    }

    pub fn move_line_down(&mut self) {
        self.line_buffer.move_line_down();
    }

    pub fn insert_char(&mut self, c: char) {
        self.line_buffer.insert_char(c);
    }

    /// Directly change the cursor position measured in bytes in the buffer
    ///
    /// ## Unicode safety:
    /// Not checked, inproper use may cause panics in following operations
    pub(crate) fn set_insertion_point(&mut self, pos: usize) {
        self.line_buffer.set_insertion_point(pos);
    }

    pub fn get_buffer(&self) -> &str {
        self.line_buffer.get_buffer()
    }

    pub fn set_buffer(&mut self, buffer: String) {
        self.line_buffer.set_buffer(buffer);
    }

    pub fn clear_to_end(&mut self) {
        self.line_buffer.clear_to_end();
    }

    fn clear_to_insertion_point(&mut self) {
        self.line_buffer.clear_to_insertion_point();
    }

    fn clear_range<R>(&mut self, range: R)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.line_buffer.clear_range(range);
    }

    pub fn offset(&self) -> usize {
        self.line_buffer.offset()
    }

    pub fn is_empty(&self) -> bool {
        self.line_buffer.is_empty()
    }

    pub fn is_cursor_at_first_line(&self) -> bool {
        self.line_buffer.is_cursor_at_first_line()
    }

    pub fn is_cursor_at_last_line(&self) -> bool {
        self.line_buffer.is_cursor_at_last_line()
    }

    pub fn reset_undo_stack(&mut self) {
        self.edits = vec![LineBuffer::new()];
        self.index_undo = 2;
    }

    fn get_index_undo(&self) -> usize {
        if let Some(c) = self.edits.len().checked_sub(self.index_undo) {
            c
        } else {
            0
        }
    }

    fn undo(&mut self) {
        // NOTE: Try-blocks should help us get rid of this indirection too
        self.undo_internal();
    }

    fn redo(&mut self) {
        // NOTE: Try-blocks should help us get rid of this indirection too
        self.redo_internal();
    }

    fn redo_internal(&mut self) -> Option<()> {
        if self.index_undo > 2 {
            self.index_undo = self.index_undo.checked_sub(2)?;
            self.undo_internal()
        } else {
            None
        }
    }

    fn undo_internal(&mut self) -> Option<()> {
        self.line_buffer = self.edits.get(self.get_index_undo())?.clone();

        if self.index_undo <= self.edits.len() {
            self.index_undo = self.index_undo.checked_add(1)?;
        }
        Some(())
    }

    pub fn remember_undo_state(&mut self, is_after_action: bool) -> Option<()> {
        self.reset_index_undo();

        if self.edits.len() > 1
            && self.edits.last()?.word_count() == self.line_buffer.word_count()
            && !is_after_action
        {
            self.edits.pop();
        }
        self.edits.push(self.line_buffer.clone());

        Some(())
    }

    fn reset_index_undo(&mut self) {
        self.index_undo = 2;
    }

    fn cut_current_line(&mut self) {
        let deletion_range = self.line_buffer.current_line_range();

        let cut_slice = &self.line_buffer.get_buffer()[deletion_range.clone()];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice, ClipboardMode::Lines);
            self.set_insertion_point(deletion_range.start);
            self.clear_range(deletion_range);
        }
    }

    fn cut_from_start(&mut self) {
        let insertion_offset = self.line_buffer.offset();
        if insertion_offset > 0 {
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[..insertion_offset],
                ClipboardMode::Normal,
            );
            self.clear_to_insertion_point();
        }
    }

    fn cut_from_line_start(&mut self) {
        let previous_offset = self.line_buffer.offset();
        self.line_buffer.move_to_line_start();
        let deletion_range = self.line_buffer.offset()..previous_offset;
        let cut_slice = &self.line_buffer.get_buffer()[deletion_range.clone()];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice, ClipboardMode::Normal);
            self.line_buffer.clear_range(deletion_range);
        }
    }

    pub fn cut_from_end(&mut self) {
        let cut_slice = &self.line_buffer.get_buffer()[self.line_buffer.offset()..];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice, ClipboardMode::Normal);
            self.clear_to_end();
        }
    }

    fn cut_to_line_end(&mut self) {
        let cut_slice = &self.line_buffer.get_buffer()
            [self.line_buffer.offset()..self.line_buffer.find_current_line_end()];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice, ClipboardMode::Normal);
            self.line_buffer.clear_to_line_end();
        }
    }

    fn cut_word_left(&mut self) {
        let insertion_offset = self.line_buffer.offset();
        let left_index = self.line_buffer.word_left_index();
        if left_index < insertion_offset {
            let cut_range = left_index..insertion_offset;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[cut_range.clone()],
                ClipboardMode::Normal,
            );
            self.clear_range(cut_range);
            self.line_buffer.set_insertion_point(left_index);
        }
    }

    fn cut_word_right(&mut self) {
        let insertion_offset = self.line_buffer.offset();
        let right_index = self.line_buffer.word_right_index();
        if right_index > insertion_offset {
            let cut_range = insertion_offset..right_index;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[cut_range.clone()],
                ClipboardMode::Normal,
            );
            self.clear_range(cut_range);
        }
    }

    fn insert_cut_buffer_before(&mut self) {
        match self.cut_buffer.get() {
            (content, ClipboardMode::Normal) => {
                self.line_buffer.insert_str(&content);
            }
            (mut content, ClipboardMode::Lines) => {
                // TODO: Simplify that?
                self.line_buffer.move_to_line_start();
                self.line_buffer.move_line_up();
                if !content.ends_with('\n') {
                    // TODO: Make sure platform requirements are met
                    content.push('\n');
                }
                self.line_buffer.insert_str(&content);
            }
        }
    }

    fn insert_cut_buffer_after(&mut self) {
        match self.cut_buffer.get() {
            (content, ClipboardMode::Normal) => {
                self.line_buffer.move_right();
                self.line_buffer.insert_str(&content);
            }
            (mut content, ClipboardMode::Lines) => {
                // TODO: Simplify that?
                self.line_buffer.move_to_line_start();
                self.line_buffer.move_line_down();
                if !content.ends_with('\n') {
                    // TODO: Make sure platform requirements are met
                    content.push('\n');
                }
                self.line_buffer.insert_str(&content);
            }
        }
    }

    fn move_right_until_char(&mut self, c: char, before_char: bool, current_line: bool) {
        if before_char {
            self.line_buffer.move_right_before(c, current_line);
        } else {
            self.line_buffer.move_right_until(c, current_line);
        }
    }

    fn move_left_until_char(&mut self, c: char, before_char: bool, current_line: bool) {
        if before_char {
            self.line_buffer.move_left_before(c, current_line);
        } else {
            self.line_buffer.move_left_until(c, current_line);
        }
    }

    fn cut_right_until_char(&mut self, c: char, before_char: bool, current_line: bool) {
        if let Some(index) = self.line_buffer.find_char_right(c, current_line) {
            // Saving the section of the string that will be deleted to be
            // stored into the buffer
            let extra = if before_char { 0 } else { c.len_utf8() };
            let cut_slice =
                &self.line_buffer.get_buffer()[self.line_buffer.offset()..index + extra];

            if !cut_slice.is_empty() {
                self.cut_buffer.set(cut_slice, ClipboardMode::Normal);

                if before_char {
                    self.line_buffer.delete_right_before_char(c, current_line);
                } else {
                    self.line_buffer.delete_right_until_char(c, current_line);
                }
            }
        }
    }

    fn cut_left_until_char(&mut self, c: char, before_char: bool, current_line: bool) {
        if let Some(index) = self.line_buffer.find_char_left(c, current_line) {
            // Saving the section of the string that will be deleted to be
            // stored into the buffer
            let extra = if before_char { c.len_utf8() } else { 0 };
            let cut_slice =
                &self.line_buffer.get_buffer()[index + extra..self.line_buffer.offset()];

            if !cut_slice.is_empty() {
                self.cut_buffer.set(cut_slice, ClipboardMode::Normal);

                if before_char {
                    self.line_buffer.delete_left_before_char(c, current_line);
                } else {
                    self.line_buffer.delete_left_until_char(c, current_line);
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_undo_initial_char() {
        let mut editor = Editor::default();
        editor.line_buffer().set_buffer(String::from("a"));
        editor.remember_undo_state(false);
        editor.line_buffer().set_buffer(String::from("ab"));
        editor.remember_undo_state(false);
        editor.line_buffer().set_buffer(String::from("ab "));
        editor.remember_undo_state(false);
        editor.line_buffer().set_buffer(String::from("ab c"));
        editor.remember_undo_state(true);

        assert_eq!(
            vec![
                LineBuffer::from(""),
                LineBuffer::from("ab "),
                LineBuffer::from("ab c")
            ],
            editor.edits
        );
    }
}
