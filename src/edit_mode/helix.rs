pub use super::hx::Helix;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EditMode, PromptEditMode, PromptHelixMode, ReedlineEvent, ReedlineRawEvent};
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn helix_edit_mode_defaults_to_normal_mode() {
        let helix_mode = Helix::default();

        let edit_mode = helix_mode.edit_mode();

        assert!(matches!(
            edit_mode,
            PromptEditMode::Helix(PromptHelixMode::Normal)
        ));
    }

    #[test]
    fn helix_edit_mode_parses_ctrl_c_event() {
        let mut helix_mode = Helix::default();
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
