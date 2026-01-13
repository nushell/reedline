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
    let range = span_to_range(content, span);

    let params = CodeActionParams {
        text_document: TextDocumentIdentifier {
            uri: uri.parse().unwrap(),
        },
        range,
        context: CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            trigger_kind: None,
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };

    let result = match request_fn("textDocument/codeAction", &params, timeout_ms) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let response: CodeActionResponse = match serde_json::from_value(result) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    convert_code_actions(response, content)
}

/// Convert LSP code actions to reedline's CodeAction type.
fn convert_code_actions(response: CodeActionResponse, content: &str) -> Vec<CodeAction> {
    response
        .into_iter()
        .filter_map(|action_or_cmd| {
            // We only handle CodeAction, not Command
            let action = match action_or_cmd {
                lsp_types::CodeActionOrCommand::CodeAction(a) => a,
                lsp_types::CodeActionOrCommand::Command(_) => return None,
            };

            // Extract text edits from the workspace edit
            let edit = action.edit?;
            let changes = edit.changes?;

            let mut replacements = Vec::new();
            for (_uri, edits) in changes {
                for text_edit in edits {
                    let edit_span = range_to_span(content, &text_edit.range);
                    replacements.push(Replacement::new(edit_span, text_edit.new_text));
                }
            }

            if replacements.is_empty() {
                return None;
            }

            Some(CodeAction::new(
                action.title,
                Fix::new(
                    action
                        .kind
                        .map(|k| k.as_str().to_string())
                        .unwrap_or_default(),
                    replacements,
                ),
            ))
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
    let mut line = 0;
    let mut col = 0;
    for (i, c) in content.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    lsp_types::Position {
        line,
        character: col,
    }
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
    let mut off = 0;
    for (i, line) in content.lines().enumerate() {
        if i == pos.line as usize {
            return off + (pos.character as usize).min(line.len());
        }
        off += line.len() + 1;
    }
    content.len()
}
