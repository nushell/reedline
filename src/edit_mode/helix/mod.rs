mod action;
mod bindings;
mod key;
mod mode;

use crate::{
    edit_mode::EditMode,
    enums::{ReedlineEvent, ReedlineRawEvent},
    PromptEditMode, PromptViMode,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use modalkit::keybindings::BindingMachine;

use self::{
    action::HelixAction,
    bindings::HelixBindings,
    key::HelixKey,
    mode::{HelixMachine, HelixMode},
};

/// A minimal custom edit mode example for Helix-style integrations.
pub struct Helix {
    machine: HelixMachine,
}

impl std::fmt::Debug for Helix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Helix")
            .field("mode", &self.machine.mode())
            .finish_non_exhaustive()
    }
}

impl Default for Helix {
    fn default() -> Self {
        Self::new(PromptViMode::Insert)
    }
}

impl Helix {
    /// Creates a Helix editor with the requested initial mode.
    pub fn new(initial_mode: PromptViMode) -> Self {
        let mut machine = HelixMachine::from_bindings::<HelixBindings>();
        Self::initialize_mode(&mut machine, initial_mode.into());

        Self { machine }
    }

    fn initialize_mode(machine: &mut HelixMachine, mode: HelixMode) {
        if mode == HelixMode::Insert {
            return;
        }

        machine.input_key(HelixKey::new(KeyCode::Esc, KeyModifiers::NONE));
        let _ = machine.pop();

        debug_assert_eq!(machine.mode(), mode);
    }

    fn key_event_from_raw(event: ReedlineRawEvent) -> Option<KeyEvent> {
        KeyEvent::try_from(event).ok()
    }

    fn is_interrupt_event(key_event: &KeyEvent) -> bool {
        matches!(
            key_event,
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
        )
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) -> ReedlineEvent {
        if Self::is_interrupt_event(&key_event) {
            return ReedlineEvent::CtrlC;
        }

        let (action, mode_changed) = self.apply_key_event(key_event);

        action
            .and_then(HelixAction::into_reedline_event)
            .unwrap_or_else(|| Self::mode_change_event(mode_changed))
    }

    fn apply_key_event(&mut self, key_event: KeyEvent) -> (Option<HelixAction>, bool) {
        let previous_mode = self.machine.mode();
        self.machine.input_key(key_event.into());

        let mode_changed = self.machine.mode() != previous_mode;
        let action = self.machine.pop().map(|(action, _ctx)| action);

        (action, mode_changed)
    }

    fn mode_change_event(mode_changed: bool) -> ReedlineEvent {
        if mode_changed {
            ReedlineEvent::Repaint
        } else {
            ReedlineEvent::None
        }
    }
}

impl EditMode for Helix {
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        let Some(key_event) = Self::key_event_from_raw(event) else {
            return ReedlineEvent::None;
        };

        self.handle_key_event(key_event)
    }

    fn edit_mode(&self) -> PromptEditMode {
        match self.machine.mode() {
            HelixMode::Insert => PromptEditMode::Vi(PromptViMode::Insert),
            HelixMode::Normal => PromptEditMode::Vi(PromptViMode::Normal),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::EditCommand;
    use crossterm::event::{Event, KeyEventKind, KeyEventState};
    use rstest::rstest;

    fn key_press(code: KeyCode, modifiers: KeyModifiers) -> ReedlineRawEvent {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
        .try_into()
        .expect("valid crossterm key event")
    }

    #[test]
    fn helix_editor_defaults_to_insert_mode() {
        let helix_editor = Helix::default();

        assert!(matches!(
            helix_editor.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Insert)
        ));
    }

    #[test]
    fn helix_editor_can_start_in_normal_mode() {
        let helix_editor = Helix::new(PromptViMode::Normal);

        assert!(matches!(
            helix_editor.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Normal)
        ));
    }

    #[test]
    fn ctrl_c_maps_to_interrupt_event() {
        let mut helix_mode = Helix::default();

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            ReedlineEvent::CtrlC
        );
    }

    #[test]
    fn pressing_esc_in_insert_mode_switches_to_normal() {
        let mut helix_mode = Helix::new(PromptViMode::Insert);

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Esc, KeyModifiers::NONE)),
            ReedlineEvent::Repaint
        );

        assert!(matches!(
            helix_mode.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Normal)
        ));
    }

    #[test]
    fn pressing_i_in_normal_mode_switches_to_insert() {
        let mut helix_mode = Helix::new(PromptViMode::Normal);

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Char('i'), KeyModifiers::NONE)),
            ReedlineEvent::Repaint
        );
        assert!(matches!(
            helix_mode.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Insert)
        ));
    }

    #[test]
    fn pressing_a_in_normal_mode_switches_to_insert_with_cursor_after_selection() {
        let mut helix_mode = Helix::new(PromptViMode::Normal);

        let event_result =
            helix_mode.parse_event(key_press(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(matches!(
            helix_mode.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Insert)
        ));
        assert_eq!(
            event_result,
            ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: false },])
        );
    }

    #[test]
    fn typing_in_insert_mode_produces_insert_char_event() {
        let mut helix_mode = Helix::new(PromptViMode::Insert);

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Char('a'), KeyModifiers::NONE)),
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('a')])
        );
    }

    #[rstest]
    #[case(KeyCode::Char('h'))]
    #[case(KeyCode::Left)]
    fn pressing_left_key_or_h_in_normal_mode_moves_cursor_left(#[case] key_code: KeyCode) {
        let mut helix_mode = Helix::new(PromptViMode::Normal);

        assert_eq!(
            helix_mode.parse_event(key_press(key_code, KeyModifiers::NONE)),
            ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }])
        );
    }

    #[rstest]
    #[case(KeyCode::Char('l'))]
    #[case(KeyCode::Right)]
    fn pressing_right_key_or_l_in_normal_mode_moves_cursor_right(#[case] key_code: KeyCode) {
        let mut helix_mode = Helix::new(PromptViMode::Normal);

        assert_eq!(
            helix_mode.parse_event(key_press(key_code, KeyModifiers::NONE)),
            ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: false }])
        );
    }
}