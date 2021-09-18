use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use super::EditMode;
use crate::{
    edit_mode::keybindings::{
        default_vi_insert_keybindings, default_vi_normal_keybindings, Keybindings,
    },
    enums::{EditCommand, ReedlineEvent},
    PromptEditMode, PromptViMode,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Mode {
    Normal,
    Insert,
}

/// This parses incomming input `Event`s like a Vi-Style editor
pub struct Vi {
    partial: Option<String>,
    insert_keybindings: Keybindings,
    normal_keybindings: Keybindings,
    mode: Mode,
}

impl Default for Vi {
    fn default() -> Self {
        Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: default_vi_normal_keybindings(),
            partial: None,
            mode: Mode::Normal,
        }
    }
}

impl EditMode for Vi {
    fn parse_event(&mut self, event: Event) -> ReedlineEvent {
        match event {
            Event::Key(KeyEvent { code, modifiers }) => match (self.mode, modifiers, code) {
                (_, KeyModifiers::NONE, KeyCode::Tab) => ReedlineEvent::HandleTab,
                (_, KeyModifiers::NONE, KeyCode::Esc) => {
                    self.mode = Mode::Normal;
                    ReedlineEvent::Repaint
                }
                (Mode::Normal, KeyModifiers::NONE, KeyCode::Char(c)) => self.parse_vi_fragment(c),
                (Mode::Normal, KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                    self.parse_vi_fragment(c.to_ascii_uppercase())
                }
                (Mode::Insert, KeyModifiers::NONE, KeyCode::Char(c)) => {
                    ReedlineEvent::Edit(vec![EditCommand::InsertChar(c)])
                }
                (Mode::Insert, KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                    ReedlineEvent::Edit(vec![EditCommand::InsertChar(c.to_ascii_uppercase())])
                }
                (_, KeyModifiers::NONE, KeyCode::Enter) => ReedlineEvent::Enter,
                (Mode::Normal, _, _) => self
                    .normal_keybindings
                    .find_binding(modifiers, code)
                    .unwrap_or_else(|| ReedlineEvent::Edit(vec![])),
                (Mode::Insert, _, _) => self
                    .insert_keybindings
                    .find_binding(modifiers, code)
                    .unwrap_or_else(|| ReedlineEvent::Edit(vec![])),
            },

            Event::Mouse(_) => ReedlineEvent::Mouse,
            Event::Resize(width, height) => ReedlineEvent::Resize(width, height),
        }
    }

    fn edit_mode(&self) -> PromptEditMode {
        match self.mode {
            Mode::Normal => PromptEditMode::Vi(PromptViMode::Normal),
            Mode::Insert => PromptEditMode::Vi(PromptViMode::Insert),
        }
    }
}

impl Vi {
    fn parse_vi_fragment(&mut self, fragment: char) -> ReedlineEvent {
        let mut output = vec![];

        let partial = self.partial.clone();

        match (partial, fragment) {
            (None, c) => match c {
                'd' => self.partial = Some("d".to_string()),
                'p' => {
                    output.push(EditCommand::PasteCutBuffer);
                }
                'h' => {
                    output.push(EditCommand::MoveLeft);
                }
                'l' => {
                    output.push(EditCommand::MoveRight);
                }
                'j' => {
                    // j in normal mode is not an editor command but it prompts us to execute the
                    // down routine
                    return ReedlineEvent::Down;
                }
                'k' => {
                    // k in normal mode is not an editor command but it prompts us to execute the
                    // up routine
                    return ReedlineEvent::Up;
                }
                'w' => {
                    output.push(EditCommand::MoveWordRight);
                }
                'b' => {
                    output.push(EditCommand::MoveWordLeft);
                }
                '0' => {
                    output.push(EditCommand::MoveToStart);
                }
                '$' => {
                    output.push(EditCommand::MoveToEnd);
                }
                'A' => {
                    output.push(EditCommand::MoveToEnd);
                    self.mode = Mode::Insert;
                }
                'D' => {
                    output.push(EditCommand::CutToEnd);
                }
                'u' => {
                    output.push(EditCommand::Undo);
                }
                'i' => {
                    // NOTE: Ability to handle this with multiple events
                    // Best to target this once the ViParser is in fully working state
                    self.mode = Mode::Insert;
                    return ReedlineEvent::Repaint;
                }
                _ => {}
            },
            (Some(partial), c) => {
                if partial == "d" {
                    match c {
                        'd' => {
                            output.push(EditCommand::MoveToStart);
                            output.push(EditCommand::CutToEnd);
                        }
                        'w' => {
                            output.push(EditCommand::CutWordRight);
                        }
                        _ => {}
                    }
                }
                self.partial = None;
            }
        };

        ReedlineEvent::Edit(output)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn edit_event(cmd: EditCommand) -> ReedlineEvent {
        ReedlineEvent::Edit(vec![cmd])
    }

    fn char_key_event(ch: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE))
    }

    #[rstest]
    #[case('h', edit_event(EditCommand::MoveLeft))]
    #[case('j', ReedlineEvent::Down)]
    #[case('k', ReedlineEvent::Up)]
    #[case('l', edit_event(EditCommand::MoveRight))]
    #[case('u', edit_event(EditCommand::Undo))]
    #[case('p', edit_event(EditCommand::PasteCutBuffer))]
    #[case('w', edit_event(EditCommand::MoveWordRight))]
    #[case('b', edit_event(EditCommand::MoveWordLeft))]
    #[case('0', edit_event(EditCommand::MoveToStart))]
    #[case('$', edit_event(EditCommand::MoveToEnd))]
    #[case('A', edit_event(EditCommand::MoveToEnd))] // Not checking if it also moves to end
    #[case('D', edit_event(EditCommand::CutToEnd))]
    fn test_single_word_vi_commands(#[case] input: char, #[case] output: ReedlineEvent) {
        let mut default_vi = Vi::default();

        let event = char_key_event(input);
        let result = default_vi.parse_event(event);

        assert_eq!(result, output);
    }

    #[rstest]
    #[case("dd", ReedlineEvent::Edit(vec![EditCommand::MoveToStart, EditCommand::CutToEnd]))]
    #[case("dw", edit_event(EditCommand::CutWordRight))]
    fn test_multiple_word_vi_commands(#[case] input: &str, #[case] output: ReedlineEvent) {
        let mut default_vi = Vi::default();

        let events = input.chars().map(char_key_event);

        // Ideally this should be a fold but map works as vi has internal state
        let result = events.map(|e| default_vi.parse_event(e)).last().unwrap();

        assert_eq!(result, output);
    }

    #[test]
    fn hitting_i_in_normal_mode_switches_the_mode() {
        let mut default_vi = Vi::default();
        let i = char_key_event('i');
        let result = default_vi.parse_event(i);
        assert_eq!(result, ReedlineEvent::Repaint);
        assert_eq!(default_vi.mode, Mode::Insert);
    }

    #[test]
    fn hitting_esc_in_insert_mode_switches_the_mode() {
        let mut vi = Vi {
            insert_keybindings: Keybindings::empty(),
            normal_keybindings: Keybindings::empty(),
            partial: None,
            mode: Mode::Normal,
        };

        let esc = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let result = vi.parse_event(esc);

        assert_eq!(result, ReedlineEvent::Repaint);
        assert_eq!(vi.mode, Mode::Normal);
    }

    #[test]
    fn ctrl_l_leads_to_clear_screen_event_in_insert_mode() {
        let mut vi = Vi {
            insert_keybindings: default_vi_insert_mode_keybindings(),
            normal_keybindings: Keybindings::empty(),
            partial: None,
            mode: Mode::Insert,
        };
        let ctrl_l = Event::Key(KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            code: KeyCode::Char('l'),
        });
        let result = vi.parse_event(ctrl_l);

        assert_eq!(result, ReedlineEvent::ClearScreen);
    }
}
