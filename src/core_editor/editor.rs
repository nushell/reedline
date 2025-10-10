use super::{edit_stack::EditStack, Clipboard, ClipboardMode, LineBuffer};
#[cfg(feature = "system_clipboard")]
use crate::core_editor::get_system_clipboard;
use crate::enums::{EditType, TextObject, TextObjectScope, TextObjectType, UndoBehavior};
use crate::{core_editor::get_local_clipboard, EditCommand};
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
    selection_anchor: Option<usize>,
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
            EditCommand::CutFromLineStart => self.cut_from_line_start(),
            EditCommand::CutFromLineNonBlankStart => self.cut_from_line_non_blank_start(),
            EditCommand::CutToEnd => self.cut_from_end(),
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
            EditCommand::CopyFromLineStart => self.copy_from_line_start(),
            EditCommand::CopyFromLineNonBlankStart => self.copy_from_line_non_blank_start(),
            EditCommand::CopyToEnd => self.copy_from_end(),
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
        }
        if !matches!(command.edit_type(), EditType::MoveCursor { select: true }) {
            self.selection_anchor = None;
        }
        if let EditType::MoveCursor { select: true } = command.edit_type() {}

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
            (_, EditType::UndoRedo) => UndoBehavior::UndoRedo,
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

    fn update_selection_anchor(&mut self, select: bool) {
        self.selection_anchor = if select {
            self.selection_anchor
                .or_else(|| Some(self.insertion_point()))
        } else {
            None
        };
    }
    fn move_to_position(&mut self, position: usize, select: bool) {
        self.update_selection_anchor(select);
        self.line_buffer.set_insertion_point(position)
    }

    pub(crate) fn move_line_up(&mut self) {
        self.line_buffer.move_line_up();
        self.update_undo_state(UndoBehavior::MoveCursor);
    }

    pub(crate) fn move_line_down(&mut self) {
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
        if matches!(undo_behavior, UndoBehavior::UndoRedo) {
            self.last_undo_behavior = UndoBehavior::UndoRedo;
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
        let offset_a = self.line_buffer.insertion_point();
        self.line_buffer.move_to_line_non_blank_start();
        let offset_b = self.line_buffer.insertion_point();
        let deletion_range = min(offset_a, offset_b)..max(offset_a, offset_b);
        self.cut_range(deletion_range);
    }

    fn cut_from_end(&mut self) {
        let cut_slice = &self.line_buffer.get_buffer()[self.line_buffer.insertion_point()..];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice, ClipboardMode::Normal);
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
        let insertion_offset = self.line_buffer.insertion_point();
        let next_char = self.line_buffer.grapheme_right_index();
        self.cut_range(insertion_offset..next_char);
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
            self.line_buffer.move_right_before(c, current_line);
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
            self.line_buffer.move_left_before(c, current_line);
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
        self.line_buffer.delete_right_grapheme();

        self.line_buffer.insert_char(character);
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
            self.cut_range(start..end);
        }
    }

    fn cut_selection_to_cut_buffer(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            self.cut_range(start..end);
            self.selection_anchor = None;
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
    pub fn get_selection(&self) -> Option<(usize, usize)> {
        self.selection_anchor.map(|selection_anchor| {
            let buffer_len = self.line_buffer.len();
            if self.insertion_point() > selection_anchor {
                (
                    selection_anchor,
                    self.line_buffer.grapheme_right_index().min(buffer_len),
                )
            } else {
                (
                    self.insertion_point(),
                    self.line_buffer
                        .grapheme_right_index_from_pos(selection_anchor)
                        .min(buffer_len),
                )
            }
        })
    }

    fn delete_selection(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            self.line_buffer.clear_range_safe(start..end);
            self.selection_anchor = None;
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

    pub(crate) fn reset_selection(&mut self) {
        self.selection_anchor = None;
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
        let offset_a = self.line_buffer.insertion_point();
        self.line_buffer.move_to_line_non_blank_start();
        let offset_b = self.line_buffer.insertion_point();
        self.line_buffer.set_insertion_point(offset_a);
        let copy_range = min(offset_a, offset_b)..max(offset_a, offset_b);
        self.copy_range(copy_range);
    }

    pub(crate) fn copy_from_end(&mut self) {
        let copy_range = self.line_buffer.insertion_point()..self.line_buffer.len();
        self.copy_range(copy_range);
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
        assert_eq!(editor.get_selection(), Some((0, 4)));

        editor.run_edit_command(&EditCommand::SwapCursorAndAnchor);
        assert_eq!(editor.selection_anchor, Some(3));
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.get_selection(), Some((0, 4)));

        editor.run_edit_command(&EditCommand::SwapCursorAndAnchor);
        assert_eq!(editor.selection_anchor, Some(0));
        assert_eq!(editor.insertion_point(), 3);
        assert_eq!(editor.get_selection(), Some((0, 4)));
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
}
