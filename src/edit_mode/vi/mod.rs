use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::{
    default_emacs_keybindings,
    enums::{EditCommand, ReedlineEvent},
    PromptEditMode, PromptViMode,
};

use super::{keybindings::Keybindings, EditMode};

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Normal,
    Insert,
}

/// This parses incomming input `Event`s like a Vi-Style editor
pub struct Vi {
    partial: Option<String>,
    keybindings: Keybindings,
    mode: Mode,
}

impl Default for Vi {
    fn default() -> Self {
        Vi {
            // FIXME: Setup proper keybinds
            keybindings: default_emacs_keybindings(),
            partial: None,
            mode: Mode::Normal,
        }
    }
}

impl EditMode for Vi {
    fn parse_event(&mut self, event: Event) -> ReedlineEvent {
        match event {
            Event::Key(KeyEvent { code, modifiers }) => match (modifiers, code) {
                (KeyModifiers::NONE, KeyCode::Tab) => ReedlineEvent::HandleTab,
                (KeyModifiers::CONTROL, KeyCode::Char('c')) => ReedlineEvent::CtrlC,
                (KeyModifiers::NONE, KeyCode::Esc) => {
                    self.mode = Mode::Normal;
                    ReedlineEvent::Repaint
                }
                (KeyModifiers::NONE, KeyCode::Char(c))
                | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                    if self.mode == Mode::Normal {
                        self.parse_vi_fragment(c)
                    } else {
                        ReedlineEvent::EditInsert(EditCommand::InsertChar(c))
                    }
                }
                (m, KeyCode::Char(c)) if m == KeyModifiers::CONTROL | KeyModifiers::ALT => {
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
                    output.push(EditCommand::PreviousHistory);
                }
                'k' => {
                    output.push(EditCommand::NextHistory);
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
