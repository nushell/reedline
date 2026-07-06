use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
    edit_mode::keybindings::{
        add_common_control_bindings, add_common_edit_bindings, add_common_navigation_bindings,
        add_common_selection_bindings, edit_bind, Keybindings,
    },
    EditCommand,
};

/// Default Helix normal-mode keybindings (shared with select mode).
///
/// These cover the keys the modal layer in [`super::Helix`] does not interpret
/// itself: control chords, navigation/menu keys and the like. Plain characters
/// (`w`, `d`, `gh`, …) are handled by the modal parser; a binding added here
/// for a plain character takes precedence over it, mirroring `Vi`.
pub fn default_helix_normal_keybindings() -> Keybindings {
    let mut kb = Keybindings::new();

    add_common_control_bindings(&mut kb);
    add_common_navigation_bindings(&mut kb);
    add_common_selection_bindings(&mut kb);
    // Like vi normal mode: Backspace moves left, Delete deletes in place.
    kb.add_binding(
        KeyModifiers::NONE,
        KeyCode::Backspace,
        edit_bind(EditCommand::MoveLeft { select: false }),
    );
    kb.add_binding(
        KeyModifiers::NONE,
        KeyCode::Delete,
        edit_bind(EditCommand::Delete),
    );

    kb
}

/// Default Helix insert-mode keybindings, with the common emacs-style editing set,
/// identical to vi insert mode.
pub fn default_helix_insert_keybindings() -> Keybindings {
    let mut kb = Keybindings::new();

    add_common_control_bindings(&mut kb);
    add_common_navigation_bindings(&mut kb);
    add_common_edit_bindings(&mut kb);
    add_common_selection_bindings(&mut kb);

    kb
}
