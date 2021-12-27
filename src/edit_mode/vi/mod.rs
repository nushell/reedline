mod command;
mod motion;
mod parser;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use super::EditMode;
use crate::{
    edit_mode::{
        keybindings::{default_vi_insert_keybindings, default_vi_normal_keybindings, Keybindings},
        vi::parser::parse,
    },
    enums::{EditCommand, ReedlineEvent},
    PromptEditMode, PromptViMode,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Mode {
    Normal,
    Insert,
}

/// This parses incoming input `Event`s like a Vi-Style editor
pub struct Vi {
    cache: Vec<char>,
    insert_keybindings: Keybindings,
    normal_keybindings: Keybindings,
    mode: Mode,
    previous: Option<ReedlineEvent>,
}

impl Default for Vi {
    fn default() -> Self {
        Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: default_vi_normal_keybindings(),
            cache: Vec::new(),
            mode: Mode::Insert,
            previous: None,
        }
    }
}

impl EditMode for Vi {
    fn parse_event(&mut self, event: Event) -> ReedlineEvent {
        match event {
            Event::Key(KeyEvent { code, modifiers }) => match (self.mode, modifiers, code) {
                (Mode::Normal, modifier, KeyCode::Char(c)) => {
                    // The repeat character is the only character that is not managed
                    // by the parser since the last event is stored in the editor
                    if c == '.' {
                        if let Some(event) = &self.previous {
                            return event.clone();
                        }
                    }

                    let char = if let KeyModifiers::SHIFT = modifier {
                        c.to_ascii_uppercase()
                    } else {
                        c
                    };
                    self.cache.push(char);

                    let res = parse(&mut self.cache.iter().peekable());

                    if res.enter_insert_mode() {
                        self.mode = Mode::Insert;
                    }

                    let event = res.to_reedline_event();
                    match event {
                        ReedlineEvent::None => {
                            if !res.is_valid() {
                                self.cache.clear();
                            }
                        }
                        _ => {
                            self.cache.clear();
                        }
                    };

                    self.previous = Some(event.clone());

                    event
                }
                (Mode::Insert, modifier, KeyCode::Char(c)) => {
                    // Note. The modifier can also be a combination of modifiers, for
                    // example:
                    //     KeyModifiers::CONTROL | KeyModifiers::ALT
                    //     KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
                    //
                    // Mixed modifiers are used by non american keyboards that have extra
                    // keys like 'alt gr'. Keep this in mind if in the future there are
                    // cases where an event is not being captured
                    let char = if let KeyModifiers::SHIFT = modifier {
                        c.to_ascii_uppercase()
                    } else {
                        c
                    };

                    ReedlineEvent::Edit(vec![EditCommand::InsertChar(char)])
                }
                (_, KeyModifiers::NONE, KeyCode::Tab) => ReedlineEvent::HandleTab,
                (_, KeyModifiers::NONE, KeyCode::Esc) => {
                    self.cache.clear();
                    self.mode = Mode::Normal;
                    ReedlineEvent::Repaint
                }
                (_, KeyModifiers::NONE, KeyCode::Enter) => ReedlineEvent::Enter,
                (Mode::Normal, _, _) => self
                    .normal_keybindings
                    .find_binding(modifiers, code)
                    .unwrap_or(ReedlineEvent::None),
                (Mode::Insert, _, _) => self
                    .insert_keybindings
                    .find_binding(modifiers, code)
                    .unwrap_or(ReedlineEvent::None),
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
