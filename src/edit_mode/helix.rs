use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::{
    edit_mode::EditMode,
    enums::{ReedlineEvent, ReedlineRawEvent},
    EditCommand, PromptEditMode, PromptHelixMode,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum HelixMode {
    Insert,
    #[default]
    Normal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum InsertStyle {
    #[default]
    Plain,
    Before,
    After,
}

/// Minimal Helix-inspired edit mode supporting Normal and Insert states.
#[derive(Default)]
pub struct Helix {
    mode: HelixMode,
    insert_style: InsertStyle,
}

impl Helix {
    fn enter_insert(&mut self, pre_cmds: Vec<ReedlineEvent>) -> ReedlineEvent {
        self.mode = HelixMode::Insert;
        let mut events = pre_cmds;
        events.push(ReedlineEvent::Repaint);
        ReedlineEvent::Multiple(events)
    }

    #[cfg(test)]
    pub(super) fn enter_plain_insert(&mut self) {
        self.mode = HelixMode::Insert;
        self.insert_style = InsertStyle::Plain;
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
                    self.insert_style = InsertStyle::Plain;
                    ReedlineEvent::Multiple(vec![
                        ReedlineEvent::Esc,
                        ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }]),
                        ReedlineEvent::Edit(vec![EditCommand::HxRestartSelection]),
                        ReedlineEvent::Repaint,
                    ])
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    ReedlineEvent::Edit(vec![EditCommand::Delete])
                }
                (KeyCode::Char(c), _) => match self.insert_style {
                    InsertStyle::Before => ReedlineEvent::Edit(vec![
                        EditCommand::InsertChar(c),
                        EditCommand::HxShiftSelectionToInsertionPoint,
                    ]),
                    InsertStyle::After => ReedlineEvent::Edit(vec![
                        EditCommand::InsertChar(c),
                        EditCommand::HxExtendSelectionToInsertionPoint,
                    ]),
                    InsertStyle::Plain => ReedlineEvent::Edit(vec![EditCommand::InsertChar(c)]),
                },
                (KeyCode::Enter, _) => ReedlineEvent::Enter,
                (KeyCode::Backspace, _) => match self.insert_style {
                    InsertStyle::Before => ReedlineEvent::Edit(vec![
                        EditCommand::Backspace,
                        EditCommand::HxShiftSelectionToInsertionPoint,
                    ]),
                    InsertStyle::After => ReedlineEvent::Edit(vec![
                        EditCommand::Backspace,
                        EditCommand::HxExtendSelectionToInsertionPoint,
                    ]),
                    InsertStyle::Plain => ReedlineEvent::Edit(vec![EditCommand::Backspace]),
                },
                (KeyCode::Delete, _) => ReedlineEvent::Edit(vec![EditCommand::Delete]),
                (KeyCode::Left, _) => {
                    self.insert_style = InsertStyle::Plain;
                    ReedlineEvent::Edit(vec![
                        EditCommand::MoveLeft { select: false },
                        EditCommand::HxClearSelection,
                    ])
                }
                (KeyCode::Right, _) => {
                    self.insert_style = InsertStyle::Plain;
                    ReedlineEvent::Edit(vec![
                        EditCommand::MoveRight { select: false },
                        EditCommand::HxClearSelection,
                    ])
                }
                (KeyCode::Home, _) => {
                    self.insert_style = InsertStyle::Plain;
                    ReedlineEvent::Edit(vec![
                        EditCommand::MoveToLineStart { select: false },
                        EditCommand::HxClearSelection,
                    ])
                }
                (KeyCode::End, _) => {
                    self.insert_style = InsertStyle::Plain;
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
                        self.insert_style = InsertStyle::Before;
                        self.enter_insert(vec![ReedlineEvent::Edit(vec![
                            EditCommand::HxEnsureSelection,
                            EditCommand::HxMoveToSelectionStart,
                        ])])
                    }
                    KeyCode::Char('a') => {
                        self.insert_style = InsertStyle::After;
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
    fn helix_edit_mode_defaults_to_normal_mode() {
        let helix_mode = Helix::default();

        let edit_mode = helix_mode.edit_mode();

        assert!(matches!(
            edit_mode,
            PromptEditMode::Helix(PromptHelixMode::Normal)
        ));
    }

    #[test]
    fn helix_edit_mode_parses_ctrl_c_event() {
        let mut helix_mode = Helix::default();

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            ReedlineEvent::CtrlC
        );
    }

    #[test]
    fn helix_edit_mode_enters_insert_with_i() {
        let mut helix_mode = Helix::default();

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
        assert_eq!(helix_mode.insert_style, InsertStyle::Before);
    }

    #[test]
    fn helix_edit_mode_enters_append_with_a() {
        let mut helix_mode = Helix::default();

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
        assert_eq!(helix_mode.insert_style, InsertStyle::After);
    }

    #[test]
    fn helix_edit_mode_exits_insert_with_escape() {
        let mut helix_mode = Helix {
            mode: HelixMode::Insert,
            insert_style: InsertStyle::After,
        };

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Esc, KeyModifiers::NONE)),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Esc,
                ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }]),
                ReedlineEvent::Edit(vec![EditCommand::HxRestartSelection]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix_mode.mode, HelixMode::Normal);
        assert_eq!(helix_mode.insert_style, InsertStyle::Plain);
    }

    #[test]
    fn helix_edit_mode_ctrl_d_is_delete_in_insert() {
        let mut helix_mode = Helix {
            mode: HelixMode::Insert,
            insert_style: InsertStyle::Plain,
        };

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            ReedlineEvent::Edit(vec![EditCommand::Delete])
        );
    }

    #[test]
    fn helix_edit_mode_ctrl_d_is_eof_in_normal() {
        let mut helix_mode = Helix::default();

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            ReedlineEvent::CtrlD
        );
    }

    #[test]
    fn helix_edit_mode_insert_char_tracks_before_mode() {
        let mut helix_mode = Helix {
            mode: HelixMode::Insert,
            insert_style: InsertStyle::Before,
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
    fn helix_edit_mode_insert_char_tracks_after_mode() {
        let mut helix_mode = Helix {
            mode: HelixMode::Insert,
            insert_style: InsertStyle::After,
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
        let mut helix_mode = Helix::default();

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
        let mut helix_mode = Helix::default();

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
        let mut helix_mode = Helix::default();

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
            insert_style: InsertStyle::Before,
        };

        assert_eq!(
            helix_mode.parse_event(key_press(KeyCode::Left, KeyModifiers::NONE)),
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
                EditCommand::HxClearSelection,
            ])
        );
        assert_eq!(helix_mode.insert_style, InsertStyle::Plain);
    }
}
