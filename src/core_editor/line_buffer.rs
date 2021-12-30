use {
    std::{convert::From, ops::Range},
    unicode_segmentation::UnicodeSegmentation,
};

/// Cursor coordinates relative to the Unicode representation of [`LineBuffer`]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct InsertionPoint {
    offset: usize,
}

impl InsertionPoint {
    pub fn new() -> Self {
        Self { offset: 0 }
    }
}

impl Default for InsertionPoint {
    fn default() -> Self {
        Self::new()
    }
}

/// In memory representation of the entered line(s) to facilitate cursor based editing.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LineBuffer {
    lines: String,
    insertion_point: InsertionPoint,
}

impl Default for LineBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl From<&str> for LineBuffer {
    fn from(input: &str) -> Self {
        let mut line_buffer = LineBuffer::new();
        line_buffer.insert_str(input);
        line_buffer
    }
}

impl LineBuffer {
    /// Create a line buffer instance
    pub fn new() -> LineBuffer {
        LineBuffer {
            lines: String::new(),
            insertion_point: InsertionPoint::new(),
        }
    }

    /// Replaces the content between [`start`..`end`] with `text`
    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        self.lines.replace_range(range, text);
    }

    /// Check to see if the line buffer is empty
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Gets the current edit position
    pub fn offset(&self) -> usize {
        self.insertion_point.offset
    }

    /// Return cursor
    fn insertion_point(&self) -> InsertionPoint {
        self.insertion_point
    }

    /// Sets the current edit position
    pub fn set_insertion_point(&mut self, offset: usize) {
        self.insertion_point = InsertionPoint { offset };
    }

    /// Output the current line in the multiline buffer
    pub fn get_buffer(&self) -> &str {
        &self.lines
    }

    /// Set to a single line of `buffer` and reset the `InsertionPoint` cursor to the end
    pub fn set_buffer(&mut self, buffer: String) {
        let offset = buffer.len();
        self.lines = buffer;
        self.insertion_point = InsertionPoint { offset };
    }

    /// Calculates the current the user is on
    pub fn line(&self) -> usize {
        let offset = self.insertion_point.offset;
        self.lines[..offset].matches('\n').count()
    }

    /// Counts the number of lines in the buffer
    pub fn num_lines(&self) -> usize {
        let count = self.lines.split('\n').count();

        if count == 0 {
            1
        } else {
            count
        }
    }

    /// Checks to see if the buffer ends with a given character
    pub fn ends_with(&self, c: char) -> bool {
        self.lines.ends_with(c)
    }

    /// Reset the insertion point to the start of the buffer
    pub fn move_to_start(&mut self) {
        self.insertion_point = InsertionPoint::new();
    }

    /// Move the cursor before the first character of the line
    pub fn move_to_line_start(&mut self) {
        self.insertion_point.offset = self.lines[..self.insertion_point.offset]
            .rfind('\n')
            .map_or(0, |offset| offset + 1);
        // str is guaranteed to be utf8, thus \n is safe to assume 1 byte long
    }

    /// Set the insertion point *behind* the last character.
    pub fn move_to_end(&mut self) {
        self.insertion_point.offset = self.lines.len();
    }

    /// Returns where the current line terminates
    ///
    /// Either:
    /// - end of buffer (`len()`)
    /// - `\n` or `\r\n` (on the first byte)
    pub fn find_current_line_end(&self) -> usize {
        self.lines[self.insertion_point.offset..]
            .find('\n')
            .map_or(self.lines.len(), |i| {
                let absolute_index = i + self.insertion_point.offset;
                if absolute_index > 0 && self.lines.as_bytes()[absolute_index - 1] == b'\r' {
                    absolute_index - 1
                } else {
                    absolute_index
                }
            })
    }

    /// Move cursor position to the end of the line
    ///
    /// Insertion will append to the line.
    /// Cursor on top of the potential `\n` or `\r` of `\r\n`
    pub fn move_to_line_end(&mut self) {
        self.insertion_point.offset = self.find_current_line_end();
    }

    /// Cursor position *behind* the next unicode grapheme to the right
    pub fn grapheme_right_index(&self) -> usize {
        self.lines[self.insertion_point.offset..]
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| self.insertion_point.offset + i)
            .unwrap_or_else(|| self.lines.len())
    }

    /// Cursor position *in front of* the next unicode grapheme to the left
    pub fn grapheme_left_index(&self) -> usize {
        self.lines[..self.insertion_point.offset]
            .grapheme_indices(true)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Cursor position *behind* the next word to the right
    pub fn word_right_index(&self) -> usize {
        self.lines[self.insertion_point.offset..]
            .split_word_bound_indices()
            .find(|(_, word)| !is_word_boundary(word))
            .map(|(i, word)| self.insertion_point.offset + i + word.len())
            .unwrap_or_else(|| self.lines.len())
    }

    /// Cursor position *in front of* the next word to the left
    pub fn word_left_index(&self) -> usize {
        self.lines[..self.insertion_point.offset]
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
        self.lines.insert(pos.offset, c);
        self.move_right();
    }

    /// Insert `&str` at the `idx` position in the current line.
    ///
    /// TODO: Check unicode validation
    pub fn insert_str(&mut self, string: &str) {
        let pos = self.insertion_point();
        self.lines.insert_str(pos.offset, string);
        self.insertion_point.offset = pos.offset + string.len();
    }

    /// Empty buffer and reset cursor
    pub fn clear(&mut self) {
        self.lines = String::new();
        self.insertion_point = InsertionPoint::new();
    }

    /// Clear everything beginning at the cursor to the right/end.
    /// Keeps the cursor at the end.
    pub fn clear_to_end(&mut self) {
        self.lines.truncate(self.insertion_point.offset);
    }

    /// Clear everything beginning at the cursor up to the end of the line.
    /// Newline character at the end remains.
    pub fn clear_to_line_end(&mut self) {
        self.clear_range(self.insertion_point.offset..self.find_current_line_end());
    }

    /// Clear from the start of the line to the cursor.
    /// Keeps the cursor at the beginning of the line.
    pub fn clear_to_insertion_point(&mut self) {
        self.clear_range(..self.insertion_point.offset);
        self.insertion_point.offset = 0;
    }

    /// Clear text covered by `range` in the current line
    ///
    /// Safety: Does not change the insertion point/offset and is thus not unicode safe!
    pub(crate) fn clear_range<R>(&mut self, range: R)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.replace_range(range, "");
    }

    /// Substitute text covered by `range` in the current line
    ///
    /// Safety: Does not change the insertion point/offset and is thus not unicode safe!
    pub(crate) fn replace_range<R>(&mut self, range: R, replace_with: &str)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.lines.replace_range(range, replace_with);
    }

    /// Checks to see if the current edit position is pointing to whitespace
    pub fn on_whitespace(&self) -> bool {
        self.lines[self.insertion_point.offset..]
            .chars()
            .next()
            .map(char::is_whitespace)
            .unwrap_or(false)
    }

    /// Gets the range of the word the current edit position is pointing to
    pub fn current_word_range(&self) -> Range<usize> {
        let right_index = self.word_right_index();
        let left_index = self.lines[..right_index]
            .split_word_bound_indices()
            .filter(|(_, word)| !is_word_boundary(word))
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);

        left_index..right_index
    }

    /// Range over the current line
    ///
    /// Starts on the first non-newline character and is an exclusive range
    /// extending beyond the potential carriage return and line feed characters
    /// terminating the line
    pub fn current_line_range(&self) -> Range<usize> {
        let left_index = self.lines[..self.insertion_point.offset]
            .rfind('\n')
            .map_or(0, |offset| offset + 1);
        let right_index = self.lines[self.insertion_point.offset..]
            .find('\n')
            .map_or(self.lines.len(), |i| i + self.insertion_point.offset + 1);

        left_index..right_index
    }

    /// Uppercases the current word
    pub fn uppercase_word(&mut self) {
        let change_range = self.current_word_range();
        let uppercased = self.get_buffer()[change_range.clone()].to_uppercase();
        self.replace_range(change_range, &uppercased);
        self.move_word_right();
    }

    /// Lowercases the current word
    pub fn lowercase_word(&mut self) {
        let change_range = self.current_word_range();
        let uppercased = self.get_buffer()[change_range.clone()].to_lowercase();
        self.replace_range(change_range, &uppercased);
        self.move_word_right();
    }

    /// Counts the number of words in the buffer
    pub fn word_count(&self) -> usize {
        self.lines.trim().split_whitespace().count()
    }

    /// Capitallize the character at insertion point and move the insertion point right one
    /// grapheme.
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
            self.move_right();
        }
    }

    /// Deletes on grapheme to the left
    pub fn delete_left_grapheme(&mut self) {
        let left_index = self.grapheme_left_index();
        let insertion_offset = self.insertion_point().offset;
        if left_index < insertion_offset {
            self.clear_range(left_index..insertion_offset);
            self.insertion_point.offset = left_index;
        }
    }

    /// Deletes one grapheme to the right
    pub fn delete_right_grapheme(&mut self) {
        let right_index = self.grapheme_right_index();
        let insertion_offset = self.insertion_point().offset;
        if right_index > insertion_offset {
            self.clear_range(insertion_offset..right_index);
        }
    }

    /// Deletes one word to the left
    pub fn delete_word_left(&mut self) {
        let left_word_index = self.word_left_index();
        self.clear_range(left_word_index..self.insertion_point().offset);
        self.insertion_point.offset = left_word_index;
    }

    /// Deletes one word to the right
    pub fn delete_word_right(&mut self) {
        let right_word_index = self.word_right_index();
        self.clear_range(self.insertion_point().offset..right_word_index);
    }

    /// Swaps current word with word on right
    pub fn swap_words(&mut self) {
        let word_1_range = self.current_word_range();
        self.move_word_right();
        let word_2_range = self.current_word_range();

        if word_1_range != word_2_range {
            self.move_word_left();
            let insertion_line = self.get_buffer();
            let word_1 = insertion_line[word_1_range.clone()].to_string();
            let word_2 = insertion_line[word_2_range.clone()].to_string();
            self.replace_range(word_2_range, &word_1);
            self.replace_range(word_1_range, &word_2);
        }
    }

    /// Swaps current grapheme with grapheme on right
    pub fn swap_graphemes(&mut self) {
        let initial_offset = self.insertion_point().offset;

        if initial_offset == 0 {
            self.move_right();
        } else if initial_offset == self.get_buffer().len() {
            self.move_left();
        }

        let updated_offset = self.insertion_point().offset;
        let grapheme_1_start = self.grapheme_left_index();
        let grapheme_2_end = self.grapheme_right_index();

        if grapheme_1_start < updated_offset && grapheme_2_end > updated_offset {
            let grapheme_1 = self.get_buffer()[grapheme_1_start..updated_offset].to_string();
            let grapheme_2 = self.get_buffer()[updated_offset..grapheme_2_end].to_string();
            self.replace_range(updated_offset..grapheme_2_end, &grapheme_1);
            self.replace_range(grapheme_1_start..updated_offset, &grapheme_2);
            self.insertion_point.offset = grapheme_2_end;
        } else {
            self.insertion_point.offset = updated_offset;
        }
    }

    /// Moves one line up
    pub fn move_line_up(&mut self) {
        if !self.is_cursor_at_first_line() {
            // If we're not at the top, move up a line in the multiline buffer
            let mut position = self.offset();
            let mut num_of_move_lefts = 0;
            let buffer = self.get_buffer().to_string();

            // Move left until we're looking at the newline
            // Observe what column we were on
            while position > 0 && &buffer[(position - 1)..position] != "\n" {
                self.move_left();
                num_of_move_lefts += 1;
                position = self.offset();
            }

            // Find start of previous line
            let mut matches = buffer[0..(position - 1)].rmatch_indices('\n');

            if let Some((pos, _)) = matches.next() {
                position = pos + 1;
            } else {
                position = 0;
            }
            self.set_insertion_point(position);

            // Move right from this position to the column we were at
            while &buffer[position..=position] != "\n" && num_of_move_lefts > 0 {
                self.move_right();
                position = self.offset();
                num_of_move_lefts -= 1;
            }
        }
    }

    /// Moves one line down
    pub fn move_line_down(&mut self) {
        if !self.is_cursor_at_last_line() {
            // If we're not at the top, move up a line in the multiline buffer
            let mut position = self.offset();
            let mut num_of_move_lefts = 0;
            let buffer = self.get_buffer().to_string();

            // Move left until we're looking at the newline
            // Observe what column we were on
            while position > 0 && &buffer[(position - 1)..position] != "\n" {
                self.move_left();
                num_of_move_lefts += 1;
                position = self.offset();
            }

            // Find start of next line
            let mut matches = buffer[position..].match_indices('\n');

            // Assume this always succeeds

            let (pos, _) = matches
                .next()
                .expect("internal error: should have found newline");

            position += pos + 1;

            self.set_insertion_point(position);

            // Move right from this position to the column we were at
            while position < buffer.len()
                && &buffer[position..=position] != "\n"
                && num_of_move_lefts > 0
            {
                self.move_right();
                position = self.offset();
                num_of_move_lefts -= 1;
            }
        }
    }

    /// Checks to see if the cursor is on the first line of the buffer
    pub fn is_cursor_at_first_line(&self) -> bool {
        !self.get_buffer()[0..self.offset()].contains('\n')
    }

    /// Checks to see if the cursor is on the last line of the buffer
    pub fn is_cursor_at_last_line(&self) -> bool {
        !self.get_buffer()[self.offset()..].contains('\n')
    }

    /// Finds index for the first occurrence of a char to the right of offset
    pub fn find_char_right(&self, c: char) -> Option<usize> {
        if self.offset() + 1 > self.lines.len() {
            return None;
        }

        let search_str = &self.lines[self.grapheme_right_index()..];
        search_str
            .find(c)
            .map(|index| index + self.grapheme_right_index())
    }

    /// Finds index for the first occurrence of a char to the left of offset
    pub fn find_char_left(&self, c: char) -> Option<usize> {
        if self.offset() + 1 > self.lines.len() {
            return None;
        }

        let search_str = &self.lines[..self.offset()];
        search_str.rfind(c)
    }

    /// Moves the insertion point until the next char to the right
    pub fn move_right_until(&mut self, c: char) -> usize {
        if let Some(index) = self.find_char_right(c) {
            self.insertion_point.offset = index;
        }

        self.insertion_point.offset
    }

    /// Moves the insertion point before the next char to the right
    pub fn move_right_before(&mut self, c: char) -> usize {
        if let Some(index) = self.find_char_right(c) {
            self.insertion_point.offset = index;
            self.insertion_point.offset = self.grapheme_left_index();
        }

        self.insertion_point.offset
    }

    /// Moves the insertion point until the next char to the left of offset
    pub fn move_left_until(&mut self, c: char) -> usize {
        if let Some(index) = self.find_char_left(c) {
            self.insertion_point.offset = index;
        }

        self.insertion_point.offset
    }

    /// Moves the insertion point before the next char to the left of offset
    pub fn move_left_before(&mut self, c: char) -> usize {
        if let Some(index) = self.find_char_left(c) {
            self.insertion_point.offset = index + c.len_utf8();
        }

        self.insertion_point.offset
    }

    /// Deletes until first character to the right of offset
    pub fn delete_right_until_char(&mut self, c: char) {
        if let Some(index) = self.find_char_right(c) {
            self.clear_range(self.offset()..index + c.len_utf8());
        }
    }

    /// Deletes before first character to the right of offset
    pub fn delete_right_before_char(&mut self, c: char) {
        if let Some(index) = self.find_char_right(c) {
            self.clear_range(self.offset()..index);
        }
    }

    /// Deletes until first character to the left of offset
    pub fn delete_left_until_char(&mut self, c: char) {
        if let Some(index) = self.find_char_left(c) {
            self.clear_range(index..self.offset());
            self.insertion_point.offset = index;
        }
    }

    /// Deletes before first character to the left of offset
    pub fn delete_left_before_char(&mut self, c: char) {
        if let Some(index) = self.find_char_left(c) {
            self.clear_range(index + c.len_utf8()..self.offset());
            self.insertion_point.offset = index + c.len_utf8();
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
    use rstest::rstest;

    fn buffer_with(content: &str) -> LineBuffer {
        let mut line_buffer = LineBuffer::new();
        line_buffer.insert_str(content);

        line_buffer
    }

    #[test]
    fn test_new_buffer_is_empty() {
        let line_buffer = LineBuffer::new();
        assert!(line_buffer.is_empty());
    }

    #[test]
    fn test_clearing_line_buffer_resets_buffer_and_insertion_point() {
        let mut buffer = buffer_with("this is a command");
        buffer.clear();
        let empty_buffer = LineBuffer::new();

        assert_eq!(buffer, empty_buffer);
    }

    #[test]
    fn insert_str_updates_insertion_point_point_correctly() {
        let mut line_buffer = LineBuffer::new();
        line_buffer.insert_str("this is a command");

        let expected_updated_insertion_point = InsertionPoint { offset: 17 };

        assert_eq!(
            expected_updated_insertion_point,
            line_buffer.insertion_point()
        );
    }

    #[test]
    fn insert_char_updates_insertion_point_point_correctly() {
        let mut line_buffer = LineBuffer::new();
        line_buffer.insert_char('c');

        let expected_updated_insertion_point = InsertionPoint { offset: 1 };

        assert_eq!(
            expected_updated_insertion_point,
            line_buffer.insertion_point()
        );
    }

    #[rstest]
    #[case("new string", InsertionPoint { offset: 10})]
    #[case("new line1\nnew line 2", InsertionPoint { offset: 20})]
    fn set_buffer_updates_insertion_point_to_new_buffer_length(
        #[case] string_to_set: &str,
        #[case] expected_insertion_point: InsertionPoint,
    ) {
        let mut line_buffer = buffer_with("test string");
        let before_operation_location = InsertionPoint { offset: 11 };
        assert_eq!(before_operation_location, line_buffer.insertion_point());

        line_buffer.set_buffer(string_to_set.to_string());

        assert_eq!(expected_insertion_point, line_buffer.insertion_point());
    }

    #[rstest]
    #[case("This is a test", "This is a tes")]
    #[case("This is a test 😊", "This is a test ")]
    #[case("", "")]
    fn delete_left_grapheme_works(#[case] input: &str, #[case] expected: &str) {
        let mut line_buffer = buffer_with(input);
        line_buffer.delete_left_grapheme();

        let expected_line_buffer = buffer_with(expected);

        assert_eq!(expected_line_buffer, line_buffer);
    }

    #[rstest]
    #[case("This is a test", "This is a tes")]
    #[case("This is a test 😊", "This is a test ")]
    #[case("", "")]
    fn delete_right_grapheme_works(#[case] input: &str, #[case] expected: &str) {
        let mut line_buffer = buffer_with(input);
        line_buffer.move_left();
        line_buffer.delete_right_grapheme();

        let expected_line_buffer = buffer_with(expected);

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

    #[rstest]
    #[case("This is a te", 4)]
    #[case("This is a test", 4)]
    #[case("This      is a test", 4)]
    fn word_count_works(#[case] input: &str, #[case] expected_count: usize) {
        let line_buffer1 = buffer_with(input);

        assert_eq!(expected_count, line_buffer1.word_count());
    }

    #[test]
    fn word_count_works_with_multiple_spaces() {
        let line_buffer = buffer_with("This   is a test");

        assert_eq!(4, line_buffer.word_count());
    }

    #[rstest]
    #[case("This is a test", 13, "This is a tesT", 14)]
    #[case("This is a test", 10, "This is a Test", 11)]
    fn capitalize_char_works(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] output: &str,
        #[case] out_location: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);
        line_buffer.capitalize_char();

        let mut expected = buffer_with(output);
        expected.set_insertion_point(out_location);

        assert_eq!(expected, line_buffer);
    }

    #[rstest]
    #[case("This is a test", 13, "This is a TEST", 14)]
    #[case("This is a test", 10, "This is a TEST", 14)]
    #[case("", 0, "", 0)]
    #[case("This", 0, "THIS", 4)]
    #[case("This", 4, "THIS", 4)]
    fn uppercase_word_works(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] output: &str,
        #[case] out_location: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);
        line_buffer.uppercase_word();

        let mut expected = buffer_with(output);
        expected.set_insertion_point(out_location);

        assert_eq!(expected, line_buffer);
    }

    #[rstest]
    #[case("This is a TEST", 13, "This is a test", 14)]
    #[case("This is a TEST", 10, "This is a test", 14)]
    #[case("", 0, "", 0)]
    #[case("THIS", 0, "this", 4)]
    #[case("THIS", 4, "this", 4)]
    fn lowercase_word_works(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] output: &str,
        #[case] out_location: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);
        line_buffer.lowercase_word();

        let mut expected = buffer_with(output);
        expected.set_insertion_point(out_location);

        assert_eq!(expected, line_buffer);
    }

    #[rstest]
    #[case("This is a test", 13, "This is a tets", 14)]
    #[case("This is a test", 14, "This is a tets", 14)] // NOTE: Swaping works in opposite direction at last index
    #[case("This is a test", 4, "Thi sis a test", 5)] // NOTE: Swaps space, moves right
    #[case("This is a test", 0, "hTis is a test", 2)]
    fn swap_graphemes_work(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] output: &str,
        #[case] out_location: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        line_buffer.swap_graphemes();

        let mut expected = buffer_with(output);
        expected.set_insertion_point(out_location);

        assert_eq!(line_buffer, expected);
    }

    #[rstest]
    #[case("This is a test", 8, "This is test a", 8)]
    #[case("This is a test", 0, "is This a test", 0)]
    #[case("This is a test", 14, "This is a test", 14)]
    fn swap_words_works(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] output: &str,
        #[case] out_location: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        line_buffer.swap_words();

        let mut expected = buffer_with(output);
        expected.set_insertion_point(out_location);

        assert_eq!(line_buffer, expected);
    }

    #[rstest]
    #[case("line 1\nline 2", 7, "line 1\nline 2", 0)]
    #[case("line 1\nline 2", 0, "line 1\nline 2", 0)]
    #[case("line\nlong line", 14, "line\nlong line", 4)]
    #[case("line\nlong line", 8, "line\nlong line", 3)]
    fn moving_up_works(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] output: &str,
        #[case] out_location: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        line_buffer.move_line_up();

        let mut expected = buffer_with(output);
        expected.set_insertion_point(out_location);

        assert_eq!(line_buffer, expected);
    }

    #[rstest]
    #[case("line 1\nline 2", 0, "line 1\nline 2", 7)]
    #[case("line 1\nline 2", 7, "line 1\nline 2", 7)]
    #[case("long line\nline", 8, "long line\nline", 14)]
    #[case("long line\nline", 4, "long line\nline", 14)]
    #[case("long line\nline", 3, "long line\nline", 13)]
    fn moving_down_works(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] output: &str,
        #[case] out_location: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        line_buffer.move_line_down();

        let mut expected = buffer_with(output);
        expected.set_insertion_point(out_location);

        assert_eq!(line_buffer, expected);
    }

    #[rstest]
    #[case("line 1\nline 2\nline 3", 0, true)]
    #[case("line 1\nline 2\nline 3", 6, true)]
    #[case("line 1\nline 2\nline 3", 8, false)]
    fn test_first_line_detection(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] expected: bool,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        assert_eq!(line_buffer.is_cursor_at_first_line(), expected);
    }

    #[rstest]
    #[case("line", 4, true)]
    #[case("line\nline", 9, true)]
    #[case("line 1\nline 2\nline 3", 8, false)]
    #[case("line 1\nline 2\nline 3", 13, false)]
    #[case("line 1\nline 2\nline 3", 14, true)]
    #[case("line 1\nline 2\nline 3", 20, true)]
    #[case("line 1\nline 2\nline 3\n", 20, false)]
    #[case("line 1\nline 2\nline 3\n", 21, true)]
    fn test_last_line_detection(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] expected: bool,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        assert_eq!(line_buffer.is_cursor_at_last_line(), expected);
    }

    #[rstest]
    #[case("abc def ghi", 0, 'd', "ef ghi")]
    #[case("abc def ghi", 0, 'i', "")]
    #[case("abc def ghi", 0, 'z', "abc def ghi")]
    #[case("abc def ghi", 2, 'd', "abef ghi")]
    #[case("abc def chi", 2, 'c', "abhi")]
    #[case("abc def chi", 8, 'i', "abc def ")]
    fn test_delete_until(
        #[case] input: &str,
        #[case] position: usize,
        #[case] c: char,
        #[case] expected: &str,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        line_buffer.delete_right_until_char(c);

        assert_eq!(line_buffer.lines, expected);
    }

    #[rstest]
    #[case("abc def ghi", 0, 'd', "def ghi")]
    #[case("abc def ghi", 0, 'i', "i")]
    #[case("abc def ghi", 0, 'z', "abc def ghi")]
    #[case("abc def ghi", 2, 'd', "abdef ghi")]
    #[case("abc def chi", 2, 'c', "abchi")]
    #[case("abc def chi", 8, 'i', "abc def i")]
    fn test_delete_before(
        #[case] input: &str,
        #[case] position: usize,
        #[case] c: char,
        #[case] expected: &str,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        line_buffer.delete_right_before_char(c);

        assert_eq!(line_buffer.lines, expected);
    }

    #[rstest]
    #[case("abc def ghi", 4, 'c', Some(2))]
    #[case("abc def ghi", 0, 'a', None)]
    #[case("abc def ghi", 6, 'a', Some(0))]
    fn find_char_left(
        #[case] input: &str,
        #[case] position: usize,
        #[case] c: char,
        #[case] expected: Option<usize>,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        assert_eq!(line_buffer.find_char_left(c), expected);
    }

    #[rstest]
    #[case("abc def ghi", 5, 'b', "aef ghi")]
    #[case("abc def ghi", 5, 'e', "abc def ghi")]
    #[case("abc def ghi", 10, 'a', "i")]
    fn test_delete_until_left(
        #[case] input: &str,
        #[case] position: usize,
        #[case] c: char,
        #[case] expected: &str,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        line_buffer.delete_left_until_char(c);

        assert_eq!(line_buffer.lines, expected);
    }

    #[rstest]
    #[case("abc def ghi", 5, 'b', "abef ghi")]
    #[case("abc def ghi", 5, 'e', "abc def ghi")]
    #[case("abc def ghi", 10, 'a', "ai")]
    fn test_delete_before_left(
        #[case] input: &str,
        #[case] position: usize,
        #[case] c: char,
        #[case] expected: &str,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        line_buffer.delete_left_before_char(c);

        assert_eq!(line_buffer.lines, expected);
    }

    #[rstest]
    #[case("line", 0, 4)]
    #[case("line\nline", 1, 4)]
    #[case("line\nline", 7, 9)]
    // TODO: Check if this behavior is desired for full vi consistency
    #[case("line\n", 4, 4)]
    #[case("line\n", 5, 5)]
    // Platform agnostic
    #[case("\n", 0, 0)]
    #[case("\r\n", 0, 0)]
    #[case("line\r\nword", 1, 4)]
    #[case("line\r\nword", 7, 10)]
    fn test_find_current_line_end(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] expected: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        assert_eq!(line_buffer.find_current_line_end(), expected);
    }

    #[rstest]
    #[case("", 0, 0)]
    #[case("\n", 0, 0)]
    #[case("\n", 1, 1)]
    #[case("a\nb", 0, 0)]
    #[case("a\nb", 1, 0)]
    #[case("a\nb", 2, 1)]
    #[case("a\nbc", 3, 1)]
    #[case("a\r\nb", 3, 1)]
    #[case("a\r\nbc", 4, 1)]
    fn test_current_line_num(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] expected: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        assert_eq!(line_buffer.line(), expected);
    }

    #[rstest]
    #[case("", 0, 1)]
    #[case("line", 0, 1)]
    #[case("\n", 0, 2)]
    #[case("line\n", 0, 2)]
    #[case("a\nb", 0, 2)]
    fn test_num_lines(#[case] input: &str, #[case] in_location: usize, #[case] expected: usize) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        assert_eq!(line_buffer.num_lines(), expected);
    }
}
