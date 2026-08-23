mod base;
mod cursors;
mod emacs;
#[cfg(feature = "helix")]
mod helix;
mod keybindings;
mod vi;

pub use base::EditMode;
pub use cursors::CursorConfig;
pub use emacs::{default_emacs_keybindings, Emacs};
#[cfg(feature = "helix")]
pub use helix::{
    default_helix_insert_keybindings, default_helix_normal_keybindings,
    default_helix_select_keybindings, Helix,
};
pub use keybindings::Keybindings;
pub use vi::{default_vi_insert_keybindings, default_vi_normal_keybindings, Vi};

use crossterm::event::{Event, KeyModifiers, MouseEvent, MouseEventKind};

use crate::{EditCommand, ReedlineEvent};

/// Lower the non-key terminal events every edit mode treats identically:
/// primary-button clicks, resize, focus, and bracketed paste with its
/// newlines normalized.
fn parse_non_key_event(event: Event) -> ReedlineEvent {
    match event {
        // Key events are each mode's own business; they never reach here.
        Event::Key(_) => ReedlineEvent::None,
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(button),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }) => ReedlineEvent::Mouse {
            column,
            row,
            button: button.into(),
        },
        Event::Mouse(_) => ReedlineEvent::None,
        Event::Resize(width, height) => ReedlineEvent::Resize(width, height),
        Event::FocusGained => ReedlineEvent::None,
        Event::FocusLost => ReedlineEvent::None,
        Event::Paste(body) => ReedlineEvent::Edit(vec![EditCommand::InsertString(
            body.replace("\r\n", "\n").replace('\r', "\n"),
        )]),
    }
}

/// A bare or shifted keypress — the chords that act as editing commands in a
/// modal normal mode. Anything else belongs to the keybinding tables.
fn is_plain_char(modifiers: KeyModifiers) -> bool {
    modifiers == KeyModifiers::NONE || modifiers == KeyModifiers::SHIFT
}

/// Modifier sets under which a `KeyCode::Char` is *typed text* (data), not a
/// chord: everything [`is_plain_char`] accepts, plus the Ctrl-Alt combinations
/// some terminals report for AltGr.
fn is_text_char(modifiers: KeyModifiers) -> bool {
    is_plain_char(modifiers)
        || modifiers == KeyModifiers::CONTROL | KeyModifiers::ALT
        || modifiers == KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
}
