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

    pub fn with_history<T: History + 'static>(mut self, value: T) -> Self {
        self.history = Some(Box::new(value));
        self
    }

    pub fn without_history(mut self) -> Self {
        self.history = None;
        self
    }

    pub fn history(&self) -> Option<&dyn History> {
        (&self.history).as_deref()
    }

    pub fn with_hints<T: Hinter + 'static>(mut self, value: T) -> Self {
        self.hinter = Some(Box::new(value));
        self
    }

    pub fn without_hints(mut self) -> Self {
        self.hinter = None;
        self
    }

    pub fn hints(&self) -> Option<&dyn Hinter> {
        (&self.hinter).as_deref()
    }

    pub fn with_completions<T: Completer + 'static>(mut self, value: T) -> Self {
        self.completer = Some(Box::new(value));
        self
    }

    pub fn without_completions(mut self) -> Self {
        self.completer = None;
        self
    }

    pub fn completions(&self) -> Option<&dyn Completer> {
        (&self.completer).as_deref()
    }

    pub fn with_highlighter<T: Highlighter + 'static>(mut self, value: T) -> Self {
        self.highlighter = Some(Box::new(value));
        self
    }

    pub fn without_highlighter(mut self) -> Self {
        self.highlighter = None;
        self
    }

    pub fn highlighter(&self) -> Option<&dyn Highlighter> {
        (&self.highlighter).as_deref()
    }

    pub fn with_validator<T: Validator + 'static>(mut self, value: T) -> Self {
        self.validator = Some(Box::new(value));
        self
    }

    pub fn without_validator(mut self) -> Self {
        self.validator = None;
        self
    }

    pub fn validator(&self) -> Option<&dyn Validator> {
        (&self.validator).as_deref()
    }

    pub fn with_transient_prompt<T: Prompt + 'static>(mut self, value: T) -> Self {
        self.transient_prompt = Some(Box::new(value));
        self
    }

    pub fn without_transient_prompt(mut self) -> Self {
        self.transient_prompt = None;
        self
    }

    pub fn transient_prompt(&self) -> Option<&dyn Prompt> {
        (&self.transient_prompt).as_deref()
    }

    pub fn with_edit_mode<T: EditMode + 'static>(mut self, value: T) -> Self {
        self.edit_mode = Some(Box::new(value));
        self
    }

    pub fn without_edit_mode(mut self) -> Self {
        self.edit_mode = None;
        self
    }

    pub fn edit_mode(&self) -> Option<&dyn EditMode> {
        (&self.edit_mode).as_deref()
    }

    pub fn with_history_exclusion_prefix(mut self, value: String) -> Self {
        self.history_exclusion_prefix = Some(value);
        self
    }

    pub fn without_history_exclusion_prefix(mut self) -> Self {
        self.history_exclusion_prefix = None;
        self
    }

    pub fn history_exclusion_prefix(&self) -> Option<&String> {
        (&self.history_exclusion_prefix).as_ref()
    }

    pub fn with_selection_style(mut self, value: Style) -> Self {
        self.visual_selection_style = Some(value);
        self
    }

    pub fn without_selection_style(mut self) -> Self {
        self.visual_selection_style = None;
        self
    }

    pub fn selection_style(&self) -> Option<&Style> {
        (&self.visual_selection_style).as_ref()
    }

    pub fn with_cursor_config(mut self, value: CursorConfig) -> Self {
        self.cursor_shapes = Some(value);
        self
    }

    pub fn without_cursor_config(mut self) -> Self {
        self.cursor_shapes = None;
        self
    }

    pub fn cursor_config(&self) -> Option<&CursorConfig> {
        (&self.cursor_shapes).as_ref()
    }

    pub fn with_history_session_id(mut self, value: HistorySessionId) -> Self {
        self.history_session_id = Some(value);
        self
    }

    pub fn without_history_session_id(mut self) -> Self {
        self.history_session_id = None;
        self
    }

    pub fn history_session_id(&self) -> Option<&HistorySessionId> {
        (&self.history_session_id).as_ref()
    }

    #[cfg(feature = "external_printer")]
    pub fn with_external_printer(mut self, value: ExternalPrinter<String>) -> Self {
        self.external_printer = Some(value);
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

    pub fn use_quick_completions(mut self, value: bool) -> Self {
        self.quick_completions = value;
        self
    }

    pub fn quick_completions(&self) -> bool {
        self.quick_completions
    }

    pub fn use_partial_completions(mut self, value: bool) -> Self {
        self.partial_completions = value;
        self
    }

    pub fn partial_completions(&self) -> bool {
        self.partial_completions
    }

    pub fn use_bracketed_paste(mut self, value: bool) -> Self {
        self.bracketed_paste = value;
        self
    }

    pub fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }

    pub fn use_kitty_keyboard_enhancement(mut self, value: bool) -> Self {
        self.kitty_protocol = value;
        self
    }

    pub fn kitty_keyboard_enhancement(&self) -> bool {
        self.kitty_protocol
    }

    pub fn use_ansi_colors(mut self, value: bool) -> Self {
        self.use_ansi_coloring = value;
        self
    }

    pub fn ansi_colors(&self) -> bool {
        self.use_ansi_coloring
    }

    pub fn add_menu(mut self, menu: ReedlineMenu) -> Self {
        self.menus.push(menu);
        self
    }

    // `menus` cannot accept a slice because ReedlineMenu is not Clone
    pub fn add_menus(mut self, menus: Vec<ReedlineMenu>) -> Self {
        self.menus.extend(menus);
        self
    }

    pub fn with_buffer_editor(mut self, mut editor: Command, temp_file: PathBuf) -> Self {
        if !editor.get_args().contains(&temp_file.as_os_str()) {
            editor.arg(&temp_file);
        }
        self.buffer_editor = Some(BufferEditor {
            command: editor,
            temp_file,
        });
        self
    }
}
