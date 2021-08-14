use crate::{core_editor::LineBuffer, ComplationActionHandler, Completer, DefaultCompleter};

/// A simple handler that will do a cycle-based rotation through the options given by the Completer
pub struct DefaultCompletionActionHandler {
    completer: Box<dyn Completer>,
    initial_line: LineBuffer,
    index: usize,
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
        }
    }
}
impl ComplationActionHandler for DefaultCompletionActionHandler {
    // With this function we handle the tab events.
    //
    // If completions vector is not empty we proceed to replace
    //  in the line_buffer only the specified range of characters.
    // If internal index is 0 it means that is the first tab event pressed.
    // If internal index is greater than completions vector, we bring it back to 0.
    fn handle(&mut self, line: &mut LineBuffer) {
        if self.index == 0 {
            self.initial_line = LineBuffer::new();
            self.initial_line.set_buffer(line.get_buffer().into());
            self.initial_line
                .set_insertion_point(line.line(), line.offset());
        } else {
            line.set_buffer(self.initial_line.get_buffer().into());
            line.set_insertion_point(self.initial_line.line(), self.initial_line.offset())
        }
        let completions = self
            .completer
            .complete(self.initial_line.get_buffer(), self.initial_line.offset());
        if !completions.is_empty() {
            match self.index {
                index if index < completions.len() => {
                    self.index += 1;
                    let span = completions[index].0;
                    let mut offset = line.offset();
                    offset += completions[index].1.len() - (span.end - span.start);

                    // TODO improve the support for multiline replace
                    line.replace(span.start..span.end, 0, &completions[index].1);
                    line.set_insertion_point(line.line(), offset);
                }
                _ => {
                    self.reset_index();
                }
            }
        }
    }

    // This function is required to reset the index
    // when following the completion we perform another action
    // that is not going to continue with the list of completions.
    fn reset_index(&mut self) {
        self.index = 0;
    }
}
