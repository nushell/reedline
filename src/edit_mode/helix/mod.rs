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
    mode: HelixMode,
    pending_char_search: Option<PendingCharSearch>,
}

impl Default for Helix {
    fn default() -> Self {
        Helix {
            insert_keybindings: default_helix_insert_keybindings(),
            normal_keybindings: default_helix_normal_keybindings(),
            mode: HelixMode::Normal,
            pending_char_search: None,
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
            pending_char_search: None,
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
                        ReedlineEvent::Multiple(vec![
                            ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }]),
                            ReedlineEvent::Repaint,
                        ])
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
                        KeyCode::Char('F'),
                    ) => self.start_char_search(PendingCharSearch::FindBack),
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::SHIFT,
                        KeyCode::Char('T'),
                    ) => self.start_char_search(PendingCharSearch::TillBack),
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::NONE,
                        KeyCode::Char('i'),
                    ) => {
                        self.mode = HelixMode::Normal;
                        self.enter_insert_mode(None)
                    }
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::NONE,
                        KeyCode::Char('a'),
                    ) => {
                        self.mode = HelixMode::Normal;
                        self.enter_insert_mode(Some(EditCommand::MoveRight { select: false }))
                    }
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::SHIFT,
                        KeyCode::Char('i'),
                    ) => {
                        self.mode = HelixMode::Normal;
                        self.enter_insert_mode(Some(EditCommand::MoveToLineStart { select: false }))
                    }
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::SHIFT,
                        KeyCode::Char('a'),
                    ) => {
                        self.mode = HelixMode::Normal;
                        self.enter_insert_mode(Some(EditCommand::MoveToLineEnd { select: false }))
                    }
                    (
                        HelixMode::Normal | HelixMode::Select,
                        KeyModifiers::NONE,
                        KeyCode::Char('c'),
                    ) => {
                        self.mode = HelixMode::Normal;
                        self.enter_insert_mode(Some(EditCommand::CutSelection))
                    }
                    (HelixMode::Normal | HelixMode::Select, _, _) => self
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
            HelixMode::Normal => PromptEditMode::Vi(PromptViMode::Normal),
            HelixMode::Insert => PromptEditMode::Vi(PromptViMode::Insert),
            HelixMode::Select => PromptEditMode::Vi(PromptViMode::Select),
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
        let mut helix = Helix::default();
        helix.mode = HelixMode::Insert;

        let result = helix.parse_event(make_key_event(KeyCode::Esc, KeyModifiers::NONE));

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
        let mut helix = Helix::default();
        helix.mode = HelixMode::Insert;

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
        let mut helix = Helix::default();
        helix.mode = HelixMode::Insert;

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
            ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: true }])
        );
    }

    #[test]
    fn l_moves_right_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('l'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: true }])
        );
    }

    #[test]
    fn w_moves_word_forward_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('w'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveWordRightStart { select: true }])
        );
    }

    #[test]
    fn b_moves_word_back_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('b'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveWordLeft { select: true }])
        );
    }

    #[test]
    fn e_moves_word_end_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('e'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveWordRightEnd { select: true }])
        );
    }

    #[test]
    fn shift_w_moves_bigword_forward_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('W'), KeyModifiers::SHIFT));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveBigWordRightStart { select: true }])
        );
    }

    #[test]
    fn shift_b_moves_bigword_back_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('B'), KeyModifiers::SHIFT));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveBigWordLeft { select: true }])
        );
    }

    #[test]
    fn shift_e_moves_bigword_end_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('E'), KeyModifiers::SHIFT));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveBigWordRightEnd { select: true }])
        );
    }

    #[test]
    fn zero_moves_to_line_start_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('0'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveToLineStart { select: true }])
        );
    }

    #[test]
    fn dollar_moves_to_line_end_with_selection_test() {
        let mut helix = Helix::default();
        assert_eq!(helix.mode, HelixMode::Normal);

        let result = helix.parse_event(make_key_event(KeyCode::Char('$'), KeyModifiers::SHIFT));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::MoveToLineEnd { select: true }])
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

        let result = helix.parse_event(make_key_event(KeyCode::Char('P'), KeyModifiers::SHIFT));

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

        let result1 = helix.parse_event(make_key_event(KeyCode::Char('F'), KeyModifiers::SHIFT));
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

        let result1 = helix.parse_event(make_key_event(KeyCode::Char('T'), KeyModifiers::SHIFT));
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
        let mut helix = Helix::default();
        helix.mode = HelixMode::Select;

        let result = helix.parse_event(make_key_event(KeyCode::Char('v'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix.mode, HelixMode::Normal);
    }

    #[test]
    fn esc_exits_select_mode_test() {
        let mut helix = Helix::default();
        helix.mode = HelixMode::Select;

        let result = helix.parse_event(make_key_event(KeyCode::Esc, KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }]),
                ReedlineEvent::Repaint,
            ])
        );
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
}
