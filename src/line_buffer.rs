use unicode_segmentation::UnicodeSegmentation;

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

pub struct LineBuffer {
    lines: Vec<String>,
    insertion_point: InsertionPoint,
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

    pub fn insertion_point(&self) -> InsertionPoint {
        self.insertion_point
    }

    pub fn set_insertion_point(&mut self, pos: InsertionPoint) {
        self.insertion_point = pos;
    }

    pub fn insertion_line(&self) -> &str {
        &self.lines[self.insertion_point.line]
    }

    pub fn set_buffer(&mut self, buffer: String) {
        self.lines = vec![buffer];
        self.insertion_point = InsertionPoint::new();
    }

    pub fn move_to_start(&mut self) {
        self.insertion_point = InsertionPoint::new();
    }

    pub fn move_to_end(&mut self) {
        if let Some(end) = self.lines.last() {
            let length_of_last_line = end.len();
            self.insertion_point.offset = length_of_last_line;
        }
    }

    pub fn grapheme_right_index(&self) -> usize {
        self.lines[self.insertion_point.line][self.insertion_point.offset..]
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| self.insertion_point.offset + i)
            .unwrap_or_else(|| self.lines[self.insertion_point.line].len())
    }

    pub fn grapheme_left_index(&self) -> usize {
        self.lines[self.insertion_point.line][..self.insertion_point.offset]
            .grapheme_indices(true)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    pub fn word_right_index(&self) -> usize {
        self.lines[self.insertion_point.line][self.insertion_point.offset..]
            .split_word_bound_indices()
            .find(|(_, word)| !is_word_boundary(word))
            .map(|(i, word)| self.insertion_point.offset + i + word.len())
            .unwrap_or_else(|| self.lines[self.insertion_point.line].len())
    }

    pub fn word_left_index(&self) -> usize {
        self.lines[self.insertion_point.line][..self.insertion_point.offset]
            .split_word_bound_indices()
            .filter(|(_, word)| !is_word_boundary(word))
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
    pub fn move_right(&mut self) {
        self.insertion_point.offset = self.grapheme_right_index();
    }

    pub fn move_left(&mut self) {
        self.insertion_point.offset = self.grapheme_left_index();
    }

    pub fn move_word_left(&mut self) -> usize {
        self.insertion_point.offset = self.word_left_index();
        self.insertion_point.offset
    }

    pub fn move_word_right(&mut self) -> usize {
        self.insertion_point.offset = self.word_right_index();
        self.insertion_point.offset
    }

    pub fn insert_char(&mut self, pos: InsertionPoint, c: char) {
        self.lines[pos.line].insert(pos.offset, c)
    }

    pub fn insert_str(&mut self, idx: usize, string: &str) {
        self.lines[self.insertion_point.line].insert_str(idx, string)
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.lines.push(String::new());
        self.insertion_point = InsertionPoint::new();
    }

    pub fn clear_to_end(&mut self) {
        self.lines[self.insertion_point.line].truncate(self.insertion_point.offset);
    }

    pub fn clear_to_insertion_point(&mut self) {
        self.clear_range(..self.insertion_point.offset);
        self.insertion_point.offset = 0;
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
        self.lines[self.insertion_point.line].replace_range(range, replace_with);
    }

    pub fn on_whitespace(&self) -> bool {
        self.lines[self.insertion_point.line][self.insertion_point.offset..]
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
