//! Code actions support for LSP integration.
//!
//! This module handles requesting and converting LSP code actions
//! to reedline's internal representation.

use super::diagnostic::{CodeAction, Fix, Replacement, Span};
use lsp_types::{
    CodeActionContext, CodeActionParams, CodeActionResponse, Range, TextDocumentIdentifier,
};
use serde_json::Value;

/// Request code actions from the LSP server for a given span.
///
/// This sends a `textDocument/codeAction` request and converts the response
/// to reedline's `CodeAction` type.
pub(super) fn request_code_actions<F>(
    uri: &str,
    content: &str,
    span: Span,
    timeout_ms: u64,
    request_fn: F,
) -> Vec<CodeAction>
where
    F: FnOnce(&str, &CodeActionParams, u64) -> Option<Value>,
{
    let Some(uri) = uri.parse().ok() else {
        return Vec::new();
    };

    let params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri },
        range: span_to_range(content, span),
        context: CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            trigger_kind: None,
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };

    request_fn("textDocument/codeAction", &params, timeout_ms)
        .and_then(|v| serde_json::from_value::<CodeActionResponse>(v).ok())
        .map(|response| convert_code_actions(response, content))
        .unwrap_or_default()
}

/// Convert LSP code actions to reedline's CodeAction type.
fn convert_code_actions(response: CodeActionResponse, content: &str) -> Vec<CodeAction> {
    response
        .into_iter()
        .filter_map(|action_or_cmd| match action_or_cmd {
            lsp_types::CodeActionOrCommand::CodeAction(action) => Some(action),
            lsp_types::CodeActionOrCommand::Command(_) => None,
        })
        .filter_map(|action| {
            let replacements: Vec<_> = action
                .edit?
                .changes?
                .into_iter()
                .flat_map(|(_, edits)| edits)
                .map(|edit| Replacement::new(range_to_span(content, &edit.range), edit.new_text))
                .collect();

            (!replacements.is_empty()).then(|| {
                let description = action.kind.map_or_else(String::new, |k| k.as_str().to_string());
                CodeAction::new(action.title, Fix::new(description, replacements))
            })
        })
        .collect()
}

/// Convert a byte span to an LSP Range.
fn span_to_range(content: &str, span: Span) -> Range {
    Range {
        start: offset_to_position(content, span.start),
        end: offset_to_position(content, span.end),
    }
}

/// Convert a byte offset to an LSP Position.
fn offset_to_position(content: &str, offset: usize) -> lsp_types::Position {
    let (line, character) = content
        .char_indices()
        .take_while(|(i, _)| *i < offset)
        .fold((0u32, 0u32), |(line, col), (_, c)| {
            if c == '\n' {
                (line + 1, 0)
            } else {
                (line, col + 1)
            }
        });

    lsp_types::Position { line, character }
}

/// Convert an LSP Range to a byte span.
fn range_to_span(content: &str, range: &Range) -> Span {
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
