use super::{
    edit_stack::EditStack, CaretGeometry, Clipboard, Cursor, LineBuffer, Movement, SelectionExtent,
};
#[cfg(feature = "system_clipboard")]
use crate::core_editor::get_system_clipboard;
use crate::core_editor::graphemes::{next_grapheme_boundary, prev_grapheme_boundary};
use crate::core_editor::{commit, line, operator_span, resolve_motion, RestPolicy};
use crate::enums::{EditType, TextObject, TextObjectScope, TextObjectType, UndoBehavior};
use crate::prompt::PromptEditMode;
use crate::{core_editor::get_local_clipboard, EditCommand};
use crate::{Direction, Granularity, MotionTarget, WordEdge, WordKind};
use std::cmp::{max, min};
use std::ops::{DerefMut, Range};

/// Stateful editor executing changes to the underlying [`LineBuffer`]
///
/// In comparison to the state-less [`LineBuffer`] the [`Editor`] keeps track of
/// the undo/redo history and has facilities for cut/copy/yank/paste
pub struct Editor {
    line_buffer: LineBuffer,
    cut_buffer: Box<dyn Clipboard>,
    #[cfg(feature = "system_clipboard")]
    system_clipboard: Box<dyn Clipboard>,
    edit_stack: EditStack<LineBuffer>,
    last_undo_behavior: UndoBehavior,
    edit_mode: PromptEditMode,
    /// Set when [`sync_edit_mode`](Self::sync_edit_mode) adopts a new rest
    /// policy without committing the cursor; cleared at the commit boundary.
    /// Lets the pre-paint sweep settle a command-less mode transition that
    /// would otherwise never re-normalize under the new policy.
    policy_unsettled: bool,
    /// When `true`, a grapheme left/right motion under a block caret (vi normal)
    /// crosses line terminators — `l` at a line's end lands on the next line's
    /// first grapheme, `h` at column 0 on the previous line's last. When `false`
    /// the motion is clamped to the current line (vim's default `h`/`l`). Bar
    /// carets (emacs, vi insert) always cross regardless, since a bar may rest in
    /// the gap around a `\n`. Defaults to `true`.
    cross_line_cursor: bool,
}

enum OperatorVerb {
    Cut,
    Copy,
    /// Cut, but a `LineWise` span keeps its line terminators so one blank line
    /// remains — vi's change operator (`cc`/`cj`/`cgg`). Identical to `Cut`
    /// for `CharWise` spans.
    Change,
    Erase,
}

/// Build a word [`MotionTarget`] — the shared shape the legacy `*Word*` command
/// sugar lowers to. The emacs-flavored bindings pass [`WordKind::Unicode`]
/// (UAX-29, proven equivalent to the old `*_index` scans); the big-WORD sugar
/// passes [`WordKind::LongWord`].
fn word_target(kind: WordKind, edge: WordEdge, direction: Direction) -> MotionTarget {
    MotionTarget::Word {
        kind,
        edge,
        direction,
    }
}

impl Default for Editor {
    fn default() -> Self {
        Editor {
            line_buffer: LineBuffer::new(),
            cut_buffer: get_local_clipboard(),
            #[cfg(feature = "system_clipboard")]
            system_clipboard: get_system_clipboard(),
            edit_stack: EditStack::new(),
            last_undo_behavior: UndoBehavior::CreateUndoPoint,
            edit_mode: PromptEditMode::Default,
            policy_unsettled: false,
            cross_line_cursor: true,
        }
    }
}

impl Editor {
    /// Get the current [`LineBuffer`]
    pub const fn line_buffer(&self) -> &LineBuffer {
        &self.line_buffer
    }

    /// Set the current [`LineBuffer`].
    /// [`UndoBehavior`] specifies how this change should be reflected on the undo stack.
    pub(crate) fn set_line_buffer(&mut self, line_buffer: LineBuffer, undo_behavior: UndoBehavior) {
        self.line_buffer = line_buffer;
        self.update_undo_state(undo_behavior);
    }

    pub(crate) fn run_edit_command(&mut self, command: &EditCommand) {
        match command {
            EditCommand::MoveToStart { select } => self.move_to_start(*select),
            EditCommand::MoveToLineStart { select } => self.move_to_line_start(*select),
            EditCommand::MoveToLineNonBlankStart { select } => {
                self.move_to_line_non_blank_start(*select)
            }
            EditCommand::MoveToEnd { select } => self.move_to_end(*select),
            EditCommand::MoveToLineEnd { select } => self.move_to_line_end(*select),
            EditCommand::MoveToPosition { position, select } => {
                self.move_head_to(*position, *select)
            }
            EditCommand::MoveLineUp { select } => self.move_line_up(*select),
            EditCommand::MoveLineDown { select } => self.move_line_down(*select),
            EditCommand::MoveLeft { select } => self.move_left(*select),
            EditCommand::MoveRight { select } => self.move_right(*select),
            EditCommand::MoveWordLeft { select } => self.move_word_left(*select),
            EditCommand::MoveBigWordLeft { select } => self.move_big_word_left(*select),
            EditCommand::MoveWordRight { select } => self.move_word_right(*select),
            EditCommand::MoveWordRightStart { select } => self.move_word_right_start(*select),
            EditCommand::MoveBigWordRightStart { select } => {
                self.move_big_word_right_start(*select)
            }
            EditCommand::MoveWordRightEnd { select } => self.move_word_right_end(*select),
            EditCommand::MoveBigWordRightEnd { select } => self.move_big_word_right_end(*select),
            EditCommand::Move(t) => {
                let head = self.resolve_head(*t);
                self.move_head_to(head, false);
            }
            EditCommand::Extend(t) => match self.caret_extent() {
                SelectionExtent::CoverLanding => {
                    let head = self.resolve_head(*t);
                    self.move_head_to(head, true);
                }
                SelectionExtent::Span => {
                    let geom = self.caret_geometry();
                    let origin = self.insertion_point();
                    let op_end = resolve_motion(self.get_buffer(), origin, *t, geom).op_end;
                    let next =
                        self.line_buffer
                            .cursor()
                            .extend_span(self.get_buffer(), op_end, geom);
                    self.place(next);
                }
            },
            EditCommand::Cut {
                target,
                granularity,
            } => {
                let sel = operator_span(
                    self.get_buffer(),
                    self.insertion_point(),
                    *target,
                    self.caret_geometry(),
                );
                self.operate(sel, OperatorVerb::Cut, *granularity);
            }
            EditCommand::Copy {
                target,
                granularity,
            } => {
                let sel = operator_span(
                    self.get_buffer(),
                    self.insertion_point(),
                    *target,
                    self.caret_geometry(),
                );
                self.operate(sel, OperatorVerb::Copy, *granularity);
            }
            EditCommand::Change {
                target,
                granularity,
            } => {
                let sel = operator_span(
                    self.get_buffer(),
                    self.insertion_point(),
                    *target,
                    self.caret_geometry(),
                );
                self.operate(sel, OperatorVerb::Change, *granularity);
            }
            EditCommand::Erase(t) => {
                let sel = operator_span(
                    self.get_buffer(),
                    self.insertion_point(),
                    *t,
                    self.caret_geometry(),
                );
                self.operate(sel, OperatorVerb::Erase, Granularity::CharWise);
            }
            EditCommand::InsertChar(c) => self.insert_char(*c),
            EditCommand::Complete => {}
            EditCommand::InsertString(str) => self.insert_str(str),
            EditCommand::InsertNewline => self.insert_newline(),
            EditCommand::InsertNewlineAbove => self.insert_newline_above(),
            EditCommand::InsertNewlineBelow => self.insert_newline_below(),
            EditCommand::ReplaceChar(chr) => self.replace_char(*chr),
            EditCommand::ReplaceChars(n_chars, str) => self.replace_chars(*n_chars, str),
            EditCommand::Backspace => self.backspace(),
            EditCommand::Delete => self.delete(),
            EditCommand::CutChar => self.cut_char(),
            EditCommand::BackspaceWord => self.line_buffer.delete_word_left(),
            EditCommand::DeleteWord => self.line_buffer.delete_word_right(),
            EditCommand::Clear => self.line_buffer.clear(),
            EditCommand::ClearToLineEnd => self.line_buffer.clear_to_line_end(),
            EditCommand::CutCurrentLine => self.cut_current_line(),
            EditCommand::CutFromStart => self.cut_from_start(),
            EditCommand::CutFromStartLinewise { leave_blank_line } => {
                self.cut_from_start_linewise(*leave_blank_line)
            }
            EditCommand::CutFromLineStart => self.cut_from_line_start(),
            EditCommand::CutFromLineNonBlankStart => self.cut_from_line_non_blank_start(),
            EditCommand::CutToEnd => self.cut_from_end(),
            EditCommand::CutToEndLinewise { leave_blank_line } => {
                self.cut_from_end_linewise(*leave_blank_line)
            }
            EditCommand::CutToLineEnd => self.cut_to_line_end(),
            EditCommand::KillLine => self.kill_line(),
            EditCommand::CutWordLeft => self.cut_word_left(),
            EditCommand::CutBigWordLeft => self.cut_big_word_left(),
            EditCommand::CutWordRight => self.cut_word_right(),
            EditCommand::CutBigWordRight => self.cut_big_word_right(),
            EditCommand::CutWordRightToNext => self.cut_word_right_to_next(),
            EditCommand::CutBigWordRightToNext => self.cut_big_word_right_to_next(),
            EditCommand::PasteCutBufferBefore => self.insert_cut_buffer_before(),
            EditCommand::PasteCutBufferAfter => self.insert_cut_buffer_after(),
            EditCommand::UppercaseWord => self.line_buffer.uppercase_word(),
            EditCommand::LowercaseWord => self.line_buffer.lowercase_word(),
            EditCommand::SwitchcaseChar => self.line_buffer.switchcase_char(),
            EditCommand::CapitalizeChar => self.line_buffer.capitalize_char(),
            EditCommand::SwapWords => self.line_buffer.swap_words(),
            EditCommand::SwapGraphemes => self.line_buffer.swap_graphemes(),
            EditCommand::Undo => self.undo(),
            EditCommand::Redo => self.redo(),
            EditCommand::CutRightUntil(c) => self.cut_right_until_char(*c, false, true),
            EditCommand::CutRightBefore(c) => self.cut_right_until_char(*c, true, true),
            EditCommand::MoveRightUntil { c, select } => {
                self.move_right_until_char(*c, false, true, *select)
            }
            EditCommand::MoveRightBefore { c, select } => {
                self.move_right_until_char(*c, true, true, *select)
            }
            EditCommand::CutLeftUntil(c) => self.cut_left_until_char(*c, false, true),
            EditCommand::CutLeftBefore(c) => self.cut_left_until_char(*c, true, true),
            EditCommand::MoveLeftUntil { c, select } => {
                self.move_left_until_char(*c, false, true, *select)
            }
            EditCommand::MoveLeftBefore { c, select } => {
                self.move_left_until_char(*c, true, true, *select)
            }
            EditCommand::SelectAll => self.select_all(),
            EditCommand::CutSelection => self.cut_selection_to_cut_buffer(),
            EditCommand::CopySelection => self.copy_selection_to_cut_buffer(),
            EditCommand::Paste => self.paste_cut_buffer(),
            EditCommand::CopyFromStart => self.copy_from_start(),
            EditCommand::CopyFromStartLinewise => self.copy_from_start_linewise(),
            EditCommand::CopyFromLineStart => self.copy_from_line_start(),
            EditCommand::CopyFromLineNonBlankStart => self.copy_from_line_non_blank_start(),
            EditCommand::CopyToEnd => self.copy_from_end(),
            EditCommand::CopyToEndLinewise => self.copy_from_end_linewise(),
            EditCommand::CopyToLineEnd => self.copy_to_line_end(),
            EditCommand::CopyWordLeft => self.copy_word_left(),
            EditCommand::CopyBigWordLeft => self.copy_big_word_left(),
            EditCommand::CopyWordRight => self.copy_word_right(),
            EditCommand::CopyBigWordRight => self.copy_big_word_right(),
            EditCommand::CopyWordRightToNext => self.copy_word_right_to_next(),
            EditCommand::CopyBigWordRightToNext => self.copy_big_word_right_to_next(),
            EditCommand::CopyRightUntil(c) => self.copy_right_until_char(*c, false, true),
            EditCommand::CopyRightBefore(c) => self.copy_right_until_char(*c, true, true),
            EditCommand::CopyLeftUntil(c) => self.copy_left_until_char(*c, false, true),
            EditCommand::CopyLeftBefore(c) => self.copy_left_until_char(*c, true, true),
            EditCommand::CopyCurrentLine => {
                let range = self.line_buffer.current_line_range();
                let copy_slice = &self.line_buffer.get_buffer()[range];
                if !copy_slice.is_empty() {
                    self.cut_buffer.set(copy_slice, Granularity::LineWise);
                }
            }
            EditCommand::CopyLeft => {
                let insertion_offset = self.line_buffer.insertion_point();
                if insertion_offset > 0 {
                    let left_index = self.line_buffer.grapheme_left_index();
                    let copy_range = left_index..insertion_offset;
                    self.cut_buffer.set(
                        &self.line_buffer.get_buffer()[copy_range],
                        Granularity::CharWise,
                    );
                }
            }
            EditCommand::CopyRight => {
                let insertion_offset = self.line_buffer.insertion_point();
                let right_index = self.line_buffer.grapheme_right_index();
                if right_index > insertion_offset {
                    let copy_range = insertion_offset..right_index;
                    self.cut_buffer.set(
                        &self.line_buffer.get_buffer()[copy_range],
                        Granularity::CharWise,
                    );
                }
            }
            EditCommand::SwapCursorAndAnchor => self
                .line_buffer
                .set_cursor(self.line_buffer.cursor().flip()),
            #[cfg(feature = "system_clipboard")]
            EditCommand::CutSelectionSystem => self.cut_selection_to_system(),
            #[cfg(feature = "system_clipboard")]
            EditCommand::CopySelectionSystem => self.copy_selection_to_system(),
            #[cfg(feature = "system_clipboard")]
            EditCommand::PasteSystem => self.paste_from_system(),
            EditCommand::CutInsidePair { left, right } => self.cut_inside_pair(*left, *right),
            EditCommand::CopyInsidePair { left, right } => self.copy_inside_pair(*left, *right),
            EditCommand::CutAroundPair { left, right } => self.cut_around_pair(*left, *right),
            EditCommand::CopyAroundPair { left, right } => self.copy_around_pair(*left, *right),
            EditCommand::CutTextObject { text_object } => self.cut_text_object(*text_object),
            EditCommand::CopyTextObject { text_object } => self.copy_text_object(*text_object),
        }
        if !matches!(command.edit_type(), EditType::MoveCursor { select: true }) {
            self.clear_selection();
        }

        self.commit_cursor();

        let new_undo_behavior = match (command, command.edit_type()) {
            (_, EditType::MoveCursor { .. }) => UndoBehavior::MoveCursor,
            (EditCommand::InsertChar(c), EditType::EditText) => UndoBehavior::InsertCharacter(*c),
            (EditCommand::Delete, EditType::EditText) => {
                let deleted_char = self.edit_stack.current().grapheme_right().chars().next();
                UndoBehavior::Delete(deleted_char)
            }
            (EditCommand::Backspace, EditType::EditText) => {
                let deleted_char = self.edit_stack.current().grapheme_left().chars().next();
                UndoBehavior::Backspace(deleted_char)
            }
            (_, EditType::UndoRedo | EditType::NoOp) => UndoBehavior::NoOp,
            (_, _) => UndoBehavior::CreateUndoPoint,
        };

        self.update_undo_state(new_undo_behavior);
    }

    pub(crate) fn clear_selection(&mut self) {
        // Collapse to the caret (the visible position), not merely drop the
        // anchor: under `Block` the stored head sits on the far edge, so dropping
        // the anchor alone would strand the cursor one grapheme past where it
        // shows. Collapsing to `point(caret)` keeps it put; the commit boundary
        // re-widens it under the active policy.
        let caret = self.line_buffer.insertion_point();
        self.line_buffer.set_cursor(Cursor::point(caret));
    }

    fn operate(&mut self, selection: Cursor, verb: OperatorVerb, granularity: Granularity) {
        // `register` is the span the cut buffer keeps; `delete` is the span that
        // leaves the buffer. They coincide except for a linewise Cut/Copy of the
        // *last* line: the deletion eats the preceding terminator so no blank line
        // is stranded, but the register must hold only the line's content —
        // otherwise a later linewise paste re-introduces that newline as a
        // spurious leading blank line.
        let (register, delete) = match granularity {
            Granularity::CharWise => {
                let r = selection.start()..selection.end();
                (r.clone(), r)
            }
            Granularity::LineWise => {
                let buf = self.get_buffer();
                let s = line::start_of_line(buf, selection.start());
                match verb {
                    // Change keeps the line terminators: only the lines'
                    // content is consumed, so one blank line remains for the
                    // re-entered insert mode. Register and deletion coincide.
                    OperatorVerb::Change => {
                        let r = s..line::end_of_line(buf, selection.end());
                        (r.clone(), r)
                    }
                    // Cut/Copy/Erase consume whole lines including the trailing
                    // `\n`. On the last line (no trailing `\n`) the *deletion*
                    // eats the whole preceding terminator instead so no stray
                    // blank line is left — 2 bytes for a `\r\n`, so the `\r` is
                    // not orphaned (e.g. a CRLF history entry "ab\r\ncd" + `dd`
                    // → "ab"; the buffer can carry CR, see `LineBuffer`'s
                    // line-ending contract). The *register* keeps just `s..e` so
                    // a later linewise paste does not gain a leading blank line.
                    _ => {
                        let e = line::start_of_next_line(buf, selection.end()).unwrap_or(buf.len());
                        let delete_start = if e == buf.len() && s > 0 {
                            if buf[..s].ends_with("\r\n") {
                                s - 2
                            } else {
                                s - 1
                            }
                        } else {
                            s
                        };
                        (s..e, delete_start..e)
                    }
                }
            }
        };

        match verb {
            OperatorVerb::Cut => {
                self.copy_range_with(register, granularity);
                self.line_buffer.clear_range_safe(delete.clone());
                self.line_buffer.set_insertion_point(delete.start);
            }
            // Change's register and deletion coincide, so one range suffices.
            OperatorVerb::Change => self.cut_range_with(delete, granularity),
            OperatorVerb::Copy => self.copy_range_with(register, granularity),
            OperatorVerb::Erase => {
                self.line_buffer.clear_range_safe(delete.clone());
                self.line_buffer.set_insertion_point(delete.start);
            }
        }
    }

    /// Plant or clear a selection anchor explicitly — retained only for test
    /// setup. Production selections open through [`move_head_to`](Self::move_head_to)
    /// (`put_cursor`) or the `Block` min-width-1 commit, neither of which needs
    /// this.
    #[cfg(test)]
    fn update_selection_anchor(&mut self, select: bool) {
        if select {
            if self.line_buffer.selection_anchor().is_none() {
                self.line_buffer
                    .set_selection_anchor(Some(self.insertion_point()));
            }
        } else {
            self.clear_selection();
        }
    }

    /// Set the current edit mode
    pub fn set_edit_mode(&mut self, mode: PromptEditMode) {
        // Called on every repaint, so skip the work when nothing relevant moved.
        // `commit_cursor` depends only on the rest policy, and the cursor is
        // already committed under the old one; re-normalize only when the policy
        // actually changes (e.g. Vi insert → normal tightens to `OnGrapheme`).
        let policy_changed = mode.rest_policy() != self.edit_mode.rest_policy();
        self.edit_mode = mode;
        // `sync_edit_mode` may have already adopted this policy without
        // committing (a command-less transition), so `policy_changed` can read
        // false here even though the cursor still owes a settle.
        if policy_changed || self.policy_unsettled {
            self.commit_cursor();
        }
    }

    /// Whether a rest-policy change is awaiting a commit (see
    /// [`policy_unsettled`](Self::policy_unsettled) field).
    pub(crate) fn policy_unsettled(&self) -> bool {
        self.policy_unsettled
    }

    /// Set whether a block-caret left/right motion crosses line terminators (see
    /// the [`cross_line_cursor`](Self::cross_line_cursor) field).
    pub(crate) fn set_cross_line_cursor(&mut self, cross: bool) {
        self.cross_line_cursor = cross;
    }

    /// Adopt `mode`'s rest policy *without* committing the cursor.
    ///
    /// Called at the parse seam, before the events a mode transition emitted
    /// are run, so those commands resolve under the new [`RestPolicy`] (e.g.
    /// the Esc→normal grapheme step-back reads `OnGrapheme`). The cursor is
    /// deliberately left where insert mode put it: the emitted commands move
    /// and commit it under the new policy, and any no-command transition is
    /// settled by the pre-paint `set_edit_mode`. Committing here would pull a
    /// caret at the line end back a grapheme, double-stepping the Esc move.
    pub fn sync_edit_mode(&mut self, mode: PromptEditMode) {
        if mode.rest_policy() != self.edit_mode.rest_policy() {
            self.policy_unsettled = true;
        }
        self.edit_mode = mode;
    }

    /// Normalize the cursor at the single commit boundary: clamp + grapheme-snap
    /// (universal), then apply the active mode's [`RestPolicy`]. Total and
    /// idempotent, so it is safe to call after any state change — including ones
    /// that move the cursor outside the command path (e.g. history navigation),
    /// which must still settle a vi-normal caret off the line end.
    pub(crate) fn commit_cursor(&mut self) {
        let committed = commit(
            self.line_buffer.get_buffer(),
            self.line_buffer.cursor(),
            self.edit_mode.rest_policy(),
        );
        self.line_buffer.set_cursor(committed);
        self.policy_unsettled = false;
    }

    /// Plain head move with no `put_cursor` geometry — retained only for test
    /// setup. Production motions route through [`move_head_to`](Self::move_head_to)
    /// so selections get the inclusive anchor-flip.
    #[cfg(test)]
    fn move_to_position(&mut self, position: usize, select: bool) {
        self.line_buffer.move_head(position, select);
    }

    /// Resolve a motion target to the byte the caret should land on.
    ///
    /// The origin is always the visible caret (Helix `move_horizontally` does the
    /// same: `range.cursor()` is the origin for both Move and Extend). The
    /// returned target is fed to [`Cursor::put_cursor`], which places the head and
    /// flips the anchor as needed — so there is no head-vs-caret origin split.
    fn resolve_head(&self, target: MotionTarget) -> usize {
        let buf = self.line_buffer.get_buffer();
        // Origin is the visible cursor position — `insertion_point()` already
        // resolves that per policy (head for Between, caret for Block).
        let origin = self.insertion_point();
        let head = resolve_motion(buf, origin, target, self.caret_geometry()).head;
        // Only a block-caret grapheme step needs a line policy at the edges; every
        // other target's line-crossing is already fixed by `resolve_motion`, and a
        // bar caret (`Between`) moves freely across the terminator either way.
        if let MotionTarget::Grapheme(direction) = target {
            if self.caret_geometry() == CaretGeometry::Block {
                return self.grapheme_line_policy(buf, origin, head, direction);
            }
        }
        head
    }

    /// The block-caret line policy for one grapheme step (`h`/`l` in vi
    /// normal/visual): per [`cross_line_cursor`](Self::cross_line_cursor), either
    /// clamp the landing to the current line, or cross the terminator onto a real
    /// cell on the adjacent line. `origin` is the step's start, `head` its raw
    /// one-grapheme landing.
    ///
    /// This is a *movement-landing* transform only. Operator spans (`d`/`c`/`y`)
    /// deliberately bypass it — they resolve straight through `resolve_motion` and
    /// delete the literal grapheme range, which must not skip the `\n` (e.g. `dl`
    /// deletes the char under the caret, never the line break). So the flag steers
    /// where the caret *rests*, not how far an operator reaches.
    fn grapheme_line_policy(
        &self,
        buf: &str,
        origin: usize,
        head: usize,
        direction: Direction,
    ) -> usize {
        if !self.cross_line_cursor {
            // vim-strict: the caret may not leave the current line.
            return match direction {
                Direction::Backward => head.max(line::start_of_line(buf, origin)),
                Direction::Forward => head.min(line::end_of_line(buf, origin)),
            };
        }
        // Cross the terminator so the caret lands on a real cell, not the `\n`.
        // Forward: skip onto the next line's first grapheme. Backward: step once
        // more onto the previous line's last grapheme — unless that line is *also*
        // a terminator (an empty line), where column 0 is the only cell.
        let is_terminator = |pos: usize| buf[pos..].starts_with(['\r', '\n']);
        if !is_terminator(head) {
            return head;
        }
        match direction {
            Direction::Forward => next_grapheme_boundary(buf, head),
            Direction::Backward => {
                let back = prev_grapheme_boundary(buf, head);
                if is_terminator(back) {
                    head // previous line is empty — rest on its column 0
                } else {
                    back
                }
            }
        }
    }

    /// Caret geometry of the active mode: [`CaretGeometry::Block`] for vi normal
    /// / visual (an inclusive motion lands *on* a grapheme and the operator eats
    /// it), [`CaretGeometry::Bar`] for emacs / vi insert (`Between`, resting on
    /// the trailing boundary). Drives the forward word-end landing and operator
    /// inclusivity in [`resolve_motion`] and the selection extension in
    /// [`Cursor::put_cursor`].
    fn caret_geometry(&self) -> CaretGeometry {
        if self.edit_mode.rest_policy() == RestPolicy::Between {
            CaretGeometry::Bar
        } else {
            CaretGeometry::Block
        }
    }

    /// Place the caret on the grapheme at `target` via [`Cursor::put_cursor`]
    /// (Helix's central op), collapsing the selection unless `select` keeps the
    /// anchor, then normalize at the commit boundary (RestPolicy snap and
    /// selection bookkeeping). The per-mode geometry (inclusive block vs
    /// exclusive bar) rides on [`caret_geometry`](Self::caret_geometry), so
    /// inclusivity is carried by the range itself — there is no
    /// `selection_inclusive` side-channel to maintain.
    ///
    /// The sink for [`CoverLanding`](SelectionExtent::CoverLanding) placement:
    /// every `Move`, and every `Extend` under that extent, funnels here after
    /// its target is resolved via [`resolve_motion`]. [`SelectionExtent::Span`]
    /// extension goes around it through [`Cursor::extend_span`].
    fn move_head_to(&mut self, target: usize, select: bool) {
        let next = self.line_buffer.cursor().put_cursor(
            self.line_buffer.get_buffer(),
            target,
            Movement::select(select),
            self.caret_geometry(),
        );
        self.place(next);
    }

    /// Install an already-placed [`Cursor`] and normalize it at the commit
    /// boundary — the shared tail of every placement strategy ([`put_cursor`]'s
    /// `CoverLanding` via [`move_head_to`](Self::move_head_to), [`extend_span`]'s
    /// `Span`). The strategy decides *where* the caret goes; `place` is *how* it
    /// lands: `set_cursor` then [`commit_cursor`](Self::commit_cursor).
    ///
    /// [`put_cursor`]: Cursor::put_cursor
    /// [`extend_span`]: Cursor::extend_span
    fn place(&mut self, next: Cursor) {
        self.line_buffer.set_cursor(next);
        self.commit_cursor();
    }

    /// The active mode's selection model: how `Extend` places the head (vi-visual
    /// `CoverLanding` vs bar/helix `Span`). Orthogonal to [`caret_geometry`](Self::caret_geometry).
    fn caret_extent(&self) -> SelectionExtent {
        self.edit_mode.selection_extent()
    }

    /// Lower a [`MotionTarget`] onto the cursor (the `Move`/`Extend` path):
    /// resolve the head per the active policy, then place it — collapsing the
    /// selection unless `select` keeps the anchor. The shared sink the legacy
    /// `MoveWord*` sugar funnels through, so they behave identically to an
    /// equivalent `Move`/`Extend` command.
    fn apply_move(&mut self, target: MotionTarget, select: bool) {
        let head = self.resolve_head(target);
        self.move_head_to(head, select);
    }

    /// Lower an operator over a [`MotionTarget`] onto the buffer (the
    /// `Cut`/`Copy` path) at char-wise granularity. The shared sink the legacy
    /// `CutWord*`/`CopyWord*` sugar funnels through: `operator_span`'s `op_end`
    /// already encodes inclusivity, so the consumed range matches the old
    /// hand-built `insertion_point..*_index` ranges.
    fn apply_operator(&mut self, target: MotionTarget, verb: OperatorVerb) {
        let sel = operator_span(
            self.get_buffer(),
            self.insertion_point(),
            target,
            self.caret_geometry(),
        );
        self.operate(sel, verb, Granularity::CharWise);
    }

    pub(crate) fn move_line_up(&mut self, select: bool) {
        if let Some(target) = self.line_buffer.line_up_target() {
            self.move_head_to(target, select);
        }
        self.update_undo_state(UndoBehavior::MoveCursor);
    }

    pub(crate) fn move_line_down(&mut self, select: bool) {
        if let Some(target) = self.line_buffer.line_down_target() {
            self.move_head_to(target, select);
        }
        self.update_undo_state(UndoBehavior::MoveCursor);
    }

    /// Get the text of the current [`LineBuffer`]
    pub fn get_buffer(&self) -> &str {
        self.line_buffer.get_buffer()
    }

    /// Edit the [`LineBuffer`] in an undo-safe manner.
    pub fn edit_buffer<F>(&mut self, func: F, undo_behavior: UndoBehavior)
    where
        F: FnOnce(&mut LineBuffer),
    {
        self.update_undo_state(undo_behavior);
        func(&mut self.line_buffer);
    }

    /// Set the text of the current [`LineBuffer`] given the specified [`UndoBehavior`]
    /// Insertion point update to the end of the buffer.
    pub(crate) fn set_buffer(&mut self, buffer: String, undo_behavior: UndoBehavior) {
        self.line_buffer.set_buffer(buffer);
        // History navigation replaces the buffer outside the command path, so
        // normalize the cursor here too (e.g. Vi normal must not sit past the end).
        self.commit_cursor();
        self.update_undo_state(undo_behavior);
    }

    pub(crate) fn insertion_point(&self) -> usize {
        // The visible / edit position is policy-dependent: a `Between` (bar)
        // cursor sits at the head; a `Block` cursor sits at the caret (its left
        // edge). This is the one place that distinction lives — motions, edits
        // and callers all read it from here.
        let cursor = self.line_buffer.cursor();
        if self.edit_mode.rest_policy() == RestPolicy::Between {
            cursor.head()
        } else {
            cursor.caret(self.line_buffer.get_buffer())
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.line_buffer.is_empty()
    }

    pub(crate) fn is_cursor_at_first_line(&self) -> bool {
        self.line_buffer.is_cursor_at_first_line()
    }

    pub(crate) fn is_cursor_at_last_line(&self) -> bool {
        self.line_buffer.is_cursor_at_last_line()
    }

    pub(crate) fn is_cursor_at_buffer_end(&self) -> bool {
        let buf = self.get_buffer();
        let cursor = self.line_buffer.cursor();
        // An active selection is never a clean end-of-buffer point. Completing a
        // history hint (or appending) here would run through `delete_selection`
        // and clobber the selection — so report `false`, matching the old
        // caret-based check, which a forward selection's caret (one inward from
        // `len`) already failed.
        if !cursor.is_empty() {
            return false;
        }
        if self.caret_geometry() == CaretGeometry::Block {
            // Cell caret (vi normal): the resting point sits *on* the last
            // grapheme, one inward from `len`. "At the end" means that cell is the
            // final one — nothing lies to its right. (A bare `head == len` check
            // never holds here, which is why a history hint stopped completing in
            // normal mode after the cursor became the single source of truth.)
            next_grapheme_boundary(buf, cursor.head()) == buf.len()
        } else {
            // Bar caret (emacs / vi insert): at the end iff the head rests past
            // the last grapheme.
            cursor.head() == buf.len()
        }
    }

    pub(crate) fn reset_undo_stack(&mut self) {
        self.edit_stack.reset();
    }

    pub(crate) fn move_to_start(&mut self, select: bool) {
        self.move_head_to(0, select);
    }

    pub(crate) fn move_to_end(&mut self, select: bool) {
        self.move_head_to(self.line_buffer.len(), select);
    }

    /// Place the edit point *past the last grapheme* (at `len`) so the next
    /// insert appends rather than splitting. A block caret rests one grapheme
    /// inward from the end, so a plain insert there lands *before* the final
    /// character — accepting a trailing history hint must append instead. Does
    /// not commit, so the following `InsertString` reads this position.
    pub(crate) fn prepare_append_at_buffer_end(&mut self) {
        self.line_buffer.set_insertion_point(self.line_buffer.len());
    }

    pub(crate) fn move_to_line_start(&mut self, select: bool) {
        self.move_head_to(self.line_buffer.line_start_index(), select);
    }

    pub(crate) fn move_to_line_non_blank_start(&mut self, select: bool) {
        self.move_head_to(self.line_buffer.line_non_blank_start_index(), select);
    }

    pub(crate) fn move_to_line_end(&mut self, select: bool) {
        self.move_head_to(self.line_buffer.find_current_line_end(), select);
    }

    fn undo(&mut self) {
        let val = self.edit_stack.undo();
        self.line_buffer = val.clone();
    }

    fn redo(&mut self) {
        let val = self.edit_stack.redo();
        self.line_buffer = val.clone();
    }

    pub(crate) fn update_undo_state(&mut self, undo_behavior: UndoBehavior) {
        if matches!(undo_behavior, UndoBehavior::NoOp) {
            self.last_undo_behavior = UndoBehavior::NoOp;
            return;
        }
        if !undo_behavior.create_undo_point_after(&self.last_undo_behavior) {
            self.edit_stack.undo();
        }
        self.edit_stack.insert(self.line_buffer.clone());
        self.last_undo_behavior = undo_behavior;
    }

    // The dedicated `*Linewise` cut/copy methods below back the legacy public
    // `EditCommand` variants only — every builtin binding now lowers through
    // `operate` + `Granularity::LineWise` (with the `Change` verb covering the
    // `leave_blank_line` flavor). Linewise span fixes belong in `operate` /
    // `core_editor::line`, not here.

    fn cut_current_line(&mut self) {
        let deletion_range = self.line_buffer.current_line_range();

        let cut_slice = &self.line_buffer.get_buffer()[deletion_range.clone()];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice, Granularity::LineWise);
            self.line_buffer.set_insertion_point(deletion_range.start);
            self.line_buffer.clear_range(deletion_range);
        }
    }

    fn cut_from_start(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        if insertion_offset > 0 {
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[..insertion_offset],
                Granularity::CharWise,
            );
            self.line_buffer.clear_to_insertion_point();
        }
    }

    fn cut_from_start_linewise(&mut self, leave_blank_line: bool) {
        let insertion_offset = self.line_buffer.insertion_point();
        let end_offset = self.line_buffer.get_buffer()[insertion_offset..]
            .find('\n')
            .map_or(self.line_buffer.len(), |offset| {
                // When leave_blank_line is true, we do **not** add 1 to the offset
                // So there will remain an empty line after the operation
                if leave_blank_line {
                    insertion_offset + offset
                } else {
                    insertion_offset + offset + 1
                }
            });
        if end_offset > 0 {
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[..end_offset],
                Granularity::LineWise,
            );
            self.line_buffer.clear_range(..end_offset);
            self.line_buffer.move_to_start();
        }
    }

    fn cut_from_line_start(&mut self) {
        let previous_offset = self.line_buffer.insertion_point();
        self.line_buffer.move_to_line_start();
        let deletion_range = self.line_buffer.insertion_point()..previous_offset;
        let cut_slice = &self.line_buffer.get_buffer()[deletion_range.clone()];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice, Granularity::CharWise);
            self.line_buffer.clear_range(deletion_range);
        }
    }

    fn cut_from_line_non_blank_start(&mut self) {
        let cursor_pos = self.line_buffer.insertion_point();
        self.line_buffer.move_to_line_non_blank_start();
        let other_pos = self.line_buffer.insertion_point();
        let deletion_range = min(cursor_pos, other_pos)..max(cursor_pos, other_pos);
        self.cut_range(deletion_range);
    }

    fn cut_from_end(&mut self) {
        let cut_slice = &self.line_buffer.get_buffer()[self.line_buffer.insertion_point()..];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice, Granularity::CharWise);
            self.line_buffer.clear_to_end();
        }
    }

    fn cut_from_end_linewise(&mut self, leave_blank_line: bool) {
        let buf = self.line_buffer.get_buffer();
        let len = buf.len();
        let nl = buf[..self.line_buffer.insertion_point()].rfind('\n');
        // The register keeps content from the line start only (no leading
        // terminator) so a later linewise paste gains no blank line. The
        // deletion eats the preceding terminator when not leaving a blank line —
        // the whole `\r\n` for CRLF (see `LineBuffer`'s line-ending contract).
        // Same register/delete split as `operate`.
        let register_start = nl.map_or(0, |offset| offset + 1);
        let delete_start = nl.map_or(0, |offset| {
            if leave_blank_line {
                offset + 1
            } else if buf[..offset].ends_with('\r') {
                offset - 1
            } else {
                offset
            }
        });

        if delete_start < len {
            let register_slice = &self.line_buffer.get_buffer()[register_start..];
            if !register_slice.is_empty() {
                self.cut_buffer.set(register_slice, Granularity::LineWise);
            }
            self.line_buffer.set_insertion_point(delete_start);
            self.line_buffer.clear_to_end();
        }
    }

    fn cut_to_line_end(&mut self) {
        let cut_slice = &self.line_buffer.get_buffer()
            [self.line_buffer.insertion_point()..self.line_buffer.find_current_line_end()];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice, Granularity::CharWise);
            self.line_buffer.clear_to_line_end();
        }
    }

    fn kill_line(&mut self) {
        if self.line_buffer.insertion_point() == self.line_buffer.find_current_line_end() {
            self.cut_char()
        } else {
            self.cut_to_line_end()
        }
    }

    fn cut_word_left(&mut self) {
        self.apply_operator(
            word_target(WordKind::Unicode, WordEdge::Start, Direction::Backward),
            OperatorVerb::Cut,
        );
    }

    fn cut_big_word_left(&mut self) {
        self.apply_operator(
            word_target(WordKind::LongWord, WordEdge::Start, Direction::Backward),
            OperatorVerb::Cut,
        );
    }

    fn cut_word_right(&mut self) {
        // emacs `M-d`: consume to the current word's trailing boundary (no skip).
        // Under a bar caret the operator span runs `origin..trailing`, matching
        // the old `insertion_point..word_right_index`.
        self.apply_operator(
            word_target(WordKind::Unicode, WordEdge::End, Direction::Forward),
            OperatorVerb::Cut,
        );
    }

    fn cut_big_word_right(&mut self) {
        self.apply_operator(
            word_target(WordKind::LongWord, WordEdge::End, Direction::Forward),
            OperatorVerb::Cut,
        );
    }

    fn cut_word_right_to_next(&mut self) {
        self.apply_operator(
            word_target(WordKind::Unicode, WordEdge::Start, Direction::Forward),
            OperatorVerb::Cut,
        );
    }

    fn cut_big_word_right_to_next(&mut self) {
        self.apply_operator(
            word_target(WordKind::LongWord, WordEdge::Start, Direction::Forward),
            OperatorVerb::Cut,
        );
    }

    fn cut_char(&mut self) {
        if self.line_buffer.selection_anchor().is_some() {
            self.cut_selection_to_cut_buffer();
        } else {
            let insertion_offset = self.line_buffer.insertion_point();
            let next_char = self.line_buffer.grapheme_right_index();
            self.cut_range(insertion_offset..next_char);
        }
    }

    fn insert_cut_buffer_before(&mut self) {
        self.delete_selection();
        insert_clipboard_content_before(&mut self.line_buffer, self.cut_buffer.deref_mut())
    }

    fn insert_cut_buffer_after(&mut self) {
        // After replacing a selection the cursor already sits at the deletion
        // point, so it must NOT skip a grapheme; only the plain no-selection `p`
        // steps past the grapheme under the cursor before inserting.
        let had_selection = self.line_buffer.selection_anchor().is_some();
        self.delete_selection();
        match self.cut_buffer.get() {
            (content, Granularity::CharWise) => {
                if !had_selection {
                    self.line_buffer.move_right();
                }
                self.line_buffer.insert_str(&content);
            }
            (mut content, Granularity::LineWise) => {
                if !content.ends_with('\n') {
                    content.push('\n');
                }
                let ip = self.line_buffer.insertion_point();
                match line::start_of_next_line(self.line_buffer.get_buffer(), ip) {
                    // A line exists below: insert at its start so the pasted lines
                    // land between current and next — i.e. below the current line.
                    Some(next) => {
                        self.line_buffer.set_insertion_point(next);
                        self.line_buffer.insert_str(&content);
                    }
                    // Last line: no line below, so append after the current line's
                    // terminator. Drop the trailing `\n` so no blank line is added,
                    // otherwise the paste would land *above* (like `P`).
                    None => {
                        let trimmed = content.strip_suffix('\n').unwrap_or(&content);
                        if self.line_buffer.is_empty() {
                            // No current line to append below — insert as-is so an
                            // empty buffer (e.g. after `dd` on the only line) does
                            // not gain a leading blank line.
                            self.line_buffer.insert_str(trimmed);
                        } else {
                            self.line_buffer.set_insertion_point(self.line_buffer.len());
                            self.line_buffer.insert_str(&format!("\n{trimmed}"));
                        }
                    }
                }
            }
        }
    }

    fn move_right_until_char(
        &mut self,
        c: char,
        before_char: bool,
        current_line: bool,
        select: bool,
    ) {
        // Route through `move_head_to` so a selecting search opens the selection
        // via `put_cursor`; the old `update_selection_anchor` + raw `set_head`
        // path dropped the anchor when starting a selection from a point.
        let Some(found) = self.line_buffer.find_char_right(c, current_line) else {
            // Miss: no movement; only settle the selection per `select`.
            if !select {
                self.clear_selection();
            }
            return;
        };
        let target = if before_char {
            self.line_buffer.grapheme_left_index_from_pos(found)
        } else {
            found
        };
        self.move_head_to(target, select);
    }

    fn move_left_until_char(
        &mut self,
        c: char,
        before_char: bool,
        current_line: bool,
        select: bool,
    ) {
        // See `move_right_until_char`.
        let Some(found) = self.line_buffer.find_char_left(c, current_line) else {
            if !select {
                self.clear_selection();
            }
            return;
        };
        let target = if before_char {
            found + c.len_utf8()
        } else {
            found
        };
        self.move_head_to(target, select);
    }

    fn cut_right_until_char(&mut self, c: char, before_char: bool, current_line: bool) {
        if let Some(index) = self.line_buffer.find_char_right(c, current_line) {
            // Saving the section of the string that will be deleted to be
            // stored into the buffer
            let extra = if before_char { 0 } else { c.len_utf8() };
            let cut_slice =
                &self.line_buffer.get_buffer()[self.line_buffer.insertion_point()..index + extra];

            if !cut_slice.is_empty() {
                self.cut_buffer.set(cut_slice, Granularity::CharWise);

                if before_char {
                    self.line_buffer.delete_right_before_char(c, current_line);
                } else {
                    self.line_buffer.delete_right_until_char(c, current_line);
                }
            }
        }
    }

    fn cut_left_until_char(&mut self, c: char, before_char: bool, current_line: bool) {
        if let Some(index) = self.line_buffer.find_char_left(c, current_line) {
            // Saving the section of the string that will be deleted to be
            // stored into the buffer
            let extra = if before_char { c.len_utf8() } else { 0 };
            let cut_slice =
                &self.line_buffer.get_buffer()[index + extra..self.line_buffer.insertion_point()];

            if !cut_slice.is_empty() {
                self.cut_buffer.set(cut_slice, Granularity::CharWise);

                if before_char {
                    self.line_buffer.delete_left_before_char(c, current_line);
                } else {
                    self.line_buffer.delete_left_until_char(c, current_line);
                }
            }
        }
    }

    fn replace_char(&mut self, character: char) {
        // Visual `r`: replace every grapheme in the selection with `character`,
        // preserving line terminators — vim's `r` over a selection.
        if let Some((start, end)) = self.get_selection() {
            use unicode_segmentation::UnicodeSegmentation;
            let replacement: String = self.line_buffer.get_buffer()[start..end]
                .graphemes(true)
                .map(|g| {
                    if g == "\n" || g == "\r\n" || g == "\r" {
                        g.to_string()
                    } else {
                        character.to_string()
                    }
                })
                .collect();
            self.line_buffer.replace_range(start..end, &replacement);
            self.line_buffer.set_cursor(Cursor::point(start));
            return;
        }
        // Anchor the in-place replace on the caret: under a Block/visual cursor
        // head is one grapheme past the caret, so deleting+inserting without
        // collapsing first would clear two graphemes and corrupt the buffer.
        self.line_buffer.collapse_to_caret();
        let insertion_point = self.line_buffer.insertion_point();
        self.line_buffer.delete_right_grapheme();

        self.line_buffer.insert_char(character);
        self.line_buffer.set_insertion_point(insertion_point);
    }

    fn replace_chars(&mut self, n_chars: usize, string: &str) {
        // See `replace_char`: collapse the selection so the deletes start at the
        // caret rather than overshooting from the head.
        self.line_buffer.collapse_to_caret();
        for _ in 0..n_chars {
            self.line_buffer.delete_right_grapheme();
        }

        self.line_buffer.insert_str(string);
    }

    fn move_left(&mut self, select: bool) {
        let head = self.resolve_head(MotionTarget::Grapheme(Direction::Backward));
        self.move_head_to(head, select);
    }

    fn move_right(&mut self, select: bool) {
        let head = self.resolve_head(MotionTarget::Grapheme(Direction::Forward));
        self.move_head_to(head, select);
    }

    fn select_all(&mut self) {
        let end = self.line_buffer.len();
        self.line_buffer.set_cursor(Cursor::new(0, end));
    }

    #[cfg(feature = "system_clipboard")]
    fn cut_selection_to_system(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            let cut_slice = &self.line_buffer.get_buffer()[start..end];
            self.system_clipboard.set(cut_slice, Granularity::CharWise);
            self.cut_range(start..end);
            self.clear_selection();
        }
    }

    fn cut_selection_to_cut_buffer(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            self.cut_range(start..end);
            self.clear_selection();
        }
    }

    #[cfg(feature = "system_clipboard")]
    fn copy_selection_to_system(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            let cut_slice = &self.line_buffer.get_buffer()[start..end];
            self.system_clipboard.set(cut_slice, Granularity::CharWise);
        }
    }

    fn copy_selection_to_cut_buffer(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            let cut_slice = &self.line_buffer.get_buffer()[start..end];
            self.cut_buffer.set(cut_slice, Granularity::CharWise);
        }
    }

    /// If a selection is active returns the selected range, otherwise None.
    /// The range is guaranteed to be ascending.
    pub fn get_selection(&self) -> Option<(usize, usize)> {
        // `None` exactly when the cursor is empty (head == anchor): with the
        // collapsed `Cursor` storage, `selection_anchor()` is derived from
        // `!is_empty()`, so an anchor on the head is simply no selection.
        self.line_buffer.selection_anchor()?;
        let cursor = self.line_buffer.cursor();

        // Inclusivity is carried by the range geometry now: `put_cursor` widens
        // the head for block / Vi-normal selections, so the selected span is just
        // the cursor's range — no captured-inclusivity `+1`.
        Some((cursor.start(), cursor.end().min(self.line_buffer.len())))
    }

    fn delete_selection(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            self.line_buffer.clear_range_safe(start..end);
            self.clear_selection();
        }
    }

    fn backspace(&mut self) {
        if self.line_buffer.selection_anchor().is_some() {
            self.delete_selection();
        } else {
            self.line_buffer.delete_left_grapheme();
        }
    }

    fn delete(&mut self) {
        if self.line_buffer.selection_anchor().is_some() {
            self.delete_selection();
        } else {
            self.line_buffer.delete_right_grapheme();
        }
    }

    fn move_word_left(&mut self, select: bool) {
        self.apply_move(
            word_target(WordKind::Unicode, WordEdge::Start, Direction::Backward),
            select,
        );
    }

    fn move_big_word_left(&mut self, select: bool) {
        self.apply_move(
            word_target(WordKind::LongWord, WordEdge::Start, Direction::Backward),
            select,
        );
    }

    fn move_word_right(&mut self, select: bool) {
        // emacs `M-f`: end of the word the cursor is inside (no skip). Under a bar
        // caret `Word{End}` resolves to the word's *trailing boundary*, which is
        // exactly `word_right_index` — so the verb path now expresses it directly.
        self.apply_move(
            word_target(WordKind::Unicode, WordEdge::End, Direction::Forward),
            select,
        );
    }

    fn move_word_right_start(&mut self, select: bool) {
        self.apply_move(
            word_target(WordKind::Unicode, WordEdge::Start, Direction::Forward),
            select,
        );
    }

    fn move_big_word_right_start(&mut self, select: bool) {
        self.apply_move(
            word_target(WordKind::LongWord, WordEdge::Start, Direction::Forward),
            select,
        );
    }

    fn move_word_right_end(&mut self, select: bool) {
        // vi-`e` lands *on* the word's last grapheme regardless of the active
        // caret, so it resolves the word-end with `inclusive = true` (block
        // reading) rather than the mode's geometry — distinct from emacs `M-f`,
        // which rests on the trailing boundary. (Unbound.)
        self.move_head_to(self.word_end_on_grapheme(WordKind::Unicode), select);
    }

    fn move_big_word_right_end(&mut self, select: bool) {
        // vi-`E` on-char — see `move_word_right_end`.
        self.move_head_to(self.word_end_on_grapheme(WordKind::LongWord), select);
    }

    /// Forward word-end resolved with block (on-grapheme) geometry, whatever the
    /// active caret. Backs the vi-`e`/`E`-style `*RightEnd` commands, whose
    /// landing is the word's last grapheme rather than its trailing boundary —
    /// so it asks the motion resolver for the block reading (`block = true`)
    /// directly instead of the mode's geometry.
    fn word_end_on_grapheme(&self, kind: WordKind) -> usize {
        let target = word_target(kind, WordEdge::End, Direction::Forward);
        resolve_motion(
            self.get_buffer(),
            self.insertion_point(),
            target,
            CaretGeometry::Block,
        )
        .head
    }

    fn insert_char(&mut self, c: char) {
        self.delete_selection();
        self.line_buffer.insert_char(c);
    }

    fn insert_str(&mut self, str: &str) {
        self.delete_selection();
        self.line_buffer.insert_str(str);
    }

    fn insert_newline(&mut self) {
        self.delete_selection();
        self.line_buffer.insert_newline();
    }

    fn insert_newline_above(&mut self) {
        let index = self.line_buffer.find_char_left('\n', false).unwrap_or(0);
        self.line_buffer.set_insertion_point(index);
        self.line_buffer.insert_newline();
    }

    fn insert_newline_below(&mut self) {
        let index = self
            .line_buffer
            .find_char_right('\n', false)
            .unwrap_or(self.line_buffer.len());
        self.line_buffer.set_insertion_point(index);
        self.line_buffer.insert_newline();
    }

    #[cfg(feature = "system_clipboard")]
    fn paste_from_system(&mut self) {
        self.delete_selection();
        insert_clipboard_content_before(&mut self.line_buffer, self.system_clipboard.deref_mut());
    }

    fn paste_cut_buffer(&mut self) {
        self.delete_selection();
        insert_clipboard_content_before(&mut self.line_buffer, self.cut_buffer.deref_mut());
    }

    fn cut_range(&mut self, range: Range<usize>) {
        self.cut_range_with(range, Granularity::CharWise);
    }

    fn cut_range_with(&mut self, range: Range<usize>, granularity: Granularity) {
        if range.start <= range.end {
            self.copy_range_with(range.clone(), granularity);
            self.line_buffer.clear_range_safe(range.clone());
            self.line_buffer.set_insertion_point(range.start);
        }
    }

    fn copy_range(&mut self, range: Range<usize>) {
        self.copy_range_with(range, Granularity::CharWise);
    }

    fn copy_range_with(&mut self, range: Range<usize>, granularity: Granularity) {
        if range.start < range.end {
            let slice = &self.line_buffer.get_buffer()[range];
            self.cut_buffer.set(slice, granularity);
        }
    }

    /// Delete text strictly between matching `open_char` and `close_char`.
    fn cut_inside_pair(&mut self, open_char: char, close_char: char) {
        if let Some(range) = self
            .line_buffer
            .range_inside_current_pair(open_char, close_char)
            .or_else(|| {
                self.line_buffer
                    .range_inside_next_pair(open_char, close_char)
            })
        {
            self.cut_range(range)
        }
    }

    /// Return the range of the word under the cursor.
    /// A word consists of a sequence of letters, digits and underscores,
    /// separated with white space.
    /// A block of whitespace under the cursor is also treated as a word.
    ///
    /// `text_object_scope` Inner includes only the word itself
    /// while Around also includes trailing whitespace,
    /// or preceding whitespace if there is no trailing whitespace.
    fn word_text_object_range(&self, text_object_scope: TextObjectScope) -> Range<usize> {
        self.line_buffer
            .current_whitespace_range()
            .unwrap_or_else(|| {
                let word_range = self.line_buffer.current_word_range();
                match text_object_scope {
                    TextObjectScope::Inner => word_range,
                    TextObjectScope::Around => {
                        self.line_buffer.expand_range_with_whitespace(word_range)
                    }
                }
            })
    }

    /// Return the range of the WORD under the cursor.
    /// A WORD consists of a sequence of non-blank characters, separated with white space.
    /// A block of whitespace under the cursor is also treated as a word.
    ///
    /// `text_object_scope` Inner includes only the word itself
    /// while Around also includes trailing whitespace,
    /// or preceding whitespace if there is no trailing whitespace.
    fn big_word_text_object_range(&self, text_object_scope: TextObjectScope) -> Range<usize> {
        self.line_buffer
            .current_whitespace_range()
            .unwrap_or_else(|| {
                let big_word_range = self.line_buffer.current_big_word_range();
                match text_object_scope {
                    TextObjectScope::Inner => big_word_range,
                    TextObjectScope::Around => self
                        .line_buffer
                        .expand_range_with_whitespace(big_word_range),
                }
            })
    }

    /// Returns `Some(Range<usize>)` for range inside the character pair in `pair_group`
    /// at or surrounding the cursor, the next pair if no pairs in `pair_group`
    /// surround the cursor, or `None` if there are no pairs from `pair_group` found.
    ///
    /// `text_object_scope` [`TextObjectScope::Inner`] includes only the range inside the pair
    /// whereas [`TextObjectScope::Around`] also includes the surrounding pair characters
    ///
    /// If multiple pair types exist, returns the innermost pair that surrounds
    /// the cursor. Handles empty pair as zero-length ranges inside pair.
    /// For asymmetric pairs like `(` `)` the search is multi-line, however,
    /// for symmetric pairs like `"` `"` the search is restricted to the current line.
    fn matching_pair_group_text_object_range(
        &self,
        text_object_scope: TextObjectScope,
        matching_pair_group: &[(char, char)],
    ) -> Option<Range<usize>> {
        self.line_buffer
            .range_inside_current_pair_in_group(matching_pair_group)
            .or_else(|| {
                self.line_buffer
                    .range_inside_next_pair_in_group(matching_pair_group)
            })
            .and_then(|pair_range| match text_object_scope {
                TextObjectScope::Inner => Some(pair_range),
                TextObjectScope::Around => self.expand_range_to_include_pair(pair_range),
            })
    }

    /// Returns `Some(Range<usize>)` for range inside brackets (`()`, `[]`, `{}`)
    /// at or surrounding the cursor, the next pair of brackets if no brackets
    /// surround the cursor, or `None` if there are no brackets found.
    ///
    /// `text_object_scope` [`TextObjectScope::Inner`] includes only the range inside the pair
    /// whereas [`TextObjectScope::Around`] also includes the surrounding pair characters
    ///
    /// If multiple bracket types exist, returns the innermost pair that surrounds
    /// the cursor. Handles empty brackets as zero-length ranges inside brackets.
    /// Includes brackets that span multiple lines.
    fn bracket_text_object_range(
        &self,
        text_object_scope: TextObjectScope,
    ) -> Option<Range<usize>> {
        const BRACKET_PAIRS: &[(char, char)] = &[('(', ')'), ('[', ']'), ('{', '}')];
        self.matching_pair_group_text_object_range(text_object_scope, BRACKET_PAIRS)
    }

    /// Returns `Some(Range<usize>)` for the range inside quotes (`""`, `''` or `\`\`\`)
    /// at the cursor, the next pair of quotes if the cursor is not within quotes,
    /// or `None` if there are no quotes found.
    ///
    /// Quotes are restricted to the current line.
    ///
    /// `text_object_scope` [`TextObjectScope::Inner`] includes only the range inside the pair
    /// whereas [`TextObjectScope::Around`] also includes the surrounding pair characters
    ///
    /// If multiple quote types exist, returns the innermost pair that surrounds
    /// the cursor. Handles empty quotes as zero-length ranges inside quote.
    fn quote_text_object_range(&self, text_object_scope: TextObjectScope) -> Option<Range<usize>> {
        const QUOTE_PAIRS: &[(char, char)] = &[('"', '"'), ('\'', '\''), ('`', '`')];
        self.matching_pair_group_text_object_range(text_object_scope, QUOTE_PAIRS)
    }

    /// Get the bounds for a text object operation
    fn text_object_range(&self, text_object: TextObject) -> Option<Range<usize>> {
        match text_object.object_type {
            TextObjectType::Word => Some(self.word_text_object_range(text_object.scope)),
            TextObjectType::BigWord => Some(self.big_word_text_object_range(text_object.scope)),
            TextObjectType::Brackets => self.bracket_text_object_range(text_object.scope),
            TextObjectType::Quote => self.quote_text_object_range(text_object.scope),
        }
    }

    fn cut_text_object(&mut self, text_object: TextObject) {
        if let Some(range) = self.text_object_range(text_object) {
            self.cut_range(range);
        }
    }

    fn copy_text_object(&mut self, text_object: TextObject) {
        if let Some(range) = self.text_object_range(text_object) {
            self.copy_range(range);
        }
    }

    pub(crate) fn copy_from_start(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        if insertion_offset > 0 {
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[..insertion_offset],
                Granularity::CharWise,
            );
        }
    }

    pub(crate) fn copy_from_start_linewise(&mut self) {
        let insertion_point = self.line_buffer.insertion_point();
        let end_offset = self.line_buffer.get_buffer()[insertion_point..]
            .find('\n')
            .map_or(self.line_buffer.len(), |offset| insertion_point + offset);
        if end_offset > 0 {
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[..end_offset],
                Granularity::LineWise,
            );
        }
        self.line_buffer.move_to_start();
    }

    pub(crate) fn copy_from_line_start(&mut self) {
        let previous_offset = self.line_buffer.insertion_point();
        let start_offset = {
            let temp_pos = self.line_buffer.insertion_point();
            self.line_buffer.move_to_line_start();
            let start = self.line_buffer.insertion_point();
            self.line_buffer.set_insertion_point(temp_pos);
            start
        };
        let copy_range = start_offset..previous_offset;
        self.copy_range(copy_range);
    }

    pub(crate) fn copy_from_line_non_blank_start(&mut self) {
        let cursor_pos = self.line_buffer.insertion_point();
        self.line_buffer.move_to_line_non_blank_start();
        let other_pos = self.line_buffer.insertion_point();
        self.line_buffer.set_insertion_point(cursor_pos);
        let copy_range = min(cursor_pos, other_pos)..max(cursor_pos, other_pos);
        self.copy_range(copy_range);
    }

    pub(crate) fn copy_from_end(&mut self) {
        let copy_range = self.line_buffer.insertion_point()..self.line_buffer.len();
        self.copy_range(copy_range);
    }

    pub(crate) fn copy_from_end_linewise(&mut self) {
        self.line_buffer.move_to_line_start();
        let copy_range = self.line_buffer.insertion_point()..self.line_buffer.len();
        if copy_range.start < copy_range.end {
            let slice = &self.line_buffer.get_buffer()[copy_range];
            self.cut_buffer.set(slice, Granularity::LineWise);
        }
    }

    pub(crate) fn copy_to_line_end(&mut self) {
        let copy_range =
            self.line_buffer.insertion_point()..self.line_buffer.find_current_line_end();
        self.copy_range(copy_range);
    }

    pub(crate) fn copy_word_left(&mut self) {
        self.apply_operator(
            word_target(WordKind::Unicode, WordEdge::Start, Direction::Backward),
            OperatorVerb::Copy,
        );
    }

    pub(crate) fn copy_big_word_left(&mut self) {
        self.apply_operator(
            word_target(WordKind::LongWord, WordEdge::Start, Direction::Backward),
            OperatorVerb::Copy,
        );
    }

    pub(crate) fn copy_word_right(&mut self) {
        // emacs forward-word end (no skip) — mirrors `cut_word_right`.
        self.apply_operator(
            word_target(WordKind::Unicode, WordEdge::End, Direction::Forward),
            OperatorVerb::Copy,
        );
    }

    pub(crate) fn copy_big_word_right(&mut self) {
        self.apply_operator(
            word_target(WordKind::LongWord, WordEdge::End, Direction::Forward),
            OperatorVerb::Copy,
        );
    }

    pub(crate) fn copy_word_right_to_next(&mut self) {
        self.apply_operator(
            word_target(WordKind::Unicode, WordEdge::Start, Direction::Forward),
            OperatorVerb::Copy,
        );
    }

    pub(crate) fn copy_big_word_right_to_next(&mut self) {
        self.apply_operator(
            word_target(WordKind::LongWord, WordEdge::Start, Direction::Forward),
            OperatorVerb::Copy,
        );
    }

    pub(crate) fn copy_right_until_char(&mut self, c: char, before_char: bool, current_line: bool) {
        if let Some(index) = self.line_buffer.find_char_right(c, current_line) {
            let extra = if before_char { 0 } else { c.len_utf8() };
            let copy_range = self.line_buffer.insertion_point()..index + extra;
            self.copy_range(copy_range);
        }
    }

    pub(crate) fn copy_left_until_char(&mut self, c: char, before_char: bool, current_line: bool) {
        if let Some(index) = self.line_buffer.find_char_left(c, current_line) {
            let extra = if before_char { c.len_utf8() } else { 0 };
            let copy_range = index + extra..self.line_buffer.insertion_point();
            self.copy_range(copy_range);
        }
    }

    /// Copy text strictly between matching `open_char` and `close_char`.
    fn copy_inside_pair(&mut self, open_char: char, close_char: char) {
        if let Some(range) = self
            .line_buffer
            .range_inside_current_pair(open_char, close_char)
            .or_else(|| {
                self.line_buffer
                    .range_inside_next_pair(open_char, close_char)
            })
        {
            self.copy_range(range);
        }
    }

    /// Expand the range to include `open_char` and `close_char`
    fn expand_range_to_include_pair(&self, range: Range<usize>) -> Option<Range<usize>> {
        let start = self.line_buffer.grapheme_left_index_from_pos(range.start);
        let end = self.line_buffer.grapheme_right_index_from_pos(range.end);

        Some(start..end)
    }

    /// Delete text around matching `open_char` and `close_char` (including the pair characters).
    fn cut_around_pair(&mut self, open_char: char, close_char: char) {
        if let Some(around_range) = self
            .line_buffer
            .range_inside_current_pair(open_char, close_char)
            .or_else(|| {
                self.line_buffer
                    .range_inside_next_pair(open_char, close_char)
            })
            .and_then(|range| self.expand_range_to_include_pair(range))
        {
            self.cut_range(around_range);
        }
    }

    /// Copy text around matching `open_char` and `close_char` (including the pair characters).
    fn copy_around_pair(&mut self, open_char: char, close_char: char) {
        if let Some(around_range) = self
            .line_buffer
            .range_inside_current_pair(open_char, close_char)
            .or_else(|| {
                self.line_buffer
                    .range_inside_next_pair(open_char, close_char)
            })
            .and_then(|range| self.expand_range_to_include_pair(range))
        {
            self.copy_range(around_range);
        }
    }
}

fn insert_clipboard_content_before(line_buffer: &mut LineBuffer, clipboard: &mut dyn Clipboard) {
    match clipboard.get() {
        (content, Granularity::CharWise) => {
            line_buffer.insert_str(&content);
        }
        (mut content, Granularity::LineWise) => {
            // TODO: Simplify that?
            line_buffer.move_to_line_start();
            line_buffer.move_line_up();
            if !content.ends_with('\n') {
                // TODO: Make sure platform requirements are met
                content.push('\n');
            }
            line_buffer.insert_str(&content);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prompt::PromptViMode;
    use crate::{Direction, FindStop, WordEdge, WordKind};
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn editor_with(buffer: &str) -> Editor {
        let mut editor = Editor::default();
        editor.set_buffer(buffer.to_string(), UndoBehavior::CreateUndoPoint);
        editor
    }

    fn vi_editor(buffer: &str, vi_mode: PromptViMode) -> Editor {
        let mut editor = editor_with(buffer);
        editor.set_edit_mode(PromptEditMode::Vi(vi_mode));
        editor
    }

    // The Vi-normal cursor invariant ("cursor never rests past the last
    // grapheme") is enforced by the `RestPolicy` commit boundary in
    // `run_edit_command`, not by a per-command clamp. These cover the
    // behavioural scenarios from nushell/reedline#1069 by driving real
    // `EditCommand`s through that boundary.

    #[test]
    fn vi_normal_clamps_cursor_off_the_end() {
        let mut editor = vi_editor("hello", PromptViMode::Normal);
        editor.run_edit_command(&EditCommand::MoveToEnd { select: false });
        // rests on the last grapheme 'o' (byte 4), not past it (byte 5)
        assert_eq!(editor.insertion_point(), 4);
    }

    #[test]
    fn vi_normal_clamps_to_line_end() {
        let mut editor = vi_editor("hello", PromptViMode::Normal);
        editor.run_edit_command(&EditCommand::MoveToLineEnd { select: false });
        assert_eq!(editor.insertion_point(), 4);
    }

    #[test]
    fn vi_insert_does_not_clamp_off_the_end() {
        let mut editor = vi_editor("hello", PromptViMode::Insert);
        editor.run_edit_command(&EditCommand::MoveToEnd { select: false });
        // insert mode's caret may sit past the last grapheme
        assert_eq!(editor.insertion_point(), 5);
    }

    #[test]
    fn vi_normal_empty_buffer_stays_at_zero() {
        let mut editor = vi_editor("", PromptViMode::Normal);
        editor.run_edit_command(&EditCommand::MoveToEnd { select: false });
        assert_eq!(editor.insertion_point(), 0);
    }

    #[test]
    fn vi_normal_within_bounds_is_unchanged() {
        let mut editor = vi_editor("hello", PromptViMode::Normal);
        editor.run_edit_command(&EditCommand::MoveToPosition {
            position: 2,
            select: false,
        });
        assert_eq!(editor.insertion_point(), 2);
    }

    #[test]
    fn vi_normal_clamps_onto_multibyte_grapheme() {
        let mut editor = vi_editor("café", PromptViMode::Normal);
        editor.run_edit_command(&EditCommand::MoveToEnd { select: false });
        // 'é' is 2 bytes, so the last grapheme starts at byte 3, not 4
        assert_eq!(editor.insertion_point(), "caf".len());
    }

    // ======================================================================
    // FLIP SAFETY NET — gates the cursor-as-truth flip (storage follows Helix)
    //
    // INVARIANT masters (`net_*`): pin `insertion_point()` / `get_selection()`
    // values the storage swap must preserve byte-for-byte. These MUST stay
    // green through the flip — they are the proof the swap was faithful.
    // (#694/#893 and the inclusive-cut cases are already pinned by the tests
    // above; these cover the gaps: Between-mode resting, no-anchor/backward
    // selection, and the bare-block-vs-deliberate-selection distinction.)
    // ======================================================================

    #[rstest]
    #[case(PromptViMode::Insert, "hello", 5)] // Between: caret may rest at len
    #[case(PromptViMode::Normal, "hello", 4)] // OnGrapheme: onto the last grapheme
    #[case(PromptViMode::Insert, "café", 5)] // multibyte, insert rests at len
    #[case(PromptViMode::Normal, "café", 3)] // multibyte, normal on last grapheme
    #[case(PromptViMode::Insert, "", 0)]
    #[case(PromptViMode::Normal, "", 0)]
    fn net_insertion_point_at_end(
        #[case] mode: PromptViMode,
        #[case] buf: &str,
        #[case] expect: usize,
    ) {
        let mut editor = vi_editor(buf, mode);
        editor.run_edit_command(&EditCommand::MoveToEnd { select: false });
        assert_eq!(editor.insertion_point(), expect);
    }

    #[test]
    fn net_insertion_point_emacs_rests_at_len() {
        // Default/Emacs is `Between`: the caret may sit past the last grapheme.
        let mut editor = editor_with("hello");
        editor.run_edit_command(&EditCommand::MoveToEnd { select: false });
        assert_eq!(editor.insertion_point(), 5);
    }

    #[test]
    fn net_get_selection_none_without_anchor() {
        // A bare cursor (no anchor planted) is not a selection.
        let editor = vi_editor("hello", PromptViMode::Normal);
        assert_eq!(editor.get_selection(), None);
    }

    #[test]
    fn net_get_selection_backward_is_ordered() {
        let mut editor = vi_editor("hello", PromptViMode::Normal);
        editor.move_to_position(3, false);
        editor.run_edit_command(&EditCommand::MoveLeft { select: true });
        editor.run_edit_command(&EditCommand::MoveLeft { select: true });
        // head left of anchor; get_selection returns an ordered (start, end).
        assert_eq!(editor.get_selection(), Some((1, 4)));
    }

    // BEHAVIOR(E): we follow helix — a bare cursor and a 1-grapheme selection
    // render the SAME (the 1-wide block IS the cursor), so we deliberately do
    // NOT distinguish them. The editor-level invariant kept here is only that a
    // deliberate selection reports a range. "A bare cursor is not highlighted"
    // is a *painter* invariant (helix rule: highlight = range minus the 1-wide
    // cursor cell), pinned when we touch the render side of the flip.
    //
    // NOTE: the exact range value is BEHAVIOR the flip may shift — the inclusive
    // `+1` goes away with `selection_inclusive`. The stable selection invariants
    // are the cut-result tests above (buffer + cut content), not raw ranges.
    #[test]
    fn net_deliberate_selection_reports_a_range() {
        let mut editor = vi_editor("hello", PromptViMode::Normal);
        editor.move_to_position(1, false);
        editor.run_edit_command(&EditCommand::MoveRight { select: true });
        assert_eq!(editor.get_selection(), Some((1, 3)));
    }

    // Esc-from-insert is lowered (in the Vi machine) to a backward grapheme
    // step, and the engine relays the new rest policy via `sync_edit_mode`
    // *before* that step runs. These replicate that seam sequence — insert
    // caret, `sync_edit_mode` (no commit), then the step — to pin the timing:
    // the step must read `OnGrapheme`, must not double-step a caret sitting at
    // the line end, and must not cross the line under the cell-caret policy.

    /// Helper: caret in insert at `at`, then the Esc seam (policy relayed
    /// without committing) followed by the backward grapheme step.
    ///
    /// Pins the line-clamped path (`cross_line_cursor = false`): these tests
    /// observe the seam timing through the at-line-edge behavior, which is only
    /// stable when the cell caret can't cross the newline. Cross-line wrapping
    /// (now the default) is covered by its own tests.
    fn esc_back_from_insert(buffer: &str, at: usize) -> Editor {
        let mut editor = vi_editor(buffer, PromptViMode::Insert);
        editor.set_cross_line_cursor(false);
        editor.line_buffer.set_insertion_point(at);
        editor.sync_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));
        editor.run_edit_command(&EditCommand::Move(MotionTarget::Grapheme(
            Direction::Backward,
        )));
        editor
    }

    #[test]
    fn esc_back_steps_one_within_line() {
        // caret on the last 'c' (as after `i`): steps back onto the first 'c'
        let editor = esc_back_from_insert("aa bb cc", 7);
        assert_eq!(editor.insertion_point(), 6);
    }

    #[test]
    fn esc_back_at_line_end_does_not_double_step() {
        // caret appended past the end (as after `A`): the relay must NOT commit
        // and pull it back, or the step would land on 6 instead of the last 'c'
        let editor = esc_back_from_insert("aa bb cc", 8);
        assert_eq!(editor.insertion_point(), 7);
    }

    #[test]
    fn esc_back_at_line_start_stays_in_line() {
        // caret at column 0 of the second line: the cell-caret can't cross the
        // newline, so it stays put rather than jumping onto the line above
        let editor = esc_back_from_insert("ab\ncd", 3);
        assert_eq!(editor.insertion_point(), 3);
    }

    #[test]
    fn esc_back_on_trailing_empty_line_stays() {
        // the `cc`/`S`-then-Esc shape: caret on a blank last line stays there
        let editor = esc_back_from_insert("a\n\n", 3);
        assert_eq!(editor.insertion_point(), 3);
    }

    // The commit boundary also fires on the two state changes that bypass
    // `run_edit_command`: buffer replacement (history navigation) and edit-mode
    // transitions (e.g. Esc into Vi normal). Both were clamped by #1069 too.

    #[test]
    fn vi_normal_set_buffer_clamps_cursor() {
        // history navigation replaces the buffer (cursor lands at the end)
        let mut editor = vi_editor("", PromptViMode::Normal);
        editor.set_buffer("hello".to_string(), UndoBehavior::CreateUndoPoint);
        assert_eq!(editor.insertion_point(), 4);
    }

    #[test]
    fn vi_normal_set_buffer_clamps_multibyte() {
        let mut editor = vi_editor("", PromptViMode::Normal);
        editor.set_buffer("café".to_string(), UndoBehavior::CreateUndoPoint);
        assert_eq!(editor.insertion_point(), "caf".len());
    }

    #[test]
    fn entering_vi_normal_clamps_cursor() {
        // simulates Esc: the cursor sits past the end in insert mode, then the
        // mode flips to normal and the commit-on-mode-change pulls it back
        let mut editor = vi_editor("hello", PromptViMode::Insert);
        editor.run_edit_command(&EditCommand::MoveToEnd { select: false });
        assert_eq!(editor.insertion_point(), 5);
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));
        assert_eq!(editor.insertion_point(), 4);
    }

    #[test]
    fn entering_vi_insert_does_not_move_cursor() {
        let mut editor = vi_editor("hello", PromptViMode::Normal);
        editor.run_edit_command(&EditCommand::MoveToPosition {
            position: 4,
            select: false,
        });
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Insert));
        assert_eq!(editor.insertion_point(), 4);
    }

    // Selections built by selecting motions still cut the right bytes after the
    // commit boundary runs on every move — including across a multibyte grapheme.

    #[test]
    fn vi_normal_selection_cut_is_inclusive() {
        let mut editor = vi_editor("hello", PromptViMode::Normal);
        editor.run_edit_command(&EditCommand::MoveToPosition {
            position: 0,
            select: false,
        });
        for _ in 0..2 {
            editor.run_edit_command(&EditCommand::MoveRight { select: true });
        }
        // head on 'l' (byte 2); Vi-normal selection is inclusive → covers [0,3)
        assert_eq!(editor.get_selection(), Some((0, 3)));
        editor.run_edit_command(&EditCommand::CutSelection);
        assert_eq!(editor.get_buffer(), "lo");
        assert_eq!(editor.cut_buffer.get().0, "hel");
    }

    #[test]
    fn vi_normal_selection_cut_spans_multibyte_grapheme() {
        let mut editor = vi_editor("caféx", PromptViMode::Normal);
        editor.run_edit_command(&EditCommand::MoveToPosition {
            position: 0,
            select: false,
        });
        for _ in 0..3 {
            editor.run_edit_command(&EditCommand::MoveRight { select: true });
        }
        // head on 'é' (byte 3); inclusive end extends over both bytes of é → 5
        assert_eq!(editor.get_selection(), Some((0, 5)));
        editor.run_edit_command(&EditCommand::CutSelection);
        assert_eq!(editor.get_buffer(), "x");
        assert_eq!(editor.cut_buffer.get().0, "café");
    }

    // Regression for #893: a single selecting move must extend the selection by
    // exactly one grapheme, not two. The default (exclusive) policy means the
    // selection end is the head — the cursor-as-range model has no place for the
    // off-by-one that produced the two-char grab.
    #[test]
    fn shift_select_grabs_one_grapheme_per_step() {
        let mut editor = editor_with("hello");
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::MoveRight { select: true });
        assert_eq!(editor.get_selection(), Some((0, 1)));
        editor.run_edit_command(&EditCommand::MoveRight { select: true });
        assert_eq!(editor.get_selection(), Some((0, 2)));
    }

    #[test]
    fn shift_select_one_grapheme_over_multibyte() {
        let mut editor = editor_with("café"); // 'é' is 2 bytes at [3,5)
        editor.move_to_position(3, false);
        editor.run_edit_command(&EditCommand::MoveRight { select: true });
        assert_eq!(editor.get_selection(), Some((3, 5))); // one grapheme, not two
    }

    #[test]
    fn select_all_captures_inclusivity_at_plant_time() {
        // `select_all` plants its anchor outside the motion path; it must still
        // capture inclusivity, so a later mode switch (vi normal → insert here)
        // can't shrink the selection by the final grapheme.
        let mut editor = vi_editor("hello", PromptViMode::Normal);
        editor.run_edit_command(&EditCommand::SelectAll);
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Insert));
        assert_eq!(editor.get_selection(), Some((0, 5)));
    }

    // --- granularity gate -------------------------------------------------
    //
    // dd/dgg/dG/yy (and the cgg/cG blank-line variant) currently lower to
    // dedicated linewise commands. The granularity axis will re-lower them
    // through the operator verbs; these golden masters pin the exact buffer,
    // cursor, cut content, and — crucially — the `Granularity::LineWise` register
    // tag (what makes paste linewise) so the re-lowering stays behavior-preserving.
    // Buffer "aaa\nbbb\nccc": a@0..3, \n@3, b@4..7, \n@7, c@8..11; cursor in "bbb".

    fn linewise_editor() -> Editor {
        let mut editor = editor_with("aaa\nbbb\nccc");
        editor.move_to_position(5, false);
        editor
    }

    #[test]
    fn cut_current_line_is_linewise() {
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::CutCurrentLine);
        assert_eq!(editor.get_buffer(), "aaa\nccc");
        assert_eq!(editor.insertion_point(), 4);
        let (content, mode) = editor.cut_buffer.get();
        assert_eq!(content, "bbb\n");
        assert!(matches!(mode, Granularity::LineWise));
    }

    // --- explicit-granularity target (Phase 2 step 3 makes these pass) ----
    //
    // The new vocab: `dd` = `Cut(LineEdge, LineWise)`, `dgg` = `Cut(BufferEdge(Bwd),
    // LineWise)`, `dG` = `Cut(BufferEdge(Fwd), LineWise)`. `operate` must snap a
    // LineWise span out to whole lines (incl. the `dG` leading-\n fixup) and tag
    // the register `LineWise`. These mirror the dedicated-command golden masters
    // above. (`operate` ignores granularity until step 3, so they start red.)

    #[test]
    fn cut_lineedge_linewise_matches_current_line() {
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::Cut {
            target: MotionTarget::LineEdge(Direction::Forward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "aaa\nccc");
        assert_eq!(editor.insertion_point(), 4);
        let (content, gran) = editor.cut_buffer.get();
        assert_eq!(content, "bbb\n");
        assert_eq!(gran, Granularity::LineWise);
    }

    #[test]
    fn cut_bufferedge_back_linewise_cuts_through_current_line() {
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::Cut {
            target: MotionTarget::BufferEdge(Direction::Backward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "ccc");
        assert_eq!(editor.insertion_point(), 0);
        let (content, gran) = editor.cut_buffer.get();
        assert_eq!(content, "aaa\nbbb\n");
        assert_eq!(gran, Granularity::LineWise);
    }

    #[test]
    fn cut_bufferedge_fwd_linewise_eats_leading_newline() {
        // the `dG` fixup: reaching buffer end consumes the *preceding* \n so no
        // stray blank line is left.
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::Cut {
            target: MotionTarget::BufferEdge(Direction::Forward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "aaa");
        assert_eq!(editor.insertion_point(), 3);
        let (content, gran) = editor.cut_buffer.get();
        // The buffer-end fixup eats the *preceding* `\n` from the deletion only;
        // the register keeps content (no leading `\n`) so paste stays blank-safe.
        assert_eq!(content, "bbb\nccc");
        assert_eq!(gran, Granularity::LineWise);
    }

    #[test]
    fn copy_lineedge_linewise_tags_register_nondestructively() {
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::Copy {
            target: MotionTarget::LineEdge(Direction::Forward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "aaa\nbbb\nccc"); // unchanged
        let (content, gran) = editor.cut_buffer.get();
        assert_eq!(content, "bbb\n");
        assert_eq!(gran, Granularity::LineWise);
    }

    #[test]
    fn cut_lineedge_charwise_stays_charwise() {
        // CharWise must NOT snap: `d$` from mid-line cuts to the line end only.
        let mut editor = linewise_editor(); // cursor 5, inside "bbb"
        editor.run_edit_command(&EditCommand::Cut {
            target: MotionTarget::LineEdge(Direction::Forward),
            granularity: Granularity::CharWise,
        });
        assert_eq!(editor.get_buffer(), "aaa\nb\nccc"); // removed "bb"
        let (content, gran) = editor.cut_buffer.get();
        assert_eq!(content, "bb");
        assert_eq!(gran, Granularity::CharWise);
    }

    #[test]
    fn cut_line_down_linewise_deletes_current_and_next() {
        // `dj` from "bbb" deletes bbb + ccc, linewise.
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::Cut {
            target: MotionTarget::Line(Direction::Forward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "aaa");
        let (content, gran) = editor.cut_buffer.get();
        // Register keeps the line content only — no leading `\n` — so a linewise
        // paste does not gain a spurious blank line.
        assert_eq!(content, "bbb\nccc");
        assert_eq!(gran, Granularity::LineWise);
    }

    #[test]
    fn cut_line_up_linewise_deletes_current_and_prev() {
        // `dk` from "bbb" deletes aaa + bbb, linewise.
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::Cut {
            target: MotionTarget::Line(Direction::Backward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "ccc");
        assert_eq!(editor.insertion_point(), 0);
        let (content, gran) = editor.cut_buffer.get();
        assert_eq!(content, "aaa\nbbb\n");
        assert_eq!(gran, Granularity::LineWise);
    }

    #[test]
    fn cut_line_down_on_last_line_cuts_only_that_line() {
        // `dj` on the last line: the motion stays put (no line below), so the
        // linewise snap consumes just the current line — including its
        // *leading* `\n` (the buffer-end fixup), leaving no stray blank line.
        let mut editor = editor_with("aaa\nbbb\nccc");
        editor.move_to_position(9, false); // inside "ccc"
        editor.run_edit_command(&EditCommand::Cut {
            target: MotionTarget::Line(Direction::Forward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "aaa\nbbb");
        let (content, gran) = editor.cut_buffer.get();
        // Register keeps content only; the leading `\n` is eaten from the
        // deletion alone, keeping a later linewise paste blank-safe.
        assert_eq!(content, "ccc");
        assert_eq!(gran, Granularity::LineWise);
    }

    #[test]
    fn dd_on_last_line_then_paste_leaves_no_blank_line() {
        // Regression: a linewise cut of the last line stored the deletion span
        // (with its eaten leading `\n`) in the register, so a later linewise
        // paste re-introduced that newline as a spurious blank line.
        let mut editor = vi_editor("ab\ncd", PromptViMode::Normal);
        editor.line_buffer.set_insertion_point(3); // on "cd"
        editor.run_edit_command(&EditCommand::Cut {
            target: MotionTarget::LineEdge(Direction::Forward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "ab");
        assert_eq!(editor.cut_buffer.get().0, "cd"); // content only
        editor.run_edit_command(&EditCommand::PasteCutBufferBefore);
        assert_eq!(editor.get_buffer(), "cd\nab"); // no leading blank line
    }

    #[test]
    fn word_operator_never_splits_a_combining_grapheme() {
        // Regression: NFD "aé" = 'a' + 'e' + U+0301 (combining acute). The
        // combining mark classifies differently, so the word-start boundary lands
        // mid-grapheme; `dw` must floor to a grapheme boundary rather than cut
        // mid-cluster and strand the combining mark at the buffer start.
        let mut editor = vi_editor("ae\u{0301}", PromptViMode::Normal);
        editor.line_buffer.set_insertion_point(0);
        editor.run_edit_command(&EditCommand::Cut {
            target: MotionTarget::Word {
                kind: WordKind::Word,
                edge: WordEdge::Start,
                direction: Direction::Forward,
            },
            granularity: Granularity::CharWise,
        });
        let buf = editor.get_buffer();
        assert!(
            !buf.starts_with('\u{0301}'),
            "word operator orphaned a combining mark: {buf:?}"
        );
    }

    #[test]
    fn paste_after_over_selection_does_not_skip_a_grapheme() {
        // Regression: paste-after replaced the selection then `move_right`,
        // skipping the first remaining grapheme, so the register landed one
        // grapheme too late ("hello" + select "hel" + register "xyz" → "lxyzo").
        let mut editor = vi_editor("hello", PromptViMode::Normal);
        editor.cut_buffer.set("xyz", Granularity::CharWise);
        editor.run_edit_command(&EditCommand::MoveToPosition {
            position: 0,
            select: false,
        });
        for _ in 0..2 {
            editor.run_edit_command(&EditCommand::MoveRight { select: true });
        }
        assert_eq!(editor.get_selection(), Some((0, 3))); // "hel"
        editor.run_edit_command(&EditCommand::PasteCutBufferAfter);
        assert_eq!(editor.get_buffer(), "xyzlo");
    }

    #[test]
    fn paste_after_linewise_on_last_line_lands_below() {
        // Regression: `p` on the last line fell back to the line start (no line
        // below), pasting *above* like `P`.
        let mut editor = editor_with("ab");
        editor.cut_buffer.set("cd", Granularity::LineWise);
        editor.line_buffer.set_insertion_point(0);
        editor.run_edit_command(&EditCommand::PasteCutBufferAfter);
        assert_eq!(editor.get_buffer(), "ab\ncd"); // below, not "cd\nab"
    }

    #[test]
    fn paste_after_linewise_into_empty_buffer_has_no_blank_line() {
        // Regression (introduced by the last-line paste fix): `dd` on the only
        // line empties the buffer, and `p` must not prepend a blank line.
        let mut editor = editor_with("");
        editor.cut_buffer.set("ab", Granularity::LineWise);
        editor.run_edit_command(&EditCommand::PasteCutBufferAfter);
        assert_eq!(editor.get_buffer(), "ab");
    }

    #[test]
    fn paste_after_linewise_middle_line_lands_below() {
        let mut editor = editor_with("a\nb");
        editor.cut_buffer.set("X", Granularity::LineWise);
        editor.line_buffer.set_insertion_point(0); // on line "a"
        editor.run_edit_command(&EditCommand::PasteCutBufferAfter);
        assert_eq!(editor.get_buffer(), "a\nX\nb");
    }

    #[test]
    fn visual_replace_char_replaces_whole_selection() {
        let mut editor = vi_editor("hello", PromptViMode::Normal);
        editor.run_edit_command(&EditCommand::MoveToPosition {
            position: 0,
            select: false,
        });
        for _ in 0..2 {
            editor.run_edit_command(&EditCommand::MoveRight { select: true });
        }
        assert_eq!(editor.get_selection(), Some((0, 3))); // "hel"
        editor.run_edit_command(&EditCommand::ReplaceChar('x'));
        assert_eq!(editor.get_buffer(), "xxxlo");
    }

    #[test]
    fn cut_line_up_on_first_line_cuts_only_that_line() {
        // `dk` on the first line: no line above, so only the current line goes.
        let mut editor = editor_with("aaa\nbbb\nccc");
        editor.move_to_position(1, false);
        editor.run_edit_command(&EditCommand::Cut {
            target: MotionTarget::Line(Direction::Backward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "bbb\nccc");
        assert_eq!(editor.insertion_point(), 0);
        let (content, gran) = editor.cut_buffer.get();
        assert_eq!(content, "aaa\n");
        assert_eq!(gran, Granularity::LineWise);
    }

    #[test]
    fn cut_line_down_on_single_line_buffer_empties_it() {
        let mut editor = editor_with("aaa");
        editor.move_to_position(1, false);
        editor.run_edit_command(&EditCommand::Cut {
            target: MotionTarget::Line(Direction::Forward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "");
        let (content, gran) = editor.cut_buffer.get();
        assert_eq!(content, "aaa");
        assert_eq!(gran, Granularity::LineWise);
    }

    // --- the Change verb (vi linewise change: `cc`/`cj`/`cgg`/`cG`) ---------
    //
    // Change is Cut with the LineWise snap keeping the line terminators: the
    // spanned lines' *content* is consumed and one blank line remains for the
    // re-entered insert mode. The register is tagged LineWise like vim's.

    #[test]
    fn change_lineedge_linewise_blanks_current_line() {
        // `cc` from "bbb": content gone, blank line kept, cursor at its start.
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::Change {
            target: MotionTarget::LineEdge(Direction::Forward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "aaa\n\nccc");
        assert_eq!(editor.insertion_point(), 4);
        let (content, gran) = editor.cut_buffer.get();
        assert_eq!(content, "bbb");
        assert_eq!(gran, Granularity::LineWise);
    }

    #[test]
    fn change_line_down_blanks_current_and_next() {
        // `cj` from "bbb": bbb + ccc collapse into one blank line.
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::Change {
            target: MotionTarget::Line(Direction::Forward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "aaa\n");
        assert_eq!(editor.insertion_point(), 4);
        let (content, gran) = editor.cut_buffer.get();
        assert_eq!(content, "bbb\nccc");
        assert_eq!(gran, Granularity::LineWise);
    }

    #[test]
    fn change_line_up_blanks_current_and_prev() {
        // `ck` from "bbb": aaa + bbb collapse into one blank line.
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::Change {
            target: MotionTarget::Line(Direction::Backward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "\nccc");
        assert_eq!(editor.insertion_point(), 0);
        let (content, gran) = editor.cut_buffer.get();
        assert_eq!(content, "aaa\nbbb");
        assert_eq!(gran, Granularity::LineWise);
    }

    #[test]
    fn change_bufferedge_back_matches_legacy_leave_blank_command() {
        // `cgg` — must reproduce `CutFromStartLinewise { leave_blank_line: true }`
        // (the golden master above) exactly.
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::Change {
            target: MotionTarget::BufferEdge(Direction::Backward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "\nccc");
        assert_eq!(editor.insertion_point(), 0);
        let (content, gran) = editor.cut_buffer.get();
        assert_eq!(content, "aaa\nbbb");
        assert_eq!(gran, Granularity::LineWise);
    }

    #[test]
    fn change_bufferedge_fwd_matches_legacy_leave_blank_command() {
        // `cG` — must reproduce `CutToEndLinewise { leave_blank_line: true }`:
        // no buffer-end fixup; the preceding `\n` stays so a blank line remains.
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::Change {
            target: MotionTarget::BufferEdge(Direction::Forward),
            granularity: Granularity::LineWise,
        });
        assert_eq!(editor.get_buffer(), "aaa\n");
        assert_eq!(editor.insertion_point(), 4);
        let (content, gran) = editor.cut_buffer.get();
        assert_eq!(content, "bbb\nccc");
        assert_eq!(gran, Granularity::LineWise);
    }

    #[test]
    fn change_charwise_behaves_like_cut() {
        // For CharWise spans Change and Cut are the same operator.
        let mut editor = linewise_editor(); // cursor 5, inside "bbb"
        editor.run_edit_command(&EditCommand::Change {
            target: MotionTarget::LineEdge(Direction::Forward),
            granularity: Granularity::CharWise,
        });
        assert_eq!(editor.get_buffer(), "aaa\nb\nccc"); // removed "bb"
        let (content, gran) = editor.cut_buffer.get();
        assert_eq!(content, "bb");
        assert_eq!(gran, Granularity::CharWise);
    }

    #[test]
    fn cut_from_start_linewise_cuts_through_current_line() {
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::CutFromStartLinewise {
            leave_blank_line: false,
        });
        assert_eq!(editor.get_buffer(), "ccc");
        assert_eq!(editor.insertion_point(), 0);
        let (content, mode) = editor.cut_buffer.get();
        assert_eq!(content, "aaa\nbbb\n");
        assert!(matches!(mode, Granularity::LineWise));
    }

    #[test]
    fn cut_from_start_linewise_leave_blank_keeps_empty_line() {
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::CutFromStartLinewise {
            leave_blank_line: true,
        });
        assert_eq!(editor.get_buffer(), "\nccc");
        assert_eq!(editor.insertion_point(), 0);
        let (content, mode) = editor.cut_buffer.get();
        assert_eq!(content, "aaa\nbbb");
        assert!(matches!(mode, Granularity::LineWise));
    }

    #[test]
    fn cut_to_end_linewise_cuts_from_current_line() {
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::CutToEndLinewise {
            leave_blank_line: false,
        });
        assert_eq!(editor.get_buffer(), "aaa");
        assert_eq!(editor.insertion_point(), 3);
        let (content, mode) = editor.cut_buffer.get();
        // Register holds content only (no leading `\n`); the deletion alone eats
        // the preceding terminator, so a later linewise paste stays blank-safe.
        assert_eq!(content, "bbb\nccc");
        assert!(matches!(mode, Granularity::LineWise));
    }

    #[test]
    fn copy_current_line_is_linewise_and_nondestructive() {
        let mut editor = linewise_editor();
        editor.run_edit_command(&EditCommand::CopyCurrentLine);
        assert_eq!(editor.get_buffer(), "aaa\nbbb\nccc"); // unchanged
        let (content, mode) = editor.cut_buffer.get();
        assert_eq!(content, "bbb\n");
        assert!(matches!(mode, Granularity::LineWise));
    }

    #[rstest]
    #[case("abc def ghi", 11, "abc def ")]
    #[case("abc def-ghi", 11, "abc def-")]
    #[case("abc def.ghi", 11, "abc ")]
    fn test_cut_word_left(#[case] input: &str, #[case] position: usize, #[case] expected: &str) {
        let mut editor = editor_with(input);
        editor.line_buffer.set_insertion_point(position);

        editor.cut_word_left();

        assert_eq!(editor.get_buffer(), expected);
    }

    #[rstest]
    #[case("abc def ghi", 11, "abc def ")]
    #[case("abc def-ghi", 11, "abc ")]
    #[case("abc def.ghi", 11, "abc ")]
    #[case("abc def gh ", 11, "abc def ")]
    fn test_cut_big_word_left(
        #[case] input: &str,
        #[case] position: usize,
        #[case] expected: &str,
    ) {
        let mut editor = editor_with(input);
        editor.line_buffer.set_insertion_point(position);

        editor.cut_big_word_left();

        assert_eq!(editor.get_buffer(), expected);
    }

    #[rstest]
    #[case("hello world", 0, 'l', 1, false, "lo world")]
    #[case("hello world", 0, 'l', 1, true, "llo world")]
    #[ignore = "Deleting two consecutive chars is not implemented correctly and needs the multiplier explicitly."]
    #[case("hello world", 0, 'l', 2, false, "o world")]
    #[case("hello world", 0, 'h', 1, false, "hello world")]
    #[case("hello world", 0, 'l', 3, true, "ld")]
    #[case("hello world", 4, 'o', 1, true, "hellorld")]
    #[case("hello world", 4, 'w', 1, false, "hellorld")]
    #[case("hello world", 4, 'o', 1, false, "hellrld")]
    fn test_cut_right_until_char(
        #[case] input: &str,
        #[case] position: usize,
        #[case] search_char: char,
        #[case] repeat: usize,
        #[case] before_char: bool,
        #[case] expected: &str,
    ) {
        let mut editor = editor_with(input);
        editor.line_buffer.set_insertion_point(position);
        for _ in 0..repeat {
            editor.cut_right_until_char(search_char, before_char, true);
        }
        assert_eq!(editor.get_buffer(), expected);
    }

    #[rstest]
    #[case("abc", 1, 'X', "aXc")]
    #[case("abc", 1, '🔄', "a🔄c")]
    #[case("a🔄c", 1, 'X', "aXc")]
    #[case("a🔄c", 1, '🔀', "a🔀c")]
    fn test_replace_char(
        #[case] input: &str,
        #[case] position: usize,
        #[case] replacement: char,
        #[case] expected: &str,
    ) {
        let mut editor = editor_with(input);
        editor.line_buffer.set_insertion_point(position);

        editor.replace_char(replacement);

        assert_eq!(editor.get_buffer(), expected);
    }

    #[test]
    fn visual_replace_char_replaces_only_the_caret_grapheme() {
        // Regression: under a Block/visual cursor head sits one grapheme past
        // the caret. `replace_char` paired the caret-based insertion point with
        // a head-based delete, clearing two graphemes ("hello" -> "lxlo").
        // Collapsing to the caret first keeps it a single-grapheme replace.
        let mut editor = vi_editor("hello", PromptViMode::Visual);
        editor.run_edit_command(&EditCommand::MoveToStart { select: false });
        editor.update_selection_anchor(true); // Block covers 'h': caret 0, head 1
        editor.replace_char('x');
        assert_eq!(editor.get_buffer(), "xello");
    }

    #[test]
    fn visual_move_line_does_not_panic_across_line_boundary() {
        // Regression: `move_line_*` measured the grapheme column from the caret
        // while `current_line_range` used the head; a selection straddling a
        // line boundary made `range.start > caret` and panicked on the slice.
        let mut editor = vi_editor("a\nbc", PromptViMode::Visual);
        editor.run_edit_command(&EditCommand::MoveToStart { select: false });
        editor.update_selection_anchor(true);
        // Drive vertical moves with the selection active — must not panic.
        editor.run_edit_command(&EditCommand::MoveLineDown { select: true });
        editor.run_edit_command(&EditCommand::MoveLineDown { select: true });
        editor.run_edit_command(&EditCommand::MoveLineUp { select: true });
    }

    #[test]
    fn linewise_cut_last_line_eats_whole_crlf_terminator() {
        // Regression (#10): cutting the last line must consume the whole
        // *preceding* terminator. On a CRLF buffer — reachable via a recalled
        // Windows history entry or `EditCommand::InsertString` — stepping back a
        // single byte left an orphan `\r` ("ab\r\ncd" + linewise cut → "ab\r").
        let mut editor = editor_with("ab\r\ncd");
        editor.operate(
            Cursor::point(5), // on 'd', the last line
            OperatorVerb::Cut,
            Granularity::LineWise,
        );
        assert_eq!(editor.get_buffer(), "ab");

        // The lone-LF case is unchanged.
        let mut editor = editor_with("ab\ncd");
        editor.operate(Cursor::point(4), OperatorVerb::Cut, Granularity::LineWise);
        assert_eq!(editor.get_buffer(), "ab");
    }

    #[test]
    fn cut_to_end_linewise_eats_whole_crlf_terminator() {
        // Sibling of #10 on the CutToEndLinewise path (a public EditCommand):
        // stepping back to the `\n` would orphan the `\r` of a CRLF terminator.
        let mut editor = editor_with("x\r\ncd");
        editor.line_buffer.set_insertion_point(3); // on 'c', the second line
        editor.cut_from_end_linewise(false);
        assert_eq!(editor.get_buffer(), "x");
        assert!(!editor.get_buffer().contains('\r'));
    }

    fn selected_text(editor: &Editor) -> String {
        let c = editor.line_buffer().cursor();
        editor.get_buffer()[c.start()..c.end()].to_string()
    }

    #[test]
    fn visual_move_word_right_end_select_covers_last_grapheme() {
        // #9: a selecting word-end motion must cover the word's last grapheme
        // (inclusive block geometry), not stop one grapheme short.
        let mut editor = vi_editor("foo bar", PromptViMode::Visual);
        editor.run_edit_command(&EditCommand::MoveToStart { select: false });
        editor.run_edit_command(&EditCommand::MoveWordRightEnd { select: true });
        assert_eq!(selected_text(&editor), "foo");
    }

    #[test]
    fn visual_move_to_line_start_after_end_keeps_anchor_grapheme() {
        // #12: extend from 'd' to the line end, then back to the line start. The
        // grapheme the selection started on ('d') must stay covered on reversal
        // (vim keeps "abc d"), which needs the put_cursor anchor-flip.
        let mut editor = vi_editor("abc def", PromptViMode::Visual);
        editor.run_edit_command(&EditCommand::MoveToPosition {
            position: 4, // on 'd'
            select: false,
        });
        editor.run_edit_command(&EditCommand::MoveToLineEnd { select: true });
        editor.run_edit_command(&EditCommand::MoveToLineStart { select: true });
        assert_eq!(selected_text(&editor), "abc d");
    }

    #[test]
    fn visual_line_jk_keeps_anchor_grapheme_covered() {
        // #13: vertical visual motion must keep the grapheme the selection
        // started on covered, even across a direction reversal. "x\ny\nz",
        // select 'y' (byte 2), then j/k/k. The old raw `set_head` path dropped
        // the anchor; routing through put_cursor keeps it.
        let mut editor = vi_editor("x\ny\nz", PromptViMode::Visual);
        editor.run_edit_command(&EditCommand::MoveToPosition {
            position: 2, // 'y'
            select: false,
        });
        editor.update_selection_anchor(true);
        editor.run_edit_command(&EditCommand::MoveLineDown { select: true });
        editor.run_edit_command(&EditCommand::MoveLineUp { select: true });
        editor.run_edit_command(&EditCommand::MoveLineUp { select: true });

        let c = editor.line_buffer().cursor();
        assert!(
            c.start() <= 2 && 2 < c.end(),
            "byte 2 ('y', the anchor) must stay covered; selection was {:?}",
            c.start()..c.end()
        );
    }

    #[test]
    fn visual_line_jk_preserves_caret_column() {
        // Starting visual at a word end (caret one grapheme before the trailing
        // space), j then k must return the caret to its column — the column is
        // the caret's, not the head's, which under a Block cursor sits one
        // grapheme further on (onto the space) and drifts the selection by a
        // grapheme on every vertical move. "ab cd\nef gh": 'b' is byte 1.
        let mut editor = vi_editor("ab cd\nef gh", PromptViMode::Visual);
        editor.run_edit_command(&EditCommand::MoveToPosition {
            position: 1, // 'b', the end of "ab"
            select: false,
        });
        editor.update_selection_anchor(true);
        for _ in 0..2 {
            editor.run_edit_command(&EditCommand::MoveLineDown { select: true });
            editor.run_edit_command(&EditCommand::MoveLineUp { select: true });
            assert_eq!(
                editor.insertion_point(),
                1,
                "caret drifted off 'b' after a j/k round-trip"
            );
        }
    }

    #[test]
    fn select_until_char_in_bar_mode_opens_selection() {
        // Regression: `MoveRightUntil { select: true }` from a point in a
        // bar-caret mode (emacs / vi-insert) must open a selection. The old
        // `update_selection_anchor` + raw `set_head` path anchored on a point
        // (empty cursor) and the move then collapsed it, dropping the anchor.
        let mut editor = editor_with("This is a test!"); // default = Between (bar)
        editor.line_buffer.set_insertion_point(0);
        editor.run_edit_command(&EditCommand::MoveRightUntil {
            c: 's',
            select: true,
        });
        // 's' is byte 3; a bar selection is exclusive, covering bytes [0, 3).
        assert_eq!(editor.get_selection(), Some((0, 3)));

        // The backward form likewise opens a selection.
        let mut editor = editor_with("This is a test!");
        editor
            .line_buffer
            .set_insertion_point(editor.line_buffer.len());
        editor.run_edit_command(&EditCommand::MoveLeftUntil {
            c: 'T',
            select: true,
        });
        assert!(editor.get_selection().is_some());
    }

    fn str_to_edit_commands(s: &str) -> Vec<EditCommand> {
        s.chars().map(EditCommand::InsertChar).collect()
    }

    #[test]
    fn test_undo_insert_works_on_work_boundaries() {
        let mut editor = editor_with("This is  a");
        for cmd in str_to_edit_commands(" test") {
            editor.run_edit_command(&cmd);
        }
        assert_eq!(editor.get_buffer(), "This is  a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is  a");
        editor.run_edit_command(&EditCommand::Redo);
        assert_eq!(editor.get_buffer(), "This is  a test");
    }

    #[test]
    fn test_undo_backspace_works_on_word_boundaries() {
        let mut editor = editor_with("This is  a test");
        for _ in 0..6 {
            editor.run_edit_command(&EditCommand::Backspace);
        }
        assert_eq!(editor.get_buffer(), "This is  ");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is  a");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is  a test");
    }

    #[test]
    fn test_undo_delete_works_on_word_boundaries() {
        let mut editor = editor_with("This  is a test");
        editor.line_buffer.set_insertion_point(0);
        for _ in 0..7 {
            editor.run_edit_command(&EditCommand::Delete);
        }
        assert_eq!(editor.get_buffer(), "s a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "is a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This  is a test");
    }

    #[test]
    fn test_undo_insert_with_newline() {
        let mut editor = editor_with("This is a");
        for cmd in str_to_edit_commands(" \n test") {
            editor.run_edit_command(&cmd);
        }
        assert_eq!(editor.get_buffer(), "This is a \n test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a \n");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a");
    }

    #[test]
    fn test_undo_backspace_with_newline() {
        let mut editor = editor_with("This is a \n test");
        for _ in 0..8 {
            editor.run_edit_command(&EditCommand::Backspace);
        }
        assert_eq!(editor.get_buffer(), "This is ");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a \n");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a \n test");
    }

    #[test]
    fn test_undo_backspace_with_crlf() {
        let mut editor = editor_with("This is a \r\n test");
        for _ in 0..8 {
            editor.run_edit_command(&EditCommand::Backspace);
        }
        assert_eq!(editor.get_buffer(), "This is ");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a \r\n");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a \r\n test");
    }

    #[test]
    fn test_undo_delete_with_newline() {
        let mut editor = editor_with("This \n is a test");
        editor.line_buffer.set_insertion_point(0);
        for _ in 0..8 {
            editor.run_edit_command(&EditCommand::Delete);
        }
        assert_eq!(editor.get_buffer(), "s a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "is a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "\n is a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This \n is a test");
    }

    #[test]
    fn test_undo_delete_with_crlf() {
        // CLRF delete is a special case, since the first character of the
        // grapheme is \r rather than \n
        let mut editor = editor_with("This \r\n is a test");
        editor.line_buffer.set_insertion_point(0);
        for _ in 0..8 {
            editor.run_edit_command(&EditCommand::Delete);
        }
        assert_eq!(editor.get_buffer(), "s a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "is a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "\r\n is a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This \r\n is a test");
    }

    #[test]
    fn test_swap_cursor_and_anchor() {
        let mut editor = editor_with("This is some test content");
        editor.line_buffer.set_insertion_point(0);
        editor.update_selection_anchor(true);

        for _ in 0..3 {
            editor.run_edit_command(&EditCommand::MoveRight { select: true });
        }
        assert_eq!(editor.line_buffer().selection_anchor(), Some(0));
        assert_eq!(editor.insertion_point(), 3);
        assert_eq!(editor.get_selection(), Some((0, 3)));

        editor.run_edit_command(&EditCommand::SwapCursorAndAnchor);
        assert_eq!(editor.line_buffer().selection_anchor(), Some(3));
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.get_selection(), Some((0, 3)));

        editor.run_edit_command(&EditCommand::SwapCursorAndAnchor);
        assert_eq!(editor.line_buffer().selection_anchor(), Some(0));
        assert_eq!(editor.insertion_point(), 3);
        assert_eq!(editor.get_selection(), Some((0, 3)));
    }

    /// Drive a single block-caret grapheme motion from `start` in vi normal mode
    /// and return the resulting insertion point.
    #[cfg(test)]
    fn normal_mode_step(buf: &str, start: usize, cross: bool, cmd: &EditCommand) -> usize {
        let mut e = editor_with(buf);
        e.set_cross_line_cursor(cross);
        e.line_buffer.set_insertion_point(start);
        e.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));
        e.run_edit_command(cmd);
        e.insertion_point()
    }

    #[test]
    fn cross_line_cursor_on_crosses_newline() {
        let r = &EditCommand::MoveRight { select: false };
        let l = &EditCommand::MoveLeft { select: false };
        // "ab\ncd": l at end of line 1 ('b'=1) lands on line 2's first char ('c'=3);
        // h at line 2's start ('c'=3) lands on line 1's last char ('b'=1).
        assert_eq!(normal_mode_step("ab\ncd", 1, true, r), 3);
        assert_eq!(normal_mode_step("ab\ncd", 3, true, l), 1);
        // `\r\n` is one grapheme: crossing skips the whole terminator.
        // "ab\r\ncd": 'b'=1, 'c'=4.
        assert_eq!(normal_mode_step("ab\r\ncd", 1, true, r), 4);
        assert_eq!(normal_mode_step("ab\r\ncd", 4, true, l), 1);
    }

    #[test]
    fn cross_line_cursor_off_clamps_to_line() {
        let r = &EditCommand::MoveRight { select: false };
        let l = &EditCommand::MoveLeft { select: false };
        // Opt-out (vim default): the motion stops at the line edge instead of
        // crossing — `l` from 'b' does not reach line 2's 'c' (3), `h` from 'c'
        // does not reach line 1's 'b' (1).
        assert_ne!(normal_mode_step("ab\ncd", 1, false, r), 3);
        assert_ne!(normal_mode_step("ab\ncd", 3, false, l), 1);
    }

    #[test]
    fn append_at_buffer_end_appends_past_block_caret() {
        // Regression: accepting a history hint in vi normal mode must append
        // *after* the last char. The block caret rests on the last grapheme, so
        // a plain insert would split it ("abc" + "def" -> "abdefc").
        let mut editor = editor_with("abc");
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));
        editor.run_edit_command(&EditCommand::MoveToLineEnd { select: false });
        editor.prepare_append_at_buffer_end();
        editor.run_edit_command(&EditCommand::InsertString("def".into()));
        assert_eq!(editor.get_buffer(), "abcdef");
        // Multibyte last grapheme must not be split either.
        let mut editor = editor_with("caf\u{e9}");
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));
        editor.run_edit_command(&EditCommand::MoveToLineEnd { select: false });
        editor.prepare_append_at_buffer_end();
        editor.run_edit_command(&EditCommand::InsertString("X".into()));
        assert_eq!(editor.get_buffer(), "caf\u{e9}X");
    }

    #[test]
    fn cursor_at_buffer_end_holds_on_last_grapheme_in_normal_mode() {
        // Regression: in vi normal mode the resting cursor sits *on* the last
        // grapheme (OnGrapheme pulls the head back), so `caret()` is one inward
        // from `len`. "At buffer end" must still hold there, or a history hint
        // never completes in normal mode (it did before the cursor refactor).
        let mut editor = editor_with("abc");
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));
        editor.run_edit_command(&EditCommand::MoveToLineEnd { select: false });
        assert!(editor.is_cursor_at_buffer_end());
        // Multibyte: resting on `é` (a 2-byte grapheme) must report end too.
        let mut editor = editor_with("caf\u{e9}");
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));
        editor.run_edit_command(&EditCommand::MoveToLineEnd { select: false });
        assert!(editor.is_cursor_at_buffer_end());
        // Not at the end: resting on the first char of a multi-char buffer.
        let mut editor = editor_with("abc");
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));
        editor.run_edit_command(&EditCommand::MoveToLineStart { select: false });
        assert!(!editor.is_cursor_at_buffer_end());
        // An active selection reaching the end is NOT a clean end point: a hint
        // completing here would delete the selection. (vi visual extending to len.)
        let mut editor = editor_with("abc");
        editor.line_buffer.set_insertion_point(0);
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));
        editor.update_selection_anchor(true);
        for _ in 0..3 {
            editor.run_edit_command(&EditCommand::MoveRight { select: true });
        }
        assert!(!editor.is_cursor_at_buffer_end());
    }

    #[test]
    fn test_vi_normal_mode_inclusive_selection() {
        let mut editor = editor_with("This is some test content");
        editor.line_buffer.set_insertion_point(0);
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));
        editor.update_selection_anchor(true);

        for _ in 0..3 {
            editor.run_edit_command(&EditCommand::MoveRight { select: true });
        }
        assert_eq!(editor.line_buffer().selection_anchor(), Some(0));
        assert_eq!(editor.insertion_point(), 3);
        // In Vi normal mode, selection should be inclusive (include character at position 3)
        assert_eq!(editor.get_selection(), Some((0, 4)));
    }

    #[test]
    fn test_vi_normal_mode_inclusive_selection_backward() {
        let mut editor = editor_with("This is some test content");
        editor.line_buffer.set_insertion_point(4); // Start at position 4 ('i')
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));
        editor.update_selection_anchor(true);

        for _ in 0..3 {
            editor.run_edit_command(&EditCommand::MoveLeft { select: true });
        }
        // Inclusivity is geometric now: the anchor flips onto the far edge of
        // its grapheme (4 → 5) so the char at 4 stays covered, instead of an
        // anchor-stays-at-4 + query-time `+1`. The selected span is unchanged.
        assert_eq!(editor.line_buffer().selection_anchor(), Some(5));
        assert_eq!(editor.insertion_point(), 1); // cursor at position 1 ('h')
                                                 // In Vi normal mode, selection should be inclusive from cursor to anchor+1
                                                 // So it should select from position 1 to 5 (inclusive of char at position 4)
        assert_eq!(editor.get_selection(), Some((1, 5)));
    }

    #[test]
    fn test_vi_normal_mode_cut_selection_backward() {
        let mut editor = editor_with("This is some test content");

        editor.line_buffer.set_insertion_point(4); // Start at position 4 (' ')
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));
        editor.update_selection_anchor(true);

        for _ in 0..3 {
            editor.run_edit_command(&EditCommand::MoveLeft { select: true });
        }

        // Should select "his " (from position 1 to 5, inclusive of char at position 4)
        assert_eq!(editor.get_selection(), Some((1, 5)));

        editor.run_edit_command(&EditCommand::CutSelection);

        // After cutting, should have "Tis some test content" (removed "his ")
        assert_eq!(editor.get_buffer(), "Tis some test content");
        assert_eq!(editor.insertion_point(), 1); // cursor should be at start of cut
    }

    #[test]
    fn test_vi_visual_mode_c_command() {
        // Test the exact scenario: select in visual mode, then press 'c'
        let mut editor = editor_with("hello world");
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));

        // Start at position 0, enter visual mode by selecting
        editor.line_buffer.set_insertion_point(0);
        editor.update_selection_anchor(true);

        // Move right 4 characters to select "hello" (from pos 0 to pos 4)
        for _ in 0..4 {
            editor.run_edit_command(&EditCommand::MoveRight { select: true });
        }

        // In vi normal mode, this should be inclusive selection
        // So we should select "hello" (positions 0-4, inclusive of position 4)
        assert_eq!(editor.get_selection(), Some((0, 5))); // should include character at position 4

        // Now simulate pressing 'c' - this should cut the selection
        editor.run_edit_command(&EditCommand::CutSelection);

        // Should have " world" left (removed "hello")
        assert_eq!(editor.get_buffer(), " world");
        assert_eq!(editor.insertion_point(), 0);
    }

    #[test]
    fn test_vi_normal_mode_c_command_with_selection() {
        // Test the exact issue: c command in vi normal mode with selection
        let mut editor = editor_with("hello world");
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));

        // Start at position 0, create selection by moving cursor
        editor.line_buffer.set_insertion_point(0);
        editor.update_selection_anchor(true);

        // Move right to select "hello" (positions 0-4, should be inclusive of pos 4)
        for _ in 0..4 {
            editor.run_edit_command(&EditCommand::MoveRight { select: true });
        }

        // In vi normal mode, selection should include character at cursor position
        assert_eq!(editor.get_selection(), Some((0, 5))); // inclusive selection

        // Now simulate pressing 'c' - this should cut the selection
        editor.run_edit_command(&EditCommand::CutSelection);

        // Should have " world" left (removed "hello" including the 'o')
        assert_eq!(editor.get_buffer(), " world");
        assert_eq!(editor.insertion_point(), 0);
    }

    #[cfg(feature = "system_clipboard")]
    mod without_system_clipboard {
        use super::*;
        #[test]
        fn test_cut_selection_system() {
            let mut editor = editor_with("This is a test!");
            // Build the whole-buffer selection head-first: under the unified
            // Cursor model, anchoring where the head already sits makes an empty
            // cursor that the next head move would collapse, so move the head to
            // the start first, then drop the anchor at the end.
            editor.line_buffer.set_insertion_point(0);
            editor
                .line_buffer
                .set_selection_anchor(Some(editor.line_buffer.len()));
            editor.run_edit_command(&EditCommand::CutSelectionSystem);
            assert!(editor.line_buffer.get_buffer().is_empty());
        }
        #[test]
        fn test_copypaste_selection_system() {
            let s = "This is a test!";
            let mut editor = editor_with(s);
            // Head-first selection build; see `test_cut_selection_system`.
            editor.line_buffer.set_insertion_point(0);
            editor
                .line_buffer
                .set_selection_anchor(Some(editor.line_buffer.len()));
            editor.run_edit_command(&EditCommand::CopySelectionSystem);
            editor.run_edit_command(&EditCommand::PasteSystem);
            pretty_assertions::assert_eq!(editor.line_buffer.len(), s.len() * 2);
        }
    }

    #[test]
    fn test_cut_inside_brackets() {
        let mut editor = editor_with("foo(bar)baz");
        editor.move_to_position(5, false); // Move inside brackets
        editor.cut_inside_pair('(', ')');
        assert_eq!(editor.get_buffer(), "foo()baz");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "bar");

        // Test with cursor outside brackets
        let mut editor = editor_with("foo(bar)baz");
        editor.move_to_position(0, false);
        editor.cut_inside_pair('(', ')');
        assert_eq!(editor.get_buffer(), "foo()baz");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "bar");

        // Test with no matching brackets
        let mut editor = editor_with("foo bar baz");
        editor.move_to_position(4, false);
        editor.cut_inside_pair('(', ')');
        assert_eq!(editor.get_buffer(), "foo bar baz");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "");
    }

    #[test]
    fn test_cut_inside_quotes() {
        let mut editor = editor_with("foo\"bar\"baz");
        editor.move_to_position(5, false); // Move inside quotes
        editor.cut_inside_pair('"', '"');
        assert_eq!(editor.get_buffer(), "foo\"\"baz");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "bar");

        // Test with cursor outside quotes
        let mut editor = editor_with("foo\"bar\"baz");
        editor.move_to_position(0, false);
        editor.cut_inside_pair('"', '"');
        assert_eq!(editor.get_buffer(), "foo\"\"baz");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "bar");

        // Test with no matching quotes
        let mut editor = editor_with("foo bar baz");
        editor.move_to_position(4, false);
        editor.cut_inside_pair('"', '"');
        assert_eq!(editor.get_buffer(), "foo bar baz");
        assert_eq!(editor.insertion_point(), 4);
    }

    #[test]
    fn test_cut_inside_nested() {
        let mut editor = editor_with("foo(bar(baz)qux)quux");
        editor.move_to_position(8, false); // Move inside inner brackets
        editor.cut_inside_pair('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar()qux)quux");
        assert_eq!(editor.insertion_point(), 8);
        assert_eq!(editor.cut_buffer.get().0, "baz");

        editor.move_to_position(4, false); // Move inside outer brackets
        editor.cut_inside_pair('(', ')');
        assert_eq!(editor.get_buffer(), "foo()quux");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "bar()qux");
    }

    #[test]
    fn test_yank_inside_brackets() {
        let mut editor = editor_with("foo(bar)baz");
        editor.move_to_position(5, false); // Move inside brackets
        editor.copy_inside_pair('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar)baz"); // Buffer shouldn't change
        assert_eq!(editor.insertion_point(), 5); // Cursor should return to original position

        // Test yanked content by pasting
        editor.paste_cut_buffer();
        assert_eq!(editor.get_buffer(), "foo(bbarar)baz");

        // Test with cursor outside brackets
        let mut editor = editor_with("foo(bar)baz");
        editor.move_to_position(0, false);
        editor.copy_inside_pair('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar)baz");
        assert_eq!(editor.insertion_point(), 0);
    }

    #[test]
    fn test_yank_inside_quotes() {
        let mut editor = editor_with("foo\"bar\"baz");
        editor.move_to_position(5, false); // Move inside quotes
        editor.copy_inside_pair('"', '"');
        assert_eq!(editor.get_buffer(), "foo\"bar\"baz"); // Buffer shouldn't change
        assert_eq!(editor.insertion_point(), 5); // Cursor should return to original position
        assert_eq!(editor.cut_buffer.get().0, "bar");

        // Test with no matching quotes
        let mut editor = editor_with("foo bar baz");
        editor.move_to_position(4, false);
        editor.copy_inside_pair('"', '"');
        assert_eq!(editor.get_buffer(), "foo bar baz");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "");
    }

    #[test]
    fn test_yank_inside_nested() {
        let mut editor = editor_with("foo(bar(baz)qux)quux");
        editor.move_to_position(8, false); // Move inside inner brackets
        editor.copy_inside_pair('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar(baz)qux)quux"); // Buffer shouldn't change
        assert_eq!(editor.insertion_point(), 8);
        assert_eq!(editor.cut_buffer.get().0, "baz");

        // Test yanked content by pasting
        editor.paste_cut_buffer();
        assert_eq!(editor.get_buffer(), "foo(bar(bazbaz)qux)quux");

        editor.move_to_position(4, false); // Move inside outer brackets
        editor.copy_inside_pair('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar(bazbaz)qux)quux");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "bar(bazbaz)qux");
    }

    #[test]
    fn test_kill_line() {
        let mut editor = editor_with("foo\nbar");
        editor.move_to_position(1, false);
        editor.kill_line();
        assert_eq!(editor.get_buffer(), "f\nbar"); // Just cut until the end of line
        assert_eq!(editor.insertion_point(), 1); // Cursor should return to original position
        assert_eq!(editor.cut_buffer.get().0, "oo");
        // continue kill line at current position.
        editor.kill_line();
        assert_eq!(editor.get_buffer(), "fbar"); // Just cut the new line character
        assert_eq!(editor.insertion_point(), 1);
        assert_eq!(editor.cut_buffer.get().0, "\n");

        // Test when editor start with newline character point.
        let mut editor = editor_with("foo\nbar");
        editor.move_to_position(3, false);
        editor.kill_line();
        assert_eq!(editor.get_buffer(), "foobar"); // Just cut the new line character
        assert_eq!(editor.insertion_point(), 3); // Cursor should return to original position
        assert_eq!(editor.cut_buffer.get().0, "\n");
        // continue kill line at current position.
        editor.kill_line();
        assert_eq!(editor.get_buffer(), "foo"); // Just cut until line end.
        assert_eq!(editor.insertion_point(), 3);
        assert_eq!(editor.cut_buffer.get().0, "bar");
        // continue kill line, all remains the same.
        editor.kill_line();
        assert_eq!(editor.get_buffer(), "foo");
        assert_eq!(editor.insertion_point(), 3);
        assert_eq!(editor.cut_buffer.get().0, "bar");
    }

    #[test]
    fn test_kill_line_with_windows_newline() {
        let mut editor = editor_with("foo\r\nbar");
        editor.move_to_position(1, false);
        editor.kill_line();
        assert_eq!(editor.get_buffer(), "f\r\nbar"); // Just cut until the end of line
        assert_eq!(editor.insertion_point(), 1); // Cursor should return to original position
        assert_eq!(editor.cut_buffer.get().0, "oo");
        // continue kill line at current position.
        editor.kill_line();
        assert_eq!(editor.get_buffer(), "fbar"); // Just cut the new line character
        assert_eq!(editor.insertion_point(), 1);
        assert_eq!(editor.cut_buffer.get().0, "\r\n");

        let mut editor = editor_with("foo\r\nbar");
        editor.move_to_position(3, false);
        editor.kill_line();
        assert_eq!(editor.get_buffer(), "foobar"); // Just cut the newline
        assert_eq!(editor.insertion_point(), 3); // Cursor should return to original position
        assert_eq!(editor.cut_buffer.get().0, "\r\n");
    }

    #[test]
    fn test_vi_normal_mode_shift_select_right_c_command() {
        // Test vi normal mode inclusive selection with cut operation
        let mut editor = editor_with("hello world");
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));

        editor.line_buffer.set_insertion_point(0);
        editor.update_selection_anchor(true);

        for _ in 0..4 {
            editor.run_edit_command(&EditCommand::MoveRight { select: true });
        }

        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.line_buffer().selection_anchor(), Some(0));
        assert_eq!(editor.get_selection(), Some((0, 5))); // inclusive selection

        editor.run_edit_command(&EditCommand::CutSelection);

        assert_eq!(editor.get_buffer(), " world");
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.cut_buffer.get().0, "hello");
    }

    #[test]
    fn test_vi_mode_selection_calculation_bug() {
        // Test selection calculation preserves original mode after mode switch
        let mut editor = editor_with("hello world");
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));

        editor.line_buffer.set_insertion_point(0);
        editor.update_selection_anchor(true);

        for _ in 0..4 {
            editor.run_edit_command(&EditCommand::MoveRight { select: true });
        }

        assert_eq!(editor.get_selection(), Some((0, 5))); // inclusive in normal mode

        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Insert));

        assert_eq!(editor.get_selection(), Some((0, 5))); // still inclusive after mode switch
    }

    #[test]
    fn test_vi_c_command_mode_switch_bug_fix() {
        // Test vi 'c' command selection behavior with mode switching
        let mut editor = editor_with("hello world");
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));

        editor.line_buffer.set_insertion_point(0);
        editor.update_selection_anchor(true);

        for _ in 0..4 {
            editor.run_edit_command(&EditCommand::MoveRight { select: true });
        }

        assert_eq!(editor.get_selection(), Some((0, 5))); // inclusive selection

        // Simulate vi 'c' command: mode switches to insert then cuts selection
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Insert));
        editor.run_edit_command(&EditCommand::CutSelection);

        assert_eq!(editor.get_buffer(), " world");
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.cut_buffer.get().0, "hello");
    }

    #[test]
    fn test_vi_x_command_with_shift_selection() {
        // Test that 'x' (cut char) works with shift+selection in vi normal mode
        let mut editor = editor_with("hello world");
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Normal));

        editor.line_buffer.set_insertion_point(0);
        editor.update_selection_anchor(true);

        for _ in 0..4 {
            editor.run_edit_command(&EditCommand::MoveRight { select: true });
        }

        assert_eq!(editor.get_selection(), Some((0, 5))); // inclusive selection

        // Simulate vi 'x' command - should cut the selection, not just one character
        editor.run_edit_command(&EditCommand::CutChar);

        assert_eq!(editor.get_buffer(), " world");
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.cut_buffer.get().0, "hello");
    }

    #[rstest]
    #[case("hello world test", 7, "hello  test", 6, "world")] // cursor inside word
    #[case("hello world test", 6, "hello  test", 6, "world")] // cursor at start of word
    #[case("hello world test", 10, "hello  test", 6, "world")] // cursor at end of word
    fn test_cut_inside_word(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] expected_buffer: &str,
        #[case] expected_cursor: usize,
        #[case] expected_cut: &str,
    ) {
        let mut editor = editor_with(input);
        editor.move_to_position(cursor_pos, false);
        editor.cut_text_object(TextObject {
            scope: TextObjectScope::Inner,
            object_type: TextObjectType::Word,
        });
        assert_eq!(editor.get_buffer(), expected_buffer);
        assert_eq!(editor.insertion_point(), expected_cursor);
        assert_eq!(editor.cut_buffer.get().0, expected_cut);
    }

    #[rstest]
    #[case("hello world test", 7, "world")] // cursor inside word
    #[case("hello world test", 6, "world")] // cursor at start of word
    #[case("hello world test", 10, "world")] // cursor at end of word
    fn test_yank_inside_word(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] expected_yank: &str,
    ) {
        let mut editor = editor_with(input);
        editor.move_to_position(cursor_pos, false);
        editor.copy_text_object(TextObject {
            scope: TextObjectScope::Inner,
            object_type: TextObjectType::Word,
        });
        assert_eq!(editor.get_buffer(), input); // Buffer shouldn't change
        assert_eq!(editor.insertion_point(), cursor_pos); // Cursor should return to original position
        assert_eq!(editor.cut_buffer.get().0, expected_yank);
    }

    #[rstest]
    #[case("hello world test", 7, "hello test", 6, "world ")] // word with following space
    #[case("hello world", 7, "hello", 5, " world")] // word at end, gets preceding space
    #[case("word test", 2, "test", 0, "word ")] // first word with following space
    #[case("hello word", 7, "hello", 5, " word")] // last word gets preceding space
    // Edge cases at end of string
    #[case("word", 2, "", 0, "word")] // single word, no whitespace
    #[case(" word", 2, "", 0, " word")] // word with only leading space
    // Edge cases with punctuation boundaries
    #[case("word.", 2, ".", 0, "word")] // word followed by punctuation
    #[case(".word", 2, ".", 1, "word")] // word preceded by punctuation
    #[case("(word)", 2, "()", 1, "word")] // word surrounded by punctuation
    #[case("hello,world", 2, ",world", 0, "hello")] // word followed by punct+word
    #[case("hello,world", 7, "hello,", 6, "world")] // word preceded by word+punct
    fn test_cut_around_word(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] expected_buffer: &str,
        #[case] expected_cursor: usize,
        #[case] expected_cut: &str,
    ) {
        let mut editor = editor_with(input);
        editor.move_to_position(cursor_pos, false);
        editor.cut_text_object(TextObject {
            scope: TextObjectScope::Around,
            object_type: TextObjectType::Word,
        });
        assert_eq!(editor.get_buffer(), expected_buffer);
        assert_eq!(editor.insertion_point(), expected_cursor);
        assert_eq!(editor.cut_buffer.get().0, expected_cut);
    }

    #[rstest]
    #[case("hello world test", 7, "world ")] // word with following space
    #[case("hello world", 7, " world")] // word at end, gets preceding space
    #[case("word test", 2, "word ")] // first word with following space
    fn test_yank_around_word(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] expected_yank: &str,
    ) {
        let mut editor = editor_with(input);
        editor.move_to_position(cursor_pos, false);
        editor.copy_text_object(TextObject {
            scope: TextObjectScope::Around,
            object_type: TextObjectType::Word,
        });
        assert_eq!(editor.get_buffer(), input); // Buffer shouldn't change
        assert_eq!(editor.insertion_point(), cursor_pos); // Cursor should return to original position
        assert_eq!(editor.cut_buffer.get().0, expected_yank);
    }

    #[rstest]
    #[case("hello big-word test", 10, "hello  test", 6, "big-word")] // big word with punctuation
    #[case("hello BIGWORD test", 10, "hello  test", 6, "BIGWORD")] // simple big word
    #[case("test@example.com file", 8, " file", 0, "test@example.com")] //cursor on email address
    #[case("test@example.com file", 17, "test@example.com ", 17, "file")] // cursor at end of "file"
    fn test_cut_inside_big_word(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] expected_buffer: &str,
        #[case] expected_cursor: usize,
        #[case] expected_cut: &str,
    ) {
        let mut editor = editor_with(input);
        editor.move_to_position(cursor_pos, false);
        editor.cut_text_object(TextObject {
            scope: TextObjectScope::Inner,
            object_type: TextObjectType::BigWord,
        });

        assert_eq!(editor.get_buffer(), expected_buffer);
        assert_eq!(editor.insertion_point(), expected_cursor);
        assert_eq!(editor.cut_buffer.get().0, expected_cut);
    }

    #[rstest]
    #[case("hello-world test", 2, "-world test", 0, "hello")] // cursor on "hello"
    #[case("hello-world test", 5, "helloworld test", 5, "-")] // cursor on "-"
    #[case("hello-world test", 8, "hello- test", 6, "world")] // cursor on "world"
    #[case("a-b-c test", 0, "-b-c test", 0, "a")] // single char "a"
    #[case("a-b-c test", 2, "a--c test", 2, "b")] // single char "b"
    fn test_cut_inside_word_with_punctuation(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] expected_buffer: &str,
        #[case] expected_cursor: usize,
        #[case] expected_cut: &str,
    ) {
        let mut editor = editor_with(input);
        editor.move_to_position(cursor_pos, false);
        editor.cut_text_object(TextObject {
            scope: TextObjectScope::Inner,
            object_type: TextObjectType::Word,
        });
        assert_eq!(editor.get_buffer(), expected_buffer);
        assert_eq!(editor.insertion_point(), expected_cursor);
        assert_eq!(editor.cut_buffer.get().0, expected_cut);
    }

    #[rstest]
    #[case("hello-world test", 2, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Word }, "-world test", "hello")] // small word gets just "hello"
    #[case("hello-world test", 2, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::BigWord }, " test", "hello-world")] // big word gets "hello-word"
    #[case("test@example.com", 6, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Word }, "test@", "example.com")] // small word in email (UAX#29 extends across punct)
    #[case("test@example.com", 6, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::BigWord }, "", "test@example.com")] // big word gets entire email
    fn test_word_vs_big_word_comparison(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] text_object: TextObject,
        #[case] expected_buffer: &str,
        #[case] expected_cut: &str,
    ) {
        let mut editor = editor_with(input);
        editor.move_to_position(cursor_pos, false);
        editor.cut_text_object(text_object);
        assert_eq!(editor.get_buffer(), expected_buffer);
        assert_eq!(editor.cut_buffer.get().0, expected_cut);
    }

    #[rstest]
    // Test inside operations (iw) at word boundaries
    #[case("hello world", 0, "hello")] // start of first word
    #[case("hello world", 4, "hello")] // end of first word
    #[case("hello world", 6, "world")] // start of second word
    #[case("hello world", 10, "world")] // end of second word
    // Test at exact word boundaries with punctuation
    #[case("hello-world", 4, "hello")] // just before punctuation
    #[case("hello-world", 5, "-")] // on punctuation
    #[case("hello-world", 6, "world")] // just after punctuation
    fn test_cut_inside_word_boundaries(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] expected_cut: &str,
    ) {
        let mut editor = editor_with(input);
        editor.move_to_position(cursor_pos, false);
        editor.cut_text_object(TextObject {
            scope: TextObjectScope::Inner,
            object_type: TextObjectType::Word,
        });
        assert_eq!(editor.cut_buffer.get().0, expected_cut);
    }

    #[rstest]
    // Test around operations (aw) at word boundaries
    #[case("hello world", 0, "hello ")] // start of first word
    #[case("hello world", 4, "hello ")] // end of first word
    #[case("hello world", 6, " world")] // start of second word (gets preceding space)
    #[case("hello world", 10, " world")] // end of second word
    #[case("word", 0, "word")] // single word, no whitespace
    #[case("word ", 0, "word ")] // word with trailing space
    #[case(" word", 1, " word")] // word with leading space
    fn test_cut_around_word_boundaries(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] expected_cut: &str,
    ) {
        let mut editor = editor_with(input);
        editor.move_to_position(cursor_pos, false);
        editor.cut_text_object(TextObject {
            scope: TextObjectScope::Around,
            object_type: TextObjectType::Word,
        });
        assert_eq!(editor.cut_buffer.get().0, expected_cut);
    }

    #[rstest]
    fn test_cut_text_object_unicode_safety() {
        let mut editor = editor_with("hello 🦀end");
        editor.move_to_position(10, false); // Position after the emoji
        editor.move_to_position(6, false); // Move to the emoji

        editor.cut_text_object(TextObject {
            scope: TextObjectScope::Inner,
            object_type: TextObjectType::Word,
        }); // Cut the emoji

        assert!(editor.line_buffer.is_valid()); // Should not panic or be invalid
    }

    #[rstest]
    // Test operations when cursor is IN WHITESPACE (middle of spaces)
    #[case("hello world test", 5, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Word }, "helloworld test", 5, " ")] // single space
    #[case("hello  world", 6, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Word }, "helloworld", 5, "  ")] // multiple spaces, cursor on second
    #[case("hello   world", 7, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Word }, "helloworld", 5, "   ")] // multiple spaces, cursor on middle
    #[case("   hello", 1, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Word }, "hello", 0, "   ")] // leading spaces, cursor on middle
    #[case("hello   ", 7, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Word }, "hello", 5, "   ")] // trailing spaces, cursor on middle
    #[case("hello\tworld", 5, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Word }, "helloworld", 5, "\t")] // tab character
    #[case("hello\nworld", 5, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Word }, "helloworld", 5, "\n")] // newline character
    #[case("hello world test", 5, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::BigWord }, "helloworld test", 5, " ")] // single space (big word)
    #[case("hello  world", 6, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::BigWord }, "helloworld", 5, "  ")] // multiple spaces (big word)
    #[case("  ", 0, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Word }, "", 0, "  ")] // only whitespace at start
    #[case("  ", 1, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Word }, "", 0, "  ")] // only whitespace at end
    #[case("hello  ", 5, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Word }, "hello", 5, "  ")] // trailing whitespace at string end
    #[case("  hello", 0, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Word }, "hello", 0, "  ")] // leading whitespace at string start
    fn test_text_object_in_whitespace(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] text_object: TextObject,
        #[case] expected_buffer: &str,
        #[case] expected_cursor: usize,
        #[case] expected_cut: &str,
    ) {
        let mut editor = editor_with(input);
        editor.move_to_position(cursor_pos, false);
        editor.cut_text_object(text_object);
        assert_eq!(editor.get_buffer(), expected_buffer);
        assert_eq!(editor.insertion_point(), expected_cursor);
        assert_eq!(editor.cut_buffer.get().0, expected_cut);
    }

    #[rstest]
    // Test text object jumping behavior in various scenarios
    // Cursor inside empty pairs should operate on current pair (cursor stays, nothing cut)
    #[case(r#"foo()bar"#, 4, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Brackets }, "foo()bar", 4, "")] // inside empty brackets
    #[case(r#"foo""bar"#, 4, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Quote }, "foo\"\"bar", 4, "")] // inside empty quotes
    // Cursor outside pairs should jump to next pair (even if empty)
    #[case(r#"foo ()bar"#, 2, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Brackets }, "foo ()bar", 5, "")] // jump to empty brackets
    #[case(r#"foo ""bar"#, 2, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Quote }, "foo \"\"bar", 5, "")] // jump to empty quote
    #[case(r#"foo (content)bar"#, 2, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Brackets }, "foo ()bar", 5, "content")] // jump to non-empty brackets
    #[case(r#"foo "content"bar"#, 2, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Quote }, "foo \"\"bar", 5, "content")] // jump to non-empty quotes
    // Cursor between pairs should jump to next pair
    #[case(r#"(first) (second)"#, 8, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Brackets }, "(first) ()", 9, "second")] // between brackets
    #[case(r#""first" "second""#, 8, TextObject { scope: TextObjectScope::Inner, object_type: TextObjectType::Quote }, "\"first\"\"second\"", 7, " ")] // between quotes
    // Around scope should include the pair characters
    #[case(r#"foo (bar)"#, 2, TextObject { scope: TextObjectScope::Around, object_type: TextObjectType::Brackets }, "foo ", 4, "(bar)")] // around includes parentheses
    #[case(r#"foo "bar""#, 2, TextObject { scope: TextObjectScope::Around, object_type: TextObjectType::Quote }, "foo ", 4, "\"bar\"")] // around includes quotes
    fn test_text_object_jumping_behavior(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] text_object: TextObject,
        #[case] expected_buffer: &str,
        #[case] expected_cursor: usize,
        #[case] expected_cut: &str,
    ) {
        let mut editor = editor_with(input);
        editor.move_to_position(cursor_pos, false);
        editor.cut_text_object(text_object);
        assert_eq!(editor.get_buffer(), expected_buffer);
        assert_eq!(editor.insertion_point(), expected_cursor);
        assert_eq!(editor.cut_buffer.get().0, expected_cut);
    }

    #[rstest]
    // Test bracket_text_object_range with Inner scope - just the content inside brackets
    #[case("foo(bar)baz", 5, TextObjectScope::Inner, Some(4..7))] // cursor inside brackets
    #[case("foo[bar]baz", 5, TextObjectScope::Inner, Some(4..7))] // square brackets
    #[case("foo{bar}baz", 5, TextObjectScope::Inner, Some(4..7))] // square brackets
    #[case("foo()bar", 4, TextObjectScope::Inner, Some(4..4))] // empty brackets
    #[case("(nested[inner]outer)", 8, TextObjectScope::Inner, Some(8..13))] // nested, innermost
    #[case("(nested[mixed{inner}brackets]outer)", 8, TextObjectScope::Inner, Some(8..28))] // nested, innermost
    #[case("next(nested[mixed{inner}brackets]outer)", 0, TextObjectScope::Inner, Some(5..38))] // next nested mixed
    #[case("foo (bar)baz", 0, TextObjectScope::Inner, Some(5..8))] // next pair from line start
    #[case("    (bar)baz", 1, TextObjectScope::Inner, Some(5..8))] // next pair from whitespace
    #[case("foo(bar)baz", 2, TextObjectScope::Inner, Some(4..7))] // next pair from word
    #[case("foo(bar\nbaz)qux", 8, TextObjectScope::Inner, Some(4..11))] // multi-line brackets
    #[case("foo\n(bar\nbaz)qux", 0, TextObjectScope::Inner, Some(5..12))] // next multi-line brackets
    #[case("foo\n(bar\nbaz)qux", 3, TextObjectScope::Around, Some(4..13))] // next multi-line brackets
    #[case("{hello}", 3, TextObjectScope::Around, Some(0..7))] // includes curly brackets
    #[case("foo()bar", 4, TextObjectScope::Around, Some(3..5))] // around empty brackets
    #[case("(nested(inner)outer)", 8, TextObjectScope::Around, Some(7..14))] // nested around includes delimiters
    #[case("start(nested(inner)outer)", 2, TextObjectScope::Around, Some(5..25))] // Next outer nested pair
    #[case("(mixed{nested)brackets", 1, TextObjectScope::Inner, Some(1..13))] // mixed nesting
    #[case("(unclosed(nested)brackets", 1, TextObjectScope::Inner, Some(10..16))] // unclosed bracket, find next closed
    #[case("no brackets here", 5, TextObjectScope::Inner, None)] // no brackets found
    #[case("(unclosed", 1, TextObjectScope::Inner, None)] // unclosed bracket
    #[case("(mismatched}", 1, TextObjectScope::Inner, None)] // mismatched brackets
    fn test_bracket_text_object_range(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] scope: TextObjectScope,
        #[case] expected: Option<std::ops::Range<usize>>,
    ) {
        let mut editor = editor_with(input);
        editor.move_to_position(cursor_pos, false);
        let result = editor.bracket_text_object_range(scope);
        assert_eq!(result, expected);
    }

    #[rstest]
    // Test quote_text_object_range with Inner scope - just the content inside quotes
    #[case(r#"foo"bar"baz"#, 5, TextObjectScope::Inner, Some(4..7))] // cursor inside double quotes
    #[case("foo'bar'baz", 5, TextObjectScope::Inner, Some(4..7))] // single quotes
    #[case("foo`bar`baz", 5, TextObjectScope::Inner, Some(4..7))] // backticks
    #[case(r#"foo""bar"#, 4, TextObjectScope::Inner, Some(4..4))] // empty quotes
    #[case(r#""nested'inner'outer""#, 8, TextObjectScope::Inner, Some(8..13))] // nested, innermost
    #[case(r#""nested`mixed'inner'backticks`outer""#, 8, TextObjectScope::Inner, Some(8..29))] // nested, innermost
    #[case(r#"next"nested'mixed`inner`quotes'outer""#, 0, TextObjectScope::Inner, Some(5..36))] // next nested mixed
    #[case(r#"foo "bar"baz"#, 0, TextObjectScope::Inner, Some(5..8))] // next pair
    #[case(r#"foo"bar"baz"#, 2, TextObjectScope::Inner, Some(4..7))] // next from inside word
    #[case(r#"foo"bar"baz"#, 4, TextObjectScope::Around, Some(3..8))] // around includes quotes
    #[case(r#"foo"bar"baz"#, 3, TextObjectScope::Around, Some(3..8))] // around on opening quote
    #[case(r#"foo"bar"baz"#, 2, TextObjectScope::Around, Some(3..8))] // around next quotes
    #[case(r#"foo""bar"#, 4, TextObjectScope::Around, Some(3..5))] // around empty quotes
    #[case(r#"foo""bar"#, 1, TextObjectScope::Around, Some(3..5))] // around empty quotes
    #[case(r#""nested"inner"outer""#, 8, TextObjectScope::Around, Some(7..14))] // nested around includes delimiters
    #[case(r#"start"nested'inner'outer""#, 2, TextObjectScope::Around, Some(5..25))] // Next outer nested pair
    #[case("no quotes here", 5, TextObjectScope::Inner, None)] // no quotes found
    #[case(r#"foo"bar"#, 1, TextObjectScope::Inner, None)] // unclosed quote
    #[case("foo'bar\nbaz'qux", 5, TextObjectScope::Inner, None)] // quotes don't span multiple lines
    #[case("foo'bar\nbaz'qux", 0, TextObjectScope::Inner, None)] // quotes don't span multiple lines
    #[case("foobar\n`baz`qux", 6, TextObjectScope::Inner, None)] // quotes don't span multiple lines
    #[case("foo\n(bar\nbaz)qux", 0, TextObjectScope::Inner, None)] // next multi-line brackets
    #[case("foo\n(bar\nbaz)qux", 3, TextObjectScope::Around, None)] // next multi-line brackets
    fn test_quote_text_object_range(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] scope: TextObjectScope,
        #[case] expected: Option<std::ops::Range<usize>>,
    ) {
        let mut editor = editor_with(input);
        editor.line_buffer.set_insertion_point(cursor_pos);
        let result = editor.quote_text_object_range(scope);
        assert_eq!(result, expected);
    }

    #[rstest]
    // Test edge cases and complex scenarios for both bracket and quote text objects
    #[case("", 0, TextObjectScope::Inner, None, None)] // empty buffer
    #[case("a", 0, TextObjectScope::Inner, None, None)] // single character
    #[case("()", 1, TextObjectScope::Inner, Some(1..1), None)] // empty brackets, cursor inside
    #[case(r#""""#, 1, TextObjectScope::Inner, None, Some(1..1))] // empty quotes, cursor inside
    #[case("([{}])", 3, TextObjectScope::Inner, Some(3..3), None)] // deeply nested brackets
    #[case(r#""'`text`'""#, 5, TextObjectScope::Inner, None, Some(3..7))] // deeply nested quotes
    #[case("(text) and [more]", 5, TextObjectScope::Around, Some(0..6), None)] // multiple bracket types
    #[case(r#""text" and 'more'"#, 5, TextObjectScope::Around, None, Some(0..6))] // multiple quote types
    fn test_text_object_edge_cases(
        #[case] input: &str,
        #[case] cursor_pos: usize,
        #[case] scope: TextObjectScope,
        #[case] expected_bracket: Option<std::ops::Range<usize>>,
        #[case] expected_quote: Option<std::ops::Range<usize>>,
    ) {
        let mut editor = editor_with(input);
        editor.move_to_position(cursor_pos, false);

        let bracket_result = editor.bracket_text_object_range(scope);
        let quote_result = editor.quote_text_object_range(scope);

        assert_eq!(bracket_result, expected_bracket);
        assert_eq!(quote_result, expected_quote);
    }

    // --- MotionTarget verbs (Move / Extend / Cut / Copy / Erase) ---
    //
    // These drive the public verbs through the full lowering
    // (`MotionTarget` -> `resolve_motion`) in the default (emacs)
    // editor, proving the substrate in isolation before any keymap emits it.

    /// `w` as a target: small-word start, forward.
    fn word_start_fwd() -> MotionTarget {
        MotionTarget::Word {
            kind: WordKind::Word,
            edge: WordEdge::Start,
            direction: Direction::Forward,
        }
    }

    #[test]
    fn move_word_forward_lands_on_next_word_start() {
        let mut editor = editor_with("foo bar baz");
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::Move(word_start_fwd()));
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.get_selection(), None); // Move collapses — no selection
    }

    #[test]
    fn move_grapheme_right_steps_over_multibyte() {
        let mut editor = editor_with("café"); // 'é' is 2 bytes: graphemes at 0,1,2,3, len 5
        editor.move_to_position(3, false);
        editor.run_edit_command(&EditCommand::Move(MotionTarget::Grapheme(
            Direction::Forward,
        )));
        assert_eq!(editor.insertion_point(), 5); // one grapheme, two bytes
    }

    #[test]
    fn extend_word_forward_keeps_anchor_at_origin() {
        let mut editor = editor_with("foo bar baz");
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::Extend(word_start_fwd()));
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.get_selection(), Some((0, 4))); // anchor stays at the origin
    }

    #[test]
    fn vi_visual_extend_word_covers_landing() {
        // `CoverLanding`: vi visual sweeps the grapheme the motion lands *on*, so
        // `Extend(w)` over "foo bar" selects "foo b" (vim's inclusive visual).
        let mut editor = editor_with("foo bar baz");
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Visual));
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::Extend(word_start_fwd()));
        assert_eq!(editor.get_selection(), Some((0, 5)));
        assert_eq!(editor.insertion_point(), 4);
    }

    #[test]
    fn emacs_extend_word_is_exclusive_span() {
        // The contrast that proves the axis is real: the *same* `Extend(w)` from
        // the *same position in a `Span` (bar) mode stops at the boundary "foo "
        // instead of sweeping the landing grapheme.
        let mut editor = editor_with("foo bar baz");
        editor.set_edit_mode(PromptEditMode::Emacs);
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::Extend(word_start_fwd()));
        assert_eq!(editor.get_selection(), Some((0, 4)))
    }

    #[test]
    fn emacs_extend_word_twice_grows_span() {
        // A second `Extend` resolves the next motion from the live head and grows
        // the existing Span — not from a collapsed or retreated caret. "foo bar
        // baz": 0 → "foo " (0,4) → "foo bar " (0,8), anchor pinned at the origin.
        let mut editor = editor_with("foo bar baz");
        editor.set_edit_mode(PromptEditMode::Emacs);
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::Extend(word_start_fwd()));
        assert_eq!(editor.get_selection(), Some((0, 4)));
        editor.run_edit_command(&EditCommand::Extend(word_start_fwd()));
        assert_eq!(editor.get_selection(), Some((0, 8)));
    }

    #[test]
    fn cut_word_forward_removes_range_and_yanks() {
        let mut editor = editor_with("foo bar baz");
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::Cut {
            target: word_start_fwd(),
            granularity: Granularity::CharWise,
        });
        assert_eq!(editor.get_buffer(), "bar baz");
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.cut_buffer.get().0, "foo ");
    }

    #[test]
    fn cut_word_backward_removes_preceding_word() {
        let mut editor = editor_with("foo bar");
        editor.move_to_position(7, false); // end of buffer
        editor.run_edit_command(&EditCommand::Cut {
            target: MotionTarget::Word {
                kind: WordKind::Word,
                edge: WordEdge::Start,
                direction: Direction::Backward,
            },
            granularity: Granularity::CharWise,
        });
        assert_eq!(editor.get_buffer(), "foo ");
        assert_eq!(editor.insertion_point(), 4); // cursor lands at the range start
        assert_eq!(editor.cut_buffer.get().0, "bar");
    }

    #[test]
    fn copy_word_forward_yanks_without_editing() {
        let mut editor = editor_with("foo bar");
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::Copy {
            target: word_start_fwd(),
            granularity: Granularity::CharWise,
        });
        assert_eq!(editor.get_buffer(), "foo bar"); // buffer untouched
        assert_eq!(editor.insertion_point(), 0); // cursor untouched
        assert_eq!(editor.cut_buffer.get().0, "foo ");
    }

    #[test]
    fn erase_word_forward_deletes_without_touching_register() {
        let mut editor = editor_with("foo bar baz");
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::Erase(word_start_fwd()));
        assert_eq!(editor.get_buffer(), "bar baz");
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.cut_buffer.get().0, ""); // register left untouched
    }

    #[test]
    fn erase_find_forward_is_inclusive() {
        // op_end (inclusive forward find) must reach Erase through `operate`:
        // `dt`-style would stop short, but `Find { On }` eats through the 'b'.
        let mut editor = editor_with("foo bar baz");
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::Erase(find(
            'b',
            Direction::Forward,
            FindStop::On,
        )));
        assert_eq!(editor.get_buffer(), "ar baz"); // removed "foo b"
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.cut_buffer.get().0, ""); // register left untouched
    }

    #[test]
    fn erase_grapheme_backward_over_multibyte() {
        // backward span (origin > op_end) across a 2-byte grapheme.
        let mut editor = editor_with("café"); // 'é' is [3,5)
        editor.move_to_position(5, false);
        editor.run_edit_command(&EditCommand::Erase(MotionTarget::Grapheme(
            Direction::Backward,
        )));
        assert_eq!(editor.get_buffer(), "caf");
        assert_eq!(editor.insertion_point(), 3);
        assert_eq!(editor.cut_buffer.get().0, ""); // register left untouched
    }

    /// `e` as a target: small-word end, forward.
    fn word_end_fwd() -> MotionTarget {
        MotionTarget::Word {
            kind: WordKind::Word,
            edge: WordEdge::End,
            direction: Direction::Forward,
        }
    }

    #[test]
    fn move_word_end_landing_follows_caret_geometry() {
        // The same `Word{End}` target lands differently by caret geometry: a
        // block caret (vi normal) rests *on* the last grapheme; a bar caret
        // (emacs / default) rests on the word's trailing boundary one past it.
        let mut block = vi_editor("foo bar", PromptViMode::Normal);
        block.move_to_position(0, false);
        block.run_edit_command(&EditCommand::Move(word_end_fwd()));
        assert_eq!(block.insertion_point(), 2); // on the second 'o'

        let mut bar = editor_with("foo bar"); // default = emacs, Between
        bar.move_to_position(0, false);
        bar.run_edit_command(&EditCommand::Move(word_end_fwd()));
        assert_eq!(bar.insertion_point(), 3); // trailing boundary, past the 'o'
    }

    #[test]
    fn cut_word_end_is_inclusive_of_last_char() {
        // vi `de`: same target as `e`, but the operator *consumes* the char the
        // motion lands on — so `de` from the start of "foo" deletes all of "foo".
        let mut editor = editor_with("foo bar");
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::Cut {
            target: word_end_fwd(),
            granularity: Granularity::CharWise,
        });
        assert_eq!(editor.get_buffer(), " bar");
        assert_eq!(editor.cut_buffer.get().0, "foo");
    }

    // --- emacs word-command lowering (legacy `*Word*` sugar) -------------
    //
    // The `MoveWord*`/`CutWord*`/`CopyWord*` commands now lower onto the
    // `MotionTarget` verb path with `WordKind::Unicode`. These pin the two
    // properties that lowering could silently break: the emacs `M-f` "rest
    // after the word" fork, and that emacs words stay UAX-29 (contractions /
    // punctuation kept whole) rather than drifting to the vi class-word rule.

    /// Differential guard: every word command must, at *every* cursor position,
    /// produce exactly what the legacy `LineBuffer::*_index` spec produces. The
    /// `*_index` methods are still present (deleted only in a later step), so
    /// this compares the live command against the pre-migration definition
    /// across a sweep of positions — catching the position-dependent skips that
    /// the hand-picked single-position tests above missed.
    #[test]
    fn word_commands_match_legacy_spec_at_every_position() {
        let buffers = [
            "foo bar baz",
            "foo.bar baz",    // punctuation kept whole by UAX-29
            "can't stop now", // contraction
            "café résumé",    // multibyte graphemes
            "  lead trail  ", // leading / trailing whitespace
            "a",
            "",
        ];
        for buf in buffers {
            for pos in (0..=buf.len()).filter(|p| buf.is_char_boundary(*p)) {
                // Each entry: the command under test, and the resolver
                // expression it must agree with in this (default = emacs, bar)
                // editor. Move commands compare the landing position; cut/copy
                // compare the resulting (buffer, register). `wb` resolves a word
                // boundary the same way the commands' plumbing does, so this pins
                // that the dispatch/operate wiring stays faithful to `locate_word`.
                fn wb(
                    lb: &LineBuffer,
                    kind: WordKind,
                    edge: WordEdge,
                    fwd: bool,
                    block: bool,
                ) -> usize {
                    let buf = lb.get_buffer();
                    let origin = lb.insertion_point();
                    // Mirror resolve_motion's block word-end identity independently
                    // (the bar boundary one cell over, rendered one cell back).
                    use crate::core_editor::graphemes::{
                        next_grapheme_boundary, prev_grapheme_boundary,
                    };
                    use crate::core_editor::word;
                    let dir = if fwd {
                        Direction::Forward
                    } else {
                        Direction::Backward
                    };
                    if block && fwd && edge == WordEdge::End {
                        let probe = next_grapheme_boundary(buf, origin);
                        prev_grapheme_boundary(buf, word::locate_word(buf, probe, kind, edge, dir))
                    } else {
                        word::locate_word(buf, origin, kind, edge, dir)
                    }
                }
                #[allow(clippy::type_complexity)]
                let moves: &[(EditCommand, fn(&LineBuffer) -> usize)] = &[
                    (EditCommand::MoveWordLeft { select: false }, |lb| {
                        wb(lb, WordKind::Unicode, WordEdge::Start, false, false)
                    }),
                    (EditCommand::MoveBigWordLeft { select: false }, |lb| {
                        wb(lb, WordKind::LongWord, WordEdge::Start, false, false)
                    }),
                    // bar caret: forward word-end rests on the trailing boundary
                    (EditCommand::MoveWordRight { select: false }, |lb| {
                        wb(lb, WordKind::Unicode, WordEdge::End, true, false)
                    }),
                    (EditCommand::MoveWordRightStart { select: false }, |lb| {
                        wb(lb, WordKind::Unicode, WordEdge::Start, true, false)
                    }),
                    (EditCommand::MoveBigWordRightStart { select: false }, |lb| {
                        wb(lb, WordKind::LongWord, WordEdge::Start, true, false)
                    }),
                    // vi-`e` on-char reading, forced block geometry
                    (EditCommand::MoveWordRightEnd { select: false }, |lb| {
                        wb(lb, WordKind::Unicode, WordEdge::End, true, true)
                    }),
                    (EditCommand::MoveBigWordRightEnd { select: false }, |lb| {
                        wb(lb, WordKind::LongWord, WordEdge::End, true, true)
                    }),
                ];
                for (cmd, legacy) in moves {
                    let mut got = editor_with(buf);
                    got.move_to_position(pos, false);
                    got.run_edit_command(cmd);
                    let mut spec = editor_with(buf);
                    spec.move_to_position(pos, false);
                    let target = legacy(&spec.line_buffer);
                    assert_eq!(
                        got.insertion_point(),
                        target,
                        "{cmd:?} at pos {pos} of {buf:?}"
                    );
                }

                // Cut/Copy commands: legacy consumed `lo..hi`. Cut compares the
                // resulting (buffer, register); Copy leaves the buffer and only
                // fills the register.
                #[allow(clippy::type_complexity)]
                let ops: &[(
                    EditCommand,
                    EditCommand,
                    fn(&LineBuffer, usize) -> (usize, usize),
                )] = &[
                    (
                        EditCommand::CutWordLeft,
                        EditCommand::CopyWordLeft,
                        |lb, ip| (wb(lb, WordKind::Unicode, WordEdge::Start, false, false), ip),
                    ),
                    (
                        EditCommand::CutBigWordLeft,
                        EditCommand::CopyBigWordLeft,
                        |lb, ip| {
                            (
                                wb(lb, WordKind::LongWord, WordEdge::Start, false, false),
                                ip,
                            )
                        },
                    ),
                    (
                        EditCommand::CutWordRight,
                        EditCommand::CopyWordRight,
                        |lb, ip| (ip, wb(lb, WordKind::Unicode, WordEdge::End, true, false)),
                    ),
                    (
                        EditCommand::CutBigWordRight,
                        EditCommand::CopyBigWordRight,
                        |lb, ip| (ip, wb(lb, WordKind::LongWord, WordEdge::End, true, false)),
                    ),
                    (
                        EditCommand::CutWordRightToNext,
                        EditCommand::CopyWordRightToNext,
                        |lb, ip| (ip, wb(lb, WordKind::Unicode, WordEdge::Start, true, false)),
                    ),
                    (
                        EditCommand::CutBigWordRightToNext,
                        EditCommand::CopyBigWordRightToNext,
                        |lb, ip| (ip, wb(lb, WordKind::LongWord, WordEdge::Start, true, false)),
                    ),
                ];
                for (cut_cmd, copy_cmd, legacy) in ops {
                    // Cut
                    let mut got = editor_with(buf);
                    got.move_to_position(pos, false);
                    got.run_edit_command(cut_cmd);
                    let mut spec = editor_with(buf);
                    spec.move_to_position(pos, false);
                    let (lo, hi) = legacy(&spec.line_buffer, pos);
                    spec.cut_range(lo..hi);
                    let got_pair = (got.get_buffer().to_string(), got.cut_buffer.get().0);
                    let spec_pair = (spec.get_buffer().to_string(), spec.cut_buffer.get().0);
                    assert_eq!(got_pair, spec_pair, "{cut_cmd:?} at pos {pos} of {buf:?}");

                    // Copy: buffer untouched, register == legacy slice.
                    let mut got = editor_with(buf);
                    got.move_to_position(pos, false);
                    got.run_edit_command(copy_cmd);
                    assert_eq!(got.get_buffer(), buf, "{copy_cmd:?} touched buffer");
                    let expect = buf.get(lo..hi).unwrap_or("");
                    assert_eq!(
                        got.cut_buffer.get().0,
                        expect,
                        "{copy_cmd:?} at pos {pos} of {buf:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn move_word_right_rests_after_word_like_emacs_meta_f() {
        // emacs `M-f`: the bar lands *after* "foo" (byte 3), not *on* its last
        // char (byte 2, where a bare `e`/`Move(End)` would stop).
        let mut editor = editor_with("foo bar");
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::MoveWordRight { select: false });
        assert_eq!(editor.insertion_point(), 3);
    }

    #[test]
    fn move_word_right_keeps_contraction_whole() {
        // UAX-29 keeps "can't" one word, so `M-f` skips past the apostrophe to
        // byte 5; a vi class-word would have stopped on the `'` at byte 3.
        let mut editor = editor_with("can't stop");
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::MoveWordRight { select: false });
        assert_eq!(editor.insertion_point(), 5);
    }

    #[test]
    fn cut_word_right_consumes_whole_contraction() {
        // emacs `M-d` over "can't" removes the whole contraction (bytes 0..5),
        // leaving " stop" — a class-word `dw` would cut only "can" (0..3).
        let mut editor = editor_with("can't stop");
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::CutWordRight);
        assert_eq!(editor.get_buffer(), " stop");
        assert_eq!(editor.cut_buffer.get().0, "can't");
    }

    #[test]
    fn move_word_right_with_select_extends_anchor() {
        // The `select` flag maps to Extend: anchor stays at the origin while the
        // head travels to the after-word rest position.
        let mut editor = editor_with("foo bar");
        editor.move_to_position(0, false);
        editor.run_edit_command(&EditCommand::MoveWordRight { select: true });
        assert_eq!(editor.insertion_point(), 3);
        assert_eq!(editor.get_selection(), Some((0, 3)));
    }

    // emacs forward-word completes the *current* word with no skip — the case a
    // vi-`e` verb path gets wrong. Pin the cursor-inside-word positions that the
    // word-start-only tests above can't see.

    #[test]
    fn move_word_right_from_midword_completes_current_word() {
        // `M-f` from byte 2 (between the o's of "foo") rests at 3 (after "foo"),
        // NOT 7 (which is where the vi-`e` skip would land).
        let mut editor = editor_with("foo bar");
        editor.move_to_position(2, false);
        editor.run_edit_command(&EditCommand::MoveWordRight { select: false });
        assert_eq!(editor.insertion_point(), 3);
    }

    #[test]
    fn cut_word_right_from_midword_consumes_rest_of_word() {
        // `M-d` from byte 2 in "foo bar" kills only "o" (the rest of "foo"),
        // leaving "fo bar" — a vi-`e` skip would have eaten "o bar".
        let mut editor = editor_with("foo bar");
        editor.move_to_position(2, false);
        editor.run_edit_command(&EditCommand::CutWordRight);
        assert_eq!(editor.get_buffer(), "fo bar");
        assert_eq!(editor.cut_buffer.get().0, "o");
    }

    #[test]
    fn cut_big_word_right_from_midword_consumes_rest_of_word() {
        let mut editor = editor_with("foo.bar baz");
        editor.move_to_position(2, false); // inside "foo.bar" (one big WORD)
        editor.run_edit_command(&EditCommand::CutBigWordRight);
        assert_eq!(editor.get_buffer(), "fo baz");
        assert_eq!(editor.cut_buffer.get().0, "o.bar");
    }

    // --- migration characterization -------------------------------------
    //
    // The new `MotionTarget` verbs must have the *same buffer effect* as the
    // dedicated commands they replace — the old command is the spec. These
    // assert `new == old` so they need no hand-computed vim semantics. They
    // pass on the pre-migration code, so they retroactively prove C1's `0`/`$`
    // re-lowering was behavior-preserving and *gate* C2's `f`/`t` re-lowering:
    // they must stay green after the motions emit `Cut/Move(Find)`.

    /// Run `cmd` on `buffer` from `cursor`; return (buffer, cursor, cut text).
    fn outcome(
        buffer: &str,
        cursor: usize,
        cmd: &EditCommand,
    ) -> (String, usize, Option<(usize, usize)>, String) {
        let mut editor = editor_with(buffer);
        editor.move_to_position(cursor, false);
        editor.run_edit_command(cmd);
        (
            editor.get_buffer().to_string(),
            editor.insertion_point(),
            editor.get_selection(),
            editor.cut_buffer.get().0,
        )
    }

    /// Assert two commands have identical effect from the same starting point.
    fn equivalent(buffer: &str, cursor: usize, new: &EditCommand, old: &EditCommand) {
        assert_eq!(outcome(buffer, cursor, new), outcome(buffer, cursor, old));
    }

    fn find(ch: char, direction: Direction, stop: FindStop) -> MotionTarget {
        MotionTarget::Find {
            ch,
            direction,
            stop,
        }
    }

    // C1 backfill: `0`/`$` line edges vs the dedicated line cut/copy commands.

    #[test]
    fn cut_line_edge_matches_dedicated_line_cuts() {
        // `d$` and `d0` on a single line.
        equivalent(
            "foo bar",
            2,
            &EditCommand::Cut {
                target: MotionTarget::LineEdge(Direction::Forward),
                granularity: Granularity::CharWise,
            },
            &EditCommand::CutToLineEnd,
        );
        equivalent(
            "foo bar",
            4,
            &EditCommand::Cut {
                target: MotionTarget::LineEdge(Direction::Backward),
                granularity: Granularity::CharWise,
            },
            &EditCommand::CutFromLineStart,
        );
    }

    #[test]
    fn copy_line_edge_matches_dedicated_line_copies() {
        equivalent(
            "foo bar",
            2,
            &EditCommand::Copy {
                target: MotionTarget::LineEdge(Direction::Forward),
                granularity: Granularity::CharWise,
            },
            &EditCommand::CopyToLineEnd,
        );
        equivalent(
            "foo bar",
            4,
            &EditCommand::Copy {
                target: MotionTarget::LineEdge(Direction::Backward),
                granularity: Granularity::CharWise,
            },
            &EditCommand::CopyFromLineStart,
        );
    }

    #[test]
    fn cut_line_edge_forward_stops_at_newline() {
        // The riskiest C1 claim: on a multiline buffer `d$` must cut only to the
        // `\n`, matching `CutToLineEnd` — not run to the buffer end.
        equivalent(
            "ab\ncd",
            0,
            &EditCommand::Cut {
                target: MotionTarget::LineEdge(Direction::Forward),
                granularity: Granularity::CharWise,
            },
            &EditCommand::CutToLineEnd,
        );
        let (buffer, cursor, _selection, cut) = outcome(
            "ab\ncd",
            0,
            &EditCommand::Cut {
                target: MotionTarget::LineEdge(Direction::Forward),
                granularity: Granularity::CharWise,
            },
        );
        assert_eq!(buffer, "\ncd");
        assert_eq!(cursor, 0);
        assert_eq!(cut, "ab");
    }

    // C3 gate: `gg`/`G` (BufferEdge) vs the dedicated MoveToStart/MoveToEnd.
    // BufferEdge ignores line breaks — it goes to the buffer edge, not a line
    // edge — so these also confirm the multiline behavior.

    #[test]
    fn move_buffer_edge_matches_move_to_start_end() {
        equivalent(
            "foo bar",
            3,
            &EditCommand::Move(MotionTarget::BufferEdge(Direction::Backward)),
            &EditCommand::MoveToStart { select: false },
        );
        equivalent(
            "foo bar",
            3,
            &EditCommand::Move(MotionTarget::BufferEdge(Direction::Forward)),
            &EditCommand::MoveToEnd { select: false },
        );
    }

    #[test]
    fn extend_buffer_edge_matches_move_to_start_end_selecting() {
        // visual `gg`/`G` — the selection must match too (now compared by `outcome`)
        equivalent(
            "foo bar",
            3,
            &EditCommand::Extend(MotionTarget::BufferEdge(Direction::Backward)),
            &EditCommand::MoveToStart { select: true },
        );
        equivalent(
            "foo bar",
            3,
            &EditCommand::Extend(MotionTarget::BufferEdge(Direction::Forward)),
            &EditCommand::MoveToEnd { select: true },
        );
    }

    #[test]
    fn buffer_edge_spans_lines() {
        // from the second line, `gg` lands at buffer start (not the line start)
        // and `G` at buffer end.
        equivalent(
            "ab\ncd",
            4,
            &EditCommand::Move(MotionTarget::BufferEdge(Direction::Backward)),
            &EditCommand::MoveToStart { select: false },
        );
        equivalent(
            "ab\ncd",
            0,
            &EditCommand::Move(MotionTarget::BufferEdge(Direction::Forward)),
            &EditCommand::MoveToEnd { select: false },
        );
    }

    // C2 gate: `f`/`t`/`F`/`T` (Find) vs the dedicated char-search commands.

    #[test]
    fn cut_find_forward_on_matches_cut_right_until() {
        // df b
        equivalent(
            "foo bar baz",
            0,
            &EditCommand::Cut {
                target: find('b', Direction::Forward, FindStop::On),
                granularity: Granularity::CharWise,
            },
            &EditCommand::CutRightUntil('b'),
        );
    }

    #[test]
    fn cut_find_forward_before_matches_cut_right_before() {
        // dt b
        equivalent(
            "foo bar baz",
            0,
            &EditCommand::Cut {
                target: find('b', Direction::Forward, FindStop::Before),
                granularity: Granularity::CharWise,
            },
            &EditCommand::CutRightBefore('b'),
        );
    }

    #[test]
    fn cut_find_backward_on_matches_cut_left_until() {
        // dF o (cursor at end of buffer)
        equivalent(
            "foo bar baz",
            11,
            &EditCommand::Cut {
                target: find('o', Direction::Backward, FindStop::On),
                granularity: Granularity::CharWise,
            },
            &EditCommand::CutLeftUntil('o'),
        );
    }

    #[test]
    fn cut_find_backward_before_matches_cut_left_before() {
        // dT o
        equivalent(
            "foo bar baz",
            11,
            &EditCommand::Cut {
                target: find('o', Direction::Backward, FindStop::Before),
                granularity: Granularity::CharWise,
            },
            &EditCommand::CutLeftBefore('o'),
        );
    }

    #[test]
    fn cut_find_absent_char_is_noop() {
        equivalent(
            "foo bar",
            0,
            &EditCommand::Cut {
                target: find('z', Direction::Forward, FindStop::On),
                granularity: Granularity::CharWise,
            },
            &EditCommand::CutRightUntil('z'),
        );
    }

    #[test]
    fn copy_find_forward_matches_copy_right_until() {
        equivalent(
            "foo bar baz",
            0,
            &EditCommand::Copy {
                target: find('b', Direction::Forward, FindStop::On),
                granularity: Granularity::CharWise,
            },
            &EditCommand::CopyRightUntil('b'),
        );
    }

    #[test]
    fn move_find_forward_matches_move_right_until() {
        // Guards the `f`-vs-`;` two-path divergence: bare `f` (which will emit
        // `Move(Find)`) must land where the replay path `;` lands — and `;`
        // keeps using `MoveRightUntil`.
        equivalent(
            "foo bar baz",
            0,
            &EditCommand::Move(find('b', Direction::Forward, FindStop::On)),
            &EditCommand::MoveRightUntil {
                c: 'b',
                select: false,
            },
        );
    }

    // The remaining three `Move` corners gate C2(b): `;`/`,` replay re-emits
    // `Move(stored Find)`, and `,` reverses the stored direction. Proving each
    // `Move(Find{..})` matches the dedicated `Move*Until`/`Move*Before` it
    // replaces means the replay migration preserves where the cursor lands —
    // including the reversed (`,`) direction.

    #[test]
    fn move_find_forward_before_matches_move_right_before() {
        // bare `;` after `t`
        equivalent(
            "foo bar baz",
            0,
            &EditCommand::Move(find('b', Direction::Forward, FindStop::Before)),
            &EditCommand::MoveRightBefore {
                c: 'b',
                select: false,
            },
        );
    }

    #[test]
    fn move_find_backward_on_matches_move_left_until() {
        // bare `;` after `F`, and the `,`-reverse of `f`
        equivalent(
            "foo bar baz",
            11,
            &EditCommand::Move(find('o', Direction::Backward, FindStop::On)),
            &EditCommand::MoveLeftUntil {
                c: 'o',
                select: false,
            },
        );
    }

    #[test]
    fn move_find_backward_before_matches_move_left_before() {
        // bare `;` after `T`, and the `,`-reverse of `t`
        equivalent(
            "foo bar baz",
            11,
            &EditCommand::Move(find('o', Direction::Backward, FindStop::Before)),
            &EditCommand::MoveLeftBefore {
                c: 'o',
                select: false,
            },
        );
    }
}
