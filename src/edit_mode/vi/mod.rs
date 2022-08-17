mod command;
mod motion;
mod parser;
mod vi_keybindings;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
pub use vi_keybindings::{default_vi_insert_keybindings, default_vi_normal_keybindings};

use super::EditMode;
use crate::{
    edit_mode::{keybindings::Keybindings, vi::parser::parse},
    enums::{EditCommand, ReedlineEvent},
    PromptEditMode, PromptViMode,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ViMode {
    Normal,
    Insert,
}

/// Vi left-right motions to or till a character.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ViToTill {
    /// f
    ToRight(char),
    /// F
    ToLeft(char),
    /// t
    TillRight(char),
    /// T
    TillLeft(char),
}

impl ViToTill {
    /// Swap the direction of the to or till for ','
    pub fn reverse(&self) -> Self {
        match self {
            ViToTill::ToRight(c) => ViToTill::ToLeft(*c),
            ViToTill::ToLeft(c) => ViToTill::ToRight(*c),
            ViToTill::TillRight(c) => ViToTill::TillLeft(*c),
            ViToTill::TillLeft(c) => ViToTill::TillRight(*c),
        }
    }
}

impl From<EditCommand> for Option<ViToTill> {
    fn from(edit: EditCommand) -> Self {
        match edit {
            EditCommand::MoveLeftBefore(c) => Some(ViToTill::TillLeft(c)),
            EditCommand::MoveLeftUntil(c) => Some(ViToTill::ToLeft(c)),
            EditCommand::MoveRightBefore(c) => Some(ViToTill::TillRight(c)),
            EditCommand::MoveRightUntil(c) => Some(ViToTill::ToRight(c)),
            _ => None,
        }
    }
}

/// This parses incoming input `Event`s like a Vi-Style editor
pub struct Vi {
    cache: Vec<char>,
    insert_keybindings: Keybindings,
    normal_keybindings: Keybindings,
    mode: ViMode,
    previous: Option<ReedlineEvent>,
    // last f, F, t, T motion for ; and ,
    last_to_till: Option<ViToTill>,
}

impl Default for Vi {
    fn default() -> Self {
        Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: default_vi_normal_keybindings(),
            cache: Vec::new(),
            mode: ViMode::Insert,
            previous: None,
            last_to_till: None,
        }
    }
}

impl Vi {
    /// Creates Vi editor using defined keybindings
    pub fn new(insert_keybindings: Keybindings, normal_keybindings: Keybindings) -> Self {
        Self {
            insert_keybindings,
            normal_keybindings,
            ..Default::default()
        }
    }
}

impl EditMode for Vi {
    fn parse_event(&mut self, event: Event) -> ReedlineEvent {
        match event {
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                state,
            }) => match (self.mode, modifiers, code) {
                (ViMode::Normal, modifier, KeyCode::Char(c)) => {
                    // The repeat character is the only character that is not managed
                    // by the parser since the last event is stored in the editor
                    if c == '.' {
                        if let Some(event) = &self.previous {
                            return event.clone();
                        }
                    }

                    let c = c.to_ascii_lowercase();

                    if let Some(event) = self.normal_keybindings.find_binding(
                        modifiers,
                        KeyCode::Char(c),
                        kind,
                        state,
                    ) {
                        event
                    } else if modifier == KeyModifiers::NONE || modifier == KeyModifiers::SHIFT {
                        self.cache.push(if modifier == KeyModifiers::SHIFT {
                            c.to_ascii_uppercase()
                        } else {
                            c
                        });

                        let res = parse(self, &mut self.cache.iter().peekable());

                        if res.enter_insert_mode() {
                            self.mode = ViMode::Insert;
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

                        // to_reedline_event() returned Multiple or None when this was written
                        if let ReedlineEvent::Multiple(ref events) = event {
                            let last_to_till =
                                if events.len() == 2 && events[0] == ReedlineEvent::RecordToTill {
                                    if let ReedlineEvent::Edit(edit) = &events[1] {
                                        edit[0].clone().into()
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                            if last_to_till.is_some() {
                                self.last_to_till = last_to_till;
                            }
                        }

                        self.previous = Some(event.clone());

                        event
                    } else {
                        ReedlineEvent::None
                    }
                }
                (ViMode::Insert, modifier, KeyCode::Char(c)) => {
                    // Note. The modifier can also be a combination of modifiers, for
                    // example:
                    //     KeyModifiers::CONTROL | KeyModifiers::ALT
                    //     KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
                    //
                    // Mixed modifiers are used by non american keyboards that have extra
                    // keys like 'alt gr'. Keep this in mind if in the future there are
                    // cases where an event is not being captured
                    let c = match modifier {
                        KeyModifiers::NONE => c,
                        _ => c.to_ascii_lowercase(),
                    };

                    if modifier == KeyModifiers::NONE
                        || modifier == KeyModifiers::SHIFT
                        || modifier == KeyModifiers::CONTROL | KeyModifiers::ALT
                        || modifier
                            == KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
                    {
                        ReedlineEvent::Edit(vec![EditCommand::InsertChar(
                            if modifier == KeyModifiers::SHIFT {
                                c.to_ascii_uppercase()
                            } else {
                                c
                            },
                        )])
                    } else {
                        self.insert_keybindings
                            .find_binding(modifier, KeyCode::Char(c), kind, state)
                            .unwrap_or(ReedlineEvent::None)
                    }
                }
                (_, KeyModifiers::NONE, KeyCode::Esc) => {
                    self.cache.clear();
                    self.mode = ViMode::Normal;
                    ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint])
                }
                (_, KeyModifiers::NONE, KeyCode::Enter) => {
                    self.mode = ViMode::Insert;
                    ReedlineEvent::Enter
                }
                (ViMode::Normal, _, _) => self
                    .normal_keybindings
                    .find_binding(modifiers, code, kind, state)
                    .unwrap_or(ReedlineEvent::None),
                (ViMode::Insert, _, _) => self
                    .insert_keybindings
                    .find_binding(modifiers, code, kind, state)
                    .unwrap_or(ReedlineEvent::None),
            },

            Event::Mouse(_) => ReedlineEvent::Mouse,
            Event::Resize(width, height) => ReedlineEvent::Resize(width, height),
            Event::FocusGained => ReedlineEvent::FocusGained,
            Event::FocusLost => ReedlineEvent::FocusLost,
            Event::Paste(s) => ReedlineEvent::Paste(s),
        }
    }

    fn edit_mode(&self) -> PromptEditMode {
        match self.mode {
            ViMode::Normal => PromptEditMode::Vi(PromptViMode::Normal),
            ViMode::Insert => PromptEditMode::Vi(PromptViMode::Insert),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    #[test]
    fn esc_leads_to_normal_mode_test() {
        let mut vi = Vi::default();
        let esc = Event::Key(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Esc,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let result = vi.parse_event(esc);

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint])
        );
        assert!(matches!(vi.mode, ViMode::Normal));
    }

    #[test]
    fn keybinding_without_modifier_test() {
        let mut keybindings = default_vi_normal_keybindings();
        keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Char('e'),
            ReedlineEvent::ClearScreen,
            KeyEventKind::Press,
            KeyEventState::NONE,
        );

        let mut vi = Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: keybindings,
            mode: ViMode::Normal,
            ..Default::default()
        };

        let esc = Event::Key(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Char('e'),
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let result = vi.parse_event(esc);

        assert_eq!(result, ReedlineEvent::ClearScreen);
    }

    #[test]
    fn keybinding_with_shift_modifier_test() {
        let mut keybindings = default_vi_normal_keybindings();
        keybindings.add_binding(
            KeyModifiers::SHIFT,
            KeyCode::Char('$'),
            ReedlineEvent::CtrlD,
            KeyEventKind::Press,
            KeyEventState::NONE,
        );

        let mut vi = Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: keybindings,
            mode: ViMode::Normal,
            ..Default::default()
        };

        let esc = Event::Key(KeyEvent {
            modifiers: KeyModifiers::SHIFT,
            code: KeyCode::Char('$'),
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let result = vi.parse_event(esc);

        assert_eq!(result, ReedlineEvent::CtrlD);
    }

    #[test]
    fn non_register_modifier_test() {
        let keybindings = default_vi_normal_keybindings();
        let mut vi = Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: keybindings,
            mode: ViMode::Normal,
            ..Default::default()
        };

        let esc = Event::Key(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Char('q'),
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let result = vi.parse_event(esc);

        assert_eq!(result, ReedlineEvent::None);
    }

    #[rstest]
    #[case('f', KeyModifiers::NONE, ViToTill::ToRight('X'))]
    #[case('f', KeyModifiers::SHIFT, ViToTill::ToLeft('X'))]
    #[case('t', KeyModifiers::NONE, ViToTill::TillRight('X'))]
    #[case('t', KeyModifiers::SHIFT, ViToTill::TillLeft('X'))]
    fn last_to_till(
        #[case] code: char,
        #[case] modifiers: KeyModifiers,
        #[case] expected: ViToTill,
    ) {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Vi::default()
        };

        let to_till = Event::Key(KeyEvent {
            code: KeyCode::Char(code),
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        vi.parse_event(to_till);

        let key_x = Event::Key(KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::SHIFT,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        vi.parse_event(key_x);

        assert_eq!(vi.last_to_till, Some(expected));
    }
}
