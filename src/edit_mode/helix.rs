use crate::{
    edit_mode::EditMode,
    enums::{EditCommand, ReedlineEvent, ReedlineRawEvent},
    PromptEditMode, PromptViMode,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use modalkit::keybindings::{
    BindingMachine, EdgeEvent, EdgePath, EdgeRepeat, EmptyKeyClass, EmptyKeyState, InputBindings,
    InputKey, ModalMachine, Mode, ModeKeys,
};

#[derive(Clone, Copy, Debug, Default, Hash, Eq, PartialEq)]
enum HelixMode {
    #[default]
    Insert,
    Normal,
}

/// A simple `InputKey` implementation around `crossterm` types.
///
/// This avoids pulling in the `crossterm` types used by `modalkit` (which can be a different
/// version than the one used by `reedline`).
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
struct HelixKey {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl HelixKey {
    fn new(mut code: KeyCode, mut modifiers: KeyModifiers) -> Self {
        if let KeyCode::Char(ref mut c) = code {
            if modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL) {
                *c = c.to_ascii_uppercase();
            } else if c.is_ascii_uppercase() {
                modifiers.insert(KeyModifiers::SHIFT);
            }

            if modifiers == KeyModifiers::SHIFT && *c != ' ' {
                modifiers -= KeyModifiers::SHIFT;
            }
        }

        Self { code, modifiers }
    }

    fn from_event(event: KeyEvent) -> Self {
        Self::new(event.code, event.modifiers)
    }

    fn get_char(&self) -> Option<char> {
        if let KeyCode::Char(c) = self.code {
            if (self.modifiers - KeyModifiers::SHIFT).is_empty() {
                return Some(c);
            }
        }

        None
    }
}

impl InputKey for HelixKey {
    type Error = std::convert::Infallible;

    fn decompose(&mut self) -> Option<Self> {
        None
    }

    fn from_macro_str(mstr: &str) -> Result<Vec<Self>, Self::Error> {
        Ok(mstr
            .chars()
            .map(|c| HelixKey::new(KeyCode::Char(c), KeyModifiers::NONE))
            .collect())
    }

    fn get_char(&self) -> Option<char> {
        self.get_char()
    }
}

impl Mode<HelixAction, EmptyKeyState> for HelixMode {}

impl From<PromptViMode> for HelixMode {
    fn from(mode: PromptViMode) -> Self {
        match mode {
            PromptViMode::Insert => HelixMode::Insert,
            PromptViMode::Normal => HelixMode::Normal,
        }
    }
}

impl ModeKeys<HelixKey, HelixAction, EmptyKeyState> for HelixMode {
    fn unmapped(
        &self,
        key: &HelixKey,
        _: &mut EmptyKeyState,
    ) -> (Vec<HelixAction>, Option<HelixMode>) {
        match self {
            HelixMode::Normal => (vec![], None),
            HelixMode::Insert => {
                if let Some(c) = key.get_char() {
                    return (vec![HelixAction::Type(c)], None);
                }

                (vec![], None)
            }
        }
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
enum HelixAction {
    Type(char),
    MoveCharRight,
    MoveCharLeft,
    #[default]
    NoOp,
}

impl HelixAction {
    fn into_reedline_event(self) -> Option<ReedlineEvent> {
        match self {
            HelixAction::Type(c) => Some(ReedlineEvent::Edit(vec![EditCommand::InsertChar(c)])),
            HelixAction::MoveCharLeft => Some(ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
            ])),
            HelixAction::MoveCharRight => Some(ReedlineEvent::Edit(vec![
                EditCommand::MoveRight { select: false },
            ])),
            HelixAction::NoOp => None,
        }
    }
}

type HelixStep = (Option<HelixAction>, Option<HelixMode>);

type HelixEdgePath = EdgePath<HelixKey, EmptyKeyClass>;

type HelixMachine = ModalMachine<HelixKey, HelixStep>;

#[derive(Default)]
struct HelixBindings;

impl HelixBindings {
    fn add_single_keypress_mapping(
        machine: &mut HelixMachine,
        mode: HelixMode,
        code: KeyCode,
        step: HelixStep,
    ) {
        let path: &HelixEdgePath = &[(
            EdgeRepeat::Once,
            EdgeEvent::Key(HelixKey::new(code, KeyModifiers::NONE)),
        )];
        machine.add_mapping(mode, path, &step);
    }
}

impl InputBindings<HelixKey, HelixStep> for HelixBindings {
    fn setup(&self, machine: &mut HelixMachine) {
        Self::add_single_keypress_mapping(
            machine,
            HelixMode::Insert,
            KeyCode::Esc,
            (None, Some(HelixMode::Normal)),
        );
        Self::add_single_keypress_mapping(
            machine,
            HelixMode::Normal,
            KeyCode::Char('i'),
            (None, Some(HelixMode::Insert)),
        );
        for code in [KeyCode::Char('h'), KeyCode::Left] {
            Self::add_single_keypress_mapping(
                machine,
                HelixMode::Normal,
                code,
                (Some(HelixAction::MoveCharLeft), None),
            );
        }
        for code in [KeyCode::Char('l'), KeyCode::Right] {
            Self::add_single_keypress_mapping(
                machine,
                HelixMode::Normal,
                code,
                (Some(HelixAction::MoveCharRight), None),
            );
        }
        Self::add_single_keypress_mapping(
            machine,
            HelixMode::Normal,
            KeyCode::Char('a'),
            (Some(HelixAction::MoveCharRight), Some(HelixMode::Insert)),
        );
    }
}

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
        self.machine.input_key(HelixKey::from_event(key_event));

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
