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

/// Simple [`Prompt`] displaying a configurable left and a right prompt.
/// For more fine-tuned configuration, implement the [`Prompt`] trait.
/// For the default configuration, use [`DefaultPrompt::default()`]
#[derive(Clone)]
pub struct DefaultPrompt {
    /// What segment should be rendered in the left (main) prompt
    pub left_prompt: DefaultPromptSegment,
    /// What segment should be rendered in the right prompt
    pub right_prompt: DefaultPromptSegment,
}

/// A struct to control the appearance of the left or right prompt in a [`DefaultPrompt`]
#[derive(Clone)]
pub enum DefaultPromptSegment {
    /// A basic user-defined prompt (i.e. just text)
    Basic(String),
    /// The path of the current working directory
    WorkingDirectory,
    /// The current date and time
    CurrentDateTime,
    /// An empty prompt segment
    Empty,
}

/// Given a prompt segment, render it to a Cow<str> that we can use to
/// easily implement [`Prompt`]'s `render_prompt_left` and `render_prompt_right`
/// functions.
fn render_prompt_segment(prompt: &DefaultPromptSegment) -> Cow<str> {
    match &prompt {
        DefaultPromptSegment::Basic(s) => Cow::Borrowed(s),
        DefaultPromptSegment::WorkingDirectory => {
            let prompt = get_working_dir().unwrap_or_else(|_| String::from("no path"));
            Cow::Owned(prompt)
        }
        DefaultPromptSegment::CurrentDateTime => Cow::Owned(get_now()),
        DefaultPromptSegment::Empty => Cow::Borrowed(""),
    }
}

impl Prompt for DefaultPrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        render_prompt_segment(&self.left_prompt)
    }

    fn render_prompt_right(&self) -> Cow<str> {
        render_prompt_segment(&self.right_prompt)
    }

    fn render_prompt_indicator(&self, edit_mode: PromptEditMode) -> Cow<str> {
        match edit_mode {
            PromptEditMode::Default | PromptEditMode::Emacs => DEFAULT_PROMPT_INDICATOR.into(),
            PromptEditMode::Vi(vi_mode) => match vi_mode {
                PromptViMode::Normal => DEFAULT_VI_NORMAL_PROMPT_INDICATOR.into(),
                PromptViMode::Insert => DEFAULT_VI_INSERT_PROMPT_INDICATOR.into(),
            },
            PromptEditMode::Custom(str) => format!("({str})").into(),
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
        DefaultPrompt {
            left_prompt: DefaultPromptSegment::WorkingDirectory,
            right_prompt: DefaultPromptSegment::CurrentDateTime,
        }
    }
}

impl DefaultPrompt {
    /// Constructor for the default prompt, which takes a configurable left and right prompt.
    /// For less customization, use [`DefaultPrompt::default`].
    /// For more fine-tuned configuration, implement the [`Prompt`] trait.
    pub const fn new(
        left_prompt: DefaultPromptSegment,
        right_prompt: DefaultPromptSegment,
    ) -> DefaultPrompt {
        DefaultPrompt {
            left_prompt,
            right_prompt,
        }
    }
}

fn get_working_dir() -> Result<String, std::io::Error> {
    let path = env::current_dir()?;
    let path_str = path.display().to_string();
    let homedir: String = match env::var("USERPROFILE") {
        Ok(win_home) => win_home,
        Err(_) => match env::var("HOME") {
            Ok(maclin_home) => maclin_home,
            Err(_) => path_str.clone(),
        },
    };
    let new_path = if path_str != homedir {
        path_str.replace(&homedir, "~")
    } else {
        path_str
    };
    Ok(new_path)
}

fn get_now() -> String {
    let now = Local::now();
    format!("{:>}", now.format("%m/%d/%Y %I:%M:%S %p"))
}
