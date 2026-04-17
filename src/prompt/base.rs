use {
    crossterm::style::Color,
    serde::{Deserialize, Serialize},
    std::{
        borrow::Cow,
        fmt::{Display, Formatter},
    },
    strum::{EnumIter, EnumString, IntoDiscriminant},
};

/// The default color for the prompt, indicator, and right prompt
pub static DEFAULT_PROMPT_COLOR: Color = Color::Green;
pub static DEFAULT_PROMPT_MULTILINE_COLOR: nu_ansi_term::Color = nu_ansi_term::Color::LightBlue;
pub static DEFAULT_INDICATOR_COLOR: Color = Color::Cyan;
pub static DEFAULT_PROMPT_RIGHT_COLOR: Color = Color::AnsiValue(5);

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
    pub const fn new(status: PromptHistorySearchStatus, search_term: String) -> Self {
        PromptHistorySearch {
            status,
            term: search_term,
        }
    }
}

/// Modes that the prompt can be in
#[derive(Serialize, Deserialize, Clone, Debug, EnumIter, Default)]
pub enum PromptEditMode {
    /// The default mode
    #[default]
    Default,

    /// Emacs normal mode
    Emacs,

    /// A vi-specific mode
    Vi(PromptViMode),

    /// A custom mode
    Custom(String),
}

/// The vi-specific modes that the prompt can be in
#[derive(Serialize, Deserialize, Clone, Debug, EnumIter, Default)]
pub enum PromptViMode {
    /// The default mode
    #[default]
    Normal,

    /// Insertion mode
    Insert,
}

/// This is the discriminant type for [`PromptEditMode`]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, EnumIter, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum PromptEditModeDiscriminants {
    /// The default mode
    #[default]
    Default,

    /// Emacs normal mode
    Emacs,

    /// Vi normal mode
    #[strum(serialize = "ViNormal", serialize = "vi_normal")]
    ViNormal,

    /// Vi insert mode
    #[strum(serialize = "ViInsert", serialize = "vi_insert")]
    ViInsert,

    /// A custom mode
    Custom,
}

impl From<PromptViMode> for PromptEditMode {
    fn from(value: PromptViMode) -> Self {
        Self::Vi(value)
    }
}

impl Display for PromptEditMode {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        use PromptViMode as Vi;
        match self {
            Self::Default => write!(f, "Default"),
            Self::Emacs => write!(f, "Emacs"),
            Self::Vi(Vi::Normal) => write!(f, "Vi_Normal"),
            Self::Vi(Vi::Insert) => write!(f, "Vi_Insert"),
            Self::Custom(s) => write!(f, "Custom_{s}"),
        }
    }
}

impl IntoDiscriminant for PromptEditMode {
    type Discriminant = PromptEditModeDiscriminants;

    fn discriminant(&self) -> Self::Discriminant {
        use PromptViMode as Vi;
        match self {
            Self::Default => Self::Discriminant::Default,
            Self::Emacs => Self::Discriminant::Emacs,
            Self::Vi(Vi::Normal) => Self::Discriminant::ViNormal,
            Self::Vi(Vi::Insert) => Self::Discriminant::ViInsert,
            Self::Custom(_) => Self::Discriminant::Custom,
        }
    }
}

/// API to provide a custom prompt.
///
/// Implementors have to provide [`str`]-based content which will be
/// displayed before the `LineBuffer` is drawn.
pub trait Prompt: Send {
    /// Provide content of the left full prompt
    fn render_prompt_left(&self) -> Cow<'_, str>;
    /// Provide content of the right full prompt
    fn render_prompt_right(&self) -> Cow<'_, str>;
    /// Render the prompt indicator (Last part of the prompt that changes based on the editor mode)
    fn render_prompt_indicator(&self, prompt_mode: PromptEditMode) -> Cow<'_, str>;
    /// Indicator to show before explicit new lines
    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str>;
    /// Render the prompt indicator for `Ctrl-R` history search
    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<'_, str>;
    /// Get the default prompt color
    fn get_prompt_color(&self) -> Color {
        DEFAULT_PROMPT_COLOR
    }
    /// Get the default multiline prompt color
    fn get_prompt_multiline_color(&self) -> nu_ansi_term::Color {
        DEFAULT_PROMPT_MULTILINE_COLOR
    }
    /// Get the default indicator color
    fn get_indicator_color(&self) -> Color {
        DEFAULT_INDICATOR_COLOR
    }
    /// Get the default right prompt color
    fn get_prompt_right_color(&self) -> Color {
        DEFAULT_PROMPT_RIGHT_COLOR
    }

    /// Whether to render right prompt on the last line
    fn right_prompt_on_last_line(&self) -> bool {
        false
    }
}
