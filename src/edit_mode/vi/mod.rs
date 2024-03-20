mod command;
mod motion;
mod parser;
mod vi_keybindings;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
pub use vi_keybindings::{default_vi_insert_keybindings, default_vi_normal_keybindings};

use self::motion::ViCharSearch;

use super::EditMode;
use crate::{
    edit_mode::{keybindings::Keybindings, vi::parser::parse},
    enums::{EditCommand, ReedlineEvent, ReedlineRawEvent},
    PromptEditMode, PromptViMode,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ViMode {
    Normal,
    Insert,
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
    alternate_esc_seq: (KeyCode, KeyCode),
    most_recent_keycode: KeyCode,
}

impl Default for Vi {
    fn default() -> Self {
        Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: default_vi_normal_keybindings(),
            alternate_esc_seq: (KeyCode::Null, KeyCode::Null),
            cache: Vec::new(),
            mode: ViMode::Insert,
            previous: None,
            last_char_search: None,
            most_recent_keycode: KeyCode::Null,
        }
    }
}

impl Vi {
    /// Creates Vi editor using defined keybindings
    pub fn new(
        insert_keybindings: Keybindings,
        normal_keybindings: Keybindings,
        alternate_esc_seq: (KeyCode, KeyCode),
    ) -> Self {
        Self {
            insert_keybindings,
            normal_keybindings,
            alternate_esc_seq,
            ..Default::default()
        }
    }
}

fn exit_insert_mode(editor: &mut Vi) -> ReedlineEvent {
    editor.most_recent_keycode = KeyCode::Null;
    editor.cache.clear();
    editor.mode = ViMode::Normal;
    ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint])
}

impl EditMode for Vi {
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        match event.into() {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => match (
                self.mode,
                modifiers,
                code,
                self.alternate_esc_seq,
                self.most_recent_keycode,
            ) {
                (ViMode::Insert, KeyModifiers::NONE, code, (e1, e2), mr)
                    if code == e2 && mr == e1 =>
                {
                    exit_insert_mode(self)
                }
                (ViMode::Insert, KeyModifiers::NONE, code, (e1, _), _) if code == e1 => {
                    self.most_recent_keycode = code;
                    ReedlineEvent::None
                }
                (
                    ViMode::Insert,
                    KeyModifiers::NONE,
                    KeyCode::Char(code),
                    (KeyCode::Char(e1), KeyCode::Char(e2)),
                    KeyCode::Char(mr),
                ) if code != e2 && mr == e1 => {
                    self.most_recent_keycode = KeyCode::Char(code);
                    ReedlineEvent::Multiple(vec![
                        ReedlineEvent::Edit(vec![EditCommand::InsertChar(e1)]),
                        ReedlineEvent::Edit(vec![EditCommand::InsertChar(code)]),
                    ])
                }

                (ViMode::Normal, modifier, KeyCode::Char(c), _, _) => {
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
                        } else if res.is_complete() {
                            if res.enters_insert_mode() {
                                self.mode = ViMode::Insert;
                            }

                            let event = res.to_reedline_event(self);
                            self.cache.clear();
                            event
                        } else {
                            ReedlineEvent::None
                        }
                    } else {
                        ReedlineEvent::None
                    }
                }
                (ViMode::Insert, modifier, KeyCode::Char(c), _, _) => {
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
                                self.most_recent_keycode = KeyCode::Char(c);
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
                (_, KeyModifiers::NONE, KeyCode::Esc, _, _) => exit_insert_mode(self),
                (_, KeyModifiers::NONE, KeyCode::Enter, _, _) => {
                    self.mode = ViMode::Insert;
                    ReedlineEvent::Enter
                }
                (ViMode::Normal, _, _, _, _) => self
                    .normal_keybindings
                    .find_binding(modifiers, code)
                    .unwrap_or(ReedlineEvent::None),
                (ViMode::Insert, _, _, _, _) => self
                    .insert_keybindings
                    .find_binding(modifiers, code)
                    .unwrap_or(ReedlineEvent::None),
            },

            Event::Mouse(_) => ReedlineEvent::Mouse,
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
            ViMode::Normal => PromptEditMode::Vi(PromptViMode::Normal),
            ViMode::Insert => PromptEditMode::Vi(PromptViMode::Insert),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn esc_leads_to_normal_mode_test() {
        let mut vi = Vi::default();
        let esc = ReedlineRawEvent::convert_from(Event::Key(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        )))
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

        let esc = ReedlineRawEvent::convert_from(Event::Key(KeyEvent::new(
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

        let esc = ReedlineRawEvent::convert_from(Event::Key(KeyEvent::new(
            KeyCode::Char('$'),
            KeyModifiers::SHIFT,
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

        let esc = ReedlineRawEvent::convert_from(Event::Key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = vi.parse_event(esc);

        assert_eq!(result, ReedlineEvent::None);
    }
}
