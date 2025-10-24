mod helix_keybindings;

use std::str::FromStr;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
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
                    (HelixMode::Normal, KeyModifiers::NONE, KeyCode::Char('v')) => {
                        self.mode = HelixMode::Select;
                        ReedlineEvent::Repaint
                    }
                    (HelixMode::Select, KeyModifiers::NONE, KeyCode::Char('v'))
                    | (HelixMode::Select, KeyModifiers::NONE, KeyCode::Esc) => {
                        self.mode = HelixMode::Normal;
                        ReedlineEvent::Repaint
                    }
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::NONE,
                        KeyCode::Char('f'),
                    ) => self.start_char_search(PendingCharSearch::Find),
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::NONE,
                        KeyCode::Char('t'),
                    ) => self.start_char_search(PendingCharSearch::Till),
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::SHIFT,
                        KeyCode::Char('f'),
                    ) => self.start_char_search(PendingCharSearch::FindBack),
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::SHIFT,
                        KeyCode::Char('t'),
                    ) => self.start_char_search(PendingCharSearch::TillBack),
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::NONE,
                        KeyCode::Char('i'),
                    ) => {
                        self.mode = HelixMode::Normal;
                        self.enter_insert_mode(None, None)
                    }
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::NONE,
                        KeyCode::Char('a'),
                    ) => {
                        self.mode = HelixMode::Normal;
                        self.enter_insert_mode(
                            Some(EditCommand::MoveRight { select: false }),
                            Some(EditCommand::MoveLeft { select: false }),
                        )
                    }
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::SHIFT,
                        KeyCode::Char('i'),
                    ) => {
                        self.mode = HelixMode::Normal;
                        self.enter_insert_mode(
                            Some(EditCommand::MoveToLineStart { select: false }),
                            None,
                        )
                    }
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::SHIFT,
                        KeyCode::Char('a'),
                    ) => {
                        self.mode = HelixMode::Normal;
                        self.enter_insert_mode(
                            Some(EditCommand::MoveToLineEnd { select: false }),
                            Some(EditCommand::MoveLeft { select: false }),
                        )
                    }
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::NONE,
                        KeyCode::Char('c'),
                    ) => {
                        self.mode = HelixMode::Normal;
                        self.enter_insert_mode(Some(EditCommand::CutSelection), None)
                    }
                    (HelixMode::Normal, _, _) => self
                        .normal_keybindings
                        .find_binding(modifiers, code)
                        .unwrap_or(ReedlineEvent::None),
                    (HelixMode::Select, _, _) => self
                        .select_keybindings
                        .find_binding(modifiers, code)
                        .unwrap_or(ReedlineEvent::None),
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
            HelixMode::Normal => PromptEditMode::Helix(PromptHelixMode::Normal),
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

        assert_eq!(result, ReedlineEvent::Repaint);
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
    fn h_moves_left_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('h'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
                EditCommand::MoveRight { select: false },
                EditCommand::MoveLeft { select: true }
            ])
        );
    }

    #[test]
    fn l_moves_right_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('l'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
                EditCommand::MoveRight { select: false },
                EditCommand::MoveRight { select: true }
            ])
        );
    }

    #[test]
    fn w_moves_word_forward_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('w'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::HelixWordRightGap])
        );
    }

    #[test]
    fn b_moves_word_back_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::HelixWordLeft])
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
                EditCommand::MoveLeft { select: false },
                EditCommand::MoveRight { select: false },
                EditCommand::MoveBigWordRightStart { select: true }
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
                EditCommand::MoveBigWordLeft { select: false },
                EditCommand::MoveBigWordRightEnd { select: true },
                EditCommand::SwapCursorAndAnchor
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
    fn zero_moves_to_line_start_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('0'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
                EditCommand::MoveRight { select: false },
                EditCommand::MoveToLineStart { select: true }
            ])
        );
    }

    #[test]
    fn dollar_moves_to_line_end_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('$'), KeyModifiers::SHIFT));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
                EditCommand::MoveRight { select: false },
                EditCommand::MoveToLineEnd { select: true }
            ])
        );
    }

    #[test]
    fn x_selects_line_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('x'), KeyModifiers::NONE));

        assert_eq!(result, ReedlineEvent::Edit(vec![EditCommand::SelectAll]));
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
            ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: false }])
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

        // Expected: MoveLeft{false}, MoveRight{false}, MoveLeft{true}
        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
                EditCommand::MoveRight { select: false },
                EditCommand::MoveLeft { select: true }
            ])
        );

        // Execute these commands on the editor
        if let ReedlineEvent::Edit(commands) = result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        // After MoveLeft{false}, MoveRight{false}, MoveLeft{true}:
        // Starting from pos 4:
        // 1. MoveLeft{false} -> pos 3, no selection
        // 2. MoveRight{false} -> pos 4, no selection
        // 3. MoveLeft{true} -> pos 3, selection anchor at 4, cursor at 3
        // get_selection() returns (cursor, grapheme_right_from_anchor)
        // So cursor should be at 3, anchor at 4, and selection should be (3, 5)
        // This selects the character at position 3 ('l') and 4 ('o')
        assert_eq!(editor.insertion_point(), 3);
        assert_eq!(editor.get_selection(), Some((3, 5)));
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
        // Move right with 'l' - the sequence is: MoveLeft{false}, MoveRight{false}, MoveRight{true}
        // From position 0: stays at 0, moves to 1, moves to 2 with selection
        let result = helix.parse_event(make_key_event(KeyCode::Char('l'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        // After the movement sequence, we should be at position 2
        // with selection from position 1 to 2 (which displays as chars at index 1)
        // get_selection returns (1, grapheme_right_from(2)) = (1, 3)
        assert_eq!(editor.insertion_point(), 2);
        assert_eq!(editor.get_selection(), Some((1, 3)));

        // Enter insert mode with 'i'
        let _result = helix.parse_event(make_key_event(KeyCode::Char('i'), KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Insert);
        editor.set_edit_mode(helix.edit_mode());

        // In insert mode, selection should be cleared automatically
        // when transitioning (though we'd need to test this with full engine)

        // Exit insert mode with Esc - since we entered with 'i', restore_cursor=false, so NO cursor movement
        let result = helix.parse_event(make_key_event(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Normal);
        editor.set_edit_mode(helix.edit_mode());

        // When entering via insert (i), Esc should NOT move cursor left
        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint,])
        );

        // Apply the Esc commands (no MoveLeft)
        if let ReedlineEvent::Multiple(events) = result {
            for event in events {
                if let ReedlineEvent::Edit(commands) = event {
                    for cmd in commands {
                        editor.run_edit_command(&cmd);
                    }
                }
            }
        }

        // After exiting insert mode from position 2, cursor stays at position 2
        assert_eq!(editor.insertion_point(), 2);

        // Now move left with 'h' from position 2
        // The sequence MoveLeft{false}, MoveRight{false}, MoveLeft{true} becomes:
        // 1. MoveLeft{false} -> moves to pos 1
        // 2. MoveRight{false} -> moves back to pos 2
        // 3. MoveLeft{true} -> moves to pos 1, with anchor at 2, creating selection
        let result = helix.parse_event(make_key_event(KeyCode::Char('h'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        // After the move sequence, cursor should be at 1 with selection including char at pos 1
        // get_selection returns (1, grapheme_right_from(2)) = (1, 3)
        assert_eq!(editor.insertion_point(), 1);
        assert_eq!(editor.get_selection(), Some((1, 3)));
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
    fn w_motion_lands_in_gap_between_words_test() {
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
            ReedlineEvent::Edit(vec![EditCommand::HelixWordRightGap])
        );

        if let ReedlineEvent::Edit(commands) = result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

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

        println!("\n=== Initial state ===");
        println!("Buffer: '{}'", editor.get_buffer());
        println!("Cursor: {}", editor.insertion_point());

        // First 'b' from end
        println!("\n=== First 'b' press (from position 11) ===");
        let first_b = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = first_b {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        println!("Cursor: {}", editor.insertion_point());
        println!("Selection: {:?}", editor.get_selection());
        if let Some((start, end)) = editor.get_selection() {
            println!("Selected text: '{}'", &editor.get_buffer()[start..end]);
        }

        // Second 'b' from start of "world"
        println!("\n=== Second 'b' press (from position 6, start of 'world') ===");
        let second_b = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = second_b {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        println!("Cursor: {}", editor.insertion_point());
        println!("Selection: {:?}", editor.get_selection());
        if let Some((start, end)) = editor.get_selection() {
            println!("Selected text: '{}'", &editor.get_buffer()[start..end]);
            println!("Expected: 'hello ' (hello + space)");

            assert_eq!(editor.get_selection(), Some((0, 6)));
        }
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

        // First `b`: expect to select the trailing word "gamma"
        let first_b = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = first_b {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        } else {
            panic!("Expected ReedlineEvent::Edit for initial `b` motion");
        }
        assert_eq!(editor.get_selection(), Some((11, 16)));

        // Second `b`: should clear the previous selection and select "beta " (including space)
        let second_b = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = second_b {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        } else {
            panic!("Expected ReedlineEvent::Edit for second `b` motion");
        }
        assert_eq!(editor.get_selection(), Some((6, 11)));
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

        /// Property: 'w' (word forward) should move cursor forward by words
        /// Tests that word movement respects word boundaries (alphanumeric vs whitespace/punctuation)
        #[test]
        fn property_w_word_forward_movement(
            word1 in "[a-z]{1,5}",        // First word (lowercase letters)
            word2 in "[a-z]{1,5}",        // Second word
            separator in "[ \t]{1,3}",    // Whitespace between words
        ) {
            use crate::core_editor::Editor;

            let mut helix = Helix::default();
            prop_assert_eq!(helix.mode, HelixMode::Normal);

            // Create a buffer with two words separated by whitespace
            let buffer_content = format!("{}{}{}", word1, separator, word2);

            let mut editor = Editor::default();
            editor.set_buffer(buffer_content.clone(), crate::UndoBehavior::CreateUndoPoint);
            editor.run_edit_command(&EditCommand::MoveToStart { select: false });
            editor.set_edit_mode(helix.edit_mode());

            // Press 'w' to move forward one word
            let w_result = helix.parse_event(make_key_event(KeyCode::Char('w'), KeyModifiers::NONE));
            prop_assert_eq!(
                w_result,
                ReedlineEvent::Edit(vec![EditCommand::HelixWordRightGap])
            );

            // Execute the command (recreate it to avoid move)
            editor.run_edit_command(&EditCommand::HelixWordRightGap);

            // PROPERTY 1: Cursor should have moved forward from the start
            let actual_cursor_pos = editor.insertion_point();
            prop_assert!(
                actual_cursor_pos > 0,
                "After 'w' from start, cursor should move forward from position 0 in buffer '{}'",
                buffer_content
            );

            // PROPERTY 2: Cursor should have moved past word1
            // The cursor should be at least past word1
            let min_pos = word1.len();
            prop_assert!(
                actual_cursor_pos >= min_pos,
                "After 'w', cursor at {} should be at least {} (past word1 '{}') for buffer '{}'",
                actual_cursor_pos,
                min_pos,
                word1,
                buffer_content
            );

            // PROPERTY 2b: Cursor should not exceed the buffer length
            prop_assert!(
                actual_cursor_pos <= buffer_content.len(),
                "After 'w', cursor at {} should not exceed buffer length {} for buffer '{}'",
                actual_cursor_pos,
                buffer_content.len(),
                buffer_content
            );

            // PROPERTY 3: A selection should exist after the movement
            let selection = editor.get_selection();
            prop_assert!(
                selection.is_some(),
                "Selection should exist after word movement"
            );

            // PROPERTY 4: Selection should start from the original cursor position (0)
            if let Some((sel_start, sel_end)) = selection {
                prop_assert_eq!(
                    sel_start, 0,
                    "Selection should start at position 0 (anchor preserved)"
                );
                prop_assert!(
                    sel_end >= actual_cursor_pos,
                    "Selection end ({}) should be at or after cursor at position {}",
                    sel_end,
                    actual_cursor_pos
                );
            }
        }

        /// Property: Multiple 'w' presses should traverse all words in a buffer
        /// Tests that repeated word forward movements eventually reach the end
        #[test]
        fn property_w_multiple_movements_reach_end(
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
            editor.run_edit_command(&EditCommand::MoveToStart { select: false });
            editor.set_edit_mode(helix.edit_mode());

            let initial_pos = editor.insertion_point();

            // Press 'w' enough times to definitely move through all words
            // (words.len() + 1 should be more than enough)
            for _ in 0..words.len() + 1 {
                let w_result = helix.parse_event(make_key_event(KeyCode::Char('w'), KeyModifiers::NONE));
                if let ReedlineEvent::Edit(commands) = w_result {
                    for cmd in &commands {
                        editor.run_edit_command(cmd);
                    }
                }
            }

            // PROPERTY: After enough 'w' presses, cursor should have moved from start
            let final_pos = editor.insertion_point();
            prop_assert!(
                final_pos > initial_pos,
                "After {} 'w' movements, cursor should move from position {} in buffer '{}' (words: {:?})",
                words.len() + 1,
                initial_pos,
                buffer_content,
                words
            );

            // PROPERTY: Cursor should not go past the end of the buffer
            prop_assert!(
                final_pos <= buffer_len,
                "Cursor at position {} should not exceed buffer length {} for buffer '{}'",
                final_pos,
                buffer_len,
                buffer_content
            );
        }

        /// Property: 'b' (word backward) should move cursor backward by words
        /// Tests backward word movement with various word and separator combinations
        #[test]
        fn property_b_word_backward_movement(
            word1 in "[a-z]{1,5}",        // First word (lowercase letters)
            word2 in "[a-z]{1,5}",        // Second word
            separator in "[ \t]{1,3}",    // Whitespace between words
        ) {
            use crate::core_editor::Editor;

            let mut helix = Helix::default();
            prop_assert_eq!(helix.mode, HelixMode::Normal);

            // Create a buffer with two words separated by whitespace
            let buffer_content = format!("{}{}{}", word1, separator, word2);
            let buffer_len = buffer_content.len();

            let mut editor = Editor::default();
            editor.set_buffer(buffer_content.clone(), crate::UndoBehavior::CreateUndoPoint);
            // set_buffer positions cursor at end - this is our starting position for 'b'
            let start_pos = editor.insertion_point();
            editor.set_edit_mode(helix.edit_mode());

            // Press 'b' to move backward one word
            // Expected command sequence for 'b' in normal mode
            let b_result = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
        prop_assert_eq!(
            b_result,
            ReedlineEvent::Edit(vec![EditCommand::HelixWordLeft])
        );

        editor.run_edit_command(&EditCommand::HelixWordLeft);

            // PROPERTY 1: Cursor should have moved backward from the end
            let actual_cursor_pos = editor.insertion_point();
            let word2_start = word1.len() + separator.len();
            prop_assert!(
                actual_cursor_pos < start_pos,
                "After 'b' from end (pos {}), cursor should move backward to position {} in buffer '{}'",
                start_pos,
                actual_cursor_pos,
                buffer_content
            );

            prop_assert_eq!(
                actual_cursor_pos,
                word2_start,
                "Cursor should land at start of trailing word (index {}) for buffer '{}'",
                word2_start,
                buffer_content
            );

            // PROPERTY 2: Cursor should not be negative or exceed buffer length
            prop_assert!(
                actual_cursor_pos <= buffer_len,
                "After 'b', cursor at {} should not exceed buffer length {} for buffer '{}'",
                actual_cursor_pos,
                buffer_len,
                buffer_content
            );

            // PROPERTY 3: Selection exists and matches the trailing word exactly (no gap)
            if let Some((sel_start, sel_end)) = editor.get_selection() {
                prop_assert_eq!(
                    sel_start,
                    actual_cursor_pos,
                    "Selection should begin at cursor position ({}) for buffer '{}'",
                    actual_cursor_pos,
                    buffer_content
                );
                prop_assert_eq!(
                    sel_end,
                    start_pos,
                    "Selection should extend back to the original cursor position ({}) for buffer '{}'",
                    start_pos,
                    buffer_content
                );

                let selected_slice = &editor.get_buffer()[sel_start..sel_end];
                prop_assert_eq!(
                    selected_slice,
                    word2.as_str(),
                    "Selection after 'b' should match trailing word '{}', got '{}'",
                    word2,
                    selected_slice
                );
            } else {
                prop_assert!(false, "Selection should exist after backward word movement");
            }
        }

        /// Property: pressing 'e' then 'b' from the start highlights the first word
        /// without pulling in the trailing separator. This mirrors tutorial Step 5/6.
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

            // Step 5: press 'e'
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

            // Step 6: press 'b'
            let b_event = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
            prop_assert_eq!(
                b_event,
                ReedlineEvent::Edit(vec![EditCommand::HelixWordLeft])
            );
            editor.run_edit_command(&EditCommand::HelixWordLeft);

            let (sel_start, sel_end) = editor
                .get_selection()
                .expect("Selection should persist after pressing 'b'");
            prop_assert_eq!(sel_start, 0);
            let selected_slice = &editor.get_buffer()[sel_start..sel_end];
            prop_assert_eq!(selected_slice, word1.as_str());
            prop_assert_eq!(editor.insertion_point(), 0);
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

        // Simulate: After pressing 'e' from start (step 5), we should have "hello" selected
        let e_result = helix.parse_event(make_key_event(KeyCode::Char('e'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = e_result {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }
        if let Some((start, end)) = editor.get_selection() {
            assert_eq!(
                &editor.get_buffer()[start..end],
                "hello",
                "Step 5: pressing 'e' should select 'hello'"
            );
        } else {
            panic!("Step 5 should result in an active selection");
        }

        // Step 6 (part 1): Press 'b' to return to the start of 'hello'
        let b_event = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = b_event {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }
        if let Some((start, end)) = editor.get_selection() {
            assert_eq!(
                &editor.get_buffer()[start..end],
                "hello",
                "Step 6: 'b' should keep the selection focused on 'hello'"
            );
            assert_eq!(
                editor.insertion_point(),
                0,
                "Step 6: cursor should return to start of 'hello'"
            );
        } else {
            panic!("Step 6 'b' should maintain an active selection");
        }

        // Step 6 (part 2): Press first 'w' - this demonstrates the selection behavior
        let first_w = helix.parse_event(make_key_event(KeyCode::Char('w'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = first_w {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }
        if let Some((start, end)) = editor.get_selection() {
            assert_eq!(
                &editor.get_buffer()[start..end],
                "hello ",
                "Step 6: first 'w' should extend the selection to include the trailing space"
            );
        } else {
            panic!("Step 6 should maintain an active selection");
        }

        // Step 7: Press second 'w' - this should select 'world'
        let second_w = helix.parse_event(make_key_event(KeyCode::Char('w'), KeyModifiers::NONE));
        editor.set_edit_mode(helix.edit_mode());
        if let ReedlineEvent::Edit(commands) = second_w {
            for cmd in &commands {
                editor.run_edit_command(cmd);
            }
        }

        if let Some((start, end)) = editor.get_selection() {
            // Verify that only 'world' remains selected
            let selected_text = &editor.get_buffer()[start..end];
            assert_eq!(
                selected_text, "world",
                "Step 7: second 'w' should deselect 'hello' and select only 'world'"
            );
        } else {
            panic!("Step 7 should result in an active selection");
        }
    }
}
