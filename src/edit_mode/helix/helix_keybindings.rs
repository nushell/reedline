use crate::{
    edit_mode::keybindings::Keybindings,
    enums::{EditCommand, ReedlineEvent},
};
use crossterm::event::{KeyCode, KeyModifiers};

fn add_normal_motion_binding(
    keybindings: &mut Keybindings,
    modifiers: KeyModifiers,
    key: char,
    command: EditCommand,
) {
    // In Normal mode, reset selection anchor before each motion
    // We move to the same position with select: false to clear the anchor
    keybindings.add_binding(
        modifiers,
        KeyCode::Char(key),
        ReedlineEvent::Edit(vec![
            EditCommand::MoveLeft { select: false },
            EditCommand::MoveRight { select: false },
            command,
        ]),
    );
}

fn add_select_motion_binding(
    keybindings: &mut Keybindings,
    modifiers: KeyModifiers,
    key: char,
    command: EditCommand,
) {
    // In Select mode, keep the anchor fixed
    keybindings.add_binding(
        modifiers,
        KeyCode::Char(key),
        ReedlineEvent::Edit(vec![command]),
    );
}

/// Returns the default keybindings for Helix normal mode
///
/// Includes:
/// - Enter: accept line
/// - Ctrl+C: abort/exit
/// - Ctrl+D: exit/EOF
/// - h/l: left/right (with selection)
/// - w/b/e: word motions (with selection)
/// - W/B/E: WORD motions (with selection)
/// - 0/$: line start/end (with selection)
/// - x: select line
/// - d: delete selection
/// - c: change selection (delete and enter insert mode)
/// - y: yank/copy selection
/// - p/P: paste after/before
/// - ;: collapse selection
/// - Alt+;: swap cursor and anchor
/// - u/U: undo/redo
pub fn default_helix_normal_keybindings() -> Keybindings {
    let mut keybindings = Keybindings::default();

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

    add_normal_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'h',
        EditCommand::MoveLeft { select: true },
    );
    add_normal_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'l',
        EditCommand::MoveRight { select: true },
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('w'),
        ReedlineEvent::Edit(vec![EditCommand::HelixWordRightGap]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('b'),
        ReedlineEvent::Edit(vec![EditCommand::HelixWordLeft]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('e'),
        ReedlineEvent::Edit(vec![
            EditCommand::ClearSelection,
            EditCommand::MoveWordRightEnd { select: true },
        ]),
    );
    add_normal_motion_binding(
        &mut keybindings,
        KeyModifiers::SHIFT,
        'w',
        EditCommand::MoveBigWordRightStart { select: true },
    );
    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::Char('b'),
        ReedlineEvent::Edit(vec![
            EditCommand::MoveBigWordLeft { select: false },
            EditCommand::MoveBigWordRightEnd { select: true },
            EditCommand::SwapCursorAndAnchor,
        ]),
    );
    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::Char('e'),
        ReedlineEvent::Edit(vec![
            EditCommand::ClearSelection,
            EditCommand::MoveBigWordRightEnd { select: true },
        ]),
    );
    add_normal_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        '0',
        EditCommand::MoveToLineStart { select: true },
    );
    add_normal_motion_binding(
        &mut keybindings,
        KeyModifiers::SHIFT,
        '$',
        EditCommand::MoveToLineEnd { select: true },
    );

    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('x'),
        ReedlineEvent::Edit(vec![EditCommand::SelectAll]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('d'),
        ReedlineEvent::Edit(vec![EditCommand::CutSelection]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('c'),
        ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::CutSelection])]),
    );
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
        ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: false }]),
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

    keybindings
}

/// Returns the default keybindings for Helix select mode
///
/// In Select mode, the selection anchor stays fixed and motions extend from it.
/// Includes the same motions as Normal mode, but without anchor reset.
pub fn default_helix_select_keybindings() -> Keybindings {
    let mut keybindings = Keybindings::default();

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

    add_select_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'h',
        EditCommand::MoveLeft { select: true },
    );
    add_select_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'l',
        EditCommand::MoveRight { select: true },
    );
    add_select_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'w',
        EditCommand::MoveWordRightStart { select: true },
    );
    add_select_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'b',
        EditCommand::MoveWordLeft { select: true },
    );
    add_select_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'e',
        EditCommand::MoveWordRightEnd { select: true },
    );
    add_select_motion_binding(
        &mut keybindings,
        KeyModifiers::SHIFT,
        'w',
        EditCommand::MoveBigWordRightStart { select: true },
    );
    add_select_motion_binding(
        &mut keybindings,
        KeyModifiers::SHIFT,
        'b',
        EditCommand::MoveBigWordLeft { select: true },
    );
    add_select_motion_binding(
        &mut keybindings,
        KeyModifiers::SHIFT,
        'e',
        EditCommand::MoveBigWordRightEnd { select: true },
    );
    add_select_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        '0',
        EditCommand::MoveToLineStart { select: true },
    );
    add_select_motion_binding(
        &mut keybindings,
        KeyModifiers::SHIFT,
        '$',
        EditCommand::MoveToLineEnd { select: true },
    );

    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('x'),
        ReedlineEvent::Edit(vec![EditCommand::SelectAll]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('d'),
        ReedlineEvent::Edit(vec![EditCommand::CutSelection]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('c'),
        ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::CutSelection])]),
    );
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
        ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: false }]),
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

    keybindings
}

/// Returns the default keybindings for Helix insert mode
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
