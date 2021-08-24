use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::{
    default_emacs_keybindings,
    enums::{EditCommand, ReedlineEvent},
    PromptEditMode, PromptViMode,
};

use super::{keybindings::Keybindings, InputParser};

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Normal,
    Insert,
}

pub struct ViInputParser {
    partial: Option<String>,
    keybindings: Keybindings,
    mode: Mode,
}

impl Default for ViInputParser {
    fn default() -> Self {
        ViInputParser {
            // FIXME: Setup proper keybinds
            keybindings: default_emacs_keybindings(),
            partial: None,
            mode: Mode::Normal,
        }
    }
}

impl InputParser for ViInputParser {
    fn parse_event(&mut self, event: Event) -> ReedlineEvent {
        match event {
            Event::Key(KeyEvent { code, modifiers }) => match (modifiers, code) {
                (KeyModifiers::NONE, KeyCode::Tab) => ReedlineEvent::HandleTab,
                (KeyModifiers::CONTROL, KeyCode::Char('c')) => ReedlineEvent::CtrlC,
                (KeyModifiers::NONE, KeyCode::Esc) => {
                    self.mode = Mode::Normal;
                    ReedlineEvent::Edit(vec![])
                }
                (KeyModifiers::NONE, KeyCode::Char(c))
                | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                    if self.mode == Mode::Normal {
                        self.parse_vi_fragment(c)
                    } else {
                        ReedlineEvent::EditInsert(EditCommand::InsertChar(c))
                    }
                }
                (KeyModifiers::NONE, KeyCode::Enter) => ReedlineEvent::Enter,
                _ => {
                    if let Some(binding) = self.keybindings.find_binding(modifiers, code) {
                        ReedlineEvent::Edit(binding)
                    } else {
                        ReedlineEvent::Edit(vec![])
                    }
                }
            },

            Event::Mouse(_) => ReedlineEvent::Mouse,
            Event::Resize(width, height) => ReedlineEvent::Resize(width, height),
        }
    }

    // HACK: This about this interface more
    fn update_keybindings(&mut self, keybindings: Keybindings) {
        self.keybindings = keybindings;
    }

    fn edit_mode(&self) -> PromptEditMode {
        match self.mode {
            Mode::Normal => PromptEditMode::Vi(PromptViMode::Normal),
            Mode::Insert => PromptEditMode::Vi(PromptViMode::Insert),
        }
    }
}

impl ViInputParser {
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
                    output.push(EditCommand::PreviousHistory);
                }
                'k' => {
                    output.push(EditCommand::NextHistory);
                }
                'i' => {
                    self.mode = Mode::Insert;
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
