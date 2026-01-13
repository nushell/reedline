//! Example demonstrating LSP diagnostics integration with reedline.
//!
//! This example spawns an LSP server (nu-lint) and displays diagnostics inline.
//!
//! Run with: cargo run --example lsp_diagnostics --features lsp_diagnostics
//!
//! Prerequisites:
//! - nu-lint must be installed and available in PATH with LSP support
//!
//! Try typing nushell code with issues like:
//! - `let x = 1` (unused variable warning)
//! - `echo "hello"` (deprecated command)

use reedline::{DefaultPrompt, LspConfig, LspDiagnosticsProvider, Reedline, Signal};
use std::io;

fn main() -> io::Result<()> {
    // Configure the LSP server
    let config = LspConfig::new("nu-lint")
        .with_args(vec!["--lsp".into()])
        .with_timeout_ms(100);

    // Create the diagnostics provider
    let mut diagnostics = LspDiagnosticsProvider::new(config);

    let mut line_editor = Reedline::create();
    let prompt = DefaultPrompt::default();

    println!("LSP Diagnostics Demo");
    println!("====================");
    println!();
    println!("Type nushell code to see diagnostics.");
    println!("Press Ctrl+C to exit.");
    println!();

    loop {
        match line_editor.read_line(&prompt)? {
            Signal::Success(buffer) => {
                // Update diagnostics for the entered text
                diagnostics.update_content(&buffer);

                // Display any diagnostics
                let diags = diagnostics.diagnostics();
                if !diags.is_empty() {
                    println!("\nDiagnostics:");
                    for diag in diags {
                        let severity = match diag.severity {
                            reedline::DiagnosticSeverity::Error => "error",
                            reedline::DiagnosticSeverity::Warning => "warning",
                            reedline::DiagnosticSeverity::Info => "info",
                            reedline::DiagnosticSeverity::Hint => "hint",
                        };
                        println!(
                            "  [{severity}] {}:{}-{}: {}",
                            diag.rule_id.as_deref().unwrap_or(""),
                            diag.span.start,
                            diag.span.end,
                            diag.message
                        );
                    }
                    println!();
                }

                if buffer.trim() == "exit" {
                    break;
                }
                println!("You entered: {buffer}");
            }
            Signal::CtrlD | Signal::CtrlC => {
                println!("\nGoodbye!");
                break;
            }
        }
    }

    Ok(())
}
