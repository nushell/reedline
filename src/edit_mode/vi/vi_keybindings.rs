use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
    edit_mode::{
        keybindings::{
            add_common_control_bindings, add_common_edit_bindings, add_common_navigation_bindings,
            add_common_selection_bindings, edit_bind,
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
    use KeyModifiers as KM;

    add_common_control_bindings(&mut kb);
    add_common_navigation_bindings(&mut kb);
    add_common_selection_bindings(&mut kb);
    // Replicate vi's default behavior for Backspace and delete
    kb.add_binding(
        KM::NONE,
        KC::Backspace,
        edit_bind(EC::MoveLeft { select: false }),
    );
    kb.add_binding(KM::NONE, KC::Delete, edit_bind(EC::Delete));

    kb
}

/// Default Vi insert keybindings
pub fn default_vi_insert_keybindings() -> Keybindings {
    let mut kb = Keybindings::new();

    add_common_control_bindings(&mut kb);
    add_common_navigation_bindings(&mut kb);
    add_common_edit_bindings(&mut kb);
    add_common_selection_bindings(&mut kb);

    kb
}
