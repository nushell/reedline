use crate::{
    edit_mode::keybindings::Keybindings,
    enums::{EditCommand, ReedlineEvent},
};
use crossterm::event::{KeyCode, KeyModifiers};

fn add_motion_binding(
    keybindings: &mut Keybindings,
    modifiers: KeyModifiers,
    key: char,
    command: EditCommand,
) {
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
/// - 0/$: line start/end (with selection)
/// - x: select line
/// - d: delete selection
/// - c: change selection (delete and enter insert mode)
/// - y: yank/copy selection
/// - p/P: paste after/before
/// - ;: collapse selection
/// - Alt+;: swap cursor and anchor
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

    add_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'h',
        EditCommand::MoveLeft { select: true },
    );
    add_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'l',
        EditCommand::MoveRight { select: true },
    );
    add_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'w',
        EditCommand::MoveWordRightStart { select: true },
    );
    add_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'b',
        EditCommand::MoveWordLeft { select: true },
    );
    add_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'e',
        EditCommand::MoveWordRightEnd { select: true },
    );
    add_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        '0',
        EditCommand::MoveToLineStart { select: true },
    );
    add_motion_binding(
        &mut keybindings,
        KeyModifiers::SHIFT,
        '$',
        EditCommand::MoveToLineEnd { select: true },
    );

    add_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'x',
        EditCommand::SelectAll,
    );
    add_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'd',
        EditCommand::CutSelection,
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('c'),
        ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::CutSelection])]),
    );
    add_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'y',
        EditCommand::CopySelection,
    );
    add_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        'p',
        EditCommand::Paste,
    );
    add_motion_binding(
        &mut keybindings,
        KeyModifiers::SHIFT,
        'P',
        EditCommand::PasteCutBufferBefore,
    );
    add_motion_binding(
        &mut keybindings,
        KeyModifiers::NONE,
        ';',
        EditCommand::MoveRight { select: false },
    );
    add_motion_binding(
        &mut keybindings,
        KeyModifiers::ALT,
        ';',
        EditCommand::SwapCursorAndAnchor,
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
