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
    Visual,
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
                (_, KeyModifiers::NONE, KeyCode::Enter) => {
                    self.mode = ViMode::Insert;
                    ReedlineEvent::Enter
                }
                (ViMode::Normal | ViMode::Visual, _, _) => self
                    .normal_keybindings
                    .find_binding(modifiers, code)
                    .unwrap_or(ReedlineEvent::None),
                (ViMode::Insert, _, _) => self
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
            ViMode::Normal | ViMode::Visual => PromptEditMode::Vi(PromptViMode::Normal),
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
}
