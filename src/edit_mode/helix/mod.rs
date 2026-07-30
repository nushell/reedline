mod helix_keybindings;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
pub use helix_keybindings::{default_helix_insert_keybindings, default_helix_normal_keybindings};

use crate::{
    Direction, EditCommand, EditMode, FindStop, Keybindings, MotionTarget, PromptEditMode,
    PromptHelixMode, ReedlineEvent, WordEdge,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelixMode {
    Normal,
    Insert,
    Select,
}
/// A prefix key waiting for its argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    /// `f`/`F`/`t`/`T` are waiting for the character to find.
    Find {
        direction: Direction,
        stop: FindStop,
    },
    /// `r` is waiting for the replacement character.
    Replace,
}

/// Every parse_event will result in one of three outcomes:
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// Absorb the `ReedlineRawEvent` -> change state -> continue parsing
    Absorb(Pending),
    /// Execute an `Action` matching the completed sequence
    Execute(Action),
    /// Reject a miss-typed sequence
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    SelectingMotion(MotionTarget),
    CollapsingMotion(MotionTarget),
    Collapse(Direction),
    Deselect,
    OnSelection(Op),
    Submit,
    ChangeMode,
    Undo,
    Redo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Cut,
    Change,
    Yank,
    Replace(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Action {
    count: usize,
    verb: Verb,
    next_mode: Option<HelixMode>,
}

impl Action {
    /// Emit `cmd` once per count.
    ///
    /// Valid only when repeating the event composes, i.e. the op re-reads
    /// the cursor each time. Motions and undo qualify; paste won't, since
    /// it writes a cursor derived from what it just inserted.
    fn repeated(self, cmd: EditCommand) -> ReedlineEvent {
        ReedlineEvent::Edit(vec![cmd; self.count])
    }
}

/// This parses incoming input `Event`s like a Helix/Kakoune-style editor: motions are
/// selection first, lowered onto the editor's [`MotionTarget`](crate::MotionTarget) verb vocabulary.
#[derive(Debug, Clone)]
pub struct Helix {
    /// Keybinding lookup table for insert mode
    insert_keybindings: Keybindings,
    /// Keybinding lookup table for normal mode
    normal_keybindings: Keybindings,
    mode: HelixMode,
    /// Count prefix being accumulated (`3w`).
    count: Option<usize>,
    /// Prefix key waiting for its argument (`f`/`r`).
    pending: Option<Pending>,
}

impl EditMode for Helix {
    fn parse_event(&mut self, event: crate::ReedlineRawEvent) -> crate::ReedlineEvent {
        match event.into() {
            Event::Key(key) => match self.mode {
                HelixMode::Insert => self.dispatch_insert(key),
                _ => self.dispatch(key),
            },
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
    fn edit_mode(&self) -> crate::PromptEditMode {
        match self.mode {
            HelixMode::Insert => PromptEditMode::Helix(PromptHelixMode::Insert),
            HelixMode::Normal => PromptEditMode::Helix(PromptHelixMode::Normal),
            HelixMode::Select => PromptEditMode::Helix(PromptHelixMode::Select),
        }
    }
}

impl Helix {
    fn dispatch(&mut self, key: KeyEvent) -> ReedlineEvent {
        // Insert should never use this code-path.
        debug_assert!(self.mode != HelixMode::Insert);
        let outcome = match (self.pending.take(), key.code) {
            // Handle a pending key event
            (Some(pending), _) => complete_pending(pending, self.count.unwrap_or(1), key),
            // Handle a count modifier
            (None, KeyCode::Char(c @ '0'..='9'))
                if key.modifiers == KeyModifiers::NONE && (c != '0' || self.count.is_some()) =>
            {
                self.count = Some(
                    self.count
                        .unwrap_or(0)
                        .saturating_mul(10)
                        .saturating_add(c.to_digit(10).unwrap_or(0) as usize),
                );
                return ReedlineEvent::None;
            }
            // Do a table lookup, else use the helix machine,
            // we don't handle insert mode in dispatch
            (None, code) => {
                if self.count.is_none() {
                    // Esc must always reach the machine, otherwise modes get stranded
                    if code != KeyCode::Esc {
                        if let Some(event) =
                            self.normal_keybindings.find_binding(key.modifiers, code)
                        {
                            return event;
                        }
                    }
                }
                interpret(self.mode, self.count.unwrap_or(1), key)
            }
        };

        match outcome {
            Outcome::Absorb(pending) => {
                self.pending = Some(pending);
                ReedlineEvent::None
            }
            Outcome::Execute(action) => {
                self.count = None;
                let event = lower(action, self.mode);
                if let Some(next_mode) = action.next_mode {
                    self.mode = next_mode;
                }
                event
            }
            Outcome::Reject => {
                self.count = None;
                ReedlineEvent::None
            }
        }
    }
    fn dispatch_insert(&mut self, key: KeyEvent) -> ReedlineEvent {
        // handle esc first since it has to always reach the machine
        if matches!(key.code, KeyCode::Esc) {
            self.mode = HelixMode::Normal;
            return ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint]);
        }
        if let Some(event) = self
            .insert_keybindings
            .find_binding(key.modifiers, key.code)
        {
            return event;
        }
        match key.code {
            KeyCode::Enter if key.modifiers == KeyModifiers::NONE => ReedlineEvent::Enter,
            KeyCode::Char(ch) if is_typed_char(key.modifiers) => {
                ReedlineEvent::Edit(vec![EditCommand::InsertChar(ch)])
            }
            _ => ReedlineEvent::None,
        }
    }
}
impl Default for Helix {
    fn default() -> Self {
        Self {
            insert_keybindings: default_helix_insert_keybindings(),
            normal_keybindings: default_helix_normal_keybindings(),
            mode: HelixMode::Insert,
            count: None,
            pending: None,
        }
    }
}

/// Complete a pending sequence
fn complete_pending(pending: Pending, count: usize, key: KeyEvent) -> Outcome {
    let ch = match key.code {
        KeyCode::Char(ch) if is_typed_char(key.modifiers) => ch,
        _ => return Outcome::Reject,
    };

    match pending {
        Pending::Find { direction, stop } => Outcome::Execute(Action {
            count,
            verb: Verb::SelectingMotion(MotionTarget::Find {
                ch,
                direction,
                stop,
            }),
            next_mode: None,
        }),
        Pending::Replace => Outcome::Execute(Action {
            count,
            verb: Verb::OnSelection(Op::Replace(ch)),
            next_mode: None,
        }),
    }
}

/// Interpret a state
fn interpret(mode: HelixMode, count: usize, key: KeyEvent) -> Outcome {
    // reject any non typeable char, this has to be changed when Alt-d is introduced
    if let KeyCode::Char(_) = key.code {
        if !is_typeable(key.modifiers) {
            return Outcome::Reject;
        }
    }
    match key.code {
        KeyCode::Char(ch) => match ch {
            'f' => Outcome::Absorb(Pending::Find {
                direction: Direction::Forward,
                stop: FindStop::On,
            }),
            'F' => Outcome::Absorb(Pending::Find {
                direction: Direction::Backward,
                stop: FindStop::On,
            }),
            't' => Outcome::Absorb(Pending::Find {
                direction: Direction::Forward,
                stop: FindStop::Before,
            }),
            'T' => Outcome::Absorb(Pending::Find {
                direction: Direction::Backward,
                stop: FindStop::Before,
            }),
            'r' => Outcome::Absorb(Pending::Replace),
            'w' => Outcome::Execute(Action {
                count,
                verb: Verb::SelectingMotion(MotionTarget::Word {
                    kind: crate::WordKind::Word,
                    edge: WordEdge::Start,
                    direction: Direction::Forward,
                }),
                next_mode: None,
            }),
            'b' => Outcome::Execute(Action {
                count,
                verb: Verb::SelectingMotion(MotionTarget::Word {
                    kind: crate::WordKind::Word,
                    edge: WordEdge::Start,
                    direction: Direction::Backward,
                }),
                next_mode: None,
            }),
            'e' => Outcome::Execute(Action {
                count,
                verb: Verb::SelectingMotion(MotionTarget::Word {
                    kind: crate::WordKind::Word,
                    edge: WordEdge::End,
                    direction: Direction::Forward,
                }),
                next_mode: None,
            }),
            'W' => Outcome::Execute(Action {
                count,
                verb: Verb::SelectingMotion(MotionTarget::Word {
                    kind: crate::WordKind::LongWord,
                    edge: WordEdge::Start,
                    direction: Direction::Forward,
                }),
                next_mode: None,
            }),
            'B' => Outcome::Execute(Action {
                count,
                verb: Verb::SelectingMotion(MotionTarget::Word {
                    kind: crate::WordKind::LongWord,
                    edge: WordEdge::Start,
                    direction: Direction::Backward,
                }),
                next_mode: None,
            }),
            'E' => Outcome::Execute(Action {
                count,
                verb: Verb::SelectingMotion(MotionTarget::Word {
                    kind: crate::WordKind::LongWord,
                    edge: WordEdge::End,
                    direction: Direction::Forward,
                }),
                next_mode: None,
            }),
            'l' => Outcome::Execute(Action {
                count,
                verb: Verb::CollapsingMotion(MotionTarget::Grapheme(Direction::Forward)),
                next_mode: None,
            }),
            'h' => Outcome::Execute(Action {
                count,
                verb: Verb::CollapsingMotion(MotionTarget::Grapheme(Direction::Backward)),
                next_mode: None,
            }),
            'v' => match mode {
                HelixMode::Normal => Outcome::Execute(Action {
                    count,
                    verb: Verb::ChangeMode,
                    next_mode: Some(HelixMode::Select),
                }),
                HelixMode::Select => Outcome::Execute(Action {
                    count,
                    verb: Verb::ChangeMode,
                    next_mode: Some(HelixMode::Normal),
                }),
                _ => Outcome::Reject,
            },
            'i' => Outcome::Execute(Action {
                count,
                verb: Verb::Collapse(Direction::Backward),
                next_mode: Some(HelixMode::Insert),
            }),
            'a' => Outcome::Execute(Action {
                count,
                verb: Verb::Collapse(Direction::Forward),
                next_mode: Some(HelixMode::Insert),
            }),
            'd' => Outcome::Execute(Action {
                count,
                verb: Verb::OnSelection(Op::Cut),
                next_mode: Some(HelixMode::Normal),
            }),
            'c' => Outcome::Execute(Action {
                count,
                verb: Verb::OnSelection(Op::Change),
                next_mode: Some(HelixMode::Insert),
            }),
            'y' => Outcome::Execute(Action {
                count,
                verb: Verb::OnSelection(Op::Yank),
                next_mode: Some(HelixMode::Normal),
            }),
            'u' => Outcome::Execute(Action {
                count,
                verb: Verb::Undo,
                next_mode: None,
            }),
            'U' => Outcome::Execute(Action {
                count,
                verb: Verb::Redo,
                next_mode: None,
            }),
            _ => Outcome::Reject,
        },
        KeyCode::Enter => Outcome::Execute(Action {
            count,
            verb: Verb::Submit,
            next_mode: Some(HelixMode::Insert),
        }),
        KeyCode::Esc => match mode {
            HelixMode::Normal => Outcome::Execute(Action {
                count,
                verb: Verb::Deselect,
                next_mode: None,
            }),
            HelixMode::Select => Outcome::Execute(Action {
                count,
                verb: Verb::ChangeMode,
                next_mode: Some(HelixMode::Normal),
            }),
            HelixMode::Insert => Outcome::Reject,
        },
        _ => Outcome::Reject,
    }
}

/// Lowers an `Action` onto `ReedlineEvent`
fn lower(action: Action, mode: HelixMode) -> ReedlineEvent {
    let event = match action.verb {
        Verb::SelectingMotion(target) => match mode {
            HelixMode::Normal => action.repeated(EditCommand::Select(target)),
            HelixMode::Select => action.repeated(EditCommand::Extend(target)),
            HelixMode::Insert => {
                // unreachable at runtime: dispatch guards against insert mode
                ReedlineEvent::None
            }
        },
        Verb::CollapsingMotion(target) => match mode {
            HelixMode::Normal => action.repeated(EditCommand::Move(target)),
            HelixMode::Select => action.repeated(EditCommand::Extend(target)),
            HelixMode::Insert => {
                // unreachable at runtime: dispatch guards against insert mode
                ReedlineEvent::None
            }
        },
        Verb::OnSelection(op) => match op {
            Op::Cut => ReedlineEvent::Edit(vec![EditCommand::CutSelection]),
            Op::Change => ReedlineEvent::Edit(vec![EditCommand::CutSelection]),
            Op::Yank => ReedlineEvent::Edit(vec![EditCommand::CopySelection]),
            Op::Replace(ch) => ReedlineEvent::Edit(vec![EditCommand::ReplaceChar(ch)]),
        },
        Verb::Collapse(dir) => ReedlineEvent::Edit(vec![EditCommand::CollapseSelection(dir)]),
        Verb::Undo => action.repeated(EditCommand::Undo),
        Verb::Redo => action.repeated(EditCommand::Redo),
        Verb::Deselect => ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint]),
        Verb::ChangeMode => ReedlineEvent::None,
        Verb::Submit => {
            return ReedlineEvent::Enter;
        }
    };

    if action.next_mode.is_some() {
        ReedlineEvent::Multiple(vec![event, ReedlineEvent::Repaint])
    } else {
        event
    }
}

fn is_typeable(modifiers: KeyModifiers) -> bool {
    modifiers == KeyModifiers::NONE || modifiers == KeyModifiers::SHIFT
}

/// Modifier sets under which a `KeyCode::Char` is *typed text* (data), not a chord
fn is_typed_char(modifiers: KeyModifiers) -> bool {
    modifiers == KeyModifiers::NONE
        || modifiers == KeyModifiers::SHIFT
        || modifiers == KeyModifiers::CONTROL | KeyModifiers::ALT
        || modifiers == KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{ReedlineRawEvent, WordKind};
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> ReedlineRawEvent {
        ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(code, modifiers))).unwrap()
    }

    fn chr(c: char) -> ReedlineRawEvent {
        let modifiers = if c.is_ascii_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        key(KeyCode::Char(c), modifiers)
    }

    fn kev(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn normal() -> Helix {
        Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        }
    }

    fn word(kind: WordKind, edge: WordEdge, direction: Direction) -> MotionTarget {
        MotionTarget::Word {
            kind,
            edge,
            direction,
        }
    }

    fn w() -> MotionTarget {
        word(WordKind::Word, WordEdge::Start, Direction::Forward)
    }

    // ---- insert path ----

    #[test]
    fn defaults_to_insert_and_inserts_chars() {
        let mut helix = Helix::default();
        assert_eq!(
            helix.edit_mode(),
            PromptEditMode::Helix(PromptHelixMode::Insert)
        );
        assert_eq!(
            helix.parse_event(chr('a')),
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('a')])
        );
    }

    #[test]
    fn insert_accepts_altgr_chars() {
        let mut helix = Helix::default();
        assert_eq!(
            helix.parse_event(key(
                KeyCode::Char('µ'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            )),
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('µ')])
        );
    }

    #[test]
    fn insert_esc_enters_normal() {
        let mut helix = Helix::default();
        assert_eq!(
            helix.parse_event(key(KeyCode::Esc, KeyModifiers::NONE)),
            ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint])
        );
        assert_eq!(helix.mode, HelixMode::Normal);
    }

    #[test]
    fn insert_enter_submits_only_without_modifiers() {
        let mut helix = Helix::default();
        assert_eq!(
            helix.parse_event(key(KeyCode::Enter, KeyModifiers::NONE)),
            ReedlineEvent::Enter
        );
        // unbound Enter chords must not submit
        assert_eq!(
            helix.parse_event(key(KeyCode::Enter, KeyModifiers::CONTROL)),
            ReedlineEvent::None
        );
    }

    // ---- motions ----

    #[rstest]
    #[case('w', WordKind::Word, WordEdge::Start, Direction::Forward)]
    #[case('W', WordKind::LongWord, WordEdge::Start, Direction::Forward)]
    #[case('e', WordKind::Word, WordEdge::End, Direction::Forward)]
    #[case('E', WordKind::LongWord, WordEdge::End, Direction::Forward)]
    #[case('b', WordKind::Word, WordEdge::Start, Direction::Backward)]
    #[case('B', WordKind::LongWord, WordEdge::Start, Direction::Backward)]
    fn word_motions_select_in_normal_mode(
        #[case] c: char,
        #[case] kind: WordKind,
        #[case] edge: WordEdge,
        #[case] direction: Direction,
    ) {
        let mut helix = normal();
        assert_eq!(
            helix.parse_event(chr(c)),
            ReedlineEvent::Edit(vec![EditCommand::Select(word(kind, edge, direction))])
        );
    }

    #[test]
    fn word_motion_extends_in_select_mode() {
        let mut helix = normal();
        let _ = helix.parse_event(chr('v'));
        assert_eq!(
            helix.parse_event(chr('w')),
            ReedlineEvent::Edit(vec![EditCommand::Extend(w())])
        );
    }

    #[test]
    fn h_and_l_collapse_in_normal_extend_in_select() {
        let mut helix = normal();
        assert_eq!(
            helix.parse_event(chr('l')),
            ReedlineEvent::Edit(vec![EditCommand::Move(MotionTarget::Grapheme(
                Direction::Forward
            ))])
        );
        let _ = helix.parse_event(chr('v'));
        assert_eq!(
            helix.parse_event(chr('h')),
            ReedlineEvent::Edit(vec![EditCommand::Extend(MotionTarget::Grapheme(
                Direction::Backward
            ))])
        );
    }

    // ---- counts ----

    #[test]
    fn count_repeats_motion() {
        let mut helix = normal();
        assert_eq!(helix.parse_event(chr('3')), ReedlineEvent::None);
        assert_eq!(
            helix.parse_event(chr('w')),
            ReedlineEvent::Edit(vec![EditCommand::Select(w()); 3])
        );
        assert_eq!(helix.count, None);
    }

    #[test]
    fn count_accumulates_digits() {
        let mut helix = normal();
        let _ = helix.parse_event(chr('1'));
        let _ = helix.parse_event(chr('2'));
        assert_eq!(
            helix.parse_event(chr('w')),
            ReedlineEvent::Edit(vec![EditCommand::Select(w()); 12])
        );
    }

    #[test]
    fn leading_zero_is_not_a_count() {
        let mut helix = normal();
        assert_eq!(helix.parse_event(chr('0')), ReedlineEvent::None);
        assert_eq!(
            helix.parse_event(chr('w')),
            ReedlineEvent::Edit(vec![EditCommand::Select(w())])
        );
    }

    #[test]
    fn live_count_suppresses_table_bindings() {
        // rule from #693: live sequence state wins over the lookup table
        let mut helix = normal();
        let _ = helix.parse_event(chr('3'));
        assert_eq!(
            helix.parse_event(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            ReedlineEvent::None
        );
        // the rejected chord killed the count
        assert_eq!(
            helix.parse_event(chr('w')),
            ReedlineEvent::Edit(vec![EditCommand::Select(w())])
        );
    }

    #[test]
    fn ctrl_c_uses_common_control_binding() {
        let mut helix = normal();
        assert_eq!(
            helix.parse_event(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            ReedlineEvent::CtrlC
        );
    }

    // ---- pending sequences ----

    #[test]
    fn find_waits_for_char_then_selects() {
        let mut helix = normal();
        assert_eq!(helix.parse_event(chr('f')), ReedlineEvent::None);
        assert_eq!(
            helix.parse_event(chr('x')),
            ReedlineEvent::Edit(vec![EditCommand::Select(MotionTarget::Find {
                ch: 'x',
                direction: Direction::Forward,
                stop: FindStop::On,
            })])
        );
    }

    #[test]
    fn till_backward_uses_stop_before() {
        let mut helix = normal();
        let _ = helix.parse_event(chr('T'));
        assert_eq!(
            helix.parse_event(chr('a')),
            ReedlineEvent::Edit(vec![EditCommand::Select(MotionTarget::Find {
                ch: 'a',
                direction: Direction::Backward,
                stop: FindStop::Before,
            })])
        );
    }

    #[test]
    fn count_survives_into_pending() {
        let mut helix = normal();
        let _ = helix.parse_event(chr('2'));
        let _ = helix.parse_event(chr('f'));
        let target = MotionTarget::Find {
            ch: 'x',
            direction: Direction::Forward,
            stop: FindStop::On,
        };
        assert_eq!(
            helix.parse_event(chr('x')),
            ReedlineEvent::Edit(vec![EditCommand::Select(target); 2])
        );
    }

    #[test]
    fn find_accepts_altgr_argument() {
        let mut helix = normal();
        let _ = helix.parse_event(chr('f'));
        assert_eq!(
            helix.parse_event(key(
                KeyCode::Char('@'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            )),
            ReedlineEvent::Edit(vec![EditCommand::Select(MotionTarget::Find {
                ch: '@',
                direction: Direction::Forward,
                stop: FindStop::On,
            })])
        );
    }

    #[test]
    fn altgr_char_is_not_a_command() {
        let mut helix = normal();
        assert_eq!(
            helix.parse_event(key(
                KeyCode::Char('w'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            )),
            ReedlineEvent::None
        );
        assert_eq!(helix.pending, None);
    }

    #[test]
    fn replace_waits_for_char() {
        let mut helix = normal();
        assert_eq!(helix.parse_event(chr('r')), ReedlineEvent::None);
        assert_eq!(
            helix.parse_event(chr('z')),
            ReedlineEvent::Edit(vec![EditCommand::ReplaceChar('z')])
        );
    }

    #[test]
    fn esc_cancels_pending_sequence() {
        let mut helix = normal();
        let _ = helix.parse_event(chr('2'));
        let _ = helix.parse_event(chr('f'));
        let _ = helix.parse_event(key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(helix.count, None);
        assert_eq!(helix.pending, None);
        // the next key is interpreted fresh, not as a find argument
        assert_eq!(
            helix.parse_event(chr('w')),
            ReedlineEvent::Edit(vec![EditCommand::Select(w())])
        );
    }

    // ---- operators and mode transitions ----

    #[rstest]
    #[case('d', Op::Cut, Some(HelixMode::Normal))]
    #[case('c', Op::Change, Some(HelixMode::Insert))]
    #[case('y', Op::Yank, Some(HelixMode::Normal))]
    fn operator_next_mode_is_mode_independent(
        #[case] c: char,
        #[case] op: Op,
        #[case] next_mode: Option<HelixMode>,
    ) {
        // operators leave select mode; `next_mode` must not depend on where
        // they started (this is where the copy-paste bugs lived)
        for mode in [HelixMode::Normal, HelixMode::Select] {
            assert_eq!(
                interpret(mode, 1, kev(KeyCode::Char(c), KeyModifiers::NONE)),
                Outcome::Execute(Action {
                    count: 1,
                    verb: Verb::OnSelection(op),
                    next_mode,
                })
            );
        }
    }

    #[rstest]
    #[case('i', Direction::Backward)]
    #[case('a', Direction::Forward)]
    fn insert_entries_collapse_to_an_edge(#[case] c: char, #[case] direction: Direction) {
        for mode in [HelixMode::Normal, HelixMode::Select] {
            assert_eq!(
                interpret(mode, 1, kev(KeyCode::Char(c), KeyModifiers::NONE)),
                Outcome::Execute(Action {
                    count: 1,
                    verb: Verb::Collapse(direction),
                    next_mode: Some(HelixMode::Insert),
                })
            );
        }
    }

    #[test]
    fn v_toggles_between_normal_and_select() {
        assert_eq!(
            interpret(
                HelixMode::Normal,
                1,
                kev(KeyCode::Char('v'), KeyModifiers::NONE)
            ),
            Outcome::Execute(Action {
                count: 1,
                verb: Verb::ChangeMode,
                next_mode: Some(HelixMode::Select),
            })
        );
        assert_eq!(
            interpret(
                HelixMode::Select,
                1,
                kev(KeyCode::Char('v'), KeyModifiers::NONE)
            ),
            Outcome::Execute(Action {
                count: 1,
                verb: Verb::ChangeMode,
                next_mode: Some(HelixMode::Normal),
            })
        );
    }

    #[test]
    fn esc_deselects_in_normal_and_leaves_select() {
        assert_eq!(
            interpret(HelixMode::Normal, 1, kev(KeyCode::Esc, KeyModifiers::NONE)),
            Outcome::Execute(Action {
                count: 1,
                verb: Verb::Deselect,
                next_mode: None,
            })
        );
        assert_eq!(
            interpret(HelixMode::Select, 1, kev(KeyCode::Esc, KeyModifiers::NONE)),
            Outcome::Execute(Action {
                count: 1,
                verb: Verb::ChangeMode,
                next_mode: Some(HelixMode::Normal),
            })
        );
    }

    #[test]
    fn operators_leave_select_mode() {
        let mut helix = normal();
        let _ = helix.parse_event(chr('v'));
        assert_eq!(
            helix.parse_event(chr('d')),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::CutSelection]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix.mode, HelixMode::Normal);
    }

    #[test]
    fn c_cuts_and_enters_insert() {
        let mut helix = normal();
        assert_eq!(
            helix.parse_event(chr('c')),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::CutSelection]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix.mode, HelixMode::Insert);
    }

    #[test]
    fn y_copies_and_stays_normal() {
        let mut helix = normal();
        assert_eq!(
            helix.parse_event(chr('y')),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::CopySelection]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix.mode, HelixMode::Normal);
    }

    #[test]
    fn esc_in_normal_deselects() {
        let mut helix = normal();
        assert_eq!(
            helix.parse_event(key(KeyCode::Esc, KeyModifiers::NONE)),
            ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint])
        );
        assert_eq!(helix.mode, HelixMode::Normal);
    }

    #[test]
    fn esc_returns_from_select_despite_table_binding() {
        // Esc is exempt from the table lookup; the generic Esc binding must
        // not strand select mode
        let mut helix = normal();
        let _ = helix.parse_event(chr('v'));
        let _ = helix.parse_event(key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(helix.mode, HelixMode::Normal);
    }

    #[test]
    fn enter_submits_bare_and_enters_insert() {
        // Enter must escape the repaint rule: the engine matches on a bare
        // `Enter` event to accept the line
        let mut helix = normal();
        assert_eq!(
            helix.parse_event(key(KeyCode::Enter, KeyModifiers::NONE)),
            ReedlineEvent::Enter
        );
        assert_eq!(helix.mode, HelixMode::Insert);
    }

    // ---- undo / redo ----

    #[rstest]
    #[case('u', EditCommand::Undo)]
    #[case('U', EditCommand::Redo)]
    fn undo_redo_lower_to_bare_edits(#[case] c: char, #[case] expected: EditCommand) {
        // `next_mode` is None, so these must escape the repaint wrap in `lower`:
        // no mode indicator changed, and an `Edit` repaints on its own. `U`
        // arrives with SHIFT, which `is_typeable` accepts.
        let mut helix = normal();
        assert_eq!(
            helix.parse_event(chr(c)),
            ReedlineEvent::Edit(vec![expected])
        );
    }

    #[test]
    fn count_repeats_undo() {
        // This is the reason undo lives in the machine rather than the binding
        // table: `dispatch` only consults the table while no count is live, so a
        // table-bound `u` would give a working `u` and a silently dead `3u`.
        let mut helix = normal();
        let _ = helix.parse_event(chr('3'));
        assert_eq!(
            helix.parse_event(chr('u')),
            ReedlineEvent::Edit(vec![EditCommand::Undo; 3])
        );
        assert_eq!(helix.count, None);
    }

    #[rstest]
    #[case('u', EditCommand::Undo)]
    #[case('U', EditCommand::Redo)]
    fn undo_redo_keep_select_mode(#[case] c: char, #[case] expected: EditCommand) {
        // Select mode is sticky: only operators and Esc leave it, and undo is
        // neither. Asserted on the machine rather than on `interpret`'s
        // `next_mode`, since "the mode survives" is the actual claim.
        let mut helix = normal();
        let _ = helix.parse_event(chr('v'));
        assert_eq!(
            helix.parse_event(chr(c)),
            ReedlineEvent::Edit(vec![expected])
        );
        assert_eq!(helix.mode, HelixMode::Select);
    }

    #[test]
    fn paste_event_produces_insert_string() {
        let mut helix = Helix::default();
        let paste = ReedlineRawEvent::try_from(Event::Paste("hello".to_string())).unwrap();
        assert_eq!(
            helix.parse_event(paste),
            ReedlineEvent::Edit(vec![EditCommand::InsertString("hello".to_string())])
        );
    }
}
