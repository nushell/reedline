use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::{
    edit_mode::EditMode,
    enums::{ReedlineEvent, ReedlineRawEvent},
    EditCommand, PromptEditMode, PromptViMode,
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

struct HxClearSelection;

impl HxClearSelection {
    fn after(command: EditCommand) -> ReedlineEvent {
        ReedlineEvent::Edit(vec![command])
    }
}

struct HxRestartSelection;

impl HxRestartSelection {
    fn event() -> ReedlineEvent {
        ReedlineEvent::Edit(vec![
            EditCommand::MoveRight { select: true },
            EditCommand::MoveLeft { select: true },
        ])
    }

    fn after(command: EditCommand) -> ReedlineEvent {
        ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![command]), Self::event()])
    }
}

struct HxEnsureSelection;

impl HxEnsureSelection {
    fn event() -> ReedlineEvent {
        HxRestartSelection::event()
    }
}

struct HxMoveToSelectionStart;

impl HxMoveToSelectionStart {
    fn event() -> ReedlineEvent {
        ReedlineEvent::None
    }
}

struct HxMoveToSelectionEnd;

impl HxMoveToSelectionEnd {
    fn event() -> ReedlineEvent {
        ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: false }])
    }
}

struct HxShiftSelectionToInsertionPoint;

impl HxShiftSelectionToInsertionPoint {
    fn after(command: EditCommand) -> ReedlineEvent {
        ReedlineEvent::Edit(vec![command])
    }
}

struct HxExtendSelectionToInsertionPoint;

impl HxExtendSelectionToInsertionPoint {
    fn after(command: EditCommand) -> ReedlineEvent {
        ReedlineEvent::Edit(vec![command])
    }
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
        let mut events: Vec<_> = pre_cmds
            .into_iter()
            .filter(|event| !matches!(event, ReedlineEvent::None))
            .collect();
        events.push(ReedlineEvent::Repaint);
        ReedlineEvent::Multiple(events)
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
                        ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }]),
                        HxRestartSelection::event(),
                        ReedlineEvent::Repaint,
                    ])
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    ReedlineEvent::Edit(vec![EditCommand::Delete])
                }
                (KeyCode::Char(c), _) => match self.selection_adjustment {
                    Some(SelectionAdjustment::Shifting) => {
                        HxShiftSelectionToInsertionPoint::after(EditCommand::InsertChar(c))
                    }
                    Some(SelectionAdjustment::Anchored) => {
                        HxExtendSelectionToInsertionPoint::after(EditCommand::InsertChar(c))
                    }
                    None => ReedlineEvent::Edit(vec![EditCommand::InsertChar(c)]),
                },
                (KeyCode::Enter, _) => ReedlineEvent::Enter,
                (KeyCode::Backspace, _) => match self.selection_adjustment {
                    Some(SelectionAdjustment::Shifting) => {
                        HxShiftSelectionToInsertionPoint::after(EditCommand::Backspace)
                    }
                    Some(SelectionAdjustment::Anchored) => {
                        HxExtendSelectionToInsertionPoint::after(EditCommand::Backspace)
                    }
                    None => ReedlineEvent::Edit(vec![EditCommand::Backspace]),
                },
                (KeyCode::Delete, _) => ReedlineEvent::Edit(vec![EditCommand::Delete]),
                (KeyCode::Left, _) => {
                    self.selection_adjustment = None;
                    HxClearSelection::after(EditCommand::MoveLeft { select: false })
                }
                (KeyCode::Right, _) => {
                    self.selection_adjustment = None;
                    HxClearSelection::after(EditCommand::MoveRight { select: false })
                }
                (KeyCode::Home, _) => {
                    self.selection_adjustment = None;
                    HxClearSelection::after(EditCommand::MoveToLineStart { select: false })
                }
                (KeyCode::End, _) => {
                    self.selection_adjustment = None;
                    HxClearSelection::after(EditCommand::MoveToLineEnd { select: false })
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
                        HxRestartSelection::event(),
                        ReedlineEvent::Repaint,
                    ]),
                    KeyCode::Char('i') => {
                        self.selection_adjustment = Some(SelectionAdjustment::Shifting);
                        self.enter_insert(vec![
                            HxEnsureSelection::event(),
                            HxMoveToSelectionStart::event(),
                        ])
                    }
                    KeyCode::Char('a') => {
                        self.selection_adjustment = Some(SelectionAdjustment::Anchored);
                        self.enter_insert(vec![
                            HxEnsureSelection::event(),
                            HxMoveToSelectionEnd::event(),
                        ])
                    }
                    KeyCode::Char('I') => self.enter_insert(vec![HxClearSelection::after(
                        EditCommand::MoveToLineStart { select: false },
                    )]),
                    KeyCode::Char('A') => self.enter_insert(vec![HxClearSelection::after(
                        EditCommand::MoveToLineEnd { select: false },
                    )]),
                    KeyCode::Char('h') => {
                        HxRestartSelection::after(EditCommand::MoveLeft { select: false })
                    }
                    KeyCode::Char('l') => ReedlineEvent::UntilFound(vec![
                        ReedlineEvent::HistoryHintComplete,
                        ReedlineEvent::MenuRight,
                        HxRestartSelection::after(EditCommand::MoveRight { select: false }),
                    ]),
                    KeyCode::Enter => ReedlineEvent::Enter,
                    _ => ReedlineEvent::None,
                }
            }
        }
    }

    fn edit_mode(&self) -> PromptEditMode {
        match self.mode {
            HelixMode::Insert => PromptEditMode::Vi(PromptViMode::Insert),
            HelixMode::Normal => PromptEditMode::Vi(PromptViMode::Normal),
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
            PromptEditMode::Vi(PromptViMode::Insert)
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
            ReedlineEvent::Multiple(vec![HxEnsureSelection::event(), ReedlineEvent::Repaint,])
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
                HxEnsureSelection::event(),
                HxMoveToSelectionEnd::event(),
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
                ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }]),
                HxRestartSelection::event(),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix_mode.mode, HelixMode::Normal);
        assert_eq!(helix_mode.selection_adjustment, None);
    }
}
