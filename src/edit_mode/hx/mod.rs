pub(crate) mod word;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::{
    edit_mode::EditMode,
    enums::{Movement, ReedlineEvent, ReedlineRawEvent, WordMotionTarget},
    EditCommand, PromptEditMode, PromptHelixMode,
};

/// Helix-style editor modes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HelixMode {
    /// Insert mode -- typing inserts text.
    Insert,
    /// Normal (command) mode -- keys are motions/actions.
    #[default]
    Normal,
    /// Visual selection mode -- motions extend the selection.
    Select,
}

/// Pending state for multi-key sequences (g_, f/t/F/T + char).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Pending {
    #[default]
    None,
    /// Waiting for second key after 'g' (gg, gh, gl).
    Goto,
    /// Waiting for target char after 'f'.
    FindForward,
    /// Waiting for target char after 't'.
    TilForward,
    /// Waiting for target char after 'F'.
    FindBackward,
    /// Waiting for target char after 'T'.
    TilBackward,
    /// Waiting for target char after 'r' (replace).
    Replace,
}

/// How the Helix selection should be adjusted as the user types in Insert mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum InsertStyle {
    /// No selection tracking (entered via `I`, `A`, `c`, or other commands
    /// that clear the selection before entering Insert mode).
    #[default]
    Plain,
    /// Entered via `i`: text is inserted *before* the selection, so both
    /// anchor and head must be shifted forward by the inserted byte length.
    Before,
    /// Entered via `a`: text is inserted *after* the selection, so the
    /// selection head extends to track the cursor.
    After,
}

/// Helix-inspired edit mode for reedline.
///
/// Supports three modes (Insert / Normal / Select) with word-granularity
/// motions and Helix-style selection semantics.
#[derive(Default)]
pub struct Helix {
    mode: HelixMode,
    pending: Pending,
    /// Accumulated numeric prefix (0 = none entered).
    count: usize,
    /// How each `InsertChar` should adjust the Helix selection.
    /// Set when entering Insert mode, reset to `Plain` on Esc.
    insert_style: InsertStyle,
}

impl Helix {
    /// Wrap motion commands for Select mode: execute motion then apply
    /// Helix `put_cursor` semantics to extend the selection.
    ///
    /// The sequence is:
    /// 1. ...cmds — the motion (MoveRight, MoveLeft, etc.) starting
    ///    from the current `cursor()` display position
    /// 2. HxSyncCursor — implements `put_cursor(extend=true)`:
    ///    adjusts anchor on direction flip, sets head with 1-width
    ///    block-cursor semantics, then sets insertion point for display
    fn hx_extend(mut cmds: Vec<EditCommand>) -> ReedlineEvent {
        cmds.push(EditCommand::HxSyncCursor);
        ReedlineEvent::Edit(cmds)
    }

    /// Dispatch an extending motion (f/t/F/T) based on the current mode.
    ///
    /// Matches Helix's `Range::point(cursor).put_cursor(pos, true)` for
    /// Normal mode: the anchor restarts at the old cursor then extends to
    /// the target, with direction-flip adjustment when going backward.
    /// Select mode simply extends the existing selection.
    fn extending_motion_event(&self, cmds: Vec<EditCommand>) -> ReedlineEvent {
        if self.mode == HelixMode::Select {
            return Self::hx_extend(cmds);
        }
        let mut v = cmds;
        v.push(EditCommand::HxSyncCursorWithRestart);
        ReedlineEvent::Edit(v)
    }

    /// Dispatch a motion event based on the current mode.
    /// - Normal => motion + collapse at new position (1-wide block cursor)
    /// - Select => motion + extend, anchor stays
    fn motion_event(&self, cmds: Vec<EditCommand>) -> ReedlineEvent {
        if self.mode == HelixMode::Select {
            return Self::hx_extend(cmds);
        }
        let mut v = cmds;
        v.push(EditCommand::HxRestartSelection);
        ReedlineEvent::Edit(v)
    }

    /// Switch to Insert mode and execute `pre_cmds` before returning Repaint.
    fn enter_insert(&mut self, pre_cmds: Vec<ReedlineEvent>) -> ReedlineEvent {
        self.mode = HelixMode::Insert;
        let mut events = pre_cmds;
        events.push(ReedlineEvent::Repaint);
        ReedlineEvent::Multiple(events)
    }

    /// Consume the accumulated count prefix, returning at least 1.
    /// Capped at 100_000 to prevent OOM from absurdly large counts.
    fn take_count(&mut self) -> usize {
        let c = self.count.clamp(1, 100_000);
        self.count = 0;
        c
    }

    /// Dispatch a word motion event. Word motions handle their own selection
    /// internally (anchor adjustment, block-cursor semantics), so they do NOT
    /// get the HxSyncCursor wrapper that simple cursor motions need.
    ///
    /// The restart for Normal mode is handled inside `hx_word_motion` so
    /// that no-progress motions (e.g. at end-of-buffer) can preserve the
    /// existing selection instead of collapsing it.
    fn word_motion_event(&self, target: WordMotionTarget, count: usize) -> ReedlineEvent {
        let movement = if self.mode == HelixMode::Select {
            Movement::Extend
        } else {
            Movement::Move
        };
        ReedlineEvent::Edit(vec![EditCommand::HxWordMotion {
            target,
            movement,
            count,
        }])
    }

    /// Resolve a pending 'g' prefix: gg, ge, gh, gl.
    fn parse_goto(&mut self, code: KeyCode) -> ReedlineEvent {
        self.pending = Pending::None;
        // Count is intentionally discarded — goto motions are absolute positions.
        self.count = 0;
        match code {
            KeyCode::Char('g') => {
                self.motion_event(vec![EditCommand::MoveToStart { select: false }])
            }
            // ge/gE not yet implemented (needs PrevWordEnd motion target).
            // Fallthrough to None.
            KeyCode::Char('h') => {
                self.motion_event(vec![EditCommand::MoveToLineStart { select: false }])
            }
            KeyCode::Char('l') => {
                self.motion_event(vec![EditCommand::MoveToLineEnd { select: false }])
            }
            _ => ReedlineEvent::None,
        }
    }

    /// Resolve a pending find/til char motion (f/t/F/T + char).
    /// These are extending motions: the selection grows from the current
    /// cursor to the target character.
    ///
    /// Resolve a pending find/til char motion (f/t/F/T + char).
    ///
    /// Normal mode: first iteration restarts anchor at old cursor then
    /// extends to target (HxSyncCursorWithRestart). Subsequent iterations
    /// only extend (HxSyncCursor) so the anchor stays fixed.
    /// Select mode: all iterations extend (HxSyncCursor).
    fn parse_find_char(&mut self, pending: Pending, c: char) -> ReedlineEvent {
        self.pending = Pending::None;
        let count = self.take_count();
        let cmd = match pending {
            Pending::FindForward => EditCommand::MoveRightUntil { c, select: false },
            Pending::TilForward => EditCommand::MoveRightBefore { c, select: false },
            Pending::FindBackward => EditCommand::MoveLeftUntil { c, select: false },
            Pending::TilBackward => EditCommand::MoveLeftBefore { c, select: false },
            _ => unreachable!(),
        };
        if count <= 1 {
            self.extending_motion_event(vec![cmd])
        } else {
            let first = self.extending_motion_event(vec![cmd.clone()]);
            let rest = Self::hx_extend(vec![cmd]);
            let mut events = vec![first];
            events.resize(count, rest);
            ReedlineEvent::Multiple(events)
        }
    }

    /// Resolve a pending 'r' + char (replace every grapheme in selection).
    fn parse_replace_char(&mut self, c: char) -> ReedlineEvent {
        self.pending = Pending::None;
        ReedlineEvent::Edit(vec![
            EditCommand::HxEnsureSelection,
            EditCommand::HxReplaceSelectionWithChar(c),
        ])
    }

    /// Handle key events in Normal or Select mode.
    ///
    /// All motion keys go through self.motion_event() which wraps them with
    /// the appropriate selection restart/extend commands based on the mode.
    /// All inner MoveLeft/MoveRight use `select: false` since selection is
    /// managed through HxRestartSelection/HxSyncCursor.
    fn parse_normal_select(&mut self, code: KeyCode, modifiers: KeyModifiers) -> ReedlineEvent {
        // ── Resolve pending multi-key sequences ─────────────────────────
        match self.pending {
            Pending::Goto => return self.parse_goto(code),
            Pending::FindForward
            | Pending::TilForward
            | Pending::FindBackward
            | Pending::TilBackward => {
                let p = self.pending;
                if let KeyCode::Char(c) = code {
                    return self.parse_find_char(p, c);
                }
                self.pending = Pending::None;
                return ReedlineEvent::None;
            }
            Pending::Replace => {
                if let KeyCode::Char(c) = code {
                    return self.parse_replace_char(c);
                }
                self.pending = Pending::None;
                return ReedlineEvent::None;
            }
            Pending::None => {}
        }

        // ── Count prefix accumulation ─────────────────────────────────
        match code {
            KeyCode::Char(c @ '1'..='9') => {
                self.count = self
                    .count
                    .saturating_mul(10)
                    .saturating_add((c as usize) - ('0' as usize));
                return ReedlineEvent::None;
            }
            KeyCode::Char('0') if self.count > 0 => {
                self.count = self.count.saturating_mul(10);
                return ReedlineEvent::None;
            }
            _ => {}
        }

        match code {
            // ── Pending keys: don't consume count, just set pending state ──
            KeyCode::Char('g') => {
                self.pending = Pending::Goto;
                return ReedlineEvent::None;
            }
            KeyCode::Char('f') => {
                self.pending = Pending::FindForward;
                return ReedlineEvent::None;
            }
            KeyCode::Char('t') => {
                self.pending = Pending::TilForward;
                return ReedlineEvent::None;
            }
            KeyCode::Char('F') => {
                self.pending = Pending::FindBackward;
                return ReedlineEvent::None;
            }
            KeyCode::Char('T') => {
                self.pending = Pending::TilBackward;
                return ReedlineEvent::None;
            }
            KeyCode::Char('r') => {
                self.pending = Pending::Replace;
                return ReedlineEvent::None;
            }
            _ => {}
        }

        // Try counted motion keys first, then fall through to mode switches
        // and selection/editing commands which don't use count.
        if let Some(event) = self.parse_motion(code) {
            return event;
        }

        // Everything below ignores count — reset so it doesn't leak.
        self.count = 0;

        match code {
            // ── Mode switches ─────────────────────────────────────────
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
            KeyCode::Char('v') => match self.mode {
                HelixMode::Normal => {
                    self.mode = HelixMode::Select;
                    ReedlineEvent::Repaint
                }
                HelixMode::Select => {
                    self.mode = HelixMode::Normal;
                    ReedlineEvent::Multiple(vec![
                        ReedlineEvent::Edit(vec![EditCommand::HxRestartSelection]),
                        ReedlineEvent::Repaint,
                    ])
                }
                HelixMode::Insert => ReedlineEvent::None,
            },
            KeyCode::Char(';') => ReedlineEvent::Edit(vec![EditCommand::HxRestartSelection]),
            _ => self.parse_non_counted(code, modifiers),
        }
    }

    /// Handle counted motion keys (h/l/j/k/w/b/e/W/B/E/Home/End).
    ///
    /// Returns `Some(event)` if the key is a motion, `None` otherwise so
    /// the caller can fall through to mode-switch and editing commands.
    fn parse_motion(&mut self, code: KeyCode) -> Option<ReedlineEvent> {
        let event = match code {
            // ── Basic motions (h/l/j/k) ───────────────────────────
            KeyCode::Char('h') => {
                let count = self.take_count();
                if self.mode == HelixMode::Select {
                    let motion = Self::hx_extend(vec![EditCommand::MoveLeft { select: false }]);
                    if count <= 1 {
                        motion
                    } else {
                        ReedlineEvent::Multiple(vec![motion; count])
                    }
                } else {
                    let mut cmds: Vec<EditCommand> =
                        vec![EditCommand::MoveLeft { select: false }; count];
                    cmds.push(EditCommand::HxRestartSelection);
                    ReedlineEvent::Edit(cmds)
                }
            }
            KeyCode::Char('l') => {
                let count = self.take_count();
                let motion = ReedlineEvent::UntilFound(vec![
                    ReedlineEvent::HistoryHintComplete,
                    ReedlineEvent::MenuRight,
                    self.motion_event(vec![EditCommand::MoveRight { select: false }]),
                ]);
                if count <= 1 {
                    motion
                } else {
                    ReedlineEvent::Multiple(vec![motion; count])
                }
            }
            KeyCode::Char('j') => {
                let count = self.take_count();
                let motion =
                    ReedlineEvent::UntilFound(vec![ReedlineEvent::MenuDown, ReedlineEvent::Down]);
                if count <= 1 {
                    motion
                } else {
                    ReedlineEvent::Multiple(vec![motion; count])
                }
            }
            KeyCode::Char('k') => {
                let count = self.take_count();
                let motion =
                    ReedlineEvent::UntilFound(vec![ReedlineEvent::MenuUp, ReedlineEvent::Up]);
                if count <= 1 {
                    motion
                } else {
                    ReedlineEvent::Multiple(vec![motion; count])
                }
            }

            // ── Word motions ──────────────────────────────────────
            KeyCode::Char('w') => {
                let count = self.take_count();
                self.word_motion_event(WordMotionTarget::NextWordStart, count)
            }
            KeyCode::Char('b') => {
                let count = self.take_count();
                self.word_motion_event(WordMotionTarget::PrevWordStart, count)
            }
            KeyCode::Char('e') => {
                let count = self.take_count();
                self.word_motion_event(WordMotionTarget::NextWordEnd, count)
            }
            KeyCode::Char('W') => {
                let count = self.take_count();
                self.word_motion_event(WordMotionTarget::NextLongWordStart, count)
            }
            KeyCode::Char('B') => {
                let count = self.take_count();
                self.word_motion_event(WordMotionTarget::PrevLongWordStart, count)
            }
            KeyCode::Char('E') => {
                let count = self.take_count();
                self.word_motion_event(WordMotionTarget::NextLongWordEnd, count)
            }

            // ── Line motions ──────────────────────────────────────
            // Helix uses gh/gl for line start/end (in the goto menu).
            // Home/End keys are bound directly as convenience.
            KeyCode::Home => {
                self.count = 0;
                self.motion_event(vec![EditCommand::MoveToLineStart { select: false }])
            }
            KeyCode::End => {
                self.count = 0;
                self.motion_event(vec![EditCommand::MoveToLineEnd { select: false }])
            }

            _ => return None,
        };
        Some(event)
    }

    /// Handle commands that operate on the selection (no count prefix).
    fn parse_non_counted(&mut self, code: KeyCode, modifiers: KeyModifiers) -> ReedlineEvent {
        match code {
            KeyCode::Char('%') => ReedlineEvent::Edit(vec![
                EditCommand::MoveToStart { select: false },
                EditCommand::HxRestartSelection,
                EditCommand::MoveToEnd { select: false },
                EditCommand::HxSyncCursor,
            ]),
            KeyCode::Char('x') | KeyCode::Char('X') => {
                // Select entire line. In multi-line Helix, x extends down and
                // X extends up; for a single-line editor both select the whole line.
                ReedlineEvent::Edit(vec![
                    EditCommand::MoveToLineStart { select: false },
                    EditCommand::HxRestartSelection,
                    EditCommand::MoveToLineEnd { select: false },
                    EditCommand::HxSyncCursor,
                ])
            }

            // ── Editing actions ──────────────────────────────────────
            // Alt+d: delete without yanking (Helix default)
            KeyCode::Char('d') if modifiers == KeyModifiers::ALT => ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::HxDeleteSelection,
                EditCommand::HxRestartSelection,
            ]),
            KeyCode::Char('d') => ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::CutSelection,
                EditCommand::HxRestartSelection,
            ]),
            // Alt+c: change without yanking (Helix default)
            KeyCode::Char('c') if modifiers == KeyModifiers::ALT => {
                self.enter_insert(vec![ReedlineEvent::Edit(vec![
                    EditCommand::HxEnsureSelection,
                    EditCommand::HxDeleteSelection,
                    EditCommand::HxClearSelection,
                ])])
            }
            KeyCode::Char('c') => self.enter_insert(vec![ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::CutSelection,
                EditCommand::HxClearSelection,
            ])]),
            KeyCode::Char('y') => ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::CopySelection,
            ]),
            KeyCode::Char('p') => ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::HxMoveToSelectionEnd,
                EditCommand::HxClearSelection,
                EditCommand::PasteCutBufferBefore,
                EditCommand::HxRestartSelection,
            ]),
            KeyCode::Char('P') => ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::HxMoveToSelectionStart,
                EditCommand::HxClearSelection,
                EditCommand::PasteCutBufferBefore,
                EditCommand::HxRestartSelection,
            ]),
            KeyCode::Char('R') => ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::HxDeleteSelection,
                EditCommand::PasteCutBufferBefore,
                EditCommand::HxRestartSelection,
            ]),
            KeyCode::Char('u') => {
                ReedlineEvent::Edit(vec![EditCommand::Undo, EditCommand::HxRestartSelection])
            }
            KeyCode::Char('U') => {
                ReedlineEvent::Edit(vec![EditCommand::Redo, EditCommand::HxRestartSelection])
            }
            KeyCode::Char('~') => ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::HxSwitchCaseSelection,
            ]),
            KeyCode::Char('o') => ReedlineEvent::Edit(vec![EditCommand::HxFlipSelection]),
            KeyCode::Enter => ReedlineEvent::Enter,
            _ => ReedlineEvent::None,
        }
    }
}

impl EditMode for Helix {
    /// Parse a raw crossterm event into a ReedlineEvent.
    ///
    /// Structure:
    /// 1. Extract key event (non-key events => None)
    /// 2. Global bindings: Ctrl+C => CtrlC, Ctrl+D => CtrlD
    /// 3. Insert mode: Esc => Normal + [Esc, Repaint]; Char(c) => InsertChar;
    ///    Enter/Backspace/Delete as expected
    /// 4. Normal/Select mode: Esc => Normal + [Esc, Repaint]; else delegate
    ///    to parse_normal_select
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event.into()
        else {
            return ReedlineEvent::None;
        };

        // ── Global bindings ───────────────────────────────────────────
        // Ctrl+C is always interrupt. Ctrl+D is EOF in Normal/Select but
        // delete-forward in Insert (handled below).
        if modifiers == KeyModifiers::CONTROL && code == KeyCode::Char('c') {
            return ReedlineEvent::CtrlC;
        }

        match self.mode {
            // ── Insert mode ───────────────────────────────────────────
            HelixMode::Insert => match (code, modifiers) {
                (KeyCode::Esc, _) => {
                    self.mode = HelixMode::Normal;
                    self.insert_style = InsertStyle::Plain;
                    ReedlineEvent::Multiple(vec![
                        ReedlineEvent::Esc,
                        // Step back so the block cursor lands ON the last typed character
                        // (Insert cursor is between chars; Normal cursor is on a char).
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
                // Arrow keys / Home / End move the cursor away from the
                // insert position — clear the selection since byte offsets
                // become meaningless after arbitrary cursor movement.
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

            // ── Normal / Select mode ──────────────────────────────────
            HelixMode::Normal | HelixMode::Select => {
                if modifiers == KeyModifiers::CONTROL && code == KeyCode::Char('d') {
                    return ReedlineEvent::CtrlD;
                }
                match code {
                    KeyCode::Esc => {
                        self.mode = HelixMode::Normal;
                        self.pending = Pending::None;
                        self.count = 0;
                        // Collapse selection so stale extended selection doesn't persist.
                        ReedlineEvent::Multiple(vec![
                            ReedlineEvent::Esc,
                            ReedlineEvent::Edit(vec![EditCommand::HxRestartSelection]),
                            ReedlineEvent::Repaint,
                        ])
                    }
                    _ => self.parse_normal_select(code, modifiers),
                }
            }
        }
    }

    /// Return the current prompt edit mode indicator.
    fn edit_mode(&self) -> PromptEditMode {
        match self.mode {
            HelixMode::Insert => PromptEditMode::Helix(PromptHelixMode::Insert),
            HelixMode::Normal => PromptEditMode::Helix(PromptHelixMode::Normal),
            HelixMode::Select => PromptEditMode::Helix(PromptHelixMode::Select),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_editor::{Editor, HxRange};
    use crate::enums::{Movement, UndoBehavior, WordMotionTarget};
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};

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

    fn edit_mode_hx(hx: &Helix) -> PromptHelixMode {
        match hx.edit_mode() {
            PromptEditMode::Helix(m) => m,
            other => panic!("unexpected prompt edit mode: {:?}", other),
        }
    }

    // ── parse_event unit tests ──────────────────────────────────────────

    #[test]
    fn ctrl_c_works_in_all_modes() {
        for initial_mode in [HelixMode::Insert, HelixMode::Normal, HelixMode::Select] {
            let mut hx = Helix {
                mode: initial_mode,
                ..Default::default()
            };
            let event = hx.parse_event(key_press(KeyCode::Char('c'), KeyModifiers::CONTROL));
            assert_eq!(event, ReedlineEvent::CtrlC);
        }
    }

    #[test]
    fn ctrl_d_eof_in_normal_and_select() {
        for initial_mode in [HelixMode::Normal, HelixMode::Select] {
            let mut hx = Helix {
                mode: initial_mode,
                ..Default::default()
            };
            let event = hx.parse_event(key_press(KeyCode::Char('d'), KeyModifiers::CONTROL));
            assert_eq!(event, ReedlineEvent::CtrlD);
        }
    }

    #[test]
    fn ctrl_d_deletes_forward_in_insert() {
        let mut hx = Helix {
            mode: HelixMode::Insert,
            ..Default::default()
        };
        let event = hx.parse_event(key_press(KeyCode::Char('d'), KeyModifiers::CONTROL));
        assert_eq!(event, ReedlineEvent::Edit(vec![EditCommand::Delete]));
    }

    #[test]
    fn esc_from_insert_enters_normal() {
        let mut hx = Helix {
            mode: HelixMode::Insert,
            ..Default::default()
        };
        let event = hx.parse_event(key_press(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(
            event,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Esc,
                ReedlineEvent::Edit(vec![EditCommand::MoveLeft { select: false }]),
                ReedlineEvent::Edit(vec![EditCommand::HxRestartSelection]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(hx.mode, HelixMode::Normal);
    }

    #[test]
    fn i_from_normal_enters_insert() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('i'));
        assert_eq!(
            event,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![
                    EditCommand::HxEnsureSelection,
                    EditCommand::HxMoveToSelectionStart,
                ]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(hx.mode, HelixMode::Insert);
        assert_eq!(hx.insert_style, InsertStyle::Before);
    }

    #[test]
    fn a_from_normal_enters_insert_after_selection() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('a'));
        assert_eq!(
            event,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![
                    EditCommand::HxEnsureSelection,
                    EditCommand::HxMoveToSelectionEnd,
                ]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(hx.mode, HelixMode::Insert);
        assert_eq!(hx.insert_style, InsertStyle::After);
    }

    #[test]
    fn v_toggles_select_mode() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        // Normal -> Select
        let event = hx.parse_event(char_key('v'));
        assert_eq!(event, ReedlineEvent::Repaint);
        assert_eq!(hx.mode, HelixMode::Select);

        // Select -> Normal (with selection restart)
        let event = hx.parse_event(char_key('v'));
        assert!(matches!(event, ReedlineEvent::Multiple(_)));
        assert_eq!(hx.mode, HelixMode::Normal);
    }

    #[test]
    fn insert_char_in_insert_mode() {
        let mut hx = Helix {
            mode: HelixMode::Insert,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('x'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('x')])
        );
    }

    #[test]
    fn insert_char_in_append_mode_extends_selection() {
        let mut hx = Helix {
            mode: HelixMode::Insert,
            insert_style: InsertStyle::After,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('x'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::InsertChar('x'),
                EditCommand::HxExtendSelectionToInsertionPoint,
            ])
        );
    }

    #[test]
    fn insert_char_in_before_mode_shifts_selection() {
        let mut hx = Helix {
            mode: HelixMode::Insert,
            insert_style: InsertStyle::Before,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('x'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::InsertChar('x'),
                EditCommand::HxShiftSelectionToInsertionPoint,
            ])
        );
    }

    #[test]
    fn h_in_normal_produces_motion_with_collapse() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('h'));
        // Normal mode: move then collapse (no visible selection).
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
                EditCommand::HxRestartSelection,
            ])
        );
    }

    #[test]
    fn h_in_select_extends_without_restart() {
        let mut hx = Helix {
            mode: HelixMode::Select,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('h'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
                EditCommand::HxSyncCursor,
            ])
        );
    }

    #[test]
    fn w_in_normal_produces_word_motion() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('w'));
        // Word motions handle their own selection (restart is internal to
        // hx_word_motion so no-progress motions can preserve the selection).
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![EditCommand::HxWordMotion {
                target: WordMotionTarget::NextWordStart,
                movement: Movement::Move,
                count: 1,
            }])
        );
    }

    #[test]
    fn enter_in_insert_mode() {
        let mut hx = Helix {
            mode: HelixMode::Insert,
            ..Default::default()
        };
        let event = hx.parse_event(key_press(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(event, ReedlineEvent::Enter);
    }

    #[test]
    fn backspace_plain_insert_is_bare() {
        let mut hx = Helix {
            mode: HelixMode::Insert,
            ..Default::default()
        };
        let event = hx.parse_event(key_press(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(event, ReedlineEvent::Edit(vec![EditCommand::Backspace]));
        assert_eq!(hx.insert_style, InsertStyle::Plain);
    }

    #[test]
    fn backspace_in_a_mode_extends_selection() {
        let mut hx = Helix {
            mode: HelixMode::Insert,
            insert_style: InsertStyle::After,
            ..Default::default()
        };
        let event = hx.parse_event(key_press(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::Backspace,
                EditCommand::HxExtendSelectionToInsertionPoint,
            ])
        );
        // insert_style stays After — selection is still tracked.
        assert_eq!(hx.insert_style, InsertStyle::After);
    }

    #[test]
    fn backspace_in_i_mode_shifts_selection() {
        let mut hx = Helix {
            mode: HelixMode::Insert,
            insert_style: InsertStyle::Before,
            ..Default::default()
        };
        let event = hx.parse_event(key_press(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::Backspace,
                EditCommand::HxShiftSelectionToInsertionPoint,
            ])
        );
        assert_eq!(hx.insert_style, InsertStyle::Before);
    }

    #[test]
    fn delete_in_insert_preserves_selection() {
        let mut hx = Helix {
            mode: HelixMode::Insert,
            insert_style: InsertStyle::Before,
            ..Default::default()
        };
        let event = hx.parse_event(key_press(KeyCode::Delete, KeyModifiers::NONE));
        assert_eq!(event, ReedlineEvent::Edit(vec![EditCommand::Delete]));
        assert_eq!(hx.insert_style, InsertStyle::Before);
    }

    #[test]
    fn arrow_left_in_insert_clears_selection() {
        let mut hx = Helix {
            mode: HelixMode::Insert,
            insert_style: InsertStyle::After,
            ..Default::default()
        };
        let event = hx.parse_event(key_press(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
                EditCommand::HxClearSelection,
            ])
        );
        assert_eq!(hx.insert_style, InsertStyle::Plain);
    }

    #[test]
    fn count_not_consumed_by_editing_commands() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        // Press '3' then 'd' — count should be discarded, not affect deletion.
        hx.parse_event(char_key('3'));
        assert_eq!(hx.count, 3);
        let event = hx.parse_event(char_key('d'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::CutSelection,
                EditCommand::HxRestartSelection,
            ])
        );
        // Count was reset.
        assert_eq!(hx.count, 0);
    }

    #[test]
    fn count_applies_to_j_k() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        hx.parse_event(char_key('3'));
        let event = hx.parse_event(char_key('j'));
        assert!(matches!(event, ReedlineEvent::Multiple(ref v) if v.len() == 3));
    }

    #[test]
    fn edit_mode_returns_correct_prompt() {
        let mut hx = Helix::default();
        assert!(matches!(edit_mode_hx(&hx), PromptHelixMode::Normal));

        hx.mode = HelixMode::Insert;
        assert!(matches!(edit_mode_hx(&hx), PromptHelixMode::Insert));

        hx.mode = HelixMode::Select;
        assert!(matches!(edit_mode_hx(&hx), PromptHelixMode::Select));
    }

    // ── Word motion integration tests ───────────────────────────────────
    //
    // Selection notation (gap / right-exclusive convention):
    //   [text]   -- forward selection: `[` = anchor, `]` = head
    //   ]text[   -- backward selection: `]` = head, `[` = anchor
    //   [h]ello  -- 1-wide selection (anchor=0, head=1)
    //
    // Helix uses CharClass-based word boundaries (Word/Punctuation/Whitespace),
    // so "foo.bar" is three words: "foo", ".", "bar".
    // WORD motions (W/B/E) split only on whitespace.

    /// Parse selection notation into (buffer, anchor, head).
    ///
    /// `[` marks the **anchor** position and `]` marks the **head** position
    /// (byte offsets into the returned buffer). The order they appear in the
    /// string determines selection direction:
    ///
    /// - Forward  `[hello ]world`  → anchor=0, head=6
    /// - Backward `]hello[ world`  → anchor=5, head=0
    /// - 1-wide   `[h]ello world`  → anchor=0, head=1
    ///
    /// Handles multi-byte UTF-8 content correctly.
    fn parse_selection(s: &str) -> (String, usize, usize) {
        let mut buf = String::new();
        let mut anchor_pos: Option<usize> = None;
        let mut head_pos: Option<usize> = None;

        for ch in s.chars() {
            match ch {
                '[' => {
                    assert!(anchor_pos.is_none(), "duplicate `[` in notation");
                    anchor_pos = Some(buf.len());
                }
                ']' => {
                    assert!(head_pos.is_none(), "duplicate `]` in notation");
                    head_pos = Some(buf.len());
                }
                _ => buf.push(ch),
            }
        }

        let anchor = anchor_pos.expect("missing `[` in notation");
        let head = head_pos.expect("missing `]` in notation");
        (buf, anchor, head)
    }

    fn editor_with(buffer: &str, cursor: usize) -> Editor {
        let mut editor = Editor::default();
        editor.set_buffer(buffer.to_string(), UndoBehavior::CreateUndoPoint);
        editor.run_edit_command(&EditCommand::MoveToPosition {
            position: cursor,
            select: false,
        });
        editor
    }

    fn run_commands(editor: &mut Editor, commands: &[EditCommand]) {
        for cmd in commands {
            editor.run_edit_command(cmd);
        }
    }

    /// Run a normal-mode motion from `input` notation and assert `expected` notation.
    fn assert_sel(commands: &[EditCommand], input: &str, expected: &str) {
        let (buf, _in_anchor, in_head) = parse_selection(input);
        let (exp_buf, exp_anchor, exp_head) = parse_selection(expected);
        assert_eq!(buf, exp_buf, "input/expected buffer mismatch");

        // Set up editor with cursor at the visual cursor position (inclusive).
        let in_cursor = HxRange {
            anchor: _in_anchor,
            head: in_head,
        }
        .cursor(&buf);

        // Word motions handle their own selection internally (restart
        // for Move mode is inside hx_word_motion).
        let mut editor = editor_with(&buf, in_cursor);
        run_commands(&mut editor, commands);

        let sel = editor
            .hx_selection()
            .expect("expected hx_selection to be set");

        assert_eq!(
            (sel.anchor, sel.head),
            (exp_anchor, exp_head),
            "\n  input:    {}\n  expected: {}\n  got: anchor={} head={}",
            input,
            expected,
            sel.anchor,
            sel.head,
        );
    }

    // ── w (word right start) ────────────────────────────────────────────
    // Test cases from schlich/helix-mode prototype.

    #[test]
    fn test_w() {
        let w = [EditCommand::HxWordMotion {
            target: WordMotionTarget::NextWordStart,
            movement: Movement::Move,
            count: 1,
        }];

        assert_sel(&w, "[h]ello world", "[hello ]world");
        assert_sel(&w, "h[e]llo world", "h[ello ]world");
        assert_sel(&w, "[h]ello   world", "[hello   ]world");
        assert_sel(&w, "[h]ello   .world", "[hello   ].world");
        assert_sel(&w, "[h]ello.world", "[hello].world");
        assert_sel(&w, "hello[.]world", "hello.[world]");
        assert_sel(&w, "[h]ello_world test", "[hello_world ]test");
        assert_sel(&w, "test [.].. next", "test [... ]next");
    }

    // ── b (word left) ───────────────────────────────────────────────────
    // Test cases from schlich/helix-mode prototype.

    #[test]
    fn test_b() {
        let b = [EditCommand::HxWordMotion {
            target: WordMotionTarget::PrevWordStart,
            movement: Movement::Move,
            count: 1,
        }];

        assert_sel(&b, "hello worl[d]", "hello ]world[");
        assert_sel(&b, "hello [w]orld", "]hello [world");
        assert_sel(&b, "hello[.]world", "]hello[.world");
        assert_sel(&b, "hello_worl[d]", "]hello_world[");
        assert_sel(&b, "test ...[n]ext", "test ]...[next");
    }

    // ── e (word right end) ──────────────────────────────────────────────
    // Test cases from schlich/helix-mode prototype.

    #[test]
    fn test_e() {
        let e = [EditCommand::HxWordMotion {
            target: WordMotionTarget::NextWordEnd,
            movement: Movement::Move,
            count: 1,
        }];

        assert_sel(&e, "[h]ello world", "[hello] world");
        assert_sel(&e, "hell[o] world", "hello[ world]");
        assert_sel(&e, "[h]ello.world", "[hello].world");
        assert_sel(&e, "hello[.]world", "hello.[world]");
        assert_sel(&e, "[h]ello_world test", "[hello_world] test");
        assert_sel(&e, "[t]est... next", "[test]... next");
        assert_sel(&e, "test[...] next", "test...[ next]");
    }

    // ── W (WORD right start) ────────────────────────────────────────────
    // Test cases from schlich/helix-mode prototype.

    #[test]
    fn test_big_w() {
        let w = [EditCommand::HxWordMotion {
            target: WordMotionTarget::NextLongWordStart,
            movement: Movement::Move,
            count: 1,
        }];

        assert_sel(&w, "[h]ello.world test", "[hello.world ]test");
        assert_sel(&w, "[h]ello world", "[hello ]world");
        assert_sel(&w, "[h]ello.world", "[hello.world]");
        assert_sel(&w, "hello.world [t]est", "hello.world [test]");
        assert_sel(&w, "[h]ello_world.test next", "[hello_world.test ]next");
        assert_sel(&w, "[t]est... next", "[test... ]next");
    }

    // ── B (WORD left) ───────────────────────────────────────────────────
    // Test cases from schlich/helix-mode prototype.

    #[test]
    fn test_big_b() {
        let b = [EditCommand::HxWordMotion {
            target: WordMotionTarget::PrevLongWordStart,
            movement: Movement::Move,
            count: 1,
        }];

        assert_sel(&b, "hello.world tes[t]", "hello.world ]test[");
        assert_sel(&b, "hello.world [t]est", "]hello.world [test");
        assert_sel(&b, "hello.worl[d]", "]hello.world[");
        assert_sel(&b, "test...nex[t]", "]test...next[");
    }

    // ── E (WORD right end) ──────────────────────────────────────────────
    // Test cases from schlich/helix-mode prototype.

    #[test]
    fn test_big_e() {
        let e = [EditCommand::HxWordMotion {
            target: WordMotionTarget::NextLongWordEnd,
            movement: Movement::Move,
            count: 1,
        }];

        assert_sel(&e, "[h]ello.world test", "[hello.world] test");
        assert_sel(&e, "[hello.world] test", "hello.world[ test]");
        assert_sel(&e, "[h]ello world", "[hello] world");
        assert_sel(&e, "[t]est...next more", "[test...next] more");
    }

    // ── parse_selection unit tests ──────────────────────────────────────

    #[test]
    fn test_parse_selection_forward() {
        let (buf, anchor, head) = parse_selection("[hello] world");
        assert_eq!(buf, "hello world");
        assert_eq!(anchor, 0);
        assert_eq!(head, 5);
    }

    #[test]
    fn test_parse_selection_backward() {
        let (buf, anchor, head) = parse_selection("]hello[ world");
        assert_eq!(buf, "hello world");
        assert_eq!(anchor, 5);
        assert_eq!(head, 0);
    }

    #[test]
    fn test_parse_selection_one_wide() {
        let (buf, anchor, head) = parse_selection("[h]ello world");
        assert_eq!(buf, "hello world");
        assert_eq!(anchor, 0);
        assert_eq!(head, 1);
    }

    #[test]
    fn test_parse_selection_mid_buffer() {
        let (buf, anchor, head) = parse_selection("hello [world]");
        assert_eq!(buf, "hello world");
        assert_eq!(anchor, 6);
        assert_eq!(head, 11);
    }

    #[test]
    fn test_parse_selection_utf8() {
        let (buf, anchor, head) = parse_selection("[café] world");
        assert_eq!(buf, "café world");
        assert_eq!(anchor, 0);
        assert_eq!(head, 5); // 'é' is 2 bytes: c(1) a(1) f(1) é(2) = 5
    }

    // ── Count prefix tests ────────────────────────────────────────────

    #[test]
    fn count_prefix_repeats_h_motion_normal() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        // Press '3' then 'h' — Normal mode batches moves + one restart.
        let event = hx.parse_event(char_key('3'));
        assert_eq!(event, ReedlineEvent::None);
        let event = hx.parse_event(char_key('h'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
                EditCommand::MoveLeft { select: false },
                EditCommand::MoveLeft { select: false },
                EditCommand::HxRestartSelection,
            ])
        );
    }

    #[test]
    fn count_prefix_repeats_h_motion_select() {
        let mut hx = Helix {
            mode: HelixMode::Select,
            ..Default::default()
        };
        // Press '3' then 'h' — Select mode needs per-step sync.
        hx.parse_event(char_key('3'));
        let event = hx.parse_event(char_key('h'));
        assert!(matches!(event, ReedlineEvent::Multiple(ref v) if v.len() == 3));
    }

    #[test]
    fn count_prefix_passes_to_word_motion() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        hx.parse_event(char_key('2'));
        let event = hx.parse_event(char_key('w'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![EditCommand::HxWordMotion {
                target: WordMotionTarget::NextWordStart,
                movement: Movement::Move,
                count: 2,
            }])
        );
    }

    #[test]
    fn count_zero_extends_digit() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        hx.parse_event(char_key('1'));
        hx.parse_event(char_key('0'));
        let event = hx.parse_event(char_key('l'));
        // Should produce 10 repetitions
        assert!(matches!(event, ReedlineEvent::Multiple(ref v) if v.len() == 10));
    }

    // ── Pending state tests ───────────────────────────────────────────

    #[test]
    fn invalid_key_after_goto_cancels() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        hx.parse_event(char_key('g'));
        let event = hx.parse_event(char_key('z')); // invalid goto target
        assert_eq!(event, ReedlineEvent::None);
        assert_eq!(hx.pending, Pending::None);
    }

    #[test]
    fn invalid_key_after_find_cancels() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        hx.parse_event(char_key('f'));
        // Esc is handled by the top-level Normal/Select match before pending
        // resolution, so it resets everything (mode, pending, count).
        let event = hx.parse_event(key_press(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(event, ReedlineEvent::Multiple(_)));
        assert_eq!(hx.pending, Pending::None);
    }

    #[test]
    fn goto_gg_moves_to_start() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        hx.parse_event(char_key('g'));
        let event = hx.parse_event(char_key('g'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveToStart { select: false },
                EditCommand::HxRestartSelection,
            ])
        );
    }

    #[test]
    fn goto_ge_is_unbound() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        hx.parse_event(char_key('g'));
        let event = hx.parse_event(char_key('e'));
        // ge is not yet implemented (needs PrevWordEnd motion target).
        assert_eq!(event, ReedlineEvent::None);
    }

    #[test]
    fn f_char_produces_extending_motion() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        hx.parse_event(char_key('f'));
        let event = hx.parse_event(char_key('x'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveRightUntil {
                    c: 'x',
                    select: false
                },
                EditCommand::HxSyncCursorWithRestart,
            ])
        );
    }

    // ── I/A mode switch tests ─────────────────────────────────────────

    #[test]
    fn big_i_enters_insert_at_line_start() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('I'));
        assert_eq!(
            event,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![
                    EditCommand::HxClearSelection,
                    EditCommand::MoveToLineStart { select: false },
                ]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(hx.mode, HelixMode::Insert);
    }

    #[test]
    fn big_a_enters_insert_at_line_end() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('A'));
        assert_eq!(
            event,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![
                    EditCommand::HxClearSelection,
                    EditCommand::MoveToLineEnd { select: false },
                ]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(hx.mode, HelixMode::Insert);
    }

    // ── Edit command tests ────────────────────────────────────────────

    #[test]
    fn d_deletes_with_yank() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('d'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::CutSelection,
                EditCommand::HxRestartSelection,
            ])
        );
    }

    #[test]
    fn alt_d_deletes_without_yank() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(key_press(KeyCode::Char('d'), KeyModifiers::ALT));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::HxDeleteSelection,
                EditCommand::HxRestartSelection,
            ])
        );
    }

    #[test]
    fn c_changes_with_yank() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('c'));
        assert_eq!(
            event,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![
                    EditCommand::HxEnsureSelection,
                    EditCommand::CutSelection,
                    EditCommand::HxClearSelection,
                ]),
                ReedlineEvent::Repaint,
            ])
        );
        assert_eq!(hx.mode, HelixMode::Insert);
    }

    #[test]
    fn y_yanks_preserving_selection() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('y'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::CopySelection,
            ])
        );
    }

    #[test]
    fn p_pastes_after_selection() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('p'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::HxMoveToSelectionEnd,
                EditCommand::HxClearSelection,
                EditCommand::PasteCutBufferBefore,
                EditCommand::HxRestartSelection,
            ])
        );
    }

    #[test]
    fn big_p_pastes_before_selection() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('P'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::HxMoveToSelectionStart,
                EditCommand::HxClearSelection,
                EditCommand::PasteCutBufferBefore,
                EditCommand::HxRestartSelection,
            ])
        );
    }

    // ── Selection command tests ───────────────────────────────────────

    #[test]
    fn semicolon_restarts_selection() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key(';'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![EditCommand::HxRestartSelection])
        );
    }

    #[test]
    fn percent_selects_entire_buffer() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('%'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveToStart { select: false },
                EditCommand::HxRestartSelection,
                EditCommand::MoveToEnd { select: false },
                EditCommand::HxSyncCursor,
            ])
        );
    }

    #[test]
    fn x_selects_entire_line() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('x'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveToLineStart { select: false },
                EditCommand::HxRestartSelection,
                EditCommand::MoveToLineEnd { select: false },
                EditCommand::HxSyncCursor,
            ])
        );
    }

    #[test]
    fn o_flips_selection() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('o'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![EditCommand::HxFlipSelection])
        );
    }

    // ── Undo/redo tests ───────────────────────────────────────────────

    #[test]
    fn u_undoes() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('u'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![EditCommand::Undo, EditCommand::HxRestartSelection,])
        );
    }

    #[test]
    fn big_u_redoes() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('U'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![EditCommand::Redo, EditCommand::HxRestartSelection,])
        );
    }

    // ── Replace char test ─────────────────────────────────────────────

    #[test]
    fn r_char_replaces_selection() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        hx.parse_event(char_key('r'));
        let event = hx.parse_event(char_key('z'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::HxReplaceSelectionWithChar('z'),
            ])
        );
    }

    // ── Tilde (switch case) test ──────────────────────────────────────

    #[test]
    fn tilde_switches_case() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(char_key('~'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::HxEnsureSelection,
                EditCommand::HxSwitchCaseSelection,
            ])
        );
    }

    // ── Esc in Normal/Select collapses pending state ──────────────────

    #[test]
    fn esc_in_normal_resets_pending_and_count() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        hx.parse_event(char_key('5')); // count
        hx.parse_event(char_key('g')); // pending goto
        let event = hx.parse_event(key_press(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(event, ReedlineEvent::Multiple(_)));
        assert_eq!(hx.mode, HelixMode::Normal);
        assert_eq!(hx.pending, Pending::None);
        assert_eq!(hx.count, 0);
    }

    // ── Enter in Normal mode submits ──────────────────────────────────

    #[test]
    fn enter_in_normal_submits() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        let event = hx.parse_event(key_press(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(event, ReedlineEvent::Enter);
    }

    // ── F/T backward find/til tests ─────────────────────────────────────

    #[test]
    fn big_f_char_produces_extending_motion() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        hx.parse_event(char_key('F'));
        let event = hx.parse_event(char_key('a'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeftUntil {
                    c: 'a',
                    select: false
                },
                EditCommand::HxSyncCursorWithRestart,
            ])
        );
    }

    #[test]
    fn big_t_char_produces_extending_motion() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        hx.parse_event(char_key('T'));
        let event = hx.parse_event(char_key('a'));
        assert_eq!(
            event,
            ReedlineEvent::Edit(vec![
                EditCommand::MoveLeftBefore {
                    c: 'a',
                    select: false
                },
                EditCommand::HxSyncCursorWithRestart,
            ])
        );
    }

    #[test]
    fn count_with_f_produces_multiple_events() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        hx.parse_event(char_key('2'));
        hx.parse_event(char_key('f'));
        let event = hx.parse_event(char_key('x'));
        // count=2: first has HxSyncCursorWithRestart, rest have HxSyncCursor
        assert!(matches!(event, ReedlineEvent::Multiple(ref v) if v.len() == 2));
    }

    // ── %/x integration tests ───────────────────────────────────────────

    #[test]
    fn percent_selects_all_integration() {
        let mut editor = editor_with("hello world", 3);
        run_commands(
            &mut editor,
            &[
                EditCommand::MoveToStart { select: false },
                EditCommand::HxRestartSelection,
                EditCommand::MoveToEnd { select: false },
                EditCommand::HxSyncCursor,
            ],
        );
        let sel = editor.hx_selection().expect("expected selection");
        assert_eq!(sel.range(), (0, 11));
    }

    #[test]
    fn x_selects_line_integration() {
        let mut editor = editor_with("hello world", 5);
        run_commands(
            &mut editor,
            &[
                EditCommand::MoveToLineStart { select: false },
                EditCommand::HxRestartSelection,
                EditCommand::MoveToLineEnd { select: false },
                EditCommand::HxSyncCursor,
            ],
        );
        let sel = editor.hx_selection().expect("expected selection");
        assert_eq!(sel.range(), (0, 11));
    }

    // ── Count prefix edge cases ─────────────────────────────────────────

    #[test]
    fn count_zero_at_start_is_not_count() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        // '0' at start should not be a count digit (count is 0, so it's not > 0)
        let event = hx.parse_event(char_key('0'));
        // Falls through to match code block (no motion bound to '0')
        assert_eq!(event, ReedlineEvent::None);
        assert_eq!(hx.count, 0);
    }

    #[test]
    fn large_count_on_short_buffer_does_not_panic() {
        let mut hx = Helix {
            mode: HelixMode::Normal,
            ..Default::default()
        };
        // Enter count 100
        hx.parse_event(char_key('1'));
        hx.parse_event(char_key('0'));
        hx.parse_event(char_key('0'));
        let event = hx.parse_event(char_key('h'));
        // Should produce 100 MoveLeft + HxRestartSelection
        match event {
            ReedlineEvent::Edit(ref cmds) => {
                assert_eq!(cmds.len(), 101); // 100 MoveLeft + 1 HxRestartSelection
            }
            _ => panic!("expected Edit event"),
        }
    }

    // ── UTF-8 word motion tests ─────────────────────────────────────────

    #[test]
    fn test_w_utf8_cafe() {
        let w = [EditCommand::HxWordMotion {
            target: WordMotionTarget::NextWordStart,
            movement: Movement::Move,
            count: 1,
        }];
        // "café world" — é is 2 bytes, so "café" ends at byte 5
        assert_sel(&w, "[c]afé world", "[café ]world");
    }

    #[test]
    fn test_b_utf8_uber() {
        let b = [EditCommand::HxWordMotion {
            target: WordMotionTarget::PrevWordStart,
            movement: Movement::Move,
            count: 1,
        }];
        // "über cool" — ü is 2 bytes
        assert_sel(&b, "über coo[l]", "über ]cool[");
    }

    // ── Esc from Insert resets insert_style ─────────────────────────────

    #[test]
    fn esc_from_insert_resets_insert_style() {
        let mut hx = Helix {
            mode: HelixMode::Insert,
            insert_style: InsertStyle::After,
            ..Default::default()
        };
        hx.parse_event(key_press(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(hx.mode, HelixMode::Normal);
        assert_eq!(hx.insert_style, InsertStyle::Plain);
    }

    // ── f/t extending motion integration tests ──────────────────────────

    #[test]
    fn f_char_integration() {
        // "hello world" cursor at 0: f+o should select from h to o (inclusive)
        let mut editor = editor_with("hello world", 0);
        editor.run_edit_command(&EditCommand::HxRestartSelection);
        editor.run_edit_command(&EditCommand::MoveRightUntil {
            c: 'o',
            select: false,
        });
        editor.run_edit_command(&EditCommand::HxSyncCursorWithRestart);
        let sel = editor.hx_selection().expect("expected selection");
        // Cursor was at 0, 'o' is at index 4. Selection should cover 0..5
        assert_eq!(sel.anchor, 0);
        assert_eq!(sel.head, 5);
    }

    #[test]
    fn t_char_integration() {
        // "hello world" cursor at 0: t+o should select from h to just before o
        let mut editor = editor_with("hello world", 0);
        editor.run_edit_command(&EditCommand::HxRestartSelection);
        editor.run_edit_command(&EditCommand::MoveRightBefore {
            c: 'o',
            select: false,
        });
        editor.run_edit_command(&EditCommand::HxSyncCursorWithRestart);
        let sel = editor.hx_selection().expect("expected selection");
        // 't' stops one before 'o' (index 4), so cursor at 3 → head=4
        assert_eq!(sel.anchor, 0);
        assert_eq!(sel.head, 4);
    }
}
