use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::{
    edit_mode::EditMode,
    enums::{ReedlineEvent, ReedlineRawEvent},
    EditCommand, PromptEditMode, PromptHelixMode,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum HelixMode {
    #[default]
    Insert,
    Normal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionAdjustment {
    Shifting,
    Anchored,
}

/// Minimal Helix-inspired edit mode supporting Normal and Insert states.
#[derive(Default)]
pub struct Helix {
    mode: HelixMode,
    selection_adjustment: Option<SelectionAdjustment>,
}

impl Helix {
    #[cfg(test)]
    pub(crate) fn normal() -> Self {
        Self {
            mode: HelixMode::Normal,
            selection_adjustment: None,
        }
    }

    fn enter_insert(&mut self, pre_cmds: Vec<ReedlineEvent>) -> ReedlineEvent {
        self.mode = HelixMode::Insert;
        let mut events = pre_cmds;
        events.push(ReedlineEvent::Repaint);
        ReedlineEvent::Multiple(events)
    }

    pub(crate) fn enter_plain_insert(&mut self) {
        self.mode = HelixMode::Insert;
        self.selection_adjustment = None;
    }
}

impl EditMode for Helix {
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event.into()
        else {
            return ReedlineEvent::None;
        };

        if modifiers == KeyModifiers::CONTROL && code == KeyCode::Char('c') {
            return ReedlineEvent::CtrlC;
        }

        match self.mode {
            HelixMode::Insert => match (code, modifiers) {
                (KeyCode::Esc, _) => {
                    self.mode = HelixMode::Normal;
                    self.selection_adjustment = None;
                    ReedlineEvent::Multiple(vec![
                        ReedlineEvent::Esc,
                        ReedlineEvent::Edit(vec![EditCommand::HxEnsureSelection]),
                        ReedlineEvent::Repaint,
                    ])
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    ReedlineEvent::Edit(vec![EditCommand::Delete])
                }
                (KeyCode::Char(c), _) => match self.selection_adjustment {
                    Some(SelectionAdjustment::Shifting) => ReedlineEvent::Edit(vec![
                        EditCommand::InsertChar(c),
                        EditCommand::HxShiftSelectionToInsertionPoint,
                    ]),
                    Some(SelectionAdjustment::Anchored) => ReedlineEvent::Edit(vec![
                        EditCommand::InsertChar(c),
                        EditCommand::HxExtendSelectionToInsertionPoint,
                    ]),
                    None => ReedlineEvent::Edit(vec![EditCommand::InsertChar(c)]),
                },
                (KeyCode::Enter, _) => ReedlineEvent::Enter,
                (KeyCode::Backspace, _) => match self.selection_adjustment {
                    Some(SelectionAdjustment::Shifting) => ReedlineEvent::Edit(vec![
                        EditCommand::Backspace,
                        EditCommand::HxShiftSelectionToInsertionPoint,
                    ]),
                    Some(SelectionAdjustment::Anchored) => ReedlineEvent::Edit(vec![
                        EditCommand::Backspace,
                        EditCommand::HxExtendSelectionToInsertionPoint,
                    ]),
                    None => ReedlineEvent::Edit(vec![EditCommand::Backspace]),
                },
                (KeyCode::Delete, _) => ReedlineEvent::Edit(vec![EditCommand::Delete]),
                (KeyCode::Left, _) => {
                    self.selection_adjustment = None;
                    ReedlineEvent::Edit(vec![
                        EditCommand::MoveLeft { select: false },
                        EditCommand::HxClearSelection,
                    ])
                }
                (KeyCode::Right, _) => {
                    self.selection_adjustment = None;
                    ReedlineEvent::Edit(vec![
                        EditCommand::MoveRight { select: false },
                        EditCommand::HxClearSelection,
                    ])
                }
                (KeyCode::Home, _) => {
                    self.selection_adjustment = None;
                    ReedlineEvent::Edit(vec![
                        EditCommand::MoveToLineStart { select: false },
                        EditCommand::HxClearSelection,
                    ])
                }
                (KeyCode::End, _) => {
                    self.selection_adjustment = None;
                    ReedlineEvent::Edit(vec![
                        EditCommand::MoveToLineEnd { select: false },
                        EditCommand::HxClearSelection,
                    ])
                }
                (KeyCode::Up, _) => ReedlineEvent::Up,
                (KeyCode::Down, _) => ReedlineEvent::Down,
                (KeyCode::Tab, _) => ReedlineEvent::None,
                _ => ReedlineEvent::None,
            },
            HelixMode::Normal => {
                if modifiers == KeyModifiers::CONTROL && code == KeyCode::Char('d') {
                    return ReedlineEvent::CtrlD;
                }

                match code {
                    KeyCode::Esc => ReedlineEvent::Multiple(vec![
                        ReedlineEvent::Esc,
                        ReedlineEvent::Edit(vec![EditCommand::HxRestartSelection]),
                        ReedlineEvent::Repaint,
                    ]),
                    KeyCode::Char('i') => {
                        self.selection_adjustment = Some(SelectionAdjustment::Shifting);
                        self.enter_insert(vec![ReedlineEvent::Edit(vec![
                            EditCommand::HxEnsureSelection,
                            EditCommand::HxMoveToSelectionStart,
                        ])])
                    }
                    KeyCode::Char('a') => {
                        self.selection_adjustment = Some(SelectionAdjustment::Anchored);
                        self.enter_insert(vec![ReedlineEvent::Edit(vec![
                            EditCommand::HxEnsureSelection,
                            EditCommand::HxMoveToSelectionEnd,
                        ])])
                    }
                    KeyCode::Char('I') => self.enter_insert(vec![ReedlineEvent::Edit(vec![
                        EditCommand::HxClearSelection,
                        EditCommand::MoveToLineStart { select: false },
                    ])]),
                    KeyCode::Char('A') => self.enter_insert(vec![ReedlineEvent::Edit(vec![
                        EditCommand::HxClearSelection,
                        EditCommand::MoveToLineEnd { select: false },
                    ])]),
                    KeyCode::Char('h') => ReedlineEvent::Edit(vec![
                        EditCommand::MoveLeft { select: false },
                        EditCommand::HxRestartSelection,
                    ]),
                    KeyCode::Char('l') => ReedlineEvent::UntilFound(vec![
                        ReedlineEvent::HistoryHintComplete,
                        ReedlineEvent::MenuRight,
                        ReedlineEvent::Edit(vec![
                            EditCommand::MoveRight { select: false },
                            EditCommand::HxRestartSelection,
                        ]),
                    ]),
                    KeyCode::Enter => ReedlineEvent::Enter,
                    _ => ReedlineEvent::None,
                }
            }
        }
    }

    fn edit_mode(&self) -> PromptEditMode {
        match self.mode {
            HelixMode::Insert => PromptEditMode::Helix(PromptHelixMode::Insert),
            HelixMode::Normal => PromptEditMode::Helix(PromptHelixMode::Normal),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn key_press(code: KeyCode, modifiers: KeyModifiers) -> ReedlineRawEvent {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
        .try_into()
        .unwrap()
    }

    fn char_key(c: char) -> ReedlineRawEvent {
        key_press(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn helix_edit_mode_defaults_to_insert_mode() {
        let helix_mode = Helix::default();

        let edit_mode = helix_mode.edit_mode();

        assert!(matches!(
            edit_mode,
            PromptEditMode::Helix(PromptHelixMode::Insert)
        ));
    }

    #[test]
    fn helix_edit_mode_parses_ctrl_c_event() {
        let mut helix_mode = Helix::normal();

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            ReedlineEvent::CtrlC
        );
    }

    #[test]
    fn helix_edit_mode_enters_insert_with_i() {
        let mut helix_mode = Helix::normal();

        assert_eq!(
            helix_mode.parse_event(char_key('i')),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![
                    EditCommand::HxEnsureSelection,
                    EditCommand::HxMoveToSelectionStart,
                ]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix_mode.mode, HelixMode::Insert);
        assert_eq!(
            helix_mode.selection_adjustment,
            Some(SelectionAdjustment::Shifting)
        );
    }

    #[test]
    fn helix_edit_mode_enters_append_with_a() {
        let mut helix_mode = Helix::normal();

        assert_eq!(
            helix_mode.parse_event(char_key('a')),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![
                    EditCommand::HxEnsureSelection,
                    EditCommand::HxMoveToSelectionEnd,
                ]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix_mode.mode, HelixMode::Insert);
        assert_eq!(
            helix_mode.selection_adjustment,
            Some(SelectionAdjustment::Anchored)
        );
    }

    #[test]
    fn helix_edit_mode_exits_insert_with_escape() {
        let mut helix_mode = Helix {
            mode: HelixMode::Insert,
            selection_adjustment: Some(SelectionAdjustment::Anchored),
        };

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Esc, KeyModifiers::NONE)),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Esc,
                ReedlineEvent::Edit(vec![EditCommand::HxEnsureSelection]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix_mode.mode, HelixMode::Normal);
        assert_eq!(helix_mode.selection_adjustment, None);
    }

    #[test]
    fn helix_edit_mode_ctrl_d_is_delete_in_insert() {
        let mut helix_mode = Helix {
            mode: HelixMode::Insert,
            selection_adjustment: None,
        };

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            ReedlineEvent::Edit(vec![EditCommand::Delete])
        );
    }

    #[test]
    fn helix_edit_mode_ctrl_d_is_eof_in_normal() {
        let mut helix_mode = Helix::normal();

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            ReedlineEvent::CtrlD
        );
    }

    #[test]
    fn helix_edit_mode_insert_char_uses_shifting_adjustment() {
        let mut helix_mode = Helix {
            mode: HelixMode::Insert,
            selection_adjustment: Some(SelectionAdjustment::Shifting),
        };

        assert_eq!(
            helix_mode.parse_event(char_key('x')),
            ReedlineEvent::Edit(vec![
                EditCommand::InsertChar('x'),
                EditCommand::HxShiftSelectionToInsertionPoint,
            ])
        );
    }

    #[test]
    fn helix_edit_mode_insert_char_uses_anchored_adjustment() {
        let mut helix_mode = Helix {
            mode: HelixMode::Insert,
            selection_adjustment: Some(SelectionAdjustment::Anchored),
        };

        assert_eq!(
            helix_mode.parse_event(char_key('x')),
            ReedlineEvent::Edit(vec![
                EditCommand::InsertChar('x'),
                EditCommand::HxExtendSelectionToInsertionPoint,
            ])
        );
    }

    #[test]
    fn helix_edit_mode_normal_h_restarts_selection() {
        let mut helix_mode = Helix::normal();

        assert_eq!(
            helix_mode.parse_event(char_key('h')),
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
                EditCommand::HxRestartSelection,
            ])
        );
    }

    #[test]
    fn helix_edit_mode_normal_l_uses_until_found() {
        let mut helix_mode = Helix::normal();

        assert_eq!(
            helix_mode.parse_event(char_key('l')),
            ReedlineEvent::UntilFound(vec![
                ReedlineEvent::HistoryHintComplete,
                ReedlineEvent::MenuRight,
                ReedlineEvent::Edit(vec![
                    EditCommand::MoveRight { select: false },
                    EditCommand::HxRestartSelection,
                ]),
            ])
        );
    }

    #[test]
    fn helix_edit_mode_big_i_enters_insert_at_line_start() {
        let mut helix_mode = Helix::normal();

        assert_eq!(
            helix_mode.parse_event(char_key('I')),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![
                    EditCommand::HxClearSelection,
                    EditCommand::MoveToLineStart { select: false },
                ]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix_mode.mode, HelixMode::Insert);
    }

    #[test]
    fn helix_edit_mode_delete_clears_selection_tracking() {
        let mut helix_mode = Helix {
            mode: HelixMode::Insert,
            selection_adjustment: Some(SelectionAdjustment::Shifting),
        };

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Left, KeyModifiers::NONE)),
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
                EditCommand::HxClearSelection,
            ])
        );
        assert_eq!(helix_mode.selection_adjustment, None);
    }
}
