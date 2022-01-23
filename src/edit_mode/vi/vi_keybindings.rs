use crate::{
    edit_mode::{keybindings::add_common_keybindings, Keybindings},
    ReedlineEvent,
};

use crossterm::event::{KeyCode as KC, KeyModifiers as KM};

/// Default Vi normal keybindings
pub fn default_vi_normal_keybindings() -> Keybindings {
    let mut kb = Keybindings::new();

    kb.add_binding(KM::CONTROL, KC::Char('c'), ReedlineEvent::CtrlC);

    kb
}

/// Default Vi insert keybindings
pub fn default_vi_insert_keybindings() -> Keybindings {
    let mut kb = Keybindings::new();

    add_common_keybindings(&mut kb);

    kb
}
