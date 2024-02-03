use nu_ansi_term::Style;

use super::*;
use paste::paste;

macro_rules! with_builder_methods {
    ($name:ident, $attribute:ident, bool) => {
        paste! {
            pub fn [<use_ $name>](mut self, value: bool) -> Self {
                self.$attribute = value;
                self
            }

            pub fn $name(&self) -> bool {
                self.$attribute
            }
        }
    };
    ($name:ident, $attribute:ident, $ty:ty) => {
        paste! {
            pub fn [<with_ $name>](mut self, value: $ty) -> Self {
                self.$attribute = Some(value);
                self
            }

            pub fn [<without_ $name>](mut self) -> Self {
                self.$attribute = None;
                self
            }

            pub fn $name(&self) -> Option<&$ty> {
                (&self.$attribute).as_ref()
            }
        }
    };
    ($name:ident, $attribute:ident, generic, $interface:ty) => {
        paste! {
            pub fn [<with_ $name>]<T: interface + 'static>(mut self, value: T) -> Self {
                self.$attribute = Some(Box::new(value));
                self
            }

            pub fn [<without_ $name>](mut self) -> Self {
                self.$attribute = None;
                self
            }

            pub fn $name(&self) -> Option<&dyn $interface> {
                (&self.$attribute).as_ref()
            }
        }
    };
}

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
        }
    }

    pub fn with_history<H: History + 'static>(mut self, history: H) -> Self {
        self.history = Some(Box::new(history));
        self
    }

    pub fn with_hints<H: Hinter + 'static>(mut self, hinter: H) -> Self {
        self.hinter = Some(Box::new(hinter));
        self
    }

    pub fn with_completion<C: Completer + 'static>(mut self, completer: C) -> Self {
        self.completer = Some(Box::new(completer));
        self
    }

    pub fn with_highlighter<H: Highlighter + 'static>(mut self, highlighter: H) -> Self {
        self.highlighter = Some(Box::new(highlighter));
        self
    }

    pub fn with_validator<V: Validator + 'static>(mut self, validator: V) -> Self {
        self.validator = Some(Box::new(validator));
        self
    }

    pub fn with_transient_prompt<P: Prompt + 'static>(mut self, prompt: P) -> Self {
        self.transient_prompt = Some(Box::new(prompt));
        self
    }

    pub fn with_edit_mode<E: EditMode + 'static>(mut self, mode: E) -> Self {
        self.edit_mode = Some(Box::new(mode));
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

    with_builder_methods!(history_exclusion_prefix, history_exclusion_prefix, String);

    with_builder_methods!(selection_style, visual_selection_style, Style);

    with_builder_methods!(cursor_config, cursor_shapes, CursorConfig);

    with_builder_methods!(history_session_id, history_session_id, HistorySessionId);

    #[cfg(feature = "external_printer")]
    with_builder_methods!(external_printer, external_printer, ExternalPrinter<String>);

    with_builder_methods!(quick_completions, quick_completions, bool);

    with_builder_methods!(partial_completions, partial_completions, bool);

    with_builder_methods!(bracketed_paste, bracketed_paste, bool);

    with_builder_methods!(kitty_keyboard_enhancement, kitty_protocol, bool);

    with_builder_methods!(ansi_colors, use_ansi_coloring, bool);

    pub fn add_menu(mut self, menu: ReedlineMenu) -> Self {
        self.menus.push(menu);
        self
    }

    // `menus` cannot accept a slice because ReedlineMenu is not Clone
    pub fn add_menus(mut self, menus: Vec<ReedlineMenu>) -> Self {
        self.menus.extend(menus);
        self
    }
}
