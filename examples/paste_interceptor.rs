// Intercept system pastes: show a long paste as a compact placeholder while
// composing, and expand it back to the full text on submit.
// cargo run --example paste_interceptor --features system_clipboard
//
// Without an interceptor, `EditCommand::PasteSystem` makes reedline read the
// clipboard and insert the text verbatim, which floods the composer when the
// clipboard holds a whole file. A `PasteInterceptor` hands that decision to the
// host: `on_paste` reads the clipboard itself and returns what (if anything)
// reedline should insert, and `expand_for_display` may rewrite the buffer on
// submit.
//
// Note the keybinding below. By default `PasteSystem` is only bound to
// Ctrl+Shift+V, which is also the paste shortcut of most terminals: the
// terminal consumes it and injects the clipboard as ordinary key events, so the
// binding is never reached (on Windows this is what tears a multi-line paste
// into one submitted line per line). A host installing an interceptor will
// therefore usually want to bind `PasteSystem` to a key the terminal does not
// take; this example uses Ctrl+V. Handling the terminal-injected paste instead
// is a different problem — see the `paste_burst` example.
//
// How to try it:
//   1. Copy three or more lines of text to the system clipboard.
//   2. Press Ctrl+V at the prompt. The line shows `[Pasted text #1 +5 lines]`
//      instead of the five pasted lines; the real text is stashed in the host.
//   3. Type some text around the placeholder, then press Enter. The submitted
//      line, and the buffer handed back in `Signal::Success`, contain the full
//      pasted text with the placeholder replaced.
//   4. Paste something shorter than three lines: it is inserted verbatim,
//      because there is nothing worth hiding.
//
// Abort with Ctrl-C or Ctrl-D.

use reedline::{
    default_emacs_keybindings, DefaultPrompt, EditCommand, Emacs, KeyCode, KeyModifiers,
    Keybindings, PasteAction, PasteInterceptor, Reedline, ReedlineEvent, Signal,
};
use std::io;
use std::sync::{Arc, Mutex};

/// Pastes at least this many lines long are replaced by a placeholder.
const PLACEHOLDER_MIN_LINES: usize = 3;

/// Bind `PasteSystem` to a key the terminal leaves alone, so the interceptor is
/// actually reachable. Only a lone `PasteSystem` is intercepted, so bind it on
/// its own rather than inside a larger batch of edit commands.
fn add_paste_keybinding(keybindings: &mut Keybindings) {
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        KeyCode::Char('v'),
        ReedlineEvent::Edit(vec![EditCommand::PasteSystem]),
    );
}

/// Keeps the full text of every paste that was replaced by a placeholder,
/// paired with the placeholder that stands in for it.
#[derive(Default)]
struct PlaceholderInterceptor {
    stash: Mutex<Vec<(String, String)>>,
}

impl PasteInterceptor for PlaceholderInterceptor {
    fn on_paste(&self) -> PasteAction {
        // Reedline does not read the clipboard once an interceptor is
        // installed. Reading it here is what lets a host inspect the raw
        // payload before deciding what the line buffer should show.
        let text = match arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
            Ok(text) if !text.is_empty() => text,
            // Empty clipboard or a read error: insert nothing.
            _ => return PasteAction::Noop,
        };

        let lines = text.lines().count();
        if lines < PLACEHOLDER_MIN_LINES {
            return PasteAction::InsertText(text);
        }

        let mut stash = self.stash.lock().expect("paste stash poisoned");
        let placeholder = format!("[Pasted text #{} +{} lines]", stash.len() + 1, lines);
        stash.push((placeholder.clone(), text));
        PasteAction::InsertText(placeholder)
    }

    fn expand_for_display(&self, buffer: &str) -> Option<String> {
        // Called on submit, before the final repaint, so reedline paints and
        // returns the expanded text. Read-only by contract: entries stay in the
        // stash, so a placeholder still expands if the line is composed again.
        let stash = self.stash.lock().expect("paste stash poisoned");
        let mut expanded = buffer.to_string();
        for (placeholder, text) in stash.iter() {
            expanded = expanded.replace(placeholder, text);
        }
        if expanded == buffer {
            None
        } else {
            Some(expanded)
        }
    }
}

fn main() -> io::Result<()> {
    println!("Copy a few lines of text, then press Ctrl+V at the prompt.");
    println!("Pastes of {PLACEHOLDER_MIN_LINES} lines or more are shown as a placeholder while");
    println!("you compose, and expand to the full text when you press Enter.");
    println!("Abort with Ctrl-C or Ctrl-D.");

    let mut keybindings = default_emacs_keybindings();
    add_paste_keybinding(&mut keybindings);

    let interceptor = Arc::new(PlaceholderInterceptor::default());
    let mut line_editor = Reedline::create()
        .with_edit_mode(Box::new(Emacs::new(keybindings)))
        .with_paste_interceptor(interceptor);
    let prompt = DefaultPrompt::default();

    loop {
        match line_editor.read_line(&prompt)? {
            Signal::Success(buffer) => {
                println!("We processed: {buffer}");
            }
            Signal::CtrlD | Signal::CtrlC => {
                println!("\nAborted!");
                break Ok(());
            }
            _ => {}
        }
    }
}
