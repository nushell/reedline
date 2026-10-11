use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
    edit_mode::keybindings::{
        add_common_control_bindings, add_common_edit_bindings, add_common_navigation_bindings,
        add_common_selection_bindings, add_extending_navigation_bindings, edit_bind, Keybindings,
    },
    EditCommand, Granularity, PromptEditMode, PromptViMode, ReedlineEvent,
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

/// Default Vi visual keybindings: the normal table with its navigation keys
/// rebound to extend the selection.
///
/// Normal's bindings move with `select: false`, which in visual drops the
/// selection and starts a new one under the cursor, so here every key does
/// what its modal twin does in visual. Up and Down never reach history and no
/// key accepts a history hint, as in helix select, which shares the
/// rebinding. Layer custom bindings onto this table rather than onto the
/// normal one.
pub fn default_vi_visual_keybindings() -> Keybindings {
    use EditCommand as EC;
    use KeyCode as KC;
    use KeyModifiers as KM;

    let mut kb = default_vi_normal_keybindings();

    add_extending_navigation_bindings(
        &mut kb,
        EC::MoveLeft { select: true },
        EC::MoveRight { select: true },
    );
    // `d`: take the selection and return to normal. A binding cannot move the
    // machine itself, so the mode change rides along as an event.
    kb.add_binding(
        KM::NONE,
        KC::Delete,
        ReedlineEvent::Multiple(vec![
            edit_bind(EC::CutSelection {
                granularity: Granularity::CharWise,
            }),
            ReedlineEvent::SwitchMode(PromptEditMode::Vi(PromptViMode::Normal)),
        ]),
    );

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
