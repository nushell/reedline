use crate::{
    edit_mode::keybindings::Keybindings,
    enums::{EditCommand, ReedlineEvent},
};
use crossterm::event::{KeyCode, KeyModifiers};

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

    // Basic commands
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

    // Character motions (with selection in Helix style)
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('h'),
        ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: true }]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('l'),
        ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: true }]),
    );

    // Word motions
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('w'),
        ReedlineEvent::Edit(vec![EditCommand::MoveWordRightStart { select: true }]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('b'),
        ReedlineEvent::Edit(vec![EditCommand::MoveWordLeft { select: true }]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('e'),
        ReedlineEvent::Edit(vec![EditCommand::MoveWordRightEnd { select: true }]),
    );

    // Line motions
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Char('0'),
        ReedlineEvent::Edit(vec![EditCommand::MoveToLineStart { select: true }]),
    );
    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::Char('$'),
        ReedlineEvent::Edit(vec![EditCommand::MoveToLineEnd { select: true }]),
    );

    // Selection commands
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
        ReedlineEvent::Multiple(vec![
            ReedlineEvent::Edit(vec![EditCommand::CutSelection]),
            // Mode will be switched to Insert by the i key handler in parse_event
        ]),
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
        KeyCode::Char('P'),
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
