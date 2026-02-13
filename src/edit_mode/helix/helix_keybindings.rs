use crate::{
    edit_mode::keybindings::Keybindings,
    enums::{EditCommand, ReedlineEvent},
};
use crossterm::event::{KeyCode, KeyModifiers};

/// Adds keybindings shared between Normal and Select modes.
///
/// These are non-motion bindings whose behavior is identical regardless of
/// whether the selection anchor resets (Normal) or stays fixed (Select):
/// Enter, Ctrl+C, Ctrl+D, x, d, y, p, P, ;, Alt+;, u, U.
fn add_common_keybindings(keybindings: &mut Keybindings) {
    keybindings.add_binding(KeyModifiers::NONE, KeyCode::Enter, ReedlineEvent::Enter);
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        KeyCode::Char('c'),
        ReedlineEvent::CtrlC,
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        KeyCode::Char('d'),
        ReedlineEvent::CtrlD,
    );

    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('x'),
        ReedlineEvent::Edit(vec![
            EditCommand::MoveToLineStart { select: false },
            EditCommand::MoveToLineEnd { select: true },
        ]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('d'),
        ReedlineEvent::Edit(vec![EditCommand::CutSelection]),
    );
    // Note: 'c' is handled in Helix::parse_event (enters insert mode after cut),
    // so it is intentionally absent from the keybinding map.
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('y'),
        ReedlineEvent::Edit(vec![EditCommand::CopySelection]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('p'),
        ReedlineEvent::Edit(vec![EditCommand::Paste]),
    );
    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::Char('p'),
        ReedlineEvent::Edit(vec![EditCommand::PasteCutBufferBefore]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char(';'),
        ReedlineEvent::Edit(vec![EditCommand::ClearSelection]),
    );
    keybindings.add_binding(
        KeyModifiers::ALT,
        KeyCode::Char(';'),
        ReedlineEvent::Edit(vec![EditCommand::SwapCursorAndAnchor]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('u'),
        ReedlineEvent::Edit(vec![EditCommand::Undo]),
    );
    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::Char('u'),
        ReedlineEvent::Edit(vec![EditCommand::Redo]),
    );
}

/// Helper: add a Normal-mode motion that resets the selection anchor first.
///
/// Emits `[ClearSelection, command]` so the anchor is re-established at the
/// current cursor position before the motion executes.
fn add_normal_motion(
    keybindings: &mut Keybindings,
    modifiers: KeyModifiers,
    key: char,
    command: EditCommand,
) {
    keybindings.add_binding(
        modifiers,
        KeyCode::Char(key),
        ReedlineEvent::Edit(vec![EditCommand::ClearSelection, command]),
    );
}

/// Returns the default keybindings for Helix normal mode.
///
/// Normal-mode motions reset the selection anchor before each movement so that
/// a fresh one-motion selection is created every time.
///
/// Motions: h, l, w, b, e, W, B, E
/// Goto (g prefix, handled in parse_event): gh, gl, gs
/// Shared: x, d, y, p/P, ;, Alt+;, u/U
pub fn default_helix_normal_keybindings() -> Keybindings {
    let mut kb = Keybindings::default();
    add_common_keybindings(&mut kb);

    // -- character motions --
    // h/l just move the cursor without selecting, keeping anchor == cursor.
    kb.add_binding(KeyModifiers::NONE, KeyCode::Char('h'), ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }]));
    kb.add_binding(KeyModifiers::NONE, KeyCode::Char('l'), ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: false }]));

    // -- word motions --
    add_normal_motion(&mut kb, KeyModifiers::NONE, 'w', EditCommand::MoveWordRight { select: true });
    add_normal_motion(&mut kb, KeyModifiers::NONE, 'b', EditCommand::MoveWordLeft { select: true });
    add_normal_motion(&mut kb, KeyModifiers::NONE, 'e', EditCommand::MoveWordRightEnd { select: true });

    // -- WORD motions --
    add_normal_motion(&mut kb, KeyModifiers::SHIFT, 'w', EditCommand::MoveBigWordRight { select: true });
    add_normal_motion(&mut kb, KeyModifiers::SHIFT, 'b', EditCommand::MoveBigWordLeft { select: true });
    add_normal_motion(&mut kb, KeyModifiers::SHIFT, 'e', EditCommand::MoveBigWordRightEnd { select: true });

    kb
}

/// Returns the default keybindings for Helix select mode.
///
/// Select-mode motions keep the existing anchor fixed and extend the selection,
/// so they emit the bare motion command without `ClearSelection`.
///
/// Motions: h, l, w, b, e, W, B, E
/// Goto (g prefix, handled in parse_event): gh, gl, gs
/// Shared:  Enter, Ctrl+C/D, x, d, y, p/P, ;, Alt+;, u/U
pub fn default_helix_select_keybindings() -> Keybindings {
    let mut kb = Keybindings::default();
    add_common_keybindings(&mut kb);

    // -- character motions --
    kb.add_binding(KeyModifiers::NONE, KeyCode::Char('h'), ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: true }]));
    kb.add_binding(KeyModifiers::NONE, KeyCode::Char('l'), ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: true }]));

    // -- word motions --
    kb.add_binding(KeyModifiers::NONE, KeyCode::Char('w'), ReedlineEvent::Edit(vec![EditCommand::MoveWordRight { select: true }]));
    kb.add_binding(KeyModifiers::NONE, KeyCode::Char('b'), ReedlineEvent::Edit(vec![EditCommand::MoveWordLeft { select: true }]));
    kb.add_binding(KeyModifiers::NONE, KeyCode::Char('e'), ReedlineEvent::Edit(vec![EditCommand::MoveWordRightEnd { select: true }]));

    // -- WORD motions --
    kb.add_binding(KeyModifiers::SHIFT, KeyCode::Char('w'), ReedlineEvent::Edit(vec![EditCommand::MoveBigWordRight { select: true }]));
    kb.add_binding(KeyModifiers::SHIFT, KeyCode::Char('b'), ReedlineEvent::Edit(vec![EditCommand::MoveBigWordLeft { select: true }]));
    kb.add_binding(KeyModifiers::SHIFT, KeyCode::Char('e'), ReedlineEvent::Edit(vec![EditCommand::MoveBigWordRightEnd { select: true }]));

    kb
}

/// Returns the default keybindings for Helix insert mode.
///
/// Includes:
/// - Backspace: delete previous character
/// - Ctrl+C: abort/exit
/// - Ctrl+D: exit/EOF
pub fn default_helix_insert_keybindings() -> Keybindings {
    let mut keybindings = Keybindings::default();

    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Backspace,
        ReedlineEvent::Edit(vec![crate::enums::EditCommand::Backspace]),
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        KeyCode::Char('c'),
        ReedlineEvent::CtrlC,
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        KeyCode::Char('d'),
        ReedlineEvent::CtrlD,
    );

    keybindings
}
