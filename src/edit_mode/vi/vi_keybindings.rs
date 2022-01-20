use crate::{
    edit_mode::{keybindings::edit_bind, Keybindings},
    ReedlineEvent,
};

use {
    crate::EditCommand as EC,
    crossterm::event::{KeyCode as KC, KeyModifiers as KM},
};

/// Default Vi normal keybindings
pub fn default_vi_normal_keybindings() -> Keybindings {
    let mut kb = Keybindings::new();

    kb.add_binding(KM::CONTROL, KC::Char('c'), vec![ReedlineEvent::CtrlC]);

    kb
}

/// Default Vi insert keybindings
pub fn default_vi_insert_keybindings() -> Keybindings {
    let mut kb = Keybindings::new();

    kb.add_binding(
        KM::NONE,
        KC::Up,
        vec![ReedlineEvent::Up, ReedlineEvent::MenuUp],
    );
    kb.add_binding(
        KM::NONE,
        KC::Down,
        vec![ReedlineEvent::Down, ReedlineEvent::MenuDown],
    );
    kb.add_binding(
        KM::NONE,
        KC::Left,
        vec![ReedlineEvent::Left, ReedlineEvent::MenuLeft],
    );
    kb.add_binding(
        KM::NONE,
        KC::Right,
        vec![
            ReedlineEvent::Complete,
            ReedlineEvent::MenuRight,
            ReedlineEvent::Right,
        ],
    );

    kb.add_binding(KM::NONE, KC::Backspace, vec![edit_bind(EC::Backspace)]);
    kb.add_binding(KM::NONE, KC::Delete, vec![edit_bind(EC::Delete)]);
    kb.add_binding(KM::NONE, KC::End, vec![edit_bind(EC::MoveToLineEnd)]);
    kb.add_binding(KM::NONE, KC::Home, vec![edit_bind(EC::MoveToLineStart)]);

    kb.add_binding(KM::CONTROL, KC::Char('c'), vec![ReedlineEvent::CtrlC]);
    kb.add_binding(
        KM::CONTROL,
        KC::Char('r'),
        vec![ReedlineEvent::SearchHistory],
    );

    kb.add_binding(
        KM::NONE,
        KC::Tab,
        vec![ReedlineEvent::ContextMenu, ReedlineEvent::MenuNext],
    );

    kb.add_binding(KM::SHIFT, KC::BackTab, vec![ReedlineEvent::MenuPrevious]);

    kb
}
