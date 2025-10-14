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
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
                EditCommand::MoveRight { select: false },
                EditCommand::MoveWordRightStart { select: true }
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
                EditCommand::MoveLeft { select: false },
                EditCommand::MoveRight { select: false },
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
                EditCommand::MoveLeft { select: false },
                EditCommand::MoveRight { select: false },
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
                EditCommand::MoveLeft { select: false },
                EditCommand::MoveRight { select: false },
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
                EditCommand::MoveLeft { select: false },
                EditCommand::MoveRight { select: false },
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

        // Start in normal mode, enter append mode with 'a' (restore_cursor = true)
        let _result = helix.parse_event(make_key_event(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Insert);

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

        // Start at position 0 in normal mode
        // Move right with 'l' - the sequence is: MoveLeft{false}, MoveRight{false}, MoveRight{true}
        // From position 0: stays at 0, moves to 1, moves to 2 with selection
        let result = helix.parse_event(make_key_event(KeyCode::Char('l'), KeyModifiers::NONE));
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

        // In insert mode, selection should be cleared automatically
        // when transitioning (though we'd need to test this with full engine)

        // Exit insert mode with Esc - since we entered with 'i', restore_cursor=false, so NO cursor movement
        let result = helix.parse_event(make_key_event(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Normal);

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
}
