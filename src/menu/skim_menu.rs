use {
    super::{Menu, MenuBuilder, MenuEvent, MenuSettings},
    crate::{
        core_editor::Editor,
        menu_functions::{completer_input, replace_in_buffer},
        painting::Painter,
        Completer, Suggestion,
    },
    skim::prelude::*,
    std::{borrow::Cow, sync::Arc},
    strip_ansi_escapes::strip,
    unicode_width::UnicodeWidthStr,
};

struct SkimSuggestionItem {
    text: String,
    display: String,
    output: String,
}

impl SkimSuggestionItem {
    fn new(index: usize, suggestion: &Suggestion, pad_width: usize) -> Self {
        let raw_value = suggestion.display_value().to_string();
        let value = strip_ansi_to_string(&raw_value);
        let value_width = value.width();
        let padding = " ".repeat(pad_width.saturating_sub(value_width) + 2);
        let description = suggestion
            .description
            .as_deref()
            .map(strip_ansi_to_string)
            .filter(|desc| !desc.is_empty());
        let display = match description.as_deref() {
            Some(desc) => format!("{value}{padding}{desc}"),
            None => value.clone(),
        };
        let text = match description.as_deref() {
            Some(desc) => format!("{value} {desc}"),
            None => value.clone(),
        };

        Self {
            text,
            display,
            output: index.to_string(),
        }
    }
}

impl SkimItem for SkimSuggestionItem {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.text)
    }

    fn display<'a>(&'a self, _context: DisplayContext<'a>) -> AnsiString<'a> {
        AnsiString::from(self.display.clone())
    }

    fn output(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.output)
    }
}

/// Menu that uses skim to select completion suggestions.
pub struct SkimMenu {
    settings: MenuSettings,
    active: bool,
    values: Vec<Suggestion>,
    selected: Option<Suggestion>,
    input: Option<String>,
    prompt: String,
    min_height: usize,
    max_height: usize,
    last_height_lines: Option<u16>,
}

impl Default for SkimMenu {
    fn default() -> Self {
        Self {
            settings: MenuSettings::default().with_name("skim_menu"),
            active: false,
            values: Vec::new(),
            selected: None,
            input: None,
            prompt: "  ".to_string(),
            min_height: 5,
            max_height: 10,
            last_height_lines: None,
        }
    }
}

impl MenuBuilder for SkimMenu {
    fn settings_mut(&mut self) -> &mut MenuSettings {
        &mut self.settings
    }
}

impl SkimMenu {
    /// Set the prompt shown in the skim picker.
    #[must_use]
    pub fn with_prompt(mut self, prompt: &str) -> Self {
        self.prompt = prompt.to_string();
        self
    }

    /// Set the minimum height for the skim picker.
    #[must_use]
    pub fn with_min_height(mut self, min_height: &str) -> Self {
        if let Ok(min_height) = min_height.parse::<usize>() {
            self.min_height = min_height;
        }
        self
    }

    /// Set the maximum height for the skim picker.
    #[must_use]
    pub fn with_height(mut self, height: &str) -> Self {
        if let Ok(height) = height.parse::<usize>() {
            self.max_height = height;
        }
        self
    }

    fn run_picker(&mut self) -> Option<usize> {
        let (min_height, max_height) = if self.min_height <= self.max_height {
            (self.min_height, self.max_height)
        } else {
            (self.max_height, self.min_height)
        };
        let desired_height = self.values.len().saturating_add(1);
        let height_lines = desired_height.clamp(min_height, max_height);
        let min_height = min_height.to_string();
        let height = height_lines.to_string();
        let clear_height = std::cmp::min(height_lines, u16::MAX as usize) as u16;
        self.last_height_lines = Some(clear_height);
        let options = SkimOptionsBuilder::default()
            .prompt(Some(self.prompt.as_str()))
            .min_height(Some(min_height.as_str()))
            .height(Some(height.as_str()))
            .no_clear_start(true)
            .no_clear(true)
            .reverse(true)
            .bind(vec!["ctrl-c:abort"])
            .multi(false)
            .build()
            .ok()?;

        let pad_width = self
            .values
            .iter()
            .map(|suggestion| strip_ansi_to_string(&suggestion.display_value().to_string()).width())
            .max()
            .unwrap_or(0);
        let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
        for (index, suggestion) in self.values.iter().enumerate() {
            let item = SkimSuggestionItem::new(index, suggestion, pad_width);
            let _ = tx.send(Arc::new(item));
        }
        drop(tx);

        let output = Skim::run_with(&options, Some(rx))?;
        if output.is_abort {
            return None;
        }
        output
            .selected_items
            .get(0)
            .and_then(|item| item.output().parse::<usize>().ok())
    }
}

impl Menu for SkimMenu {
    fn settings(&self) -> &MenuSettings {
        &self.settings
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn menu_event(&mut self, event: MenuEvent) {
        match event {
            MenuEvent::Activate(_) => self.active = true,
            MenuEvent::Deactivate => {
                self.active = false;
                self.input = None;
                self.selected = None;
            }
            _ => {}
        }
    }

    fn uses_external_picker(&self) -> bool {
        true
    }

    fn external_pick(
        &mut self,
        editor: &mut Editor,
        completer: &mut dyn Completer,
    ) -> Option<Suggestion> {
        self.update_values(editor, completer);
        let selection = match self.values.len() {
            0 => {
                self.last_height_lines = None;
                None
            }
            1 => {
                self.last_height_lines = None;
                self.values.get(0).cloned()
            }
            _ => self
                .run_picker()
                .and_then(|index| self.values.get(index).cloned()),
        };

        self.selected = selection.clone();
        selection
    }

    fn can_quick_complete(&self) -> bool {
        false
    }

    fn can_partially_complete(
        &mut self,
        _values_updated: bool,
        _editor: &mut Editor,
        _completer: &mut dyn Completer,
    ) -> bool {
        false
    }

    fn update_values(&mut self, editor: &mut Editor, completer: &mut dyn Completer) {
        if self.settings.only_buffer_difference && self.input.is_none() {
            self.input = Some(editor.get_buffer().to_string());
        }

        let (input, pos) = completer_input(
            editor.get_buffer(),
            editor.insertion_point(),
            self.input.as_deref(),
            self.settings.only_buffer_difference,
        );

        let (values, _) = completer.complete_with_base_ranges(&input, pos);
        self.values = values;
    }

    fn update_working_details(
        &mut self,
        _editor: &mut Editor,
        _completer: &mut dyn Completer,
        _painter: &Painter,
    ) {
    }

    fn replace_in_buffer(&self, editor: &mut Editor) {
        replace_in_buffer(self.selected.clone(), editor);
    }

    fn menu_required_lines(&self, _terminal_columns: u16) -> u16 {
        0
    }

    fn menu_string(&self, _available_lines: u16, _use_ansi_coloring: bool) -> String {
        String::new()
    }

    fn min_rows(&self) -> u16 {
        0
    }

    fn get_values(&self) -> &[Suggestion] {
        &self.values
    }

    fn take_external_clear_height(&mut self) -> Option<u16> {
        self.last_height_lines.take()
    }
}

fn strip_ansi_to_string(value: &str) -> String {
    String::from_utf8_lossy(&strip(value.as_bytes())).into_owned()
}
