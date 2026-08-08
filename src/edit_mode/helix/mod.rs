mod helix_keybindings;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
pub use helix_keybindings::{default_helix_insert_keybindings, default_helix_normal_keybindings};

use crate::{
    Direction, EditCommand, EditMode, FindStop, Granularity, Keybindings, MotionTarget,
    PromptEditMode, PromptHelixMode, ReedlineEvent, WordEdge,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelixMode {
    Normal,
    Insert,
    Select,
}
/// A prefix key waiting for its argument.
///
/// `Find` and `Replace` take an arbitrary char as data, so no finite key
/// sequence can spell them; `Goto` takes one key from a fixed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    /// `f`/`F`/`t`/`T` are waiting for the character to find.
    Find {
        direction: Direction,
        stop: FindStop,
    },
    /// `r` is waiting for the replacement character.
    Replace,
    /// `g` is waiting for the goto target (`h`/`l`/`g`/`e`).
    Goto,
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
    Paste(Direction),
    /// Open a blank line below (`Forward`, `o`) or above (`Backward`, `O`).
    OpenLine(Direction),
    /// `%`. Whole-buffer, so both edges move and no [`MotionTarget`] applies.
    SelectAll,
    /// `x`. Selection-shaped rather than motion-shaped: it moves both edges,
    /// which no [`MotionTarget`] can express.
    SelectLine,
    /// `j`/`k`. The only verb that does not lower to a [`MotionTarget`]: which
    /// of line movement and history traversal applies is decided by the engine
    /// against the *whole* buffer, above where a motion resolves.
    LineOrHistory(Direction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Cut,
    Change,
    Yank,
    Replace(char),
    /// `~`. Keeps the selection, like `Yank`, so a further op reuses the span.
    Switchcase,
    /// `` ` ``. Keeps the selection, as `Switchcase` does.
    Lowercase,
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

/// Shorthand for the `Outcome::Execute(Action { .. })` arms of the key tables.
fn exec(count: usize, verb: Verb, next_mode: Option<HelixMode>) -> Outcome {
    Outcome::Execute(Action {
        count,
        verb,
        next_mode,
    })
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
    /// Prefix key waiting for its argument (`f`/`r`/`g`).
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
    /// Replace the insert-mode keybinding table, keeping the normal-mode
    /// default.
    ///
    /// Layer onto the defaults rather than starting from
    /// [`Keybindings::empty`]: the table is consulted before the state machine
    /// runs, so an empty one silently drops every bound key.
    ///
    ///     # use reedline::{default_helix_insert_keybindings, Helix};
    ///     let mut bindings = default_helix_insert_keybindings();
    ///     // bindings.add_binding(..);
    ///     let helix = Helix::default().with_insert_keybindings(bindings);
    #[must_use]
    pub fn with_insert_keybindings(mut self, keybindings: Keybindings) -> Self {
        self.insert_keybindings = keybindings;
        self
    }

    /// Replace the normal-mode keybinding table, keeping the insert-mode
    /// default. Shared by normal and select mode, and consulted before the
    /// state machine, so a binding here shadows the built-in key of the same
    /// name.
    #[must_use]
    pub fn with_normal_keybindings(mut self, keybindings: Keybindings) -> Self {
        self.normal_keybindings = keybindings;
        self
    }

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
                // Cap the count: every consumer repeats O(count) work on one
                // keystroke, so an absurd prefix must not freeze the REPL.
                self.count = Some(
                    self.count
                        .unwrap_or(0)
                        .saturating_mul(10)
                        .saturating_add(c.to_digit(10).unwrap_or(0) as usize)
                        .min(u16::MAX as usize),
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
                interpret(self.mode, self.count, key)
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
            KeyCode::Char(ch) if is_text_char(key.modifiers) => {
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
        KeyCode::Char(ch) if is_text_char(key.modifiers) => ch,
        _ => return Outcome::Reject,
    };

    match pending {
        Pending::Find { direction, stop } => exec(
            count,
            Verb::SelectingMotion(MotionTarget::Find {
                ch,
                direction,
                stop,
            }),
            None,
        ),
        Pending::Replace => exec(count, Verb::OnSelection(Op::Replace(ch)), None),
        Pending::Goto => match ch {
            'h' => exec(
                count,
                Verb::CollapsingMotion(MotionTarget::LineEdge(Direction::Backward)),
                None,
            ),
            'l' => exec(
                count,
                Verb::CollapsingMotion(MotionTarget::LineEdge(Direction::Forward)),
                None,
            ),
            'g' => exec(
                count,
                Verb::CollapsingMotion(MotionTarget::BufferEdge(Direction::Backward)),
                None,
            ),
            'e' => exec(
                count,
                Verb::CollapsingMotion(MotionTarget::BufferEdge(Direction::Forward)),
                None,
            ),
            's' => exec(
                count,
                Verb::CollapsingMotion(MotionTarget::LineStartNonBlank),
                None,
            ),
            _ => Outcome::Reject,
        },
    }
}

/// Interpret a state
///
/// `count` stays `Option` so a typed `1` is distinguishable from no count;
/// only the goto prefix cares.
fn interpret(mode: HelixMode, count: Option<usize>, key: KeyEvent) -> Outcome {
    // Reject any non-typeable char. Alt-modified keys reach the keybinding table
    // in `dispatch` instead, which is where `Alt-d` and ``Alt-` `` are bound.
    if let KeyCode::Char(_) = key.code {
        if !is_plain_char(key.modifiers) {
            return Outcome::Reject;
        }
    }
    // Helix reads `3gg` as "go to line 3", which has no `MotionTarget` yet, so
    // a counted `g` falls through to the reject arm rather than acting as `gg`.
    if key.code == KeyCode::Char('g') && count.is_none() {
        return Outcome::Absorb(Pending::Goto);
    }
    let count = count.unwrap_or(1);
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
            'w' => exec(
                count,
                Verb::SelectingMotion(MotionTarget::Word {
                    kind: crate::WordKind::Word,
                    edge: WordEdge::Start,
                    direction: Direction::Forward,
                }),
                None,
            ),
            'b' => exec(
                count,
                Verb::SelectingMotion(MotionTarget::Word {
                    kind: crate::WordKind::Word,
                    edge: WordEdge::Start,
                    direction: Direction::Backward,
                }),
                None,
            ),
            'e' => exec(
                count,
                Verb::SelectingMotion(MotionTarget::Word {
                    kind: crate::WordKind::Word,
                    edge: WordEdge::End,
                    direction: Direction::Forward,
                }),
                None,
            ),
            'W' => exec(
                count,
                Verb::SelectingMotion(MotionTarget::Word {
                    kind: crate::WordKind::LongWord,
                    edge: WordEdge::Start,
                    direction: Direction::Forward,
                }),
                None,
            ),
            'B' => exec(
                count,
                Verb::SelectingMotion(MotionTarget::Word {
                    kind: crate::WordKind::LongWord,
                    edge: WordEdge::Start,
                    direction: Direction::Backward,
                }),
                None,
            ),
            'E' => exec(
                count,
                Verb::SelectingMotion(MotionTarget::Word {
                    kind: crate::WordKind::LongWord,
                    edge: WordEdge::End,
                    direction: Direction::Forward,
                }),
                None,
            ),
            'l' => exec(
                count,
                Verb::CollapsingMotion(MotionTarget::Grapheme(Direction::Forward)),
                None,
            ),
            'h' => exec(
                count,
                Verb::CollapsingMotion(MotionTarget::Grapheme(Direction::Backward)),
                None,
            ),
            'j' => exec(count, Verb::LineOrHistory(Direction::Forward), None),
            'k' => exec(count, Verb::LineOrHistory(Direction::Backward), None),
            'x' => exec(count, Verb::SelectLine, None),
            '%' => exec(count, Verb::SelectAll, None),
            '~' => exec(count, Verb::OnSelection(Op::Switchcase), None),
            '`' => exec(count, Verb::OnSelection(Op::Lowercase), None),
            // Insert at the line's first non-blank, append past its last
            // grapheme. Both collapse first: insert mode rests between
            // graphemes, so the block cursor must not survive the switch.
            'I' => exec(
                count,
                Verb::CollapsingMotion(MotionTarget::LineStartNonBlank),
                Some(HelixMode::Insert),
            ),
            'A' => exec(
                count,
                Verb::CollapsingMotion(MotionTarget::LineEdge(Direction::Forward)),
                Some(HelixMode::Insert),
            ),
            'v' => match mode {
                HelixMode::Normal => exec(count, Verb::ChangeMode, Some(HelixMode::Select)),
                HelixMode::Select => exec(count, Verb::ChangeMode, Some(HelixMode::Normal)),
                _ => Outcome::Reject,
            },
            'i' => exec(
                count,
                Verb::Collapse(Direction::Backward),
                Some(HelixMode::Insert),
            ),
            'a' => exec(
                count,
                Verb::Collapse(Direction::Forward),
                Some(HelixMode::Insert),
            ),
            'd' => exec(count, Verb::OnSelection(Op::Cut), Some(HelixMode::Normal)),
            'c' => exec(
                count,
                Verb::OnSelection(Op::Change),
                Some(HelixMode::Insert),
            ),
            'y' => exec(count, Verb::OnSelection(Op::Yank), Some(HelixMode::Normal)),
            'o' => exec(
                count,
                Verb::OpenLine(Direction::Forward),
                Some(HelixMode::Insert),
            ),
            'O' => exec(
                count,
                Verb::OpenLine(Direction::Backward),
                Some(HelixMode::Insert),
            ),
            'u' => exec(count, Verb::Undo, None),
            'U' => exec(count, Verb::Redo, None),
            'p' => exec(
                count,
                Verb::Paste(Direction::Forward),
                Some(HelixMode::Normal),
            ),
            'P' => exec(
                count,
                Verb::Paste(Direction::Backward),
                Some(HelixMode::Normal),
            ),
            _ => Outcome::Reject,
        },
        KeyCode::Enter => exec(count, Verb::Submit, Some(HelixMode::Insert)),
        // Esc deviates from helix, which keeps the selection in normal mode:
        // the single engine-level `Esc` event both dismisses menus and clears
        // the selection, and with `;` not yet bound it is also the only way to
        // drop a selection.
        KeyCode::Esc => match mode {
            HelixMode::Normal => exec(count, Verb::Deselect, None),
            HelixMode::Select => exec(count, Verb::ChangeMode, Some(HelixMode::Normal)),
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
            // Helix has no linewise register: `d` and `c` cut exactly the
            // selection, whatever it spans. They differ only in `next_mode`,
            // which `interpret` already set.
            Op::Cut | Op::Change => ReedlineEvent::Edit(vec![EditCommand::CutSelection {
                granularity: Granularity::CharWise,
            }]),
            Op::Yank => ReedlineEvent::Edit(vec![EditCommand::CopySelection]),
            Op::Replace(ch) => ReedlineEvent::Edit(vec![EditCommand::ReplaceChar(ch)]),
            Op::Switchcase => ReedlineEvent::Edit(vec![EditCommand::SwitchcaseSelection]),
            Op::Lowercase => ReedlineEvent::Edit(vec![EditCommand::LowercaseSelection]),
        },
        Verb::Collapse(dir) => ReedlineEvent::Edit(vec![EditCommand::CollapseSelection(dir)]),
        // Only the first open seeks; the rest go *above* the blank line it just
        // made. A repeated `InsertNewlineBelow` would find no `\n` past that
        // line and append at the buffer end, and a plain `InsertNewline` would
        // delete the resting selection, which under `BlockOverNewline` always
        // covers a grapheme.
        Verb::OpenLine(direction) => {
            let first = match direction {
                Direction::Forward => EditCommand::InsertNewlineBelow,
                Direction::Backward => EditCommand::InsertNewlineAbove,
            };
            let mut cmds = vec![first];
            cmds.resize(action.count.max(1), EditCommand::InsertNewlineAbove);
            ReedlineEvent::Edit(cmds)
        }
        Verb::Undo => action.repeated(EditCommand::Undo),
        Verb::Redo => action.repeated(EditCommand::Redo),
        Verb::Paste(direction) => ReedlineEvent::Edit(vec![EditCommand::PasteAtSelectionEdge {
            direction,
            count: action.count,
        }]),
        // Each press grows the selection one line, thus a count is just the
        // command repeated: it re-reads the selection every time.
        Verb::SelectAll => ReedlineEvent::Edit(vec![EditCommand::SelectAll]),
        Verb::SelectLine => action.repeated(EditCommand::SelectLine),
        Verb::Deselect => ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint]),
        Verb::ChangeMode => ReedlineEvent::None,
        // `Up`/`Down` already carry the whole rule: move by line while another
        // line is there, walk history at the buffer edge, and prefix-search it
        // when the caret sits at the buffer end. A menu takes the keys first, or
        // `j` would move the caret out from under an open one.
        //
        // Select mode extends by line instead and never reaches history, which
        // would replace the buffer the selection is anchored in. `Multiple`
        // carries the count, since `repeated` only multiplies `EditCommand`s.
        Verb::LineOrHistory(direction) => {
            let event = match mode {
                // `MoveLine*`, not `Extend(MotionTarget::Line)`: the target lands
                // on the line *start*, while `line_down_target` keeps the column,
                // so only this reaches the grapheme normal mode would land on.
                HelixMode::Select => ReedlineEvent::Edit(vec![match direction {
                    Direction::Forward => EditCommand::MoveLineDown { select: true },
                    Direction::Backward => EditCommand::MoveLineUp { select: true },
                }]),
                _ => ReedlineEvent::UntilFound(match direction {
                    Direction::Forward => vec![ReedlineEvent::MenuDown, ReedlineEvent::Down],
                    Direction::Backward => vec![ReedlineEvent::MenuUp, ReedlineEvent::Up],
                }),
            };
            ReedlineEvent::Multiple(vec![event; action.count.max(1)])
        }
        // Collapse forward first, as `a` does. The resting selection outlives the
        // `next_mode` flip to insert, so `InsertNewline` on incomplete input
        // opens with `delete_selection` and eats the covered grapheme, and
        // `submit_buffer`'s final repaint leaves the selection highlight in the
        // scrollback. Forward specifically: the break belongs *past* the covered
        // grapheme, where `Deselect` would land before it and vi's `MoveRight`
        // one beyond, since a helix head already sits on the far edge.
        Verb::Submit => {
            return ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::CollapseSelection(Direction::Forward)]),
                ReedlineEvent::Enter,
            ]);
        }
    };

    if action.next_mode.is_some() {
        ReedlineEvent::Multiple(vec![event, ReedlineEvent::Repaint])
    } else {
        event
    }
}

/// A bare or shifted keypress — the only chords that act as normal-mode
/// commands. Anything else (Ctrl/Alt chords) belongs to the keybinding table.
fn is_plain_char(modifiers: KeyModifiers) -> bool {
    modifiers == KeyModifiers::NONE || modifiers == KeyModifiers::SHIFT
}

/// Modifier sets under which a `KeyCode::Char` is *typed text* (data), not a
/// chord: everything [`is_plain_char`] accepts, plus the Ctrl-Alt combinations
/// some terminals report for AltGr.
fn is_text_char(modifiers: KeyModifiers) -> bool {
    is_plain_char(modifiers)
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
    fn x_selects_a_line_once_per_count() {
        // Repeating composes here: each application re-reads the selection and
        // grows it by one line.
        let mut helix = normal();
        assert_eq!(
            helix.parse_event(chr('x')),
            ReedlineEvent::Edit(vec![EditCommand::SelectLine])
        );
        assert_eq!(helix.parse_event(chr('3')), ReedlineEvent::None);
        assert_eq!(
            helix.parse_event(chr('x')),
            ReedlineEvent::Edit(vec![EditCommand::SelectLine; 3])
        );
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

    // ---- goto ----

    #[rstest]
    #[case('h', MotionTarget::LineEdge(Direction::Backward))]
    #[case('l', MotionTarget::LineEdge(Direction::Forward))]
    #[case('g', MotionTarget::BufferEdge(Direction::Backward))]
    #[case('e', MotionTarget::BufferEdge(Direction::Forward))]
    fn goto_moves_in_normal_and_extends_in_select(#[case] c: char, #[case] target: MotionTarget) {
        let mut helix = normal();
        assert_eq!(helix.parse_event(chr('g')), ReedlineEvent::None);
        assert_eq!(
            helix.parse_event(chr(c)),
            ReedlineEvent::Edit(vec![EditCommand::Move(target)])
        );

        let _ = helix.parse_event(chr('v'));
        let _ = helix.parse_event(chr('g'));
        assert_eq!(
            helix.parse_event(chr(c)),
            ReedlineEvent::Edit(vec![EditCommand::Extend(target)])
        );
    }

    #[test]
    fn g_absorbs_without_emitting() {
        let mut helix = normal();
        assert_eq!(helix.parse_event(chr('g')), ReedlineEvent::None);
        assert_eq!(helix.pending, Some(Pending::Goto));
    }

    #[rstest]
    #[case('h', MotionTarget::LineEdge(Direction::Backward))]
    #[case('l', MotionTarget::LineEdge(Direction::Forward))]
    #[case('e', MotionTarget::BufferEdge(Direction::Forward))]
    fn goto_shadows_the_bare_binding_for_the_same_key(
        #[case] c: char,
        #[case] target: MotionTarget,
    ) {
        // `dispatch` checks `pending` before the table and before `interpret`.
        let mut helix = normal();
        let bare = helix.parse_event(chr(c));
        assert_ne!(bare, ReedlineEvent::Edit(vec![EditCommand::Move(target)]));

        let _ = helix.parse_event(chr('g'));
        assert_eq!(
            helix.parse_event(chr(c)),
            ReedlineEvent::Edit(vec![EditCommand::Move(target)])
        );
    }

    #[test]
    fn goto_keeps_select_mode() {
        let mut helix = normal();
        let _ = helix.parse_event(chr('v'));
        let _ = helix.parse_event(chr('g'));
        let _ = helix.parse_event(chr('h'));
        assert_eq!(helix.mode, HelixMode::Select);
    }

    #[test]
    fn unbound_goto_target_rejects_and_clears() {
        let mut helix = normal();
        let _ = helix.parse_event(chr('g'));
        assert_eq!(helix.parse_event(chr('z')), ReedlineEvent::None);
        assert_eq!(helix.pending, None);
        // the machine is usable again, not stuck holding the prefix
        assert_eq!(
            helix.parse_event(chr('w')),
            ReedlineEvent::Edit(vec![EditCommand::Select(w())])
        );
    }

    #[test]
    fn esc_cancels_pending_goto() {
        let mut helix = normal();
        let _ = helix.parse_event(chr('g'));
        let _ = helix.parse_event(key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(helix.pending, None);
        // `h` is a grapheme step again, not a goto target
        assert_eq!(
            helix.parse_event(chr('h')),
            ReedlineEvent::Edit(vec![EditCommand::Move(MotionTarget::Grapheme(
                Direction::Backward
            ))])
        );
    }

    #[test]
    fn a_live_count_rejects_the_goto_prefix() {
        let mut helix = normal();
        let _ = helix.parse_event(chr('3'));
        assert_eq!(helix.parse_event(chr('g')), ReedlineEvent::None);
        assert_eq!(helix.pending, None);
        assert_eq!(helix.count, None);
        assert_eq!(helix.parse_event(chr('g')), ReedlineEvent::None);
        assert_eq!(helix.pending, Some(Pending::Goto));
    }

    #[test]
    fn a_typed_one_is_still_a_count() {
        // `unwrap_or(1)` here would make `1gg` silently become `gg`.
        let mut helix = normal();
        let _ = helix.parse_event(chr('1'));
        assert_eq!(helix.count, Some(1));
        assert_eq!(helix.parse_event(chr('g')), ReedlineEvent::None);
        assert_eq!(helix.pending, None);
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
                interpret(mode, None, kev(KeyCode::Char(c), KeyModifiers::NONE)),
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
                interpret(mode, None, kev(KeyCode::Char(c), KeyModifiers::NONE)),
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
                None,
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
                None,
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
            interpret(
                HelixMode::Normal,
                None,
                kev(KeyCode::Esc, KeyModifiers::NONE)
            ),
            Outcome::Execute(Action {
                count: 1,
                verb: Verb::Deselect,
                next_mode: None,
            })
        );
        assert_eq!(
            interpret(
                HelixMode::Select,
                None,
                kev(KeyCode::Esc, KeyModifiers::NONE)
            ),
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
                ReedlineEvent::Edit(vec![EditCommand::CutSelection {
                    granularity: Granularity::CharWise,
                }]),
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
                ReedlineEvent::Edit(vec![EditCommand::CutSelection {
                    granularity: Granularity::CharWise,
                }]),
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
    fn enter_collapses_then_submits_and_enters_insert() {
        // Enter must still escape the repaint rule: `next_mode` would otherwise
        // append a `Repaint` that the submitting `Enter` never reaches, since
        // the engine returns on the first `Exits`. Asserted on the event rather
        // than driven, since escaping that wrapping is not observable from the
        // editor's state.
        let mut helix = normal();
        assert_eq!(
            helix.parse_event(key(KeyCode::Enter, KeyModifiers::NONE)),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::CollapseSelection(Direction::Forward)]),
                ReedlineEvent::Enter,
            ])
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
        // arrives with SHIFT, which `is_plain_char` accepts.
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

    // ---- paste ----

    #[rstest]
    #[case('p', Direction::Forward)]
    #[case('P', Direction::Backward)]
    fn paste_carries_the_edge_direction(#[case] c: char, #[case] direction: Direction) {
        // `next_mode` is `Some`, so the repaint rule wraps the edit — same shape
        // the operators produce.
        let mut helix = normal();
        assert_eq!(
            helix.parse_event(chr(c)),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::PasteAtSelectionEdge {
                    direction,
                    count: 1
                }]),
                ReedlineEvent::Repaint,
            ])
        );
    }

    #[test]
    fn paste_carries_the_count_in_the_command() {
        // Paste must not go through `Action::repeated`: repeating the event
        // re-anchors at each paste, so the selection would end up covering only
        // the last copy. One command carrying 3, not three commands.
        let mut helix = normal();
        let _ = helix.parse_event(chr('3'));
        assert_eq!(
            helix.parse_event(chr('p')),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::PasteAtSelectionEdge {
                    direction: Direction::Forward,
                    count: 3,
                }]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix.count, None);
    }

    #[rstest]
    #[case('p')]
    #[case('P')]
    fn paste_leaves_select_mode(#[case] c: char) {
        let mut helix = normal();
        let _ = helix.parse_event(chr('v'));
        let _ = helix.parse_event(chr(c));
        assert_eq!(helix.mode, HelixMode::Normal);
    }

    #[rstest]
    #[case('p', KeyModifiers::NONE, Direction::Forward)]
    #[case('P', KeyModifiers::SHIFT, Direction::Backward)]
    fn paste_next_mode_is_mode_independent(
        #[case] c: char,
        #[case] modifiers: KeyModifiers,
        #[case] direction: Direction,
    ) {
        // Like the operators, paste's `next_mode` must not depend on where it
        // started: it returns to normal from select and is inert in normal.
        for mode in [HelixMode::Normal, HelixMode::Select] {
            assert_eq!(
                interpret(mode, None, kev(KeyCode::Char(c), modifiers)),
                Outcome::Execute(Action {
                    count: 1,
                    verb: Verb::Paste(direction),
                    next_mode: Some(HelixMode::Normal),
                })
            );
        }
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

    // ---- open line ----

    #[rstest]
    #[case('o', EditCommand::InsertNewlineBelow)]
    #[case('O', EditCommand::InsertNewlineAbove)]
    fn open_line_enters_insert(#[case] c: char, #[case] expected: EditCommand) {
        let mut helix = normal();
        assert_eq!(
            helix.parse_event(chr(c)),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![expected]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix.mode, HelixMode::Insert);
    }

    #[rstest]
    #[case('o', KeyModifiers::NONE, Direction::Forward)]
    #[case('O', KeyModifiers::SHIFT, Direction::Backward)]
    fn open_line_next_mode_is_mode_independent(
        #[case] c: char,
        #[case] modifiers: KeyModifiers,
        #[case] direction: Direction,
    ) {
        for mode in [HelixMode::Normal, HelixMode::Select] {
            assert_eq!(
                interpret(mode, None, kev(KeyCode::Char(c), modifiers)),
                Outcome::Execute(Action {
                    count: 1,
                    verb: Verb::OpenLine(direction),
                    next_mode: Some(HelixMode::Insert),
                })
            );
        }
    }

    #[test]
    fn count_seeks_once_then_opens_above() {
        let mut helix = normal();
        let _ = helix.parse_event(chr('3'));
        assert_eq!(
            helix.parse_event(chr('o')),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![
                    EditCommand::InsertNewlineBelow,
                    EditCommand::InsertNewlineAbove,
                    EditCommand::InsertNewlineAbove,
                ]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(helix.count, None);
    }
}
