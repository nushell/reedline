use crate::history::History;

#[derive(Clone)]
pub struct BasicSearch {
    pub result: Option<(usize, usize)>,
    pub search_string: String,
}

pub enum BasicSearchCommand {
    InsertChar(char),
    Backspace,
    Next,
}

impl BasicSearch {
    pub fn new(search_string: String) -> Self {
        Self {
            result: None,
            search_string,
        }
    }

    pub fn step(&mut self, command: BasicSearchCommand, history: &History) {
        let mut start = self
            .result
            .map(|(history_index, _)| history_index)
            .unwrap_or(0);

        match command {
            BasicSearchCommand::InsertChar(c) => {
                self.search_string.push(c);
            }
            BasicSearchCommand::Backspace => {
                self.search_string.pop(); // TODO: Unicode grapheme?
            }
            BasicSearchCommand::Next => {
                start += 1;
            }
        }

        if self.search_string.is_empty() {
            self.result = None;
        } else {
            self.result = history
                .iter()
                .enumerate()
                .skip(start)
                .filter_map(|(history_index, s)| {
                    s.match_indices(&self.search_string)
                        .next()
                        .map(|(offset, _)| (history_index, offset))
                })
                .next();
        }
    }
}
