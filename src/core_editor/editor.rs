use crate::core_editor::get_default_clipboard;

use super::{Clipboard, LineBuffer};

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

    pub fn move_to_start(&mut self) {
        self.line_buffer.move_to_start();
    }

    pub fn move_to_end(&mut self) {
        self.line_buffer.move_to_end();
    }

    pub fn move_left(&mut self) {
        self.line_buffer.move_left();
    }

    pub fn move_right(&mut self) {
        self.line_buffer.move_right();
    }

    pub fn move_word_left(&mut self) {
        self.line_buffer.move_word_left();
    }

    pub fn move_word_right(&mut self) {
        self.line_buffer.move_word_right();
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

    pub fn backspace(&mut self) {
        self.line_buffer.delete_left_grapheme();
    }

    pub fn delete(&mut self) {
        self.line_buffer.delete_right_grapheme();
    }

    pub fn backspace_word(&mut self) {
        self.line_buffer.delete_word_left();
    }

    pub fn delete_word(&mut self) {
        self.line_buffer.delete_word_right();
    }

    pub fn clear(&mut self) {
        self.line_buffer.clear();
    }

    pub fn uppercase_word(&mut self) {
        self.line_buffer.uppercase_word();
    }

    pub fn lowercase_word(&mut self) {
        self.line_buffer.lowercase_word();
    }

    pub fn capitalize_char(&mut self) {
        self.line_buffer.capitalize_char();
    }

    pub fn swap_words(&mut self) {
        self.line_buffer.swap_words();
    }

    pub fn swap_graphemes(&mut self) {
        self.line_buffer.swap_graphemes();
    }

    pub fn set_insertion_point(&mut self, pos: usize) {
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

    pub fn clear_to_insertion_point(&mut self) {
        self.line_buffer.clear_to_insertion_point();
    }

    pub fn clear_range<R>(&mut self, range: R)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.line_buffer.clear_range(range);
    }

    pub fn offset(&self) -> usize {
        self.line_buffer.offset()
    }

    pub fn line(&self) -> usize {
        self.line_buffer.line()
    }

    pub fn num_lines(&self) -> usize {
        self.line_buffer.num_lines()
    }

    pub fn is_empty(&self) -> bool {
        self.line_buffer.is_empty()
    }

    pub fn ends_with(&self, c: char) -> bool {
        self.line_buffer.ends_with(c)
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

    pub fn undo(&mut self) {
        // NOTE: Try-blocks should help us get rid of this indirection too
        self.undo_internal();
    }

    pub fn redo(&mut self) {
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

    pub fn cut_from_start(&mut self) {
        let insertion_offset = self.line_buffer.offset();
        if insertion_offset > 0 {
            self.cut_buffer
                .set(&self.line_buffer.get_buffer()[..insertion_offset]);
            self.clear_to_insertion_point();
        }
    }

    pub fn cut_from_end(&mut self) {
        let cut_slice = &self.line_buffer.get_buffer()[self.line_buffer.offset()..];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice);
            self.clear_to_end();
        }
    }

    pub fn cut_word_left(&mut self) {
        let insertion_offset = self.line_buffer.offset();
        let left_index = self.line_buffer.word_left_index();
        if left_index < insertion_offset {
            let cut_range = left_index..insertion_offset;
            self.cut_buffer
                .set(&self.line_buffer.get_buffer()[cut_range.clone()]);
            self.clear_range(cut_range);
            self.line_buffer.set_insertion_point(left_index);
        }
    }

    pub fn cut_word_right(&mut self) {
        let insertion_offset = self.line_buffer.offset();
        let right_index = self.line_buffer.word_right_index();
        if right_index > insertion_offset {
            let cut_range = insertion_offset..right_index;
            self.cut_buffer
                .set(&self.line_buffer.get_buffer()[cut_range.clone()]);
            self.clear_range(cut_range);
        }
    }

    pub fn insert_cut_buffer(&mut self) {
        let cut_buffer = self.cut_buffer.get();
        self.line_buffer.insert_str(&cut_buffer);
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
