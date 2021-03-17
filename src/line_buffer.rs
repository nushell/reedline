use std::ops::Deref;
use unicode_segmentation::UnicodeSegmentation;

pub struct LineBuffer {
    buffer: String,
    insertion_point: usize,
}

impl Deref for LineBuffer {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl LineBuffer {
    pub fn new() -> LineBuffer {
        LineBuffer {
            buffer: String::new(),
            insertion_point: 0,
        }
    }

    pub fn set_insertion_point(&mut self, pos: usize) {
        self.insertion_point = pos;
    }

    pub fn get_insertion_point(&self) -> usize {
        self.insertion_point
    }

    pub fn set_buffer(&mut self, buffer: String) {
        self.buffer = buffer;
    }

    pub fn move_to_end(&mut self) -> usize {
        self.insertion_point = self.buffer.len();

        self.insertion_point
    }

    pub fn grapheme_right_index(&self) -> usize {
        self.buffer[self.insertion_point..]
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| self.insertion_point + i)
            .unwrap_or_else(|| self.buffer.len())
    }

    pub fn grapheme_left_index(&self) -> usize {
        self.buffer[..self.insertion_point]
            .grapheme_indices(true)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    pub fn word_right_index(&self) -> usize {
        self.buffer[self.insertion_point..]
            .split_word_bound_indices()
            .find(|(_, word)| !is_word_boundary(word))
            .map(|(i, word)| self.insertion_point + i + word.len())
            .unwrap_or_else(|| self.buffer.len())
    }

    pub fn word_left_index(&self) -> usize {
        self.buffer[..self.insertion_point]
            .split_word_bound_indices()
            .filter(|(_, word)| !is_word_boundary(word))
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
    pub fn move_right(&mut self) {
        self.insertion_point = self.grapheme_right_index();
    }

    pub fn move_left(&mut self) {
        self.insertion_point = self.grapheme_left_index();
    }

    pub fn move_word_left(&mut self) -> usize {
        self.insertion_point = self.word_left_index();
        self.insertion_point
    }

    pub fn move_word_right(&mut self) -> usize {
        self.insertion_point = self.word_right_index();
        self.insertion_point
    }

    pub fn insert_char(&mut self, pos: usize, c: char) {
        self.buffer.insert(pos, c)
    }

    pub fn insert_str(&mut self, idx: usize, string: &str) {
        self.buffer.insert_str(idx, string)
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.insertion_point = 0;
    }

    pub fn clear_to_end(&mut self) {
        self.buffer.truncate(self.insertion_point);
    }

    pub fn clear_to_insertion_point(&mut self) {
        self.clear_range(..self.insertion_point);
        self.insertion_point = 0;
    }

    pub fn clear_range<R>(&mut self, range: R)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.replace_range(range, "");
    }

    pub fn replace_range<R>(&mut self, range: R, replace_with: &str)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.buffer.replace_range(range, replace_with);
    }

    pub fn on_whitespace(&self) -> bool {
        self.buffer[self.get_insertion_point()..]
            .chars()
            .next()
            .map(|c| c.is_whitespace())
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
    "😊";
    "🤦🏼‍♂️";
}
