//! Diagnostic types and utilities for reedline's LSP integration.
//!
//! Re-exports LSP types and provides helper functions for styling and
//! converting between LSP positions and byte offsets.

use nu_ansi_term::{Color, Style};

// Re-export LSP types for public use
pub use lsp_types::{CodeAction, Diagnostic, DiagnosticSeverity, Range, TextEdit};

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
///
/// Used internally for buffer manipulation. LSP uses line/character positions,
/// but buffer operations need byte offsets.
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

/// Convert an LSP Range to a byte Span.
pub fn range_to_span(content: &str, range: &Range) -> Span {
    Span::new(
        position_to_offset(content, &range.start),
        position_to_offset(content, &range.end),
    )
}

/// Convert an LSP Position to a byte offset.
fn position_to_offset(content: &str, pos: &lsp_types::Position) -> usize {
    let target_line = pos.line as usize;
    content
        .lines()
        .enumerate()
        .scan(0usize, |offset, (i, line)| {
            let current_offset = *offset;
            *offset += line.len() + 1;
            Some((i, line, current_offset))
        })
        .find(|(i, _, _)| *i == target_line)
        .map(|(_, line, offset)| offset + (pos.character as usize).min(line.len()))
        .unwrap_or(content.len())
}

/// Format diagnostic messages for display below the prompt.
///
/// Renders diagnostics with vertical connecting lines and handlebars spanning the diagnostic:
/// ```text
/// ╎ ╰────╯ Unnecessary '^' prefix on external command 'head'
/// ╰ Use 'first N' to get the first N items
/// ```
///
/// # Arguments
/// * `diagnostics` - The diagnostics to format
/// * `buffer` - The text buffer content (for converting ranges to columns)
/// * `prompt_width` - The visual width of the prompt (for alignment)
/// * `use_ansi_coloring` - Whether to apply ANSI color codes
pub fn format_diagnostic_messages(
    diagnostics: &[Diagnostic],
    buffer: &str,
    prompt_width: usize,
    use_ansi_coloring: bool,
) -> String {
    use itertools::Itertools;

    // Convert and sort diagnostics by start column
    let diag_infos: Vec<DiagRenderInfo> = diagnostics
        .iter()
        .map(|d| {
            let span = range_to_span(buffer, &d.range);
            DiagRenderInfo {
                start_col: prompt_width + span.start_column(buffer),
                end_col: prompt_width + span.end_column(buffer),
                severity: d.severity.unwrap_or(DiagnosticSeverity::WARNING),
                message: d.message.clone(),
            }
        })
        .sorted_by_key(|d| d.start_col)
        .collect();

    diag_infos
        .iter()
        .enumerate()
        .map(|(i, diag)| {
            format_diagnostic_line(
                diag.start_col,
                diag.end_col,
                diag.severity,
                &diag.message,
                &diag_infos[i + 1..],
                use_ansi_coloring,
            )
        })
        .join("\n")
}

/// Pre-computed diagnostic info for rendering.
struct DiagRenderInfo {
    start_col: usize,
    end_col: usize,
    severity: DiagnosticSeverity,
    message: String,
}

/// Format a single diagnostic line with vertical connectors for future diagnostics.
fn format_diagnostic_line(
    start_col: usize,
    end_col: usize,
    severity: DiagnosticSeverity,
    message: &str,
    future_diags: &[DiagRenderInfo],
    use_ansi_coloring: bool,
) -> String {
    let vertical_connectors = build_vertical_connectors(start_col, future_diags, use_ansi_coloring);
    let connector_width = vertical_connectors
        .iter()
        .map(|(col, _)| col + 1)
        .max()
        .unwrap_or(0);

    let padding = " ".repeat(start_col.saturating_sub(connector_width));
    let handlebar = build_handlebar(
        end_col.saturating_sub(start_col),
        severity,
        use_ansi_coloring,
    );
    let styled_message = style_text(message, severity, use_ansi_coloring);

    // Merge vertical connectors into the line
    let prefix = merge_connectors_with_padding(&vertical_connectors, connector_width);

    format!("{prefix}{padding}{handlebar} {styled_message}")
}

/// Build vertical connector positions for future diagnostics that come before the current column.
fn build_vertical_connectors(
    current_col: usize,
    future_diags: &[DiagRenderInfo],
    use_ansi_coloring: bool,
) -> Vec<(usize, String)> {
    future_diags
        .iter()
        .filter(|d| d.start_col < current_col)
        .map(|d| {
            let connector = style_text("╎", d.severity, use_ansi_coloring);
            (d.start_col, connector)
        })
        .collect()
}

/// Merge vertical connectors into a string with proper spacing.
fn merge_connectors_with_padding(connectors: &[(usize, String)], total_width: usize) -> String {
    if connectors.is_empty() {
        return String::new();
    }

    connectors
        .iter()
        .fold(
            (String::new(), 0usize),
            |(mut acc, col), (connector_col, connector)| {
                acc.push_str(&" ".repeat(connector_col.saturating_sub(col)));
                acc.push_str(connector);
                (acc, connector_col + 1)
            },
        )
        .0
        + &" "
            .repeat(total_width.saturating_sub(connectors.last().map(|(c, _)| c + 1).unwrap_or(0)))
}

/// Build the handlebar (╰───╯ or ╰) for a diagnostic span.
fn build_handlebar(
    span_width: usize,
    severity: DiagnosticSeverity,
    use_ansi_coloring: bool,
) -> String {
    if span_width <= 1 {
        style_text("╰", severity, use_ansi_coloring)
    } else {
        let middle = "─".repeat(span_width.saturating_sub(2));
        format!(
            "{}{}{}",
            style_text("╰", severity, use_ansi_coloring),
            style_text(&middle, severity, use_ansi_coloring),
            style_text("╯", severity, use_ansi_coloring)
        )
    }
}

/// Apply styling to text based on severity and coloring preference.
fn style_text(text: &str, severity: DiagnosticSeverity, use_ansi_coloring: bool) -> String {
    if use_ansi_coloring {
        message_style(severity).paint(text).to_string()
    } else {
        text.to_string()
    }
}
