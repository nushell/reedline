//! Example demonstrating LSP diagnostics integration with reedline.
//!
//! This example spawns an LSP server (nu-lint) and displays diagnostics inline
//! in real-time as you type. It also demonstrates the diagnostic fix menu.
//!
//! Run with: cargo run --example lsp_diagnostics --features lsp_diagnostics
//!
//! Prerequisites:
//! - nu-lint must be installed and available in PATH with LSP support
//!
//! Try typing nushell code with issues like:
//! - `let x = 1` (unused variable warning)
//! - `echo "hello"` (deprecated command)
//!
//! Press Alt+f to open the fix menu when cursor is on a diagnostic with available fixes.

use crossterm::event::{KeyCode, KeyModifiers};
use reedline::{
    default_emacs_keybindings, DefaultPrompt, Emacs, Keybindings, LspConfig,
    LspDiagnosticsProvider, Reedline, ReedlineEvent, Signal,
};
use std::{env::var, io};

fn main() -> io::Result<()> {
    // Configure the LSP server
    // Use nu-lint from PATH or specify the full path
    let nu_lint_bin = var("NU_LINT_PATH").unwrap_or("nu-lint".to_string());
    let config = LspConfig {
        server_bin: nu_lint_bin,
        server_args: vec!["--lsp".into()],
        timeout_ms: 100,
        uri_scheme: "repl".to_string(),
    };

    // Create the diagnostics provider
    let diagnostics = LspDiagnosticsProvider::new(config);

    // Set up keybindings with the diagnostic fix menu
    let mut keybindings = default_emacs_keybindings();
    add_diagnostic_fix_keybinding(&mut keybindings);

    let edit_mode = Box::new(Emacs::new(keybindings));

    // Create reedline with LSP diagnostics integration
    // Diagnostics will be displayed inline as underlines while typing
    let mut line_editor = Reedline::create()
        .with_lsp_diagnostics(diagnostics)
        .with_edit_mode(edit_mode);

    let prompt = DefaultPrompt::default();

    println!("LSP Diagnostics Demo");
    println!("====================");
    println!();
    println!("Type nushell code to see diagnostics as underlines while typing.");
    println!("Press Alt+f to open the fix menu when on a diagnostic.");
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

/// Add keybinding for the diagnostic fix menu (Alt+f)
fn add_diagnostic_fix_keybinding(keybindings: &mut Keybindings) {
    keybindings.add_binding(
        KeyModifiers::ALT,
        KeyCode::Char('f'),
        ReedlineEvent::OpenDiagnosticFixMenu,
    );
}
