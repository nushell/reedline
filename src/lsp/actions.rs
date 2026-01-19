//! Code actions support for LSP integration.
//!
//! This module handles requesting LSP code actions.

use super::diagnostic::Span;
use lsp_types::{
    CodeAction, CodeActionContext, CodeActionParams, CodeActionResponse, Range,
    TextDocumentIdentifier,
};
use serde_json::Value;

/// Request code actions from the LSP server for a given span.
///
/// Returns the raw LSP code actions. Conversion to byte spans happens
/// in the diagnostic fix menu when needed.
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
        .map(filter_code_actions)
        .unwrap_or_default()
}

/// Filter LSP response to only include actual code actions (not commands).
fn filter_code_actions(response: CodeActionResponse) -> Vec<CodeAction> {
    response
        .into_iter()
        .filter_map(|action_or_cmd| match action_or_cmd {
            lsp_types::CodeActionOrCommand::CodeAction(action) => Some(action),
            lsp_types::CodeActionOrCommand::Command(_) => None,
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
