//! Example demonstrating LSP diagnostics integration with reedline.
//!
//! This example spawns an LSP server (nu-lint) and displays diagnostics inline
//! in real-time as you type.
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
    // Use nu-lint from PATH or specify the full path
    let nu_lint_cmd = std::env::var("NU_LINT_PATH")
        .unwrap_or_else(|_| "nu-lint".to_string());
    let config = LspConfig::new(nu_lint_cmd)
        .with_args(vec!["--lsp".into()])
        .with_timeout_ms(100);

    // Create the diagnostics provider
    let diagnostics = LspDiagnosticsProvider::new(config);

    // Create reedline with LSP diagnostics integration
    // Diagnostics will be displayed inline as underlines while typing
    let mut line_editor = Reedline::create().with_lsp_diagnostics(diagnostics);
    let prompt = DefaultPrompt::default();

    println!("LSP Diagnostics Demo");
    println!("====================");
    println!();
    println!("Type nushell code to see diagnostics as underlines while typing.");
    println!("Press Ctrl+C to exit.");
    println!();

    loop {
        match line_editor.read_line(&prompt)? {
            Signal::Success(buffer) => {
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
