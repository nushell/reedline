use unicode_segmentation::UnicodeSegmentation;

/// Cursor coordinates relative to the Unicode representation of [`LineBuffer`]
#[derive(Clone, Copy)]
pub struct InsertionPoint {
    pub line: usize,
    pub offset: usize,
}

impl InsertionPoint {
    pub fn new() -> Self {
        Self { line: 0, offset: 0 }
    }
}

impl Default for InsertionPoint {
    fn default() -> Self {
        Self::new()
    }
}

/// In memory representation of the entered line(s) to facilitate cursor based editing.
pub struct LineBuffer {
    lines: Vec<String>,
    insertion_point: InsertionPoint,
}

impl Default for LineBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl LineBuffer {
    pub fn new() -> LineBuffer {
        LineBuffer {
            lines: vec![String::new()],
            insertion_point: InsertionPoint::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() || self.lines.len() == 1 && self.lines[0].is_empty()
    }

    /// Return 2D-cursor (line_number, col_in_line)
    pub fn insertion_point(&self) -> InsertionPoint {
        self.insertion_point
    }

    pub fn set_insertion_point(&mut self, pos: InsertionPoint) {
        self.insertion_point = pos;
    }

    /// Output the current line in the multiline buffer
    pub fn get_buffer(&self) -> &str {
        &self.lines[self.insertion_point.line]
    }

    /// Set to a single line of `buffer` and reset the `InsertionPoint` cursor
    pub fn set_buffer(&mut self, buffer: String) {
        self.lines = vec![buffer];
        self.insertion_point = InsertionPoint::new();
    }

    /// Reset the insertion point to the start of the buffer
    pub fn move_to_start(&mut self) {
        self.insertion_point = InsertionPoint::new();
    }

    /// Set the insertion point *behind* the last character.
    pub fn move_to_end(&mut self) {
        if let Some(end) = self.lines.last() {
            let length_of_last_line = end.len();
            self.insertion_point.offset = length_of_last_line;
        }
    }

    /// Cursor position *behind* the next unicode grapheme to the right
    pub fn grapheme_right_index(&self) -> usize {
        self.lines[self.insertion_point.line][self.insertion_point.offset..]
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| self.insertion_point.offset + i)
            .unwrap_or_else(|| self.lines[self.insertion_point.line].len())
    }

    /// Cursor position *in front of* the next unicode grapheme to the left
    pub fn grapheme_left_index(&self) -> usize {
        self.lines[self.insertion_point.line][..self.insertion_point.offset]
            .grapheme_indices(true)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Cursor position *behind* the next word to the right
    pub fn word_right_index(&self) -> usize {
        self.lines[self.insertion_point.line][self.insertion_point.offset..]
            .split_word_bound_indices()
            .find(|(_, word)| !is_word_boundary(word))
            .map(|(i, word)| self.insertion_point.offset + i + word.len())
            .unwrap_or_else(|| self.lines[self.insertion_point.line].len())
    }

    /// Cursor position *in front of* the next word to the left
    pub fn word_left_index(&self) -> usize {
        self.lines[self.insertion_point.line][..self.insertion_point.offset]
            .split_word_bound_indices()
            .filter(|(_, word)| !is_word_boundary(word))
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Move cursor position *behind* the next unicode grapheme to the right
    pub fn move_right(&mut self) {
        self.insertion_point.offset = self.grapheme_right_index();
    }

    /// Move cursor position *in front of* the next unicode grapheme to the left
    pub fn move_left(&mut self) {
        self.insertion_point.offset = self.grapheme_left_index();
    }

    /// Move cursor position *in front of* the next word to the left
    pub fn move_word_left(&mut self) -> usize {
        self.insertion_point.offset = self.word_left_index();
        self.insertion_point.offset
    }

    /// Move cursor position *behind* the next word to the right
    pub fn move_word_right(&mut self) -> usize {
        self.insertion_point.offset = self.word_right_index();
        self.insertion_point.offset
    }

    /// Insert a single character at the given cursor postion
    pub fn insert_char(&mut self, pos: InsertionPoint, c: char) {
        self.lines[pos.line].insert(pos.offset, c)
    }

    /// Insert `&str` at the `idx` position in the current line.
    ///
    /// TODO: Check unicode validation
    pub fn insert_str(&mut self, idx: usize, string: &str) {
        self.lines[self.insertion_point.line].insert_str(idx, string)
    }

    /// Empty buffer and reset cursor
    pub fn clear(&mut self) {
        self.lines.clear();
        self.lines.push(String::new());
        self.insertion_point = InsertionPoint::new();
    }

    /// Clear everything beginning at the cursor to the right/end.
    /// Keeps the cursor at the end.
    pub fn clear_to_end(&mut self) {
        self.lines[self.insertion_point.line].truncate(self.insertion_point.offset);
    }

    /// Clear from the start of the line to the cursor.
    /// Keeps the cursor at the beginning of the line.
    pub fn clear_to_insertion_point(&mut self) {
        self.clear_range(..self.insertion_point.offset);
        self.insertion_point.offset = 0;
    }

    /// Clear text covered by `range` in the current line
    ///
    /// TODO: Check unicode validation
    pub fn clear_range<R>(&mut self, range: R)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.replace_range(range, "");
    }

    /// Substitute text covered by `range` in the current line
    ///
    /// TODO: Check unicode validation
    pub fn replace_range<R>(&mut self, range: R, replace_with: &str)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.lines[self.insertion_point.line].replace_range(range, replace_with);
    }

    pub fn on_whitespace(&self) -> bool {
        self.lines[self.insertion_point.line][self.insertion_point.offset..]
            .chars()
            .next()
            .map(char::is_whitespace)
            .unwrap_or(false)
    }
}

/// Match any sequence of characters that are considered a word boundary
fn is_word_boundary(s: &str) -> bool {
    !s.chars().any(char::is_alphanumeric)
}

#[test]
fn emoji_test() {
    //TODO
    // "😊";
    // "🤦🏼‍♂️";
}
