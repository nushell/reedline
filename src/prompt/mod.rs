mod base;
mod default;

pub use base::{
    Prompt, PromptEditMode, PromptEditModeDiscriminants, PromptHistorySearch,
    PromptHistorySearchStatus, PromptViMode, DEFAULT_INDICATOR_COLOR, DEFAULT_PROMPT_COLOR,
    DEFAULT_PROMPT_MULTILINE_COLOR, DEFAULT_PROMPT_RIGHT_COLOR,
};

pub use default::{DefaultPrompt, DefaultPromptSegment};
