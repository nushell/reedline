use crate::{Prompt, PromptEditMode, PromptHistorySearch, PromptHistorySearchStatus, PromptViMode};

use {
    chrono::Local,
    std::{borrow::Cow, env},
};

/// The default prompt indicator
pub static DEFAULT_PROMPT_INDICATOR: &str = "〉";
pub static DEFAULT_VI_INSERT_PROMPT_INDICATOR: &str = ": ";
pub static DEFAULT_VI_NORMAL_PROMPT_INDICATOR: &str = "〉";
pub static DEFAULT_MULTILINE_INDICATOR: &str = "::: ";

/// Simple two-line [`Prompt`] displaying the current working directory and the time above the entry line.
#[derive(Clone)]
pub struct DefaultPrompt;

impl Prompt for DefaultPrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        {
            let left_prompt = get_working_dir().unwrap_or_else(|_| String::from("no path"));

            Cow::Owned(left_prompt)
        }
    }

    fn render_prompt_right(&self) -> Cow<str> {
        {
            Cow::Owned(get_now())
        }
    }

    fn render_prompt_indicator(&self, edit_mode: PromptEditMode) -> Cow<str> {
        match edit_mode {
            PromptEditMode::Default | PromptEditMode::Emacs => DEFAULT_PROMPT_INDICATOR.into(),
            PromptEditMode::Vi(vi_mode) => match vi_mode {
                PromptViMode::Normal => DEFAULT_VI_NORMAL_PROMPT_INDICATOR.into(),
                PromptViMode::Insert => DEFAULT_VI_INSERT_PROMPT_INDICATOR.into(),
            },
            PromptEditMode::Custom(str) => format!("({})", str).into(),
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

impl DefaultPrompt {
    /// Constructor for the default prompt, which takes the amount of spaces required between the left and right-hand sides of the prompt
    pub fn new() -> DefaultPrompt {
        DefaultPrompt {}
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
