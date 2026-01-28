mod command;
mod motion;
mod parser;
mod vi_keybindings;

use std::str::FromStr;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
pub use vi_keybindings::{default_vi_insert_keybindings, default_vi_normal_keybindings};

use self::motion::ViCharSearch;

use super::EditMode;
use crate::{
    edit_mode::{keybindings::Keybindings, vi::parser::parse, KeyCombination, KeySequenceState},
    enums::{EditCommand, EventStatus, ReedlineEvent, ReedlineRawEvent},
    PromptEditMode, PromptViMode,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ViMode {
    Normal,
    Insert,
    Visual,
}

impl FromStr for ViMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "normal" => Ok(ViMode::Normal),
            "insert" => Ok(ViMode::Insert),
            "visual" => Ok(ViMode::Visual),
            _ => Err(()),
        }
    }
}

/// This parses incoming input `Event`s like a Vi-Style editor
pub struct Vi {
    cache: Vec<char>,
    insert_keybindings: Keybindings,
    normal_keybindings: Keybindings,
    insert_sequence_state: KeySequenceState,
    mode: ViMode,
    previous: Option<ReedlineEvent>,
    // last f, F, t, T motion for ; and ,
    last_char_search: Option<ViCharSearch>,
}

impl Default for Vi {
    fn default() -> Self {
        Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: default_vi_normal_keybindings(),
            cache: Vec::new(),
            insert_sequence_state: KeySequenceState::default(),
            mode: ViMode::Insert,
            previous: None,
            last_char_search: None,
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
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        match event.into() {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => match (self.mode, modifiers, code) {
                (ViMode::Normal, KeyModifiers::NONE, KeyCode::Char('v')) => {
                    self.cache.clear();
                    self.mode = ViMode::Visual;
                    ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint])
                }
                (ViMode::Normal | ViMode::Visual, modifier, KeyCode::Char(c)) => {
                    let c = c.to_ascii_lowercase();

                    if let Some(event) = self
                        .normal_keybindings
                        .find_binding(modifiers, KeyCode::Char(c))
                    {
                        event
                    } else if modifier == KeyModifiers::NONE || modifier == KeyModifiers::SHIFT {
                        self.cache.push(if modifier == KeyModifiers::SHIFT {
                            c.to_ascii_uppercase()
                        } else {
                            c
                        });

                        let res = parse(&mut self.cache.iter().peekable());

                        if !res.is_valid() {
                            self.cache.clear();
                            ReedlineEvent::None
                        } else if res.is_complete(self.mode) {
                            let event = res.to_reedline_event(self);
                            if let Some(mode) = res.changes_mode(self.mode) {
                                self.mode = mode;
                            }
                            self.cache.clear();
                            event
                        } else {
                            ReedlineEvent::None
                        }
                    } else {
                        ReedlineEvent::None
                    }
                }
                (ViMode::Insert, modifier, KeyCode::Char(c)) => {
                    self.handle_insert_key(modifier, KeyCode::Char(c))
                }
                (_, KeyModifiers::NONE, KeyCode::Esc) => {
                    self.cache.clear();
                    self.mode = ViMode::Normal;
                    self.insert_sequence_state.clear();
                    ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint])
                }
                (ViMode::Normal | ViMode::Visual, _, _) => self
                    .normal_keybindings
                    .find_binding(modifiers, code)
                    .unwrap_or_else(|| {
                        // Default Enter behavior when no custom binding
                        if modifiers == KeyModifiers::NONE && code == KeyCode::Enter {
                            self.mode = ViMode::Insert;
                            ReedlineEvent::Enter
                        } else {
                            ReedlineEvent::None
                        }
                    }),
                (ViMode::Insert, _, _) => self.handle_insert_key(modifiers, code),
            },

            Event::Mouse(_) => self.with_flushed_insert_sequence(ReedlineEvent::Mouse),
            Event::Resize(width, height) => {
                self.with_flushed_insert_sequence(ReedlineEvent::Resize(width, height))
            }
            Event::FocusGained => self.with_flushed_insert_sequence(ReedlineEvent::None),
            Event::FocusLost => self.with_flushed_insert_sequence(ReedlineEvent::None),
            Event::Paste(body) => self.with_flushed_insert_sequence(ReedlineEvent::Edit(vec![
                EditCommand::InsertString(body.replace("\r\n", "\n").replace('\r', "\n")),
            ])),
        }
    }

    fn edit_mode(&self) -> PromptEditMode {
        match self.mode {
            ViMode::Normal | ViMode::Visual => PromptEditMode::Vi(PromptViMode::Normal),
            ViMode::Insert => PromptEditMode::Vi(PromptViMode::Insert),
        }
    }

    fn handle_mode_specific_event(&mut self, event: ReedlineEvent) -> EventStatus {
        match event {
            ReedlineEvent::ViChangeMode(mode_str) => match ViMode::from_str(&mode_str) {
                Ok(mode) => {
                    self.mode = mode;
                    self.insert_sequence_state.clear();
                    EventStatus::Handled
                }
                Err(_) => EventStatus::Inapplicable,
            },
            _ => EventStatus::Inapplicable,
        }
    }

    fn has_pending_sequence(&self) -> bool {
        matches!(self.mode, ViMode::Insert) && self.insert_sequence_state.is_pending()
    }

    fn flush_pending_sequence(&mut self) -> Option<ReedlineEvent> {
        if !matches!(self.mode, ViMode::Insert) {
            return None;
        }

        let keybindings = &self.insert_keybindings;
        self.insert_sequence_state
            .flush(|combo| Self::insert_single_key_event(keybindings, combo))
    }
}

impl Vi {
    fn normalize_key_combo(modifier: KeyModifiers, code: KeyCode) -> KeyCombination {
        let key_code = match code {
            KeyCode::Char(c) => {
                let c = match modifier {
                    KeyModifiers::NONE => c,
                    _ => c.to_ascii_lowercase(),
                };
                KeyCode::Char(c)
            }
            other => other,
        };

        KeyCombination { modifier, key_code }
    }

    fn insert_single_key_event(keybindings: &Keybindings, combo: KeyCombination) -> ReedlineEvent {
        match combo.key_code {
            KeyCode::Char(c) => keybindings
                .find_binding(combo.modifier, KeyCode::Char(c))
                .unwrap_or_else(|| {
                    if combo.modifier == KeyModifiers::NONE
                        || combo.modifier == KeyModifiers::SHIFT
                        || combo.modifier == KeyModifiers::CONTROL | KeyModifiers::ALT
                        || combo.modifier
                            == KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
                    {
                        ReedlineEvent::Edit(vec![EditCommand::InsertChar(
                            if combo.modifier == KeyModifiers::SHIFT {
                                c.to_ascii_uppercase()
                            } else {
                                c
                            },
                        )])
                    } else {
                        ReedlineEvent::None
                    }
                }),
            code => keybindings
                .find_binding(combo.modifier, code)
                .unwrap_or_else(|| {
                    if combo.modifier == KeyModifiers::NONE && code == KeyCode::Enter {
                        ReedlineEvent::Enter
                    } else {
                        ReedlineEvent::None
                    }
                }),
        }
    }

    fn handle_insert_key(&mut self, modifier: KeyModifiers, code: KeyCode) -> ReedlineEvent {
        let combo = Self::normalize_key_combo(modifier, code);
        let keybindings = &self.insert_keybindings;
        self.insert_sequence_state
            .process_combo(keybindings, combo, |combo| {
                Self::insert_single_key_event(keybindings, combo)
            })
            .unwrap_or(ReedlineEvent::None)
    }

    fn with_flushed_insert_sequence(&mut self, event: ReedlineEvent) -> ReedlineEvent {
        let Some(flush_event) = self.flush_pending_sequence() else {
            return event;
        };

        if matches!(event, ReedlineEvent::None) {
            return flush_event;
        }

        match flush_event {
            ReedlineEvent::Multiple(mut events) => {
                events.push(event);
                ReedlineEvent::Multiple(events)
            }
            other => ReedlineEvent::Multiple(vec![other, event]),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::KeyCombination;
    use pretty_assertions::assert_eq;

    #[test]
    fn esc_leads_to_normal_mode_test() {
        let mut vi = Vi::default();
        let esc =
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
                .unwrap();
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
        );

        let mut vi = Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: keybindings,
            mode: ViMode::Normal,
            ..Default::default()
        };

        let esc = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('e'),
            KeyModifiers::NONE,
        )))
        .unwrap();
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
        );

        let mut vi = Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: keybindings,
            mode: ViMode::Normal,
            ..Default::default()
        };

        let esc = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('$'),
            KeyModifiers::SHIFT,
        )))
        .unwrap();
        let result = vi.parse_event(esc);

        assert_eq!(result, ReedlineEvent::CtrlD);
    }

    #[test]
    fn keybinding_with_super_modifier_test() {
        let mut keybindings = default_vi_normal_keybindings();
        keybindings.add_binding(
            KeyModifiers::SUPER,
            KeyCode::Char('$'),
            ReedlineEvent::CtrlD,
        );

        let mut vi = Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: keybindings,
            mode: ViMode::Normal,
            ..Default::default()
        };

        let esc = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('$'),
            KeyModifiers::SUPER,
        )))
        .unwrap();
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

        let esc = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = vi.parse_event(esc);

        assert_eq!(result, ReedlineEvent::None);
    }

    #[test]
    fn insert_sequence_binding_emits_event() {
        let mut insert_keybindings = default_vi_insert_keybindings();
        let exit_event = ReedlineEvent::Multiple(vec![
            ReedlineEvent::Esc,
            ReedlineEvent::ViChangeMode("normal".to_string()),
            ReedlineEvent::Repaint,
        ]);
        insert_keybindings.add_sequence_binding(
            vec![
                KeyCombination {
                    modifier: KeyModifiers::NONE,
                    key_code: KeyCode::Char('j'),
                },
                KeyCombination {
                    modifier: KeyModifiers::NONE,
                    key_code: KeyCode::Char('j'),
                },
            ],
            exit_event.clone(),
        );

        let mut vi = Vi {
            insert_keybindings,
            normal_keybindings: default_vi_normal_keybindings(),
            mode: ViMode::Insert,
            ..Default::default()
        };

        let first = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let second = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        )))
        .unwrap();

        assert_eq!(vi.parse_event(first), ReedlineEvent::None);
        assert_eq!(vi.parse_event(second), exit_event);
    }
}
