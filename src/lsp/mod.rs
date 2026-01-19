//! LSP integration for inline diagnostics.
//!
//! This module provides a minimal LSP client for real-time diagnostics in the REPL.
//!
//! # Example
//!
//! ```ignore
//! use reedline::{LspConfig, LspDiagnosticsProvider};
//!
//! let config = LspConfig::new("nu-lint").with_args(vec!["--lsp".into()]);
//! let mut provider = LspDiagnosticsProvider::new(config);
//!
//! provider.update_content("let x = 1");
//! for diag in provider.diagnostics() {
//!     println!("{}: {}", diag.severity, diag.message);
//! }
//! ```

mod actions;
mod client;
mod diagnostic;

pub use client::{LspConfig, LspDiagnosticsProvider};
pub use diagnostic::{
    message_style, underline_style, CodeAction, Diagnostic, DiagnosticSeverity, Fix, Replacement,
    Span,
};
