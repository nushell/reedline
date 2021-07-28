use super::{Clipboard, LineBuffer};

pub struct Editor {
    line_buffer: LineBuffer,
    clip_buffer: Box<dyn Clipboard>,
}

impl Editor {
    pub fn new(line_buffer: LineBuffer, clip_buffer: Box<dyn Clipboard>) -> Editor {
        Editor {
            line_buffer,
            clip_buffer,
        }
    }

    pub fn line_buffer(&mut self) -> &mut LineBuffer {
        &mut self.line_buffer
    }

    pub fn set_line_buffer(&mut self, line_buffer: LineBuffer) {
        self.line_buffer = line_buffer;
    }

    pub fn move_to_start(&mut self) {
        self.line_buffer.move_to_start()
    }

    pub fn move_to_end(&mut self) {
        self.line_buffer.move_to_end()
    }

    pub fn move_left(&mut self) {
        self.line_buffer.move_left()
    }

    pub fn move_right(&mut self) {
        self.line_buffer.move_right()
    }

    pub fn move_word_left(&mut self) {
        self.line_buffer.move_word_left();
    }

    pub fn move_word_right(&mut self) {
        self.line_buffer.move_word_right();
    }

    pub fn insert_char(&mut self, c: char) {
        self.line_buffer.insert_char(c)
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

    pub fn set_insertion_point(&mut self, line: usize, pos: usize) {
        self.line_buffer.set_insertion_point(line, pos)
    }

    pub fn get_buffer(&self) -> &str {
        self.line_buffer.get_buffer()
    }

    pub fn set_buffer(&mut self, buffer: String) {
        self.line_buffer.set_buffer(buffer)
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

    pub fn offset(&self) -> usize {
        self.line_buffer.offset()
    }

    pub fn line(&self) -> usize {
        self.line_buffer.line()
    }

    pub fn is_empty(&self) -> bool {
        self.line_buffer.is_empty()
    }

    pub fn undo(&mut self) -> Option<()> {
        self.line_buffer.undo()
    }

    pub fn redo(&mut self) -> Option<()> {
        self.line_buffer.redo()
    }

    pub fn reset_olds(&mut self) {
        self.line_buffer.reset_olds()
    }

    pub fn set_previous_lines(&mut self, is_after_action: bool) -> Option<()> {
        self.line_buffer.set_previous_lines(is_after_action)
    }

    pub fn word_right_index(&self) -> usize {
        self.line_buffer.word_right_index()
    }

    pub fn word_left_index(&self) -> usize {
        self.line_buffer.word_left_index()
    }

    pub fn insert_str(&mut self, string: &str) {
        self.line_buffer.insert_str(string);
    }
}
