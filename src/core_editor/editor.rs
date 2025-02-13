use super::{edit_stack::EditStack, Clipboard, ClipboardMode, LineBuffer};
#[cfg(feature = "system_clipboard")]
use crate::core_editor::get_system_clipboard;
use crate::enums::{EditType, UndoBehavior};
use crate::{core_editor::get_local_clipboard, EditCommand};
use std::ops::DerefMut;

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
            EditCommand::CutToEnd => self.cut_from_end(),
            EditCommand::CutToLineEnd => self.cut_to_line_end(),
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
            EditCommand::CutInside { left, right } => self.cut_inside(*left, *right),
            EditCommand::YankInside { left, right } => self.yank_inside(*left, *right),
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

    fn cut_word_left(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let left_index = self.line_buffer.word_left_index();
        if left_index < insertion_offset {
            let cut_range = left_index..insertion_offset;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[cut_range.clone()],
                ClipboardMode::Normal,
            );
            self.line_buffer.clear_range(cut_range);
            self.line_buffer.set_insertion_point(left_index);
        }
    }

    fn cut_big_word_left(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let left_index = self.line_buffer.big_word_left_index();
        if left_index < insertion_offset {
            let cut_range = left_index..insertion_offset;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[cut_range.clone()],
                ClipboardMode::Normal,
            );
            self.line_buffer.clear_range(cut_range);
            self.line_buffer.set_insertion_point(left_index);
        }
    }

    fn cut_word_right(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let right_index = self.line_buffer.word_right_index();
        if right_index > insertion_offset {
            let cut_range = insertion_offset..right_index;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[cut_range.clone()],
                ClipboardMode::Normal,
            );
            self.line_buffer.clear_range(cut_range);
        }
    }

    fn cut_big_word_right(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let right_index = self.line_buffer.next_whitespace();
        if right_index > insertion_offset {
            let cut_range = insertion_offset..right_index;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[cut_range.clone()],
                ClipboardMode::Normal,
            );
            self.line_buffer.clear_range(cut_range);
        }
    }

    fn cut_word_right_to_next(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let right_index = self.line_buffer.word_right_start_index();
        if right_index > insertion_offset {
            let cut_range = insertion_offset..right_index;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[cut_range.clone()],
                ClipboardMode::Normal,
            );
            self.line_buffer.clear_range(cut_range);
        }
    }

    fn cut_big_word_right_to_next(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let right_index = self.line_buffer.big_word_right_start_index();
        if right_index > insertion_offset {
            let cut_range = insertion_offset..right_index;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[cut_range.clone()],
                ClipboardMode::Normal,
            );
            self.line_buffer.clear_range(cut_range);
        }
    }

    fn cut_char(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let right_index = self.line_buffer.grapheme_right_index();
        if right_index > insertion_offset {
            let cut_range = insertion_offset..right_index;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[cut_range.clone()],
                ClipboardMode::Normal,
            );
            self.line_buffer.clear_range(cut_range);
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
            let cut_slice = &self.line_buffer.get_buffer()[start..end];
            self.system_clipboard.set(cut_slice, ClipboardMode::Normal);
            self.line_buffer.clear_range_safe(start, end);
            self.selection_anchor = None;
        }
    }

    fn cut_selection_to_cut_buffer(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            let cut_slice = &self.line_buffer.get_buffer()[start..end];
            self.cut_buffer.set(cut_slice, ClipboardMode::Normal);
            self.line_buffer.clear_range_safe(start, end);
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
            self.line_buffer.clear_range_safe(start, end);
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

    /// Delete text strictly between matching `left_char` and `right_char`.
    /// Places deleted text into the cut buffer.
    /// Leaves the parentheses/quotes/etc. themselves.
    /// On success, move the cursor just after the `left_char`.
    /// If matching chars can't be found, restore the original cursor.
    pub(crate) fn cut_inside(&mut self, left_char: char, right_char: char) {
        let buffer_len = self.line_buffer.len();

        if let Some((lp, rp)) =
            self.line_buffer
                .find_matching_pair(left_char, right_char, self.insertion_point())
        {
            let inside_start = lp + left_char.len_utf8();
            if inside_start < rp && rp <= buffer_len {
                let inside_slice = &self.line_buffer.get_buffer()[inside_start..rp];
                if !inside_slice.is_empty() {
                    self.cut_buffer.set(inside_slice, ClipboardMode::Normal);
                    self.line_buffer.clear_range_safe(inside_start, rp);
                }
                self.line_buffer
                    .set_insertion_point(lp + left_char.len_utf8());
            }
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
        let copy_slice = &self.line_buffer.get_buffer()[copy_range];
        if !copy_slice.is_empty() {
            self.cut_buffer.set(copy_slice, ClipboardMode::Normal);
        }
    }

    pub(crate) fn copy_from_end(&mut self) {
        let copy_slice = &self.line_buffer.get_buffer()[self.line_buffer.insertion_point()..];
        if !copy_slice.is_empty() {
            self.cut_buffer.set(copy_slice, ClipboardMode::Normal);
        }
    }

    pub(crate) fn copy_to_line_end(&mut self) {
        let copy_slice = &self.line_buffer.get_buffer()
            [self.line_buffer.insertion_point()..self.line_buffer.find_current_line_end()];
        if !copy_slice.is_empty() {
            self.cut_buffer.set(copy_slice, ClipboardMode::Normal);
        }
    }

    pub(crate) fn copy_word_left(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let left_index = self.line_buffer.word_left_index();
        if left_index < insertion_offset {
            let copy_range = left_index..insertion_offset;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[copy_range],
                ClipboardMode::Normal,
            );
        }
    }

    pub(crate) fn copy_big_word_left(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let left_index = self.line_buffer.big_word_left_index();
        if left_index < insertion_offset {
            let copy_range = left_index..insertion_offset;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[copy_range],
                ClipboardMode::Normal,
            );
        }
    }

    pub(crate) fn copy_word_right(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let right_index = self.line_buffer.word_right_index();
        if right_index > insertion_offset {
            let copy_range = insertion_offset..right_index;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[copy_range],
                ClipboardMode::Normal,
            );
        }
    }

    pub(crate) fn copy_big_word_right(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let right_index = self.line_buffer.next_whitespace();
        if right_index > insertion_offset {
            let copy_range = insertion_offset..right_index;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[copy_range],
                ClipboardMode::Normal,
            );
        }
    }

    pub(crate) fn copy_word_right_to_next(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let right_index = self.line_buffer.word_right_start_index();
        if right_index > insertion_offset {
            let copy_range = insertion_offset..right_index;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[copy_range],
                ClipboardMode::Normal,
            );
        }
    }

    pub(crate) fn copy_big_word_right_to_next(&mut self) {
        let insertion_offset = self.line_buffer.insertion_point();
        let right_index = self.line_buffer.big_word_right_start_index();
        if right_index > insertion_offset {
            let copy_range = insertion_offset..right_index;
            self.cut_buffer.set(
                &self.line_buffer.get_buffer()[copy_range],
                ClipboardMode::Normal,
            );
        }
    }

    pub(crate) fn copy_right_until_char(&mut self, c: char, before_char: bool, current_line: bool) {
        if let Some(index) = self.line_buffer.find_char_right(c, current_line) {
            let extra = if before_char { 0 } else { c.len_utf8() };
            let copy_slice =
                &self.line_buffer.get_buffer()[self.line_buffer.insertion_point()..index + extra];
            if !copy_slice.is_empty() {
                self.cut_buffer.set(copy_slice, ClipboardMode::Normal);
            }
        }
    }

    pub(crate) fn copy_left_until_char(&mut self, c: char, before_char: bool, current_line: bool) {
        if let Some(index) = self.line_buffer.find_char_left(c, current_line) {
            let extra = if before_char { c.len_utf8() } else { 0 };
            let copy_slice =
                &self.line_buffer.get_buffer()[index + extra..self.line_buffer.insertion_point()];
            if !copy_slice.is_empty() {
                self.cut_buffer.set(copy_slice, ClipboardMode::Normal);
            }
        }
    }

    /// Yank text strictly between matching `left_char` and `right_char`.
    /// Copies it into the cut buffer without removing anything.
    /// Leaves the buffer unchanged and restores the original cursor.
    pub(crate) fn yank_inside(&mut self, left_char: char, right_char: char) {
        let old_pos = self.insertion_point();
        let buffer_len = self.line_buffer.len();

        if let Some((lp, rp)) =
            self.line_buffer
                .find_matching_pair(left_char, right_char, self.insertion_point())
        {
            let inside_start = lp + left_char.len_utf8();
            if inside_start < rp && rp <= buffer_len {
                let inside_slice = &self.line_buffer.get_buffer()[inside_start..rp];
                if !inside_slice.is_empty() {
                    self.cut_buffer.set(inside_slice, ClipboardMode::Normal);
                }
            }
        }

        // Always restore the cursor position
        self.line_buffer.set_insertion_point(old_pos);
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
        editor.cut_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo()baz");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "bar");

        // Test with cursor outside brackets
        let mut editor = editor_with("foo(bar)baz");
        editor.move_to_position(0, false);
        editor.cut_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar)baz");
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.cut_buffer.get().0, "");

        // Test with no matching brackets
        let mut editor = editor_with("foo bar baz");
        editor.move_to_position(4, false);
        editor.cut_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo bar baz");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "");
    }

    #[test]
    fn test_cut_inside_quotes() {
        let mut editor = editor_with("foo\"bar\"baz");
        editor.move_to_position(5, false); // Move inside quotes
        editor.cut_inside('"', '"');
        assert_eq!(editor.get_buffer(), "foo\"\"baz");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "bar");

        // Test with cursor outside quotes
        let mut editor = editor_with("foo\"bar\"baz");
        editor.move_to_position(0, false);
        editor.cut_inside('"', '"');
        assert_eq!(editor.get_buffer(), "foo\"bar\"baz");
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.cut_buffer.get().0, "");

        // Test with no matching quotes
        let mut editor = editor_with("foo bar baz");
        editor.move_to_position(4, false);
        editor.cut_inside('"', '"');
        assert_eq!(editor.get_buffer(), "foo bar baz");
        assert_eq!(editor.insertion_point(), 4);
    }

    #[test]
    fn test_cut_inside_nested() {
        let mut editor = editor_with("foo(bar(baz)qux)quux");
        editor.move_to_position(8, false); // Move inside inner brackets
        editor.cut_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar()qux)quux");
        assert_eq!(editor.insertion_point(), 8);
        assert_eq!(editor.cut_buffer.get().0, "baz");

        editor.move_to_position(4, false); // Move inside outer brackets
        editor.cut_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo()quux");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "bar()qux");
    }

    #[test]
    fn test_yank_inside_brackets() {
        let mut editor = editor_with("foo(bar)baz");
        editor.move_to_position(5, false); // Move inside brackets
        editor.yank_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar)baz"); // Buffer shouldn't change
        assert_eq!(editor.insertion_point(), 5); // Cursor should return to original position

        // Test yanked content by pasting
        editor.paste_cut_buffer();
        assert_eq!(editor.get_buffer(), "foo(bbarar)baz");

        // Test with cursor outside brackets
        let mut editor = editor_with("foo(bar)baz");
        editor.move_to_position(0, false);
        editor.yank_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar)baz");
        assert_eq!(editor.insertion_point(), 0);
    }

    #[test]
    fn test_yank_inside_quotes() {
        let mut editor = editor_with("foo\"bar\"baz");
        editor.move_to_position(5, false); // Move inside quotes
        editor.yank_inside('"', '"');
        assert_eq!(editor.get_buffer(), "foo\"bar\"baz"); // Buffer shouldn't change
        assert_eq!(editor.insertion_point(), 5); // Cursor should return to original position
        assert_eq!(editor.cut_buffer.get().0, "bar");

        // Test with no matching quotes
        let mut editor = editor_with("foo bar baz");
        editor.move_to_position(4, false);
        editor.yank_inside('"', '"');
        assert_eq!(editor.get_buffer(), "foo bar baz");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "");
    }

    #[test]
    fn test_yank_inside_nested() {
        let mut editor = editor_with("foo(bar(baz)qux)quux");
        editor.move_to_position(8, false); // Move inside inner brackets
        editor.yank_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar(baz)qux)quux"); // Buffer shouldn't change
        assert_eq!(editor.insertion_point(), 8);
        assert_eq!(editor.cut_buffer.get().0, "baz");

        // Test yanked content by pasting
        editor.paste_cut_buffer();
        assert_eq!(editor.get_buffer(), "foo(bar(bazbaz)qux)quux");

        editor.move_to_position(4, false); // Move inside outer brackets
        editor.yank_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar(bazbaz)qux)quux");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "bar(bazbaz)qux");
    }
}
