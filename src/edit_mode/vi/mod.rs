mod command;
mod motion;
mod parser;
mod vi_keybindings;

use std::str::FromStr;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
pub use vi_keybindings::{default_vi_insert_keybindings, default_vi_normal_keybindings};

use self::motion::ViCharSearch;

use super::EditMode;
use crate::{
    edit_mode::{keybindings::Keybindings, vi::parser::parse},
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

                        let res = parse(self.mode, &mut self.cache.iter().peekable());

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

                    self.insert_keybindings
                        .find_binding(modifier, KeyCode::Char(c))
                        .unwrap_or_else(|| {
                            if modifier == KeyModifiers::NONE
                                || modifier == KeyModifiers::SHIFT
                                || modifier == KeyModifiers::CONTROL | KeyModifiers::ALT
                                || modifier
                                    == KeyModifiers::CONTROL
                                        | KeyModifiers::ALT
                                        | KeyModifiers::SHIFT
                            {
                                ReedlineEvent::Edit(vec![EditCommand::InsertChar(
                                    if modifier == KeyModifiers::SHIFT {
                                        c.to_ascii_uppercase()
                                    } else {
                                        c
                                    },
                                )])
                            } else {
                                ReedlineEvent::None
                            }
                        })
                }
                (_, KeyModifiers::NONE, KeyCode::Esc) => {
                    self.cache.clear();
                    self.mode = ViMode::Normal;
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
                (ViMode::Insert, _, _) => self
                    .insert_keybindings
                    .find_binding(modifiers, code)
                    .unwrap_or_else(|| {
                        // Default Enter behavior when no custom binding
                        if modifiers == KeyModifiers::NONE && code == KeyCode::Enter {
                            ReedlineEvent::Enter
                        } else {
                            ReedlineEvent::None
                        }
                    }),
            },

            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(button),
                column,
                row,
                modifiers: KeyModifiers::NONE,
            }) => ReedlineEvent::Mouse {
                column,
                row,
                button: button.into(),
            },
            Event::Mouse(_) => ReedlineEvent::None,
            Event::Resize(width, height) => ReedlineEvent::Resize(width, height),
            Event::FocusGained => ReedlineEvent::None,
            Event::FocusLost => ReedlineEvent::None,
            Event::Paste(body) => ReedlineEvent::Edit(vec![EditCommand::InsertString(
                body.replace("\r\n", "\n").replace('\r', "\n"),
            )]),
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
                    EventStatus::Handled
                }
                Err(_) => EventStatus::Inapplicable,
            },
            _ => EventStatus::Inapplicable,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> ReedlineRawEvent {
        ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(code, modifiers))).unwrap()
    }

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
    fn v_in_normal_enters_visual() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('v'), KeyModifiers::NONE));

        assert!(matches!(vi.mode, ViMode::Visual));
        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint])
        );
    }

    #[test]
    fn esc_from_visual_returns_to_normal() {
        let mut vi = Vi {
            mode: ViMode::Visual,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(vi.mode, ViMode::Normal));
    }

    #[test]
    fn esc_clears_cache() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(
            !vi.cache.is_empty(),
            "cache should hold the partial sequence"
        );

        let _ = vi.parse_event(key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(vi.cache.is_empty(), "Esc should clear the cache");
    }

    #[test]
    fn unbound_char_in_normal_feeds_parser() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };

        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        let result = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![
                EditCommand::CutWordRightToNext,
            ])]),
        );
        assert!(
            vi.cache.is_empty(),
            "cache should be cleared after a complete sequence"
        );
    }

    #[test]
    fn incomplete_sequence_returns_none_and_holds_cache() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));

        assert_eq!(result, ReedlineEvent::None);
        assert_eq!(vi.cache, vec!['d']);
    }

    #[test]
    fn shift_char_pushed_uppercase_into_cache() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::SHIFT));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![
                EditCommand::MoveBigWordRightStart { select: false },
            ])]),
        );
    }

    #[test]
    fn d_in_visual_emits_cut_selection_and_returns_to_normal() {
        let mut vi = Vi {
            mode: ViMode::Visual,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::CutSelection])]),
        );
        assert!(matches!(vi.mode, ViMode::Normal));
    }

    #[test]
    fn non_char_key_in_normal_uses_keybindings() {
        let mut kb = default_vi_normal_keybindings();
        kb.add_binding(KeyModifiers::NONE, KeyCode::Up, ReedlineEvent::Up);

        let mut vi = Vi {
            normal_keybindings: kb,
            mode: ViMode::Normal,
            ..Default::default()
        };

        let result = vi.parse_event(key(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(result, ReedlineEvent::Up);
    }

    #[test]
    fn enter_in_normal_with_no_binding_submits_and_enters_insert() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(result, ReedlineEvent::Enter);
        assert!(matches!(vi.mode, ViMode::Insert));
    }

    #[test]
    fn unbound_char_in_insert_inserts_char() {
        let mut vi = Vi {
            mode: ViMode::Insert,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('x'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('x')]),
        );
    }

    #[test]
    fn shift_char_in_insert_inserts_uppercase() {
        let mut vi = Vi {
            mode: ViMode::Insert,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('a'), KeyModifiers::SHIFT));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('A')]),
        );
    }

    #[test]
    fn ctrl_char_in_insert_with_no_binding_returns_none() {
        let mut vi = Vi {
            mode: ViMode::Insert,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('z'), KeyModifiers::CONTROL));

        assert_eq!(result, ReedlineEvent::None);
    }

    #[test]
    fn i_in_normal_transitions_to_insert() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('i'), KeyModifiers::NONE));
        assert!(matches!(vi.mode, ViMode::Insert));
    }

    #[test]
    fn previous_set_after_complete_command() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        let _ = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::NONE));
        assert!(
            vi.previous.is_some(),
            "previous should track the last complete command"
        );
    }

    #[test]
    fn paste_event_produces_insert_string() {
        let mut vi = Vi::default();
        let paste = ReedlineRawEvent::try_from(Event::Paste("hello".to_string())).unwrap();
        let result = vi.parse_event(paste);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::InsertString("hello".to_string())]),
        );
    }

    #[test]
    fn resize_event_passes_through() {
        let mut vi = Vi::default();
        let resize = ReedlineRawEvent::try_from(Event::Resize(80, 24)).unwrap();
        let result = vi.parse_event(resize);
        assert_eq!(result, ReedlineEvent::Resize(80, 24));
    }

    #[test]
    fn focus_gained_returns_none() {
        let mut vi = Vi::default();
        let ev = ReedlineRawEvent::try_from(Event::FocusGained).unwrap();
        assert_eq!(vi.parse_event(ev), ReedlineEvent::None);
    }

    #[test]
    fn mouse_down_event_produces_mouse_event() {
        let mut vi = Vi::default();
        let ev = ReedlineRawEvent::try_from(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 5,
            row: 10,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();

        assert_eq!(
            vi.parse_event(ev),
            ReedlineEvent::Mouse {
                column: 5,
                row: 10,
                button: crate::enums::MouseButton::Left,
            },
        );
    }

    #[test]
    fn multiplier_repeats_operator_motion() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('2'), KeyModifiers::NONE));
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        let result = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::CutWordRightToNext]),
                ReedlineEvent::Edit(vec![EditCommand::CutWordRightToNext]),
            ]),
        );
        assert!(vi.cache.is_empty());
    }

    #[test]
    fn multiplier_alone_repeats_motion() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('3'), KeyModifiers::NONE));
        let result = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::NONE));

        let mv = ReedlineEvent::Edit(vec![EditCommand::MoveWordRightStart { select: false }]);
        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![mv.clone(), mv.clone(), mv]),
        );
        assert!(vi.cache.is_empty());
    }

    #[test]
    fn partial_multiplier_holds_cache() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('2'), KeyModifiers::NONE));

        assert_eq!(result, ReedlineEvent::None);
        assert_eq!(vi.cache, vec!['2']);
    }

    #[test]
    fn invalid_motion_after_operator_clears_cache() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        assert_eq!(vi.cache, vec!['d']);

        let result = vi.parse_event(key(KeyCode::Char('z'), KeyModifiers::NONE));

        assert_eq!(result, ReedlineEvent::None);
        assert!(
            vi.cache.is_empty(),
            "an invalid motion should drop the cached operator",
        );
    }

    #[test]
    fn linewise_dd_emits_cut_current_line() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        let result = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::CutCurrentLine])]),
        );
        assert!(vi.cache.is_empty());
    }

    #[test]
    fn repeated_dot_accumulates_nesting_in_previous() {
        // Each `.` press wraps `previous` in an outer Multiple AND
        // assigns the wrapped result back to `previous`. So every
        // press deepens the nesting by one. Observationally fine
        // (engine flattens via recursion), but `previous` grows
        // unboundedly over a session. Worse with multipliers — `2.`
        // doubles the inner Vec each call.
        fn depth(ev: &ReedlineEvent) -> usize {
            match ev {
                ReedlineEvent::Multiple(v) if v.len() == 1 => 1 + depth(&v[0]),
                _ => 0,
            }
        }
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        let _ = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::NONE));
        assert_eq!(depth(vi.previous.as_ref().unwrap()), 1);

        let _ = vi.parse_event(key(KeyCode::Char('.'), KeyModifiers::NONE));
        assert_eq!(depth(vi.previous.as_ref().unwrap()), 2);

        let _ = vi.parse_event(key(KeyCode::Char('.'), KeyModifiers::NONE));
        assert_eq!(depth(vi.previous.as_ref().unwrap()), 3);

        let _ = vi.parse_event(key(KeyCode::Char('.'), KeyModifiers::NONE));
        assert_eq!(depth(vi.previous.as_ref().unwrap()), 4);
    }

    #[test]
    fn dot_replays_previous_wrapped_in_outer_multiple() {
        // `.` produces Multiple([previous]) and writes it back to
        // `previous`. See `repeated_dot_accumulates_nesting_in_previous`
        // for the consequences — this assertion just pins the shape.
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        let dw = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::NONE));
        assert!(vi.previous.is_some());

        let dot = vi.parse_event(key(KeyCode::Char('.'), KeyModifiers::NONE));
        assert_eq!(dot, ReedlineEvent::Multiple(vec![dw]));
    }
}
