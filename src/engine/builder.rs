use nu_ansi_term::Style;

use super::*;

pub struct ReedlineBuilder {
    history: Option<Box<dyn History>>,
    edit_mode: Option<Box<dyn EditMode>>,
    history_exclusion_prefix: Option<String>,
    validator: Option<Box<dyn Validator>>,
    completer: Option<Box<dyn Completer>>,
    quick_completions: bool,
    partial_completions: bool,
    highlighter: Option<Box<dyn Highlighter>>,
    buffer_editor: Option<BufferEditor>,
    visual_selection_style: Option<Style>,
    hinter: Option<Box<dyn Hinter>>,
    transient_prompt: Option<Box<dyn Prompt>>,
    menus: Vec<ReedlineMenu>,
    use_ansi_coloring: bool,
    bracketed_paste: bool,
    kitty_protocol: bool,
    #[cfg(feature = "external_printer")]
    external_printer: Option<ExternalPrinter<String>>,
    cursor_shapes: Option<CursorConfig>,
    history_session_id: Option<HistorySessionId>,
}

impl ReedlineBuilder {
    /// Create a new [`Builder`](ReedlineBuilder) with default configuration.
    pub const fn new() -> Self {
        Self {
            history: None,
            history_exclusion_prefix: None,
            validator: None,
            completer: None,
            partial_completions: false,
            quick_completions: false,
            highlighter: None,
            buffer_editor: None,
            visual_selection_style: None,
            hinter: None,
            transient_prompt: None,
            edit_mode: None,
            menus: Vec::new(),
            use_ansi_coloring: true,
            bracketed_paste: false,
            kitty_protocol: false,
            #[cfg(feature = "external_printer")]
            external_printer: None,
            cursor_shapes: None,
            history_session_id: None,
        }
    }

    /// Construct an [engine](crate::Reedline).
    #[must_use]
    pub fn build(self) -> Reedline {
        let mut bracketed_paste = BracketedPasteGuard::default();
        bracketed_paste.set(self.bracketed_paste);

        let mut kitty_protocol = KittyProtocolGuard::default();
        kitty_protocol.set(self.kitty_protocol);

        Reedline {
            editor: Editor::default(),
            painter: Painter::new(std::io::BufWriter::new(std::io::stderr())),
            hide_hints: false,
            history_cursor_on_excluded: false,
            history_last_run_id: None,
            history_excluded_item: None,

            history: self
                .history
                .unwrap_or_else(|| Box::<FileBackedHistory>::default()),
            completer: self
                .completer
                .unwrap_or_else(|| Box::<DefaultCompleter>::default()),
            highlighter: self
                .highlighter
                .unwrap_or_else(|| Box::<ExampleHighlighter>::default()),
            visual_selection_style: self
                .visual_selection_style
                .unwrap_or_else(|| Style::new().on(Color::LightGray)),
            edit_mode: self.edit_mode.unwrap_or_else(|| Box::<Emacs>::default()),

            hinter: self.hinter,
            validator: self.validator,
            use_ansi_coloring: self.use_ansi_coloring,
            quick_completions: self.quick_completions,
            partial_completions: self.partial_completions,
            history_exclusion_prefix: self.history_exclusion_prefix,
            buffer_editor: self.buffer_editor,
            transient_prompt: self.transient_prompt,
            menus: self.menus,
            cursor_shapes: self.cursor_shapes,
            history_session_id: self.history_session_id,
            bracketed_paste,
            kitty_protocol,
            #[cfg(feature = "external_printer")]
            external_printer: self.external_printer,

            history_cursor: HistoryCursor::new(
                HistoryNavigationQuery::Normal(LineBuffer::default()),
                self.history_session_id,
            ),
            input_mode: InputMode::Regular,
            executing_host_command: false,
        }
    }

    /// Use a [`History`](crate::History).
    pub fn with_history<H: History + 'static>(mut self, history: H) -> Self {
        self.history = Some(Box::new(history));
        self
    }

    /// Remove the current [`History`](crate::History), if any.
    pub fn without_history(mut self) -> Self {
        self.history = None;
        self
    }

    pub fn history(&self) -> Option<&dyn History> {
        (&self.history).as_deref()
    }

    /// Use a [`Hinter`](crate::Hinter).
    pub fn with_hints<H: Hinter + 'static>(mut self, hints: H) -> Self {
        self.hinter = Some(Box::new(hints));
        self
    }

    /// Remove the current [`Hinter`](crate::Hinter), if any.
    pub fn without_hints(mut self) -> Self {
        self.hinter = None;
        self
    }

    pub fn hints(&self) -> Option<&dyn Hinter> {
        (&self.hinter).as_deref()
    }

    /// Use a [`Completer`](crate::Completer).
    pub fn with_completions<C: Completer + 'static>(mut self, completions: C) -> Self {
        self.completer = Some(Box::new(completions));
        self
    }

    /// Remove the current [`Completer`](crate::Completer), if any.
    pub fn without_completions(mut self) -> Self {
        self.completer = None;
        self
    }

    pub fn completions(&self) -> Option<&dyn Completer> {
        (&self.completer).as_deref()
    }

    /// Use a [`Highlighter`](crate::Highlighter).
    pub fn with_highlighter<H: Highlighter + 'static>(mut self, highlighter: H) -> Self {
        self.highlighter = Some(Box::new(highlighter));
        self
    }

    /// Remove the current [`Highlighter`](crate::Highlighter), if any.
    pub fn without_highlighter(mut self) -> Self {
        self.highlighter = None;
        self
    }

    pub fn highlighter(&self) -> Option<&dyn Highlighter> {
        (&self.highlighter).as_deref()
    }

    /// Use a [`Validator`](crate::Validator).
    pub fn with_validator<V: Validator + 'static>(mut self, validator: V) -> Self {
        self.validator = Some(Box::new(validator));
        self
    }

    /// Remove the current [`Validator`](crate::Validator), if any.
    pub fn without_validator(mut self) -> Self {
        self.validator = None;
        self
    }

    pub fn validator(&self) -> Option<&dyn Validator> {
        (&self.validator).as_deref()
    }

    /// Use of a different prompt on submitted inputs.
    /// This *won't* touch the most recent prompt.
    pub fn with_transient_prompt<P: Prompt + 'static>(mut self, prompt: P) -> Self {
        self.transient_prompt = Some(Box::new(prompt));
        self
    }

    /// Use the same prompt on submitted inputs and draft inputs.
    pub fn without_transient_prompt(mut self) -> Self {
        self.transient_prompt = None;
        self
    }

    pub fn transient_prompt(&self) -> Option<&dyn Prompt> {
        (&self.transient_prompt).as_deref()
    }

    /// Set the initial edit mode
    pub fn with_edit_mode<E: EditMode + 'static>(mut self, edit_mode: E) -> Self {
        self.edit_mode = Some(Box::new(edit_mode));
        self
    }

    /// Unset the initial edit mode
    pub fn without_edit_mode(mut self) -> Self {
        self.edit_mode = None;
        self
    }

    pub fn edit_mode(&self) -> Option<&dyn EditMode> {
        (&self.edit_mode).as_deref()
    }

    /// Configure a history exclusion
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
    pub fn with_history_exclusion_prefix(mut self, ignore_prefix: String) -> Self {
        self.history_exclusion_prefix = Some(ignore_prefix);
        self
    }

    pub fn without_history_exclusion_prefix(mut self) -> Self {
        self.history_exclusion_prefix = None;
        self
    }

    pub fn history_exclusion_prefix(&self) -> Option<&String> {
        (&self.history_exclusion_prefix).as_ref()
    }

    pub fn with_selection_style(mut self, selection_style: Style) -> Self {
        self.visual_selection_style = Some(selection_style);
        self
    }

    pub fn without_selection_style(mut self) -> Self {
        self.visual_selection_style = None;
        self
    }

    /// Configure the styling of visual selection
    pub fn selection_style(&self) -> Option<&Style> {
        (&self.visual_selection_style).as_ref()
    }

    /// Let reedline dynamically determine and change the cursor shape depending
    /// on the the current edit mode. Don't use this if the cursor shape is set elsewhere,
    /// e.g. in the terminal settings or by ansi escape sequences.
    pub fn with_cursor_config(mut self, cursor_config: CursorConfig) -> Self {
        self.cursor_shapes = Some(cursor_config);
        self
    }

    pub fn without_cursor_config(mut self) -> Self {
        self.cursor_shapes = None;
        self
    }

    pub fn cursor_config(&self) -> Option<&CursorConfig> {
        (&self.cursor_shapes).as_ref()
    }

    /// Set a new history session id
    /// This should be used in situations where the user initially did not have a history_session_id
    /// and then later realized they want to have one without restarting the application.
    pub fn with_history_session_id(mut self, session: HistorySessionId) -> Self {
        self.history_session_id = Some(session);
        self
    }

    pub fn without_history_session_id(mut self) -> Self {
        self.history_session_id = None;
        self
    }

    pub fn history_session_id(&self) -> Option<HistorySessionId> {
        self.history_session_id.clone()
    }

    #[cfg(feature = "external_printer")]
    pub fn with_external_printer(mut self, printer: ExternalPrinter<String>) -> Self {
        self.external_printer = Some(printer);
        self
    }

    #[cfg(feature = "external_printer")]
    pub fn without_external_printer(mut self) -> Self {
        self.external_printer = None;
        self
    }

    #[cfg(feature = "external_printer")]
    pub fn external_printer(&self) -> Option<&ExternalPrinter<String>> {
        (&self.external_printer).as_ref()
    }

    /// Set whether to use quick completions. They will select and fill a completion
    /// if it's the only suggested one.
    pub fn use_quick_completions(mut self, enabled: bool) -> Self {
        self.quick_completions = enabled;
        self
    }

    pub fn quick_completions(&self) -> bool {
        self.quick_completions
    }

    /// Set whether to use partial completions. They will fill the buffer
    /// with the longest common substring.
    pub fn use_partial_completions(mut self, enabled: bool) -> Self {
        self.partial_completions = enabled;
        self
    }

    pub fn partial_completions(&self) -> bool {
        self.partial_completions
    }

    /// Set whether reedline should use bracketed paste for pasted input.
    ///
    /// This currently alters the behavior for multiline pastes as pasting of regular text will
    /// execute after every complete new line as determined by the [`Validator`]. With enabled
    /// bracketed paste all lines will appear in the buffer and can then be submitted with a
    /// separate enter.
    ///
    /// Most terminals should support or ignore this option. For full compatibility, keep it disabled.
    pub fn use_bracketed_paste(mut self, enabled: bool) -> Self {
        self.bracketed_paste = enabled;
        self
    }

    pub fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }

    pub fn use_kitty_keyboard_enhancement(mut self, enabled: bool) -> Self {
        self.kitty_protocol = enabled;
        self
    }

    /// Set whether reedline uses the kitty keyboard enhancement protocol
    ///
    /// This allows disambiguation of more events than the traditional standard.
    /// Note that not all terminal emulators support this protocol.
    /// You can check for that with [`crate::kitty_protocol_available`]
    /// `Reedline` will perform this check internally
    ///
    /// You can read more at [https://sw.kovidgoyal.net/kitty/keyboard-protocol/]
    pub fn kitty_keyboard_enhancement(&self) -> bool {
        self.kitty_protocol
    }

    /// Set whether reedline should use ansi colors in the terminal output.
    pub fn use_ansi_colors(mut self, enabled: bool) -> Self {
        self.use_ansi_coloring = enabled;
        self
    }

    pub fn ansi_colors(&self) -> bool {
        self.use_ansi_coloring
    }

    /// Add a menu.
    pub fn add_menu(mut self, menu: ReedlineMenu) -> Self {
        self.menus.push(menu);
        self
    }

    pub fn add_menus(mut self, menus: Vec<ReedlineMenu>) -> Self {
        // `menus` cannot accept a slice because ReedlineMenu is not Clone
        self.menus.extend(menus);
        self
    }

    /// Allow editing the line buffer through an external editor.
    ///
    /// You are responsible for providing a file path that is unique to this reedline session
    ///
    /// # Example
    /// ```rust,no_run
    /// // Create a reedline object with vim as editor
    ///
    /// use reedline::Reedline;
    /// use std::env::temp_dir;
    /// use std::process::Command;
    ///
    /// let temp_file = std::env::temp_dir().join("my-random-unique.file");
    /// let mut command = Command::new("vim");
    /// // you can provide additional flags:
    /// command.arg("-p"); // open in a vim tab (just for demonstration)
    /// // you don't have to pass the filename to the command
    /// let mut line_editor =
    /// Reedline::create().with_buffer_editor(command, temp_file);
    /// ```
    pub fn with_buffer_editor(mut self, mut editor: Command, temp_file: PathBuf) -> Self {
        if !editor.get_args().any(|arg| arg == temp_file.as_os_str()) {
            editor.arg(&temp_file);
        }
        self.buffer_editor = Some(BufferEditor {
            command: editor,
            temp_file,
        });
        self
    }
}
