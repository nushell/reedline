use crate::text_manipulation;

use {
    crate::{
        clip_buffer::{get_default_clipboard, Clipboard},
        completer::{ComplationActionHandler, DefaultCompletionActionHandler},
        default_emacs_keybindings,
        hinter::{DefaultHinter, Hinter},
        history::{FileBackedHistory, History, HistoryNavigationQuery},
        keybindings::{default_vi_insert_keybindings, default_vi_normal_keybindings, Keybindings},
        line_buffer::LineBuffer,
        painter::Painter,
        prompt::{PromptEditMode, PromptHistorySearch, PromptHistorySearchStatus, PromptViMode},
        DefaultHighlighter, EditCommand, EditMode, Highlighter, Prompt, Signal, ViEngine,
    },
    crossterm::{
        cursor::position,
        event::{poll, read, Event, KeyCode, KeyEvent, KeyModifiers},
        terminal, Result,
    },
    std::{collections::HashMap, io::stdout, time::Duration},
};

#[derive(Debug, PartialEq, Eq)]
enum InputMode {
    Regular,
    HistorySearch,
    HistoryTraversal,
}

/// Line editor engine
///
/// ## Example usage
/// ```no_run
/// use reedline::{Reedline, Signal, DefaultPrompt};
/// let mut line_editor = Reedline::new();
/// let prompt = DefaultPrompt::default();
///
/// let out = line_editor.read_line(&prompt).unwrap();
/// match out {
///    Signal::Success(content) => {
///        // process content
///    }
///    _ => {
///        eprintln!("Entry aborted!");
///    }
/// }
/// ```
pub struct Reedline {
    line_buffer: LineBuffer,

    // Cut buffer
    cut_buffer: Box<dyn Clipboard>,

    // History
    history: Box<dyn History>,
    input_mode: InputMode,

    // Stdout
    painter: Painter,

    // Keybindings
    keybindings: HashMap<EditMode, Keybindings>,

    // Edit mode
    edit_mode: EditMode,

    // Dirty bits
    need_full_repaint: bool,

    // Partial command
    //partial_command: Option<char>,

    // Vi normal mode state engine
    vi_engine: ViEngine,

    tab_handler: Box<dyn ComplationActionHandler>,
}

impl Default for Reedline {
    fn default() -> Self {
        Self::new()
    }
}

impl Reedline {
    /// Create a new [`Reedline`] engine with a local [`History`] that is not synchronized to a file.
    pub fn new() -> Reedline {
        let history = Box::new(FileBackedHistory::default());
        let cut_buffer = Box::new(get_default_clipboard());
        let buffer_highlighter = Box::new(DefaultHighlighter::default());
        let hinter = Box::new(DefaultHinter::default());
        let painter = Painter::new(stdout(), buffer_highlighter, hinter);
        let mut keybindings_hashmap = HashMap::new();
        keybindings_hashmap.insert(EditMode::Emacs, default_emacs_keybindings());
        keybindings_hashmap.insert(EditMode::ViInsert, default_vi_insert_keybindings());
        keybindings_hashmap.insert(EditMode::ViNormal, default_vi_normal_keybindings());

        Reedline {
            line_buffer: LineBuffer::new(),
            cut_buffer,
            history,
            input_mode: InputMode::Regular,
            painter,
            keybindings: keybindings_hashmap,
            edit_mode: EditMode::Emacs,
            need_full_repaint: false,
            //partial_command: None,
            vi_engine: ViEngine::new(),
            tab_handler: Box::new(DefaultCompletionActionHandler::default()),
        }
    }

    /// A builder to include the hinter in your instance of the Reedline engine
    /// # Example
    /// ```rust,no_run
    /// //Cargo.toml
    /// //[dependencies]
    /// //nu-ansi-term = "*"
    /// use {
    ///     nu_ansi_term::{Color, Style},
    ///     reedline::{DefaultCompleter, DefaultHinter, Reedline},
    /// };
    ///
    /// let commands = vec![
    ///     "test".into(),
    ///     "hello world".into(),
    ///     "hello world reedline".into(),
    ///     "this is the reedline crate".into(),
    /// ];
    /// let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));
    ///
    /// let mut line_editor = Reedline::new().with_hinter(Box::new(
    ///     DefaultHinter::default()
    ///     .with_completer(completer) // or .with_history()
    ///     // .with_inside_line()
    ///     .with_style(Style::new().italic().fg(Color::LightGray)),
    /// ));
    /// ```
    pub fn with_hinter(mut self, hinter: Box<dyn Hinter>) -> Reedline {
        self.painter.set_hinter(hinter);
        self
    }

    /// A builder to configure the completion action handler to use in your instance of the reedline engine
    /// # Example
    /// ```rust,no_run
    /// // Create a reedline object with tab completions support
    ///
    /// use reedline::{DefaultCompleter, DefaultCompletionActionHandler, Reedline};
    ///
    /// let commands = vec![
    ///   "test".into(),
    ///   "hello world".into(),
    ///   "hello world reedline".into(),
    ///   "this is the reedline crate".into(),
    /// ];
    /// let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));
    ///
    /// let mut line_editor = Reedline::new().with_completion_action_handler(Box::new(
    ///   DefaultCompletionActionHandler::default().with_completer(completer),
    /// ));
    /// ```
    pub fn with_completion_action_handler(
        mut self,
        tab_handler: Box<dyn ComplationActionHandler>,
    ) -> Reedline {
        self.tab_handler = tab_handler;
        self
    }

    /// A builder that configures the highlighter for your instance of the Reedline engine
    /// # Example
    /// ```rust,no_run
    /// // Create a reedline object with highlighter support
    ///
    /// use reedline::{DefaultHighlighter, Reedline};
    ///
    /// let commands = vec![
    ///   "test".into(),
    ///   "hello world".into(),
    ///   "hello world reedline".into(),
    ///   "this is the reedline crate".into(),
    /// ];
    /// let mut line_editor =
    /// Reedline::new().with_highlighter(Box::new(DefaultHighlighter::new(commands)));
    /// ```
    pub fn with_highlighter(mut self, highlighter: Box<dyn Highlighter>) -> Reedline {
        self.painter.set_highlighter(highlighter);
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
    /// let mut line_editor = Reedline::new()
    ///     .with_history(history)
    ///     .expect("Error configuring reedline with history");
    /// ```
    pub fn with_history(mut self, history: Box<dyn History>) -> std::io::Result<Reedline> {
        self.history = history;

        Ok(self)
    }

    /// A builder which configures the keybindings for your instance of the Reedline engine
    pub fn with_keybindings(mut self, keybindings: Keybindings) -> Reedline {
        self.keybindings.insert(EditMode::Emacs, keybindings);

        self
    }

    /// A builder which configures the edit mode for your instance of the Reedline engine
    pub fn with_edit_mode(mut self, edit_mode: EditMode) -> Reedline {
        self.edit_mode = edit_mode;

        self
    }

    /// Gets the current keybindings for Emacs mode
    pub fn get_keybindings(&self) -> &Keybindings {
        self.keybindings
            .get(&EditMode::Emacs)
            .expect("Internal error: emacs should always be supported")
    }

    /// Sets the keybindings to the given keybindings
    /// Note: keybindings are set on the emacs mode. The vi mode is not configurable
    pub fn update_keybindings(&mut self, keybindings: Keybindings) {
        self.keybindings.insert(EditMode::Emacs, keybindings);
    }

    /// Get the current edit mode
    pub fn edit_mode(&self) -> EditMode {
        self.edit_mode
    }

    /// Returns the corresponding expected prompt style for the given edit mode
    pub fn prompt_edit_mode(&self) -> PromptEditMode {
        match self.edit_mode {
            EditMode::ViInsert => PromptEditMode::Vi(PromptViMode::Insert),
            EditMode::ViNormal => PromptEditMode::Vi(PromptViMode::Normal),
            EditMode::Emacs => PromptEditMode::Emacs,
        }
    }

    fn find_keybinding(
        &self,
        modifier: KeyModifiers,
        key_code: KeyCode,
    ) -> Option<Vec<EditCommand>> {
        self.keybindings
            .get(&self.edit_mode)
            .expect("Internal error: expected to find keybindings for edit mode")
            .find_binding(modifier, key_code)
    }

    /// Output the complete [`History`] chronologically with numbering to the terminal
    pub fn print_history(&mut self) -> Result<()> {
        let history: Vec<_> = self
            .history
            .iter_chronologic()
            .cloned()
            .enumerate()
            .collect();

        for (i, entry) in history {
            self.print_line(&format!("{}\t{}", i + 1, entry))?;
        }
        Ok(())
    }

    /// Wait for input and provide the user with a specified [`Prompt`].
    ///
    /// Returns a [`crossterm::Result`] in which the `Err` type is [`crossterm::ErrorKind`]
    /// to distinguish I/O errors and the `Ok` variant wraps a [`Signal`] which
    /// handles user inputs.
    pub fn read_line(&mut self, prompt: &dyn Prompt) -> Result<Signal> {
        terminal::enable_raw_mode()?;

        let result = self.read_line_helper(prompt);

        terminal::disable_raw_mode()?;

        result
    }

    /// Writes `msg` to the terminal with a following carriage return and newline
    pub fn print_line(&mut self, msg: &str) -> Result<()> {
        self.painter.paint_line(msg)
    }

    /// Goes to the beginning of the next line
    ///
    /// Also works in raw mode
    pub fn print_crlf(&mut self) -> Result<()> {
        self.painter.paint_crlf()
    }

    /// Dispatches the applicable [`EditCommand`] actions for editing the history search string.
    ///
    /// Only modifies internal state, does not perform regular output!
    fn run_history_commands(&mut self, commands: &[EditCommand]) {
        for command in commands {
            match command {
                EditCommand::InsertChar(c) => {
                    let navigation = self.history.get_navigation();
                    if let HistoryNavigationQuery::SubstringSearch(mut substring) = navigation {
                        substring.push(*c);
                        self.history
                            .set_navigation(HistoryNavigationQuery::SubstringSearch(substring));
                    } else {
                        self.history
                            .set_navigation(HistoryNavigationQuery::SubstringSearch(String::from(
                                *c,
                            )))
                    }
                    self.history.back();
                }
                EditCommand::Backspace => {
                    let navigation = self.history.get_navigation();

                    if let HistoryNavigationQuery::SubstringSearch(substring) = navigation {
                        let new_substring = text_manipulation::remove_last_grapheme(&substring);

                        self.history
                            .set_navigation(HistoryNavigationQuery::SubstringSearch(
                                new_substring.to_string(),
                            ));
                        self.history.back()
                    }
                }
                EditCommand::SearchHistory | EditCommand::Up | EditCommand::PreviousHistory => {
                    self.history.back();
                }
                EditCommand::Down | EditCommand::NextHistory => {
                    self.history.forward();
                    // Hacky way to ensure that we don't fall of into failed search going forward
                    if self.history.string_at_cursor().is_none() {
                        self.history.back();
                    }
                }
                _ => {
                    self.input_mode = InputMode::Regular;
                }
            }
        }
    }

    fn move_to_start(&mut self) {
        self.line_buffer.move_to_start()
    }

    fn move_to_end(&mut self) {
        self.line_buffer.move_to_end()
    }

    fn move_left(&mut self) {
        self.line_buffer.move_left()
    }

    fn move_right(&mut self) {
        self.line_buffer.move_right()
    }

    fn move_word_left(&mut self) {
        self.line_buffer.move_word_left();
    }

    fn move_word_right(&mut self) {
        self.line_buffer.move_word_right();
    }

    fn insert_char(&mut self, c: char) {
        self.line_buffer.insert_char(c)
    }

    fn backspace(&mut self) {
        self.line_buffer.delete_left_grapheme();
    }

    fn delete(&mut self) {
        self.line_buffer.delete_right_grapheme();
    }

    fn backspace_word(&mut self) {
        self.line_buffer.delete_word_left();
    }

    fn delete_word(&mut self) {
        self.line_buffer.delete_word_right();
    }

    fn clear(&mut self) {
        self.line_buffer.clear();
    }

    fn up_command(&mut self) {
        // If we're at the top, then:
        if !self.line_buffer.get_buffer()[0..self.line_buffer.offset()].contains('\n') {
            // If we're at the top, move to previous history
            self.previous_history();
        } else {
            // If we're not at the top, move up a line in the multiline buffer
            let mut position = self.line_buffer.offset();
            let mut num_of_move_lefts = 0;
            let buffer = self.line_buffer.get_buffer().to_string();

            // Move left until we're looking at the newline
            // Observe what column we were on
            while position > 0 && &buffer[(position - 1)..position] != "\n" {
                self.line_buffer.move_left();
                num_of_move_lefts += 1;
                position = self.line_buffer.offset();
            }

            // Find start of previous line
            let mut matches = buffer[0..(position - 1)].rmatch_indices('\n');

            if let Some((pos, _)) = matches.next() {
                position = pos + 1;
            } else {
                position = 0;
            }
            self.set_offset(position);

            // Move right from this position to the column we were at
            while &buffer[position..(position + 1)] != "\n" && num_of_move_lefts > 0 {
                self.line_buffer.move_right();
                position = self.line_buffer.offset();
                num_of_move_lefts -= 1;
            }
        }
    }

    fn down_command(&mut self) {
        // If we're at the top, then:
        if !self.line_buffer.get_buffer()[self.line_buffer.offset()..].contains('\n') {
            // If we're at the top, move to previous history
            self.next_history();
        } else {
            // If we're not at the top, move up a line in the multiline buffer
            let mut position = self.line_buffer.offset();
            let mut num_of_move_lefts = 0;
            let buffer = self.line_buffer.get_buffer().to_string();

            // Move left until we're looking at the newline
            // Observe what column we were on
            while position > 0 && &buffer[(position - 1)..position] != "\n" {
                self.line_buffer.move_left();
                num_of_move_lefts += 1;
                position = self.line_buffer.offset();
            }

            // Find start of next line
            let mut matches = buffer[position..].match_indices('\n');

            // Assume this always succeeds

            let (pos, _) = matches
                .next()
                .expect("internal error: should have found newline");

            position += pos + 1;

            self.set_offset(position);

            // Move right from this position to the column we were at
            while &buffer[position..(position + 1)] != "\n" && num_of_move_lefts > 0 {
                self.line_buffer.move_right();
                position = self.line_buffer.offset();
                num_of_move_lefts -= 1;
            }
        }
    }

    fn append_to_history(&mut self) {
        self.history.append(self.insertion_line().to_string());
    }

    fn previous_history(&mut self) {
        if self.input_mode != InputMode::HistoryTraversal {
            self.input_mode = InputMode::HistoryTraversal;
            self.set_history_navigation_based_on_line_buffer();
        }

        self.history.back();
        self.update_buffer_from_history();
    }

    fn next_history(&mut self) {
        if self.input_mode != InputMode::HistoryTraversal {
            self.input_mode = InputMode::HistoryTraversal;
            self.set_history_navigation_based_on_line_buffer();
        }

        self.history.forward();
        self.update_buffer_from_history();
    }

    fn set_history_navigation_based_on_line_buffer(&mut self) {
        if self.line_buffer.is_empty()
            || self.line_buffer.offset() != self.line_buffer.get_buffer().len()
        {
            self.history
                .set_navigation(HistoryNavigationQuery::Normal(self.line_buffer.clone()));
        } else {
            let buffer = self.insertion_line().to_string();
            self.history
                .set_navigation(HistoryNavigationQuery::PrefixSearch(buffer));
        }
    }

    fn search_history(&mut self) {
        self.input_mode = InputMode::HistorySearch;
        self.history
            .set_navigation(HistoryNavigationQuery::SubstringSearch("".to_string()));
    }

    fn cut_from_start(&mut self) {
        let insertion_offset = self.line_buffer.offset();
        if insertion_offset > 0 {
            self.cut_buffer
                .set(&self.line_buffer.get_buffer()[..insertion_offset]);
            self.clear_to_insertion_point();
        }
    }

    fn cut_from_end(&mut self) {
        let cut_slice = &self.line_buffer.get_buffer()[self.line_buffer.offset()..];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice);
            self.clear_to_end();
        }
    }

    fn cut_word_left(&mut self) {
        let insertion_offset = self.line_buffer.offset();
        let left_index = self.line_buffer.word_left_index();
        if left_index < insertion_offset {
            let cut_range = left_index..insertion_offset;
            self.cut_buffer
                .set(&self.line_buffer.get_buffer()[cut_range.clone()]);
            self.clear_range(cut_range);
            self.set_offset(left_index);
        }
    }

    fn cut_word_right(&mut self) {
        let insertion_offset = self.line_buffer.offset();
        let right_index = self.line_buffer.word_right_index();
        if right_index > insertion_offset {
            let cut_range = insertion_offset..right_index;
            self.cut_buffer
                .set(&self.line_buffer.get_buffer()[cut_range.clone()]);
            self.clear_range(cut_range);
        }
    }

    fn insert_cut_buffer(&mut self) {
        let cut_buffer = self.cut_buffer.get();
        self.line_buffer.insert_str(&cut_buffer);
    }

    fn uppercase_word(&mut self) {
        self.line_buffer.uppercase_word();
    }

    fn lowercase_word(&mut self) {
        self.line_buffer.lowercase_word();
    }

    fn capitalize_char(&mut self) {
        self.line_buffer.capitalize_char();
    }

    fn swap_words(&mut self) {
        self.line_buffer.swap_words();
    }

    fn swap_graphemes(&mut self) {
        self.line_buffer.swap_graphemes();
    }

    fn enter_vi_insert_mode(&mut self) {
        self.edit_mode = EditMode::ViInsert;
        self.need_full_repaint = true;
        //self.partial_command = None;
    }

    fn enter_vi_normal_mode(&mut self) {
        self.edit_mode = EditMode::ViNormal;
        self.need_full_repaint = true;
        //self.partial_command = None;
    }

    /// Executes [`EditCommand`] actions by modifying the internal state appropriately. Does not output itself.
    fn run_edit_commands(&mut self, commands: &[EditCommand]) {
        // Handle command for history inputs
        if self.input_mode == InputMode::HistorySearch {
            self.run_history_commands(commands);
            return;
        }

        if self.input_mode == InputMode::HistoryTraversal {
            for command in commands {
                match command {
                    EditCommand::Up
                    | EditCommand::Down
                    | EditCommand::NextHistory
                    | EditCommand::PreviousHistory => {}
                    _ => {
                        if matches!(
                            self.history.get_navigation(),
                            HistoryNavigationQuery::Normal(_)
                        ) {
                            if let Some(string) = self.history.string_at_cursor() {
                                self.set_buffer(string)
                            }
                        }
                        self.input_mode = InputMode::Regular;
                    }
                }
            }
        }

        // Vim mode transformations
        let commands = match self.edit_mode {
            EditMode::ViNormal => self.vi_engine.handle(commands),
            _ => commands.into(),
        };

        // Run the commands over the edit buffer
        for command in &commands {
            match command {
                EditCommand::MoveToStart => self.move_to_start(),
                EditCommand::MoveToEnd => self.move_to_end(),
                EditCommand::MoveLeft => self.move_left(),
                EditCommand::MoveRight => self.move_right(),
                EditCommand::MoveWordLeft => self.move_word_left(),
                EditCommand::MoveWordRight => self.move_word_right(),
                EditCommand::InsertChar(c) => self.insert_char(*c),
                EditCommand::Backspace => self.backspace(),
                EditCommand::Delete => self.delete(),
                EditCommand::BackspaceWord => self.backspace_word(),
                EditCommand::DeleteWord => self.delete_word(),
                EditCommand::Clear => self.clear(),
                EditCommand::AppendToHistory => self.append_to_history(),
                EditCommand::PreviousHistory => self.previous_history(),
                EditCommand::NextHistory => self.next_history(),
                EditCommand::Up => self.up_command(),
                EditCommand::Down => self.down_command(),
                EditCommand::SearchHistory => self.search_history(),
                EditCommand::CutFromStart => self.cut_from_start(),
                EditCommand::CutToEnd => self.cut_from_end(),
                EditCommand::CutWordLeft => self.cut_word_left(),
                EditCommand::CutWordRight => self.cut_word_right(),
                EditCommand::PasteCutBuffer => self.insert_cut_buffer(),
                EditCommand::UppercaseWord => self.uppercase_word(),
                EditCommand::LowercaseWord => self.lowercase_word(),
                EditCommand::CapitalizeChar => self.capitalize_char(),
                EditCommand::SwapWords => self.swap_words(),
                EditCommand::SwapGraphemes => self.swap_graphemes(),
                EditCommand::EnterViInsert => self.enter_vi_insert_mode(),
                EditCommand::EnterViNormal => self.enter_vi_normal_mode(),
                EditCommand::Undo => {
                    self.line_buffer.undo();
                }
                EditCommand::Redo => {
                    self.line_buffer.redo();
                }
                _ => {}
            }

            if [
                EditCommand::MoveToEnd,
                EditCommand::MoveToStart,
                EditCommand::MoveLeft,
                EditCommand::MoveRight,
                EditCommand::MoveWordLeft,
                EditCommand::MoveWordRight,
                EditCommand::Backspace,
                EditCommand::Delete,
                EditCommand::BackspaceWord,
                EditCommand::DeleteWord,
                EditCommand::CutFromStart,
                EditCommand::CutToEnd,
                EditCommand::CutWordLeft,
                EditCommand::CutWordRight,
            ]
            .contains(command)
            {
                self.line_buffer.set_previous_lines(true);
            }
        }
    }

    /// Set the cursor position as understood by the underlying [`LineBuffer`] for the current line
    fn set_offset(&mut self, pos: usize) {
        self.line_buffer
            .set_insertion_point(self.line_buffer.line(), pos)
    }

    /// Get the current line of a multi-line edit [`LineBuffer`]
    fn insertion_line(&self) -> &str {
        self.line_buffer.get_buffer()
    }

    /// Reset the [`LineBuffer`] to be a line specified by `buffer`
    fn set_buffer(&mut self, buffer: String) {
        self.line_buffer.set_buffer(buffer)
    }

    fn clear_to_end(&mut self) {
        self.line_buffer.clear_to_end()
    }

    fn clear_to_insertion_point(&mut self) {
        self.line_buffer.clear_to_insertion_point()
    }

    fn clear_range<R>(&mut self, range: R)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.line_buffer.clear_range(range)
    }

    /// Heuristic to predetermine if we need to poll the terminal if the text wrapped around.
    fn maybe_wrap(&self, terminal_width: u16, start_offset: u16, c: char) -> bool {
        use unicode_width::UnicodeWidthStr;

        let mut test_buffer = self.insertion_line().to_string();
        test_buffer.push(c);

        let display_width = UnicodeWidthStr::width(test_buffer.as_str()) + start_offset as usize;

        display_width >= terminal_width as usize
    }

    /// Clear the screen by printing enough whitespace to start the prompt or
    /// other output back at the first line of the terminal.
    pub fn clear_screen(&mut self) -> Result<()> {
        self.painter.clear_screen()?;

        Ok(())
    }

    /// Display only the prompt components preceding the buffer
    ///
    /// Used to restore the prompt indicator after a search etc. that affected
    /// the prompt
    fn queue_prompt_indicator(&mut self, prompt: &dyn Prompt) -> Result<()> {
        // print our prompt
        let prompt_mode = self.prompt_edit_mode();
        self.painter.queue_prompt_indicator(prompt, prompt_mode)?;

        Ok(())
    }

    /// Repaint logic for the normal input prompt buffer
    ///
    /// Requires coordinates where the input buffer begins after the prompt.
    fn buffer_paint(&mut self, prompt_offset: (u16, u16)) -> Result<()> {
        let cursor_position_in_buffer = self.line_buffer.offset();
        let buffer_to_paint = self.insertion_line().to_string();

        self.painter.queue_buffer(
            buffer_to_paint,
            prompt_offset,
            cursor_position_in_buffer,
            self.history.as_ref(),
        )?;
        self.painter.flush()?;

        Ok(())
    }

    fn full_repaint(
        &mut self,
        prompt: &dyn Prompt,
        prompt_origin: (u16, u16),
        terminal_size: (u16, u16),
    ) -> Result<(u16, u16)> {
        let prompt_mode = self.prompt_edit_mode();
        let buffer_to_paint = self.insertion_line().to_string();

        let cursor_position_in_buffer = self.line_buffer.offset();

        self.painter.repaint_everything(
            prompt,
            prompt_mode,
            prompt_origin,
            cursor_position_in_buffer,
            buffer_to_paint,
            terminal_size,
            self.history.as_ref(),
        )

        // Ok(prompt_offset)
    }

    /// Repaint logic for the history reverse search
    ///
    /// Overwrites the prompt indicator and highlights the search string
    /// separately from the result bufer.
    fn history_search_paint(&mut self, prompt: &dyn Prompt) -> Result<()> {
        let navigation = self.history.get_navigation();

        if let HistoryNavigationQuery::SubstringSearch(substring) = navigation {
            let status = if !substring.is_empty() && self.history.string_at_cursor().is_none() {
                PromptHistorySearchStatus::Failing
            } else {
                PromptHistorySearchStatus::Passing
            };

            let prompt_history_search = PromptHistorySearch::new(status, substring);

            self.painter
                .queue_history_search_indicator(prompt, prompt_history_search)?;

            match self.history.string_at_cursor() {
                Some(string) => {
                    self.painter
                        .queue_history_search_result(&string, string.len())?;
                    self.painter.flush()?;
                }

                None => {
                    self.painter.clear_until_newline()?;
                }
            }
        }

        Ok(())
    }

    fn update_buffer_from_history(&mut self) {
        match self.history.get_navigation() {
            HistoryNavigationQuery::Normal(original) => {
                if let Some(buffer_to_paint) = self.history.string_at_cursor() {
                    self.line_buffer.set_buffer(buffer_to_paint.clone());
                    self.set_offset(buffer_to_paint.len())
                } else {
                    self.line_buffer = original
                }
            }
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                if let Some(prefix_result) = self.history.string_at_cursor() {
                    self.line_buffer.set_buffer(prefix_result.clone());
                    self.set_offset(prefix_result.len());
                } else {
                    self.line_buffer.set_buffer(prefix.clone());
                    self.set_offset(prefix.len());
                }
            }
            HistoryNavigationQuery::SubstringSearch(_) => todo!(),
        }
    }

    /// Helper implemting the logic for [`Reedline::read_line()`] to be wrapped
    /// in a `raw_mode` context.
    fn read_line_helper(&mut self, prompt: &dyn Prompt) -> Result<Signal> {
        let mut terminal_size = terminal::size()?;

        let mut prompt_origin = {
            let (column, row) = position()?;
            if (column, row) == (0, 0) {
                (0, 0)
            } else if row + 1 == terminal_size.1 {
                self.painter.paint_carriage_return()?;
                (0, row.saturating_sub(1))
            } else if row + 2 == terminal_size.1 {
                self.painter.paint_carriage_return()?;
                (0, row)
            } else {
                (0, row + 1)
            }
        };

        // set where the input begins
        let mut prompt_offset = self.full_repaint(prompt, prompt_origin, terminal_size)?;

        // Redraw if Ctrl-L was used
        if self.input_mode == InputMode::HistorySearch {
            self.history_search_paint(prompt)?;
        }

        loop {
            if poll(Duration::from_secs(1))? {
                match read()? {
                    Event::Key(KeyEvent { code, modifiers }) => {
                        match (modifiers, code, self.edit_mode) {
                            (KeyModifiers::NONE, KeyCode::Tab, _) => {
                                self.tab_handler.handle(&mut self.line_buffer);
                            }
                            (KeyModifiers::CONTROL, KeyCode::Char('d'), _) => {
                                self.tab_handler.reset_index();
                                if self.line_buffer.is_empty() {
                                    self.line_buffer.reset_olds();
                                    return Ok(Signal::CtrlD);
                                } else if let Some(binding) = self.find_keybinding(modifiers, code)
                                {
                                    self.run_edit_commands(&binding);
                                }
                            }
                            (KeyModifiers::CONTROL, KeyCode::Char('c'), _) => {
                                self.tab_handler.reset_index();
                                if let Some(binding) = self.find_keybinding(modifiers, code) {
                                    self.run_edit_commands(&binding);
                                }
                                self.line_buffer.reset_olds();
                                return Ok(Signal::CtrlC);
                            }
                            (KeyModifiers::CONTROL, KeyCode::Char('l'), EditMode::Emacs) => {
                                self.tab_handler.reset_index();
                                self.line_buffer.reset_olds();
                                return Ok(Signal::CtrlL);
                            }
                            (KeyModifiers::NONE, KeyCode::Char(c), x)
                            | (KeyModifiers::SHIFT, KeyCode::Char(c), x)
                                if x == EditMode::ViNormal =>
                            {
                                self.tab_handler.reset_index();
                                self.run_edit_commands(&[EditCommand::ViCommandFragment(c)]);
                                self.line_buffer.set_previous_lines(false);
                            }
                            (KeyModifiers::NONE, KeyCode::Char(c), x)
                            | (KeyModifiers::SHIFT, KeyCode::Char(c), x)
                                if x != EditMode::ViNormal =>
                            {
                                self.tab_handler.reset_index();
                                let line_start = if self.line_buffer.line() == 0 {
                                    prompt_offset.0
                                } else {
                                    0
                                };
                                if self.maybe_wrap(terminal_size.0, line_start, c) {
                                    let (original_column, original_row) = position()?;
                                    self.run_edit_commands(&[EditCommand::InsertChar(c)]);

                                    self.buffer_paint(prompt_offset)?;

                                    let (new_column, _) = position()?;

                                    if new_column < original_column
                                        && original_row + 1 == (terminal_size.1)
                                    {
                                        // We have wrapped off bottom of screen, and prompt is on new row
                                        // We need to update the prompt location in this case
                                        prompt_origin.1 -= 1;
                                        prompt_offset.1 -= 1;
                                    }
                                } else {
                                    self.run_edit_commands(&[EditCommand::InsertChar(c)]);
                                }
                                self.line_buffer.set_previous_lines(false);
                            }
                            (KeyModifiers::NONE, KeyCode::Enter, x) if x != EditMode::ViNormal => {
                                match self.input_mode {
                                    InputMode::Regular | InputMode::HistoryTraversal => {
                                        let buffer = self.insertion_line().to_string();

                                        self.run_edit_commands(&[
                                            EditCommand::AppendToHistory,
                                            EditCommand::Clear,
                                        ]);
                                        self.print_crlf()?;
                                        self.tab_handler.reset_index();
                                        self.line_buffer.reset_olds();

                                        return Ok(Signal::Success(buffer));
                                    }
                                    InputMode::HistorySearch => {
                                        self.queue_prompt_indicator(prompt)?;

                                        if let Some(string) = self.history.string_at_cursor() {
                                            self.set_buffer(string)
                                        }

                                        self.input_mode = InputMode::Regular;
                                    }
                                }
                            }
                            _ => {
                                self.tab_handler.reset_index();
                                if let Some(binding) = self.find_keybinding(modifiers, code) {
                                    self.run_edit_commands(&binding);
                                }
                            }
                        }
                    }
                    Event::Mouse(_) => {}
                    Event::Resize(width, height) => {
                        terminal_size = (width, height);
                        // TODO properly adjusting prompt_origin on resizing while lines > 1
                        prompt_origin.1 = position()?.1.saturating_sub(1);
                        prompt_offset = self.full_repaint(prompt, prompt_origin, terminal_size)?;
                        continue;
                    }
                }
                if self.insertion_line().to_string().is_empty() {
                    self.tab_handler.reset_index();
                }
            } else {
                // No key event:
                // Repaint the prompt for the clock
                self.need_full_repaint = true;
            }

            // Repainting
            if self.input_mode == InputMode::HistorySearch {
                self.history_search_paint(prompt)?;
            } else if self.need_full_repaint {
                prompt_offset = self.full_repaint(prompt, prompt_origin, terminal_size)?;
                self.need_full_repaint = false;
            } else {
                self.buffer_paint(prompt_offset)?;
            }
        }
    }
}
