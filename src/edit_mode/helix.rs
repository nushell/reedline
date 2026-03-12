use crate::{
    edit_mode::EditMode,
    enums::{ReedlineEvent, ReedlineRawEvent},
    PromptEditMode, PromptViMode,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum HelixMode {
    #[default]
    Insert,
    Normal,
}

/// A minimal custom edit mode example for Helix-style integrations.
#[derive(Default)]
pub struct Helix {
    mode: HelixMode,
}

impl Helix {
    #[cfg(test)]
    pub(crate) fn insert() -> Self {
        Self {
            mode: HelixMode::Insert,
        }
    }

    #[cfg(test)]
    pub(crate) fn normal() -> Self {
        Self {
            mode: HelixMode::Normal,
        }
    }
}

impl EditMode for Helix {
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        match event.into() {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => match (self.mode, modifiers, code) {
                (_, KeyModifiers::CONTROL, KeyCode::Char('c')) => ReedlineEvent::CtrlC,
                (HelixMode::Insert, _, KeyCode::Esc) => {
                    self.mode = HelixMode::Normal;
                    ReedlineEvent::Repaint
                }
                (HelixMode::Normal, _, KeyCode::Char('i')) => {
                    self.mode = HelixMode::Insert;
                    ReedlineEvent::Repaint
                }
                _ => ReedlineEvent::None,
            },
            _ => ReedlineEvent::None,
        }
    }

    fn edit_mode(&self) -> PromptEditMode {
        match self.mode {
            HelixMode::Insert => PromptEditMode::Vi(PromptViMode::Insert),
            HelixMode::Normal => PromptEditMode::Vi(PromptViMode::Normal),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn key_press(code: KeyCode, modifiers: KeyModifiers) -> ReedlineRawEvent {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
        .try_into()
        .unwrap()
    }

    #[test]
    fn helix_editor_defaults_to_insert_mode() {
        let helix_editor = Helix::default();

        assert_eq!(helix_editor.mode, HelixMode::Insert);
    }

    #[test]
    fn normal_mode_parses_ctrl_c_event() {
        let mut helix_mode = Helix::normal();

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            ReedlineEvent::CtrlC
        );
    }

    #[test]
    fn pressing_esc_in_insert_mode_switches_to_normal() {
        let mut helix_mode = Helix::insert();
        helix_mode.parse_event(key_press(KeyCode::Esc, KeyModifiers::NONE));

        assert_eq!(helix_mode.mode, HelixMode::Normal);
    }

    #[test]
    fn pressing_i_in_normal_mode_switches_to_insert() {
        let mut helix_mode = Helix::normal();
        helix_mode.parse_event(key_press(KeyCode::Char('i'), KeyModifiers::NONE));

        assert_eq!(helix_mode.mode, HelixMode::Insert);
    }
}
