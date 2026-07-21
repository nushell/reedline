use crate::terminal_extensions::semantic_prompt::{PromptKind, SemanticPromptMarkers};
use crate::{CursorConfig, PromptEditMode, PromptViMode};

use {
    super::utils::{coerce_crlf, estimate_required_lines, line_width},
    crate::{
        menu::{Menu, ReedlineMenu},
        painting::PromptLines,
        Prompt,
    },
    crossterm::{
        cursor::{self, MoveTo, RestorePosition, SavePosition},
        style::{Attribute, Print, ResetColor, SetAttribute, SetForegroundColor},
        terminal::{self, Clear, ClearType},
        QueueableCommand,
    },
    std::io::{Result, Write},
    std::ops::RangeInclusive,
    unicode_segmentation::UnicodeSegmentation,
    unicode_width::UnicodeWidthStr,
};
#[cfg(feature = "external_printer")]
use {crate::LineBuffer, crossterm::cursor::MoveUp};

// Returns a string that skips N number of lines with the next offset of lines
// An offset of 0 would return only one line after skipping the required lines
fn skip_buffer_lines(string: &str, skip: usize, offset: Option<usize>) -> &str {
    let mut matches = string.match_indices('\n');
    let index = if skip == 0 {
        0
    } else {
        matches
            .clone()
            .nth(skip - 1)
            .map(|(index, _)| index + 1)
            .unwrap_or(string.len())
    };

    let limit = match offset {
        Some(offset) => {
            let offset = skip + offset;
            matches
                .nth(offset)
                .map(|(index, _)| index)
                .unwrap_or(string.len())
        }
        None => string.len(),
    };

    string[index..limit].trim_end_matches('\n')
}

fn skip_buffer_lines_range(string: &str, skip: usize, offset: Option<usize>) -> (usize, usize) {
    let mut matches = string.match_indices('\n');
    let index = if skip == 0 {
        0
    } else {
        matches
            .clone()
            .nth(skip - 1)
            .map(|(index, _)| index + 1)
            .unwrap_or(string.len())
    };

    let limit = match offset {
        Some(offset) => {
            let offset = skip + offset;
            matches
                .nth(offset)
                .map(|(index, _)| index)
                .unwrap_or(string.len())
        }
        None => string.len(),
    };

    (index, limit)
}

/// The writer used by crossterm operations.
///
/// In production this is a buffered stderr handle. During tests it can be
/// backed by a sink, so painting runs normally without spilling escape
/// sequences onto the real terminal: crossterm writes straight to the file
/// descriptor, which bypasses libtest's output capture.
pub enum W {
    /// Buffered stderr — the real terminal.
    // Constructed only in non-test builds; under `cfg(test)` we always use `Sink`.
    #[cfg_attr(test, allow(dead_code))]
    Terminal(std::io::BufWriter<std::io::Stderr>),
    /// Discards all output, used in tests.
    #[cfg(test)]
    Sink(std::io::Sink),
    /// Captures all output into a buffer so tests can assert on the exact
    /// escape-byte stream the painter emits (not tmux-specific — any
    /// output-level invariant).
    #[cfg(test)]
    Capture(Vec<u8>),
}

impl W {
    /// Writer targeting the real terminal (buffered stderr).
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn terminal() -> Self {
        W::Terminal(std::io::BufWriter::new(std::io::stderr()))
    }

    /// Writer that discards everything, for tests that exercise painting
    /// without printing to the terminal.
    #[cfg(test)]
    pub(crate) fn sink() -> Self {
        W::Sink(std::io::sink())
    }

    /// Writer that buffers everything written to it, so a test can inspect the
    /// emitted bytes after painting.
    #[cfg(test)]
    pub(crate) fn capture() -> Self {
        W::Capture(Vec::new())
    }

    /// Bytes captured so far. Panics unless this is a [`W::capture`] writer.
    #[cfg(test)]
    pub(crate) fn captured(&self) -> &[u8] {
        match self {
            W::Capture(buf) => buf,
            _ => panic!("captured() called on a non-capturing writer"),
        }
    }
}

impl Write for W {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        match self {
            W::Terminal(w) => w.write(buf),
            #[cfg(test)]
            W::Sink(w) => w.write(buf),
            #[cfg(test)]
            W::Capture(w) => w.write(buf),
        }
    }

    fn flush(&mut self) -> Result<()> {
        match self {
            W::Terminal(w) => w.flush(),
            #[cfg(test)]
            W::Sink(w) => w.flush(),
            #[cfg(test)]
            W::Capture(w) => w.flush(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PainterSuspendedState {
    previous_prompt_rows_range: RangeInclusive<u16>,
}

/// Screen bounds of the right prompt when it is visible.
#[derive(Debug, Clone, Copy)]
pub struct RightPromptBounds {
    pub row: u16,
    pub start_col: u16,
    pub end_col: u16,
}

#[derive(Debug, Clone)]
pub struct RenderSnapshot {
    pub screen_width: u16,
    pub screen_height: u16,
    pub prompt_start_row: u16,
    pub prompt_height: u16,
    pub large_buffer: bool,
    pub prompt_str_left: String,
    pub prompt_indicator: String,
    pub before_cursor: String,
    pub after_cursor: String,
    pub first_buffer_col: u16,
    pub menu_active: bool,
    pub menu_start_row: Option<u16>,
    pub large_buffer_extra_rows_after_prompt: Option<usize>,
    pub large_buffer_offset: Option<usize>,
    pub right_prompt: Option<RightPromptBounds>,
}

#[derive(Debug, PartialEq, Eq)]
enum PromptRowSelector {
    UseExistingPrompt { start_row: u16 },
    MakeNewPrompt { new_row: u16 },
}

// Selects the row where the next prompt should start on, taking into account and whether it should re-use a previous
// prompt.
fn select_prompt_row(
    suspended_state: Option<&PainterSuspendedState>,
    (column, row): (u16, u16), // NOTE: Positions are 0 based here
) -> PromptRowSelector {
    if let Some(painter_state) = suspended_state {
        // The painter was suspended, try to re-use the last prompt position to avoid
        // unnecessarily making new prompts.
        if painter_state.previous_prompt_rows_range.contains(&row) {
            // Cursor is still in the range of the previous prompt, re-use it.
            let start_row = *painter_state.previous_prompt_rows_range.start();
            return PromptRowSelector::UseExistingPrompt { start_row };
        } else {
            // There was some output or cursor is outside of the range of previous prompt make a
            // fresh new prompt.
        }
    }

    // Assumption: if the cursor is not on the zeroth column,
    //   there is content we want to leave intact, thus advance to the next row.
    let new_row = if column > 0 { row + 1 } else { row };
    PromptRowSelector::MakeNewPrompt { new_row }
}

/// Layout values computed once per paint cycle, shared between rendering and snapshot creation.
pub(crate) struct PromptLayout {
    /// Total rows scrolled off the top (before prompt adjustment).
    extra_rows: usize,
    /// Rows scrolled off after subtracting prompt lines.
    extra_rows_after_prompt: usize,
    /// Lines to skip from before_cursor for menu space (large buffer only).
    large_buffer_offset: Option<usize>,

    /// Right prompt bounds (`Some` when rendered).
    right_prompt: Option<RightPromptBounds>,

    /// Row where the menu starts.
    menu_start_row: Option<u16>,

    /// Buffer start column on first visible line.
    first_buffer_col: u16,
}

/// Cached row where the prompt starts on screen, together with its
/// freshness.
///
/// Call [`PromptStartRow::invalidate`] from any new code path that
/// yields the tty or writes content the painter doesn't track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptStartRow {
    /// No row known yet (pre-`initialize_prompt_position`).
    Unverified,
    /// Last-known row, but something may have moved the cursor or written
    /// content the painter doesn't model since. The next `repaint_buffer`
    /// must re-query before trusting it.
    Stale(u16),
    /// Row matches the terminal as of the last successful query or paint.
    Verified(u16),
}

impl PromptStartRow {
    /// Painter's best understanding of the prompt's screen row,
    /// regardless of freshness. Defaults to `0` when never initialized,
    /// which should only happen before `initialize_prompt_position` runs;
    /// we don't expect that to happen in normal flow.
    pub(crate) fn last_known_row(self) -> u16 {
        match self {
            PromptStartRow::Verified(r) | PromptStartRow::Stale(r) => r,
            PromptStartRow::Unverified => 0,
        }
    }

    /// Record `row` as freshly verified.
    pub(crate) fn mark_verified(&mut self, row: u16) {
        *self = PromptStartRow::Verified(row);
    }

    /// Demote to stale, preserving the last-known row if any. Idempotent.
    pub(crate) fn invalidate(&mut self) {
        if let PromptStartRow::Verified(r) = *self {
            *self = PromptStartRow::Stale(r);
        }
    }
}

/// Implementation of the output to the terminal
pub struct Painter {
    // Stdout
    stdout: W,
    prompt_start_row: PromptStartRow,
    // The number of lines that the prompt takes up
    prompt_height: u16,
    terminal_size: (u16, u16),
    last_required_lines: u16,
    large_buffer: bool,
    just_resized: bool,
    after_cursor_lines: Option<String>,
    /// Optional semantic prompt markers for terminal integration (OSC 133/633)
    semantic_markers: Option<Box<dyn SemanticPromptMarkers>>,
    /// Layout computed during the last paint cycle.
    pub(crate) last_layout: Option<PromptLayout>,
}

impl Painter {
    pub(crate) fn new(stdout: W) -> Self {
        Painter {
            stdout,
            prompt_start_row: PromptStartRow::Unverified,
            prompt_height: 0,
            terminal_size: (0, 0),
            last_required_lines: 0,
            large_buffer: false,
            just_resized: false,
            after_cursor_lines: None,
            semantic_markers: None,
            last_layout: None,
        }
    }

    /// Height of the current terminal window
    pub fn screen_height(&self) -> u16 {
        self.terminal_size.1
    }

    /// Width of the current terminal window
    pub fn screen_width(&self) -> u16 {
        self.terminal_size.0
    }

    /// Sets the semantic prompt markers for terminal integration (OSC 133/633)
    pub fn set_semantic_markers(&mut self, markers: Option<Box<dyn SemanticPromptMarkers>>) {
        self.semantic_markers = markers;
    }

    /// Returns a reference to the semantic prompt markers, if any
    pub fn semantic_markers(&self) -> Option<&dyn SemanticPromptMarkers> {
        self.semantic_markers.as_deref()
    }
    /// Returns the empty lines from the prompt down.
    pub fn remaining_lines_real(&self) -> u16 {
        self.screen_height()
            .saturating_sub(self.prompt_start_row.last_known_row())
            .saturating_sub(self.prompt_height)
    }

    /// Returns the number of lines that are available or can be made available by
    /// stripping the prompt.
    ///
    /// If you want the number of empty lines below the prompt,
    /// use [`Painter::remaining_lines_real`] instead.
    pub fn remaining_lines(&self) -> u16 {
        self.screen_height()
            .saturating_sub(self.prompt_start_row.last_known_row())
    }

    /// Computes layout values shared between rendering and snapshot creation.
    fn compute_layout(&self, lines: &PromptLines, menu: Option<&ReedlineMenu>) -> PromptLayout {
        let screen_width = self.screen_width();
        let screen_height = self.screen_height();

        // Large buffer extra rows computation
        let (extra_rows, extra_rows_after_prompt) = if self.large_buffer {
            let prompt_lines = lines.prompt_lines_with_wrap(screen_width) as usize;
            let prompt_indicator_lines = lines.prompt_indicator.lines().count();
            let before_cursor_lines = lines.before_cursor.lines().count();
            let total_lines_before =
                prompt_lines + prompt_indicator_lines + before_cursor_lines - 1;
            let extra = total_lines_before.saturating_sub(screen_height as usize);
            (extra, extra.saturating_sub(prompt_lines))
        } else {
            (0, 0)
        };

        // Large buffer offset for menu space
        let large_buffer_offset = if self.large_buffer {
            let cursor_distance = lines.distance_from_prompt(screen_width);
            menu.and_then(|menu| {
                if cursor_distance >= screen_height.saturating_sub(1) {
                    let rows = lines
                        .before_cursor
                        .lines()
                        .count()
                        .saturating_sub(extra_rows_after_prompt)
                        .saturating_sub(menu.min_rows() as usize);
                    Some(rows)
                } else {
                    None
                }
            })
        } else {
            None
        };

        // Right prompt layout — only visible when the prompt itself hasn't scrolled off
        let right_prompt =
            if lines.prompt_str_right.is_empty() || self.large_buffer && extra_rows > 0 {
                None
            } else {
                let prompt_length_right = line_width(&lines.prompt_str_right);
                let start_position = screen_width.saturating_sub(prompt_length_right as u16);
                let input_width = lines.estimate_right_prompt_line_width(screen_width);

                if input_width <= start_position {
                    let mut row = self.prompt_start_row.last_known_row();
                    if lines.right_prompt_on_last_line {
                        row += lines.prompt_lines_with_wrap(screen_width);
                    }
                    Some(RightPromptBounds {
                        row,
                        start_col: start_position,
                        end_col: start_position.saturating_add(prompt_length_right as u16),
                    })
                } else {
                    None
                }
            };

        // Menu start row
        let menu_start_row = menu.map(|menu| {
            let cursor_distance = lines.distance_from_prompt(screen_width);
            if cursor_distance >= screen_height.saturating_sub(1) {
                screen_height.saturating_sub(menu.min_rows())
            } else {
                self.prompt_start_row.last_known_row() + cursor_distance + 1
            }
        });

        // First buffer column
        let first_buffer_col = if self.large_buffer && extra_rows_after_prompt > 0 {
            0
        } else {
            let prompt_line = format!("{}{}", lines.prompt_str_left, lines.prompt_indicator);
            let last_prompt_line = prompt_line.lines().last().unwrap_or_default();
            let width = line_width(last_prompt_line);
            if width > u16::MAX as usize {
                u16::MAX
            } else {
                width as u16
            }
        };

        PromptLayout {
            extra_rows,
            extra_rows_after_prompt,
            large_buffer_offset,
            right_prompt,
            menu_start_row,
            first_buffer_col,
        }
    }

    /// Returns the state necessary before suspending the painter (to run a host command event).
    ///
    /// This state will be used to re-initialize the painter to re-use last prompt if possible.
    pub fn state_before_suspension(&self) -> PainterSuspendedState {
        let start_row = self.prompt_start_row.last_known_row();
        let final_row = start_row + self.last_required_lines;
        PainterSuspendedState {
            previous_prompt_rows_range: start_row..=final_row,
        }
    }

    /// Sets the prompt origin position and screen size for a new line editor
    /// invocation
    ///
    /// Not to be used for resizes during a running line editor, use
    /// [`Painter::handle_resize()`] instead
    pub(crate) fn initialize_prompt_position(
        &mut self,
        suspended_state: Option<&PainterSuspendedState>,
    ) -> Result<()> {
        // Update the terminal size
        self.terminal_size = {
            let size = terminal::size()?;
            // if reported size is 0, 0 -
            // use a default size to avoid divide by 0 panics
            if size == (0, 0) {
                (80, 24)
            } else {
                size
            }
        };
        let prompt_selector = select_prompt_row(suspended_state, cursor::position()?);
        let new_row = match prompt_selector {
            PromptRowSelector::UseExistingPrompt { start_row } => start_row,
            PromptRowSelector::MakeNewPrompt { new_row } => {
                // If we are on the last line and would move beyond the last line, we need to make
                // room for the prompt.
                // Otherwise printing the prompt would scroll off the stored prompt
                // origin, causing issues after repaints.
                if new_row == self.screen_height() {
                    self.print_crlf()?;
                    new_row.saturating_sub(1)
                } else {
                    new_row
                }
            }
        };
        // Matches the cursor row we just measured; mark verified so
        // subsequent paints can skip the possibly-expensive
        // drift-detection call to cursor::position().
        self.prompt_start_row.mark_verified(new_row);
        Ok(())
    }

    /// Mark `prompt_start_row` as possibly out of sync — the next
    /// `repaint_buffer` will re-query the terminal. Call from any path
    /// that lets something other than reedline's rendering move the
    /// cursor (e.g. `$EDITOR`).
    pub(crate) fn invalidate_prompt_start_row(&mut self) {
        self.prompt_start_row.invalidate();
    }

    /// Main painter for the prompt and buffer
    /// It queues all the actions required to print the prompt together with
    /// lines that make the buffer.
    /// Using the prompt lines object in this function it is estimated how the
    /// prompt should scroll up and how much space is required to print all the
    /// lines for the buffer
    ///
    /// Note. The `ScrollUp` operation in `crossterm` deletes lines from the top of
    /// the screen.
    pub(crate) fn repaint_buffer(
        &mut self,
        prompt: &dyn Prompt,
        lines: &PromptLines,
        prompt_mode: PromptEditMode,
        menu: Option<&ReedlineMenu>,
        use_ansi_coloring: bool,
        cursor_config: &Option<CursorConfig>,
    ) -> Result<()> {
        // Reset any ANSI styling that may have been left by external commands
        // This ensures the prompt is not affected by previous output styling
        // Note: Attribute::Reset (SGR 0) resets all attributes including colors
        self.stdout.queue(SetAttribute(Attribute::Reset))?;

        self.stdout.queue(cursor::Hide)?;

        let screen_width = self.screen_width();
        let screen_height = self.screen_height();

        // We add one here as [`PromptLines::prompt_lines_with_wrap`] intentionally subtracts 1 from the real value.
        self.prompt_height = lines.prompt_lines_with_wrap(screen_width) + 1;
        let lines_before_cursor = lines.required_lines(screen_width, true, None);

        // Calibrate prompt start position for multi-line prompt/content before cursor. Check issue #841/#848/#930
        if self.just_resized {
            let resized_row = self
                .prompt_start_row
                .last_known_row()
                .saturating_sub(lines_before_cursor - 1);
            // Leave as `Stale` so the drift check below still runs this
            // paint and self-heals if the arithmetic landed wrong.
            // Resize is infrequent; one extra call to cursor::position()
            // per resize is fine.
            self.prompt_start_row = PromptStartRow::Stale(resized_row);
            self.just_resized = false;
        }

        // Lines and distance parameters
        let remaining_lines = self.remaining_lines();
        let required_lines = lines.required_lines(screen_width, false, menu);

        // Marking the painter state as larger buffer to avoid animations
        self.large_buffer = required_lines >= screen_height;

        // True if the prompt has scrolled above the cached
        // `prompt_start_row` and the caller must re-anchor at row 0.
        // When not verified, query the terminal; promote to verified if
        // the query confirms no drift, so later paints can skip it.
        let should_reset_anchor = match self.prompt_start_row {
            PromptStartRow::Verified(_) => false,
            PromptStartRow::Stale(row) => match cursor::position() {
                // The `+1` handles the case where the previous output
                // ended without a newline, leaving the cursor on the
                // same row as the next prompt.
                Ok(position) => {
                    let drifted = position.1 + 1 < row;
                    if !drifted {
                        self.prompt_start_row.mark_verified(row);
                    }
                    drifted
                }
                Err(_) => false,
            },
            // `initialize_prompt_position` runs before any
            // `repaint_buffer`, so this branch is unreachable in normal
            // flow. Panic loudly in debug builds; in release, force a
            // re-anchor since the alternative is drawing over screen
            // content at row 0 with no scroll.
            PromptStartRow::Unverified => {
                debug_assert!(
                    false,
                    "repaint_buffer reached before initialize_prompt_position"
                );
                true
            }
        };

        // Moving the start position of the cursor based on the size of the required lines
        if self.large_buffer || should_reset_anchor {
            for _ in 0..screen_height.saturating_sub(lines_before_cursor) {
                self.stdout.queue(Print(&coerce_crlf("\n")))?;
            }
            // The reset puts the prompt at row 0; cache is back in sync.
            self.prompt_start_row.mark_verified(0);
        } else if required_lines >= remaining_lines {
            let extra = required_lines.saturating_sub(remaining_lines);
            self.queue_universal_scroll(extra)?;
            let scrolled_row = self.prompt_start_row.last_known_row().saturating_sub(extra);
            self.prompt_start_row.mark_verified(scrolled_row);
        }

        // Moving the cursor to the start of the prompt
        // from this position everything will be printed
        let anchor_row = self.prompt_start_row.last_known_row();
        self.clear_from_anchor(anchor_row)?;

        let layout = self.compute_layout(lines, menu);

        if self.large_buffer {
            self.print_large_buffer(prompt, lines, menu, use_ansi_coloring, &layout)?;
        } else {
            self.print_small_buffer(prompt, lines, menu, use_ansi_coloring, &layout)?;
        }

        self.last_layout = Some(layout);

        // The last_required_lines is used to calculate safe range of the current prompt.
        self.last_required_lines = required_lines;

        self.after_cursor_lines = if !lines.after_cursor.is_empty() {
            Some(lines.after_cursor.to_string())
        } else {
            None
        };

        self.stdout.queue(RestorePosition)?;

        if let Some(shapes) = cursor_config {
            let shape = match &prompt_mode {
                PromptEditMode::Emacs => shapes.emacs,
                PromptEditMode::Vi(PromptViMode::Insert) => shapes.vi_insert,
                PromptEditMode::Vi(PromptViMode::Normal | PromptViMode::Visual) => shapes.vi_normal,
                _ => None,
            };
            if let Some(shape) = shape {
                self.stdout.queue(shape)?;
            }
        }
        self.stdout.queue(cursor::Show)?;

        self.stdout.flush()
    }

    /// Captures the current screen layout into a [`RenderSnapshot`] that records
    /// prompt geometry, buffer positions, right-prompt bounds, and menu state.
    /// This snapshot is later used by [`Self::screen_to_buffer_offset`] to map a
    /// terminal (column, row) click coordinate to a byte offset in the editing buffer.
    pub(crate) fn render_snapshot(
        &self,
        lines: &PromptLines,
        menu: Option<&ReedlineMenu>,
        raw_before: &str,
        raw_after: &str,
        layout: &PromptLayout,
    ) -> RenderSnapshot {
        let large_buffer_extra_rows_after_prompt = if self.large_buffer {
            Some(layout.extra_rows_after_prompt)
        } else {
            None
        };
        let large_buffer_offset = layout.large_buffer_offset;

        RenderSnapshot {
            screen_width: self.screen_width(),
            screen_height: self.screen_height(),
            prompt_start_row: self.prompt_start_row.last_known_row(),
            prompt_height: self.prompt_height,
            large_buffer: self.large_buffer,
            prompt_str_left: lines.prompt_str_left.to_string(),
            prompt_indicator: lines.prompt_indicator.to_string(),
            before_cursor: raw_before.to_string(),
            after_cursor: raw_after.to_string(),
            first_buffer_col: layout.first_buffer_col,
            menu_active: menu.is_some(),
            menu_start_row: layout.menu_start_row,
            large_buffer_extra_rows_after_prompt,
            large_buffer_offset,
            right_prompt: layout.right_prompt,
        }
    }

    /// Maps a terminal screen coordinate (column, row) to a byte offset in the
    /// combined editing buffer (`before_cursor + after_cursor`).
    ///
    /// Returns `None` when the click lands outside the editable area: above the
    /// prompt, inside the right prompt, inside the menu, or past the end of
    /// visible buffer content.
    ///
    /// The algorithm walks grapheme-by-grapheme through the visible portion of
    /// the buffer, tracking the current (row, col) on screen. Wide characters
    /// and line wrapping are accounted for. When the tracked position matches
    /// the target coordinate, the corresponding byte offset is returned.
    pub(crate) fn screen_to_buffer_offset(
        &self,
        snapshot: &RenderSnapshot,
        column: u16,
        row: u16,
    ) -> Option<usize> {
        // Clicks above the prompt are not in the buffer.
        if row < snapshot.prompt_start_row {
            return None;
        }

        // Clicks inside the menu area are not in the buffer.
        if snapshot.menu_active {
            if let Some(menu_start_row) = snapshot.menu_start_row {
                if row >= menu_start_row {
                    return None;
                }
            }
        }

        // Clicks inside the right prompt area are not in the buffer.
        if let Some(rp) = &snapshot.right_prompt {
            if row == rp.row && column >= rp.start_col && column < rp.end_col {
                return None;
            }
        }

        // Convert the absolute screen row to a row relative to the prompt start.
        let screen_width = snapshot.screen_width;
        let target_row = row.saturating_sub(snapshot.prompt_start_row);

        // Determine which relative row the buffer content begins on. When the
        // buffer hasn't scrolled, it starts on the last line of the prompt;
        // otherwise it starts at row 0 (the prompt itself has scrolled off).
        let buffer_start_row = if snapshot.large_buffer
            && snapshot.large_buffer_extra_rows_after_prompt.unwrap_or(0) > 0
        {
            0
        } else {
            snapshot.prompt_height.saturating_sub(1)
        };

        // Click landed in the prompt area before any buffer text.
        if target_row < buffer_start_row {
            return None;
        }

        // Compute the visible byte ranges of the before-cursor and after-cursor
        // buffer segments, accounting for lines scrolled off-screen in large
        // buffers and space reserved for menus.
        let (before_start, before_end) = if snapshot.large_buffer {
            skip_buffer_lines_range(
                &snapshot.before_cursor,
                snapshot.large_buffer_extra_rows_after_prompt.unwrap_or(0),
                snapshot.large_buffer_offset,
            )
        } else {
            (0, snapshot.before_cursor.len())
        };
        let before_visible = &snapshot.before_cursor[before_start..before_end];
        let full_before_visible = before_start == 0 && before_end == snapshot.before_cursor.len();

        let (after_start, after_end) = if snapshot.large_buffer {
            if snapshot.menu_active {
                let end = snapshot
                    .after_cursor
                    .find('\n')
                    .unwrap_or(snapshot.after_cursor.len());
                (0, end)
            } else {
                let cursor_distance = estimate_required_lines(
                    &format!(
                        "{}{}{}",
                        snapshot.prompt_str_left, snapshot.prompt_indicator, snapshot.before_cursor
                    ),
                    screen_width,
                )
                .saturating_sub(1) as u16;
                let remaining_lines = snapshot.screen_height.saturating_sub(cursor_distance);
                let offset = remaining_lines.saturating_sub(1) as usize;
                skip_buffer_lines_range(&snapshot.after_cursor, 0, Some(offset))
            }
        } else {
            (0, snapshot.after_cursor.len())
        };
        let after_visible = &snapshot.after_cursor[after_start..after_end];
        let full_after_visible = after_start == 0 && after_end == snapshot.after_cursor.len();
        let full_buffer_visible = full_before_visible && full_after_visible;

        // Walk through visible buffer content grapheme-by-grapheme, tracking
        // the screen position. When we hit the target (column, row) we return
        // the corresponding byte offset in the full buffer.
        let mut current_row = buffer_start_row;
        let mut current_col = if current_row == buffer_start_row {
            snapshot.first_buffer_col
        } else {
            0
        };

        let mut check_segment = |segment: &str, base_offset: usize| -> Option<usize> {
            for (index, grapheme) in segment.grapheme_indices(true) {
                if grapheme == "\n" {
                    current_row = current_row.saturating_add(1);
                    current_col = 0;
                    continue;
                }

                let width = grapheme.width().max(1) as u16;
                if current_col.saturating_add(width) > screen_width {
                    current_row = current_row.saturating_add(1);
                    current_col = 0;
                }

                if current_row == target_row
                    && column >= current_col
                    && column < current_col.saturating_add(width)
                {
                    return Some(base_offset + index);
                }

                current_col = current_col.saturating_add(width);
            }

            None
        };

        if let Some(offset) = check_segment(before_visible, before_start) {
            return Some(offset);
        }

        let after_base = snapshot.before_cursor.len().saturating_add(after_start);
        if let Some(offset) = check_segment(after_visible, after_base) {
            return Some(offset);
        }

        // Click is past all buffer content but still on the last buffer row;
        // treat it as a click at the very end of the buffer.
        if full_buffer_visible && target_row == current_row && column >= current_col {
            return Some(snapshot.before_cursor.len() + snapshot.after_cursor.len());
        }

        None
    }

    fn print_right_prompt(&mut self, lines: &PromptLines, layout: &PromptLayout) -> Result<()> {
        let Some(rp) = &layout.right_prompt else {
            return Ok(());
        };

        self.stdout
            .queue(SavePosition)?
            .queue(cursor::MoveTo(rp.start_col, rp.row))?;

        // Emit right prompt marker (OSC 133;P;k=r)
        if let Some(markers) = &self.semantic_markers {
            self.stdout
                .queue(Print(markers.prompt_start(PromptKind::Right)))?;
        }

        self.stdout
            .queue(Print(&coerce_crlf(&lines.prompt_str_right)))?
            .queue(RestorePosition)?;

        Ok(())
    }

    /// Move to `row` and erase everything below it.
    ///
    /// Clearing while the cursor is on the home cell (0,0) makes tmux's
    /// `scroll-on-clear` (default on) snapshot the whole screen into scrollback
    /// on every repaint, so the prompt/menu piles up in tmux history (#1062).
    /// At row 0 we therefore erase from column 1 to dodge tmux's `cx == 0`
    /// guard, then step back to column 0; the one skipped cell is overwritten by
    /// whatever is printed next. Every other row takes the plain path.
    fn clear_from_anchor(&mut self, row: u16) -> Result<()> {
        if row == 0 {
            self.stdout
                .queue(cursor::MoveTo(1, row))?
                .queue(Clear(ClearType::FromCursorDown))?
                .queue(cursor::MoveTo(0, row))?;
        } else {
            self.stdout
                .queue(cursor::MoveTo(0, row))?
                .queue(Clear(ClearType::FromCursorDown))?;
        }
        Ok(())
    }

    fn print_menu(
        &mut self,
        menu: &dyn Menu,
        use_ansi_coloring: bool,
        layout: &PromptLayout,
    ) -> Result<()> {
        let starting_row = layout.menu_start_row.unwrap_or(0);
        let remaining_lines = self.screen_height().saturating_sub(starting_row);
        let menu_string = menu.menu_string(remaining_lines, use_ansi_coloring);
        self.clear_from_anchor(starting_row)?;
        self.stdout
            .queue(Print(menu_string.trim_end_matches('\n')))?;

        Ok(())
    }

    fn print_small_buffer(
        &mut self,
        prompt: &dyn Prompt,
        lines: &PromptLines,
        menu: Option<&ReedlineMenu>,
        use_ansi_coloring: bool,
        layout: &PromptLayout,
    ) -> Result<()> {
        // Emit prompt start marker (OSC 133;A;k=i for primary prompt)
        if let Some(markers) = &self.semantic_markers {
            self.stdout
                .queue(Print(markers.prompt_start(PromptKind::Primary)))?;
        }

        // print our prompt with color
        if use_ansi_coloring {
            self.stdout
                .queue(SetForegroundColor(prompt.get_prompt_color()))?;
        }

        self.stdout
            .queue(Print(&coerce_crlf(&lines.prompt_str_left)))?;

        if use_ansi_coloring {
            self.stdout
                .queue(SetForegroundColor(prompt.get_indicator_color()))?;
        }

        self.stdout
            .queue(Print(&coerce_crlf(&lines.prompt_indicator)))?;

        if use_ansi_coloring {
            self.stdout
                .queue(SetForegroundColor(prompt.get_prompt_right_color()))?;
        }

        self.print_right_prompt(lines, layout)?;

        // Emit command input start marker (OSC 133;B) after prompt (including right prompt)
        if let Some(markers) = &self.semantic_markers {
            self.stdout.queue(Print(markers.command_input_start()))?;
        }

        if use_ansi_coloring {
            self.stdout
                .queue(SetAttribute(Attribute::Reset))?
                .queue(ResetColor)?;
        }

        self.stdout
            .queue(Print(&lines.before_cursor))?
            .queue(SavePosition)?
            .queue(Print(&lines.after_cursor))?;

        if let Some(menu) = menu {
            self.print_menu(menu, use_ansi_coloring, layout)?;
        } else {
            self.stdout.queue(Print(&lines.hint))?;
        }

        Ok(())
    }

    fn print_large_buffer(
        &mut self,
        prompt: &dyn Prompt,
        lines: &PromptLines,
        menu: Option<&ReedlineMenu>,
        use_ansi_coloring: bool,
        layout: &PromptLayout,
    ) -> Result<()> {
        let screen_width = self.screen_width();
        let screen_height = self.screen_height();
        let cursor_distance = lines.distance_from_prompt(screen_width);
        let remaining_lines = screen_height.saturating_sub(cursor_distance);

        let extra_rows = layout.extra_rows;
        let extra_rows_after_prompt = layout.extra_rows_after_prompt;

        // Emit prompt start marker (OSC 133;A;k=i for primary prompt) only if prompt is visible
        if extra_rows == 0 {
            if let Some(markers) = &self.semantic_markers {
                self.stdout
                    .queue(Print(markers.prompt_start(PromptKind::Primary)))?;
            }
        }

        // print our prompt with color
        if use_ansi_coloring {
            self.stdout
                .queue(SetForegroundColor(prompt.get_prompt_color()))?;
        }

        // In case the prompt is made out of multiple lines, the prompt is split by
        // lines and only the required ones are printed
        let prompt_skipped = skip_buffer_lines(&lines.prompt_str_left, extra_rows, None);
        self.stdout.queue(Print(&coerce_crlf(prompt_skipped)))?;

        if extra_rows == 0 {
            if use_ansi_coloring {
                self.stdout
                    .queue(SetForegroundColor(prompt.get_prompt_right_color()))?;
            }

            self.print_right_prompt(lines, layout)?;
        }

        if use_ansi_coloring {
            self.stdout
                .queue(SetForegroundColor(prompt.get_indicator_color()))?;
        }
        let indicator_skipped =
            skip_buffer_lines(&lines.prompt_indicator, extra_rows_after_prompt, None);
        self.stdout.queue(Print(&coerce_crlf(indicator_skipped)))?;

        // Emit command input start marker (OSC 133;B) after prompt indicator
        if let Some(markers) = &self.semantic_markers {
            self.stdout.queue(Print(markers.command_input_start()))?;
        }

        if use_ansi_coloring {
            self.stdout.queue(ResetColor)?;
        }

        // Selecting the lines before the cursor that will be printed
        let before_cursor_skipped = skip_buffer_lines(
            &lines.before_cursor,
            extra_rows_after_prompt,
            layout.large_buffer_offset,
        );
        self.stdout.queue(Print(before_cursor_skipped))?;
        self.stdout.queue(SavePosition)?;

        if let Some(menu) = menu {
            // TODO: Also solve the difficult problem of displaying (parts of)
            // the content after the cursor with the completion menu
            // This only shows the rest of the line the cursor is on
            if let Some(newline) = lines.after_cursor.find('\n') {
                self.stdout.queue(Print(&lines.after_cursor[0..newline]))?;
            } else {
                self.stdout.queue(Print(&lines.after_cursor))?;
            }
            self.print_menu(menu, use_ansi_coloring, layout)?;
        } else {
            // Selecting lines for the hint
            // The -1 subtraction is done because the remaining lines consider the line where the
            // cursor is located as a remaining line. That has to be removed to get the correct offset
            // for the after-cursor and hint lines
            let offset = remaining_lines.saturating_sub(1) as usize;
            // Selecting lines after the cursor
            let after_cursor_skipped = skip_buffer_lines(&lines.after_cursor, 0, Some(offset));
            self.stdout.queue(Print(after_cursor_skipped))?;
            // Hint lines
            let hint_skipped = skip_buffer_lines(&lines.hint, 0, Some(offset));
            self.stdout.queue(Print(hint_skipped))?;
        }

        Ok(())
    }

    /// Updates prompt origin and offset to handle a screen resize event
    pub(crate) fn handle_resize(&mut self, width: u16, height: u16) {
        self.terminal_size = (width, height);

        self.invalidate_prompt_start_row();

        // `cursor::position()` is blocking and can time out, but a
        // resize happens infrequently enough that we accept the cost.
        // The row stored below is the *cursor* row, not the prompt's
        // screen origin; `just_resized` in `repaint_buffer` re-anchors
        // it on the next paint.
        //
        // Known bug: on iterm2 and kitty, clearing the screen via CMD-K
        // doesn't reset the cursor position — possibly a `position()`
        // bug.
        #[cfg(not(test))]
        {
            if let Ok(position) = cursor::position() {
                self.prompt_start_row = PromptStartRow::Stale(position.1);
                self.just_resized = true;
            }
        }
    }

    /// Writes `line` to the terminal followed by `\r\n` and
    /// invalidates the cached prompt anchor since the line scrolls the
    /// terminal independently of the painter.
    pub(crate) fn paint_line(&mut self, line: &str) -> Result<()> {
        // Invalidate up front: a partial write below can still leave
        // bytes in the kernel/tty buffer and displace the cursor.
        self.invalidate_prompt_start_row();
        self.stdout.queue(Print(line))?.queue(Print("\r\n"))?;
        self.stdout.flush()
    }

    /// Goes to the beginning of the next line
    ///
    /// Also works in raw mode
    pub(crate) fn print_crlf(&mut self) -> Result<()> {
        self.stdout.queue(Print("\r\n"))?;

        self.stdout.flush()
    }

    /// Clear the screen by printing enough whitespace to start the prompt or
    /// other output back at the first line of the terminal.
    pub(crate) fn clear_screen(&mut self) -> Result<()> {
        self.stdout
            .queue(Clear(ClearType::All))?
            .queue(MoveTo(0, 0))?
            .flush()?;
        self.initialize_prompt_position(None)
    }

    pub(crate) fn clear_scrollback(&mut self) -> Result<()> {
        self.stdout
            .queue(Clear(ClearType::All))?
            .queue(Clear(ClearType::Purge))?
            .queue(MoveTo(0, 0))?
            .flush()?;
        self.initialize_prompt_position(None)
    }

    // The prompt is moved to the end of the buffer after the event was handled
    pub(crate) fn move_cursor_to_end(&mut self) -> Result<()> {
        if let Some(after_cursor) = &self.after_cursor_lines {
            self.stdout
                .queue(Clear(ClearType::FromCursorDown))?
                .queue(Print(after_cursor))?;
        }
        self.print_crlf()
    }

    /// Prints an external message
    ///
    /// This function doesn't flush the buffer. So buffer should be flushed
    /// afterwards perhaps by repainting the prompt via `repaint_buffer()`.
    #[cfg(feature = "external_printer")]
    pub(crate) fn print_external_message(
        &mut self,
        messages: Vec<String>,
        line_buffer: &LineBuffer,
        prompt: &dyn Prompt,
    ) -> Result<()> {
        // adding 3 seems to be right for first line-wrap
        let prompt_len = prompt.render_prompt_right().len() + 3;
        let mut buffer_num_lines = 0_u16;
        for (i, line) in line_buffer.get_buffer().lines().enumerate() {
            let screen_lines = match i {
                0 => {
                    // the first line has to deal with the prompt
                    let first_line_len = line.len() + prompt_len;
                    // at least, it is one line
                    ((first_line_len as u16) / (self.screen_width())) + 1
                }
                _ => {
                    // the n-th line, no prompt, at least, it is one line
                    ((line.len() as u16) / self.screen_width()) + 1
                }
            };
            // count up screen-lines
            buffer_num_lines = buffer_num_lines.saturating_add(screen_lines);
        }
        // move upward to start print if the line-buffer is more than one screen-line
        if buffer_num_lines > 1 {
            self.stdout.queue(MoveUp(buffer_num_lines - 1))?;
        }
        let erase_line = format!("\r{}\r", " ".repeat(self.screen_width().into()));
        let max_row = self.screen_height().saturating_sub(1);
        let starting_row = self.prompt_start_row.last_known_row();
        // Invalidate up front: a `?` early-return below can leave
        // bytes in the buffer with the cache still claiming `Verified`.
        self.invalidate_prompt_start_row();
        let mut row = starting_row;
        for line in messages {
            self.stdout.queue(Print(&erase_line))?;
            // Note: we don't use `print_line` here because we don't want to
            // flush right now. The subsequent repaint of the prompt will cause
            // immediate flush anyways. And if we flush here, every external
            // print causes visible flicker.
            self.stdout.queue(Print(line))?.queue(Print("\r\n"))?;
            row = row.saturating_add(1).min(max_row);
        }
        // Track the row by counting `\r\n`s — matches reedline's
        // historical behavior. The drift check is one-sided so a
        // message that secretly scrolls more rows than we counted
        // (embedded `\n`, certain CSI sequences) can still incorrectly
        // anchor.
        self.prompt_start_row = PromptStartRow::Stale(row);
        Ok(())
    }

    /// Queue scroll of `num` lines to `self.stdout`.
    ///
    /// On some platforms and terminals (e.g. windows terminal, alacritty on windows and linux)
    /// using special escape sequence '\[e<num>S' (provided by [`ScrollUp`]) does not put lines
    /// that go offscreen in scrollback history. This method prints newlines near the edge of screen
    /// (which always works) instead. See [here](https://github.com/nushell/nushell/issues/9166)
    /// for more info on subject.
    ///
    /// ## Note
    /// This method does not return cursor to the original position and leaves it at the first
    /// column of last line. **Be sure to use [`MoveTo`] afterwards if this is not the desired
    /// location**
    fn queue_universal_scroll(&mut self, num: u16) -> Result<()> {
        // If cursor is not near end of screen printing new will not scroll terminal.
        // Move it to the last line to ensure that every newline results in scroll
        self.stdout.queue(MoveTo(0, self.screen_height() - 1))?;
        for _ in 0..num {
            self.stdout.queue(Print(&coerce_crlf("\n")))?;
        }
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn force_prompt_anchored_for_test(&mut self, row: u16) {
        self.prompt_start_row = PromptStartRow::Verified(row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::MenuEvent;
    use crate::{Completer, Editor, PromptHistorySearch, Suggestion};
    use pretty_assertions::assert_eq;
    use std::borrow::Cow;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum MarkerCall {
        PromptPrimary,
        PromptRight,
        CommandInput,
    }

    struct RecordingMarkers {
        calls: Arc<Mutex<Vec<MarkerCall>>>,
    }

    impl SemanticPromptMarkers for RecordingMarkers {
        fn prompt_start(&self, kind: PromptKind) -> Cow<'_, str> {
            let mut calls = self.calls.lock().expect("marker lock poisoned");
            match kind {
                PromptKind::Primary => calls.push(MarkerCall::PromptPrimary),
                PromptKind::Right => calls.push(MarkerCall::PromptRight),
                PromptKind::Secondary => {}
            }
            Cow::Borrowed("")
        }

        fn command_input_start(&self) -> Cow<'_, str> {
            let mut calls = self.calls.lock().expect("marker lock poisoned");
            calls.push(MarkerCall::CommandInput);
            Cow::Borrowed("")
        }
    }

    struct TestPrompt;

    impl Prompt for TestPrompt {
        fn render_prompt_left(&self) -> Cow<'_, str> {
            "> ".into()
        }

        fn render_prompt_right(&self) -> Cow<'_, str> {
            "RP".into()
        }

        fn render_prompt_indicator(&self, _prompt_mode: PromptEditMode) -> Cow<'_, str> {
            "".into()
        }

        fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
            "".into()
        }

        fn render_prompt_history_search_indicator(
            &self,
            _history_search: PromptHistorySearch,
        ) -> Cow<'_, str> {
            "".into()
        }
    }

    #[test]
    fn test_skip_lines() {
        let string = "sentence1\nsentence2\nsentence3\n";

        assert_eq!(skip_buffer_lines(string, 1, None), "sentence2\nsentence3");
        assert_eq!(skip_buffer_lines(string, 2, None), "sentence3");
        assert_eq!(skip_buffer_lines(string, 3, None), "");
        assert_eq!(skip_buffer_lines(string, 4, None), "");
    }

    #[test]
    fn test_skip_lines_no_newline() {
        let string = "sentence1";

        assert_eq!(skip_buffer_lines(string, 0, None), "sentence1");
        assert_eq!(skip_buffer_lines(string, 1, None), "");
    }

    #[test]
    fn test_skip_lines_with_limit() {
        let string = "sentence1\nsentence2\nsentence3\nsentence4\nsentence5";

        assert_eq!(
            skip_buffer_lines(string, 1, Some(1)),
            "sentence2\nsentence3",
        );

        assert_eq!(
            skip_buffer_lines(string, 1, Some(2)),
            "sentence2\nsentence3\nsentence4",
        );

        assert_eq!(
            skip_buffer_lines(string, 2, Some(1)),
            "sentence3\nsentence4",
        );

        assert_eq!(
            skip_buffer_lines(string, 1, Some(10)),
            "sentence2\nsentence3\nsentence4\nsentence5",
        );

        assert_eq!(
            skip_buffer_lines(string, 0, Some(1)),
            "sentence1\nsentence2",
        );

        assert_eq!(skip_buffer_lines(string, 0, Some(0)), "sentence1",);
        assert_eq!(skip_buffer_lines(string, 1, Some(0)), "sentence2",);
    }

    #[test]
    fn test_select_new_prompt_with_no_state_no_output() {
        assert_eq!(
            select_prompt_row(None, (0, 12)),
            PromptRowSelector::MakeNewPrompt { new_row: 12 }
        );
    }

    #[test]
    fn test_select_new_prompt_with_no_state_but_output() {
        assert_eq!(
            select_prompt_row(None, (3, 12)),
            PromptRowSelector::MakeNewPrompt { new_row: 13 }
        );
    }

    #[test]
    fn test_select_existing_prompt() {
        let state = PainterSuspendedState {
            previous_prompt_rows_range: 11..=13,
        };
        assert_eq!(
            select_prompt_row(Some(&state), (0, 12)),
            PromptRowSelector::UseExistingPrompt { start_row: 11 }
        );
        assert_eq!(
            select_prompt_row(Some(&state), (3, 12)),
            PromptRowSelector::UseExistingPrompt { start_row: 11 }
        );
    }

    fn base_snapshot() -> RenderSnapshot {
        RenderSnapshot {
            screen_width: 20,
            screen_height: 10,
            prompt_start_row: 0,
            prompt_height: 1,
            large_buffer: false,
            prompt_str_left: "> ".to_string(),
            prompt_indicator: "".to_string(),
            before_cursor: "".to_string(),
            after_cursor: "".to_string(),
            first_buffer_col: 2,
            menu_active: false,
            menu_start_row: None,
            large_buffer_extra_rows_after_prompt: None,
            large_buffer_offset: None,
            right_prompt: None,
        }
    }

    #[test]
    fn test_screen_to_buffer_simple() {
        let mut snapshot = base_snapshot();
        snapshot.before_cursor = "hello world".to_string();

        let painter = Painter::new(W::sink());
        assert_eq!(painter.screen_to_buffer_offset(&snapshot, 2, 0), Some(0));
        assert_eq!(painter.screen_to_buffer_offset(&snapshot, 3, 0), Some(1));
    }

    #[test]
    fn test_clicks_past_eol_clamps() {
        let mut snapshot = base_snapshot();
        snapshot.before_cursor = "hi".to_string();

        let painter = Painter::new(W::sink());
        assert_eq!(painter.screen_to_buffer_offset(&snapshot, 10, 0), Some(2));
    }

    #[test]
    fn test_wrapped_line_mapping() {
        let mut snapshot = base_snapshot();
        snapshot.screen_width = 5;
        snapshot.before_cursor = "abcdef".to_string();

        let painter = Painter::new(W::sink());
        assert_eq!(painter.screen_to_buffer_offset(&snapshot, 1, 1), Some(4));
    }

    #[test]
    fn test_multiline_mapping() {
        let mut snapshot = base_snapshot();
        snapshot.before_cursor = "ab\ncd".to_string();

        let painter = Painter::new(W::sink());
        assert_eq!(painter.screen_to_buffer_offset(&snapshot, 1, 1), Some(4));
    }

    #[test]
    fn test_large_buffer_skips_lines() {
        let mut snapshot = base_snapshot();
        snapshot.large_buffer = true;
        snapshot.first_buffer_col = 0;
        snapshot.before_cursor = "line1\nline2\nline3".to_string();
        snapshot.large_buffer_extra_rows_after_prompt = Some(1);

        let painter = Painter::new(W::sink());
        assert_eq!(painter.screen_to_buffer_offset(&snapshot, 0, 0), Some(6));
    }

    #[test]
    fn test_click_in_right_prompt_ignored() {
        let mut snapshot = base_snapshot();
        snapshot.before_cursor = "hello".to_string();
        snapshot.right_prompt = Some(RightPromptBounds {
            row: 0,
            start_col: 10,
            end_col: 12,
        });

        let painter = Painter::new(W::sink());
        assert_eq!(painter.screen_to_buffer_offset(&snapshot, 10, 0), None);
    }

    #[test]
    fn test_click_in_menu_ignored() {
        let mut snapshot = base_snapshot();
        snapshot.menu_active = true;
        snapshot.menu_start_row = Some(2);

        let painter = Painter::new(W::sink());
        assert_eq!(painter.screen_to_buffer_offset(&snapshot, 0, 2), None);
    }

    fn make_painter(width: u16, height: u16, large_buffer: bool) -> Painter {
        let mut p = Painter::new(W::sink());
        p.terminal_size = (width, height);
        p.prompt_start_row.mark_verified(0);
        p.prompt_height = 1;
        p.large_buffer = large_buffer;
        p
    }

    fn make_lines<'a>(
        left: &'a str,
        indicator: &'a str,
        right: &'a str,
        before: &'a str,
        after: &'a str,
    ) -> PromptLines<'a> {
        PromptLines {
            prompt_str_left: Cow::Borrowed(left),
            prompt_str_right: Cow::Borrowed(right),
            prompt_indicator: Cow::Borrowed(indicator),
            before_cursor: Cow::Borrowed(before),
            after_cursor: Cow::Borrowed(after),
            hint: Cow::Borrowed(""),
            right_prompt_on_last_line: false,
        }
    }

    /// Paint once into a capture buffer and return the exact bytes emitted.
    /// `anchor_row` is the cached prompt-start row; it is marked verified so the
    /// painter takes the no-drift path and never queries the real terminal.
    fn capture_repaint(prompt: &dyn Prompt, lines: &PromptLines, anchor_row: u16) -> String {
        let mut p = Painter::new(W::capture());
        p.terminal_size = (20, 10);
        p.prompt_start_row.mark_verified(anchor_row);
        p.prompt_height = 1;
        p.repaint_buffer(prompt, lines, PromptEditMode::Default, None, false, &None)
            .expect("repaint_buffer failed");
        String::from_utf8_lossy(p.stdout.captured()).into_owned()
    }

    #[test]
    fn repaint_at_row_0_does_not_erase_from_home_cell() {
        // tmux `scroll-on-clear` (default on) copies the whole screen into
        // scrollback when an erase-below is issued at the home cell (0,0).
        // crossterm encodes MoveTo(0,0) as "\x1b[1;1H" and Clear(FromCursorDown)
        // as "\x1b[J", so that contiguous pair is exactly the bug (#1062).
        // Deliberately coupled to crossterm's escape encoding — it's the
        // byte-level contract we care about.
        let out = capture_repaint(&TestPrompt, &make_lines("> ", "", "RP", "hello", ""), 0);
        assert!(
            !out.contains("\x1b[1;1H\x1b[J"),
            "erase-below at home cell (0,0) would make tmux snapshot the prompt to history; emitted: {out:?}"
        );
    }

    #[test]
    fn repaint_below_row_0_still_clears_from_anchor() {
        // Sanity: away from the home cell the plain MoveTo + erase-below is
        // correct (tmux is not triggered), so the workaround must not apply
        // there. MoveTo(0,3) == "\x1b[4;1H".
        let out = capture_repaint(&TestPrompt, &make_lines("> ", "", "RP", "hello", ""), 3);
        assert!(
            out.contains("\x1b[4;1H\x1b[J"),
            "expected an erase-below from the anchor row; emitted: {out:?}"
        );
    }

    /// Minimal `Menu` whose only real method is `menu_string` — the sole method
    /// `print_menu` exercises. Everything else is unreachable in these tests.
    struct TestMenu(String);

    impl Menu for TestMenu {
        fn menu_string(&self, _available_lines: u16, _use_ansi_coloring: bool) -> String {
            self.0.clone()
        }
        fn is_active(&self) -> bool {
            true
        }
        fn menu_event(&mut self, _event: MenuEvent) {
            unimplemented!()
        }
        fn can_quick_complete(&self) -> bool {
            unimplemented!()
        }
        fn can_partially_complete(
            &mut self,
            _values_updated: bool,
            _editor: &mut Editor,
            _completer: &mut dyn Completer,
        ) -> bool {
            unimplemented!()
        }
        fn update_values(&mut self, _editor: &mut Editor, _completer: &mut dyn Completer) {
            unimplemented!()
        }
        fn reset_position(&mut self) {
            unimplemented!()
        }
        fn update_working_details(
            &mut self,
            _editor: &mut Editor,
            _completer: &mut dyn Completer,
            _painter: &Painter,
        ) {
            unimplemented!()
        }
        fn replace_in_buffer(&self, _editor: &mut Editor) {
            unimplemented!()
        }
        fn menu_required_lines(&self, _terminal_columns: u16) -> u16 {
            unimplemented!()
        }
        fn min_rows(&self) -> u16 {
            unimplemented!()
        }
        fn get_values(&self) -> &[Suggestion] {
            unimplemented!()
        }
    }

    /// Paint a menu into a capture buffer and return the emitted bytes, with the
    /// menu starting at `menu_start_row` (None exercises the `unwrap_or(0)`).
    fn capture_print_menu(menu: &dyn Menu, menu_start_row: Option<u16>) -> String {
        let mut p = Painter::new(W::capture());
        p.terminal_size = (20, 10);
        let layout = PromptLayout {
            extra_rows: 0,
            extra_rows_after_prompt: 0,
            large_buffer_offset: None,
            right_prompt: None,
            menu_start_row,
            first_buffer_col: 0,
        };
        p.print_menu(menu, false, &layout)
            .expect("print_menu failed");
        String::from_utf8_lossy(p.stdout.captured()).into_owned()
    }

    #[test]
    fn print_menu_at_row_0_does_not_erase_from_home_cell() {
        // Same tmux trigger as the prompt path, latent in print_menu via
        // `menu_start_row.unwrap_or(0)`: a menu drawn at row 0 must not emit the
        // home-cell erase-below (#1062).
        let menu = TestMenu("item1\nitem2".to_string());
        let out = capture_print_menu(&menu, Some(0));
        assert!(
            !out.contains("\x1b[1;1H\x1b[J"),
            "erase-below at home cell (0,0) would make tmux snapshot to history; emitted: {out:?}"
        );
    }

    #[test]
    fn print_menu_none_start_row_treated_as_row_0() {
        // `unwrap_or(0)` makes a None start row clear from row 0, so it must
        // honour the same guard.
        let menu = TestMenu("item1".to_string());
        let out = capture_print_menu(&menu, None);
        assert!(
            !out.contains("\x1b[1;1H\x1b[J"),
            "None start row falls back to row 0 and must not erase from home cell; emitted: {out:?}"
        );
    }

    #[test]
    fn test_layout_small_buffer_defaults() {
        let painter = make_painter(20, 10, false);
        let lines = make_lines("> ", "", "", "hello", "");
        let layout = painter.compute_layout(&lines, None);

        assert_eq!(layout.extra_rows, 0);
        assert_eq!(layout.extra_rows_after_prompt, 0);
        assert_eq!(layout.large_buffer_offset, None);
        assert_eq!(layout.first_buffer_col, 2); // "> " is 2 chars wide
        assert_eq!(layout.menu_start_row, None);
    }

    #[test]
    fn test_layout_right_prompt_rendered() {
        let painter = make_painter(40, 10, false);
        let lines = make_lines("> ", "", "RP", "hi", "");
        let layout = painter.compute_layout(&lines, None);

        let rp = layout
            .right_prompt
            .expect("right prompt should be rendered");
        assert_eq!(rp.row, 0);
        assert_eq!(rp.start_col, 38); // 40 - 2
        assert_eq!(rp.end_col, 40);
    }

    #[test]
    fn test_layout_right_prompt_hidden_when_input_too_wide() {
        let painter = make_painter(10, 10, false);
        // Prompt "> " (2) + before "12345678" (8) = 10 which equals start_position (10-2=8)
        // input_width(10) > start_position(8) so right prompt should not render
        let lines = make_lines("> ", "", "RP", "12345678", "");
        let layout = painter.compute_layout(&lines, None);

        assert!(layout.right_prompt.is_none());
    }

    #[test]
    fn test_layout_large_buffer_extra_rows() {
        // Screen is 5 lines tall, buffer content exceeds it.
        // prompt_lines_with_wrap(""> ") = 0
        // prompt_indicator_lines("") = 0
        // before_cursor has 7 lines
        // total_lines_before = 0 + 0 + 7 - 1 = 6
        // extra_rows = 6 - 5 = 1
        // extra_rows_after_prompt = 1 - 0 = 1
        let painter = make_painter(20, 5, true);
        let lines = make_lines("> ", "", "", "l1\nl2\nl3\nl4\nl5\nl6\nl7", "");
        let layout = painter.compute_layout(&lines, None);

        assert_eq!(layout.extra_rows, 1);
        assert_eq!(layout.extra_rows_after_prompt, 1);
        assert_eq!(layout.first_buffer_col, 0); // scrolled, so col 0
    }

    #[test]
    fn test_layout_right_prompt_suppressed_in_large_buffer() {
        // When extra_rows > 0 the prompt has scrolled off, so right prompt
        // should not be rendered — this was a bug in the old render_snapshot.
        let painter = make_painter(20, 5, true);
        let lines = make_lines("> ", "", "RP", "l1\nl2\nl3\nl4\nl5\nl6\nl7", "");
        let layout = painter.compute_layout(&lines, None);

        assert!(layout.extra_rows > 0);
        assert!(layout.right_prompt.is_none());
    }

    #[test]
    fn test_layout_large_buffer_no_scroll_keeps_right_prompt() {
        // Large buffer flag set but content fits — extra_rows == 0
        // Right prompt should still render
        let painter = make_painter(20, 10, true);
        let lines = make_lines("> ", "", "RP", "short", "");
        let layout = painter.compute_layout(&lines, None);

        assert_eq!(layout.extra_rows, 0);
        assert!(layout.right_prompt.is_some());
    }

    #[test]
    fn test_layout_first_buffer_col_with_multiline_prompt() {
        let painter = make_painter(20, 10, false);
        // Multi-line prompt: last line is "$ " (2 chars)
        let lines = make_lines("line1\n$ ", "", "", "hello", "");
        let layout = painter.compute_layout(&lines, None);

        assert_eq!(layout.first_buffer_col, 2);
    }

    #[test]
    fn test_prompt_marker_order_in_small_buffer() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let markers = RecordingMarkers {
            calls: Arc::clone(&calls),
        };

        let mut painter = Painter::new(W::sink());
        painter.terminal_size = (20, 10);
        painter.prompt_start_row.mark_verified(0);
        painter.prompt_height = 1;
        painter.set_semantic_markers(Some(Box::new(markers)));

        let prompt = TestPrompt;
        let lines = PromptLines::new(&prompt, PromptEditMode::Default, None, "", "", "");
        let layout = painter.compute_layout(&lines, None);

        painter
            .print_small_buffer(&prompt, &lines, None, false, &layout)
            .expect("print_small_buffer failed");

        let recorded = calls.lock().expect("marker lock poisoned").clone();
        assert_eq!(
            recorded,
            vec![
                MarkerCall::PromptPrimary,
                MarkerCall::PromptRight,
                MarkerCall::CommandInput
            ]
        );
    }
}
