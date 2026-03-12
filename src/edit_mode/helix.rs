use crate::{
    enums::{EventStatus, ReedlineEvent, ReedlineRawEvent},
    PromptEditMode, PromptViMode,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use super::EditMode;

/// A minimal custom edit mode example for Helix-style integrations.
#[derive(Default)]
pub struct Helix;

impl EditMode for Helix {
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        match Event::from(event) {
            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }) => ReedlineEvent::CtrlC,
            _ => ReedlineEvent::None,
        }
    }

    fn edit_mode(&self) -> PromptEditMode {
        PromptEditMode::Vi(PromptViMode::Normal)
    }

    fn handle_mode_specific_event(&mut self, _event: ReedlineEvent) -> EventStatus {
        EventStatus::Inapplicable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PromptViMode;

    #[test]
    fn helix_edit_mode_defaults_to_normal_mode() {
        let helix_mode = Helix;

        let edit_mode = helix_mode.edit_mode();

        assert!(matches!(
            edit_mode,
            PromptEditMode::Vi(PromptViMode::Normal)
        ));
    }

    #[test]
    fn helix_edit_mode_parses_ctrl_c_event() {
        let mut helix_mode = Helix;
        let ctrl_c_raw_event = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )));

        assert_eq!(
            helix_mode.parse_event(ctrl_c_raw_event.unwrap()),
            ReedlineEvent::CtrlC
        );
    }
}
