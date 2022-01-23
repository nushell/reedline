use {
    chrono::Local,
    crossterm::style::Color,
    std::{borrow::Cow, env},
};

/// The default color for the prompt
pub static DEFAULT_PROMPT_COLOR: Color = Color::Blue;

/// The default prompt indicator
pub static DEFAULT_PROMPT_INDICATOR: &str = "〉";
pub static DEFAULT_VI_INSERT_PROMPT_INDICATOR: &str = ": ";
pub static DEFAULT_VI_VISUAL_PROMPT_INDICATOR: &str = "v ";
pub static DEFAULT_MENU_INDICATOR: &str = "| ";
pub static DEFAULT_HISTORY_INDICATOR: &str = "? ";
pub static DEFAULT_MULTILINE_INDICATOR: &str = "::: ";

/// The current success/failure of the history search
pub enum PromptHistorySearchStatus {
    /// Success for the search
    Passing,

    /// Failure to find the search
    Failing,
}

/// A representation of the history search
pub struct PromptHistorySearch {
    /// The status of the search
    pub status: PromptHistorySearchStatus,

    /// The search term used during the search
    pub term: String,
}

impl PromptHistorySearch {
    /// A constructor to create a history search
    pub fn new(status: PromptHistorySearchStatus, search_term: String) -> Self {
        PromptHistorySearch {
            status,
            term: search_term,
        }
    }
}

/// Modes that the prompt can be in
pub enum PromptEditMode {
    /// The default mode
    Default,

    /// Emacs normal mode
    Emacs,

    /// A vi-specific mode
    Vi(PromptViMode),

    /// A custom mode
    Custom(String),

    /// Menu edit mode
    Menu,

    /// History menu edit mode
    HistoryMenu,
}

/// The vi-specific modes that the prompt can be in
pub enum PromptViMode {
    /// The default mode
    Normal,

    /// Insertion mode
    Insert,

    /// Visual mode
    Visual,
}

/// API to provide a custom prompt.
///
/// Implementors have to provide [`str`]-based content which will be
/// displayed before the `LineBuffer` is drawn.
pub trait Prompt: Send {
    /// Provide content off the right full prompt
    fn render_prompt_left(&self) -> Cow<str>;
    /// Provide content off the left full prompt
    fn render_prompt_right(&self) -> Cow<str>;
    /// Render the default prompt indicator
    fn render_prompt_indicator(&self, prompt_mode: PromptEditMode) -> Cow<str>;
    /// Render the default prompt indicator
    fn render_prompt_multiline_indicator(&self) -> Cow<str>;
    /// Render the default prompt indicator
    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<str>;
    /// Render the vi insert mode prompt indicator
    /// Get back the prompt color
    fn get_prompt_color(&self) -> Color {
        DEFAULT_PROMPT_COLOR
    }
}

impl Prompt for DefaultPrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        DefaultPrompt::render_prompt_left(self)
    }

    fn render_prompt_right(&self) -> Cow<str> {
        DefaultPrompt::render_prompt_right(self)
    }

    fn render_prompt_indicator(&self, edit_mode: PromptEditMode) -> Cow<str> {
        match edit_mode {
            PromptEditMode::Default | PromptEditMode::Emacs => DEFAULT_PROMPT_INDICATOR.into(),
            PromptEditMode::Vi(vi_mode) => match vi_mode {
                PromptViMode::Normal => DEFAULT_PROMPT_INDICATOR.into(),
                PromptViMode::Insert => DEFAULT_VI_INSERT_PROMPT_INDICATOR.into(),
                PromptViMode::Visual => DEFAULT_VI_VISUAL_PROMPT_INDICATOR.into(),
            },
            PromptEditMode::Custom(str) => {
                DefaultPrompt::default_wrapped_custom_string(&str).into()
            }
            PromptEditMode::Menu => DEFAULT_MENU_INDICATOR.into(),
            PromptEditMode::HistoryMenu => DEFAULT_HISTORY_INDICATOR.into(),
        }
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        Cow::Borrowed(DEFAULT_MULTILINE_INDICATOR)
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "",
            PromptHistorySearchStatus::Failing => "failing ",
        };
        // NOTE: magic strings, given there is logic on how these compose I am not sure if it
        // is worth extracting in to static constant
        Cow::Owned(format!(
            "({}reverse-search: {}) ",
            prefix, history_search.term
        ))
    }
}

impl Default for DefaultPrompt {
    fn default() -> Self {
        DefaultPrompt::new()
    }
}

/// Simple two-line [`Prompt`] displaying the current working directory and the time above the entry line.
#[derive(Clone)]
pub struct DefaultPrompt;

impl DefaultPrompt {
    /// Constructor for the default prompt, which takes the amount of spaces required between the left and right-hand sides of the prompt
    pub fn new() -> DefaultPrompt {
        DefaultPrompt {}
    }

    fn render_prompt_left(&self) -> Cow<str> {
        let left_prompt = get_working_dir().unwrap_or_else(|_| String::from("no path"));

        Cow::Owned(left_prompt)
    }

    fn render_prompt_right(&self) -> Cow<str> {
        Cow::Owned(get_now())
    }

    fn default_wrapped_custom_string(str: &str) -> String {
        format!("({})", str)
    }
}

fn get_working_dir() -> Result<String, std::io::Error> {
    let path = env::current_dir()?;
    Ok(path.display().to_string())
}

fn get_now() -> String {
    let now = Local::now();
    format!("{:>}", now.format("%m/%d/%Y %I:%M:%S %p"))
}
