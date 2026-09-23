use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
    edit_mode::keybindings::{
        add_common_control_bindings, add_common_edit_bindings, add_common_navigation_bindings,
        add_common_selection_bindings, add_extending_navigation_bindings, edit_bind, Keybindings,
    },
    Direction, EditCommand, MotionTarget,
};

/// Default Helix normal-mode keybindings.
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
    // `Alt-d` drops the selection without filling the register, where `d`
    // clobbers it. Select mode wants it too, so the select table keeps it.
    kb.add_binding(
        KeyModifiers::ALT,
        KeyCode::Char('d'),
        edit_bind(EditCommand::EraseSelection),
    );
    // Uppercase is the Alt-modified backtick; plain backtick lowercases and is
    // typeable, so the state machine takes that one.
    kb.add_binding(
        KeyModifiers::ALT,
        KeyCode::Char('`'),
        edit_bind(EditCommand::UppercaseSelection),
    );

    kb
}

/// Default Helix select-mode keybindings.
///
/// The normal-mode table with every mode-blind navigation key overridden by
/// its extending twin, so a motion typed on a key mirrors what its modal
/// sibling does in select mode: arrows extend like `h`/`j`/`k`/`l`, word
/// chords like `b`/`w`, line and buffer edges like `gh`/`gl`/`gg`/`ge`.
/// Up/Down never reach menus or history, matching the modal `j`/`k`, since
/// history traversal would replace the buffer the selection is anchored in.
/// The history-hint events ride only on normal-mode `End`/`Ctrl-e`/
/// `Ctrl-Right`: accepting a hint inserts text, which select mode must not do.
pub fn default_helix_select_keybindings() -> Keybindings {
    use Direction as D;
    use MotionTarget as MT;

    let mut kb = default_helix_normal_keybindings();

    add_extending_navigation_bindings(
        &mut kb,
        EditCommand::Extend(MT::Grapheme(D::Backward)),
        EditCommand::Extend(MT::Grapheme(D::Forward)),
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
