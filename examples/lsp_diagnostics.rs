//! Example demonstrating LSP diagnostics integration with reedline.
//!
//! This example spawns an LSP server and displays diagnostics inline in real-time
//! as you type. It uses the same `REEDLINE_LS` environment variable that nu-cli uses.
//!
//! Run with:
//!   REEDLINE_LS="nu-lint --lsp" cargo run --example lsp_diagnostics --features lsp_diagnostics
//!
//! Prerequisites:
//! - An LSP server that supports diagnostics (e.g., nu-lint for nushell)
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
    // Use the same env var as nu-cli for consistency
    // REEDLINE_LS should contain the full command, e.g., "nu-lint --lsp"
    let Some(command) = var("REEDLINE_LS").ok() else {
        eprintln!("Error: REEDLINE_LS environment variable not set.");
        eprintln!("Set it to the full LSP server command (e.g., \"nu-lint --lsp\").");
        eprintln!();
        eprintln!("Example: REEDLINE_LS=\"nu-lint --lsp\" cargo run --example lsp_diagnostics --features lsp_diagnostics");
        std::process::exit(1);
    };

    let config = LspConfig {
        command,
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
    println!("Press Ctrl+. or Alt+f to open the fix menu when on a diagnostic.");
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

/// Add keybinding for the diagnostic fix menu (Alt+f and Ctrl+.)
fn add_diagnostic_fix_keybinding(keybindings: &mut Keybindings) {
    keybindings.add_binding(
        KeyModifiers::ALT,
        KeyCode::Char('f'),
        ReedlineEvent::OpenDiagnosticFixMenu,
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        KeyCode::Char('.'),
        ReedlineEvent::OpenDiagnosticFixMenu,
    );
}
