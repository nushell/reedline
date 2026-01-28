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
    visual_keybindings: Keybindings,
    sequence_state: KeySequenceState,
    mode: ViMode,
    previous: Option<ReedlineEvent>,
    // last f, F, t, T motion for ; and ,
    last_char_search: Option<ViCharSearch>,
}

impl Default for Vi {
    fn default() -> Self {
        Self::new(
            default_vi_insert_keybindings(),
            default_vi_normal_keybindings(),
        )
    }
}

impl Vi {
    /// Creates Vi editor using defined keybindings
    pub fn new(insert_keybindings: Keybindings, normal_keybindings: Keybindings) -> Self {
        let mut visual_keybindings = normal_keybindings.clone();
        visual_keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Esc,
            ReedlineEvent::ViExitToNormalMode,
        );
        let _ = visual_keybindings.remove_binding(KeyModifiers::NONE, KeyCode::Char('v'));

        Self::new_with_visual_keybindings(
            insert_keybindings,
            normal_keybindings,
            visual_keybindings,
        )
    }

    /// Creates Vi editor using defined keybindings, including visual mode
    pub fn new_with_visual_keybindings(
        insert_keybindings: Keybindings,
        normal_keybindings: Keybindings,
        visual_keybindings: Keybindings,
    ) -> Self {
        Self {
            insert_keybindings,
            normal_keybindings,
            visual_keybindings,
            cache: Vec::new(),
            sequence_state: KeySequenceState::default(),
            mode: ViMode::Insert,
            previous: None,
            last_char_search: None,
        }
    }
}

impl EditMode for Vi {
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        match event.into() {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => self.handle_key_event(modifiers, code),

            Event::Mouse(_) => self.with_flushed_sequence(ReedlineEvent::Mouse),
            Event::Resize(width, height) => {
                self.with_flushed_sequence(ReedlineEvent::Resize(width, height))
            }
            Event::FocusGained => self.with_flushed_sequence(ReedlineEvent::None),
            Event::FocusLost => self.with_flushed_sequence(ReedlineEvent::None),
            Event::Paste(body) => {
                self.with_flushed_sequence(ReedlineEvent::Edit(vec![EditCommand::InsertString(
                    body.replace("\r\n", "\n").replace('\r', "\n"),
                )]))
            }
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
            ReedlineEvent::ViChangeMode(mode_str) => ViMode::from_str(&mode_str)
                .map(|mode| self.set_mode(mode))
                .unwrap_or(EventStatus::Inapplicable),
            ReedlineEvent::ViExitToNormalMode => self.set_mode(ViMode::Normal),
            _ => EventStatus::Inapplicable,
        }
    }

    fn has_pending_sequence(&self) -> bool {
        self.sequence_state.is_pending()
    }

    fn flush_pending_sequence(&mut self) -> Option<ReedlineEvent> {
        let resolution = self.sequence_state.flush_with_combos();
        resolution.into_event(|combo| self.single_key_event_without_sequences(combo))
    }
}

impl Vi {
    fn set_mode(&mut self, mode: ViMode) -> EventStatus {
        self.mode = mode;
        self.cache.clear();
        self.sequence_state.clear();
        EventStatus::Handled
    }

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

    fn handle_key_event(&mut self, modifier: KeyModifiers, code: KeyCode) -> ReedlineEvent {
        if matches!(self.mode, ViMode::Normal | ViMode::Visual)
            && !self.cache.is_empty()
            && matches!(code, KeyCode::Char(_))
        {
            let combo = Self::normalize_key_combo(modifier, code);
            return self.normal_visual_single_key_event(combo);
        }

        let combo = Self::normalize_key_combo(modifier, code);
        let keybindings = match self.mode {
            ViMode::Normal => &self.normal_keybindings,
            ViMode::Visual => &self.visual_keybindings,
            ViMode::Insert => &self.insert_keybindings,
        };
        let resolution = self
            .sequence_state
            .process_combo(keybindings, combo);

        resolution
            .into_event(|combo| self.single_key_event_without_sequences(combo))
            .unwrap_or(ReedlineEvent::None)
    }

    fn keybindings_for_mode(&self, mode: ViMode) -> &Keybindings {
        match mode {
            ViMode::Normal => &self.normal_keybindings,
            ViMode::Visual => &self.visual_keybindings,
            ViMode::Insert => &self.insert_keybindings,
        }
    }

    fn single_key_event_without_sequences(&mut self, combo: KeyCombination) -> ReedlineEvent {
        match self.mode {
            ViMode::Insert => Self::insert_single_key_event(&self.insert_keybindings, combo),
            ViMode::Normal | ViMode::Visual => self.normal_visual_single_key_event(combo),
        }
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

    fn normal_visual_single_key_event(&mut self, combo: KeyCombination) -> ReedlineEvent {
        let mode = self.mode;
        let keybindings = self.keybindings_for_mode(mode);
        let cache_pending = !self.cache.is_empty();
        match combo.key_code {
            KeyCode::Char(c) => {
                let c = c.to_ascii_lowercase();

                if !cache_pending {
                    if let Some(event) = keybindings.find_binding(combo.modifier, KeyCode::Char(c))
                    {
                        return event;
                    }
                }

                if combo.modifier == KeyModifiers::NONE || combo.modifier == KeyModifiers::SHIFT {
                    self.cache.push(if combo.modifier == KeyModifiers::SHIFT {
                        c.to_ascii_uppercase()
                    } else {
                        c
                    });

                    let res = parse(&mut self.cache.iter().peekable());

                    if !res.is_valid() {
                        self.cache.clear();
                        ReedlineEvent::None
                    } else if res.is_complete(mode) {
                        let event = res.to_reedline_event(self);
                        if let Some(new_mode) = res.changes_mode(mode) {
                            self.mode = new_mode;
                            self.sequence_state.clear();
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
            code => keybindings
                .find_binding(combo.modifier, code)
                .unwrap_or_else(|| {
                    if combo.modifier == KeyModifiers::NONE && code == KeyCode::Enter {
                        self.mode = ViMode::Insert;
                        self.sequence_state.clear();
                        ReedlineEvent::Enter
                    } else {
                        ReedlineEvent::None
                    }
                }),
        }
    }

    fn with_flushed_sequence(&mut self, event: ReedlineEvent) -> ReedlineEvent {
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
    use crate::{EditCommand, KeyCombination};
    use pretty_assertions::assert_eq;

    #[test]
    fn esc_in_insert_emits_exit_to_normal() {
        let mut vi = Vi::default();
        let esc =
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
                .unwrap();
        let result = vi.parse_event(esc);

        assert_eq!(result, ReedlineEvent::ViExitToNormalMode);
    }

    #[test]
    fn esc_in_normal_repaints() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let esc =
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
                .unwrap();
        let result = vi.parse_event(esc);

        assert_eq!(result, ReedlineEvent::Repaint);
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
        let exit_event = ReedlineEvent::ViExitToNormalMode;
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

    #[test]
    fn normal_mode_f_space_moves_to_space() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };

        let f = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('f'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let space = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
        )))
        .unwrap();

        assert_eq!(vi.parse_event(f), ReedlineEvent::None);
        assert_eq!(
            vi.parse_event(space),
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![
                EditCommand::MoveRightUntil {
                    c: ' ',
                    select: false,
                },
            ])])
        );
    }

    #[test]
    fn normal_mode_t_space_moves_before_space() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };

        let t = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('t'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let space = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
        )))
        .unwrap();

        assert_eq!(vi.parse_event(t), ReedlineEvent::None);
        assert_eq!(
            vi.parse_event(space),
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![
                EditCommand::MoveRightBefore {
                    c: ' ',
                    select: false,
                },
            ])])
        );
    }

    #[test]
    fn normal_mode_dt_space_cuts_before_space() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };

        let d = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let t = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('t'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let space = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
        )))
        .unwrap();

        assert_eq!(vi.parse_event(d), ReedlineEvent::None);
        assert_eq!(vi.parse_event(t), ReedlineEvent::None);
        assert_eq!(
            vi.parse_event(space),
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![
                EditCommand::CutRightBefore(' ')
            ])])
        );
    }

    #[test]
    fn pending_motion_ignores_space_binding() {
        let mut normal_keybindings = default_vi_normal_keybindings();
        normal_keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Char(' '),
            ReedlineEvent::Edit(vec![EditCommand::InsertChar(' ')]),
        );

        let mut vi = Vi::new(default_vi_insert_keybindings(), normal_keybindings);
        vi.mode = ViMode::Normal;

        let f = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('f'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let space = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
        )))
        .unwrap();

        assert_eq!(vi.parse_event(f), ReedlineEvent::None);
        assert_eq!(
            vi.parse_event(space),
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![
                EditCommand::MoveRightUntil {
                    c: ' ',
                    select: false,
                },
            ])])
        );
    }
}
