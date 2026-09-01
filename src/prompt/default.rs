//! [`DefaultPrompt`] and the indicator constants it renders.
//!
//! The four mode indicators are two columns of ASCII each, thus a mode switch
//! never reflows the input line and no terminal needs a font it might not
//! have. [`DEFAULT_MULTILINE_INDICATOR`] is deliberately wider, since it marks
//! a line the prompt did not start rather than a mode.
//!
//! The modal three are named for the state, not the dialect: vi and helix
//! share every value, and the one state whose name they disagree on (vi's
//! visual, helix's select) would otherwise have nowhere to sit.

use crate::prompt::base::PromptHelixMode;
use crate::{Prompt, PromptEditMode, PromptHistorySearch, PromptHistorySearchStatus, PromptViMode};

use {
    chrono::Local,
    std::{borrow::Cow, env},
};

/// Emacs and [`PromptEditMode::Default`], and the glyph the modal insert modes
/// share. See the module docs for the contract every indicator here keeps.
pub static DEFAULT_PROMPT_INDICATOR: &str = "> ";
/// Modal insert. The same glyph as [`DEFAULT_PROMPT_INDICATOR`] on purpose:
/// both `Vi::default` and `Helix::default` start here, and nothing about
/// typing differs from emacs, so the mode a newcomer lands in should not look
/// like a mode they do not know.
pub static DEFAULT_INSERT_PROMPT_INDICATOR: &str = "> ";
/// Modal normal. Keystrokes are commands here, which is what the ex prompt's
/// `:` has meant all along.
pub static DEFAULT_NORMAL_PROMPT_INDICATOR: &str = ": ";
/// Vi visual and helix select: the mode whose motions grow a span.
pub static DEFAULT_SELECT_PROMPT_INDICATOR: &str = "+ ";
/// Continuation lines, in every edit mode. Wider than the indicators on
/// purpose: it marks a line the prompt did not start, not a mode.
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
fn render_prompt_segment(prompt: &DefaultPromptSegment) -> Cow<'_, str> {
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
    fn render_prompt_left(&self) -> Cow<'_, str> {
        render_prompt_segment(&self.left_prompt)
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        render_prompt_segment(&self.right_prompt)
    }

    fn render_prompt_indicator(&self, edit_mode: PromptEditMode) -> Cow<'_, str> {
        match edit_mode {
            PromptEditMode::Default | PromptEditMode::Emacs => DEFAULT_PROMPT_INDICATOR.into(),
            PromptEditMode::Helix(hx_mode) => match hx_mode {
                PromptHelixMode::Normal => DEFAULT_NORMAL_PROMPT_INDICATOR.into(),
                PromptHelixMode::Select => DEFAULT_SELECT_PROMPT_INDICATOR.into(),
                PromptHelixMode::Insert => DEFAULT_INSERT_PROMPT_INDICATOR.into(),
            },
            PromptEditMode::Vi(vi_mode) => match vi_mode {
                PromptViMode::Normal => DEFAULT_NORMAL_PROMPT_INDICATOR.into(),
                PromptViMode::Visual => DEFAULT_SELECT_PROMPT_INDICATOR.into(),
                PromptViMode::Insert => DEFAULT_INSERT_PROMPT_INDICATOR.into(),
            },
            PromptEditMode::Custom(str) => format!("({str})").into(),
        }
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed(DEFAULT_MULTILINE_INDICATOR)
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
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
    let cwd = env::current_dir()?;
    // `USERPROFILE` on Windows, `HOME` elsewhere. Avoids `env::home_dir()`,
    // which is buggy on Windows before 1.85 (above our 1.63 MSRV).
    let home = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(std::path::PathBuf::from);
    Ok(format_working_dir(&cwd, home.as_deref()))
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

#[cfg(test)]
mod tests {
    use super::{
        format_working_dir, DefaultPrompt, DEFAULT_INSERT_PROMPT_INDICATOR,
        DEFAULT_MULTILINE_INDICATOR, DEFAULT_NORMAL_PROMPT_INDICATOR, DEFAULT_PROMPT_INDICATOR,
        DEFAULT_SELECT_PROMPT_INDICATOR,
    };
    use crate::{Prompt, PromptEditMode, PromptHelixMode, PromptViMode};
    use rstest::rstest;
    use std::path::{Path, PathBuf};
    use unicode_width::UnicodeWidthStr;

    /// The whole point of the assignment, pinned as literals rather than as the
    /// constants: insert shares emacs's glyph since every modal session starts
    /// there, and the three states whose keys mean different things each render
    /// differently. Asserting against the constants would restate the `match`.
    #[rstest]
    #[case::default(PromptEditMode::Default, "> ")]
    #[case::emacs(PromptEditMode::Emacs, "> ")]
    #[case::vi_insert(PromptEditMode::Vi(PromptViMode::Insert), "> ")]
    #[case::vi_normal(PromptEditMode::Vi(PromptViMode::Normal), ": ")]
    #[case::vi_visual(PromptEditMode::Vi(PromptViMode::Visual), "+ ")]
    #[case::helix_insert(PromptEditMode::Helix(PromptHelixMode::Insert), "> ")]
    #[case::helix_normal(PromptEditMode::Helix(PromptHelixMode::Normal), ": ")]
    #[case::helix_select(PromptEditMode::Helix(PromptHelixMode::Select), "+ ")]
    #[case::custom(PromptEditMode::Custom("fish".into()), "(fish)")]
    fn indicator_table_splits_the_modal_states(
        #[case] mode: PromptEditMode,
        #[case] expected: &str,
    ) {
        assert_eq!(
            DefaultPrompt::default().render_prompt_indicator(mode),
            expected
        );
    }

    /// The equal-width contract the module docs promise, asserted on the
    /// shipped constants. `Custom` is exempt: its width is the caller's.
    #[rstest]
    #[case::base(DEFAULT_PROMPT_INDICATOR)]
    #[case::insert(DEFAULT_INSERT_PROMPT_INDICATOR)]
    #[case::normal(DEFAULT_NORMAL_PROMPT_INDICATOR)]
    #[case::select(DEFAULT_SELECT_PROMPT_INDICATOR)]
    fn mode_indicators_are_two_columns_of_ascii(#[case] indicator: &str) {
        assert!(indicator.is_ascii());
        assert_eq!(indicator.width(), 2);
    }

    /// The continuation marker is the one that opts out, so pin that it does:
    /// a silent narrowing to two columns would make it read as a mode.
    #[test]
    fn the_multiline_indicator_is_wider_than_a_mode() {
        assert!(DEFAULT_MULTILINE_INDICATOR.is_ascii());
        assert!(DEFAULT_MULTILINE_INDICATOR.width() > 2);
    }

    #[cfg(unix)]
    #[test]
    fn home_is_collapsed_to_tilde() {
        let home = Path::new("/home/alice");
        let cwd = PathBuf::from("/home/alice/projects");
        assert_eq!(format_working_dir(&cwd, Some(home)), "~/projects");
    }

    #[cfg(unix)]
    #[test]
    fn cwd_equal_to_home_is_just_tilde() {
        // Regression: `cd ~` rendered the absolute path, not `~`.
        let home = Path::new("/home/alice");
        let cwd = PathBuf::from("/home/alice");
        assert_eq!(format_working_dir(&cwd, Some(home)), "~");
    }

    #[cfg(unix)]
    #[test]
    fn shared_prefix_is_not_collapsed() {
        // Regression: String::replace turned `/home/alicebob` into `~bob`.
        let home = Path::new("/home/alice");
        let cwd = PathBuf::from("/home/alicebob/x");
        assert_eq!(format_working_dir(&cwd, Some(home)), "/home/alicebob/x");
    }

    #[cfg(unix)]
    #[test]
    fn missing_home_leaves_path_untouched() {
        let cwd = PathBuf::from("/var/log");
        assert_eq!(format_working_dir(&cwd, None), "/var/log");
    }

    #[cfg(windows)]
    #[test]
    fn home_is_collapsed_to_tilde() {
        let home = Path::new(r"C:\Users\alice");
        let cwd = PathBuf::from(r"C:\Users\alice\projects");
        assert_eq!(format_working_dir(&cwd, Some(home)), r"~\projects");
    }

    #[cfg(windows)]
    #[test]
    fn cwd_equal_to_home_is_just_tilde() {
        // Regression: `cd ~` previously rendered the absolute path instead of `~`.
        let home = Path::new(r"C:\Users\alice");
        let cwd = PathBuf::from(r"C:\Users\alice");
        assert_eq!(format_working_dir(&cwd, Some(home)), "~");
    }

    #[cfg(windows)]
    #[test]
    fn shared_prefix_is_not_collapsed() {
        // Regression: String::replace turned `C:\Users\alice` into `~bob`.
        let home = Path::new(r"C:\Users\alice");
        let cwd = PathBuf::from(r"C:\Users\alicebob\x");
        assert_eq!(format_working_dir(&cwd, Some(home)), r"C:\Users\alicebob\x");
    }

    #[cfg(windows)]
    #[test]
    fn missing_home_leaves_path_untouched() {
        let cwd = PathBuf::from(r"C:\Windows\System32");
        assert_eq!(format_working_dir(&cwd, None), r"C:\Windows\System32");
    }
}
