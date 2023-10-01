use crossterm::event::{KeyCode, KeyModifiers};

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

/// Default hx normal keybindings
pub fn default_hx_normal_keybindings() -> Keybindings {
    let mut kb = Keybindings::new();
    use EditCommand as EC;
    use KeyCode as KC;
    use KeyModifiers as KM;

    add_common_control_bindings(&mut kb);
    add_common_navigation_bindings(&mut kb);
    kb
}

/// Default Vi insert keybindings
pub fn default_hx_insert_keybindings() -> Keybindings {
    let mut kb = Keybindings::new();

    add_common_control_bindings(&mut kb);
    add_common_navigation_bindings(&mut kb);
    add_common_edit_bindings(&mut kb);

    kb
}
