//! Semantic prompt support for OSC 133 and OSC 633 escape sequences.
//!
//! These escape sequences help terminals understand the structure of prompts,
//! user input, and command output. This enables features like:
//! - Jumping between prompts
//! - Selecting command output
//! - Visual indicators for prompt regions
//!
//! ## Protocol Overview
//!
//! - `A` - Marks the start of a prompt (with optional `k=` kind parameter)
//! - `B` - Marks the end of prompt and start of user input
//! - `C` - Marks the start of command execution (emitted by shell, not reedline)
//! - `D` - Marks the end of command execution with exit code (emitted by shell)
//! - `P` - Sets a property (used for right prompt marker `k=r`)
//!
//! ## Prompt Kinds
//!
//! - `k=i` - Interactive/primary prompt
//! - `k=s` - Secondary/continuation prompt (for multiline input)
//! - `k=r` - Right prompt
//!
//! ## Testing with Ghostty
//!
//! To visualize semantic prompt markers in Ghostty, enable "Overlay Semantic Prompts"
//! under the Renderer section of the terminal inspector.

use std::borrow::Cow;

/// The kind of prompt being rendered
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    /// Primary/interactive prompt (the main left prompt)
    Primary,
    /// Secondary/continuation prompt (for multiline input)
    Secondary,
    /// Right-aligned prompt
    Right,
}

/// Trait for providing semantic prompt escape sequences.
///
/// Implement this trait to provide custom semantic prompt markers.
/// Built-in implementations are provided for OSC 133 and OSC 633.
pub trait SemanticPromptMarkers: Send + Sync {
    /// Returns the escape sequence to emit at the start of a prompt.
    ///
    /// For `Primary` prompts, this should return `A;k=i` (or equivalent).
    /// For `Secondary` prompts, this should return `A;k=s`.
    /// For `Right` prompts, this should return `P;k=r`.
    fn prompt_start(&self, kind: PromptKind) -> Cow<'_, str>;

    /// Returns the escape sequence to emit at the end of prompt text,
    /// before user input begins.
    ///
    /// This should return the `B` marker.
    fn command_input_start(&self) -> Cow<'_, str>;
}

/// OSC 133 semantic prompt markers (FinalTerm/iTerm2 protocol).
///
/// Use this for terminals that support OSC 133, such as:
/// - iTerm2
/// - Ghostty
/// - WezTerm
/// - Kitty
#[derive(Debug, Clone, Copy, Default)]
pub struct Osc133Markers;

impl Osc133Markers {
    /// Create a boxed instance for use with `Reedline::with_semantic_prompt()`
    pub fn boxed() -> Box<dyn SemanticPromptMarkers> {
        Box::new(Self)
    }
}

impl SemanticPromptMarkers for Osc133Markers {
    fn prompt_start(&self, kind: PromptKind) -> Cow<'_, str> {
        match kind {
            PromptKind::Primary => Cow::Borrowed("\x1b]133;A;k=i\x1b\\"), // Normally this would be 'A', but using 'P' to avoid newline issues
            PromptKind::Secondary => Cow::Borrowed("\x1b]133;A;k=s\x1b\\"), // Normally this would be 'A', but using 'P' to avoid newline issues
            PromptKind::Right => Cow::Borrowed("\x1b]133;P;k=r\x1b\\"),
        }
    }

    fn command_input_start(&self) -> Cow<'_, str> {
        Cow::Borrowed("\x1b]133;B\x1b\\")
    }
}

/// OSC 133 semantic prompt markers with click events enabled.
///
/// Use this when you want terminals to send mouse click events for
/// click-to-cursor support.
#[derive(Debug, Clone, Copy, Default)]
pub struct Osc133ClickEventsMarkers;

impl Osc133ClickEventsMarkers {
    /// Create a boxed instance for use with `Reedline::with_mouse_click()`.
    pub fn boxed() -> Box<dyn SemanticPromptMarkers> {
        Box::new(Self)
    }
}

impl SemanticPromptMarkers for Osc133ClickEventsMarkers {
    fn prompt_start(&self, kind: PromptKind) -> Cow<'_, str> {
        match kind {
            PromptKind::Primary => Cow::Borrowed("\x1b]133;A;k=i;click_events=1\x1b\\"),
            PromptKind::Secondary => Cow::Borrowed("\x1b]133;A;k=s;click_events=1\x1b\\"),
            PromptKind::Right => Cow::Borrowed("\x1b]133;P;k=r\x1b\\"),
        }
    }

    fn command_input_start(&self) -> Cow<'_, str> {
        Cow::Borrowed("\x1b]133;B\x1b\\")
    }
}

/// OSC 633 semantic prompt markers (VS Code terminal protocol).
///
/// Use this for VS Code's integrated terminal. OSC 633 is VS Code's
/// extension of OSC 133 with additional features like command line
/// reporting and working directory properties.
#[derive(Debug, Clone, Copy, Default)]
pub struct Osc633Markers;

impl Osc633Markers {
    /// Create a boxed instance for use with `Reedline::with_semantic_prompt()`
    pub fn boxed() -> Box<dyn SemanticPromptMarkers> {
        Box::new(Self)
    }
}

impl SemanticPromptMarkers for Osc633Markers {
    fn prompt_start(&self, kind: PromptKind) -> Cow<'_, str> {
        match kind {
            PromptKind::Primary => Cow::Borrowed("\x1b]633;A;k=i\x1b\\"), // Since VS Code supports OSC 633, we can use 'A' here
            PromptKind::Secondary => Cow::Borrowed("\x1b]633;A;k=s\x1b\\"), // Since VS Code supports OSC 633, we can use 'A' here
            PromptKind::Right => Cow::Borrowed("\x1b]633;P;k=r\x1b\\"),
        }
    }

    fn command_input_start(&self) -> Cow<'_, str> {
        Cow::Borrowed("\x1b]633;B\x1b\\")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_osc133_primary_prompt_start() {
        let markers = Osc133Markers;
        assert_eq!(
            markers.prompt_start(PromptKind::Primary).as_ref(),
            "\x1b]133;A;k=i\x1b\\" // Override 'A' with 'P' to avoid newline issues
        );
    }

    #[test]
    fn test_osc133_secondary_prompt_start() {
        let markers = Osc133Markers;
        assert_eq!(
            markers.prompt_start(PromptKind::Secondary).as_ref(),
            "\x1b]133;A;k=s\x1b\\" // Override 'A' with 'P' to avoid newline issues
        );
    }

    #[test]
    fn test_osc133_right_prompt_start() {
        let markers = Osc133Markers;
        assert_eq!(
            markers.prompt_start(PromptKind::Right).as_ref(),
            "\x1b]133;P;k=r\x1b\\"
        );
    }

    #[test]
    fn test_osc133_command_input_start() {
        let markers = Osc133Markers;
        assert_eq!(markers.command_input_start().as_ref(), "\x1b]133;B\x1b\\");
    }

    #[test]
    fn test_osc133_click_events_primary_prompt_start() {
        let markers = Osc133ClickEventsMarkers;
        assert_eq!(
            markers.prompt_start(PromptKind::Primary).as_ref(),
            "\x1b]133;A;k=i;click_events=1\x1b\\"
        );
    }

    #[test]
    fn test_osc133_click_events_secondary_prompt_start() {
        let markers = Osc133ClickEventsMarkers;
        assert_eq!(
            markers.prompt_start(PromptKind::Secondary).as_ref(),
            "\x1b]133;A;k=s;click_events=1\x1b\\"
        );
    }

    #[test]
    fn test_osc133_click_events_right_prompt_start() {
        let markers = Osc133ClickEventsMarkers;
        assert_eq!(
            markers.prompt_start(PromptKind::Right).as_ref(),
            "\x1b]133;P;k=r\x1b\\"
        );
    }

    #[test]
    fn test_osc133_click_events_command_input_start() {
        let markers = Osc133ClickEventsMarkers;
        assert_eq!(markers.command_input_start().as_ref(), "\x1b]133;B\x1b\\");
    }

    #[test]
    fn test_osc633_primary_prompt_start() {
        let markers = Osc633Markers;
        assert_eq!(
            markers.prompt_start(PromptKind::Primary).as_ref(),
            "\x1b]633;A;k=i\x1b\\"
        );
    }

    #[test]
    fn test_osc633_secondary_prompt_start() {
        let markers = Osc633Markers;
        assert_eq!(
            markers.prompt_start(PromptKind::Secondary).as_ref(),
            "\x1b]633;A;k=s\x1b\\"
        );
    }

    #[test]
    fn test_osc633_right_prompt_start() {
        let markers = Osc633Markers;
        assert_eq!(
            markers.prompt_start(PromptKind::Right).as_ref(),
            "\x1b]633;P;k=r\x1b\\"
        );
    }

    #[test]
    fn test_osc633_command_input_start() {
        let markers = Osc633Markers;
        assert_eq!(markers.command_input_start().as_ref(), "\x1b]633;B\x1b\\");
    }

    #[test]
    fn test_string_terminator_is_st_not_bel() {
        // Verify we use ST (\x1b\\) not BEL (\x07) as the string terminator
        let osc133 = Osc133Markers;
        let osc633 = Osc633Markers;

        // Check OSC 133 markers don't contain BEL
        assert!(!osc133.prompt_start(PromptKind::Primary).contains('\x07'));
        assert!(!osc133.command_input_start().contains('\x07'));

        // Check OSC 633 markers don't contain BEL
        assert!(!osc633.prompt_start(PromptKind::Primary).contains('\x07'));
        assert!(!osc633.command_input_start().contains('\x07'));

        // Check they contain ST
        assert!(osc133.prompt_start(PromptKind::Primary).contains("\x1b\\"));
        assert!(osc633.prompt_start(PromptKind::Primary).contains("\x1b\\"));
    }
}
