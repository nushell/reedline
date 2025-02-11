use {
    itertools::Itertools,
    std::{convert::From, ops::Range},
    unicode_segmentation::UnicodeSegmentation,
};

/// In memory representation of the entered line(s) including a cursor position to facilitate cursor based editing.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct LineBuffer {
    lines: String,
    insertion_point: usize,
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
        Self::default()
    }

    /// Check to see if the line buffer is empty
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Check if the line buffer is valid utf-8 and the cursor sits on a valid grapheme boundary
    pub fn is_valid(&self) -> bool {
        self.lines.is_char_boundary(self.insertion_point())
            && (self
                .lines
                .grapheme_indices(true)
                .any(|(i, _)| i == self.insertion_point())
                || self.insertion_point() == self.lines.len())
            && std::str::from_utf8(self.lines.as_bytes()).is_ok()
    }

    #[cfg(test)]
    fn assert_valid(&self) {
        assert!(
            self.lines.is_char_boundary(self.insertion_point()),
            "Not on valid char boundary"
        );
        assert!(
            self.lines
                .grapheme_indices(true)
                .any(|(i, _)| i == self.insertion_point())
                || self.insertion_point() == self.lines.len(),
            "Not on valid grapheme"
        );
        assert!(
            std::str::from_utf8(self.lines.as_bytes()).is_ok(),
            "Not valid utf-8"
        );
    }

    /// Gets the current edit position
    pub fn insertion_point(&self) -> usize {
        self.insertion_point
    }

    /// Sets the current edit position
    /// ## Unicode safety:
    /// Not checked, improper use may cause panics in following operations
    pub fn set_insertion_point(&mut self, offset: usize) {
        self.insertion_point = offset;
    }

    /// Output the current line in the multiline buffer
    pub fn get_buffer(&self) -> &str {
        &self.lines
    }

    /// Set to a single line of `buffer` and reset the `InsertionPoint` cursor to the end
    pub fn set_buffer(&mut self, buffer: String) {
        self.lines = buffer;
        self.insertion_point = self.lines.len();
    }

    /// Calculates the current the user is on
    ///
    /// Zero-based index
    pub fn line(&self) -> usize {
        self.lines[..self.insertion_point].matches('\n').count()
    }

    /// Counts the number of lines in the buffer
    pub fn num_lines(&self) -> usize {
        self.lines.split('\n').count()
    }

    /// Checks to see if the buffer ends with a given character
    pub fn ends_with(&self, c: char) -> bool {
        self.lines.ends_with(c)
    }

    /// Reset the insertion point to the start of the buffer
    pub fn move_to_start(&mut self) {
        self.insertion_point = 0;
    }

    /// Move the cursor before the first character of the line
    pub fn move_to_line_start(&mut self) {
        self.insertion_point = self.lines[..self.insertion_point]
            .rfind('\n')
            .map_or(0, |offset| offset + 1);
        // str is guaranteed to be utf8, thus \n is safe to assume 1 byte long
    }

    /// Move cursor position to the end of the line
    ///
    /// Insertion will append to the line.
    /// Cursor on top of the potential `\n` or `\r` of `\r\n`
    pub fn move_to_line_end(&mut self) {
        self.insertion_point = self.find_current_line_end();
    }

    /// Set the insertion point *behind* the last character.
    pub fn move_to_end(&mut self) {
        self.insertion_point = self.lines.len();
    }

    /// Get the length of the buffer
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Returns where the current line terminates
    ///
    /// Either:
    /// - end of buffer (`len()`)
    /// - `\n` or `\r\n` (on the first byte)
    pub fn find_current_line_end(&self) -> usize {
        self.lines[self.insertion_point..].find('\n').map_or_else(
            || self.lines.len(),
            |i| {
                let absolute_index = i + self.insertion_point;
                if absolute_index > 0 && self.lines.as_bytes()[absolute_index - 1] == b'\r' {
                    absolute_index - 1
                } else {
                    absolute_index
                }
            },
        )
    }

    /// Cursor position *behind* the next unicode grapheme to the right
    pub fn grapheme_right_index(&self) -> usize {
        self.lines[self.insertion_point..]
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| self.insertion_point + i)
            .unwrap_or_else(|| self.lines.len())
    }

    /// Cursor position *in front of* the next unicode grapheme to the left
    pub fn grapheme_left_index(&self) -> usize {
        self.lines[..self.insertion_point]
            .grapheme_indices(true)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Cursor position *behind* the next unicode grapheme to the right from the given position
    pub fn grapheme_right_index_from_pos(&self, pos: usize) -> usize {
        self.lines[pos..]
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| pos + i)
            .unwrap_or_else(|| self.lines.len())
    }

    /// Cursor position *behind* the next word to the right
    pub fn word_right_index(&self) -> usize {
        self.lines[self.insertion_point..]
            .split_word_bound_indices()
            .find(|(_, word)| !is_whitespace_str(word))
            .map(|(i, word)| self.insertion_point + i + word.len())
            .unwrap_or_else(|| self.lines.len())
    }

    /// Cursor position *behind* the next WORD to the right
    pub fn big_word_right_index(&self) -> usize {
        let mut found_ws = false;

        self.lines[self.insertion_point..]
            .split_word_bound_indices()
            .find(|(_, word)| {
                found_ws = found_ws || is_whitespace_str(word);
                found_ws && !is_whitespace_str(word)
            })
            .map(|(i, word)| self.insertion_point + i + word.len())
            .unwrap_or_else(|| self.lines.len())
    }

    /// Cursor position *at end of* the next word to the right
    pub fn word_right_end_index(&self) -> usize {
        self.lines[self.insertion_point..]
            .split_word_bound_indices()
            .find_map(|(i, word)| {
                word.grapheme_indices(true)
                    .next_back()
                    .map(|x| self.insertion_point + x.0 + i)
                    .filter(|x| !is_whitespace_str(word) && *x != self.insertion_point)
            })
            .unwrap_or_else(|| {
                self.lines
                    .grapheme_indices(true)
                    .last()
                    .map(|x| x.0)
                    .unwrap_or(0)
            })
    }

    /// Cursor position *at end of* the next WORD to the right
    pub fn big_word_right_end_index(&self) -> usize {
        self.lines[self.insertion_point..]
            .split_word_bound_indices()
            .tuple_windows()
            .find_map(|((prev_i, prev_word), (_, word))| {
                if is_whitespace_str(word) {
                    prev_word
                        .grapheme_indices(true)
                        .next_back()
                        .map(|x| self.insertion_point + x.0 + prev_i)
                        .filter(|x| *x != self.insertion_point)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                self.lines
                    .grapheme_indices(true)
                    .last()
                    .map(|x| x.0)
                    .unwrap_or(0)
            })
    }

    /// Cursor position *in front of* the next word to the right
    pub fn word_right_start_index(&self) -> usize {
        self.lines[self.insertion_point..]
            .split_word_bound_indices()
            .find(|(i, word)| *i != 0 && !is_whitespace_str(word))
            .map(|(i, _)| self.insertion_point + i)
            .unwrap_or_else(|| self.lines.len())
    }

    /// Cursor position *in front of* the next WORD to the right
    pub fn big_word_right_start_index(&self) -> usize {
        let mut found_ws = false;

        self.lines[self.insertion_point..]
            .split_word_bound_indices()
            .find(|(i, word)| {
                found_ws = found_ws || *i != 0 && is_whitespace_str(word);
                found_ws && *i != 0 && !is_whitespace_str(word)
            })
            .map(|(i, _)| self.insertion_point + i)
            .unwrap_or_else(|| self.lines.len())
    }

    /// Cursor position *in front of* the next word to the left
    pub fn word_left_index(&self) -> usize {
        self.lines[..self.insertion_point]
            .split_word_bound_indices()
            .filter(|(_, word)| !is_whitespace_str(word))
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Cursor position *in front of* the next WORD to the left
    pub fn big_word_left_index(&self) -> usize {
        self.lines[..self.insertion_point]
            .split_word_bound_indices()
            .fold(None, |last_word_index, (i, word)| {
                match (last_word_index, is_whitespace_str(word)) {
                    (None, true) => None,
                    (None, false) => Some(i),
                    (Some(v), true) => {
                        if is_whitespace_str(&self.lines[i..self.insertion_point]) {
                            Some(v)
                        } else {
                            None
                        }
                    }
                    (Some(v), false) => Some(v),
                }
            })
            .unwrap_or(0)
    }

    /// Cursor position on the next whitespace
    pub fn next_whitespace(&self) -> usize {
        self.lines[self.insertion_point..]
            .split_word_bound_indices()
            .find(|(i, word)| *i != 0 && is_whitespace_str(word))
            .map(|(i, _)| self.insertion_point + i)
            .unwrap_or_else(|| self.lines.len())
    }

    /// Move cursor position *behind* the next unicode grapheme to the right
    pub fn move_right(&mut self) {
        self.insertion_point = self.grapheme_right_index();
    }

    /// Move cursor position *in front of* the next unicode grapheme to the left
    pub fn move_left(&mut self) {
        self.insertion_point = self.grapheme_left_index();
    }

    /// Move cursor position *in front of* the next word to the left
    pub fn move_word_left(&mut self) {
        self.insertion_point = self.word_left_index();
    }

    /// Move cursor position *in front of* the next WORD to the left
    pub fn move_big_word_left(&mut self) {
        self.insertion_point = self.big_word_left_index();
    }

    /// Move cursor position *behind* the next word to the right
    pub fn move_word_right(&mut self) {
        self.insertion_point = self.word_right_index();
    }

    /// Move cursor position to the start of the next word
    pub fn move_word_right_start(&mut self) {
        self.insertion_point = self.word_right_start_index();
    }

    /// Move cursor position to the start of the next WORD
    pub fn move_big_word_right_start(&mut self) {
        self.insertion_point = self.big_word_right_start_index();
    }

    /// Move cursor position to the end of the next word
    pub fn move_word_right_end(&mut self) {
        self.insertion_point = self.word_right_end_index();
    }

    /// Move cursor position to the end of the next WORD
    pub fn move_big_word_right_end(&mut self) {
        self.insertion_point = self.big_word_right_end_index();
    }

    ///Insert a single character at the insertion point and move right
    pub fn insert_char(&mut self, c: char) {
        self.lines.insert(self.insertion_point, c);
        self.move_right();
    }

    /// Insert `&str` at the cursor position in the current line.
    ///
    /// Sets cursor to end of inserted string
    ///
    /// ## Unicode safety:
    /// Does not validate the incoming string or the current cursor position
    pub fn insert_str(&mut self, string: &str) {
        self.lines.insert_str(self.insertion_point(), string);
        self.insertion_point = self.insertion_point() + string.len();
    }

    /// Inserts the system specific new line character
    ///
    /// - On Unix systems LF (`"\n"`)
    /// - On Windows CRLF (`"\r\n"`)
    pub fn insert_newline(&mut self) {
        #[cfg(target_os = "windows")]
        self.insert_str("\r\n");
        #[cfg(not(target_os = "windows"))]
        self.insert_char('\n');
    }

    /// Empty buffer and reset cursor
    pub fn clear(&mut self) {
        self.lines = String::new();
        self.insertion_point = 0;
    }

    /// Clear everything beginning at the cursor to the right/end.
    /// Keeps the cursor at the end.
    pub fn clear_to_end(&mut self) {
        self.lines.truncate(self.insertion_point);
    }

    /// Clear beginning at the cursor up to the end of the line.
    /// Newline character at the end remains.
    pub fn clear_to_line_end(&mut self) {
        self.clear_range(self.insertion_point..self.find_current_line_end());
    }

    /// Clear from the start of the buffer to the cursor.
    /// Keeps the cursor at the beginning of the line/buffer.
    pub fn clear_to_insertion_point(&mut self) {
        self.clear_range(..self.insertion_point);
        self.insertion_point = 0;
    }

    /// Clear all contents between `start` and `end` and change insertion point if necessary.
    ///
    /// If the cursor is located between `start` and `end` it is adjusted to `start`.
    /// If the cursor is located after `end` it is adjusted to stay at its current char boundary.
    pub fn clear_range_safe(&mut self, start: usize, end: usize) {
        let (start, end) = if start > end {
            (end, start)
        } else {
            (start, end)
        };
        if self.insertion_point <= start {
            // No action necessary
        } else if self.insertion_point < end {
            self.insertion_point = start;
        } else {
            // Insertion point after end
            self.insertion_point -= end - start;
        }
        self.clear_range(start..end);
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
    pub fn replace_range<R>(&mut self, range: R, replace_with: &str)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.lines.replace_range(range, replace_with);
    }

    /// Checks to see if the current edit position is pointing to whitespace
    pub fn on_whitespace(&self) -> bool {
        self.lines[self.insertion_point..]
            .chars()
            .next()
            .map(char::is_whitespace)
            .unwrap_or(false)
    }

    /// Get the grapheme immediately to the right of the cursor, if any
    pub fn grapheme_right(&self) -> &str {
        &self.lines[self.insertion_point..self.grapheme_right_index()]
    }

    /// Get the grapheme immediately to the left of the cursor, if any
    pub fn grapheme_left(&self) -> &str {
        &self.lines[self.grapheme_left_index()..self.insertion_point]
    }

    /// Gets the range of the word the current edit position is pointing to
    pub fn current_word_range(&self) -> Range<usize> {
        let right_index = self.word_right_index();
        let left_index = self.lines[..right_index]
            .split_word_bound_indices()
            .filter(|(_, word)| !is_whitespace_str(word))
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
        let left_index = self.lines[..self.insertion_point]
            .rfind('\n')
            .map_or(0, |offset| offset + 1);
        let right_index = self.lines[self.insertion_point..]
            .find('\n')
            .map_or_else(|| self.lines.len(), |i| i + self.insertion_point + 1);

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

    /// Switches the ASCII case of the current char
    pub fn switchcase_char(&mut self) {
        let insertion_offset = self.insertion_point();
        let right_index = self.grapheme_right_index();

        if right_index > insertion_offset {
            let change_range = insertion_offset..right_index;
            let swapped = self.get_buffer()[change_range.clone()]
                .chars()
                .map(|c| {
                    if c.is_ascii_uppercase() {
                        c.to_ascii_lowercase()
                    } else {
                        c.to_ascii_uppercase()
                    }
                })
                .collect::<String>();
            self.replace_range(change_range, &swapped);
            self.move_right();
        }
    }

    /// Capitalize the character at insertion point (or the first character
    /// following the whitespace at the insertion point) and move the insertion
    /// point right one grapheme.
    pub fn capitalize_char(&mut self) {
        if self.on_whitespace() {
            self.move_word_right();
            self.move_word_left();
        }
        let insertion_offset = self.insertion_point();
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
        let insertion_offset = self.insertion_point();
        if left_index < insertion_offset {
            self.clear_range(left_index..insertion_offset);
            self.insertion_point = left_index;
        }
    }

    /// Deletes one grapheme to the right
    pub fn delete_right_grapheme(&mut self) {
        let right_index = self.grapheme_right_index();
        let insertion_offset = self.insertion_point();
        if right_index > insertion_offset {
            self.clear_range(insertion_offset..right_index);
        }
    }

    /// Deletes one word to the left
    pub fn delete_word_left(&mut self) {
        let left_word_index = self.word_left_index();
        self.clear_range(left_word_index..self.insertion_point());
        self.insertion_point = left_word_index;
    }

    /// Deletes one word to the right
    pub fn delete_word_right(&mut self) {
        let right_word_index = self.word_right_index();
        self.clear_range(self.insertion_point()..right_word_index);
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
        let initial_offset = self.insertion_point();

        if initial_offset == 0 {
            self.move_right();
        } else if initial_offset == self.get_buffer().len() {
            self.move_left();
        }

        let updated_offset = self.insertion_point();
        let grapheme_1_start = self.grapheme_left_index();
        let grapheme_2_end = self.grapheme_right_index();

        if grapheme_1_start < updated_offset && grapheme_2_end > updated_offset {
            let grapheme_1 = self.get_buffer()[grapheme_1_start..updated_offset].to_string();
            let grapheme_2 = self.get_buffer()[updated_offset..grapheme_2_end].to_string();
            self.replace_range(updated_offset..grapheme_2_end, &grapheme_1);
            self.replace_range(grapheme_1_start..updated_offset, &grapheme_2);
            self.insertion_point = grapheme_2_end;
        } else {
            self.insertion_point = updated_offset;
        }
    }

    /// Moves one line up
    pub fn move_line_up(&mut self) {
        if !self.is_cursor_at_first_line() {
            let old_range = self.current_line_range();

            let grapheme_col = self.lines[old_range.start..self.insertion_point()]
                .graphemes(true)
                .count();

            // Platform independent way to jump to the previous line.
            // Doesn't matter if `\n` or `\r\n` terminated line.
            // Maybe replace with more explicit implementation.
            self.set_insertion_point(old_range.start);
            self.move_left();

            let new_range = self.current_line_range();
            let new_line = &self.lines[new_range.clone()];

            self.insertion_point = new_line
                .grapheme_indices(true)
                .take(grapheme_col + 1)
                .last()
                .map_or(new_range.start, |(i, _)| i + new_range.start);
        }
    }

    /// Moves one line down
    pub fn move_line_down(&mut self) {
        if !self.is_cursor_at_last_line() {
            let old_range = self.current_line_range();

            let grapheme_col = self.lines[old_range.start..self.insertion_point()]
                .graphemes(true)
                .count();

            // Exclusive range, thus guaranteed to be in the next line
            self.set_insertion_point(old_range.end);

            let new_range = self.current_line_range();
            let new_line = &self.lines[new_range.clone()];

            // Slightly different to move_line_up to account for the special
            // case of the last line without newline char at the end.
            // -> use `self.find_current_line_end()`
            self.insertion_point = new_line
                .grapheme_indices(true)
                .nth(grapheme_col)
                .map_or_else(
                    || self.find_current_line_end(),
                    |(i, _)| i + new_range.start,
                );
        }
    }

    /// Checks to see if the cursor is on the first line of the buffer
    pub fn is_cursor_at_first_line(&self) -> bool {
        !self.get_buffer()[0..self.insertion_point()].contains('\n')
    }

    /// Checks to see if the cursor is on the last line of the buffer
    pub fn is_cursor_at_last_line(&self) -> bool {
        !self.get_buffer()[self.insertion_point()..].contains('\n')
    }

    /// Finds index for the first occurrence of a char to the right of offset
    pub fn find_char_right(&self, c: char, current_line: bool) -> Option<usize> {
        // Skip current grapheme
        let char_offset = self.grapheme_right_index();
        let range = if current_line {
            char_offset..self.current_line_range().end
        } else {
            char_offset..self.lines.len()
        };
        self.lines[range].find(c).map(|index| index + char_offset)
    }

    /// Finds index for the first occurrence of a char to the left of offset
    pub fn find_char_left(&self, c: char, current_line: bool) -> Option<usize> {
        let range = if current_line {
            self.current_line_range().start..self.insertion_point()
        } else {
            0..self.insertion_point()
        };
        self.lines[range.clone()].rfind(c).map(|i| i + range.start)
    }

    /// Moves the insertion point until the next char to the right
    pub fn move_right_until(&mut self, c: char, current_line: bool) -> usize {
        if let Some(index) = self.find_char_right(c, current_line) {
            self.insertion_point = index;
        }

        self.insertion_point
    }

    /// Moves the insertion point before the next char to the right
    pub fn move_right_before(&mut self, c: char, current_line: bool) -> usize {
        if let Some(index) = self.find_char_right(c, current_line) {
            self.insertion_point = index;
            self.insertion_point = self.grapheme_left_index();
        }

        self.insertion_point
    }

    /// Moves the insertion point until the next char to the left of offset
    pub fn move_left_until(&mut self, c: char, current_line: bool) -> usize {
        if let Some(index) = self.find_char_left(c, current_line) {
            self.insertion_point = index;
        }

        self.insertion_point
    }

    /// Moves the insertion point before the next char to the left of offset
    pub fn move_left_before(&mut self, c: char, current_line: bool) -> usize {
        if let Some(index) = self.find_char_left(c, current_line) {
            self.insertion_point = index + c.len_utf8();
        }

        self.insertion_point
    }

    /// Deletes until first character to the right of offset
    pub fn delete_right_until_char(&mut self, c: char, current_line: bool) {
        if let Some(index) = self.find_char_right(c, current_line) {
            self.clear_range(self.insertion_point()..index + c.len_utf8());
        }
    }

    /// Deletes before first character to the right of offset
    pub fn delete_right_before_char(&mut self, c: char, current_line: bool) {
        if let Some(index) = self.find_char_right(c, current_line) {
            self.clear_range(self.insertion_point()..index);
        }
    }

    /// Deletes until first character to the left of offset
    pub fn delete_left_until_char(&mut self, c: char, current_line: bool) {
        if let Some(index) = self.find_char_left(c, current_line) {
            self.clear_range(index..self.insertion_point());
            self.insertion_point = index;
        }
    }

    /// Deletes before first character to the left of offset
    pub fn delete_left_before_char(&mut self, c: char, current_line: bool) {
        if let Some(index) = self.find_char_left(c, current_line) {
            self.clear_range(index + c.len_utf8()..self.insertion_point());
            self.insertion_point = index + c.len_utf8();
        }
    }

    /// Attempts to find the matching `(left_char, right_char)` pair *enclosing*
    /// the cursor position, respecting nested pairs.
    ///
    /// Algorithm:
    /// 1. Walk left from `cursor` until we find the "outermost" `left_char`,
    ///    ignoring any extra `right_char` we see (i.e., we keep a depth counter).
    /// 2. Then from that left bracket, walk right to find the matching `right_char`,
    ///    also respecting nesting.
    ///
    /// Returns `Some((left_index, right_index))` if found, or `None` otherwise.
    pub fn find_matching_pair(
        &self,
        left_char: char,
        right_char: char,
        cursor: usize,
    ) -> Option<(usize, usize)> {
        // encode to &str so we can compare with &strs later
        let mut tmp = ([0u8; 4], [0u8, 4]);
        let left_str = left_char.encode_utf8(&mut tmp.0);
        let right_str = right_char.encode_utf8(&mut tmp.1);
        // search left for left char
        let to_cursor = self.lines.get(..=cursor)?;
        let left_index = find_with_depth(to_cursor, left_str, right_str, true)?;

        // search right for right char
        let scan_start = left_index + left_char.len_utf8();
        let after_left = self.lines.get(scan_start..)?;
        let right_offset = find_with_depth(after_left, right_str, left_str, false)?;

        Some((left_index, scan_start + right_offset))
    }
}

/// Helper function for [`LineBuffer::find_matching_pair`]
fn find_with_depth(
    slice: &str,
    deep_char: &str,
    shallow_char: &str,
    reverse: bool,
) -> Option<usize> {
    let mut depth: i32 = 0;

    let mut indices: Vec<_> = slice.grapheme_indices(true).collect();
    if reverse {
        indices.reverse();
    }

    for (idx, c) in indices.into_iter() {
        match c {
            c if c == deep_char && depth == 0 => return Some(idx),
            c if c == deep_char => depth -= 1,
            // special case: shallow char at end of slice shouldn't affect depth.
            // cursor over right bracket should be counted as the end of the pair,
            // not as a closing a separate nested pair
            c if c == shallow_char && idx == (slice.len() - 1) => (),
            c if c == shallow_char => depth += 1,
            _ => (),
        }
    }

    None
}

/// Match any sequence of characters that are considered a word boundary
fn is_whitespace_str(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
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
        line_buffer.assert_valid();
    }

    #[test]
    fn test_clearing_line_buffer_resets_buffer_and_insertion_point() {
        let mut line_buffer = buffer_with("this is a command");
        line_buffer.clear();
        let empty_buffer = LineBuffer::new();

        assert_eq!(line_buffer, empty_buffer);
        line_buffer.assert_valid();
    }

    #[test]
    fn insert_str_updates_insertion_point_point_correctly() {
        let mut line_buffer = LineBuffer::new();
        line_buffer.insert_str("this is a command");

        let expected_updated_insertion_point = 17;

        assert_eq!(
            expected_updated_insertion_point,
            line_buffer.insertion_point()
        );
        line_buffer.assert_valid();
    }

    #[test]
    fn insert_char_updates_insertion_point_point_correctly() {
        let mut line_buffer = LineBuffer::new();
        line_buffer.insert_char('c');

        let expected_updated_insertion_point = 1;

        assert_eq!(
            expected_updated_insertion_point,
            line_buffer.insertion_point()
        );
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("new string", 10)]
    #[case("new line1\nnew line 2", 20)]
    fn set_buffer_updates_insertion_point_to_new_buffer_length(
        #[case] string_to_set: &str,
        #[case] expected_insertion_point: usize,
    ) {
        let mut line_buffer = buffer_with("test string");
        let before_operation_location = 11;
        assert_eq!(before_operation_location, line_buffer.insertion_point());

        line_buffer.set_buffer(string_to_set.to_string());

        assert_eq!(expected_insertion_point, line_buffer.insertion_point());
        line_buffer.assert_valid();
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
        line_buffer.assert_valid();
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
        line_buffer.assert_valid();
    }

    #[test]
    fn delete_word_left_works() {
        let mut line_buffer = buffer_with("This is a test");
        line_buffer.delete_word_left();

        let expected_line_buffer = buffer_with("This is a ");

        assert_eq!(expected_line_buffer, line_buffer);
        line_buffer.assert_valid();
    }

    #[test]
    fn delete_word_right_works() {
        let mut line_buffer = buffer_with("This is a test");
        line_buffer.move_word_left();
        line_buffer.delete_word_right();

        let expected_line_buffer = buffer_with("This is a ");

        assert_eq!(expected_line_buffer, line_buffer);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("", 0, 0)] // Basecase
    #[case("word", 0, 3)] // Cursor on top of the last grapheme of the word
    #[case("word and another one", 0, 3)]
    #[case("word and another one", 3, 7)] // repeat calling will move
    #[case("word and another one", 4, 7)] // Starting from whitespace works
    #[case("word\nline two", 0, 3)] // Multiline...
    #[case("word\nline two", 3, 8)] // ... continues to next word end
    #[case("weirdö characters", 0, 5)] // Multibyte unicode at the word end (latin UTF-8 should be two bytes long)
    #[case("weirdö characters", 5, 17)] // continue with unicode (latin UTF-8 should be two bytes long)
    #[case("weirdö", 0, 5)] // Multibyte unicode at the buffer end is fine as well
    #[case("weirdö", 5, 5)] // Multibyte unicode at the buffer end is fine as well
    #[case("word😇 with emoji", 0, 3)] // (Emojis are a separate word)
    #[case("word😇 with emoji", 3, 4)] // Moves to end of "emoji word" as it is one grapheme, on top of the first byte
    #[case("😇", 0, 0)] // More UTF-8 shenanigans
    fn test_move_word_right_end(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] expected: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        line_buffer.move_word_right_end();

        assert_eq!(line_buffer.insertion_point(), expected);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("This is a test", 13, "This is a tesT", 14)]
    #[case("This is a test", 10, "This is a Test", 11)]
    #[case("This is a test", 9, "This is a Test", 11)]
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
        line_buffer.assert_valid();
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
        line_buffer.assert_valid();
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
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("", 0, "", 0)]
    #[case("a test", 2, "a Test", 3)]
    #[case("a Test", 2, "a test", 3)]
    #[case("test", 0, "Test", 1)]
    #[case("Test", 0, "test", 1)]
    #[case("test", 3, "tesT", 4)]
    #[case("tesT", 3, "test", 4)]
    #[case("ß", 0, "ß", 2)]
    fn switchcase_char(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] output: &str,
        #[case] out_location: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);
        line_buffer.switchcase_char();

        let mut expected = buffer_with(output);
        expected.set_insertion_point(out_location);

        assert_eq!(expected, line_buffer);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("This is a test", 13, "This is a tets", 14)]
    #[case("This is a test", 14, "This is a tets", 14)] // NOTE: Swapping works in opposite direction at last index
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
        line_buffer.assert_valid();
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
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("line 1\nline 2", 7, 0)]
    #[case("line 1\nline 2", 8, 1)]
    #[case("line 1\nline 2", 0, 0)]
    #[case("line\nlong line", 14, 4)]
    #[case("line\nlong line", 8, 3)]
    #[case("line 1\n😇line 2", 11, 1)]
    #[case("line\n\nline", 8, 5)]
    fn moving_up_works(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] out_location: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        line_buffer.move_line_up();

        let mut expected = buffer_with(input);
        expected.set_insertion_point(out_location);

        assert_eq!(line_buffer, expected);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("line 1", 0, 0)]
    #[case("line 1\nline 2", 0, 7)]
    #[case("line 1\n😇line 2", 1, 11)]
    #[case("line 😇 1\nline 2 long", 9, 18)]
    #[case("line 1\nline 2", 7, 7)]
    #[case("long line\nline", 8, 14)]
    #[case("long line\nline", 4, 14)]
    #[case("long line\nline", 3, 13)]
    #[case("long line\nline\nline", 8, 14)]
    #[case("line\n\nline", 3, 5)]
    fn moving_down_works(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] out_location: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        line_buffer.move_line_down();

        let mut expected = buffer_with(input);
        expected.set_insertion_point(out_location);

        assert_eq!(line_buffer, expected);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("line", 4, true)]
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
        line_buffer.assert_valid();

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
        line_buffer.assert_valid();

        assert_eq!(line_buffer.is_cursor_at_last_line(), expected);
    }

    #[rstest]
    #[case("abc def ghi", 0, 'c', true, 2)]
    #[case("abc def ghi", 0, 'a', true, 0)]
    #[case("abc def ghi", 0, 'z', true, 0)]
    #[case("a😇c", 0, 'c', true, 5)]
    #[case("😇bc", 0, 'c', true, 5)]
    #[case("abc\ndef", 0, 'f', true, 0)]
    #[case("abc\ndef", 3, 'f', true, 3)]
    #[case("abc\ndef", 0, 'f', false, 6)]
    #[case("abc\ndef", 3, 'f', false, 6)]
    fn test_move_right_until(
        #[case] input: &str,
        #[case] position: usize,
        #[case] c: char,
        #[case] current_line: bool,
        #[case] expected: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        line_buffer.move_right_until(c, current_line);

        assert_eq!(line_buffer.insertion_point(), expected);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("abc def ghi", 0, 'd', true, 3)]
    #[case("abc def ghi", 3, 'd', true, 3)]
    #[case("a😇c", 0, 'c', true, 1)]
    #[case("😇bc", 0, 'c', true, 4)]
    fn test_move_right_before(
        #[case] input: &str,
        #[case] position: usize,
        #[case] c: char,
        #[case] current_line: bool,
        #[case] expected: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        line_buffer.move_right_before(c, current_line);

        assert_eq!(line_buffer.insertion_point(), expected);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("abc def ghi", 0, 'd', true, "ef ghi")]
    #[case("abc def ghi", 0, 'i', true, "")]
    #[case("abc def ghi", 0, 'z', true, "abc def ghi")]
    #[case("abc def ghi", 0, 'a', true, "abc def ghi")]
    fn test_delete_until(
        #[case] input: &str,
        #[case] position: usize,
        #[case] c: char,
        #[case] current_line: bool,
        #[case] expected: &str,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        line_buffer.delete_right_until_char(c, current_line);

        assert_eq!(line_buffer.lines, expected);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("abc def ghi", 0, 'b', true, "bc def ghi")]
    #[case("abc def ghi", 0, 'i', true, "i")]
    #[case("abc def ghi", 0, 'z', true, "abc def ghi")]
    fn test_delete_before(
        #[case] input: &str,
        #[case] position: usize,
        #[case] c: char,
        #[case] current_line: bool,
        #[case] expected: &str,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        line_buffer.delete_right_before_char(c, current_line);

        assert_eq!(line_buffer.lines, expected);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("abc def ghi", 4, 'c', true, 2)]
    #[case("abc def ghi", 0, 'a', true, 0)]
    #[case("abc def ghi", 6, 'a', true, 0)]
    fn test_move_left_until(
        #[case] input: &str,
        #[case] position: usize,
        #[case] c: char,
        #[case] current_line: bool,
        #[case] expected: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        line_buffer.move_left_until(c, current_line);

        assert_eq!(line_buffer.insertion_point(), expected);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("abc def ghi", 4, 'c', true, 3)]
    #[case("abc def ghi", 0, 'a', true, 0)]
    #[case("abc def ghi", 6, 'a', true, 1)]
    fn test_move_left_before(
        #[case] input: &str,
        #[case] position: usize,
        #[case] c: char,
        #[case] current_line: bool,
        #[case] expected: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        line_buffer.move_left_before(c, current_line);

        assert_eq!(line_buffer.insertion_point(), expected);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("abc def ghi", 5, 'b', true, "aef ghi")]
    #[case("abc def ghi", 5, 'e', true, "abc def ghi")]
    #[case("abc def ghi", 10, 'a', true, "i")]
    #[case("z\nabc def ghi", 10, 'z', true, "z\nabc def ghi")]
    #[case("z\nabc def ghi", 12, 'z', false, "i")]
    fn test_delete_until_left(
        #[case] input: &str,
        #[case] position: usize,
        #[case] c: char,
        #[case] current_line: bool,
        #[case] expected: &str,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        line_buffer.delete_left_until_char(c, current_line);

        assert_eq!(line_buffer.lines, expected);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("abc def ghi", 5, 'b', true, "abef ghi")]
    #[case("abc def ghi", 5, 'e', true, "abc def ghi")]
    #[case("abc def ghi", 10, 'a', true, "ai")]
    fn test_delete_before_left(
        #[case] input: &str,
        #[case] position: usize,
        #[case] c: char,
        #[case] current_line: bool,
        #[case] expected: &str,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        line_buffer.delete_left_before_char(c, current_line);

        assert_eq!(line_buffer.lines, expected);
        line_buffer.assert_valid();
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
        line_buffer.assert_valid();

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
        line_buffer.assert_valid();

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
        line_buffer.assert_valid();

        assert_eq!(line_buffer.num_lines(), expected);
    }

    #[rstest]
    #[case("", 0, 0)]
    #[case("line", 0, 4)]
    #[case("\n", 0, 0)]
    #[case("line\n", 0, 4)]
    #[case("a\nb", 2, 3)]
    #[case("a\nb", 0, 1)]
    #[case("a\r\nb", 0, 1)]
    fn test_move_to_line_end(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] expected: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        line_buffer.move_to_line_end();

        assert_eq!(line_buffer.insertion_point(), expected);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("", 0, 0)]
    #[case("line", 3, 0)]
    #[case("\n", 1, 1)]
    #[case("\n", 0, 0)]
    #[case("\nline", 3, 1)]
    #[case("a\nb", 2, 2)]
    #[case("a\nb", 3, 2)]
    #[case("a\r\nb", 3, 3)]
    fn test_move_to_line_start(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] expected: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        line_buffer.move_to_line_start();

        assert_eq!(line_buffer.insertion_point(), expected);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("", 0, 0..0)]
    #[case("line", 0, 0..4)]
    #[case("line\n", 0, 0..5)]
    #[case("line\n", 4, 0..5)]
    #[case("line\r\n", 0, 0..6)]
    #[case("line\r\n", 4, 0..6)] // Position 5 would be invalid from a grapheme perspective
    #[case("line\nsecond", 5, 5..11)]
    #[case("line\r\nsecond", 7, 6..12)]
    fn test_current_line_range(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] expected: Range<usize>,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);
        line_buffer.assert_valid();

        assert_eq!(line_buffer.current_line_range(), expected);
    }

    #[rstest]
    #[case("This is a test", 7, "This is", 7)]
    #[case("This is a test\nunrelated", 7, "This is\nunrelated", 7)]
    #[case("This is a test\r\nunrelated", 7, "This is\r\nunrelated", 7)]
    fn test_clear_to_line_end(
        #[case] input: &str,
        #[case] in_location: usize,
        #[case] output: &str,
        #[case] out_location: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(in_location);

        line_buffer.clear_to_line_end();

        let mut expected = buffer_with(output);
        expected.set_insertion_point(out_location);

        assert_eq!(expected, line_buffer);
        line_buffer.assert_valid();
    }

    #[rstest]
    #[case("abc def ghi", 10, 8)]
    #[case("abc def-ghi", 10, 8)]
    #[case("abc def.ghi", 10, 4)]
    fn test_word_left_index(#[case] input: &str, #[case] position: usize, #[case] expected: usize) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        let index = line_buffer.word_left_index();

        assert_eq!(index, expected);
    }

    #[rstest]
    #[case("abc def ghi", 10, 8)]
    #[case("abc def-ghi", 10, 4)]
    #[case("abc def.ghi", 10, 4)]
    #[case("abc def   i", 10, 4)]
    fn test_big_word_left_index(
        #[case] input: &str,
        #[case] position: usize,
        #[case] expected: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        let index = line_buffer.big_word_left_index();

        assert_eq!(index, expected,);
    }

    #[rstest]
    #[case("abc def ghi", 0, 4)]
    #[case("abc-def ghi", 0, 3)]
    #[case("abc.def ghi", 0, 8)]
    fn test_word_right_start_index(
        #[case] input: &str,
        #[case] position: usize,
        #[case] expected: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        let index = line_buffer.word_right_start_index();

        assert_eq!(index, expected);
    }

    #[rstest]
    #[case("abc def ghi", 0, 4)]
    #[case("abc-def ghi", 0, 8)]
    #[case("abc.def ghi", 0, 8)]
    fn test_big_word_right_start_index(
        #[case] input: &str,
        #[case] position: usize,
        #[case] expected: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        let index = line_buffer.big_word_right_start_index();

        assert_eq!(index, expected);
    }

    #[rstest]
    #[case("abc def ghi", 0, 2)]
    #[case("abc-def ghi", 0, 2)]
    #[case("abc.def ghi", 0, 6)]
    #[case("abc", 1, 2)]
    #[case("abc", 2, 2)]
    #[case("abc def", 2, 6)]
    fn test_word_right_end_index(
        #[case] input: &str,
        #[case] position: usize,
        #[case] expected: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        let index = line_buffer.word_right_end_index();

        assert_eq!(index, expected);
    }

    #[rstest]
    #[case("abc def ghi", 0, 2)]
    #[case("abc-def ghi", 0, 6)]
    #[case("abc-def ghi", 5, 6)]
    #[case("abc-def ghi", 6, 10)]
    #[case("abc.def ghi", 0, 6)]
    #[case("abc", 1, 2)]
    #[case("abc", 2, 2)]
    #[case("abc def", 2, 6)]
    #[case("abc-def", 6, 6)]
    fn test_big_word_right_end_index(
        #[case] input: &str,
        #[case] position: usize,
        #[case] expected: usize,
    ) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        let index = line_buffer.big_word_right_end_index();

        assert_eq!(index, expected);
    }

    #[rstest]
    #[case("abc def", 0, 3)]
    #[case("abc def ghi", 3, 7)]
    #[case("abc", 1, 3)]
    fn test_next_whitespace(#[case] input: &str, #[case] position: usize, #[case] expected: usize) {
        let mut line_buffer = buffer_with(input);
        line_buffer.set_insertion_point(position);

        let index = line_buffer.next_whitespace();

        assert_eq!(index, expected);
    }

    #[rstest]
    #[case("abc", 0, 1)] // Basic ASCII
    #[case("abc", 1, 2)] // From middle position
    #[case("abc", 2, 3)] // From last char
    #[case("abc", 3, 3)] // From end of string
    #[case("🦀rust", 0, 4)] // Unicode emoji
    #[case("🦀rust", 4, 5)] // After emoji
    #[case("é́", 0, 4)] // Combining characters
    fn test_grapheme_right_index_from_pos(
        #[case] input: &str,
        #[case] position: usize,
        #[case] expected: usize,
    ) {
        let mut line = LineBuffer::new();
        line.insert_str(input);
        assert_eq!(
            line.grapheme_right_index_from_pos(position),
            expected,
            "input: {input:?}, pos: {position}"
        );
    }

    #[rstest]
    #[case("(abc)", 0, '(', ')', Some((0, 4)))] // Basic matching
    #[case("(abc)", 4, '(', ')', Some((0, 4)))] // Cursor at end
    #[case("(abc)", 2, '(', ')', Some((0, 4)))] // Cursor in middle
    #[case("((abc))", 0, '(', ')', Some((0, 6)))] // Nested pairs outer
    #[case("((abc))", 1, '(', ')', Some((1, 5)))] // Nested pairs inner
    #[case("(abc)(def)", 0, '(', ')', Some((0, 4)))] // Multiple pairs first
    #[case("(abc)(def)", 5, '(', ')', Some((5, 9)))] // Multiple pairs second
    #[case("(abc", 0, '(', ')', None)] // Incomplete open
    #[case("abc)", 3, '(', ')', None)] // Incomplete close
    #[case("()", 0, '(', ')', Some((0, 1)))] // Empty pair
    #[case("()", 1, '(', ')', Some((0, 1)))] // Empty pair from end
    #[case("(αβγ)", 0, '(', ')', Some((0, 7)))] // Unicode content
    #[case("([)]", 0, '(', ')', Some((0, 2)))] // Mixed brackets
    #[case("\"abc\"", 0, '"', '"', Some((0, 4)))] // Quotes
    fn test_find_matching_pair(
        #[case] input: &str,
        #[case] cursor: usize,
        #[case] left_char: char,
        #[case] right_char: char,
        #[case] expected: Option<(usize, usize)>,
    ) {
        let buf = LineBuffer::from(input);
        assert_eq!(
            buf.find_matching_pair(left_char, right_char, cursor),
            expected,
            "Failed for input: {}, cursor: {}",
            input,
            cursor
        );
    }
}
