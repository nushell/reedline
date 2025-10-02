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

    /// Move the cursor before the first non whitespace character of the line
    pub fn move_to_line_non_blank_start(&mut self) {
        let line_start = self.lines[..self.insertion_point]
            .rfind('\n')
            .map_or(0, |offset| offset + 1);
        // str is guaranteed to be utf8, thus \n is safe to assume 1 byte long

        self.insertion_point = self.lines[line_start..]
            .find(|c: char| !c.is_whitespace() || c == '\n')
            .map(|offset| line_start + offset)
            .unwrap_or(self.lines.len());
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
        self.grapheme_right_index_from_pos(self.insertion_point)
    }

    /// Cursor position *in front of* the next unicode grapheme to the left
    pub fn grapheme_left_index(&self) -> usize {
        self.grapheme_left_index_from_pos(self.insertion_point)
    }

    /// Cursor position *behind* the next unicode grapheme to the right from the given position
    pub fn grapheme_right_index_from_pos(&self, pos: usize) -> usize {
        self.lines[pos..]
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| pos + i)
            .unwrap_or_else(|| self.lines.len())
    }

    /// Cursor position *behind* the previous unicode grapheme to the left from the given position
    pub(crate) fn grapheme_left_index_from_pos(&self, pos: usize) -> usize {
        self.lines[..pos]
            .grapheme_indices(true)
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
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
                    .next_back()
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
                    .next_back()
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
            .next_back()
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

    /// Returns true if cursor is at the end of the buffer with preceding whitespace.
    fn at_end_of_line_with_preceding_whitespace(&self) -> bool {
        !self.is_empty() // No point checking if empty
        && self.insertion_point == self.lines.len()
        && self.lines.chars().last().map_or(false, |c| c.is_whitespace())
    }

    /// Cursor position at the end of the current whitespace block.
    fn current_whitespace_end_index(&self) -> usize {
        self.lines[self.insertion_point..]
            .char_indices()
            .find(|(_, ch)| !ch.is_whitespace())
            .map(|(i, _)| self.insertion_point + i)
            .unwrap_or(self.lines.len())
    }

    /// Cursor position at the start of the current whitespace block.
    fn current_whitespace_start_index(&self) -> usize {
        self.lines[..self.insertion_point]
            .char_indices()
            .rev()
            .find(|(_, ch)| !ch.is_whitespace())
            .map(|(i, _)| i + 1)
            .unwrap_or(0)
    }

    /// Returns the range of consecutive whitespace characters that includes
    /// the cursor position. If cursor is at the end of trailing whitespace, includes
    /// that trailing block. Return None if no surrounding whitespace.
    pub(crate) fn current_whitespace_range(&self) -> Option<Range<usize>> {
        let range_start = self.current_whitespace_start_index();
        if self.on_whitespace() {
            let range_end = self.current_whitespace_end_index();
            Some(range_start..range_end)
        } else if self.at_end_of_line_with_preceding_whitespace() {
            Some(range_start..self.insertion_point)
        } else {
            None
        }
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
    pub fn clear_range_safe(&mut self, range: Range<usize>) {
        let (start, end) = if range.start > range.end {
            (range.end, range.start)
        } else {
            (range.start, range.end)
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
            .next_back()
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

    /// Returns `Some(Range<usize>)` for the range inside the surrounding
    /// `open_char` and `close_char`, or `None` if no pair is found.
    ///
    /// If cursor is positioned just before an opening character, treat it as
    /// being "inside" that pair.
    ///
    /// For symmetric characters (e.g. quotes), the search is restricted to the current line only.
    /// For asymmetric characters (e.g. brackets), the search spans the entire buffer.
    pub(crate) fn range_inside_current_pair(
        &self,
        open_char: char,
        close_char: char,
    ) -> Option<Range<usize>> {
        let only_search_current_line: bool = open_char == close_char;
        let find_range_between_pair_at_position = |pos| {
            self.range_between_matching_pair_at_pos(
                pos,
                only_search_current_line,
                open_char,
                close_char,
            )
        };

        // First try to find pair from current cursor position
        find_range_between_pair_at_position(self.insertion_point).or_else(|| {
            // Second try, if cursor is positioned just before an opening character,
            // treat it as being "inside" that pair and try from the next position
            self.grapheme_right()
                .starts_with(open_char)
                .then(|| find_range_between_pair_at_position(self.grapheme_right_index()))
                .flatten()
        })
    }

    /// Returns `Some(Range<usize>)` for the range inside the next pair
    /// or `None` if no pair is found
    ///
    /// Search forward from the cursor to find the next occurrence of `open_char`
    /// (including char at cursors current position), then finds its matching
    /// `close_char` and returns the range of text inside those characters.
    /// Note the end of Range is exclusive so the end of the range returned so
    /// the end of the range is index of final char + 1.
    ///
    /// For symmetric characters (e.g. quotes), the search is restricted to the current line only.
    /// For asymmetric characters (e.g. brackets), the search spans the entire buffer.
    pub(crate) fn range_inside_next_pair(
        &self,
        open_char: char,
        close_char: char,
    ) -> Option<Range<usize>> {
        let only_search_current_line: bool = open_char == close_char;

        // Find the next opening character, including the current position
        let open_pair_index = if self.grapheme_right().starts_with(open_char) {
            self.insertion_point
        } else {
            self.find_char_right(open_char, only_search_current_line)?
        };

        self.range_between_matching_pair_at_pos(
            self.grapheme_right_index_from_pos(open_pair_index),
            only_search_current_line,
            open_char,
            close_char,
        )
    }

    /// Returns `Some(Range<usize>)` for the range inside the pair `open_char`
    /// and `close_char` surrounding the cursor position NOT including the character
    /// at the current cursor position, or `None` if no valid pair is found.
    ///
    /// This is the underlying algorithm used by both `range_inside_current_pair` and
    /// `range_inside_next_pair`.
    /// It uses a forward-first search approach:
    /// 1. Search forward from cursor to find the closing character (ignoring nested pairs)
    /// 2. Search backward from closing to find the matching opening character
    /// 3. Return the range between them
    fn range_between_matching_pair_at_pos(
        &self,
        position: usize,
        only_search_current_line: bool,
        open_char: char,
        close_char: char,
    ) -> Option<Range<usize>> {
        let search_range = if only_search_current_line {
            self.current_line_range()
        } else {
            0..self.lines.len()
        };

        let after_cursor = &self.lines[position..search_range.end];
        let close_pair_index_after_cursor =
            Self::find_index_of_matching_pair(after_cursor, open_char, close_char, false)?;
        let close_char_index_in_buffer = position + close_pair_index_after_cursor;

        let start_to_close_char = &self.lines[search_range.start..close_char_index_in_buffer];

        Self::find_index_of_matching_pair(start_to_close_char, open_char, close_char, true).map(
            |open_char_index_from_start| {
                let open_char_index_in_buffer = search_range.start + open_char_index_from_start;
                (open_char_index_in_buffer + open_char.len_utf8())..close_char_index_in_buffer
            },
        )
    }

    /// Find the index of a pair character that matches the nesting depth at the
    /// start or end of `slice` using depth counting to handle nested pairs.
    /// Helper for [`LineBuffer::range_between_matching_pair_at_pos`]
    ///
    /// If `search_backwards` is false:
    /// Find close_char at same level of nesting as start of slice.
    ///
    /// If `search_backwards` is true:
    /// Find open_char at same level of nesting as end of slice.
    ///
    /// Returns index of the open or closing character that matches the start of slice,
    /// or `None` if not found.
    fn find_index_of_matching_pair(
        slice: &str,
        open_char: char,
        close_char: char,
        search_backwards: bool,
    ) -> Option<usize> {
        let mut depth = 0;
        let mut graphemes: Vec<(usize, &str)> = slice.grapheme_indices(true).collect();

        if search_backwards {
            graphemes.reverse();
        }

        let (target, increment) = if search_backwards {
            (open_char, close_char)
        } else {
            (close_char, open_char)
        };

        for (index, grapheme) in graphemes {
            if let Some(char) = grapheme.chars().next() {
                if char == target {
                    if depth == 0 {
                        return Some(index);
                    }
                    depth -= 1;
                } else if char == increment && index > 0 {
                    depth += 1;
                }
            }
        }
        None
    }

    /// Returns `Some(Range<usize>)` for the range inside pair in `pair_group`
    /// at cursor position including pair of character at current cursor position,
    /// or `None` if cursor is not inside or at a pair included in `pair_group.
    ///
    /// If the opening and closing char in the pair are equal then search is
    /// restricted to the current line.
    ///
    /// If multiple pair types are found in the buffer or line, return the innermost
    /// pair that surrounds the cursor. Handles empty quotes as zero-length ranges inside quote.
    pub(crate) fn range_inside_current_pair_in_group(
        &self,
        matching_pair_group: &[(char, char)],
    ) -> Option<Range<usize>> {
        matching_pair_group
            .iter()
            .filter_map(|(open_char, close_char)| {
                self.range_inside_current_pair(*open_char, *close_char)
            })
            .min_by_key(|range| range.len())
    }

    /// Returns `Some(Range<usize>)` for the range inside the next pair in `pair_group`
    /// or `None` if cursor is not inside a pair included in `pair_group`.
    ///
    /// If the opening and closing char in the pair are equal then search is
    /// restricted to the current line.
    ///
    /// If multiple pair types are found in the buffer or line, return the innermost
    /// pair that surrounds the cursor. Handles empty pairs as zero-length ranges
    /// inside pair (this enables caller to still get the location of the pair).
    pub(crate) fn range_inside_next_pair_in_group(
        &self,
        matching_pair_group: &[(char, char)],
    ) -> Option<Range<usize>> {
        matching_pair_group
            .iter()
            .filter_map(|(open_char, close_char)| {
                self.range_inside_next_pair(*open_char, *close_char)
            })
            .min_by_key(|range| range.start)
    }

    /// Get the range of the current big word (WORD) at cursor position
    pub(crate) fn current_big_word_range(&self) -> Range<usize> {
        let right_index = self.big_word_right_end_index();

        let mut left_index = 0;
        for (i, char) in self.lines[..right_index].char_indices().rev() {
            if char.is_whitespace() {
                left_index = i + char.len_utf8();
                break;
            }
        }
        left_index..(right_index + 1)
    }

    /// Return range of `range` expanded with neighbouring whitespace for "around" operations
    /// Prioritizes whitespace after the word, falls back to whitespace before if none after
    pub(crate) fn expand_range_with_whitespace(&self, range: Range<usize>) -> Range<usize> {
        let end = self.next_non_whitespace_index(range.end);
        let start = if end == range.end {
            self.prev_non_whitespace_index(range.start)
        } else {
            range.start
        };
        start..end
    }

    /// Return next non-whitespace character index after `pos`
    fn next_non_whitespace_index(&self, pos: usize) -> usize {
        self.lines[pos..]
            .char_indices()
            .find(|(_, char)| !char.is_whitespace())
            .map_or(self.lines.len(), |(i, _)| pos + i)
    }

    /// Extend range leftward to include leading whitespace
    fn prev_non_whitespace_index(&self, pos: usize) -> usize {
        self.lines[..pos]
            .char_indices()
            .rev()
            .find(|(_, char)| !char.is_whitespace())
            .map_or(0, |(i, char)| i + char.len_utf8())
    }
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

    const BRACKET_PAIRS: &[(char, char); 3] = &[('(', ')'), ('[', ']'), ('{', '}')];
    const QUOTE_PAIRS: &[(char, char); 3] = &[('"', '"'), ('\'', '\''), ('`', '`')];
    // Tests for range_inside_current_quote - cursor inside or on the boundary
    #[rstest]
    #[case("foo(bar)baz", 5, BRACKET_PAIRS, Some(4..7))] // cursor on 'a' in "bar"
    #[case("foo[bar]baz", 5, BRACKET_PAIRS, Some(4..7))] // square brackets
    #[case("foo{bar}baz", 5, BRACKET_PAIRS, Some(4..7))] // curly brackets
    #[case("foo(bar(baz)qux)end", 9, BRACKET_PAIRS, Some(8..11))] // cursor on 'a' in "baz", finds inner
    #[case("foo(bar(baz)qux)end", 5, BRACKET_PAIRS, Some(4..15))] // cursor on 'a' in "bar", finds outer
    #[case("foo([bar])baz", 6, BRACKET_PAIRS, Some(5..8))] // mixed bracket types, cursor on 'a' - should find [bar], not (...)
    #[case("foo[(bar)]baz", 6, BRACKET_PAIRS, Some(5..8))] // reversed nesting, cursor on 'a' - should find (bar), not [...]
    #[case("foo(bar)baz", 4, BRACKET_PAIRS, Some(4..7))] // cursor just after opening bracket
    #[case("foo(bar)baz", 7, BRACKET_PAIRS, Some(4..7))] // cursor just before closing bracket
    #[case("foo[]bar", 4, BRACKET_PAIRS, Some(4..4))] // empty square brackets
    #[case("(content)", 0, BRACKET_PAIRS, Some(1..8))] // brackets at buffer start/end
    #[case("a(b)c", 2, BRACKET_PAIRS, Some(2..3))] // minimal case - cursor inside brackets
    #[case(r#"foo("bar")baz"#, 6, BRACKET_PAIRS, Some(4..9))] // quotes inside brackets
    #[case(r#"foo"(bar)"baz"#, 6, BRACKET_PAIRS, Some(5..8))] // brackets inside quotes
    #[case("())", 1, BRACKET_PAIRS, Some(1..1))] // extra closing bracket
    #[case("", 0, BRACKET_PAIRS, None)] // empty buffer
    #[case("(", 0, BRACKET_PAIRS, None)] // single opening bracket
    #[case(")", 0, BRACKET_PAIRS, None)] // single closing bracket
    #[case("", 0, BRACKET_PAIRS, None)] // empty buffer
    #[case(r#"foo"bar"baz"#, 5, QUOTE_PAIRS, Some(4..7))] // cursor on 'a' in "bar"
    #[case("foo'bar'baz", 5, QUOTE_PAIRS, Some(4..7))] // single quotes
    #[case("foo`bar`baz", 5, QUOTE_PAIRS, Some(4..7))] // backticks
    #[case(r#"'foo"baz`bar`taz"baz'"#, 0, QUOTE_PAIRS, Some(1..20))] // backticks
    #[case(r#""foo"'bar'`baz`"#, 0, QUOTE_PAIRS, Some(1..4))] // cursor at start, should find first (double)
    #[case("no quotes here", 5, QUOTE_PAIRS, None)] // no quotes in buffer
    #[case(r#"unclosed "quotes"#, 10, QUOTE_PAIRS, None)] // unmatched quotes
    #[case("", 0, QUOTE_PAIRS, None)] // empty buffer
    fn test_range_inside_current_pair_group(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] pairs: &[(char, char); 3],
        #[case] expected: Option<Range<usize>>,
    ) {
        let mut buf = LineBuffer::from(input);
        buf.set_insertion_point(cursor_pos);
        assert_eq!(buf.range_inside_current_pair_in_group(pairs), expected);
    }

    // Tests for range_inside_next_pair_in_group - cursor before pairs, return range inside next pair if exists
    #[rstest]
    #[case("foo (bar)baz", 1, BRACKET_PAIRS, Some(5..8))] // cursor before brackets
    #[case("foo []bar", 1, BRACKET_PAIRS, Some(5..5))] // cursor before empty brackets
    #[case("(first)(second)", 4, BRACKET_PAIRS, Some(8..14))] // inside first, should find second
    #[case("foo{bar[baz]qux}end", 0, BRACKET_PAIRS, Some(4..15))] // cursor at start, finds outermost
    #[case("foo{bar[baz]qux}end", 1, BRACKET_PAIRS, Some(4..15))] // cursor before nested, finds innermost
    #[case("foo{bar[baz]qux}end", 4, BRACKET_PAIRS, Some(8..11))] // cursor before nested, finds innermost
    #[case("(){}[]", 0, BRACKET_PAIRS, Some(1..1))] // cursor at start, finds first empty pair
    #[case("(){}[]", 2, BRACKET_PAIRS, Some(3..3))] // cursor between pairs, finds next
    #[case("no brackets here", 5, BRACKET_PAIRS, None)] // no brackets found
    #[case("", 0, BRACKET_PAIRS, None)] // empty buffer
    #[case(r#"foo "'bar'" baz"#, 1, QUOTE_PAIRS, Some(5..10))] // cursor before nested quotes
    #[case(r#"foo '' "bar" baz"#, 1, QUOTE_PAIRS, Some(5..5))] // cursor before first quotes
    #[case(r#""foo"'bar`b'az`"#, 1, QUOTE_PAIRS, Some(6..11))] // cursor inside first quotes, find single quotes
    #[case(r#""foo"'bar'`baz`"#, 6, QUOTE_PAIRS, Some(11..14))] // cursor after second quotes, find backticks
    #[case(r#"zaz'foo"b`a`r"baz'zaz"#, 3, QUOTE_PAIRS, Some(4..17))] // range inside outermost nested quotes
    #[case(r#""""#, 0, QUOTE_PAIRS, Some(1..1))] // single quote pair (empty) - should find it ahead
    #[case(r#"""asdf"#, 0, QUOTE_PAIRS, Some(1..1))] // unmatched trailing quote
    #[case(r#""foo"'bar'`baz`"#, 0, QUOTE_PAIRS, Some(1..4))] // cursor at start, should find first quotes
    #[case(r#"foo'bar""#, 1, QUOTE_PAIRS, None)] // mismatched quotes
    #[case("no quotes here", 5, QUOTE_PAIRS, None)] // no quotes in buffer
    #[case("", 0, QUOTE_PAIRS, None)] // empty buffer
    fn test_range_inside_next_pair_in_group(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] pairs: &[(char, char); 3],
        #[case] expected: Option<Range<usize>>,
    ) {
        let mut buf = LineBuffer::from(input);
        buf.set_insertion_point(cursor_pos);
        assert_eq!(buf.range_inside_next_pair_in_group(pairs), expected);
    }

    // Tests for range_inside_current_pair - when cursor is inside a pair
    #[rstest]
    #[case("(abc)", 1, '(', ')', Some(1..4))] // cursor inside simple pair
    #[case("foo(bar)baz", 3, '(', ')', Some(4..7))] // cursor inside pair
    #[case("[abc]", 1, '[', ']', Some(1..4))] // square brackets
    #[case("{abc}", 1, '{', '}', Some(1..4))] // curly brackets
    #[case("foo(🦀bar)baz", 8, '(', ')', Some(4..11))] // emoji inside brackets - cursor inside (on 'b')
    #[case("🦀(bar)🦀", 6, '(', ')', Some(5..8))] // emoji outside brackets - cursor inside
    #[case("()", 1, '(', ')', Some(1..1))] // empty pair
    #[case("foo()bar", 4, '(', ')', Some(4..4))] // empty pair - cursor inside
    // Cases where cursor is not inside any pair
    #[case("(abc)", 0, '(', ')', Some(1..4))] // cursor at start, not inside
    #[case("foo(bar)baz", 2, '(', ')', None)] // cursor before pair
    #[case("foo(bar)baz", 0, '(', ')', None)] // cursor at start of buffer
    #[case("", 0, '(', ')', None)] // empty string
    #[case("no brackets", 5, '(', ')', None)] // no brackets
    #[case("(unclosed", 1, '(', ')', None)] // unclosed bracket
    #[case("unclosed)", 1, '(', ')', None)] // unclosed bracket
    #[case("end of line", 11, '(', ')', None)] // unclosed bracket
    fn test_range_inside_current_pair(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] open_char: char,
        #[case] close_char: char,
        #[case] expected: Option<Range<usize>>,
    ) {
        let mut buf = LineBuffer::from(input);
        buf.set_insertion_point(cursor_pos);
        let result = buf.range_inside_current_pair(open_char, close_char);
        assert_eq!(
            result, expected,
            "Failed for input: '{}', cursor: {}, chars: '{}' '{}'",
            input, cursor_pos, open_char, close_char
        );
    }

    // Tests for range_inside_next_pair - when looking for the next pair forward
    #[rstest]
    #[case("(abc)", 0, '(', ')', Some(1..4))] // cursor at start, find first pair
    #[case("foo(bar)baz", 2, '(', ')', Some(4..7))] // cursor before pair
    #[case("(first)(second)", 4, '(', ')', Some(8..14))] // inside first, should find second
    #[case("()", 0, '(', ')', Some(1..1))] // empty pair
    #[case("foo()bar", 2, '(', ')', Some(4..4))] // empty pair
    #[case("[abc]", 0, '[', ']', Some(1..4))] // square brackets
    #[case("{abc}", 0, '{', '}', Some(1..4))] // curly brackets
    #[case("foo(🦀bar)baz", 0, '(', ')', Some(4..11))] // emoji inside brackets - find from start
    #[case("🦀(bar)🦀", 0, '(', ')', Some(5..8))] // emoji outside brackets - find from start
    #[case("", 0, '(', ')', None)] // empty string
    #[case("no brackets", 5, '(', ')', None)] // no brackets
    #[case("(unclosed", 1, '(', ')', None)] // unclosed bracket
    #[case("(abc)", 4, '(', ')', None)] // cursor after pair, no more pairs
    #[case(r#""""#, 0, '"', '"', Some(1..1))] // single quote pair (empty) - should find it ahead
    #[case(r#"""asdf"#, 0, '"', '"', Some(1..1))] // unmatched quote - should find it ahead
    #[case(r#""foo"'bar'`baz`"#, 0, '"', '"', Some(1..4))] // cursor at start, should find first quotes
    fn test_range_inside_next_pair(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] open_char: char,
        #[case] close_char: char,
        #[case] expected: Option<Range<usize>>,
    ) {
        let mut buf = LineBuffer::from(input);
        buf.set_insertion_point(cursor_pos);
        let result = buf.range_inside_next_pair(open_char, close_char);
        assert_eq!(
            result, expected,
            "Failed for input: '{}', cursor: {}, chars: '{}' '{}'",
            input, cursor_pos, open_char, close_char
        );
    }

    #[rstest]
    // Test next quote is restricted to single line
    #[case("line1\n\"quote\"", 7, '"', '"', None)] // Inside second line quote, no quotes after
    #[case("\"quote\"\nline2", 2, '"', '"', None)] // No next quote on current line
    #[case("line1\n\"quote\"", 6, '"', '"', Some(7..12))] // cursor at start of line 2
    #[case("line1\n\"quote\"", 0, '"', '"', None)] // cursor line 1 doesn't find quote on line 2
    #[case("line1\n\"quote\"", 5, '"', '"', None)] // cursor at end of line 1
    fn test_multiline_next_quote_multiline(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] open_char: char,
        #[case] close_char: char,
        #[case] expected: Option<Range<usize>>,
    ) {
        let mut buf = LineBuffer::from(input);
        buf.set_insertion_point(cursor_pos);
        let result = buf.range_inside_next_pair(open_char, close_char);
        assert_eq!(
            result,
            expected,
            "MULTILINE TEST - Input: {:?}, cursor: {}, chars: '{}' '{}', lines: {:?}",
            input,
            cursor_pos,
            open_char,
            close_char,
            input.lines().collect::<Vec<_>>()
        );
    }

    // Test that range_inside_current_pair work across multiple lines
    #[rstest]
    #[case("line1\n(bracket)", 7, '(', ')', Some(7..14))] // cursor at bracket start on line 2
    #[case("(bracket)\nline2", 2, '(', ')', Some(1..8))] // cursor inside bracket on line 1
    #[case("line1\n(bracket)", 5, '(', ')', None)] // cursor end of line 1
    #[case("(1\ninner\n3)", 4, '(', ')', Some(1..10))] // bracket spanning 3 lines
    #[case("(1\ninner\n3)", 2, '(', ')', Some(1..10))] // bracket spanning 3 lines, cursor end of line 1
    #[case("outer(\ninner(\ndeep\n)\nback\n)", 15, '(', ')', Some(13..19))] // nested multiline brackets
    #[case("outer(\ninner(\ndeep\n)\nback\n)", 8, '(', ')', Some(6..26))] // nested multiline brackets
    #[case("{\nkey: [\n  value\n]\n}", 10, '[', ']', Some(8..17))] // mixed bracket types across lines
    fn test_multiline_bracket_behavior(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] open_char: char,
        #[case] close_char: char,
        #[case] expected: Option<Range<usize>>,
    ) {
        let mut buf = LineBuffer::from(input);
        buf.set_insertion_point(cursor_pos);
        let result = buf.range_inside_current_pair(open_char, close_char);
        assert_eq!(
            result,
            expected,
            "MULTILINE BRACKET TEST - Input: {:?}, cursor: {}, chars: '{}' '{}', lines: {:?}",
            input,
            cursor_pos,
            open_char,
            close_char,
            input.lines().collect::<Vec<_>>()
        );
    }

    // Test next brackets work across multiple lines (unlike quotes which are line-restricted)
    #[rstest]
    #[case("line1\n(bracket)", 2, '(', ')', Some(7..14))] // cursor at bracket start on line 2
    #[case("line1\n(bracket)", 5, '(', ')', Some(7..14))] // cursor end of line 1
    #[case("outer(\ninner(\ndeep\n)\nback\n)", 0, '(', ')', Some(6..26))] // nested multiline brackets
    #[case("outer(\ninner(\ndeep\n)\nback\n)", 8, '(', ')', Some(13..19))] // nested multiline brackets
    fn test_multiline_next_bracket_behavior(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] open_char: char,
        #[case] close_char: char,
        #[case] expected: Option<Range<usize>>,
    ) {
        let mut buf = LineBuffer::from(input);
        buf.set_insertion_point(cursor_pos);
        let result = buf.range_inside_next_pair(open_char, close_char);
        assert_eq!(
            result,
            expected,
            "MULTILINE BRACKET TEST - Input: {:?}, cursor: {}, chars: '{}' '{}', lines: {:?}",
            input,
            cursor_pos,
            open_char,
            close_char,
            input.lines().collect::<Vec<_>>()
        );
    }

    // Unicode safety tests for core pair-finding functionality
    #[rstest]
    #[case("(🦀)", 1, '(', ')', Some(1..5))] // emoji inside brackets
    #[case("🦀(text)🦀", 5, '(', ')', Some(5..9))] // emojis outside brackets
    #[case("(multi👨‍👩‍👧‍👦family)", 1, '(', ')', Some(1..37))] // complex emoji family inside (25 bytes)
    #[case("(åëïöü)", 1, '(', ')', Some(1..11))] // accented characters
    #[case("(mixed🦀åëïtext)", 1, '(', ')', Some(1..20))] // mixed unicode content
    #[case("'🦀emoji🦀'", 1, '\'', '\'', Some(1..14))] // emojis in quotes
    #[case("'mixed👨‍👩‍👧‍👦åëï'", 1, '\'', '\'', Some(1..37))] // complex 25 byte family emoji
    fn test_range_inside_current_pair_unicode_safety(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] open_char: char,
        #[case] close_char: char,
        #[case] expected: Option<Range<usize>>,
    ) {
        let mut buf = LineBuffer::from(input);
        buf.set_insertion_point(cursor_pos);
        let result = buf.range_inside_current_pair(open_char, close_char);
        assert_eq!(result, expected);
        // Verify buffer remains valid after operations
        assert!(buf.is_valid());
    }

    #[rstest]
    #[case("start🦀(content)end", 0, '(', ')', Some(10..17))] // emoji before brackets
    #[case("start(🦀)end", 0, '(', ')', Some(6..10))] // emoji inside brackets to find
    #[case("🦀'text'🦀", 0, '\'', '\'', Some(5..9))] // emoji before quotes
    #[case("start'🦀text🦀'", 0, '\'', '\'', Some(6..18))] // emoji before quotes
    #[case("start'multi👨‍👩‍👧‍👦family'end", 0, '\'', '\'', Some(6..42))] // complex 25 byte family emoji
    #[case("start'👨‍👩‍👧‍👦multifamily'end", 0, '\'', '\'', Some(6..42))] // complex 25 byte family emoji
    fn test_range_inside_next_pair_unicode_safety(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] open_char: char,
        #[case] close_char: char,
        #[case] expected: Option<Range<usize>>,
    ) {
        let mut buf = LineBuffer::from(input);
        buf.set_insertion_point(cursor_pos);
        let result = buf.range_inside_next_pair(open_char, close_char);
        assert_eq!(result, expected);
        // Verify buffer remains valid after operations
        assert!(buf.is_valid());
    }
}
