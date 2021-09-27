use crate::core_editor::get_default_clipboard;

use super::{
    undo_stack::{BasicUndoStack, UndoStack},
    Clipboard, LineBuffer,
};

pub struct Editor {
    line_buffer: LineBuffer,
    cut_buffer: Box<dyn Clipboard>,
    undo_stack: Box<dyn UndoStack<LineBuffer>>,
}

impl Default for Editor {
    fn default() -> Self {
        Editor {
            line_buffer: LineBuffer::new(),
            cut_buffer: Box::new(get_default_clipboard()),
            undo_stack: Box::new(BasicUndoStack::new()),
        }
    }
}

impl Editor {
    pub fn line_buffer(&mut self) -> &mut LineBuffer {
        &mut self.line_buffer
    }

    pub fn set_line_buffer(&mut self, line_buffer: LineBuffer) {
        self.line_buffer = line_buffer;
        self.insert_to_undo_stack();
    }

    pub fn move_to_start(&mut self) {
        self.line_buffer.move_to_start();
        self.insert_to_undo_stack();
    }

    pub fn move_to_end(&mut self) {
        self.line_buffer.move_to_end();
        self.insert_to_undo_stack();
    }

    pub fn move_left(&mut self) {
        self.line_buffer.move_left();
        self.insert_to_undo_stack();
    }

    pub fn move_right(&mut self) {
        self.line_buffer.move_right();
        self.insert_to_undo_stack();
    }

    pub fn move_word_left(&mut self) {
        self.line_buffer.move_word_left();
        self.insert_to_undo_stack();
    }

    pub fn move_word_right(&mut self) {
        self.line_buffer.move_word_right();
        self.insert_to_undo_stack();
    }

    pub fn move_line_up(&mut self) {
        self.line_buffer.move_line_up();
        self.insert_to_undo_stack();
    }

    pub fn move_line_down(&mut self) {
        self.line_buffer.move_line_down();
        self.insert_to_undo_stack();
    }

    pub fn insert_char(&mut self, c: char) {
        self.line_buffer.insert_char(c);
        self.insert_to_undo_stack();
    }

    pub fn backspace(&mut self) {
        self.line_buffer.delete_left_grapheme();
        self.insert_to_undo_stack();
    }

    pub fn delete(&mut self) {
        self.line_buffer.delete_right_grapheme();
        self.insert_to_undo_stack();
    }

    pub fn backspace_word(&mut self) {
        self.line_buffer.delete_word_left();
        self.insert_to_undo_stack();
    }

    pub fn delete_word(&mut self) {
        self.line_buffer.delete_word_right();
        self.insert_to_undo_stack();
    }

    pub fn clear(&mut self) {
        self.line_buffer.clear();
        self.insert_to_undo_stack();
    }

    pub fn uppercase_word(&mut self) {
        self.line_buffer.uppercase_word();
        self.insert_to_undo_stack();
    }

    pub fn lowercase_word(&mut self) {
        self.line_buffer.lowercase_word();
        self.insert_to_undo_stack();
    }

    pub fn capitalize_char(&mut self) {
        self.line_buffer.capitalize_char();
        self.insert_to_undo_stack();
    }

    pub fn swap_words(&mut self) {
        self.line_buffer.swap_words();
        self.insert_to_undo_stack();
    }

    pub fn swap_graphemes(&mut self) {
        self.line_buffer.swap_graphemes();
        self.insert_to_undo_stack();
    }

    pub fn set_insertion_point(&mut self, pos: usize) {
        self.line_buffer.set_insertion_point(pos);
        self.insert_to_undo_stack();
    }

    pub fn get_buffer(&self) -> &str {
        self.line_buffer.get_buffer()
    }

    pub fn set_buffer(&mut self, buffer: String) {
        self.line_buffer.set_buffer(buffer);
        self.insert_to_undo_stack();
    }

    pub fn clear_to_end(&mut self) {
        self.line_buffer.clear_to_end();
        self.insert_to_undo_stack();
    }

    pub fn clear_to_insertion_point(&mut self) {
        self.line_buffer.clear_to_insertion_point();
        self.insert_to_undo_stack();
    }

    pub fn clear_range<R>(&mut self, range: R)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.line_buffer.clear_range(range);
        self.insert_to_undo_stack();
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

    pub fn reset_olds(&mut self) {
        self.undo_stack.reset();
    }

    pub fn undo(&mut self) {
        let val = self.undo_stack.undo();
        self.line_buffer = val.clone();
    }

    pub fn redo(&mut self) {
        let val = self.undo_stack.redo();
        self.line_buffer = val.clone();
    }

    // pub fn set_previous_lines(&mut self, is_after_action: bool) -> Option<()> {
    //     self.reset_index_undo();
    //
    //     if self.edits.len() > 1
    //         && self.edits.last()?.word_count() == self.line_buffer.word_count()
    //         && !is_after_action
    //     {
    //         self.edits.pop();
    //     }
    //     self.edits.push(self.line_buffer.clone());
    //
    //     Some(())
    // }
    //

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

    fn insert_to_undo_stack(&mut self) {
        // PERF: Use some strategy to insert into undo stack instead of inserting everything
        self.undo_stack.insert(self.line_buffer.clone());
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    #[test]
    fn undo_which_captures_all_operations() {
        let mut editor = Editor::default();

        editor.insert_char('a');
        editor.insert_char('b');
        editor.insert_char(' ');
        editor.insert_char('c');

        let expected_edits = vec![
            LineBuffer::new(),
            LineBuffer::from("a"),
            LineBuffer::from("ab"),
            LineBuffer::from("ab "),
            LineBuffer::from("ab c"),
        ];

        let actual_edits: Vec<LineBuffer> = editor.undo_stack.edits().cloned().collect();

        assert_eq!(expected_edits, actual_edits);
    }
}
