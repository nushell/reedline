use unicode_segmentation::UnicodeSegmentation;

pub struct LineBuffer {
    buffer: String,
    insertion_point: usize,
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

    pub fn get_buffer(&self) -> &str {
        &self.buffer
    }

    fn get_grapheme_indices(&self) -> Vec<(usize, &str)> {
        UnicodeSegmentation::grapheme_indices(self.buffer.as_str(), true).collect()
    }

    pub fn inc_insertion_point(&mut self) {
        // let char_indices: Vec<_> = self.buffer.char_indices().collect();

        // for i in 0..char_indices.len() {

        // }
        // for (idx, c) in char_indices {
        //     if idx == self.insertion_point
        // }
        let grapheme_indices = self.get_grapheme_indices();
        for i in 0..grapheme_indices.len() {
            if grapheme_indices[i].0 == self.insertion_point && i < (grapheme_indices.len() - 1) {
                self.insertion_point = grapheme_indices[i + 1].0;
                return;
            }
        }
        self.insertion_point = self.buffer.len();

        //TODO if we should have found the boundary but didn't, we should panic
    }

    pub fn dec_insertion_point(&mut self) {
        let grapheme_indices = self.get_grapheme_indices();
        if self.insertion_point == self.buffer.len() {
            if let Some(index_pair) = grapheme_indices.last() {
                self.insertion_point = index_pair.0;
            } else {
                self.insertion_point = 0;
            }
        } else {
            for i in 0..grapheme_indices.len() {
                if grapheme_indices[i].0 == self.insertion_point && i > 1 {
                    self.insertion_point = grapheme_indices[i - 1].0;
                    return;
                }
            }
            self.insertion_point = 0;
        }

        // self.insertion_point -= 1;
    }

    pub fn get_buffer_len(&self) -> usize {
        self.buffer.len()
    }

    pub fn slice_buffer(&self, pos: usize) -> &str {
        &self.buffer[pos..]
    }

    pub fn insert_char(&mut self, pos: usize, c: char) {
        self.buffer.insert(pos, c)
    }

    pub fn remove_char(&mut self, pos: usize) -> char {
        self.buffer.remove(pos)
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn pop(&mut self) -> Option<char> {
        self.buffer.pop()
    }

    pub fn clear(&mut self) {
        self.buffer.clear()
    }

    pub fn move_word_left(&mut self) -> usize {
        let mut words = self.buffer[..self.insertion_point - 1] // valid UTF-8 slice when insertion_point at grapheme boundary
            .split_word_bound_indices()
            .rev();

        while let Some((index, word)) = words.next() {
            if !is_word_boundary(word) {
                self.insertion_point = index;
                return self.insertion_point;
            }
        }

        self.insertion_point = 0;
        self.insertion_point
    }

    pub fn move_word_right(&mut self) -> usize {
        let mut words = self.buffer[self.insertion_point..].split_word_bound_indices();

        while let Some((offset, word)) = words.next() {
            if !is_word_boundary(word) {
                // Move the insertion point just past the end of the next word
                self.insertion_point += offset + word.len();
                return self.insertion_point;
            }
        }

        self.insertion_point = self.buffer.len();
        self.insertion_point
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
