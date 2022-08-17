use crossterm::event::{KeyCode, KeyEventKind, KeyEventState, KeyModifiers};

use crate::{
    edit_mode::{
        keybindings::{
            add_common_control_bindings, add_common_edit_bindings, add_common_navigation_bindings,
            edit_bind,
        },
        Keybindings,
    },
    EditCommand,
};

/// Default Vi normal keybindings
pub fn default_vi_normal_keybindings() -> Keybindings {
    let mut kb = Keybindings::new();
    use EditCommand as EC;
    use KeyCode as KC;
    use KeyEventKind as KEK;
    use KeyEventState as KES;
    use KeyModifiers as KM;

    add_common_control_bindings(&mut kb);
    add_common_navigation_bindings(&mut kb);
    // Replicate vi's default behavior for Backspace and delete
    kb.add_binding(
        KM::NONE,
        KC::Backspace,
        KEK::Press,
        KES::NONE,
        edit_bind(EC::MoveLeft),
    );
    kb.add_binding(
        KM::NONE,
        KC::Delete,
        KEK::Press,
        KES::NONE,
        edit_bind(EC::Delete),
    );

    kb
}

/// Default Vi insert keybindings
pub fn default_vi_insert_keybindings() -> Keybindings {
    let mut kb = Keybindings::new();

    add_common_control_bindings(&mut kb);
    add_common_navigation_bindings(&mut kb);
    add_common_edit_bindings(&mut kb);

    kb
}
