mod helix_keybindings;

use std::str::FromStr;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
pub use helix_keybindings::{
    default_helix_insert_keybindings, default_helix_normal_keybindings,
    default_helix_select_keybindings,
};

use super::EditMode;
use crate::{
    edit_mode::keybindings::Keybindings,
    enums::{EditCommand, EventStatus, ReedlineEvent, ReedlineRawEvent},
    PromptEditMode, PromptHelixMode,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum HelixMode {
    Normal,
    Insert,
    Select,
    Goto,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum PendingCharSearch {
    Find,
    Till,
    FindBack,
    TillBack,
}

impl PendingCharSearch {
    fn to_command(self, c: char) -> EditCommand {
        match self {
            PendingCharSearch::Find => EditCommand::MoveRightUntil { c, select: true },
            PendingCharSearch::Till => EditCommand::MoveRightBefore { c, select: true },
            PendingCharSearch::FindBack => EditCommand::MoveLeftUntil { c, select: true },
            PendingCharSearch::TillBack => EditCommand::MoveLeftBefore { c, select: true },
        }
    }
}

impl FromStr for HelixMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "normal" => Ok(HelixMode::Normal),
            "insert" => Ok(HelixMode::Insert),
            "select" => Ok(HelixMode::Select),
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
    select_keybindings: Keybindings,
    mode: HelixMode,
    pending_char_search: Option<PendingCharSearch>,
    /// Command to run when exiting insert mode (e.g., move cursor left for append modes)
    insert_mode_exit_adjustment: Option<EditCommand>,
}

impl Default for Helix {
    fn default() -> Self {
        Helix {
            insert_keybindings: default_helix_insert_keybindings(),
            normal_keybindings: default_helix_normal_keybindings(),
            select_keybindings: default_helix_select_keybindings(),
            mode: HelixMode::Normal,
            pending_char_search: None,
            insert_mode_exit_adjustment: None,
        }
    }
}

impl Helix {
    /// Creates a Helix editor with custom keybindings
    pub fn new(
        insert_keybindings: Keybindings,
        normal_keybindings: Keybindings,
        select_keybindings: Keybindings,
    ) -> Self {
        Self {
            insert_keybindings,
            normal_keybindings,
            select_keybindings,
            mode: HelixMode::Normal,
            pending_char_search: None,
            insert_mode_exit_adjustment: None,
        }
    }
}

impl Helix {
    fn enter_insert_mode(
        &mut self,
        edit_command: Option<EditCommand>,
        exit_adjustment: Option<EditCommand>,
    ) -> ReedlineEvent {
        self.mode = HelixMode::Insert;
        self.insert_mode_exit_adjustment = exit_adjustment;
        match edit_command {
            None => ReedlineEvent::Repaint,
            Some(cmd) => ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![cmd]),
                ReedlineEvent::Repaint,
            ]),
        }
    }

    fn handle_pending_char_search(&mut self, code: KeyCode) -> Option<ReedlineEvent> {
        if let Some(search_type) = self.pending_char_search.take() {
            if let KeyCode::Char(c) = code {
                Some(ReedlineEvent::Edit(vec![search_type.to_command(c)]))
            } else {
                // Non-char key pressed, cancel the search
                Some(ReedlineEvent::None)
            }
        } else {
            None
        }
    }

    fn start_char_search(&mut self, search_type: PendingCharSearch) -> ReedlineEvent {
        self.pending_char_search = Some(search_type);
        ReedlineEvent::None
    }

}

impl EditMode for Helix {
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        match event.into() {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => {
                // Handle pending character search (f/t/F/T waiting for char)
                if let Some(event) = self.handle_pending_char_search(code) {
                    return event;
                }

                match (self.mode, modifiers, code) {
                    // -- Normal / Select: character keys --
                    // Normalise to lowercase so that crossterm's Shift+W
                    // (KeyCode::Char('W') + SHIFT) matches bindings registered
                    // with lowercase chars.
                    (HelixMode::Normal | HelixMode::Select, modifier, KeyCode::Char(c)) => {
                        // Normalise to lowercase so that crossterm's Shift+W
                        // (Char('W') + SHIFT) matches bindings registered with
                        // lowercase chars.  The original modifier is preserved
                        // for keybinding lookups (Ctrl, Alt, etc.). The Kitty Keyboard
                        // Protocol requires base unicode (lowercase) so we normalize to lowercase.
                        let c = c.to_ascii_lowercase();

                        match (modifier, c) {
                            // Select mode toggle
                            (KeyModifiers::NONE, 'v') if self.mode == HelixMode::Normal => {
                                self.mode = HelixMode::Select;
                                ReedlineEvent::Repaint
                            }
                            (KeyModifiers::NONE, 'v') if self.mode == HelixMode::Select => {
                                self.mode = HelixMode::Normal;
                                ReedlineEvent::Repaint
                            }
                            // Character search
                            (KeyModifiers::NONE, 'f') => {
                                self.start_char_search(PendingCharSearch::Find)
                            }
                            (KeyModifiers::NONE, 't') => {
                                self.start_char_search(PendingCharSearch::Till)
                            }
                            (KeyModifiers::SHIFT, 'f') => {
                                self.start_char_search(PendingCharSearch::FindBack)
                            }
                            (KeyModifiers::SHIFT, 't') => {
                                self.start_char_search(PendingCharSearch::TillBack)
                            }
                            // Insert mode entry
                            (KeyModifiers::NONE, 'i') => {
                                self.enter_insert_mode(
                                    Some(EditCommand::MoveToSelectionStart),
                                    None,
                                )
                            }
                            (KeyModifiers::NONE, 'a') => {
                                self.enter_insert_mode(
                                    Some(EditCommand::MoveToSelectionEnd),
                                    Some(EditCommand::MoveLeft { select: false }),
                                )
                            }
                            (KeyModifiers::SHIFT, 'i') => {
                                self.enter_insert_mode(
                                    Some(EditCommand::MoveToLineStart { select: false }),
                                    None,
                                )
                            }
                            (KeyModifiers::SHIFT, 'a') => {
                                self.enter_insert_mode(
                                    Some(EditCommand::MoveToLineEnd { select: false }),
                                    Some(EditCommand::MoveLeft { select: false }),
                                )
                            }
                            // Goto mode (only from Normal mode)
                            (KeyModifiers::NONE, 'g')
                                if self.mode == HelixMode::Normal =>
                            {
                                self.mode = HelixMode::Goto;
                                ReedlineEvent::None
                            }
                            // Change (cut + insert)
                            (KeyModifiers::NONE, 'c') => {
                                self.enter_insert_mode(Some(EditCommand::CutSelection), None)
                            }
                            // Everything else: look up in the keybinding map
                            // (handles Ctrl+C, Ctrl+D, Alt+;, W/B/E/P/U, etc.)
                            _ => {
                                let kb = if self.mode == HelixMode::Select {
                                    &self.select_keybindings
                                } else {
                                    &self.normal_keybindings
                                };
                                kb.find_binding(modifier, KeyCode::Char(c))
                                    .unwrap_or(ReedlineEvent::None)
                            }
                        }
                    }
                    // -- Normal / Select: non-character keys (Enter, Esc, etc.) --
                    (HelixMode::Select, KeyModifiers::NONE, KeyCode::Esc) => {
                        self.mode = HelixMode::Normal;
                        ReedlineEvent::Repaint
                    }
                    (HelixMode::Normal, _, _) => self
                        .normal_keybindings
                        .find_binding(modifiers, code)
                        .unwrap_or(ReedlineEvent::None),
                    (HelixMode::Select, _, _) => self
                        .select_keybindings
                        .find_binding(modifiers, code)
                        .unwrap_or(ReedlineEvent::None),
                    // -- Goto mode: second key of the g-prefix menu --
                    (HelixMode::Goto, _, KeyCode::Char('h')) => {
                        self.mode = HelixMode::Normal;
                        ReedlineEvent::Edit(vec![
                            EditCommand::MoveToLineStart { select: false },
                        ])
                    }
                    (HelixMode::Goto, _, KeyCode::Char('l')) => {
                        self.mode = HelixMode::Normal;
                        ReedlineEvent::Edit(vec![
                            EditCommand::MoveToLineEnd { select: false },
                        ])
                    }
                    (HelixMode::Goto, _, KeyCode::Char('s')) => {
                        self.mode = HelixMode::Normal;
                        ReedlineEvent::Edit(vec![
                            EditCommand::MoveToLineNonBlankStart { select: false },
                        ])
                    }
                    // Any other key in Goto mode: cancel and return to Normal
                    (HelixMode::Goto, _, _) => {
                        self.mode = HelixMode::Normal;
                        ReedlineEvent::None
                    }
                    (HelixMode::Insert, KeyModifiers::NONE, KeyCode::Esc) => {
                        self.mode = HelixMode::Normal;
                        let mut events = vec![];
                        if let Some(cmd) = self.insert_mode_exit_adjustment.take() {
                            events.push(ReedlineEvent::Edit(vec![cmd]));
                        }
                        events.extend([ReedlineEvent::Esc, ReedlineEvent::Repaint]);
                        ReedlineEvent::Multiple(events)
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
                                if modifier == KeyModifiers::NONE || modifier == KeyModifiers::SHIFT
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
                    (HelixMode::Insert, _, _) => self
                        .insert_keybindings
                        .find_binding(modifiers, code)
                        .unwrap_or(ReedlineEvent::None),
                }
            }

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
            HelixMode::Normal | HelixMode::Goto => PromptEditMode::Helix(PromptHelixMode::Normal),
            HelixMode::Insert => PromptEditMode::Helix(PromptHelixMode::Insert),
            HelixMode::Select => PromptEditMode::Helix(PromptHelixMode::Select),
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
    use proptest::prelude::*;

    fn make_key_event(code: KeyCode, modifiers: KeyModifiers) -> ReedlineRawEvent {
        ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(code, modifiers))).unwrap()
    }

    #[test]
    fn i_enters_insert_mode_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('i'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::MoveToSelectionStart]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix.mode, HelixMode::Insert);
    }

    #[test]
    fn esc_returns_to_normal_mode_test() {
        let mut helix = Helix {
            mode: HelixMode::Insert,
            ..Default::default()
        };

        let result = helix.parse_event(make_key_event(KeyCode::Esc, KeyModifiers::NONE));

        // When restore_cursor is false (default), Esc should NOT move cursor left
        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint])
        );
        assert_eq!(helix.mode, HelixMode::Normal);
    }

    #[test]
    fn insert_text_in_insert_mode_test() {
        let mut helix = Helix {
            mode: HelixMode::Insert,
            ..Default::default()
        };

        let result = helix.parse_event(make_key_event(KeyCode::Char('h'), KeyModifiers::NONE));

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

        let result = helix.parse_event(make_key_event(KeyCode::Char('q'), KeyModifiers::NONE));

        assert_eq!(result, ReedlineEvent::None);
        assert_eq!(helix.mode, HelixMode::Normal);
    }

    #[test]
    fn a_enters_insert_after_cursor_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('a'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::MoveToSelectionEnd]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix.mode, HelixMode::Insert);
        // Esc should emit MoveLeft exit adjustment
        let esc = helix.parse_event(make_key_event(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(
            esc,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }]),
                ReedlineEvent::Esc,
                ReedlineEvent::Repaint,
            ])
        );
    }

    #[test]
    fn shift_i_enters_insert_at_line_start_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('i'), KeyModifiers::SHIFT));

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

        let result = helix.parse_event(make_key_event(KeyCode::Char('a'), KeyModifiers::SHIFT));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::MoveToLineEnd { select: false }]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix.mode, HelixMode::Insert);
        // Esc should emit MoveLeft exit adjustment
        let esc = helix.parse_event(make_key_event(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(
            esc,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }]),
                ReedlineEvent::Esc,
                ReedlineEvent::Repaint,
            ])
        );
    }

    #[test]
    fn ctrl_c_aborts_in_normal_mode_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL));

        assert_eq!(result, ReedlineEvent::CtrlC);
    }

    #[test]
    fn ctrl_c_aborts_in_insert_mode_test() {
        let mut helix = Helix {
            mode: HelixMode::Insert,
            ..Default::default()
        };

        let result = helix.parse_event(make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL));

        assert_eq!(result, ReedlineEvent::CtrlC);
    }

    #[test]
    fn ctrl_d_exits_in_normal_mode_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('d'), KeyModifiers::CONTROL));

        assert_eq!(result, ReedlineEvent::CtrlD);
    }

    #[test]
    fn ctrl_d_exits_in_insert_mode_test() {
        let mut helix = Helix {
            mode: HelixMode::Insert,
            ..Default::default()
        };

        let result = helix.parse_event(make_key_event(KeyCode::Char('d'), KeyModifiers::CONTROL));

        assert_eq!(result, ReedlineEvent::CtrlD);
    }

    #[test]
    fn h_moves_left_without_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('h'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }])
        );
    }

    #[test]
    fn l_moves_right_without_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('l'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: false }])
        );
    }

    #[test]
    fn w_moves_word_forward_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('w'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::ClearSelection,
                EditCommand::MoveWordRight { select: true }
            ])
        );
    }

    #[test]
    fn b_moves_word_back_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::ClearSelection,
                EditCommand::MoveWordLeft { select: true }
            ])
        );
    }

    #[test]
    fn e_moves_word_end_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('e'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::ClearSelection,
                EditCommand::MoveWordRightEnd { select: true }
            ])
        );
    }

    #[test]
    fn shift_w_moves_bigword_forward_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('w'), KeyModifiers::SHIFT));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::ClearSelection,
                EditCommand::MoveBigWordRight { select: true }
            ])
        );
    }

    #[test]
    fn shift_b_moves_bigword_back_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::SHIFT));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::ClearSelection,
                EditCommand::MoveBigWordLeft { select: true }
            ])
        );
    }

    #[test]
    fn shift_e_moves_bigword_end_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('e'), KeyModifiers::SHIFT));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::ClearSelection,
                EditCommand::MoveBigWordRightEnd { select: true }
            ])
        );
    }


    #[test]
    fn x_selects_current_line_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('x'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveToLineStart { select: false },
                EditCommand::MoveToLineEnd { select: true },
            ])
        );
    }

    #[test]
    fn d_deletes_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('d'), KeyModifiers::NONE));

        assert_eq!(result, ReedlineEvent::Edit(vec![EditCommand::CutSelection]));
    }

    #[test]
    fn semicolon_collapses_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char(';'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::ClearSelection])
        );
    }

    #[test]
    fn c_changes_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('c'), KeyModifiers::NONE));

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

        let result = helix.parse_event(make_key_event(KeyCode::Char('y'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::CopySelection])
        );
    }

    #[test]
    fn p_pastes_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('p'), KeyModifiers::NONE));

        assert_eq!(result, ReedlineEvent::Edit(vec![EditCommand::Paste]));
    }

    #[test]
    fn shift_p_pastes_before_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('p'), KeyModifiers::SHIFT));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::PasteCutBufferBefore])
        );
    }

    #[test]
    fn alt_semicolon_swaps_cursor_and_anchor_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char(';'), KeyModifiers::ALT));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::SwapCursorAndAnchor])
        );
    }

    #[test]
    fn f_char_finds_next_char_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result1 = helix.parse_event(make_key_event(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(result1, ReedlineEvent::None);
        assert_eq!(helix.pending_char_search, Some(PendingCharSearch::Find));

        let result2 = helix.parse_event(make_key_event(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(
            result2,
            ReedlineEvent::Edit(vec![EditCommand::MoveRightUntil {
                c: 'x',
                select: true
            }])
        );
        assert_eq!(helix.pending_char_search, None);
    }

    #[test]
    fn t_char_moves_till_next_char_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result1 = helix.parse_event(make_key_event(KeyCode::Char('t'), KeyModifiers::NONE));
        assert_eq!(result1, ReedlineEvent::None);
        assert_eq!(helix.pending_char_search, Some(PendingCharSearch::Till));

        let result2 = helix.parse_event(make_key_event(KeyCode::Char('y'), KeyModifiers::NONE));
        assert_eq!(
            result2,
            ReedlineEvent::Edit(vec![EditCommand::MoveRightBefore {
                c: 'y',
                select: true
            }])
        );
        assert_eq!(helix.pending_char_search, None);
    }

    #[test]
    fn shift_f_finds_previous_char_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result1 = helix.parse_event(make_key_event(KeyCode::Char('f'), KeyModifiers::SHIFT));
        assert_eq!(result1, ReedlineEvent::None);
        assert_eq!(helix.pending_char_search, Some(PendingCharSearch::FindBack));

        let result2 = helix.parse_event(make_key_event(KeyCode::Char('z'), KeyModifiers::NONE));
        assert_eq!(
            result2,
            ReedlineEvent::Edit(vec![EditCommand::MoveLeftUntil {
                c: 'z',
                select: true
            }])
        );
        assert_eq!(helix.pending_char_search, None);
    }

    #[test]
    fn shift_t_moves_till_previous_char_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result1 = helix.parse_event(make_key_event(KeyCode::Char('t'), KeyModifiers::SHIFT));
        assert_eq!(result1, ReedlineEvent::None);
        assert_eq!(helix.pending_char_search, Some(PendingCharSearch::TillBack));

        let result2 = helix.parse_event(make_key_event(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(
            result2,
            ReedlineEvent::Edit(vec![EditCommand::MoveLeftBefore {
                c: 'a',
                select: true
            }])
        );
        assert_eq!(helix.pending_char_search, None);
    }

    #[test]
    fn v_enters_select_mode_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('v'), KeyModifiers::NONE));

        assert_eq!(result, ReedlineEvent::Repaint);
        assert_eq!(helix.mode, HelixMode::Select);
    }

    #[test]
    fn v_exits_select_mode_test() {
        let mut helix = Helix {
            mode: HelixMode::Select,
            ..Default::default()
        };

        let result = helix.parse_event(make_key_event(KeyCode::Char('v'), KeyModifiers::NONE));

        assert_eq!(result, ReedlineEvent::Repaint);
        assert_eq!(helix.mode, HelixMode::Normal);
    }

    #[test]
    fn esc_exits_select_mode_test() {
        let mut helix = Helix {
            mode: HelixMode::Select,
            ..Default::default()
        };

        let result = helix.parse_event(make_key_event(KeyCode::Esc, KeyModifiers::NONE));

        assert_eq!(result, ReedlineEvent::Repaint);
        assert_eq!(helix.mode, HelixMode::Normal);
    }

    #[test]
    fn pending_char_search_cancels_on_non_char_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result1 = helix.parse_event(make_key_event(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(result1, ReedlineEvent::None);
        assert_eq!(helix.pending_char_search, Some(PendingCharSearch::Find));

        let result2 = helix.parse_event(make_key_event(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(result2, ReedlineEvent::None);
        assert_eq!(helix.pending_char_search, None);
    }

    #[test]
    fn normal_mode_returns_normal_prompt_mode_test() {
        let helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);
        assert_eq!(
            helix.edit_mode(),
            PromptEditMode::Helix(PromptHelixMode::Normal)
        );
    }

    #[test]
    fn insert_mode_returns_insert_prompt_mode_test() {
        let helix = Helix {
            mode: HelixMode::Insert,
            ..Default::default()
        };
        assert_eq!(
            helix.edit_mode(),
            PromptEditMode::Helix(PromptHelixMode::Insert)
        );
    }

    #[test]
    fn select_mode_returns_select_prompt_mode_test() {
        let helix = Helix {
            mode: HelixMode::Select,
            ..Default::default()
        };
        assert_eq!(
            helix.edit_mode(),
            PromptEditMode::Helix(PromptHelixMode::Select)
        );
    }

    #[test]
    fn cursor_selection_sync_after_insert_mode_test() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        // Start in normal mode, enter append mode with 'a' (restore_cursor = true)
        let _result = helix.parse_event(make_key_event(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Insert);
        editor.set_edit_mode(helix.edit_mode());

        // Type some text in insert mode
        for event in &[
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('h')]),
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('e')]),
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('l')]),
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('l')]),
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('o')]),
        ] {
            if let ReedlineEvent::Edit(commands) = event {
                for cmd in commands {
                    editor.run_edit_command(cmd);
                }
            }
        }

        // Verify we have "hello" and cursor is at position 5 (end of buffer)
        assert_eq!(editor.get_buffer(), "hello");
        assert_eq!(editor.insertion_point(), 5);

        // Exit insert mode with Esc - since we entered with 'a', restore_cursor=true, so cursor moves left
        let result = helix.parse_event(make_key_event(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Normal);
        editor.set_edit_mode(helix.edit_mode());

        // In Helix, when entering via append (a), Esc moves cursor left to restore position
        // The result includes MoveLeft, then Esc (which resets selection), then Repaint
        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }]),
                ReedlineEvent::Esc,
                ReedlineEvent::Repaint,
            ])
        );

        // Apply the move left command
        if let ReedlineEvent::Multiple(events) = result {
            for event in events {
                if let ReedlineEvent::Edit(commands) = event {
                    for cmd in commands {
                        editor.run_edit_command(&cmd);
                    }
                }
            }
        }

        // After Esc + MoveLeft, cursor should be at position 4
        assert_eq!(editor.insertion_point(), 4);

        // After processing ReedlineEvent::Esc, selection should be reset
        // The cursor should still be at position 5

        // Now press 'h' to move left in normal mode
        // Starting from pos 4 (on the 'o'), we want to move left to pos 3 (on the second 'l')
        let result = helix.parse_event(make_key_event(KeyCode::Char('h'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());

        // Expected: MoveLeft{false} — h/l just move the cursor, no selection
        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }])
        );

        // Execute these commands on the editor
        if let ReedlineEvent::Edit(commands) = result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        // MoveLeft{false} collapses selection to 1-char at cursor
        assert_eq!(editor.insertion_point(), 3);
        assert_eq!(editor.get_selection(), Some((3, 4)));
    }

    #[test]
    fn cursor_selection_sync_after_mode_transitions_test() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer("world".to_string(), crate::UndoBehavior::CreateUndoPoint);
        // set_buffer moves cursor to end, so move it back to start
        editor.run_edit_command(&EditCommand::MoveToStart { select: false });
        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        // Start at position 0 in normal mode
        // Move right with 'l' — just moves cursor, no selection
        let result = helix.parse_event(make_key_event(KeyCode::Char('l'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        // MoveRight{false} collapses selection to 1-char at cursor
        assert_eq!(editor.insertion_point(), 1);
        assert_eq!(editor.get_selection(), Some((1, 2)));

        // Enter insert mode with 'i'
        let _result = helix.parse_event(make_key_event(KeyCode::Char('i'), KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Insert);
        editor.set_edit_mode(helix.edit_mode());

        // Exit insert mode with Esc - since we entered with 'i', restore_cursor=false, so NO cursor movement
        let result = helix.parse_event(make_key_event(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Normal);
        editor.set_edit_mode(helix.edit_mode());

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint,])
        );

        // Apply the Esc commands (no MoveLeft since entered with 'i')
        if let ReedlineEvent::Multiple(events) = result {
            for event in events {
                if let ReedlineEvent::Edit(commands) = event {
                    for cmd in commands {
                        editor.run_edit_command(&cmd);
                    }
                }
            }
        }

        // Cursor stays at position 1
        assert_eq!(editor.insertion_point(), 1);

        // Now move left with 'h' from position 1 — just moves cursor, no selection
        let result = helix.parse_event(make_key_event(KeyCode::Char('h'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        // MoveLeft{false} collapses selection to 1-char at cursor
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.get_selection(), Some((0, 1)));
    }

    #[test]
    fn e_motion_highlights_full_word_from_start_test() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer(
            "hello world".to_string(),
            crate::UndoBehavior::CreateUndoPoint,
        );
        editor.run_edit_command(&EditCommand::MoveToStart { select: false });

        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        let result = helix.parse_event(make_key_event(KeyCode::Char('e'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::ClearSelection,
                EditCommand::MoveWordRightEnd { select: true }
            ])
        );

        if let ReedlineEvent::Edit(commands) = result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.get_selection(), Some((0, 5)));
    }

    #[test]
    fn w_motion_selects_to_next_word_start_test() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer(
            "hello world".to_string(),
            crate::UndoBehavior::CreateUndoPoint,
        );
        editor.run_edit_command(&EditCommand::MoveToStart { select: false });

        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        let result = helix.parse_event(make_key_event(KeyCode::Char('w'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::ClearSelection,
                EditCommand::MoveWordRight { select: true }
            ])
        );

        if let ReedlineEvent::Edit(commands) = result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        // Cursor lands on 'w' (pos 5); inclusive selection covers "hello "
        assert_eq!(editor.insertion_point(), 5);
        assert_eq!(editor.get_selection(), Some((0, 6)));
    }

    #[test]
    fn test_b_selection_from_end_detailed() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer(
            "hello world".to_string(),
            crate::UndoBehavior::CreateUndoPoint,
        );
        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        // First 'b' from end (pos 11)
        let first_b = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = first_b {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        // Cursor on 'w' (pos 6), anchor at end (pos 11) → "world"
        assert_eq!(editor.insertion_point(), 6);
        assert_eq!(editor.get_selection(), Some((6, 11)));

        // Second 'b' from start of "world"
        let second_b = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = second_b {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        // Cursor on 'h' (pos 0), anchor at pos 6 → "hello " (does NOT bleed into "world")
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.get_selection(), Some((0, 6)));
    }

    #[test]
    fn repeated_b_motion_clears_previous_word_selection() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer(
            "alpha beta gamma".to_string(),
            crate::UndoBehavior::CreateUndoPoint,
        );
        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        // First `b` from end (pos 16): selects "gamma"
        let first_b = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = first_b {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        } else {
            panic!("Expected ReedlineEvent::Edit for initial `b` motion");
        }
        assert_eq!(editor.insertion_point(), 11);
        assert_eq!(editor.get_selection(), Some((11, 16)));

        // Second `b`: fresh selection from pos 11 backward to pos 6 ("beta")
        // anchor=11, cursor=6, inclusive → (6, 12)
        let second_b = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = second_b {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        } else {
            panic!("Expected ReedlineEvent::Edit for second `b` motion");
        }
        assert_eq!(editor.insertion_point(), 6);
    }

    #[test]
    fn select_mode_keeps_anchor_on_backward_motion() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer(
            "alpha beta gamma".to_string(),
            crate::UndoBehavior::CreateUndoPoint,
        );
        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        // Enter select mode with 'v'
        let _ = helix.parse_event(make_key_event(KeyCode::Char('v'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        assert_eq!(helix.mode, HelixMode::Select);

        // Perform first backward word motion in select mode
        let first_b = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = first_b {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        } else {
            panic!("Expected ReedlineEvent::Edit for first `b` in select mode");
        }
        assert_eq!(editor.get_selection(), Some((11, 16)));

        // Second `b` should extend the selection while keeping anchor at the buffer end
        let second_b = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = second_b {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        } else {
            panic!("Expected ReedlineEvent::Edit for second `b` in select mode");
        }
        assert_eq!(editor.get_selection(), Some((6, 16)));
    }

    // Property-based tests using proptest to verify character search works correctly
    // for arbitrary characters. Split into separate tests for each keybind.

    proptest! {
        /// Property: 'f' (find forward) should find the character and move cursor to it
        /// Tests both command generation AND actual editor behavior
        #[test]
        fn property_f_find_forward_any_char(
            search_char_str in "[a-zA-Z0-9 ]",  // ASCII alphanum and space only for reliable testing
            prefix in "[a-z]{1,5}",  // Random prefix before the search char (at least 1 char)
            suffix in "[a-z]{0,5}"   // Random suffix after the search char
        ) {
            use crate::core_editor::Editor;

            // Convert string to char (regex generates a single-char string)
            let search_char = search_char_str.chars().next().unwrap();

            let mut helix = Helix::default();
            prop_assert_eq!(helix.mode, HelixMode::Normal);

            // Press 'f' to enter find mode
            let f_result = helix.parse_event(make_key_event(KeyCode::Char('f'), KeyModifiers::NONE));
            prop_assert_eq!(f_result, ReedlineEvent::None);
            prop_assert_eq!(helix.pending_char_search, Some(PendingCharSearch::Find));

            // Press the search character
            let char_result = helix.parse_event(make_key_event(KeyCode::Char(search_char), KeyModifiers::NONE));
            prop_assert_eq!(
                char_result,
                ReedlineEvent::Edit(vec![EditCommand::MoveRightUntil { c: search_char, select: true }])
            );
            prop_assert_eq!(helix.pending_char_search, None);

            // PROPERTY-BASED ASSERTION: Test actual editor behavior
            // Create a buffer with the search character in it
            let buffer_content = format!("{}{}{}", prefix, search_char, suffix);

            // MoveRightUntil searches for the character AFTER the current cursor position
            // We need to work with character indices (not byte indices) for Unicode safety
            let chars: Vec<char> = buffer_content.chars().collect();

            // Skip test if buffer is empty or too short
            prop_assume!(chars.len() >= 2);

            // For this test, we need the search char to appear AFTER position 0
            // So we skip if the search character is at position 0
            let start_char_idx = 0;

            // Find the first occurrence of search_char AFTER the starting character index
            let expected_char_idx = chars.iter()
                .skip(start_char_idx + 1)
                .position(|&c| c == search_char)
                .map(|pos| start_char_idx + 1 + pos);

            // Only test if the character exists after the starting cursor position
            if let Some(expected_idx) = expected_char_idx {
                let mut editor = Editor::default();
                editor.set_buffer(buffer_content.clone(), crate::UndoBehavior::CreateUndoPoint);
                editor.run_edit_command(&EditCommand::MoveToStart { select: false });
                editor.set_edit_mode(helix.edit_mode());

                // Execute the find command
                let find_command = EditCommand::MoveRightUntil { c: search_char, select: true };
                editor.run_edit_command(&find_command);

                // Convert character index to byte position for comparison with editor
                let expected_byte_pos = chars.iter().take(expected_idx).map(|c| c.len_utf8()).sum::<usize>();

                // PROPERTY: The cursor should move to the first occurrence of the search character
                // AFTER the current cursor position (not including the position we're at)
                prop_assert_eq!(
                    editor.insertion_point(),
                    expected_byte_pos,
                    "Cursor should move to byte position {} (character {} '{}' in buffer '{}')",
                    expected_byte_pos,
                    expected_idx,
                    search_char,
                    buffer_content
                );

                // Additional property: The selection should exist and include the found character
                let selection = editor.get_selection();
                prop_assert!(
                    selection.is_some(),
                    "Selection should exist after find command"
                );

                // Property: The selection should start at the initial cursor position
                if let Some((sel_start, _sel_end)) = selection {
                    prop_assert_eq!(sel_start, 0, "Selection should start at the initial cursor position (byte 0)");
                }
            }
        }

        /// Property: 't' (till forward) should produce MoveRightBefore for any valid character
        #[test]
        fn property_t_till_forward_any_char(
            search_char in any::<char>().prop_filter("Valid printable chars", |c| c.is_alphanumeric() || c.is_whitespace())
        ) {
            let mut helix = Helix::default();
            prop_assert_eq!(helix.mode, HelixMode::Normal);

            // Press 't' to enter till mode
            let t_result = helix.parse_event(make_key_event(KeyCode::Char('t'), KeyModifiers::NONE));
            prop_assert_eq!(t_result, ReedlineEvent::None);
            prop_assert_eq!(helix.pending_char_search, Some(PendingCharSearch::Till));

            // Press the search character
            let char_result = helix.parse_event(make_key_event(KeyCode::Char(search_char), KeyModifiers::NONE));
            prop_assert_eq!(
                char_result,
                ReedlineEvent::Edit(vec![EditCommand::MoveRightBefore { c: search_char, select: true }])
            );
            prop_assert_eq!(helix.pending_char_search, None);
        }

        /// Property: 'F' (find backward) should produce MoveLeftUntil for any valid character
        #[test]
        fn property_shift_f_find_backward_any_char(
            search_char in any::<char>().prop_filter("Valid printable chars", |c| c.is_alphanumeric() || c.is_whitespace())
        ) {
            let mut helix = Helix::default();
            prop_assert_eq!(helix.mode, HelixMode::Normal);

            // Press 'F' (shift+f) to enter find backward mode
            let f_back_result = helix.parse_event(make_key_event(KeyCode::Char('f'), KeyModifiers::SHIFT));
            prop_assert_eq!(f_back_result, ReedlineEvent::None);
            prop_assert_eq!(helix.pending_char_search, Some(PendingCharSearch::FindBack));

            // Press the search character
            let char_result = helix.parse_event(make_key_event(KeyCode::Char(search_char), KeyModifiers::NONE));
            prop_assert_eq!(
                char_result,
                ReedlineEvent::Edit(vec![EditCommand::MoveLeftUntil { c: search_char, select: true }])
            );
            prop_assert_eq!(helix.pending_char_search, None);
        }

        /// Property: 'T' (till backward) should produce MoveLeftBefore for any valid character
        #[test]
        fn property_shift_t_till_backward_any_char(
            search_char in any::<char>().prop_filter("Valid printable chars", |c| c.is_alphanumeric() || c.is_whitespace())
        ) {
            let mut helix = Helix::default();
            prop_assert_eq!(helix.mode, HelixMode::Normal);

            // Press 'T' (shift+t) to enter till backward mode
            let t_back_result = helix.parse_event(make_key_event(KeyCode::Char('t'), KeyModifiers::SHIFT));
            prop_assert_eq!(t_back_result, ReedlineEvent::None);
            prop_assert_eq!(helix.pending_char_search, Some(PendingCharSearch::TillBack));

            // Press the search character
            let char_result = helix.parse_event(make_key_event(KeyCode::Char(search_char), KeyModifiers::NONE));
            prop_assert_eq!(
                char_result,
                ReedlineEvent::Edit(vec![EditCommand::MoveLeftBefore { c: search_char, select: true }])
            );
            prop_assert_eq!(helix.pending_char_search, None);
        }

        // /// Property: 'w' (word forward) should move cursor right before the start of the next word
        // /// with an inclusive selection from the old position to the new position.
        // #[test]
        // fn property_w_word_forward_movement(
        //     word1 in "[a-z]{1,5}",        // First word (lowercase letters)
        //     word2 in "[a-z]{1,5}",        // Second word
        //     separator in "[ ]{1,3}",    // Whitespace between words
        // ) {
        //     use crate::core_editor::Editor;

        //     let mut helix = Helix::default();
        //     prop_assert_eq!(helix.mode, HelixMode::Normal);

        //     let buffer_content = format!("{}{}{}", word1, separator, word2);

        //     let mut editor = Editor::default();
        //     editor.set_buffer(buffer_content.clone(), crate::UndoBehavior::CreateUndoPoint);
        //     editor.run_edit_command(&EditCommand::MoveToStart { select: false });
        //     editor.set_edit_mode(helix.edit_mode());

        //     // Press 'w' to move forward one word
        //     let w_result = helix.parse_event(make_key_event(KeyCode::Char('w'), KeyModifiers::NONE));
        //     let w_commands = vec![
        //         EditCommand::ClearSelection,
        //         EditCommand::MoveWordRight { select: true },
        //     ];
        //     prop_assert_eq!(
        //         w_result,
        //         ReedlineEvent::Edit(w_commands.clone())
        //     );
        //     for cmd in &w_commands {
        //         editor.run_edit_command(cmd);
        //     }

        //     // PROPERTY 1: Cursor should land right before start of word2
        //     let expected_pos = word1.len() + separator.len() - 1;
        //     prop_assert_eq!(
        //         editor.insertion_point(),
        //         expected_pos,
        //         "After 'w' from start, cursor should be right before word2 start ({}) in buffer '{}'",
        //         expected_pos,
        //         buffer_content
        //     );

        //     // PROPERTY 2: A selection should exist (inclusive, from 0 to cursor+1)
        //     let selection = editor.get_selection();
        //     prop_assert!(selection.is_some(), "Selection should exist after 'w'");
        //     if let Some((sel_start, _sel_end)) = selection {
        //         prop_assert_eq!(sel_start, 0, "Selection should start at position 0");
        //     }
        // }

        // /// Property: Multiple 'w' presses should traverse all words in a buffer
        // /// Tests that repeated word forward movements eventually reach the end
        // #[test]
        // fn property_w_multiple_movements_reach_end(
        //     words in prop::collection::vec("[a-z]{1,5}", 2..=5),  // 2-5 words
        // ) {
        //     use crate::core_editor::Editor;

        //     let mut helix = Helix::default();

        //     // Create buffer with words separated by single spaces
        //     let buffer_content = words.join(" ");
        //     let buffer_len = buffer_content.len();

        //     // Skip if buffer is too short
        //     prop_assume!(buffer_len > 0);

        //     let mut editor = Editor::default();
        //     editor.set_buffer(buffer_content.clone(), crate::UndoBehavior::CreateUndoPoint);
        //     editor.run_edit_command(&EditCommand::MoveToStart { select: false });
        //     editor.set_edit_mode(helix.edit_mode());

        //     let initial_pos = editor.insertion_point();

        //     // Press 'w' enough times to definitely move through all words
        //     // (words.len() + 1 should be more than enough)
        //     for _ in 0..words.len() + 1 {
        //         let w_result = helix.parse_event(make_key_event(KeyCode::Char('w'), KeyModifiers::NONE));
        //         if let ReedlineEvent::Edit(commands) = w_result {
        //             for cmd in &commands {
        //                 editor.run_edit_command(cmd);
        //             }
        //         }
        //     }

        //     // PROPERTY: After enough 'w' presses, cursor should have moved from start
        //     let final_pos = editor.insertion_point();
        //     prop_assert!(
        //         final_pos > initial_pos,
        //         "After {} 'w' movements, cursor should move from position {} in buffer '{}' (words: {:?})",
        //         words.len() + 1,
        //         initial_pos,
        //         buffer_content,
        //         words
        //     );

        //     // PROPERTY: Cursor should not go past the end of the buffer
        //     prop_assert!(
        //         final_pos <= buffer_len,
        //         "Cursor at position {} should not exceed buffer length {} for buffer '{}'",
        //         final_pos,
        //         buffer_len,
        //         buffer_content
        //     );
        // }

        /// Property: 'b' (word backward) should move cursor backward to the start
        /// of the previous word with an inclusive selection.
        #[test]
        fn property_b_word_backward_movement(
            word1 in "[a-z]{1,5}",        // First word (lowercase letters)
            word2 in "[a-z]{1,5}",        // Second word
            separator in "[ \t]{1,3}",    // Whitespace between words
        ) {
            use crate::core_editor::Editor;

            let mut helix = Helix::default();
            prop_assert_eq!(helix.mode, HelixMode::Normal);

            let buffer_content = format!("{}{}{}", word1, separator, word2);

            let mut editor = Editor::default();
            editor.set_buffer(buffer_content.clone(), crate::UndoBehavior::CreateUndoPoint);
            let _start_pos = editor.insertion_point();
            editor.set_edit_mode(helix.edit_mode());

            // Press 'b' to move backward one word
            let b_result = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
            let b_commands = vec![
                EditCommand::ClearSelection,
                EditCommand::MoveWordLeft { select: true },
            ];
            prop_assert_eq!(
                b_result,
                ReedlineEvent::Edit(b_commands.clone())
            );
            for cmd in &b_commands {
                editor.run_edit_command(cmd);
            }

            // PROPERTY 1: Cursor should land at start of word2
            let word2_start = word1.len() + separator.len();
            prop_assert_eq!(
                editor.insertion_point(),
                word2_start,
                "Cursor should land at start of word2 ({}) in buffer '{}'",
                word2_start,
                buffer_content
            );

            // PROPERTY 2: Selection should exist
            let selection = editor.get_selection();
            prop_assert!(selection.is_some(), "Selection should exist after 'b'");

            // PROPERTY 3: Selection should cover from cursor to original position
            if let Some((sel_start, _sel_end)) = selection {
                prop_assert_eq!(
                    sel_start,
                    word2_start,
                    "Selection start should be at cursor ({}) for buffer '{}'",
                    word2_start,
                    buffer_content
                );
            }
        }

        /// Property: pressing 'e' then 'b' from the start: 'e' selects the first word,
        /// 'b' creates a fresh backward selection from the cursor position.
    #[test]
    fn property_e_then_b_keeps_first_word_selection(
        word1 in "[a-z]{1,5}",
        word2 in "[a-z]{1,5}",
        separator in "[ \t]{1,3}",
    ) {
            use crate::core_editor::Editor;

            let mut helix = Helix::default();
            prop_assert_eq!(helix.mode, HelixMode::Normal);

            let buffer_content = format!("{}{}{}", word1, separator, word2);
            let mut editor = Editor::default();
            editor.set_buffer(buffer_content.clone(), crate::UndoBehavior::CreateUndoPoint);
            editor.run_edit_command(&EditCommand::MoveToStart { select: false });
            editor.set_edit_mode(helix.edit_mode());

            // Step 5: press 'e' — selects from start to end of first word
            let e_event = helix.parse_event(make_key_event(KeyCode::Char('e'), KeyModifiers::NONE));
            let e_commands = vec![
                EditCommand::ClearSelection,
                EditCommand::MoveWordRightEnd { select: true },
            ];
            prop_assert_eq!(
                e_event,
                ReedlineEvent::Edit(e_commands.clone())
            );
            for cmd in &e_commands {
                editor.run_edit_command(cmd);
            }

            let (first_sel_start, first_sel_end) = editor
                .get_selection()
                .expect("Selection should exist after pressing 'e'");
            prop_assert_eq!(first_sel_start, 0);
            let first_slice = &editor.get_buffer()[first_sel_start..first_sel_end];
            prop_assume!(first_slice == word1.as_str());

            // Step 6: press 'b' — creates fresh selection backward from cursor
            let b_event = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
            let b_commands = vec![
                EditCommand::ClearSelection,
                EditCommand::MoveWordLeft { select: true },
            ];
            prop_assert_eq!(
                b_event,
                ReedlineEvent::Edit(b_commands.clone())
            );
            for cmd in &b_commands {
                editor.run_edit_command(cmd);
            }

            // Cursor should be at start of buffer
            prop_assert_eq!(editor.insertion_point(), 0);
            // Selection should exist
            let selection = editor.get_selection();
            prop_assert!(selection.is_some(), "Selection should exist after 'b'");
        }

        /// Property: Multiple 'b' presses from end should traverse all words backward
        /// Tests that repeated backward movements eventually reach the start
        #[test]
        fn property_b_multiple_movements_reach_start(
            words in prop::collection::vec("[a-z]{1,5}", 2..=5),  // 2-5 words
        ) {
            use crate::core_editor::Editor;

            let mut helix = Helix::default();

            // Create buffer with words separated by single spaces
            let buffer_content = words.join(" ");
            let buffer_len = buffer_content.len();

            // Skip if buffer is too short
            prop_assume!(buffer_len > 0);

            let mut editor = Editor::default();
            editor.set_buffer(buffer_content.clone(), crate::UndoBehavior::CreateUndoPoint);
            // Cursor starts at end after set_buffer
            editor.set_edit_mode(helix.edit_mode());

            let initial_pos = editor.insertion_point();

            // Press 'b' enough times to traverse all words backward
            // (words.len() + 1 should be more than enough)
            for _ in 0..words.len() + 1 {
                let b_result = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
                if let ReedlineEvent::Edit(commands) = b_result {
                    for cmd in &commands {
                        editor.run_edit_command(cmd);
                    }
                }
            }

            // PROPERTY: After enough 'b' presses, cursor should have moved backward
            let final_pos = editor.insertion_point();
            prop_assert!(
                final_pos < initial_pos,
                "After {} 'b' movements, cursor should move backward from position {} to {} in buffer '{}' (words: {:?})",
                words.len() + 1,
                initial_pos,
                final_pos,
                buffer_content,
                words
            );

            // PROPERTY: Cursor should not be negative
            prop_assert!(
                final_pos <= buffer_len,
                "Cursor at position {} should not exceed buffer length {} for buffer '{}'",
                final_pos,
                buffer_len,
                buffer_content
            );

            // PROPERTY: Cursor should be near the start after enough backward movements
            // After enough 'b' presses, we should be at or near position 0
            let first_word_len = words.first().map(|w| w.len()).unwrap_or(0);
            prop_assert!(
                final_pos <= first_word_len,
                "After {} 'b' movements, cursor at {} should be in or before first word for buffer '{}' (words: {:?})",
                words.len() + 1,
                final_pos,
                buffer_content,
                words
            );
        }
    }

    #[test]
    fn tutorial_step_6_and_7_workflow_test() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer(
            "hello world".to_string(),
            crate::UndoBehavior::CreateUndoPoint,
        );
        editor.run_edit_command(&EditCommand::MoveToStart { select: false });

        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        // Press 'e' from start: selects "hello" (inclusive, cursor on 'o' at pos 4)
        let e_result = helix.parse_event(make_key_event(KeyCode::Char('e'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = e_result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }
        assert_eq!(editor.get_selection(), Some((0, 5)), "'e' should select 'hello'");

        // Press 'b': fresh backward selection from pos 4 to pos 0
        let b_event = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = b_event {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }
        assert_eq!(editor.insertion_point(), 0, "'b' cursor should be at start");
        assert!(editor.get_selection().is_some(), "'b' should have a selection");

        // Press 'w': fresh forward selection from pos 0 to space before "world" (pos 5)
        // Inclusive selection covers "hello" = (0, 6)
        let w_event = helix.parse_event(make_key_event(KeyCode::Char('w'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = w_event {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }
        assert_eq!(editor.insertion_point(), 5, "'w' cursor should be on space before 'world'");
        assert_eq!(editor.get_selection(), Some((0, 6)), "'w' inclusive selection covers 'hello '");

        // Press 'w' again: fresh forward selection from pos 6 to end (pos 11)
        // "hello world" has 11 chars; MoveWordRightStart from pos 6 goes to 11 (end)
        let w2_event = helix.parse_event(make_key_event(KeyCode::Char('w'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = w2_event {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }
        // At end of buffer, cursor stays at 11; selection from 6 to 11
        assert!(editor.get_selection().is_some(), "second 'w' should have a selection");
    }

    // =====================================================================
    // Regression tests for ClearSelection-based anchor reset
    // =====================================================================

    /// Pressing 'l' at position 0 should move the cursor right without
    /// creating a selection (cursor and anchor stay coincidental).
    #[test]
    fn l_motion_from_buffer_start_moves_without_selection_test() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer("hello".to_string(), crate::UndoBehavior::CreateUndoPoint);
        editor.run_edit_command(&EditCommand::MoveToStart { select: false });

        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        // Press 'l' to move right from position 0
        let l_result = helix.parse_event(make_key_event(KeyCode::Char('l'), KeyModifiers::NONE));
        if let ReedlineEvent::Edit(commands) = l_result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        // Cursor should be at position 1, 1-char selection at cursor
        assert_eq!(editor.insertion_point(), 1);
        assert_eq!(
            editor.get_selection(),
            Some((1, 2)),
            "h/l should collapse to 1-char selection at cursor"
        );
    }

    /// Pressing 'h' at the end of the buffer should move the cursor left
    /// with a 1-char selection at the new cursor position.
    #[test]
    fn h_motion_from_buffer_end_moves_without_selection_test() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer("hello".to_string(), crate::UndoBehavior::CreateUndoPoint);
        // set_buffer places cursor at end (position 5, past last char)

        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        // Press 'h' to move left from end
        let h_result = helix.parse_event(make_key_event(KeyCode::Char('h'), KeyModifiers::NONE));
        if let ReedlineEvent::Edit(commands) = h_result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        // Cursor should be at position 4, 1-char selection at cursor
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(
            editor.get_selection(),
            Some((4, 5)),
            "h/l should collapse to 1-char selection at cursor"
        );
    }

    /// After making a selection with 'e', pressing ';' should collapse the
    /// selection while keeping the cursor at its current position.
    /// Regression test: the old implementation used MoveRight{select:false}
    /// which moved the cursor right by one character.
    #[test]
    fn semicolon_collapses_without_moving_cursor_test() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer(
            "hello world".to_string(),
            crate::UndoBehavior::CreateUndoPoint,
        );
        editor.run_edit_command(&EditCommand::MoveToStart { select: false });

        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        // Press 'e' to select "hello" (cursor at 4, anchor at 0)
        let e_result = helix.parse_event(make_key_event(KeyCode::Char('e'), KeyModifiers::NONE));
        if let ReedlineEvent::Edit(commands) = e_result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.get_selection(), Some((0, 5))); // "hello" selected

        // Press ';' to collapse selection
        let semi_result =
            helix.parse_event(make_key_event(KeyCode::Char(';'), KeyModifiers::NONE));
        if let ReedlineEvent::Edit(commands) = semi_result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        // Cursor should stay at position 4 (on the 'o' of "hello")
        assert_eq!(
            editor.insertion_point(),
            4,
            "';' should not move the cursor"
        );
        // Selection should collapse to 1-char at cursor
        assert_eq!(
            editor.get_selection(),
            Some((4, 5)),
            "';' should collapse to 1-char selection at cursor"
        );
    }

    /// In Select mode, ';' should also collapse the selection without moving.
    #[test]
    fn semicolon_collapses_in_select_mode_test() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer(
            "alpha beta gamma".to_string(),
            crate::UndoBehavior::CreateUndoPoint,
        );

        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        // Enter select mode
        let _ = helix.parse_event(make_key_event(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Select);
        editor.set_edit_mode(helix.edit_mode());

        // Move backward with 'b' to create a selection
        let b_result = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
        if let ReedlineEvent::Edit(commands) = b_result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        let pos_before_semi = editor.insertion_point();
        assert!(
            editor.get_selection().is_some(),
            "Should have a selection before ';'"
        );

        // Press ';' to collapse
        let semi_result =
            helix.parse_event(make_key_event(KeyCode::Char(';'), KeyModifiers::NONE));
        if let ReedlineEvent::Edit(commands) = semi_result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        assert_eq!(
            editor.insertion_point(),
            pos_before_semi,
            "';' in select mode should not move the cursor"
        );
        // Selection collapses to 1-char at cursor in Helix mode
        assert_eq!(
            editor.get_selection(),
            Some((pos_before_semi, pos_before_semi + 1)),
            "';' in select mode should collapse to 1-char selection"
        );
    }

    /// Repeated 'l' presses from position 0 should move the cursor right
    /// one position each time without creating any selection.
    #[test]
    fn repeated_l_from_start_moves_without_selection_test() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer("abcde".to_string(), crate::UndoBehavior::CreateUndoPoint);
        editor.run_edit_command(&EditCommand::MoveToStart { select: false });

        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        // First 'l': cursor moves from 0 to 1, no selection
        let result = helix.parse_event(make_key_event(KeyCode::Char('l'), KeyModifiers::NONE));
        if let ReedlineEvent::Edit(commands) = result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }
        assert_eq!(editor.insertion_point(), 1);
        assert_eq!(editor.get_selection(), Some((1, 2)));

        // Second 'l': cursor moves from 1 to 2, 1-char selection
        let result = helix.parse_event(make_key_event(KeyCode::Char('l'), KeyModifiers::NONE));
        if let ReedlineEvent::Edit(commands) = result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }
        assert_eq!(editor.insertion_point(), 2);
        assert_eq!(editor.get_selection(), Some((2, 3)));

        // Third 'l': cursor moves from 2 to 3, 1-char selection
        let result = helix.parse_event(make_key_event(KeyCode::Char('l'), KeyModifiers::NONE));
        if let ReedlineEvent::Edit(commands) = result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }
        assert_eq!(editor.insertion_point(), 3);
        assert_eq!(editor.get_selection(), Some((3, 4)));
    }

    /// On a single-line buffer, 'x' should select the entire line content.
    #[test]
    fn x_selects_single_line_content_test() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer(
            "hello world".to_string(),
            crate::UndoBehavior::CreateUndoPoint,
        );
        // Place cursor in the middle
        editor.run_edit_command(&EditCommand::MoveToPosition {
            position: 5,
            select: false,
        });

        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        let result = helix.parse_event(make_key_event(KeyCode::Char('x'), KeyModifiers::NONE));
        if let ReedlineEvent::Edit(commands) = result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        // Cursor should be at end of line, selection covering the whole line
        let selection = editor.get_selection().expect("x should create a selection");
        let selected = &editor.get_buffer()[selection.0..selection.1];
        assert_eq!(
            selected, "hello world",
            "x should select the entire current line"
        );
    }

    /// On a multi-line buffer, 'x' should select only the current line
    /// (including its trailing newline), not the entire buffer.
    #[test]
    fn x_selects_current_line_in_multiline_buffer_test() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer(
            "first\nsecond\nthird".to_string(),
            crate::UndoBehavior::CreateUndoPoint,
        );
        // Move cursor into the second line ("second")
        editor.run_edit_command(&EditCommand::MoveToPosition {
            position: 8,
            select: false,
        });

        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        let result = helix.parse_event(make_key_event(KeyCode::Char('x'), KeyModifiers::NONE));
        if let ReedlineEvent::Edit(commands) = result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        let selection = editor
            .get_selection()
            .expect("x should create a selection on the current line");
        let selected = &editor.get_buffer()[selection.0..selection.1];
        // In Helix, 'x' selects the full line including the trailing newline.
        // MoveToLineEnd lands on the \n, and inclusive selection extends one
        // grapheme right, so the newline is included in the selection.
        assert_eq!(
            selected, "second\n",
            "x should select the current line (including trailing newline), not the entire buffer"
        );
    }

    /// Pressing 'c' should cut the selection and enter insert mode.
    /// This is handled by parse_event, not the keybinding map.
    #[test]
    fn c_enters_insert_mode_via_parse_event_test() {
        use crate::core_editor::Editor;

        let mut editor = Editor::default();
        editor.set_buffer(
            "hello world".to_string(),
            crate::UndoBehavior::CreateUndoPoint,
        );
        editor.run_edit_command(&EditCommand::MoveToStart { select: false });

        let mut helix = Helix::default();
        editor.set_edit_mode(helix.edit_mode());

        // First select a word with 'e'
        let e_result = helix.parse_event(make_key_event(KeyCode::Char('e'), KeyModifiers::NONE));
        if let ReedlineEvent::Edit(commands) = e_result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }
        assert_eq!(helix.mode, HelixMode::Normal);
        assert!(
            editor.get_selection().is_some(),
            "Should have selection after 'e'"
        );

        // Press 'c' to change: should cut selection and enter insert mode
        let c_result = helix.parse_event(make_key_event(KeyCode::Char('c'), KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Insert);

        // Apply the result
        if let ReedlineEvent::Multiple(events) = c_result {
            for event in events {
                if let ReedlineEvent::Edit(commands) = event {
                    for cmd in &commands {
                        editor.run_edit_command(cmd);
                    }
                }
            }
        }

        // "hello" should have been deleted, leaving " world"
        assert_eq!(editor.get_buffer(), " world");
    }

    // =====================================================================
    // Shift-modifier tests using real crossterm key event format
    //
    // Crossterm reports shifted letter keys with the UPPERCASE char,
    // e.g. pressing Shift+W sends KeyCode::Char('W') + SHIFT.
    // These tests verify that Helix correctly normalises to lowercase
    // before matching parse_event arms and keybinding lookups.
    // =====================================================================

    /// Helper: create a key event the way crossterm actually reports a
    /// shifted letter press (uppercase char + SHIFT modifier).
    fn make_shifted_key(letter: char) -> ReedlineRawEvent {
        make_key_event(
            KeyCode::Char(letter.to_ascii_uppercase()),
            KeyModifiers::SHIFT,
        )
    }

    #[test]
    fn shift_i_uppercase_enters_insert_at_line_start_test() {
        let mut helix = Helix::default();
        let result = helix.parse_event(make_shifted_key('I'));
        assert_eq!(helix.mode, HelixMode::Insert);
        // Should produce a MoveToLineStart command
        if let ReedlineEvent::Multiple(events) = result {
            let has_move_to_start = events.iter().any(|e| {
                matches!(
                    e,
                    ReedlineEvent::Edit(cmds) if cmds.contains(&EditCommand::MoveToLineStart { select: false })
                )
            });
            assert!(
                has_move_to_start,
                "Shift+I should produce MoveToLineStart"
            );
        } else {
            panic!("Shift+I should produce ReedlineEvent::Multiple, got {result:?}");
        }
    }

    #[test]
    fn shift_a_uppercase_enters_insert_at_line_end_test() {
        let mut helix = Helix::default();
        let result = helix.parse_event(make_shifted_key('A'));
        assert_eq!(helix.mode, HelixMode::Insert);
        if let ReedlineEvent::Multiple(events) = result {
            let has_move_to_end = events.iter().any(|e| {
                matches!(
                    e,
                    ReedlineEvent::Edit(cmds) if cmds.contains(&EditCommand::MoveToLineEnd { select: false })
                )
            });
            assert!(has_move_to_end, "Shift+A should produce MoveToLineEnd");
        } else {
            panic!("Shift+A should produce ReedlineEvent::Multiple, got {result:?}");
        }
    }

    #[test]
    fn shift_f_uppercase_enters_find_back_mode_test() {
        let mut helix = Helix::default();
        let result = helix.parse_event(make_shifted_key('F'));
        assert_eq!(result, ReedlineEvent::None);
        assert_eq!(helix.pending_char_search, Some(PendingCharSearch::FindBack));
    }

    #[test]
    fn shift_t_uppercase_enters_till_back_mode_test() {
        let mut helix = Helix::default();
        let result = helix.parse_event(make_shifted_key('T'));
        assert_eq!(result, ReedlineEvent::None);
        assert_eq!(
            helix.pending_char_search,
            Some(PendingCharSearch::TillBack)
        );
    }

    #[test]
    fn shift_w_uppercase_moves_bigword_forward_test() {
        let mut helix = Helix::default();
        let result = helix.parse_event(make_shifted_key('W'));
        // Should match the Shift+w keybinding (WORD forward)
        assert!(
            matches!(result, ReedlineEvent::Edit(_)),
            "Shift+W should produce an edit event, got {result:?}"
        );
    }

    #[test]
    fn shift_b_uppercase_moves_bigword_back_test() {
        let mut helix = Helix::default();
        let result = helix.parse_event(make_shifted_key('B'));
        assert!(
            matches!(result, ReedlineEvent::Edit(_)),
            "Shift+B should produce an edit event, got {result:?}"
        );
    }

    #[test]
    fn shift_e_uppercase_moves_bigword_end_test() {
        let mut helix = Helix::default();
        let result = helix.parse_event(make_shifted_key('E'));
        assert!(
            matches!(result, ReedlineEvent::Edit(_)),
            "Shift+E should produce an edit event, got {result:?}"
        );
    }

    #[test]
    fn shift_p_uppercase_pastes_before_test() {
        let mut helix = Helix::default();
        let result = helix.parse_event(make_shifted_key('P'));
        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::PasteCutBufferBefore]),
            "Shift+P should paste before cursor"
        );
    }

    #[test]
    fn shift_u_uppercase_redoes_test() {
        let mut helix = Helix::default();
        let result = helix.parse_event(make_shifted_key('U'));
        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::Redo]),
            "Shift+U should redo"
        );
    }

    #[test]
    fn shift_w_uppercase_in_select_mode_test() {
        let mut helix = Helix::default();
        // Enter select mode
        let _ = helix.parse_event(make_key_event(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Select);

        let result = helix.parse_event(make_shifted_key('W'));
        assert!(
            matches!(result, ReedlineEvent::Edit(_)),
            "Shift+W in Select mode should produce an edit event, got {result:?}"
        );
    }

    #[test]
    fn shift_f_uppercase_in_select_mode_test() {
        let mut helix = Helix::default();
        let _ = helix.parse_event(make_key_event(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Select);

        let result = helix.parse_event(make_shifted_key('F'));
        assert_eq!(result, ReedlineEvent::None);
        assert_eq!(helix.pending_char_search, Some(PendingCharSearch::FindBack));
    }

    #[test]
    fn shift_i_uppercase_from_select_mode_enters_insert_test() {
        let mut helix = Helix::default();
        let _ = helix.parse_event(make_key_event(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Select);

        let _ = helix.parse_event(make_shifted_key('I'));
        assert_eq!(
            helix.mode,
            HelixMode::Insert,
            "Shift+I from Select mode should enter Insert"
        );
    }

    #[test]
    fn g_h_moves_to_line_start_normal_mode_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        // Press 'g' — enters Goto mode
        let result1 = helix.parse_event(make_key_event(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(result1, ReedlineEvent::None);
        assert_eq!(helix.mode, HelixMode::Goto);

        // Press 'h' — goto line start (collapses selection), returns to Normal
        let result2 = helix.parse_event(make_key_event(KeyCode::Char('h'), KeyModifiers::NONE));
        assert_eq!(
            result2,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveToLineStart { select: false },
            ])
        );
        assert_eq!(helix.mode, HelixMode::Normal);
    }

    #[test]
    fn g_l_moves_to_line_end_normal_mode_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let _ = helix.parse_event(make_key_event(KeyCode::Char('g'), KeyModifiers::NONE));
        let result = helix.parse_event(make_key_event(KeyCode::Char('l'), KeyModifiers::NONE));
        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveToLineEnd { select: false },
            ])
        );
    }

    #[test]
    fn g_s_moves_to_non_blank_start_normal_mode_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let _ = helix.parse_event(make_key_event(KeyCode::Char('g'), KeyModifiers::NONE));
        let result = helix.parse_event(make_key_event(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveToLineNonBlankStart { select: false },
            ])
        );
    }

    #[test]
    fn g_does_not_enter_goto_from_select_mode_test() {
        let mut helix = Helix::default();
        let _ = helix.parse_event(make_key_event(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Select);

        // 'g' in Select mode should NOT enter Goto mode (falls through to keybinding map)
        let result = helix.parse_event(make_key_event(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Select, "g should not change mode in Select");
        assert_eq!(result, ReedlineEvent::None);
    }

    #[test]
    fn g_unknown_key_cancels_goto_mode_test() {
        let mut helix = Helix::default();
        let _ = helix.parse_event(make_key_event(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Goto);

        let result = helix.parse_event(make_key_event(KeyCode::Char('z'), KeyModifiers::NONE));
        assert_eq!(result, ReedlineEvent::None);
        assert_eq!(helix.mode, HelixMode::Normal);
    }
}
