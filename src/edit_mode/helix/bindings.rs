use crossterm::event::{KeyCode, KeyModifiers};
use modalkit::keybindings::{
    EdgeEvent, EdgePath, EdgeRepeat, EmptyKeyClass, InputBindings,
};

use super::{
    key::HelixKey,
    mode::{HelixMachine, HelixMode, HelixStep},
};

#[derive(Default)]
pub(super) struct HelixBindings;

impl HelixBindings {
    fn add_single_keypress_mapping(
        machine: &mut HelixMachine,
        mode: HelixMode,
        code: KeyCode,
        step: HelixStep,
    ) {
        let path: &EdgePath<HelixKey, EmptyKeyClass> = &[(
            EdgeRepeat::Once,
            EdgeEvent::Key(HelixKey::new(code, KeyModifiers::NONE)),
        )];
        machine.add_mapping(mode, path, &step);
    }

    fn add_bindings(machine: &mut HelixMachine, mode: HelixMode, bindings: &[(KeyCode, HelixStep)]) {
        for (code, step) in bindings {
            Self::add_single_keypress_mapping(machine, mode, *code, step.clone());
        }
    }
}

impl InputBindings<HelixKey, HelixStep> for HelixBindings {
    fn setup(&self, machine: &mut HelixMachine) {
        let insert_bindings = [(KeyCode::Esc, (None, Some(HelixMode::Normal)))];
        let normal_bindings = [
            (KeyCode::Char('i'), (None, Some(HelixMode::Insert))),
            (
                KeyCode::Char('h'),
                (Some(super::action::HelixAction::MoveCharLeft), None),
            ),
            (
                KeyCode::Left,
                (Some(super::action::HelixAction::MoveCharLeft), None),
            ),
            (
                KeyCode::Char('l'),
                (Some(super::action::HelixAction::MoveCharRight), None),
            ),
            (
                KeyCode::Right,
                (Some(super::action::HelixAction::MoveCharRight), None),
            ),
            (
                KeyCode::Char('a'),
                (
                    Some(super::action::HelixAction::MoveCharRight),
                    Some(HelixMode::Insert),
                ),
            ),
        ];

        Self::add_bindings(machine, HelixMode::Insert, &insert_bindings);
        Self::add_bindings(machine, HelixMode::Normal, &normal_bindings);
    }
}