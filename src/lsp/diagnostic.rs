//! Diagnostic types for reedline's LSP integration.

use nu_ansi_term::{Color, Style};

pub use lsp_types::DiagnosticSeverity;

/// Get a style for underlining diagnostic spans in the source code.
///
/// Uses underline only (no text color change) to keep the source
/// code readable while still indicating the diagnostic location.
pub fn underline_style(_severity: DiagnosticSeverity) -> Style {
    Style::new().underline()
}

/// Get a dimmed style for diagnostic messages displayed below the prompt.
///
/// Uses muted colors to be less visually intrusive while still indicating severity.
pub fn message_style(severity: DiagnosticSeverity) -> Style {
    match severity {
        DiagnosticSeverity::ERROR => Style::new().fg(Color::Fixed(167)), // muted red
        DiagnosticSeverity::WARNING => Style::new().fg(Color::Fixed(179)), // muted yellow/orange
        DiagnosticSeverity::INFORMATION => Style::new().fg(Color::Fixed(110)), // muted blue
        DiagnosticSeverity::HINT => Style::new().fg(Color::Fixed(246)),  // gray
        _ => Style::new().fg(Color::Fixed(246)),                         // default to gray
    }
}

/// A byte span within the input buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    /// Start byte offset (inclusive)
    pub start: usize,
    /// End byte offset (exclusive)
    pub end: usize,
}

impl Span {
    /// Create a new span.
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Get the visual column of the start position.
    pub fn start_column(&self, content: &str) -> usize {
        byte_offset_to_column(content, self.start)
    }

    /// Get the visual column of the end position.
    pub fn end_column(&self, content: &str) -> usize {
        byte_offset_to_column(content, self.end)
    }
}

/// Convert a byte offset to a visual column position.
///
/// Accounts for unicode character widths (e.g., CJK characters are 2 columns wide).
fn byte_offset_to_column(s: &str, byte_offset: usize) -> usize {
    use unicode_width::UnicodeWidthChar;
    s.char_indices()
        .take_while(|(pos, _)| *pos < byte_offset.min(s.len()))
        .map(|(_, ch)| ch.width().unwrap_or(0))
        .sum()
}

/// A single diagnostic message.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The severity level
    pub severity: DiagnosticSeverity,
    /// Byte span in the source text
    pub span: Span,
    /// Short message
    pub message: String,
    /// Optional longer description
    pub detail: Option<String>,
    /// Rule ID for grouping/filtering
    pub rule_id: Option<String>,
    /// Optional fix
    pub fix: Option<Fix>,
}

impl Diagnostic {
    /// Create a new diagnostic.
    pub fn new(severity: DiagnosticSeverity, span: Span, message: impl Into<String>) -> Self {
        Self {
            severity,
            span,
            message: message.into(),
            detail: None,
            rule_id: None,
            fix: None,
        }
    }

    /// Create an error diagnostic.
    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::ERROR, span, message)
    }

    /// Create a warning diagnostic.
    pub fn warning(span: Span, message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::WARNING, span, message)
    }

    /// Add a detail message.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Add a rule ID.
    #[must_use]
    pub fn with_rule_id(mut self, rule_id: impl Into<String>) -> Self {
        self.rule_id = Some(rule_id.into());
        self
    }

    /// Add a fix.
    #[must_use]
    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }
}

/// An automated fix for a diagnostic.
#[derive(Debug, Clone)]
pub struct Fix {
    /// Description of what the fix does
    pub description: String,
    /// The replacements to apply
    pub replacements: Vec<Replacement>,
}

impl Fix {
    /// Create a new fix.
    pub fn new(description: impl Into<String>, replacements: Vec<Replacement>) -> Self {
        Self {
            description: description.into(),
            replacements,
        }
    }

    /// Create a simple fix that replaces a single span.
    pub fn replace(
        description: impl Into<String>,
        span: Span,
        new_text: impl Into<String>,
    ) -> Self {
        Self::new(description, vec![Replacement::new(span, new_text)])
    }
}

/// A single text replacement.
#[derive(Debug, Clone)]
pub struct Replacement {
    /// The span to replace
    pub span: Span,
    /// The new text to insert
    pub new_text: String,
}

impl Replacement {
    /// Create a new replacement.
    pub fn new(span: Span, new_text: impl Into<String>) -> Self {
        Self {
            span,
            new_text: new_text.into(),
        }
    }
}

/// A code action that can be applied to fix or improve code.
#[derive(Debug, Clone)]
pub struct CodeAction {
    /// Title shown to the user
    pub title: String,
    /// The fix to apply
    pub fix: Fix,
}

impl CodeAction {
    /// Create a new code action.
    pub fn new(title: impl Into<String>, fix: Fix) -> Self {
        Self {
            title: title.into(),
            fix,
        }
    }
}
