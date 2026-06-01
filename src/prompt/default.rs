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

#[allow(deprecated)] // home_dir is deprecated under our MSRV (1.63) but un-deprecated in 1.85
fn get_working_dir() -> Result<String, std::io::Error> {
    let cwd = env::current_dir()?;
    Ok(format_working_dir(&cwd, env::home_dir().as_deref()))
}

/// Render `cwd` for the prompt, collapsing `home` to `~` when it is a prefix.
fn format_working_dir(cwd: &std::path::Path, home: Option<&std::path::Path>) -> String {
    if let Some(home) = home {
        if let Ok(suffix) = cwd.strip_prefix(home) {
            let mut path = std::path::PathBuf::from("~");
            if !suffix.as_os_str().is_empty() {
                path = path.join(suffix);
            }
            return path.display().to_string();
        }
    }
    cwd.display().to_string()
}

fn get_now() -> String {
    let now = Local::now();
    format!("{:>}", now.format("%m/%d/%Y %I:%M:%S %p"))
}

#[cfg(all(test, unix))]
mod tests {
    use super::format_working_dir;
    use std::path::{Path, PathBuf};

    #[test]
    fn home_is_collapsed_to_tilde() {
        let home = Path::new("/home/alice");
        let cwd = PathBuf::from("/home/alice/projects");
        assert_eq!(format_working_dir(&cwd, Some(home)), "~/projects");
    }

    #[test]
    fn cwd_equal_to_home_is_just_tilde() {
        // Regression: `cd ~` previously rendered the absolute path instead of `~`.
        let home = Path::new("/home/alice");
        let cwd = PathBuf::from("/home/alice");
        assert_eq!(format_working_dir(&cwd, Some(home)), "~");
    }

    #[test]
    fn shared_prefix_is_not_collapsed() {
        // Regression: String::replace turned `/home/alicebob` into `~bob`.
        let home = Path::new("/home/alice");
        let cwd = PathBuf::from("/home/alicebob/x");
        assert_eq!(format_working_dir(&cwd, Some(home)), "/home/alicebob/x");
    }

    #[test]
    fn missing_home_leaves_path_untouched() {
        let cwd = PathBuf::from("/var/log");
        assert_eq!(format_working_dir(&cwd, None), "/var/log");
    }
}
