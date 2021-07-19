use {std::ops::Range, unicode_segmentation::UnicodeSegmentation};

/// Cursor coordinates relative to the Unicode representation of [`LineBuffer`]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
#[derive(Debug, PartialEq, Eq)]
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

    /// Replaces the content between [`start`..`end`] with `text`
    pub fn replace(&mut self, range: Range<usize>, line_num: usize, text: &str) {
        self.lines[line_num].replace_range(range, text);
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
        let buffer = buffer.lines().map(|s| s.into()).collect::<Vec<String>>();

        // Note: `buffer` will have at least one element so the following operations are safe
        let last_line_index = buffer.len() - 1;
        let last_line_length = buffer.last().unwrap().len();

        self.lines = buffer;
        self.insertion_point = InsertionPoint {
            line: last_line_index,
            offset: last_line_length,
        };
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

    ///Insert a single character at the insertion point and move right
    pub fn insert_char(&mut self, c: char) {
        let pos = self.insertion_point();
        self.lines[pos.line].insert(pos.offset, c);
        self.move_right();
    }

    /// Insert `&str` at the `idx` position in the current line.
    ///
    /// TODO: Check unicode validation
    pub fn insert_str(&mut self, string: &str) {
        let pos = self.insertion_point();
        self.lines[pos.line].insert_str(pos.offset, string);
        self.insertion_point.offset = pos.offset + string.len();
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

    pub fn uppercase_word(&mut self) {
        let insertion_offset = self.insertion_point().offset;
        let right_index = self.word_right_index();

        if right_index > insertion_offset {
            let change_range = insertion_offset..right_index;
            let uppercased = self.get_buffer()[change_range.clone()].to_uppercase();
            self.replace_range(change_range, &uppercased);
            self.move_word_right();
        }
    }

    pub fn lowercase_word(&mut self) {
        let insertion_offset = self.insertion_point().offset;
        let right_index = self.word_right_index();
        if right_index > insertion_offset {
            let change_range = insertion_offset..right_index;
            let lowercased = self.get_buffer()[change_range.clone()].to_lowercase();
            self.replace_range(change_range, &lowercased);
            self.move_word_right();
        }
    }

    pub fn capitalize_char(&mut self) {
        if self.on_whitespace() {
            self.move_word_right();
            self.move_word_left();
        }
        let insertion_offset = self.insertion_point().offset;
        let right_index = self.grapheme_right_index();
        if right_index > insertion_offset {
            let change_range = insertion_offset..right_index;
            let uppercased = self.get_buffer()[change_range.clone()].to_uppercase();
            self.replace_range(change_range, &uppercased);
            self.move_word_right();
        }
    }

    pub fn delete_left_grapheme(&mut self) {
        let left_index = self.grapheme_left_index();
        let insertion_offset = self.insertion_point().offset;
        if left_index < insertion_offset {
            self.clear_range(left_index..insertion_offset);
            self.insertion_point.offset = left_index
        }
    }

    pub fn delete_right_grapheme(&mut self) {
        let right_index = self.grapheme_right_index();
        let insertion_offset = self.insertion_point().offset;
        if right_index > insertion_offset {
            self.clear_range(insertion_offset..right_index);
        }
    }

    pub fn delete_word_left(&mut self) {
        let left_word_index = self.word_left_index();
        self.clear_range(left_word_index..self.insertion_point().offset);
        self.insertion_point.offset = left_word_index;
    }

    pub fn delete_word_right(&mut self) {
        let right_word_index = self.word_right_index();
        self.clear_range(self.insertion_point().offset..right_word_index);
    }

    pub fn swap_words(&mut self) {
        let old_insertion_point = self.insertion_point().offset;
        self.move_word_right();
        let word_2_end = self.insertion_point().offset;
        self.move_word_left();
        let word_2_start = self.insertion_point().offset;
        self.move_word_left();
        let word_1_start = self.insertion_point().offset;
        let word_1_end = self.word_right_index();

        if word_1_start < word_1_end && word_1_end < word_2_start && word_2_start < word_2_end {
            let insertion_line = self.get_buffer();
            let word_1 = insertion_line[word_1_start..word_1_end].to_string();
            let word_2 = insertion_line[word_2_start..word_2_end].to_string();
            self.replace_range(word_2_start..word_2_end, &word_1);
            self.replace_range(word_1_start..word_1_end, &word_2);
            self.insertion_point.offset = word_2_end;
        } else {
            self.insertion_point.offset = old_insertion_point;
        }
    }

    pub fn swap_graphemes(&mut self) {
        let insertion_offset = self.insertion_point().offset;

        if insertion_offset == 0 {
            self.move_right()
        } else if insertion_offset == self.get_buffer().len() {
            self.move_left()
        }
        let grapheme_1_start = self.grapheme_left_index();
        let grapheme_2_end = self.grapheme_right_index();

        if grapheme_1_start < insertion_offset && grapheme_2_end > insertion_offset {
            let grapheme_1 = self.get_buffer()[grapheme_1_start..insertion_offset].to_string();
            let grapheme_2 = self.get_buffer()[insertion_offset..grapheme_2_end].to_string();
            self.replace_range(insertion_offset..grapheme_2_end, &grapheme_1);
            self.replace_range(grapheme_1_start..insertion_offset, &grapheme_2);
            self.insertion_point.offset = grapheme_2_end;
        } else {
            self.insertion_point.offset = insertion_offset;
        }
    }
}

/// Match any sequence of characters that are considered a word boundary
fn is_word_boundary(s: &str) -> bool {
    !s.chars().any(char::is_alphanumeric)
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn buffer_with(content: &str) -> LineBuffer {
        let mut line_buffer = LineBuffer::new();
        line_buffer.insert_str(content);

        line_buffer
    }

    #[test]
    fn test_new_buffer_is_empty() {
        let line_buffer = LineBuffer::new();
        assert!(line_buffer.is_empty())
    }

    #[test]
    fn test_clearing_line_buffer_resets_buffer_and_insertion_point() {
        let mut buffer = buffer_with("this is a command");
        buffer.clear();
        let empty_buffer = LineBuffer::new();

        assert_eq!(buffer, empty_buffer)
    }

    #[test]
    fn insert_str_updates_insertion_point_point_correctly() {
        let mut line_buffer = LineBuffer::new();
        line_buffer.insert_str("this is a command");

        let expected_updated_insertion_point = InsertionPoint {
            line: 0,
            offset: 17,
        };

        assert_eq!(
            expected_updated_insertion_point,
            line_buffer.insertion_point()
        );
    }

    #[test]
    fn insert_char_updates_insertion_point_point_correctly() {
        let mut line_buffer = LineBuffer::new();
        line_buffer.insert_char('c');

        let expected_updated_insertion_point = InsertionPoint { line: 0, offset: 1 };

        assert_eq!(
            expected_updated_insertion_point,
            line_buffer.insertion_point()
        );
    }

    #[test]
    fn set_buffer_updates_insertion_point_to_new_buffer_length() {
        let mut line_buffer = buffer_with("test string");
        let before_operation_location = InsertionPoint {
            line: 0,
            offset: 11,
        };
        assert_eq!(before_operation_location, line_buffer.insertion_point());

        line_buffer.set_buffer("new string".to_string());

        let after_operation_location = InsertionPoint {
            line: 0,
            offset: 10,
        };
        assert_eq!(after_operation_location, line_buffer.insertion_point());
    }

    #[test]
    fn set_buffer_works_with_multi_line_string() {
        let mut line_buffer = buffer_with("test string");
        let before_operation_location = InsertionPoint {
            line: 0,
            offset: 11,
        };
        assert_eq!(before_operation_location, line_buffer.insertion_point());

        line_buffer.set_buffer("new line 1\nnew_line 2".to_string());

        let after_operation_location = InsertionPoint {
            line: 1,
            offset: 10,
        };
        assert_eq!(after_operation_location, line_buffer.insertion_point());
    }

    #[test]
    fn delete_left_grapheme_works() {
        let mut line_buffer = buffer_with("This is a test");
        line_buffer.delete_left_grapheme();

        let expected_line_buffer = buffer_with("This is a tes");

        assert_eq!(expected_line_buffer, line_buffer);
    }

    #[test]
    fn delete_left_grapheme_works_with_emojis() {
        let mut line_buffer = buffer_with("This is a test 😊");
        line_buffer.delete_left_grapheme();

        let expected_line_buffer = buffer_with("This is a test ");

        assert_eq!(expected_line_buffer, line_buffer);
    }

    #[test]
    fn delete_left_grapheme_on_an_empty_buffer_is_a_no_op() {
        let mut line_buffer = buffer_with("");
        line_buffer.delete_left_grapheme();

        let expected_line_buffer = buffer_with("");

        assert_eq!(expected_line_buffer, line_buffer);
    }

    #[test]
    fn delete_right_grapheme_works() {
        let mut line_buffer = buffer_with("This is a test");
        line_buffer.move_left();
        line_buffer.delete_right_grapheme();

        let expected_line_buffer = buffer_with("This is a tes");

        assert_eq!(expected_line_buffer, line_buffer);
    }

    #[test]
    fn delete_right_grapheme_works_with_emojis() {
        let mut line_buffer = buffer_with("This is a test 😊");
        line_buffer.move_left();
        line_buffer.delete_right_grapheme();

        let expected_line_buffer = buffer_with("This is a test ");

        assert_eq!(expected_line_buffer, line_buffer);
    }

    #[test]
    fn delete_right_grapheme_on_an_empty_buffer_is_a_no_op() {
        let mut line_buffer = buffer_with("");
        line_buffer.delete_right_grapheme();

        let expected_line_buffer = buffer_with("");

        assert_eq!(expected_line_buffer, line_buffer);
    }

    #[test]
    fn delete_word_left_works() {
        let mut line_buffer = buffer_with("This is a test");
        line_buffer.delete_word_left();

        let expected_line_buffer = buffer_with("This is a ");

        assert_eq!(expected_line_buffer, line_buffer);
    }

    #[test]
    fn delete_word_right_works() {
        let mut line_buffer = buffer_with("This is a test");
        line_buffer.move_word_left();
        line_buffer.delete_word_right();

        let expected_line_buffer = buffer_with("This is a ");

        assert_eq!(expected_line_buffer, line_buffer);
    }

    #[test]
    #[ignore] // Note: Not sure if this is the intended behaviour
    fn uppercase_word_works_when_one_last_index() {
        let mut line_buffer = buffer_with("This is a test");
        line_buffer.uppercase_word();

        let expected_line_buffer = buffer_with("This is a TEST");

        assert_eq!(expected_line_buffer, line_buffer);
    }

    #[test]
    fn uppercase_word_works() {
        let mut line_buffer = buffer_with("This is a test");
        line_buffer.move_word_left();
        line_buffer.uppercase_word();

        let expected_line_buffer = buffer_with("This is a TEST");

        assert_eq!(expected_line_buffer, line_buffer);
    }
}

#[test]
fn emoji_test() {
    //TODO
    // "😊";
    // "🤦🏼‍♂️";
}
