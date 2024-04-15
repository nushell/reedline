use std::{path::PathBuf, process};

use nu_ansi_term::Style;

use crate::*;

impl super::Reedline {
    /// Set the [`History`](crate::History) of a constructed engine.
    /// Prefer [`ReedlineBuilder::with_history`](crate::engine::builder::ReedlineBuilder::with_history)
    /// if you don't need to change this while using reedline.
    pub fn with_history(&mut self, history: Box<dyn History>) -> &mut Self {
        self.history = history;
        self
    }

    /// Set the [`Hinter`](crate::Hinter) of a constructed engine.
    /// Prefer [`ReedlineBuilder::with_hints`](crate::engine::builder::ReedlineBuilder::with_hints)
    /// if you don't need to change this while using reedline.
    pub fn with_hints(&mut self, hints: Option<Box<dyn Hinter>>) -> &mut Self {
        self.hinter = hints;
        self
    }

    /// Set the [`Completer`](crate::Completer) of a constructed engine.
    /// Prefer [`ReedlineBuilder::with_completions`](crate::engine::builder::ReedlineBuilder::with_completions)
    /// if you don't need to change this while using reedline.
    pub fn with_completions(&mut self, completions: Box<dyn Completer>) -> &mut Self {
        self.completer = completions;
        self
    }

    /// Set the [`Highlighter`](crate::Highlighter) of a constructed engine.
    /// Prefer [`ReedlineBuilder::with_highlighter`](crate::engine::builder::ReedlineBuilder::with_highlighter)
    /// if you don't need to change this while using reedline.
    pub fn with_highlighter(&mut self, highlighter: Box<dyn Highlighter>) -> &mut Self {
        self.highlighter = highlighter;
        self
    }

    /// Set the [`Validator`](crate::Validator) of a constructed engine.
    /// Prefer [`ReedlineBuilder::with_validator`](crate::engine::builder::ReedlineBuilder::with_validator)
    /// if you don't need to change this while using reedline.
    pub fn with_validator(&mut self, validator: Option<Box<dyn Validator>>) -> &mut Self {
        self.validator = validator;
        self
    }

    /// Use a [`Prompt`](crate::Prompt) as the the transient prompt of a constructed engine.
    /// Prefer [`ReedlineBuilder::with_transient_prompt`](crate::engine::builder::ReedlineBuilder::with_transient_prompt)
    /// if you don't need to change this while using reedline.
    pub fn with_transient_prompt(&mut self, prompt: Option<Box<dyn Prompt>>) -> &mut Self {
        self.transient_prompt = prompt;
        self
    }

    /// Set the edit mode of a constructed engine.
    /// Prefer [`ReedlineBuilder::with_edit_mode`](crate::engine::builder::ReedlineBuilder::with_edit_mode)
    /// if you don't need to change this while using reedline.
    pub fn with_edit_mode(&mut self, edit_mode: Box<dyn EditMode>) -> &mut Self {
        self.edit_mode = edit_mode;
        self
    }

    /// Set the history exclusion prefix of a construncted engine.
    /// Prefer [`ReedlineBuilder::with_history_exclusion_prefix`](crate::engine::builder::ReedlineBuilder::with_history_exclusion_prefix)
    /// if you don't need to change this while using reedline.
    pub fn with_history_exclusion_prefix(&mut self, ignore_prefix: Option<String>) -> &mut Self {
        self.history_exclusion_prefix = ignore_prefix;
        self
    }

    /// Set the visual selection style of a construncted engine.
    /// Prefer [`ReedlineBuilder::with_selection_style`](crate::engine::builder::ReedlineBuilder::with_selection_style)
    /// if you don't need to change this while using reedline.
    pub fn with_selection_style(&mut self, selection_style: Style) -> &mut Self {
        self.visual_selection_style = selection_style;
        self
    }

    /// Configure the cursor shapes depending on the edit mode.
    /// Prefer [`ReedlineBuilder::with_cursor_config`](crate::engine::builder::ReedlineBuilder::with_cursor_config)
    /// if you don't need to change this while using reedline.
    pub fn with_cursor_config(&mut self, cursor_config: Option<CursorConfig>) -> &mut Self {
        self.cursor_shapes = cursor_config;
        self
    }

    /// Set the history session id.
    /// Prefer [`ReedlineBuilder::with_history_session_id`](crate::engine::builder::ReedlineBuilder::with_history_session_id)
    /// if you don't need to change this while using reedline.
    pub fn with_history_session_id(&mut self, session: Option<HistorySessionId>) {
        self.history_session_id = session;
    }

    /// The history session id, or [`None`](Option::None) if no session is attached.
    pub fn history_session_id(&self) -> Option<HistorySessionId> {
        self.history_session_id.clone()
    }

    /// Set whether to use quick completions.
    /// Prefer [`ReedlineBuilder::use_quick_completions`](crate::engine::builder::ReedlineBuilder::use_quick_completions)
    /// if you don't need to change this while using reedline.
    pub fn use_quick_completions(&mut self, enabled: bool) -> &mut Self {
        self.quick_completions = enabled;
        self
    }

    /// Set whether to use partial completions.
    /// Prefer [`ReedlineBuilder::use_partial_completions`](crate::engine::builder::ReedlineBuilder::use_partial_completions)
    /// if you don't need to change this while using reedline.
    pub fn use_partial_completions(&mut self, enabled: bool) -> &mut Self {
        self.partial_completions = enabled;
        self
    }

    /// Set whether to use bracketed paste.
    /// Prefer [`ReedlineBuilder::use_bracketed_paste`](crate::engine::builder::ReedlineBuilder::use_bracketed_paste)
    /// if you don't need to change this while using reedline.
    pub fn use_bracketed_paste(&mut self, enabled: bool) -> &mut Self {
        self.bracketed_paste.set(enabled);
        self
    }

    /// Set whether to use the enhanced keyboard protocol.
    /// Prefer [`ReedlineBuilder::use_kitty_keyboard_enhancement`](crate::engine::builder::ReedlineBuilder::use_kitty_keyboard_enhancement)
    /// if you don't need to change this while using reedline.
    pub fn use_kitty_keyboard_enhancement(&mut self, enabled: bool) -> &mut Self {
        self.kitty_protocol.set(enabled);
        self
    }

    /// Set whether ANSI escape sequences should be used to provide colored terminal output.
    /// Prefer [`ReedlineBuilder::use_ansi_colors`](crate::engine::builder::ReedlineBuilder::use_ansi_colors)
    /// if you don't need to change this while using reedline.
    pub fn use_ansi_colors(&mut self, enabled: bool) -> &mut Self {
        self.use_ansi_coloring = enabled;
        self
    }

    /// Add a menu.
    /// Prefer [`ReedlineBuilder::add_menu`](crate::engine::builder::ReedlineBuilder::add_menu)
    /// if you don't need to add a menu while using reedline.
    pub fn add_menu(&mut self, menu: ReedlineMenu) -> &mut Self {
        self.menus.push(menu);
        self
    }

    /// Allow the line buffer to be edited through a ephemeral file at the given path with the specified editor.
    /// Prefer [`ReedlineBuilder::with_buffer_editor`](crate::engine::builder::ReedlineBuilder::with_buffer_editor)
    /// if you don't need to change this while using reedline.
    pub fn with_buffer_editor(
        &mut self,
        editor: process::Command,
        temp_file: PathBuf,
    ) -> &mut Self {
        let mut editor = editor;
        if !editor.get_args().any(|arg| arg == temp_file.as_os_str()) {
            editor.arg(&temp_file);
        }
        self.buffer_editor = Some(super::BufferEditor {
            command: editor,
            temp_file,
        });
        self
    }
}
