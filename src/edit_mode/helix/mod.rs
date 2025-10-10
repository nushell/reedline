mod helix_keybindings;

use std::str::FromStr;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
pub use helix_keybindings::{default_helix_insert_keybindings, default_helix_normal_keybindings};

use super::EditMode;
use crate::{
    edit_mode::keybindings::Keybindings,
    enums::{EditCommand, EventStatus, ReedlineEvent, ReedlineRawEvent},
    PromptEditMode, PromptViMode,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum HelixMode {
    Normal,
    Insert,
}

impl FromStr for HelixMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "normal" => Ok(HelixMode::Normal),
            "insert" => Ok(HelixMode::Insert),
            _ => Err(()),
        }
    }
}

/// This parses incoming input `Event`s using Helix-style modal editing
///
/// Helix mode starts in Normal mode by default (unlike Vi mode which starts in Insert).
/// It supports basic insert mode entry (i/a/I/A) and escape back to normal mode.
pub struct Helix {
    insert_keybindings: Keybindings,
    normal_keybindings: Keybindings,
    mode: HelixMode,
}

impl Default for Helix {
    fn default() -> Self {
        Helix {
            insert_keybindings: default_helix_insert_keybindings(),
            normal_keybindings: default_helix_normal_keybindings(),
            mode: HelixMode::Normal,
        }
    }
}

impl Helix {
    /// Creates a Helix editor with custom keybindings
    pub fn new(insert_keybindings: Keybindings, normal_keybindings: Keybindings) -> Self {
        Self {
            insert_keybindings,
            normal_keybindings,
            mode: HelixMode::Normal,
        }
    }
}

impl Helix {
    fn enter_insert_mode(&mut self, edit_command: Option<EditCommand>) -> ReedlineEvent {
        self.mode = HelixMode::Insert;
        match edit_command {
            None => ReedlineEvent::Repaint,
            Some(cmd) => ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![cmd]),
                ReedlineEvent::Repaint,
            ]),
        }
    }
}

impl EditMode for Helix {
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        match event.into() {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => match (self.mode, modifiers, code) {
                (HelixMode::Normal, KeyModifiers::NONE, KeyCode::Char('i')) => {
                    self.enter_insert_mode(None)
                }
                (HelixMode::Normal, KeyModifiers::NONE, KeyCode::Char('a')) => {
                    self.enter_insert_mode(Some(EditCommand::MoveRight { select: false }))
                }
                (HelixMode::Normal, KeyModifiers::SHIFT, KeyCode::Char('i')) => {
                    self.enter_insert_mode(Some(EditCommand::MoveToLineStart { select: false }))
                }
                (HelixMode::Normal, KeyModifiers::SHIFT, KeyCode::Char('a')) => {
                    self.enter_insert_mode(Some(EditCommand::MoveToLineEnd { select: false }))
                }
                (HelixMode::Normal, KeyModifiers::NONE, KeyCode::Char('c')) => {
                    self.enter_insert_mode(Some(EditCommand::CutSelection))
                }
                (HelixMode::Normal, _, _) => self
                    .normal_keybindings
                    .find_binding(modifiers, code)
                    .unwrap_or(ReedlineEvent::None),
                (HelixMode::Insert, KeyModifiers::NONE, KeyCode::Esc) => {
                    self.mode = HelixMode::Normal;
                    ReedlineEvent::Multiple(vec![
                        ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }]),
                        ReedlineEvent::Esc,
                        ReedlineEvent::Repaint,
                    ])
                }
                (HelixMode::Insert, KeyModifiers::NONE, KeyCode::Enter) => ReedlineEvent::Enter,
                (HelixMode::Insert, modifier, KeyCode::Char(c)) => {
                    let c = match modifier {
                        KeyModifiers::NONE => c,
                        _ => c.to_ascii_lowercase(),
                    };

                    self.insert_keybindings
                        .find_binding(modifier, KeyCode::Char(c))
                        .unwrap_or_else(|| {
                            if modifier == KeyModifiers::NONE || modifier == KeyModifiers::SHIFT {
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
                (HelixMode::Insert, _, _) => self
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
            HelixMode::Normal => PromptEditMode::Vi(PromptViMode::Normal),
            HelixMode::Insert => PromptEditMode::Vi(PromptViMode::Insert),
        }
    }

    fn handle_mode_specific_event(&mut self, _event: ReedlineEvent) -> EventStatus {
        EventStatus::Inapplicable
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn i_enters_insert_mode_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let i_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('i'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(i_key);

        assert_eq!(result, ReedlineEvent::Repaint);
        assert_eq!(helix.mode, HelixMode::Insert);
    }

    #[test]
    fn esc_returns_to_normal_mode_test() {
        let mut helix = Helix::default();
        helix.mode = HelixMode::Insert;

        let esc =
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
                .unwrap();
        let result = helix.parse_event(esc);

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }]),
                ReedlineEvent::Esc,
                ReedlineEvent::Repaint
            ])
        );
        assert_eq!(helix.mode, HelixMode::Normal);
    }

    #[test]
    fn insert_text_in_insert_mode_test() {
        let mut helix = Helix::default();
        helix.mode = HelixMode::Insert;

        let h_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('h'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(h_key);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('h')])
        );
        assert_eq!(helix.mode, HelixMode::Insert);
    }

    #[test]
    fn normal_mode_ignores_unbound_chars_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        // Use 'q' which is not bound to anything
        let q_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(q_key);

        assert_eq!(result, ReedlineEvent::None);
        assert_eq!(helix.mode, HelixMode::Normal);
    }

    #[test]
    fn a_enters_insert_after_cursor_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let a_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(a_key);

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: false }]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix.mode, HelixMode::Insert);
    }

    #[test]
    fn shift_i_enters_insert_at_line_start_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let shift_i_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('i'),
            KeyModifiers::SHIFT,
        )))
        .unwrap();
        let result = helix.parse_event(shift_i_key);

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::MoveToLineStart { select: false }]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix.mode, HelixMode::Insert);
    }

    #[test]
    fn shift_a_enters_insert_at_line_end_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let shift_a_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::SHIFT,
        )))
        .unwrap();
        let result = helix.parse_event(shift_a_key);

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::MoveToLineEnd { select: false }]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix.mode, HelixMode::Insert);
    }

    #[test]
    fn ctrl_c_aborts_in_normal_mode_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let ctrl_c = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )))
        .unwrap();
        let result = helix.parse_event(ctrl_c);

        assert_eq!(result, ReedlineEvent::CtrlC);
    }

    #[test]
    fn ctrl_c_aborts_in_insert_mode_test() {
        let mut helix = Helix::default();
        helix.mode = HelixMode::Insert;

        let ctrl_c = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )))
        .unwrap();
        let result = helix.parse_event(ctrl_c);

        assert_eq!(result, ReedlineEvent::CtrlC);
    }

    #[test]
    fn ctrl_d_exits_in_normal_mode_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let ctrl_d = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('d'),
            KeyModifiers::CONTROL,
        )))
        .unwrap();
        let result = helix.parse_event(ctrl_d);

        assert_eq!(result, ReedlineEvent::CtrlD);
    }

    #[test]
    fn ctrl_d_exits_in_insert_mode_test() {
        let mut helix = Helix::default();
        helix.mode = HelixMode::Insert;

        let ctrl_d = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('d'),
            KeyModifiers::CONTROL,
        )))
        .unwrap();
        let result = helix.parse_event(ctrl_d);

        assert_eq!(result, ReedlineEvent::CtrlD);
    }

    #[test]
    fn h_moves_left_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let h_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('h'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(h_key);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: true }])
        );
    }

    #[test]
    fn l_moves_right_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let l_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(l_key);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: true }])
        );
    }

    #[test]
    fn w_moves_word_forward_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let w_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('w'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(w_key);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveWordRightStart { select: true }])
        );
    }

    #[test]
    fn b_moves_word_back_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let b_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('b'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(b_key);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveWordLeft { select: true }])
        );
    }

    #[test]
    fn e_moves_word_end_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let e_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('e'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(e_key);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveWordRightEnd { select: true }])
        );
    }

    #[test]
    fn zero_moves_to_line_start_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let zero_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('0'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(zero_key);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveToLineStart { select: true }])
        );
    }

    #[test]
    fn dollar_moves_to_line_end_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let dollar_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('$'),
            KeyModifiers::SHIFT,
        )))
        .unwrap();
        let result = helix.parse_event(dollar_key);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveToLineEnd { select: true }])
        );
    }

    #[test]
    fn x_selects_line_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let x_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(x_key);

        assert_eq!(result, ReedlineEvent::Edit(vec![EditCommand::SelectAll]));
    }

    #[test]
    fn d_deletes_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let d_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(d_key);

        assert_eq!(result, ReedlineEvent::Edit(vec![EditCommand::CutSelection]));
    }

    #[test]
    fn semicolon_collapses_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let semicolon_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char(';'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(semicolon_key);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: false }])
        );
    }

    #[test]
    fn c_changes_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let c_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(c_key);

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::CutSelection]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix.mode, HelixMode::Insert);
    }

    #[test]
    fn y_yanks_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let y_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(y_key);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::CopySelection])
        );
    }

    #[test]
    fn p_pastes_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let p_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('p'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = helix.parse_event(p_key);

        assert_eq!(result, ReedlineEvent::Edit(vec![EditCommand::Paste]));
    }

    #[test]
    fn shift_p_pastes_before_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let p_key = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('P'),
            KeyModifiers::SHIFT,
        )))
        .unwrap();
        let result = helix.parse_event(p_key);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::PasteCutBufferBefore])
        );
    }

    #[test]
    fn alt_semicolon_swaps_cursor_and_anchor_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let alt_semicolon = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char(';'),
            KeyModifiers::ALT,
        )))
        .unwrap();
        let result = helix.parse_event(alt_semicolon);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::SwapCursorAndAnchor])
        );
    }
}
