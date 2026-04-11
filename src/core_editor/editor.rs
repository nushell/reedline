use super::{edit_stack::EditStack, Clipboard, ClipboardMode, LineBuffer};
#[cfg(feature = "system_clipboard")]
use crate::core_editor::get_system_clipboard;
#[cfg(all(feature = "helix", not(test)))]
#[path = "../edit_mode/hx/word.rs"]
mod word;
#[cfg(all(feature = "helix", test))]
use crate::edit_mode::hx::word;
use crate::enums::{EditType, TextObject, TextObjectScope, TextObjectType, UndoBehavior};
use crate::prompt::{PromptEditMode, PromptViMode};
use crate::{core_editor::get_local_clipboard, EditCommand};
use std::cmp::{max, min};
use std::ops::{DerefMut, Range};
#[cfg(feature = "helix")]
use unicode_segmentation::UnicodeSegmentation;

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
    selection_anchor: Option<usize>,
    selection_mode: Option<PromptEditMode>,
    edit_mode: PromptEditMode,
    #[cfg(feature = "helix")]
    hx_selection: Option<HxRange>,
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
            selection_anchor: None,
            selection_mode: None,
            edit_mode: PromptEditMode::Default,
            #[cfg(feature = "helix")]
            hx_selection: None,
        }
    }
}

// ── Helix selection range ─────────────────────────────────────────────

/// A single selection range with anchor and head.
///
/// Both `anchor` and `head` are byte offsets into the buffer.
/// The anchor is where the selection started; the head is where
/// the cursor currently sits. Uses gap indexing (left-inclusive,
/// right-exclusive), matching Helix semantics.
#[cfg(feature = "helix")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HxRange {
    pub(crate) anchor: usize,
    pub(crate) head: usize,
}

#[cfg(feature = "helix")]
impl HxRange {
    /// Ascending byte range for slicing/rendering.
    /// Returns (min, max) of anchor and head.
    #[must_use]
    pub fn range(&self) -> (usize, usize) {
        (min(self.head, self.anchor), max(self.head, self.anchor))
    }

    /// Clamp anchor and head so they lie on char boundaries within `buf`.
    /// Selections can become stale after buffer mutations (undo, delete, paste).
    pub fn clamp(&mut self, buf: &str) {
        let len = buf.len();
        self.anchor = Self::snap_to_char_boundary(buf, min(self.anchor, len));
        self.head = Self::snap_to_char_boundary(buf, min(self.head, len));
    }

    /// Round a byte offset down to the nearest char boundary in `buf`.
    fn snap_to_char_boundary(buf: &str, offset: usize) -> usize {
        if offset >= buf.len() {
            return buf.len();
        }
        // Walk backwards until we find a char boundary.
        let mut pos = offset;
        while !buf.is_char_boundary(pos) && pos > 0 {
            pos -= 1;
        }
        pos
    }

    /// Block cursor position: the byte offset where the cursor
    /// should be rendered. For a forward range the cursor sits
    /// one grapheme before head; for a backward or empty range
    /// it sits at head.
    #[must_use]
    pub fn cursor(&self, buf: &str) -> usize {
        // Clamp head to buffer length to avoid panic on stale selections.
        let head = min(self.head, buf.len());
        let anchor = min(self.anchor, buf.len());
        if head > anchor {
            buf[..head]
                .grapheme_indices(true)
                .next_back()
                .map_or(head, |(i, _)| i)
        } else {
            head
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
                self.move_to_position(*position, *select)
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
            EditCommand::InsertChar(c) => self.insert_char(*c),
            EditCommand::Complete => {}
            EditCommand::InsertString(str) => self.insert_str(str),
            EditCommand::InsertNewline => self.insert_newline(),
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
                    self.cut_buffer.set(copy_slice, ClipboardMode::Lines);
                }
            }
            EditCommand::CopyLeft => {
                let insertion_offset = self.line_buffer.insertion_point();
                if insertion_offset > 0 {
                    let left_index = self.line_buffer.grapheme_left_index();
                    let copy_range = left_index..insertion_offset;
                    self.cut_buffer.set(
                        &self.line_buffer.get_buffer()[copy_range],
                        ClipboardMode::Normal,
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
                        ClipboardMode::Normal,
                    );
                }
            }
            EditCommand::SwapCursorAndAnchor => self.swap_cursor_and_anchor(),
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
            #[cfg(feature = "helix")]
            EditCommand::HxRestartSelection => self.hx_restart_selection(),
            #[cfg(feature = "helix")]
            EditCommand::HxClearSelection => self.reset_hx_state(),
            #[cfg(feature = "helix")]
            EditCommand::HxEnsureSelection => self.hx_ensure_selection(),
            #[cfg(feature = "helix")]
            EditCommand::HxSyncCursor => self.hx_sync_cursor(),
            #[cfg(feature = "helix")]
            EditCommand::HxSyncCursorWithRestart => self.hx_sync_cursor_with_restart(),
            #[cfg(feature = "helix")]
            EditCommand::HxWordMotion {
                target,
                movement,
                count,
            } => self.hx_word_motion(*target, *movement, *count),
            #[cfg(feature = "helix")]
            EditCommand::HxFlipSelection => self.hx_flip_selection(),
            #[cfg(feature = "helix")]
            EditCommand::HxMoveToSelectionStart => self.hx_move_to_selection_start(),
            #[cfg(feature = "helix")]
            EditCommand::HxMoveToSelectionEnd => self.hx_move_to_selection_end(),
            #[cfg(feature = "helix")]
            EditCommand::HxSwitchCaseSelection => self.hx_switch_case_selection(),
            #[cfg(feature = "helix")]
            EditCommand::HxReplaceSelectionWithChar(c) => self.hx_replace_selection_with_char(*c),
            #[cfg(feature = "helix")]
            EditCommand::HxDeleteSelection => self.hx_delete_selection(),
            #[cfg(feature = "helix")]
            EditCommand::HxExtendSelectionToInsertionPoint => {
                self.hx_extend_selection_to_insertion_point()
            }
            #[cfg(feature = "helix")]
            EditCommand::HxShiftSelectionToInsertionPoint => {
                self.hx_shift_selection_to_insertion_point()
            }
        }
        if !matches!(command.edit_type(), EditType::MoveCursor { select: true }) {
            self.clear_selection();
        }

        // NoOp commands (e.g. Hx selection bookkeeping) must not touch the undo
        // stack at all — otherwise they corrupt the redo chain after an undo.
        if command.edit_type() == EditType::NoOp {
            return;
        }

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

    fn swap_cursor_and_anchor(&mut self) {
        if let Some(anchor) = self.selection_anchor {
            self.selection_anchor = Some(self.insertion_point());
            self.line_buffer.set_insertion_point(anchor);
        }
    }

    pub(crate) fn clear_selection(&mut self) {
        self.selection_anchor = None;
        self.selection_mode = None;
        // NOTE: hx_selection is NOT cleared here. Helix mode manages its own
        // selection lifecycle via reset_hx_state() / hx_restart_selection().
    }

    /// Disable Helix selection tracking (e.g. when switching away from Helix mode).
    #[cfg(feature = "helix")]
    pub(crate) fn reset_hx_state(&mut self) {
        self.hx_selection = None;
    }

    /// Reset the hx selection to a 1-grapheme-wide range at the current
    /// insertion point, matching Helix's invariant that selections always
    /// cover at least one grapheme.
    ///
    /// On an empty buffer, clears the selection instead (no grapheme to
    /// select).
    #[cfg(feature = "helix")]
    pub(crate) fn hx_restart_selection(&mut self) {
        if self.line_buffer.get_buffer().is_empty() {
            self.hx_selection = None;
            return;
        }
        let pos = self.insertion_point();
        let next = self.line_buffer.grapheme_right_index_from_pos(pos);
        self.hx_selection = Some(HxRange {
            anchor: pos,
            head: next,
        });
    }

    /// Ensure an hx selection exists.  If one already exists, leave it
    /// untouched; otherwise create a collapsed selection at the cursor.
    #[cfg(feature = "helix")]
    pub(crate) fn hx_ensure_selection(&mut self) {
        if self.hx_selection.is_none() {
            self.hx_restart_selection();
        }
    }

    /// Implements Helix `put_cursor` semantics for Select mode.
    ///
    /// Reads the current insertion point (where the motion landed), then:
    /// 1. Adjusts the anchor if the selection direction flipped
    ///    (Helix "1-width" block-cursor semantics).
    /// 2. Sets head: for forward selections, one grapheme past the
    ///    cursor position; for backward, at the cursor position.
    /// 3. Sets the insertion point to `sel.cursor()` for display.
    ///
    /// Motions start from `cursor()` (the display position), which is
    /// consistent with Helix's `move_horizontally`.
    #[cfg(feature = "helix")]
    pub(crate) fn hx_sync_cursor(&mut self) {
        let pos = self.insertion_point();
        if let Some(sel) = &mut self.hx_selection {
            let buf = self.line_buffer.get_buffer();
            sel.clamp(buf);

            // Anchor adjustment when direction flips.
            if sel.head >= sel.anchor && pos < sel.anchor {
                // Was forward, now backward: advance anchor by 1 grapheme.
                sel.anchor = buf[sel.anchor..]
                    .grapheme_indices(true)
                    .nth(1)
                    .map_or(buf.len(), |(i, _)| sel.anchor + i);
            } else if sel.head < sel.anchor && pos >= sel.anchor {
                // Was backward, now forward: retreat anchor by 1 grapheme.
                sel.anchor = buf[..sel.anchor]
                    .grapheme_indices(true)
                    .next_back()
                    .map_or(0, |(i, _)| i);
            }

            // Set head with 1-width semantics.
            if pos >= sel.anchor {
                sel.head = buf[pos..]
                    .grapheme_indices(true)
                    .nth(1)
                    .map_or(buf.len(), |(i, _)| pos + i);
            } else {
                sel.head = pos;
            }

            self.line_buffer.set_insertion_point(sel.cursor(buf));
        }
    }

    /// Atomic restart + sync for extending motions in Normal mode.
    ///
    /// Compares the current insertion point with the existing selection's
    /// display cursor.  If they differ (the motion moved), restarts the
    /// anchor at the old cursor position and syncs the head to the new
    /// position.  If they match (the motion was a no-op), the selection
    /// is left unchanged — matching Helix's `map_or(range, ...)` pattern.
    ///
    /// Direction-flip logic (as in `hx_sync_cursor`) is intentionally
    /// absent here: restart always creates a fresh anchor at the old cursor
    /// position, so the selection is either strictly forward (motion moved
    /// right) or strictly backward (motion moved left) — never a flip from
    /// a prior extended selection.
    #[cfg(feature = "helix")]
    pub(crate) fn hx_sync_cursor_with_restart(&mut self) {
        let pos = self.insertion_point();
        if let Some(sel) = &mut self.hx_selection {
            let buf = self.line_buffer.get_buffer();
            sel.clamp(buf);

            let old_cursor = sel.cursor(buf);
            if pos == old_cursor {
                // Motion made no progress — leave the selection as-is.
                return;
            }

            // Restart: collapse to a point at the old cursor, then extend to
            // the new position.  This matches Helix's
            //   Range::point(cursor).put_cursor(text, pos, true)
            // When the motion goes backward (pos < cursor), advance the
            // anchor by one grapheme so the cursor character is included.
            if pos < old_cursor {
                sel.anchor = buf[old_cursor..]
                    .grapheme_indices(true)
                    .nth(1)
                    .map_or(buf.len(), |(i, _)| old_cursor + i);
                sel.head = pos;
            } else {
                sel.anchor = old_cursor;
                sel.head = buf[pos..]
                    .grapheme_indices(true)
                    .nth(1)
                    .map_or(buf.len(), |(i, _)| pos + i);
            }

            self.line_buffer.set_insertion_point(sel.cursor(buf));
        } else {
            // No selection exists yet; create one.
            self.hx_restart_selection();
        }
    }

    /// Get a reference to the current Helix selection, if any.
    ///
    /// Exposes the full [`HxRange`] with anchor/head distinction, unlike
    /// [`get_selection`](Self::get_selection) which only returns `(min, max)`.
    #[cfg(feature = "helix")]
    #[allow(dead_code)] // available for internal use; currently used in tests
    pub(crate) fn hx_selection(&self) -> Option<&HxRange> {
        self.hx_selection.as_ref()
    }

    #[cfg(feature = "helix")]
    #[cfg(test)]
    pub(crate) fn set_hx_selection(&mut self, sel: HxRange) {
        self.hx_selection = Some(sel);
    }

    /// Run a word motion that takes and returns an HxRange.
    ///
    /// For `Movement::Move` (Normal mode): restarts the selection at the
    /// current cursor position before computing the motion.
    /// For `Movement::Extend` (Select mode): operates on the existing
    /// selection with anchor preserved.
    ///
    /// If the motion makes no progress (cursor position unchanged),
    /// the existing selection is preserved — matching Helix behavior
    /// where a failed motion at end-of-buffer keeps the last selection.
    #[cfg(feature = "helix")]
    fn hx_word_motion(
        &mut self,
        target: crate::enums::WordMotionTarget,
        movement: crate::enums::Movement,
        count: usize,
    ) {
        let buf = self.line_buffer.get_buffer();
        let pos = self.insertion_point();
        let next = self.line_buffer.grapheme_right_index_from_pos(pos);
        let mut sel = self.hx_selection.unwrap_or(HxRange {
            anchor: pos,
            head: next,
        });
        sel.clamp(buf);

        // For Move mode, restart selection at cursor before the motion.
        let input_sel = match movement {
            crate::enums::Movement::Move => {
                let cursor_pos = sel.cursor(buf);
                let cursor_next = self.line_buffer.grapheme_right_index_from_pos(cursor_pos);
                HxRange {
                    anchor: cursor_pos,
                    head: cursor_next,
                }
            }
            crate::enums::Movement::Extend => sel,
        };

        let new = word::word_move(buf, &input_sel, count, target);

        // If the cursor position didn't change, the motion made no
        // progress (e.g. at end-of-buffer). Keep the existing selection.
        if new.cursor(buf) == sel.cursor(buf) {
            return;
        }

        self.line_buffer.set_insertion_point(new.cursor(buf));
        self.hx_selection = Some(new);
    }

    /// Swap anchor and head of the Helix selection, updating the cursor.
    #[cfg(feature = "helix")]
    fn hx_flip_selection(&mut self) {
        if let Some(sel) = &mut self.hx_selection {
            sel.clamp(self.line_buffer.get_buffer());
            std::mem::swap(&mut sel.anchor, &mut sel.head);
            let buf = self.line_buffer.get_buffer();
            self.line_buffer.set_insertion_point(sel.cursor(buf));
        }
    }

    /// Move cursor to the ascending start of the Helix selection.
    #[cfg(feature = "helix")]
    fn hx_move_to_selection_start(&mut self) {
        if let Some(sel) = &mut self.hx_selection {
            sel.clamp(self.line_buffer.get_buffer());
            self.line_buffer.set_insertion_point(sel.range().0);
        }
    }

    /// Move cursor past the ascending end of the Helix selection.
    #[cfg(feature = "helix")]
    fn hx_move_to_selection_end(&mut self) {
        if let Some(sel) = &mut self.hx_selection {
            sel.clamp(self.line_buffer.get_buffer());
            self.line_buffer.set_insertion_point(sel.range().1);
        }
    }

    /// Transform the text inside the Helix selection, then update the
    /// selection to cover the (possibly resized) replacement.
    #[cfg(feature = "helix")]
    fn hx_transform_selection(&mut self, transform: impl FnOnce(&str) -> String) {
        if let Some(sel) = &mut self.hx_selection {
            sel.clamp(self.line_buffer.get_buffer());
            let (start, end) = sel.range();
            let selected = &self.line_buffer.get_buffer()[start..end];
            let result = transform(selected);
            let new_end = start + result.len();
            self.line_buffer.clear_range_safe(start..end);
            self.line_buffer.set_insertion_point(start);
            self.line_buffer.insert_str(&result);
            // Preserve selection direction over the new content.
            if sel.anchor <= sel.head {
                sel.anchor = start;
                sel.head = new_end;
            } else {
                sel.anchor = new_end;
                sel.head = start;
            }
            // Restore cursor to the display position within the updated selection.
            let buf = self.line_buffer.get_buffer();
            self.line_buffer.set_insertion_point(sel.cursor(buf));
        }
    }

    /// Toggle case of every character in the Helix selection.
    #[cfg(feature = "helix")]
    fn hx_switch_case_selection(&mut self) {
        self.hx_transform_selection(|selected| {
            selected
                .chars()
                .flat_map(|c| {
                    if c.is_lowercase() {
                        c.to_uppercase().collect::<Vec<_>>()
                    } else {
                        c.to_lowercase().collect::<Vec<_>>()
                    }
                })
                .collect()
        });
    }

    /// Replace every character in the Helix selection with the given char.
    /// Counts characters (not grapheme clusters) to match Helix behavior.
    #[cfg(feature = "helix")]
    fn hx_replace_selection_with_char(&mut self, c: char) {
        self.hx_transform_selection(|selected| {
            let char_count = selected.chars().count();
            std::iter::repeat(c).take(char_count).collect()
        });
    }

    /// Delete the Helix selection range without saving to the cut buffer.
    /// Clears hx_selection afterwards so subsequent commands see no selection.
    #[cfg(feature = "helix")]
    fn hx_delete_selection(&mut self) {
        if let Some(mut sel) = self.hx_selection.take() {
            sel.clamp(self.line_buffer.get_buffer());
            let (start, end) = sel.range();
            self.line_buffer.clear_range_safe(start..end);
            self.line_buffer.set_insertion_point(start);
        }
    }

    /// Extend the Helix selection head to the current insertion point.
    /// Used in `a` (append) insert mode so the selection grows as the
    /// user types. The anchor is pinned to the selection start (min side)
    /// and head tracks the cursor forward. This normalization ensures
    /// backward selections are handled correctly: a backward selection
    /// (anchor > head) is flipped so anchor holds the start before extending.
    #[cfg(feature = "helix")]
    fn hx_extend_selection_to_insertion_point(&mut self) {
        let pos = self.insertion_point();
        if let Some(sel) = &mut self.hx_selection {
            sel.anchor = sel.range().0;
            sel.head = pos;
        }
    }

    /// Shift both anchor and head of the Helix selection so the selection
    /// tracks the same text after an edit before it.
    ///
    /// Called after `InsertChar` or `Backspace` in `i` (insert-before) mode.
    /// The cursor sits at the selection start; after the edit it has moved
    /// forward (insert) or backward (backspace).  We compute the delta as
    /// `insertion_point − selection_start` and apply it to both ends.
    #[cfg(feature = "helix")]
    fn hx_shift_selection_to_insertion_point(&mut self) {
        let pos = self.insertion_point();
        if let Some(sel) = &mut self.hx_selection {
            let start = sel.range().0;
            if pos > start {
                let shift = pos - start;
                sel.anchor += shift;
                sel.head += shift;
            } else if pos < start {
                let shift = start - pos;
                sel.anchor = sel.anchor.saturating_sub(shift);
                sel.head = sel.head.saturating_sub(shift);
            }
        }
    }

    fn update_selection_anchor(&mut self, select: bool) {
        if select {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.insertion_point());
                self.selection_mode = Some(self.edit_mode.clone());
            }
        } else {
            self.clear_selection();
        }
    }

    /// Set the current edit mode
    pub fn set_edit_mode(&mut self, mode: PromptEditMode) {
        self.edit_mode = mode;
    }

    /// Check if the editor is currently in Helix edit mode.
    fn is_hx_mode(&self) -> bool {
        #[cfg(feature = "helix")]
        {
            matches!(self.edit_mode, PromptEditMode::Helix(_))
        }
        #[cfg(not(feature = "helix"))]
        {
            false
        }
    }
    fn move_to_position(&mut self, position: usize, select: bool) {
        self.update_selection_anchor(select);
        self.line_buffer.set_insertion_point(position)
    }

    pub(crate) fn move_line_up(&mut self, select: bool) {
        self.update_selection_anchor(select);
        self.line_buffer.move_line_up();
        self.update_undo_state(UndoBehavior::MoveCursor);
    }

    pub(crate) fn move_line_down(&mut self, select: bool) {
        self.update_selection_anchor(select);
        self.line_buffer.move_line_down();
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
        self.update_undo_state(undo_behavior);
    }

    pub(crate) fn insertion_point(&self) -> usize {
        self.line_buffer.insertion_point()
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
        self.line_buffer.insertion_point() == self.get_buffer().len()
    }

    pub(crate) fn reset_undo_stack(&mut self) {
        self.edit_stack.reset();
    }

    pub(crate) fn move_to_start(&mut self, select: bool) {
        self.update_selection_anchor(select);
        self.line_buffer.move_to_start();
    }

    pub(crate) fn move_to_end(&mut self, select: bool) {
        self.update_selection_anchor(select);
        self.line_buffer.move_to_end();
    }

    pub(crate) fn move_to_line_start(&mut self, select: bool) {
        self.update_selection_anchor(select);
        self.line_buffer.move_to_line_start();
    }

    pub(crate) fn move_to_line_non_blank_start(&mut self, select: bool) {
        self.update_selection_anchor(select);
        self.line_buffer.move_to_line_non_blank_start();
    }

    pub(crate) fn move_to_line_end(&mut self, select: bool) {
        self.update_selection_anchor(select);
        self.line_buffer.move_to_line_end();
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

    fn cut_current_line(&mut self) {
        let deletion_range = self.line_buffer.current_line_range();

        let cut_slice = &self.line_buffer.get_buffer()[deletion_range.clone()];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice, ClipboardMode::Lines);
            self.line_buffer.set_insertion_point(deletion_range.start);
            self.line_buffer.clear_range(deletion_range);
        }
    }

    fn cut_from_start(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        if insertion_offset > 0 {
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[..insertion_offset],
                ClipboardMode::Normal,
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
                ClipboardMode::Lines,
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
            self.cut_buffer.set(cut_slice, ClipboardMode::Normal);
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
            self.cut_buffer.set(cut_slice, ClipboardMode::Normal);
            self.line_buffer.clear_to_end();
        }
    }

    fn cut_from_end_linewise(&mut self, leave_blank_line: bool) {
        let start_offset = self.line_buffer.get_buffer()[..self.line_buffer.insertion_point()]
            .rfind('\n')
            .map_or(0, |offset| {
                // When leave_blank_line is true, we add 1 to the offset
                // So the \n character is not truncated
                if leave_blank_line {
                    offset + 1
                } else {
                    offset
                }
            });

        let cut_slice = &self.line_buffer.get_buffer()[start_offset..];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice, ClipboardMode::Lines);
            self.line_buffer.set_insertion_point(start_offset);
            self.line_buffer.clear_to_end();
        }
    }

    fn cut_to_line_end(&mut self) {
        let cut_slice = &self.line_buffer.get_buffer()
            [self.line_buffer.insertion_point()..self.line_buffer.find_current_line_end()];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice, ClipboardMode::Normal);
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
        let insertion_offset = self.line_buffer.insertion_point();
        let word_start = self.line_buffer.word_left_index();
        self.cut_range(word_start..insertion_offset);
    }

    fn cut_big_word_left(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let big_word_start = self.line_buffer.big_word_left_index();
        self.cut_range(big_word_start..insertion_offset);
    }

    fn cut_word_right(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let word_end = self.line_buffer.word_right_index();
        self.cut_range(insertion_offset..word_end);
    }

    fn cut_big_word_right(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let big_word_end = self.line_buffer.next_whitespace();
        self.cut_range(insertion_offset..big_word_end);
    }

    fn cut_word_right_to_next(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let next_word_start = self.line_buffer.word_right_start_index();
        self.cut_range(insertion_offset..next_word_start);
    }

    fn cut_big_word_right_to_next(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let next_big_word_start = self.line_buffer.big_word_right_start_index();
        self.cut_range(insertion_offset..next_big_word_start);
    }

    fn cut_char(&mut self) {
        if self.selection_anchor.is_some() {
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
        self.delete_selection();
        match self.cut_buffer.get() {
            (content, ClipboardMode::Normal) => {
                self.line_buffer.move_right();
                self.line_buffer.insert_str(&content);
            }
            (mut content, ClipboardMode::Lines) => {
                // TODO: Simplify that?
                self.line_buffer.move_to_line_start();
                self.line_buffer.move_line_down();
                if !content.ends_with('\n') {
                    // TODO: Make sure platform requirements are met
                    content.push('\n');
                }
                self.line_buffer.insert_str(&content);
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
        self.update_selection_anchor(select);
        if before_char {
            let skip = self.is_hx_mode();
            self.line_buffer.move_right_before(c, current_line, skip);
        } else {
            self.line_buffer.move_right_until(c, current_line);
        }
    }

    fn move_left_until_char(
        &mut self,
        c: char,
        before_char: bool,
        current_line: bool,
        select: bool,
    ) {
        self.update_selection_anchor(select);
        if before_char {
            let skip = self.is_hx_mode();
            self.line_buffer.move_left_before(c, current_line, skip);
        } else {
            self.line_buffer.move_left_until(c, current_line);
        }
    }

    fn cut_right_until_char(&mut self, c: char, before_char: bool, current_line: bool) {
        if let Some(index) = self.line_buffer.find_char_right(c, current_line) {
            // Saving the section of the string that will be deleted to be
            // stored into the buffer
            let extra = if before_char { 0 } else { c.len_utf8() };
            let cut_slice =
                &self.line_buffer.get_buffer()[self.line_buffer.insertion_point()..index + extra];

            if !cut_slice.is_empty() {
                self.cut_buffer.set(cut_slice, ClipboardMode::Normal);

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
                self.cut_buffer.set(cut_slice, ClipboardMode::Normal);

                if before_char {
                    self.line_buffer.delete_left_before_char(c, current_line);
                } else {
                    self.line_buffer.delete_left_until_char(c, current_line);
                }
            }
        }
    }

    fn replace_char(&mut self, character: char) {
        let insertion_point = self.line_buffer.insertion_point();
        self.line_buffer.delete_right_grapheme();

        self.line_buffer.insert_char(character);
        self.line_buffer.set_insertion_point(insertion_point);
    }

    fn replace_chars(&mut self, n_chars: usize, string: &str) {
        for _ in 0..n_chars {
            self.line_buffer.delete_right_grapheme();
        }

        self.line_buffer.insert_str(string);
    }

    fn move_left(&mut self, select: bool) {
        self.update_selection_anchor(select);
        self.line_buffer.move_left();
    }

    fn move_right(&mut self, select: bool) {
        self.update_selection_anchor(select);
        self.line_buffer.move_right();
    }

    fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.line_buffer.move_to_end();
    }

    #[cfg(feature = "system_clipboard")]
    fn cut_selection_to_system(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            let cut_slice = &self.line_buffer.get_buffer()[start..end];
            self.system_clipboard.set(cut_slice, ClipboardMode::Normal);
            self.cut_range(start..end);
            self.clear_selection();
            #[cfg(feature = "helix")]
            self.reset_hx_state();
        }
    }

    fn cut_selection_to_cut_buffer(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            self.cut_range(start..end);
            self.clear_selection();
            #[cfg(feature = "helix")]
            self.reset_hx_state();
        }
    }

    #[cfg(feature = "system_clipboard")]
    fn copy_selection_to_system(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            let cut_slice = &self.line_buffer.get_buffer()[start..end];
            self.system_clipboard.set(cut_slice, ClipboardMode::Normal);
        }
    }

    fn copy_selection_to_cut_buffer(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            let cut_slice = &self.line_buffer.get_buffer()[start..end];
            self.cut_buffer.set(cut_slice, ClipboardMode::Normal);
        }
    }

    /// If a selection is active returns the selected range, otherwise None.
    /// The range is guaranteed to be ascending.
    ///
    /// # Helix selection design note
    ///
    /// When `hx` is enabled and `hx_selection` is `Some`, this returns the
    /// hx range with priority. This is correct for **read-only** consumers
    /// (painter highlighting, `CopySelection`, `CutSelection`) but
    /// **`delete_selection()` deliberately ignores it** via the
    /// `selection_anchor.is_none()` early return. The reason: Helix keeps
    /// the selection visible as context during Insert mode — implicit
    /// deletion by `insert_char`/`backspace` etc. must not consume it.
    /// Only explicit Helix commands (`CutSelection`, `HxDeleteSelection`)
    /// should delete the hx-selected text.
    pub fn get_selection(&self) -> Option<(usize, usize)> {
        #[cfg(feature = "helix")]
        if let Some(sel) = &self.hx_selection {
            return Some(sel.range());
        }

        let selection_anchor = self.selection_anchor?;

        // Use the mode that was active when the selection was created, not the current mode
        let inclusive = matches!(
            self.selection_mode.as_ref().unwrap_or(&self.edit_mode),
            PromptEditMode::Vi(PromptViMode::Normal)
        );

        let selection_is_from_left_to_right = selection_anchor < self.insertion_point();

        let start_pos = if selection_is_from_left_to_right {
            selection_anchor
        } else {
            self.insertion_point()
        };

        let end_pos = if selection_is_from_left_to_right {
            if inclusive {
                self.line_buffer.grapheme_right_index()
            } else {
                self.insertion_point()
            }
        } else {
            // selection is from right to left
            if inclusive {
                self.line_buffer
                    .grapheme_right_index_from_pos(selection_anchor)
            } else {
                selection_anchor
            }
        };

        Some((start_pos, end_pos.min(self.line_buffer.len())))
    }

    fn delete_selection(&mut self) {
        // Only delete based on legacy selection_anchor (Vi visual mode,
        // Emacs mark).  Helix mode manages its own selection deletion via
        // explicit commands (CutSelection, HxDeleteSelection) — the
        // hx_selection must NOT be consumed implicitly by insert_char,
        // backspace, etc., because Helix keeps the selection visible as
        // context while typing in insert mode.
        if self.selection_anchor.is_none() {
            return;
        }
        if let Some((start, end)) = self.get_selection() {
            self.line_buffer.clear_range_safe(start..end);
            self.clear_selection();
            #[cfg(feature = "helix")]
            self.reset_hx_state();
        }
    }

    fn backspace(&mut self) {
        if self.selection_anchor.is_some() {
            self.delete_selection();
        } else {
            self.line_buffer.delete_left_grapheme();
        }
    }

    fn delete(&mut self) {
        if self.selection_anchor.is_some() {
            self.delete_selection();
        } else {
            self.line_buffer.delete_right_grapheme();
        }
    }

    fn move_word_left(&mut self, select: bool) {
        self.move_to_position(self.line_buffer.word_left_index(), select);
    }

    fn move_big_word_left(&mut self, select: bool) {
        self.move_to_position(self.line_buffer.big_word_left_index(), select);
    }

    fn move_word_right(&mut self, select: bool) {
        self.move_to_position(self.line_buffer.word_right_index(), select);
    }

    fn move_word_right_start(&mut self, select: bool) {
        self.move_to_position(self.line_buffer.word_right_start_index(), select);
    }

    fn move_big_word_right_start(&mut self, select: bool) {
        self.move_to_position(self.line_buffer.big_word_right_start_index(), select);
    }

    fn move_word_right_end(&mut self, select: bool) {
        self.move_to_position(self.line_buffer.word_right_end_index(), select);
    }

    fn move_big_word_right_end(&mut self, select: bool) {
        self.move_to_position(self.line_buffer.big_word_right_end_index(), select);
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
        if range.start <= range.end {
            self.copy_range(range.clone());
            self.line_buffer.clear_range_safe(range.clone());
            self.line_buffer.set_insertion_point(range.start);
        }
    }

    fn copy_range(&mut self, range: Range<usize>) {
        if range.start < range.end {
            let slice = &self.line_buffer.get_buffer()[range];
            self.cut_buffer.set(slice, ClipboardMode::Normal);
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
                ClipboardMode::Normal,
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
                ClipboardMode::Lines,
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
            self.cut_buffer.set(slice, ClipboardMode::Lines);
        }
    }

    pub(crate) fn copy_to_line_end(&mut self) {
        let copy_range =
            self.line_buffer.insertion_point()..self.line_buffer.find_current_line_end();
        self.copy_range(copy_range);
    }

    pub(crate) fn copy_word_left(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let word_start = self.line_buffer.word_left_index();
        self.copy_range(word_start..insertion_offset);
    }

    pub(crate) fn copy_big_word_left(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let big_word_start = self.line_buffer.big_word_left_index();
        self.copy_range(big_word_start..insertion_offset);
    }

    pub(crate) fn copy_word_right(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let word_end = self.line_buffer.word_right_index();
        self.copy_range(insertion_offset..word_end);
    }

    pub(crate) fn copy_big_word_right(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let big_word_end = self.line_buffer.next_whitespace();
        self.copy_range(insertion_offset..big_word_end);
    }

    pub(crate) fn copy_word_right_to_next(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let next_word_start = self.line_buffer.word_right_start_index();
        self.copy_range(insertion_offset..next_word_start);
    }

    pub(crate) fn copy_big_word_right_to_next(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let next_big_word_start = self.line_buffer.big_word_right_start_index();
        self.copy_range(insertion_offset..next_big_word_start);
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
        (content, ClipboardMode::Normal) => {
            line_buffer.insert_str(&content);
        }
        (mut content, ClipboardMode::Lines) => {
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
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn editor_with(buffer: &str) -> Editor {
        let mut editor = Editor::default();
        editor.set_buffer(buffer.to_string(), UndoBehavior::CreateUndoPoint);
        editor
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
        assert_eq!(editor.selection_anchor, Some(0));
        assert_eq!(editor.insertion_point(), 3);
        assert_eq!(editor.get_selection(), Some((0, 3)));

        editor.run_edit_command(&EditCommand::SwapCursorAndAnchor);
        assert_eq!(editor.selection_anchor, Some(3));
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.get_selection(), Some((0, 3)));

        editor.run_edit_command(&EditCommand::SwapCursorAndAnchor);
        assert_eq!(editor.selection_anchor, Some(0));
        assert_eq!(editor.insertion_point(), 3);
        assert_eq!(editor.get_selection(), Some((0, 3)));
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
        assert_eq!(editor.selection_anchor, Some(0));
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
        assert_eq!(editor.selection_anchor, Some(4));
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
            editor.selection_anchor = Some(editor.line_buffer.len());
            editor.line_buffer.set_insertion_point(0);
            editor.run_edit_command(&EditCommand::CutSelectionSystem);
            assert!(editor.line_buffer.get_buffer().is_empty());
        }
        #[test]
        fn test_copypaste_selection_system() {
            let s = "This is a test!";
            let mut editor = editor_with(s);
            editor.selection_anchor = Some(editor.line_buffer.len());
            editor.line_buffer.set_insertion_point(0);
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
        assert_eq!(editor.selection_anchor, Some(0));
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

    #[cfg(feature = "helix")]
    mod hx_selection_tests {
        use super::{editor_with, HxRange};
        use crate::prompt::{PromptEditMode, PromptHelixMode};
        use crate::EditCommand;
        use pretty_assertions::assert_eq;

        #[test]
        fn restart_sets_anchor_and_head() {
            let mut editor = editor_with("hello world");
            editor.line_buffer.set_insertion_point(3);
            editor.hx_restart_selection();
            let sel = editor.hx_selection().unwrap();
            assert_eq!(sel.anchor, 3);
            // 1-grapheme-wide: head is one grapheme past anchor
            assert_eq!(sel.head, 4);
            assert_eq!(editor.get_selection(), Some((3, 4)));
        }

        #[test]
        fn sync_cursor_extends_selection_forward() {
            let mut editor = editor_with("hello world");
            editor.line_buffer.set_insertion_point(0);
            editor.hx_restart_selection();
            // Simulate a motion landing at position 5
            editor.line_buffer.set_insertion_point(5);
            editor.hx_sync_cursor();
            let sel = editor.hx_selection().unwrap();
            assert_eq!(sel.anchor, 0);
            // Forward range: head is one grapheme past cursor (1-width semantics)
            assert_eq!(sel.head, 6);
            assert_eq!(editor.get_selection(), Some((0, 6)));
        }

        #[test]
        fn restart_after_sync_resets_both() {
            let mut editor = editor_with("hello world");
            editor.line_buffer.set_insertion_point(0);
            editor.hx_restart_selection();
            editor.line_buffer.set_insertion_point(5);
            editor.hx_sync_cursor();
            // Now restart at the cursor display position
            editor.hx_restart_selection();
            let sel = editor.hx_selection().unwrap();
            assert_eq!(sel.anchor, 5);
            // 1-grapheme-wide: head is one grapheme past anchor
            assert_eq!(sel.head, 6);
        }

        #[test]
        fn sync_cursor_backward_selection() {
            let mut editor = editor_with("hello world");
            editor.line_buffer.set_insertion_point(5);
            editor.hx_restart_selection();
            // Simulate a motion landing at position 2
            editor.line_buffer.set_insertion_point(2);
            editor.hx_sync_cursor();
            let sel = editor.hx_selection().unwrap();
            // Backward: anchor shifts forward by 1 grapheme (direction flip)
            assert_eq!(sel.anchor, 6);
            assert_eq!(sel.head, 2);
            assert!(sel.head < sel.anchor);
        }

        #[test]
        fn reset_hx_state_clears_hx() {
            let mut editor = editor_with("hello");
            editor.line_buffer.set_insertion_point(0);
            editor.hx_restart_selection();
            editor.reset_hx_state();
            assert!(editor.hx_selection().is_none());
            assert_eq!(editor.get_selection(), None);
        }

        #[test]
        fn hx_selection_takes_priority_over_legacy() {
            let mut editor = editor_with("hello world");
            // Set legacy selection
            editor.selection_anchor = Some(0);
            editor.line_buffer.set_insertion_point(3);
            // Set hx selection to different range
            editor.hx_selection = Some(HxRange { anchor: 1, head: 4 });
            // hx_selection should win
            assert_eq!(editor.get_selection(), Some((1, 4)));
        }

        #[test]
        fn ensure_selection_creates_when_none() {
            let mut editor = editor_with("hello");
            editor.line_buffer.set_insertion_point(2);
            assert!(editor.hx_selection().is_none());
            editor.hx_ensure_selection();
            let sel = editor.hx_selection().unwrap();
            assert_eq!(sel.anchor, 2);
            assert_eq!(sel.head, 3);
        }

        #[test]
        fn ensure_selection_preserves_existing() {
            let mut editor = editor_with("hello world");
            editor.hx_selection = Some(HxRange { anchor: 0, head: 5 });
            editor.hx_ensure_selection();
            let sel = editor.hx_selection().unwrap();
            assert_eq!((sel.anchor, sel.head), (0, 5));
        }

        #[test]
        fn flip_selection_swaps_anchor_and_head() {
            let mut editor = editor_with("hello world");
            editor.hx_selection = Some(HxRange { anchor: 0, head: 5 });
            editor.hx_flip_selection();
            let sel = editor.hx_selection().unwrap();
            assert_eq!((sel.anchor, sel.head), (5, 0));
        }

        #[test]
        fn move_to_selection_start_forward() {
            let mut editor = editor_with("hello world");
            editor.line_buffer.set_insertion_point(8);
            editor.hx_selection = Some(HxRange { anchor: 2, head: 7 });
            editor.hx_move_to_selection_start();
            assert_eq!(editor.insertion_point(), 2);
        }

        #[test]
        fn move_to_selection_start_backward() {
            let mut editor = editor_with("hello world");
            editor.line_buffer.set_insertion_point(0);
            editor.hx_selection = Some(HxRange { anchor: 7, head: 2 });
            editor.hx_move_to_selection_start();
            assert_eq!(editor.insertion_point(), 2);
        }

        #[test]
        fn move_to_selection_end_forward() {
            let mut editor = editor_with("hello world");
            editor.line_buffer.set_insertion_point(0);
            editor.hx_selection = Some(HxRange { anchor: 2, head: 7 });
            editor.hx_move_to_selection_end();
            assert_eq!(editor.insertion_point(), 7);
        }

        #[test]
        fn switch_case_toggles() {
            let mut editor = editor_with("Hello");
            editor.hx_selection = Some(HxRange { anchor: 0, head: 5 });
            editor.hx_switch_case_selection();
            assert_eq!(editor.get_buffer(), "hELLO");
            // Selection should still cover the full word
            let sel = editor.hx_selection().unwrap();
            assert_eq!((sel.anchor, sel.head), (0, 5));
        }

        #[test]
        fn replace_selection_with_char() {
            let mut editor = editor_with("hello world");
            editor.hx_selection = Some(HxRange { anchor: 0, head: 5 });
            editor.hx_replace_selection_with_char('x');
            assert_eq!(editor.get_buffer(), "xxxxx world");
            let sel = editor.hx_selection().unwrap();
            assert_eq!((sel.anchor, sel.head), (0, 5));
        }

        #[test]
        fn delete_selection_removes_range() {
            let mut editor = editor_with("hello world");
            editor.hx_selection = Some(HxRange { anchor: 0, head: 6 });
            editor.hx_delete_selection();
            assert_eq!(editor.get_buffer(), "world");
            assert_eq!(editor.insertion_point(), 0);
            assert!(editor.hx_selection().is_none());
        }

        #[test]
        fn delete_selection_backward_range() {
            let mut editor = editor_with("hello world");
            editor.hx_selection = Some(HxRange { anchor: 6, head: 0 });
            editor.hx_delete_selection();
            assert_eq!(editor.get_buffer(), "world");
            assert_eq!(editor.insertion_point(), 0);
        }

        #[test]
        fn restart_on_empty_buffer_clears() {
            let mut editor = editor_with("");
            editor.hx_restart_selection();
            assert!(editor.hx_selection().is_none());
        }

        // ── sync_cursor direction flip tests ────────────────────────────

        #[test]
        fn sync_cursor_backward_to_forward_flip() {
            // Start with backward selection (anchor > head), then move forward past anchor.
            let mut editor = editor_with("hello world");
            editor.line_buffer.set_insertion_point(5);
            editor.hx_restart_selection();
            // Force backward: simulate motion landing at 2
            editor.line_buffer.set_insertion_point(2);
            editor.hx_sync_cursor();
            let sel = editor.hx_selection().unwrap();
            assert!(sel.head < sel.anchor, "should be backward");

            // Now simulate motion landing past the (adjusted) anchor
            editor.line_buffer.set_insertion_point(8);
            editor.hx_sync_cursor();
            let sel = editor.hx_selection().unwrap();
            assert!(
                sel.head > sel.anchor,
                "should flip to forward: anchor={} head={}",
                sel.anchor,
                sel.head
            );
        }

        // ── transform_selection with length change ──────────────────────

        #[test]
        fn replace_selection_with_char_multibyte_shrinks() {
            // Replace multi-byte chars with single-byte — selection should shrink.
            let mut editor = editor_with("café world");
            // "café" = bytes [0..5) (é is 2 bytes), 4 chars
            editor.hx_selection = Some(HxRange { anchor: 0, head: 5 });
            editor.hx_replace_selection_with_char('x');
            // 4 chars → 4 'x' = 4 bytes
            assert_eq!(editor.get_buffer(), "xxxx world");
            let sel = editor.hx_selection().unwrap();
            assert_eq!((sel.anchor, sel.head), (0, 4));
        }

        #[test]
        fn replace_selection_with_multibyte_char_expands() {
            // Replace ASCII selection with a multi-byte char — selection should expand.
            let mut editor = editor_with("hello world");
            editor.hx_selection = Some(HxRange { anchor: 0, head: 5 });
            // 'é' is 2 bytes; 5 chars × 2 bytes = 10 bytes
            editor.hx_replace_selection_with_char('é');
            assert_eq!(editor.get_buffer(), "ééééé world");
            let sel = editor.hx_selection().unwrap();
            assert_eq!((sel.anchor, sel.head), (0, 10));
        }

        #[test]
        fn replace_counts_chars_not_graphemes() {
            // "e\u{0301}" is 2 chars (e + combining acute) but 1 grapheme cluster.
            // Replace should count chars, matching Helix behavior.
            let mut editor = editor_with("e\u{0301}!");
            // "e\u{0301}" = 3 bytes (e=1, combining=2), "!" = 1 byte → total 4 bytes
            // Selection covers "e\u{0301}!" = 4 bytes, 3 chars
            editor.hx_selection = Some(HxRange { anchor: 0, head: 4 });
            editor.hx_replace_selection_with_char('x');
            // 3 chars → "xxx" = 3 bytes
            assert_eq!(editor.get_buffer(), "xxx");
            let sel = editor.hx_selection().unwrap();
            assert_eq!((sel.anchor, sel.head), (0, 3));
        }

        #[test]
        fn switch_case_preserves_multibyte() {
            let mut editor = editor_with("café");
            editor.hx_selection = Some(HxRange { anchor: 0, head: 5 });
            editor.hx_switch_case_selection();
            assert_eq!(editor.get_buffer(), "CAFÉ");
            let sel = editor.hx_selection().unwrap();
            // É is also 2 bytes, so length stays the same
            assert_eq!((sel.anchor, sel.head), (0, 5));
        }

        // ── Full edit sequences through run_edit_command ────────────────

        #[test]
        fn word_motion_then_delete() {
            use crate::enums::{Movement, WordMotionTarget};
            let mut editor = editor_with("hello world test");
            editor.line_buffer.set_insertion_point(0);
            // w: select "hello " (Normal mode = Move)
            editor.run_edit_command(&EditCommand::HxWordMotion {
                target: WordMotionTarget::NextWordStart,
                movement: Movement::Move,
                count: 1,
            });
            let sel = editor.hx_selection().unwrap();
            let selected = &editor.get_buffer()[sel.range().0..sel.range().1];
            assert_eq!(selected, "hello ");
            // d: cut selection
            editor.run_edit_command(&EditCommand::CutSelection);
            assert_eq!(editor.get_buffer(), "world test");
        }

        #[test]
        fn two_word_motions_then_delete() {
            use crate::enums::{Movement, WordMotionTarget};
            let mut editor = editor_with("aaa bbb ccc ddd");
            editor.line_buffer.set_insertion_point(0);
            // First w: select "aaa "
            editor.run_edit_command(&EditCommand::HxWordMotion {
                target: WordMotionTarget::NextWordStart,
                movement: Movement::Move,
                count: 1,
            });
            // Second w: restart and select "bbb "
            editor.run_edit_command(&EditCommand::HxWordMotion {
                target: WordMotionTarget::NextWordStart,
                movement: Movement::Move,
                count: 1,
            });
            let sel = editor.hx_selection().unwrap();
            let selected = &editor.get_buffer()[sel.range().0..sel.range().1];
            assert_eq!(selected, "bbb ");
            // d: cut "bbb "
            editor.run_edit_command(&EditCommand::CutSelection);
            assert_eq!(editor.get_buffer(), "aaa ccc ddd");
        }

        #[test]
        fn word_motion_with_count_2() {
            use crate::enums::{Movement, WordMotionTarget};
            let mut editor = editor_with("aaa bbb ccc ddd");
            editor.line_buffer.set_insertion_point(0);
            // 2w: skip two words at once
            editor.run_edit_command(&EditCommand::HxWordMotion {
                target: WordMotionTarget::NextWordStart,
                movement: Movement::Move,
                count: 2,
            });
            let sel = editor.hx_selection().unwrap();
            let selected = &editor.get_buffer()[sel.range().0..sel.range().1];
            // Each count iteration in word_right restarts the anchor at the
            // new word boundary (Helix semantics), so 2w selects only the
            // second word span, not both words from the origin.
            assert_eq!(selected, "bbb ");
        }

        // ── h/l motions through run_edit_command ────────────────────────

        #[test]
        fn h_motion_moves_left_and_restarts() {
            let mut editor = editor_with("hello");
            editor.line_buffer.set_insertion_point(3);
            editor.hx_restart_selection();
            // Simulate Normal mode: MoveLeft then HxRestartSelection
            editor.run_edit_command(&EditCommand::MoveLeft { select: false });
            editor.run_edit_command(&EditCommand::HxRestartSelection);
            assert_eq!(editor.insertion_point(), 2);
            let sel = editor.hx_selection().unwrap();
            // 1-wide selection at position 2
            assert_eq!(sel.anchor, 2);
            assert_eq!(sel.head, 3);
        }

        #[test]
        fn l_motion_moves_right_and_restarts() {
            let mut editor = editor_with("hello");
            editor.line_buffer.set_insertion_point(1);
            editor.hx_restart_selection();
            // Simulate Normal mode: MoveRight then HxRestartSelection
            editor.run_edit_command(&EditCommand::MoveRight { select: false });
            editor.run_edit_command(&EditCommand::HxRestartSelection);
            assert_eq!(editor.insertion_point(), 2);
            let sel = editor.hx_selection().unwrap();
            assert_eq!(sel.anchor, 2);
            assert_eq!(sel.head, 3);
        }

        // ── f/t motions through run_edit_command ────────────────────────

        #[test]
        fn f_motion_extends_to_char() {
            let mut editor = editor_with("hello world");
            editor.line_buffer.set_insertion_point(0);
            editor.run_edit_command(&EditCommand::HxRestartSelection);
            // Simulate Normal mode f w: MoveRightUntil + HxSyncCursorWithRestart
            editor.run_edit_command(&EditCommand::MoveRightUntil {
                c: 'w',
                select: false,
            });
            editor.run_edit_command(&EditCommand::HxSyncCursorWithRestart);
            let sel = editor.hx_selection().unwrap();
            // Forward selection from 0 to past 'w' (byte 6 + 1 = 7)
            assert_eq!(sel.anchor, 0);
            assert!(sel.head > 6, "head should be past 'w': head={}", sel.head);
        }

        #[test]
        fn t_motion_stops_before_char() {
            let mut editor = editor_with("hello world");
            editor.line_buffer.set_insertion_point(0);
            editor.run_edit_command(&EditCommand::HxRestartSelection);
            // Simulate Normal mode t w: MoveRightBefore + HxSyncCursorWithRestart
            editor.run_edit_command(&EditCommand::MoveRightBefore {
                c: 'w',
                select: false,
            });
            editor.run_edit_command(&EditCommand::HxSyncCursorWithRestart);
            let sel = editor.hx_selection().unwrap();
            // Should stop before 'w' (at the space, byte 5)
            assert_eq!(sel.anchor, 0);
            // Cursor should be before 'w'
            let cursor = sel.cursor(editor.get_buffer());
            assert!(cursor < 6, "cursor should be before 'w': cursor={}", cursor);
        }

        #[test]
        fn t_motion_twice_preserves_selection() {
            let mut editor = editor_with("hello world");
            editor.line_buffer.set_insertion_point(0);
            editor.run_edit_command(&EditCommand::HxRestartSelection);
            // First t w — extending motion uses HxSyncCursorWithRestart
            editor.run_edit_command(&EditCommand::MoveRightBefore {
                c: 'w',
                select: false,
            });
            editor.run_edit_command(&EditCommand::HxSyncCursorWithRestart);

            let sel_after_first = *editor.hx_selection().unwrap();

            // Second t w — should NOT collapse the selection
            editor.run_edit_command(&EditCommand::MoveRightBefore {
                c: 'w',
                select: false,
            });
            editor.run_edit_command(&EditCommand::HxSyncCursorWithRestart);

            let sel_after_second = *editor.hx_selection().unwrap();
            assert_eq!(
                sel_after_first.anchor, sel_after_second.anchor,
                "anchor should not change on repeat t: first={:?}, second={:?}",
                sel_after_first, sel_after_second
            );
            assert_eq!(
                sel_after_first.head, sel_after_second.head,
                "head should not change on repeat t: first={:?}, second={:?}",
                sel_after_first, sel_after_second
            );
        }

        #[test]
        fn t_then_reverse_t_restarts_correctly() {
            // "abcabc": cursor at 0, selection [a] = {anchor:0, head:1}.
            // `ta` restarts: anchor = old cursor (0), extends to before
            // second 'a' → {0, 3}, cursor on 'c' at 2.
            // `Ta` restarts: anchor = old cursor (2), backward → anchor
            // advances to 3 (direction flip), head = 1.
            // Selection = {3, 1}, covering "bc" backward.
            let mut editor = editor_with("abcabc");
            editor.line_buffer.set_insertion_point(0);
            editor.run_edit_command(&EditCommand::HxRestartSelection);

            // Forward `ta`
            editor.run_edit_command(&EditCommand::MoveRightBefore {
                c: 'a',
                select: false,
            });
            editor.run_edit_command(&EditCommand::HxSyncCursorWithRestart);
            let sel = *editor.hx_selection().unwrap();
            assert_eq!((sel.anchor, sel.head), (0, 3), "after ta");

            // Backward `Ta`
            editor.run_edit_command(&EditCommand::MoveLeftBefore {
                c: 'a',
                select: false,
            });
            editor.run_edit_command(&EditCommand::HxSyncCursorWithRestart);
            let sel = *editor.hx_selection().unwrap();
            // Anchor restarts at old cursor (2), then flips to 3 because
            // the motion went backward. Head = 1 (after 'a' at 0).
            assert_eq!(
                (sel.anchor, sel.head),
                (3, 1),
                "anchor should flip to 3, head at 1"
            );
        }

        #[test]
        fn t_motion_advances_to_next_occurrence() {
            // "axbxc": cursor at 0, next grapheme is 'x'.
            // Helix t x skips the immediate 'x' and stops before
            // the second 'x' (byte 2, before byte 3).
            let mut editor = editor_with("axbxc");
            editor.set_edit_mode(PromptEditMode::Helix(PromptHelixMode::Normal));
            editor.line_buffer.set_insertion_point(0);
            editor.run_edit_command(&EditCommand::HxRestartSelection);

            // t x from pos 0: skip immediate 'x' at 1, find 'x' at 3,
            // stop before it → cursor at 2.
            editor.run_edit_command(&EditCommand::MoveRightBefore {
                c: 'x',
                select: false,
            });
            editor.run_edit_command(&EditCommand::HxSyncCursorWithRestart);
            let sel = *editor.hx_selection().unwrap();
            // Selection from 0 to byte 3 (just past 'b')
            assert_eq!(sel.anchor, 0);
            assert_eq!(sel.head, 3);

            // Second t x: cursor is at 2 ('b'), next grapheme is 'x' at 3.
            // Skip that 'x', no third 'x' → no movement → selection preserved.
            editor.run_edit_command(&EditCommand::MoveRightBefore {
                c: 'x',
                select: false,
            });
            editor.run_edit_command(&EditCommand::HxSyncCursorWithRestart);
            let sel2 = *editor.hx_selection().unwrap();
            assert_eq!(sel2.anchor, sel.anchor, "selection should be preserved");
            assert_eq!(sel2.head, sel.head, "selection should be preserved");
        }

        // ── sync_cursor_with_restart when no prior selection ─────────

        #[test]
        fn sync_cursor_with_restart_creates_fresh_selection() {
            // No prior hx_selection; should create a 1-wide selection at cursor.
            let mut editor = editor_with("hello");
            editor.line_buffer.set_insertion_point(2);
            assert!(editor.hx_selection().is_none());

            editor.run_edit_command(&EditCommand::HxSyncCursorWithRestart);
            let sel = editor.hx_selection().unwrap();
            // Fresh restart creates a 1-wide selection: anchor=2, head=3.
            assert_eq!(sel.anchor, 2);
            assert_eq!(sel.head, 3);
        }

        // ── SelectionAdjustment::Shifting (`i`) tracking ───────────

        #[test]
        fn i_mode_shift_tracks_insertion() {
            // Simulate: 'i' mode — type 'xy' before selection [w]orld
            // The selection [w] (anchor=0, head=1) should shift to (2, 3).
            let mut editor = editor_with("world");
            editor.line_buffer.set_insertion_point(0);
            editor.run_edit_command(&EditCommand::HxRestartSelection);
            let sel0 = *editor.hx_selection().unwrap();
            assert_eq!((sel0.anchor, sel0.head), (0, 1));

            // Move to selection start (i mode).
            editor.run_edit_command(&EditCommand::HxMoveToSelectionStart);

            // Insert 'x', then shift.
            editor.run_edit_command(&EditCommand::InsertChar('x'));
            editor.run_edit_command(&EditCommand::HxShiftSelectionToInsertionPoint);
            let sel1 = *editor.hx_selection().unwrap();
            assert_eq!((sel1.anchor, sel1.head), (1, 2));

            // Insert 'y', then shift.
            editor.run_edit_command(&EditCommand::InsertChar('y'));
            editor.run_edit_command(&EditCommand::HxShiftSelectionToInsertionPoint);
            let sel2 = *editor.hx_selection().unwrap();
            assert_eq!((sel2.anchor, sel2.head), (2, 3));

            // Buffer should be "xyworld".
            assert_eq!(editor.get_buffer(), "xyworld");
        }

        #[test]
        fn i_mode_shift_tracks_backspace() {
            // Simulate: 'i' mode — type 'xy' then backspace once.
            // Start: "world", selection [w] at (0, 1).
            let mut editor = editor_with("world");
            editor.line_buffer.set_insertion_point(0);
            editor.run_edit_command(&EditCommand::HxRestartSelection);

            // Move to selection start (i mode), insert 'x', 'y'.
            editor.run_edit_command(&EditCommand::HxMoveToSelectionStart);
            editor.run_edit_command(&EditCommand::InsertChar('x'));
            editor.run_edit_command(&EditCommand::HxShiftSelectionToInsertionPoint);
            editor.run_edit_command(&EditCommand::InsertChar('y'));
            editor.run_edit_command(&EditCommand::HxShiftSelectionToInsertionPoint);
            // Selection is now (2, 3) covering 'w' in "xyworld".
            let sel = *editor.hx_selection().unwrap();
            assert_eq!((sel.anchor, sel.head), (2, 3));

            // Backspace: deletes 'y', cursor goes from 2 to 1.
            editor.run_edit_command(&EditCommand::Backspace);
            editor.run_edit_command(&EditCommand::HxShiftSelectionToInsertionPoint);
            let sel = *editor.hx_selection().unwrap();
            assert_eq!((sel.anchor, sel.head), (1, 2));
            assert_eq!(editor.get_buffer(), "xworld");
        }

        #[test]
        fn i_mode_escape_keeps_cursor_after_inserted_text() {
            let mut editor = editor_with("world");
            editor.set_edit_mode(PromptEditMode::Helix(PromptHelixMode::Insert));
            editor.line_buffer.set_insertion_point(0);
            editor.run_edit_command(&EditCommand::HxRestartSelection);

            editor.run_edit_command(&EditCommand::HxMoveToSelectionStart);
            editor.run_edit_command(&EditCommand::InsertChar('x'));
            editor.run_edit_command(&EditCommand::HxShiftSelectionToInsertionPoint);
            editor.run_edit_command(&EditCommand::InsertChar('y'));
            editor.run_edit_command(&EditCommand::HxShiftSelectionToInsertionPoint);

            assert_eq!(editor.get_buffer(), "xyworld");
            assert_eq!(editor.insertion_point(), 2);

            editor.run_edit_command(&EditCommand::HxEnsureSelection);

            assert_eq!(editor.get_buffer(), "xyworld");
            assert_eq!(editor.insertion_point(), 2);

            let sel = *editor.hx_selection().unwrap();
            assert_eq!((sel.anchor, sel.head), (2, 3));
        }

        #[test]
        fn a_mode_escape_preserves_entire_selection() {
            let mut editor = editor_with("hello");
            editor.set_edit_mode(PromptEditMode::Helix(PromptHelixMode::Insert));
            editor.line_buffer.set_insertion_point(0);
            editor.run_edit_command(&EditCommand::HxRestartSelection);

            editor.run_edit_command(&EditCommand::HxMoveToSelectionEnd);
            editor.run_edit_command(&EditCommand::InsertChar('x'));
            editor.run_edit_command(&EditCommand::HxExtendSelectionToInsertionPoint);
            editor.run_edit_command(&EditCommand::InsertChar('y'));
            editor.run_edit_command(&EditCommand::HxExtendSelectionToInsertionPoint);

            assert_eq!(editor.get_buffer(), "hxyello");
            assert_eq!(editor.insertion_point(), 3);

            let before_escape = *editor.hx_selection().unwrap();
            assert_eq!((before_escape.anchor, before_escape.head), (0, 3));

            editor.run_edit_command(&EditCommand::HxEnsureSelection);

            assert_eq!(editor.get_buffer(), "hxyello");
            assert_eq!(editor.insertion_point(), 3);

            let after_escape = *editor.hx_selection().unwrap();
            assert_eq!(after_escape, before_escape);
        }

        #[test]
        fn a_mode_extend_tracks_backspace() {
            // Start: "hello", selection [h] at (0, 1).
            let mut editor = editor_with("hello");
            editor.line_buffer.set_insertion_point(0);
            editor.run_edit_command(&EditCommand::HxRestartSelection);

            // Move to selection end (a mode), insert 'x', 'y'.
            editor.run_edit_command(&EditCommand::HxMoveToSelectionEnd);
            editor.run_edit_command(&EditCommand::InsertChar('x'));
            editor.run_edit_command(&EditCommand::HxExtendSelectionToInsertionPoint);
            editor.run_edit_command(&EditCommand::InsertChar('y'));
            editor.run_edit_command(&EditCommand::HxExtendSelectionToInsertionPoint);
            // Selection is (0, 3) covering "hxy" in "hxyello".
            let sel = *editor.hx_selection().unwrap();
            assert_eq!((sel.anchor, sel.head), (0, 3));

            // Backspace: deletes 'y', cursor from 3 to 2.
            editor.run_edit_command(&EditCommand::Backspace);
            editor.run_edit_command(&EditCommand::HxExtendSelectionToInsertionPoint);
            let sel = *editor.hx_selection().unwrap();
            assert_eq!((sel.anchor, sel.head), (0, 2));
            assert_eq!(editor.get_buffer(), "hxello");
        }

        // ── SelectionAdjustment::Anchored (`a`) tracking ───────────

        #[test]
        fn a_mode_extend_tracks_insertion() {
            // Simulate: 'a' mode — type 'xy' after selection [h]ello
            // The selection should extend from (0, 1) → (0, 2) → (0, 3).
            let mut editor = editor_with("hello");
            editor.line_buffer.set_insertion_point(0);
            editor.run_edit_command(&EditCommand::HxRestartSelection);

            // Move to selection end (a mode).
            editor.run_edit_command(&EditCommand::HxMoveToSelectionEnd);

            // Insert 'x', then extend.
            editor.run_edit_command(&EditCommand::InsertChar('x'));
            editor.run_edit_command(&EditCommand::HxExtendSelectionToInsertionPoint);
            let sel1 = *editor.hx_selection().unwrap();
            assert_eq!((sel1.anchor, sel1.head), (0, 2));

            // Insert 'y', then extend.
            editor.run_edit_command(&EditCommand::InsertChar('y'));
            editor.run_edit_command(&EditCommand::HxExtendSelectionToInsertionPoint);
            let sel2 = *editor.hx_selection().unwrap();
            assert_eq!((sel2.anchor, sel2.head), (0, 3));

            // Buffer should be "hxyello".
            assert_eq!(editor.get_buffer(), "hxyello");
        }

        #[test]
        fn a_mode_extend_normalizes_backward_selection() {
            // Start with a backward selection: anchor=5, head=2.
            // After 'a' mode extend, anchor should be normalized to min (2).
            let mut editor = editor_with("hello world");
            editor.set_hx_selection(HxRange { anchor: 5, head: 2 });

            // Move to selection end (range().1 = 5).
            editor.run_edit_command(&EditCommand::HxMoveToSelectionEnd);
            assert_eq!(editor.insertion_point(), 5);

            // Insert 'x', then extend.
            editor.run_edit_command(&EditCommand::InsertChar('x'));
            editor.run_edit_command(&EditCommand::HxExtendSelectionToInsertionPoint);
            let sel = *editor.hx_selection().unwrap();
            // anchor should be normalized to 2 (min), head at 6 (insertion point).
            assert_eq!((sel.anchor, sel.head), (2, 6));
        }
    }
}
