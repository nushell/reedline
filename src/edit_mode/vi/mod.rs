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
                (ViMode::Normal, modifier, KeyCode::Char(c)) => {
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
                (ViMode::Normal, _, _) => self
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

    #[allow(non_snake_case)]
    mod integration {
        use super::*;
        use crate::{Reedline, ReedlineEvent};
        use pretty_assertions::assert_eq;
        use rstest::rstest;

        fn unwrap_edits(event: ReedlineEvent) -> Vec<EditCommand> {
            match event {
                ReedlineEvent::Edit(c) => c,
                // It's not clear what the difference is between these, but vi mode often returns Multiple
                ReedlineEvent::Multiple(events) => {
                    events.into_iter().flat_map(unwrap_edits).collect()
                }
                other => panic!("unexpected event {other:#?}"),
            }
        }

        #[rstest]
        #[case::edit_dw_P(&["^", "dw", "P"])]
        #[case::edit_3dw_P(&["^", "3dw", "P"])]
        #[case::edit_d3w_P(&["^", "d3w", "P"])]
        #[case::edit_dw_dw_P(&[
            // duplicate the first word, because dw dw P should drop the first word
            "^", "dw", "P", "P", "b",
            // run the actual test, and it should put us back where we started.
            "dw", "dw", "P"])]
        #[case::edit_dd_u(&["dd", "u"])]
        // not actually a no-op because it adds a newline, but we .trim_end()
        #[case::edit_dd_p(&["dd", "p"])]
        #[case::edit_dd_P_uu(&["dd", "P", "u", "u"])]
        // FIXME: this happens on the second line, so doesn't actually delete two lines
        // I can't work out how to use "k" to go to the line above because it generates an
        // UntilFound([MenuUp, Up]) event, and I'm not sure how to handle that.
        #[case::edit_d2d_p(&["d2d", "p"])]
        fn sum_to_zero(#[case] commands: &[&str]) {
            let initial_input = "the quick brown fox\njumps over the lazy dog";
            let keybindings = default_vi_normal_keybindings();
            let mut vi = Vi {
                insert_keybindings: default_vi_insert_keybindings(),
                normal_keybindings: keybindings,
                mode: ViMode::Normal,
                ..Default::default()
            };

            let mut reedline = Reedline::create();
            reedline.run_edit_commands(&[EditCommand::InsertString(initial_input.into())]);

            for command in commands {
                let command: Vec<char> = command.chars().collect();
                let parsed = parse(&mut command.iter().peekable());
                let commands = unwrap_edits(parsed.to_reedline_event(&mut vi));

                reedline.run_edit_commands(&commands)
            }

            assert_eq!(initial_input, reedline.current_buffer_contents().trim_end());
        }
    }
}
