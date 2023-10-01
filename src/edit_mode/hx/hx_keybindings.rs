use crate::edit_mode::{
    keybindings::{
        add_common_control_bindings, add_common_edit_bindings, add_common_navigation_bindings,
    },
    Keybindings,
};

/// Default Hx normal keybindings
pub fn default_hx_normal_keybindings() -> Keybindings {
    let mut kb = Keybindings::new();

    add_common_control_bindings(&mut kb);
    add_common_navigation_bindings(&mut kb);
    kb
}

/// Default Hx insert keybindings
pub fn default_hx_insert_keybindings() -> Keybindings {
    let mut kb = Keybindings::new();

    add_common_control_bindings(&mut kb);
    add_common_navigation_bindings(&mut kb);
    add_common_edit_bindings(&mut kb);

    kb
}
