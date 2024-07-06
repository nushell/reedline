mod cwd_aware;
mod default;
pub use cwd_aware::CwdAwareHinter;
pub use default::DefaultHinter;

use unicode_segmentation::UnicodeSegmentation;

pub fn is_whitespace_str(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

pub fn get_first_token(string: &str) -> String {
    let mut reached_content = false;
    let result = string
        .split_word_bounds()
        .take_while(|word| match (is_whitespace_str(word), reached_content) {
            (_, true) => false,
            (true, false) => true,
            (false, false) => {
                reached_content = true;
                true
            }
        })
        .collect::<Vec<&str>>()
        .join("")
        .to_string();
    result
}

use crate::History;
/// A trait that's responsible for returning the hint for the current line and position
/// Hints are often shown in-line as part of the buffer, showing the user text they can accept or ignore
pub trait Hinter: Send {
    /// Handle the hinting duty by using the line, position, and current history
    ///
    /// Returns the formatted output to show the user
    fn handle(
        &mut self,
        line: &str,
        pos: usize,
        history: &dyn History,
        use_ansi_coloring: bool,
        cwd: &str,
    ) -> String;

    /// Return the current hint unformatted to perform the completion of the full hint
    fn complete_hint(&self) -> String;

    /// Return the first semantic token of the hint
    /// for incremental completion
    fn next_hint_token(&self) -> String;
}
