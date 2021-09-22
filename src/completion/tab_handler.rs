use crate::{core_editor::LineBuffer, ComplationActionHandler, Completer, DefaultCompleter};

/// A simple handler that will do a cycle-based rotation through the options given by the Completer
pub struct DefaultCompletionActionHandler {
    completer: Box<dyn Completer>,
    initial_line: LineBuffer,
    index: usize,

    last_buffer: Option<LineBuffer>,
}

impl DefaultCompletionActionHandler {
    /// Build a DefaultCompletionActionHander configured to use a specific completer
    ///
    /// # Arguments
    ///
    /// * `completer`    The completion logic to use
    ///
    /// # Example
    /// ```
    /// use reedline::{DefaultCompletionActionHandler, DefaultCompleter, Completer, Span};
    ///
    /// let mut completer = DefaultCompleter::default();
    /// completer.insert(vec!["test-hyphen","test_underscore"].iter().map(|s| s.to_string()).collect());
    /// assert_eq!(
    ///     completer.complete("te",2),
    ///     vec![(Span { start: 0, end: 2 }, "test".into())]);
    ///
    /// let mut completions = DefaultCompletionActionHandler::default().with_completer(Box::new(completer));
    /// ```
    pub fn with_completer(
        mut self,
        completer: Box<dyn Completer>,
    ) -> DefaultCompletionActionHandler {
        self.completer = completer;
        self
    }
}
impl Default for DefaultCompletionActionHandler {
    fn default() -> Self {
        DefaultCompletionActionHandler {
            completer: Box::new(DefaultCompleter::default()),
            initial_line: LineBuffer::new(),
            index: 0,
            last_buffer: None,
        }
    }
}

impl DefaultCompletionActionHandler {
    fn reset_index(&mut self) {
        self.index = 0;
    }
}

impl ComplationActionHandler for DefaultCompletionActionHandler {
    // With this function we handle the tab events.
    //
    // If completions vector is not empty we proceed to replace
    //  in the line_buffer only the specified range of characters.
    // If internal index is 0 it means that is the first tab event pressed.
    // If internal index is greater than completions vector, we bring it back to 0.
    fn handle(&mut self, present_buffer: &mut LineBuffer) {
        if let Some(last_buffer) = &self.last_buffer {
            if last_buffer != present_buffer {
                self.reset_index();
            }
        }

        // NOTE: This is required to cycle through the tabs for what is presently present in the
        // buffer. Without this `repetitive_calls_to_handle_works` will not work
        if self.index == 0 {
            self.initial_line = present_buffer.clone();
        } else {
            *present_buffer = self.initial_line.clone();
        }

        let completions = self
            .completer
            .complete(present_buffer.get_buffer(), present_buffer.offset());

        if !completions.is_empty() {
            match self.index {
                index if index < completions.len() => {
                    self.index += 1;
                    let span = completions[index].0;

                    let mut offset = present_buffer.offset();
                    offset += completions[index].1.len() - (span.end - span.start);

                    // TODO improve the support for multiline replace
                    present_buffer.replace(span.start..span.end, &completions[index].1);
                    present_buffer.set_insertion_point(offset);
                }
                _ => {
                    self.reset_index();
                }
            }
        }
        self.last_buffer = Some(present_buffer.clone());
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn get_tab_handler_with(values: Vec<&'_ str>) -> DefaultCompletionActionHandler {
        let mut completer = DefaultCompleter::default();
        completer.insert(values.iter().map(|s| s.to_string()).collect());

        DefaultCompletionActionHandler::default().with_completer(Box::new(completer))
    }

    fn buffer_with(content: &str) -> LineBuffer {
        let mut line_buffer = LineBuffer::new();
        line_buffer.insert_str(content);

        line_buffer
    }

    #[test]
    fn repetitive_calls_to_handle_works() {
        let mut tab = get_tab_handler_with(vec!["login", "logout"]);
        let mut buf = buffer_with("lo");
        tab.handle(&mut buf);

        assert_eq!(buf, buffer_with("login"));
        tab.handle(&mut buf);
        assert_eq!(buf, buffer_with("logout"));
        tab.handle(&mut buf);
        assert_eq!(buf, buffer_with("lo"));
    }

    #[test]
    fn behaviour_with_hyphens_and_underscores() {
        let mut tab = get_tab_handler_with(vec!["test-hyphen", "test_underscore"]);
        let mut buf = buffer_with("te");
        tab.handle(&mut buf);

        assert_eq!(buf, buffer_with("test"));
        tab.handle(&mut buf);
        assert_eq!(buf, buffer_with("te"));
    }

    #[test]
    fn auto_resets_on_new_query() {
        let mut tab = get_tab_handler_with(vec!["login", "logout", "exit"]);
        let mut buf = buffer_with("log");
        tab.handle(&mut buf);

        assert_eq!(buf, buffer_with("login"));
        let mut new_buf = buffer_with("ex");
        tab.handle(&mut new_buf);
        assert_eq!(new_buf, buffer_with("exit"));
    }

    #[test]
    fn same_string_different_places() {
        let mut tab = get_tab_handler_with(vec!["that", "this"]);
        let mut buf = buffer_with("th is my test th");

        // Hitting tab after `th` fills the first completion `that`
        buf.set_insertion_point(2);
        tab.handle(&mut buf);
        let mut expected_buffer = buffer_with("that is my test th");
        expected_buffer.set_insertion_point(4);
        assert_eq!(buf, expected_buffer);

        // updating the cursor to end should reset the completions
        buf.set_insertion_point(18);
        tab.handle(&mut buf);
        assert_eq!(buf, buffer_with("that is my test that"));
    }
}
