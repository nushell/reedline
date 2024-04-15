use nu_ansi_term::Style;

use crate::*;

impl super::Reedline {
    /// Toggle whether reedline enables bracketed paste to reed copied content
    ///
    /// This currently alters the behavior for multiline pastes as pasting of regular text will
    /// execute after every complete new line as determined by the [`Validator`]. With enabled
    /// bracketed paste all lines will appear in the buffer and can then be submitted with a
    /// separate enter.
    ///
    /// At this point most terminals should support it or ignore the setting of the necessary
    /// flags. For full compatibility, keep it disabled.
    pub fn use_bracketed_paste(mut self, enable: bool) -> Self {
        self.bracketed_paste.set(enable);
        self
    }

    /// Toggle whether reedline uses the kitty keyboard enhancement protocol
    ///
    /// This allows us to disambiguate more events than the traditional standard
    /// Only available with a few terminal emulators.
    /// You can check for that with [`crate::kitty_protocol_available`]
    /// `Reedline` will perform this check internally
    ///
    /// Read more: <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>
    pub fn use_kitty_keyboard_enhancement(mut self, enable: bool) -> Self {
        self.kitty_protocol.set(enable);
        self
    }

    /// Return the previously generated history session id
    pub fn get_history_session_id(&self) -> Option<HistorySessionId> {
        self.history_session_id
    }

    /// Set a new history session id
    /// This should be used in situations where the user initially did not have a history_session_id
    /// and then later realized they want to have one without restarting the application.
    pub fn set_history_session_id(&mut self, session: Option<HistorySessionId>) {
        self.history_session_id = session;
    }

    /// A builder to include a [`Hinter`] in your instance of the Reedline engine
    /// # Example
    /// ```rust
    /// //Cargo.toml
    /// //[dependencies]
    /// //nu-ansi-term = "*"
    /// use {
    ///     nu_ansi_term::{Color, Style},
    ///     reedline::{DefaultHinter, Reedline},
    /// };
    ///
    /// let mut line_editor = Reedline::create().with_hinter(Box::new(
    ///     DefaultHinter::default()
    ///     .with_style(Style::new().italic().fg(Color::LightGray)),
    /// ));
    /// ```
    #[must_use]
    pub fn with_hinter(mut self, hinter: Box<dyn Hinter>) -> Self {
        self.hinter = Some(hinter);
        self
    }

    /// Remove current [`Hinter`]
    #[must_use]
    pub fn disable_hints(mut self) -> Self {
        self.hinter = None;
        self
    }

    /// A builder to configure the tab completion
    /// # Example
    /// ```rust
    /// // Create a reedline object with tab completions support
    ///
    /// use reedline::{DefaultCompleter, Reedline};
    ///
    /// let commands = vec![
    ///   "test".into(),
    ///   "hello world".into(),
    ///   "hello world reedline".into(),
    ///   "this is the reedline crate".into(),
    /// ];
    /// let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));
    ///
    /// let mut line_editor = Reedline::create().with_completer(completer);
    /// ```
    #[must_use]
    pub fn with_completer(mut self, completer: Box<dyn Completer>) -> Self {
        self.completer = completer;
        self
    }

    /// Turn on quick completions. These completions will auto-select if the completer
    /// ever narrows down to a single entry.
    #[must_use]
    pub fn with_quick_completions(mut self, quick_completions: bool) -> Self {
        self.quick_completions = quick_completions;
        self
    }

    /// Turn on partial completions. These completions will fill the buffer with the
    /// smallest common string from all the options
    #[must_use]
    pub fn with_partial_completions(mut self, partial_completions: bool) -> Self {
        self.partial_completions = partial_completions;
        self
    }

    /// A builder which enables or disables the use of ansi coloring in the prompt
    /// and in the command line syntax highlighting.
    #[must_use]
    pub fn with_ansi_colors(mut self, use_ansi_coloring: bool) -> Self {
        self.use_ansi_coloring = use_ansi_coloring;
        self
    }

    /// A builder that configures the highlighter for your instance of the Reedline engine
    /// # Example
    /// ```rust
    /// // Create a reedline object with highlighter support
    ///
    /// use reedline::{ExampleHighlighter, Reedline};
    ///
    /// let commands = vec![
    ///   "test".into(),
    ///   "hello world".into(),
    ///   "hello world reedline".into(),
    ///   "this is the reedline crate".into(),
    /// ];
    /// let mut line_editor =
    /// Reedline::create().with_highlighter(Box::new(ExampleHighlighter::new(commands)));
    /// ```
    #[must_use]
    pub fn with_highlighter(mut self, highlighter: Box<dyn Highlighter>) -> Self {
        self.highlighter = highlighter;
        self
    }

    /// A builder that configures the style used for visual selection
    #[must_use]
    pub fn with_visual_selection_style(mut self, style: Style) -> Self {
        self.visual_selection_style = style;
        self
    }

    /// A builder which configures the history for your instance of the Reedline engine
    /// # Example
    /// ```rust,no_run
    /// // Create a reedline object with history support, including history size limits
    ///
    /// use reedline::{FileBackedHistory, Reedline};
    ///
    /// let history = Box::new(
    /// FileBackedHistory::with_file(5, "history.txt".into())
    ///     .expect("Error configuring history with file"),
    /// );
    /// let mut line_editor = Reedline::create()
    ///     .with_history(history);
    /// ```
    #[must_use]
    pub fn with_history(mut self, history: Box<dyn History>) -> Self {
        self.history = history;
        self
    }

    /// A builder which configures history exclusion for your instance of the Reedline engine
    /// # Example
    /// ```rust,no_run
    /// // Create a reedline instance with history that will *not* include commands starting with a space
    ///
    /// use reedline::{FileBackedHistory, Reedline};
    ///
    /// let history = Box::new(
    /// FileBackedHistory::with_file(5, "history.txt".into())
    ///     .expect("Error configuring history with file"),
    /// );
    /// let mut line_editor = Reedline::create()
    ///     .with_history(history)
    ///     .with_history_exclusion_prefix(Some(" ".into()));
    /// ```
    #[must_use]
    pub fn with_history_exclusion_prefix(mut self, ignore_prefix: Option<String>) -> Self {
        self.history_exclusion_prefix = ignore_prefix;
        self
    }

    /// A builder that configures the validator for your instance of the Reedline engine
    /// # Example
    /// ```rust
    /// // Create a reedline object with validator support
    ///
    /// use reedline::{DefaultValidator, Reedline};
    ///
    /// let mut line_editor =
    /// Reedline::create().with_validator(Box::new(DefaultValidator));
    /// ```
    #[must_use]
    pub fn with_validator(mut self, validator: Box<dyn Validator>) -> Self {
        self.validator = Some(validator);
        self
    }

    /// Remove the current [`Validator`]
    #[must_use]
    pub fn disable_validator(mut self) -> Self {
        self.validator = None;
        self
    }

    /// Set a different prompt to be used after submitting each line
    #[must_use]
    pub fn with_transient_prompt(mut self, transient_prompt: Box<dyn Prompt>) -> Self {
        self.transient_prompt = Some(transient_prompt);
        self
    }

    /// A builder which configures the edit mode for your instance of the Reedline engine
    #[must_use]
    pub fn with_edit_mode(mut self, edit_mode: Box<dyn EditMode>) -> Self {
        self.edit_mode = edit_mode;
        self
    }

    /// A builder that appends a menu to the engine
    #[must_use]
    pub fn with_menu(mut self, menu: ReedlineMenu) -> Self {
        self.menus.push(menu);
        self
    }

    /// A builder that clears the list of menus added to the engine
    #[must_use]
    pub fn clear_menus(mut self) -> Self {
        self.menus = Vec::new();
        self
    }

    /// A builder that adds the history item id
    #[must_use]
    pub fn with_history_session_id(mut self, session: Option<HistorySessionId>) -> Self {
        self.history_session_id = session;
        self
    }

    /// A builder that enables reedline changing the cursor shape based on the current edit mode.
    /// The current implementation sets the cursor shape when drawing the prompt.
    /// Do not use this if the cursor shape is set elsewhere, e.g. in the terminal settings or by ansi escape sequences.
    pub fn with_cursor_config(mut self, cursor_shapes: CursorConfig) -> Self {
        self.cursor_shapes = Some(cursor_shapes);
        self
    }

    /// Returns the corresponding expected prompt style for the given edit mode
    pub fn prompt_edit_mode(&self) -> PromptEditMode {
        self.edit_mode.edit_mode()
    }
}
