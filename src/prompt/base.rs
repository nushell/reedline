use {
    crate::core_editor::{RestPolicy, SelectionExtent},
    nu_ansi_term::Color,
    serde::{Deserialize, Serialize},
    std::{
        borrow::Cow,
        fmt::{Display, Formatter},
    },
    strum::{EnumIter, EnumString, IntoDiscriminant},
};

// The *light* variants are deliberate. Before the nu-ansi-term migration these
// were crossterm's `Color::Green`/`Color::Cyan`, which are palette 10 and 14;
// crossterm spells the dark ones `DarkGreen`/`DarkCyan`. nu-ansi-term has no
// `Dark*` prefix, so its `Green` is palette 2. Naming them here would darken
// every default prompt.

/// The default color for the prompt
pub static DEFAULT_PROMPT_COLOR: Color = Color::LightGreen;
/// The default color for the multiline prompt indicator
pub static DEFAULT_PROMPT_MULTILINE_COLOR: Color = Color::LightBlue;
/// The default color for the prompt indicator
pub static DEFAULT_INDICATOR_COLOR: Color = Color::LightCyan;
/// The default color for the right prompt
pub static DEFAULT_PROMPT_RIGHT_COLOR: Color = Color::Purple;

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
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
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

impl PromptEditMode {
    pub(crate) fn rest_policy(&self) -> RestPolicy {
        match self {
            PromptEditMode::Vi(PromptViMode::Normal) => RestPolicy::OnGrapheme,
            // Visual selections are min-width-1: the cursor always covers at
            // least the grapheme it sits on, so an empty point widens to a block.
            PromptEditMode::Vi(PromptViMode::Visual) => RestPolicy::Block,
            PromptEditMode::Vi(PromptViMode::Insert)
            | PromptEditMode::Default
            | PromptEditMode::Emacs => RestPolicy::Between,
            // No catch-all `_ =>` arm over the variants on purpose: a future
            // variant (e.g. a Helix mode) then fails to compile here until it is
            // given an explicit policy, rather than silently defaulting. The `_`
            // below only ignores the custom mode's name.
            PromptEditMode::Custom(_) => RestPolicy::Between,
        }
    }

    pub(crate) fn selection_extent(&self) -> SelectionExtent {
        match self {
            // Vi normal/visual sweep the block cursor over the grapheme it
            // lands on (vim's inclusive visual: `vw` selects "foo b").
            PromptEditMode::Vi(_) => SelectionExtent::CoverLanding,
            // The bar modes never form a block selection, and `op_end` is
            // exclusive for the word/line/grapheme motions they emit (a forward
            // find stays inclusive, matching its operator span), so the
            // gap-indexed `Span` is the natural reading. Helix will use this one!
            PromptEditMode::Default | PromptEditMode::Emacs | PromptEditMode::Custom(_) => {
                SelectionExtent::Span
            }
        }
    }
}

/// The vi-specific modes that the prompt can be in
#[derive(Serialize, Deserialize, Clone, Debug, EnumIter, Default, PartialEq, Eq)]
pub enum PromptViMode {
    /// The default mode
    #[default]
    Normal,

    /// Insertion mode
    Insert,

    /// Visual (selection) mode — like normal, but the cursor carries a
    /// min-width-1 selection that motions extend.
    Visual,
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
            Self::Vi(Vi::Visual) => write!(f, "Vi_Visual"),
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
            // Visual shares Normal's discriminant: it uses the normal-mode
            // keybindings, differing only in selection geometry.
            Self::Vi(Vi::Normal | Vi::Visual) => Self::Discriminant::ViNormal,
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
    fn get_prompt_multiline_color(&self) -> Color {
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn selection_extent_maps_vi_to_cover_landing_and_bar_modes_to_span() {
        // Pin the dispatch table itself: the PR's headline invariant is that vi
        // stays on `CoverLanding` (a strict noop) while the bar modes move to the
        // `Span` model. Asserting the mapping here fails loudly at the switch if a
        // future refactor accidentally reroutes a mode, rather than surfacing as a
        // downstream selection assertion in some editor test.
        use PromptViMode::{Insert, Normal, Visual};
        for mode in [Normal, Insert, Visual] {
            assert_eq!(
                PromptEditMode::Vi(mode).selection_extent(),
                SelectionExtent::CoverLanding,
            );
        }
        assert_eq!(
            PromptEditMode::Emacs.selection_extent(),
            SelectionExtent::Span,
        );
        assert_eq!(
            PromptEditMode::Default.selection_extent(),
            SelectionExtent::Span,
        );
        assert_eq!(
            PromptEditMode::Custom("anything".into()).selection_extent(),
            SelectionExtent::Span,
        );
    }
}
