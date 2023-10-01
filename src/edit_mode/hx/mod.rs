mod command;
mod hx_keybindings;
mod motion;
mod parser;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
pub use hx_keybindings::{default_hx_insert_keybindings, default_hx_normal_keybindings};

use self::motion::HxCharSearch;

use super::EditMode;
use crate::{
    edit_mode::{hx::parser::parse, keybindings::Keybindings},
    enums::{EditCommand, ReedlineEvent, ReedlineRawEvent},
    prompt::PromptHxMode,
    PromptEditMode,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum HxMode {
    Normal,
    Insert,
}

/// This parses incoming input `Event`s like the helix editor
pub struct Hx {
    cache: Vec<char>,
    insert_keybindings: Keybindings,
    normal_keybindings: Keybindings,
    mode: HxMode,
    previous: Option<ReedlineEvent>,
    last_char_search: Option<HxCharSearch>,
}

impl Default for Hx {
    fn default() -> Self {
        Hx {
            insert_keybindings: default_hx_insert_keybindings(),
            normal_keybindings: default_hx_normal_keybindings(),
            cache: Vec::new(),
            mode: HxMode::Insert,
            previous: None,
            last_char_search: None,
        }
    }
}

impl Hx {
    /// Creates hx editor using defined keybindings
    pub fn new(insert_keybindings: Keybindings, normal_keybindings: Keybindings) -> Self {
        Self {
            insert_keybindings,
            normal_keybindings,
            ..Default::default()
        }
    }
}

impl EditMode for Hx {
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        match event.into() {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => match (self.mode, modifiers, code) {
                (HxMode::Normal, modifier, KeyCode::Char(c)) => {
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
                                self.mode = HxMode::Insert;
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
                (HxMode::Insert, modifier, KeyCode::Char(c)) => {
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
                    self.mode = HxMode::Normal;
                    ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint])
                }
                (_, KeyModifiers::NONE, KeyCode::Enter) => {
                    self.mode = HxMode::Insert;
                    ReedlineEvent::Enter
                }
                (HxMode::Normal, _, _) => self
                    .normal_keybindings
                    .find_binding(modifiers, code)
                    .unwrap_or(ReedlineEvent::None),
                (HxMode::Insert, _, _) => self
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
            HxMode::Normal => PromptEditMode::Hx(PromptHxMode::Normal),
            HxMode::Insert => PromptEditMode::Hx(PromptHxMode::Insert),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn esc_leads_to_normal_mode_test() {
        let mut hx = Hx::default();
        let esc = ReedlineRawEvent::convert_from(Event::Key(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = hx.parse_event(esc);

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint])
        );
        assert!(matches!(hx.mode, HxMode::Normal));
    }

    #[test]
    fn keybinding_without_modifier_test() {
        let mut keybindings = default_hx_normal_keybindings();
        keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Char('e'),
            ReedlineEvent::ClearScreen,
        );

        let mut vi = Hx {
            insert_keybindings: default_hx_insert_keybindings(),
            normal_keybindings: keybindings,
            mode: HxMode::Normal,
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
        let mut keybindings = default_hx_normal_keybindings();
        keybindings.add_binding(
            KeyModifiers::SHIFT,
            KeyCode::Char('$'),
            ReedlineEvent::CtrlD,
        );

        let mut hx = Hx {
            insert_keybindings: default_hx_insert_keybindings(),
            normal_keybindings: keybindings,
            mode: HxMode::Normal,
            ..Default::default()
        };

        let esc = ReedlineRawEvent::convert_from(Event::Key(KeyEvent::new(
            KeyCode::Char('$'),
            KeyModifiers::SHIFT,
        )))
        .unwrap();
        let result = hx.parse_event(esc);

        assert_eq!(result, ReedlineEvent::CtrlD);
    }

    #[test]
    fn non_register_modifier_test() {
        let keybindings = default_hx_normal_keybindings();
        let mut hx = Hx {
            insert_keybindings: default_hx_insert_keybindings(),
            normal_keybindings: keybindings,
            mode: HxMode::Normal,
            ..Default::default()
        };

        let esc = ReedlineRawEvent::convert_from(Event::Key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = hx.parse_event(esc);

        assert_eq!(result, ReedlineEvent::None);
    }
}
