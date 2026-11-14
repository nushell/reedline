//! A frame painter: lay the screen out first, then diff it against the last one.
//!
//! The painter in `painter.rs` streams text at the terminal and asks the
//! terminal to remember where the cursor landed. Every row count it needs is
//! then re-derived from strings, and three helpers doing that disagree with
//! each other (a prompt ending in `\n`, a single line wrapping past the
//! bottom).
//!
//! This module lays the prompt, buffer, hint, menu and right prompt out into
//! a [`Frame`] first: rows of [`Cell`]s, one grapheme each, plus the cursor's
//! position within the frame. One grapheme walk, [`advance_grapheme`], does
//! all the wrapping, so there is exactly one row-counting model. The committer
//! in `Painter::repaint_frame` then diffs the new frame against the last one
//! and rewrites only the cells that changed, each row positioned absolutely from
//! the prompt's anchor row.
//!
//! Escape sequences never sit in the text. A starship prompt or a menu arrives
//! as a finished string, so the layout tracks what those sequences leave in
//! effect, and each cell records that as its [`CellStyle`]. That takes no SGR
//! parser: the state is the sequences since the last reset, kept opaque, so
//! two orderings of the same attributes merely cost a redraw. Semantic prompt
//! marks are positions on the frame, [`Mark`], not part of any cell. A
//! sequence the layout does not recognize stays where it was, as a zero-width
//! cell whose text is the sequence.

use super::utils::advance_grapheme;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// What the terminal has to be told before drawing a cell, beyond the cell.
///
/// Shared strings rather than indices into a table, since the diff compares
/// cells of two different frames.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CellStyle {
    /// The SGR sequences in effect, since the last reset.
    pub(crate) sgr: Arc<str>,
    /// The OSC 8 sequence that opened the hyperlink in effect, if any.
    pub(crate) link: Arc<str>,
}

impl CellStyle {
    /// Append what takes the terminal from this style to `to`.
    pub(crate) fn write_change_to(&self, to: &CellStyle, out: &mut String) {
        if self.link != to.link {
            out.push_str(if to.link.is_empty() {
                LINK_END
            } else {
                &to.link
            });
        }
        if self.sgr != to.sgr {
            if !self.sgr.is_empty() {
                out.push_str("\x1b[0m");
            }
            out.push_str(&to.sgr);
        }
    }
}

/// One grapheme on screen and the style it is drawn in.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Cell {
    /// The grapheme itself, or an unrecognized escape sequence with `width` 0.
    pub(crate) grapheme: String,
    /// Columns the grapheme occupies.
    pub(crate) width: u16,
    pub(crate) style: CellStyle,
    /// Index of the grapheme in the editing buffer, counting from the start
    /// of the buffer, when the grapheme came from it. Prompt, hint and menu
    /// cells have none. An index rather than a byte offset: the buffer text
    /// arrives highlighted and CRLF-coerced, so its bytes are not the buffer's,
    /// but its graphemes are.
    pub(crate) index: Option<usize>,
}

/// One screen row of a [`Frame`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Row {
    pub(crate) cells: Vec<Cell>,
}

impl Cell {
    /// Whether this cell draws the same as `other`. The buffer index is not
    /// drawn, so it is not compared: an insert renumbers every later cell
    /// without changing what any of them shows.
    pub(crate) fn same_glyph(&self, other: &Cell) -> bool {
        self.grapheme == other.grapheme && self.width == other.width && self.style == other.style
    }
}

impl Row {
    /// Columns drawn on this row.
    pub(crate) fn width(&self) -> u16 {
        self.cells
            .iter()
            .fold(0u16, |acc, cell| acc.saturating_add(cell.width))
    }

    /// Column the cell at `index` starts on; the row's width past the end.
    pub(crate) fn column_of(&self, index: usize) -> u16 {
        self.cells
            .iter()
            .take(index)
            .fold(0u16, |acc, cell| acc.saturating_add(cell.width))
    }

    /// How many leading cells draw the same as `old`'s, so a redraw can
    /// start after them.
    pub(crate) fn common_prefix(&self, old: &Row) -> usize {
        self.cells
            .iter()
            .zip(&old.cells)
            .take_while(|(new, old)| new.same_glyph(old))
            .count()
    }

    /// The row's text with the escapes removed, for tests and debugging.
    #[cfg(test)]
    pub(crate) fn text(&self) -> String {
        self.cells
            .iter()
            .filter(|cell| cell.width > 0)
            .map(|cell| cell.grapheme.as_str())
            .collect()
    }

    /// The buffer grapheme drawn at `column`, if the cell there came from the
    /// buffer.
    pub(crate) fn index_at(&self, column: u16) -> Option<usize> {
        let mut col = 0u16;
        for cell in &self.cells {
            let end = col.saturating_add(cell.width);
            if cell.width > 0 && column >= col && column < end {
                return cell.index;
            }
            col = end;
        }
        None
    }

    /// Pad with blanks up to `column`.
    fn pad_to(&mut self, column: u16) {
        let mut width = self.width();
        while width < column {
            self.cells.push(Cell {
                grapheme: " ".to_owned(),
                width: 1,
                ..Cell::default()
            });
            width += 1;
        }
    }

    /// Append `text` to this row without wrapping. For the right prompt, which
    /// is only placed once it is known to fit.
    fn push_unwrapped(&mut self, text: &str, tracker: &mut StyleTracker) {
        for token in tokens(text) {
            match token {
                Token::Escape(seq) => {
                    if !tracker.apply(seq) {
                        self.cells.push(raw_cell(seq, tracker));
                    }
                }
                Token::Grapheme(grapheme) => self.cells.push(Cell {
                    grapheme: grapheme.to_owned(),
                    width: grapheme.width() as u16,
                    style: tracker.style.clone(),
                    index: None,
                }),
            }
        }
    }
}

/// A zero-width cell holding an escape sequence the layout does not recognize,
/// so it is still sent, in place, and a change to it still redraws.
fn raw_cell(seq: &str, tracker: &StyleTracker) -> Cell {
    Cell {
        grapheme: seq.to_owned(),
        width: 0,
        style: tracker.style.clone(),
        index: None,
    }
}

/// What the escape sequences seen so far leave in effect.
#[derive(Default)]
struct StyleTracker {
    style: CellStyle,
}

impl StyleTracker {
    /// Take `seq` into the state. `false` when it is not a style at all.
    fn apply(&mut self, seq: &str) -> bool {
        if is_sgr(seq) {
            let sgr = if is_sgr_reset(seq) {
                // `0;31` clears what came before and then sets red.
                if seq.len() > "\x1b[0m".len() {
                    seq.to_owned()
                } else {
                    String::new()
                }
            } else {
                format!("{}{seq}", self.style.sgr)
            };
            self.style.sgr = Arc::from(sgr);
            true
        } else if is_link(seq) {
            self.style.link = if closes_link(seq) {
                Arc::default()
            } else {
                Arc::from(seq)
            };
            true
        } else {
            false
        }
    }
}

/// Which semantic prompt mark a [`Mark`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkKind {
    Prompt,
    Input,
    RightPrompt,
}

/// A semantic prompt mark (OSC 133) and the frame position it belongs at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mark {
    pub(crate) kind: MarkKind,
    pub(crate) row: u16,
    pub(crate) col: u16,
    pub(crate) sequence: String,
}

/// Where the right prompt was placed in a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RightPromptAt {
    pub(crate) row: u16,
    pub(crate) start_col: u16,
    pub(crate) end_col: u16,
}

/// The laid-out screen content of one paint, before any viewport is applied.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Frame {
    pub(crate) rows: Vec<Row>,
    /// The cursor as `(column, row)` within the frame. Always names a row
    /// that exists: a filled row or a trailing newline leaves the cursor on
    /// a row no glyph reaches, and that row is pushed so the committer
    /// reserves it.
    pub(crate) cursor: (u16, u16),
    /// Rows the prompt occupies, wrapping included, at least 1.
    pub(crate) prompt_rows: u16,
    pub(crate) right_prompt: Option<RightPromptAt>,
    /// Row the menu starts on, if one was laid out.
    pub(crate) menu_row: Option<u16>,
    /// Row the status line starts on, if one was laid out.
    pub(crate) status_row: Option<u16>,
    /// Rows the status line takes; 0 without one.
    pub(crate) status_rows: u16,
    pub(crate) marks: Vec<Mark>,
}

impl Frame {
    /// Rows the frame needs on screen.
    pub(crate) fn height(&self) -> u16 {
        self.rows.len().min(u16::MAX as usize) as u16
    }

    /// The first row to show when only `screen_rows` fit: as far down as
    /// needed to keep the cursor row visible, and no further than needed to
    /// fill the screen.
    pub(crate) fn viewport_top(&self, screen_rows: u16) -> u16 {
        self.height().saturating_sub(screen_rows).min(self.cursor.1)
    }

    /// The buffer grapheme drawn at `(column, row)` of the frame.
    pub(crate) fn index_at(&self, column: u16, row: u16) -> Option<usize> {
        self.rows.get(row as usize)?.index_at(column)
    }

    fn set_status(&mut self, status: Option<(u16, u16)>) {
        if let Some((row, rows)) = status {
            self.status_row = Some(row);
            self.status_rows = rows;
        }
    }
}

/// Everything the layout draws, in the strings the painter already has.
pub(crate) struct LayoutInput<'a> {
    pub(crate) prompt_left: &'a str,
    pub(crate) prompt_indicator: &'a str,
    pub(crate) prompt_right: &'a str,
    pub(crate) right_prompt_on_last_line: bool,
    pub(crate) before_cursor: &'a str,
    pub(crate) after_cursor: &'a str,
    pub(crate) hint: &'a str,
    /// Escape prefixes for the prompt, indicator and right prompt, when
    /// coloring is on. Each is emitted before its text and followed by a reset.
    pub(crate) colors: Option<Colors>,
    /// Semantic prompt markers (OSC 133): before the prompt, before the
    /// buffer, and before the right prompt.
    pub(crate) markers: Option<Markers>,
    /// A rendered menu. Replaces the hint and everything after the cursor's
    /// row, as the streaming painter's menu does.
    pub(crate) menu: Option<&'a str>,
    /// The status line, on rows of its own below the buffer and the hint,
    /// and above a menu. Empty draws none.
    pub(crate) status: &'a str,
    pub(crate) width: u16,
}

pub(crate) struct Colors {
    pub(crate) prompt: String,
    pub(crate) indicator: String,
    pub(crate) right: String,
}

pub(crate) struct Markers {
    pub(crate) prompt_start: String,
    pub(crate) input_start: String,
    pub(crate) right_start: String,
}

/// Lay `input` out into a frame `input.width` columns wide.
pub(crate) fn layout(input: &LayoutInput<'_>) -> Frame {
    // A zero-width terminal has no layout; one column at least gives every
    // grapheme a row rather than a panic.
    let width = input.width.max(1);
    let mut builder = Builder::new(width);

    if let Some(markers) = &input.markers {
        builder.mark(MarkKind::Prompt, &markers.prompt_start);
    }
    if let Some(colors) = &input.colors {
        builder.escape(&colors.prompt);
    }
    builder.put(input.prompt_left, None);
    if let Some(colors) = &input.colors {
        builder.escape(&colors.indicator);
    }
    builder.put(input.prompt_indicator, None);
    if input.colors.is_some() {
        builder.escape("\x1b[0m");
    }
    let prompt_end = builder.position();
    let prompt_rows = prompt_end.1 + 1;

    if let Some(markers) = &input.markers {
        builder.mark(MarkKind::Input, &markers.input_start);
    }
    let mut index = 0usize;
    builder.put(input.before_cursor, Some(&mut index));
    let cursor = builder.position();

    if let Some(menu) = input.menu {
        // What follows the cursor on its own row stays; the status line and
        // the menu take the rows below, as the streaming painter clears them.
        builder.put_first_row(input.after_cursor, Some(&mut index));
        let status = builder.put_block(input.status);
        let menu_row = builder.put_block(Some(menu)).map(|(row, _)| row);
        let mut frame = builder.frame(cursor, prompt_rows);
        frame.menu_row = menu_row;
        frame.set_status(status);
        place_right_prompt(&mut frame, input, prompt_end.1, width);
        return frame;
    }

    builder.put(input.after_cursor, Some(&mut index));
    builder.put(input.hint, None);
    let status = builder.put_block(input.status);
    let mut frame = builder.frame(cursor, prompt_rows);
    frame.set_status(status);
    place_right_prompt(&mut frame, input, prompt_end.1, width);
    frame
}

/// Put the right prompt on `prompt_row` (or the first row) when the row has
/// room for it to the right of what is drawn there.
fn place_right_prompt(frame: &mut Frame, input: &LayoutInput<'_>, prompt_row: u16, width: u16) {
    if input.prompt_right.is_empty() {
        return;
    }
    let row = if input.right_prompt_on_last_line {
        prompt_row
    } else {
        0
    };
    let right_width = tokens(input.prompt_right)
        .filter_map(|token| match token {
            Token::Grapheme(grapheme) => Some(grapheme.width() as u16),
            Token::Escape(_) => None,
        })
        .fold(0u16, u16::saturating_add);
    let Some(start_col) = width.checked_sub(right_width) else {
        return;
    };
    let Some(target) = frame.rows.get_mut(row as usize) else {
        return;
    };
    if target.width() > start_col {
        return;
    }
    target.pad_to(start_col);
    let mut tracker = StyleTracker::default();
    if let Some(colors) = &input.colors {
        target.push_unwrapped(&colors.right, &mut tracker);
    }
    target.push_unwrapped(input.prompt_right, &mut tracker);
    if let Some(markers) = input
        .markers
        .as_ref()
        .filter(|markers| !markers.right_start.is_empty())
    {
        frame.marks.push(Mark {
            kind: MarkKind::RightPrompt,
            row,
            col: start_col,
            sequence: markers.right_start.clone(),
        });
    }
    frame.right_prompt = Some(RightPromptAt {
        row,
        start_col,
        end_col: start_col.saturating_add(right_width),
    });
}

/// The grapheme walk that fills rows.
struct Builder {
    width: u16,
    rows: Vec<Row>,
    row: u16,
    col: u16,
    tracker: StyleTracker,
    marks: Vec<Mark>,
}

impl Builder {
    fn new(width: u16) -> Self {
        Self {
            width,
            rows: vec![Row::default()],
            row: 0,
            col: 0,
            tracker: StyleTracker::default(),
            marks: Vec::new(),
        }
    }

    /// Where the next grapheme would start: a filled row resolves to the home
    /// column of the next one, as `resolve_wrap` does.
    fn position(&self) -> (u16, u16) {
        if self.col >= self.width {
            (0, self.row.saturating_add(1))
        } else {
            (self.col, self.row)
        }
    }

    fn escape(&mut self, seq: &str) {
        if seq.is_empty() || self.tracker.apply(seq) {
            return;
        }
        let cell = raw_cell(seq, &self.tracker);
        self.ensure_row(self.row);
        self.rows[self.row as usize].cells.push(cell);
    }

    /// Record `sequence` as belonging where the next grapheme would start.
    fn mark(&mut self, kind: MarkKind, sequence: &str) {
        if sequence.is_empty() {
            return;
        }
        let (col, row) = self.position();
        self.marks.push(Mark {
            kind,
            row,
            col,
            sequence: sequence.to_owned(),
        });
    }

    /// Append `text`, wrapping at the width and breaking rows at newlines.
    /// With `index`, each grapheme (newlines included) takes the next buffer
    /// index.
    fn put(&mut self, text: &str, mut index: Option<&mut usize>) {
        for token in tokens(text) {
            match token {
                Token::Escape(seq) => self.escape(seq),
                Token::Grapheme(grapheme) => {
                    let at = index.as_deref_mut().map(|index| {
                        let at = *index;
                        *index += 1;
                        at
                    });
                    match grapheme {
                        "\n" | "\r\n" => self.newline(),
                        // Lone carriage returns do not occur in coerced text.
                        "\r" => {}
                        _ => self.glyph(grapheme, at),
                    }
                }
            }
        }
    }

    /// As [`Builder::put`], stopping at the first newline. Graphemes past it
    /// still consume indices, so later text keeps its numbering.
    fn put_first_row(&mut self, text: &str, index: Option<&mut usize>) {
        let (first, rest) = match text.find('\n') {
            Some(at) => text.split_at(at),
            None => (text, ""),
        };
        let mut index = index;
        self.put(first, index.as_deref_mut());
        if let Some(index) = index {
            *index += rest.graphemes(true).count();
        }
    }

    fn newline(&mut self) {
        self.row = self.row.saturating_add(1);
        self.col = 0;
        self.ensure_row(self.row);
    }

    /// Step onto the row [`Builder::position`] resolves a filled row to, so
    /// that the next newline counts from where the cursor is.
    fn resolve(&mut self) {
        let (col, row) = self.position();
        self.row = row;
        self.col = col;
        self.ensure_row(row);
    }

    /// Put `text` on rows of its own below everything so far: a status line
    /// or a menu. Returns the row it starts on and the rows it takes, or
    /// `None` for empty text, which takes no row at all.
    fn put_block<'t>(&mut self, text: impl Into<Option<&'t str>>) -> Option<(u16, u16)> {
        let text = text.into().filter(|text| !text.is_empty())?;
        // A row the text filled leaves the cursor on the next one, and the
        // block opens below *that*, not on it.
        self.resolve();
        self.newline();
        let start = self.row;
        // A block starts clean, whatever the text above it left open.
        self.tracker = StyleTracker::default();
        self.put(text, None);
        Some((start, self.row.saturating_sub(start).saturating_add(1)))
    }

    fn ensure_row(&mut self, row: u16) {
        while self.rows.len() <= row as usize {
            self.rows.push(Row::default());
        }
    }

    fn glyph(&mut self, grapheme: &str, index: Option<usize>) {
        let ((row, col), width) = advance_grapheme((self.row, self.col), grapheme, self.width);
        self.ensure_row(row);
        self.rows[row as usize].cells.push(Cell {
            grapheme: grapheme.to_owned(),
            width,
            style: self.tracker.style.clone(),
            index,
        });
        self.row = row;
        self.col = col.saturating_add(width);
    }

    fn frame(mut self, cursor: (u16, u16), prompt_rows: u16) -> Frame {
        // The cursor may rest on a row no glyph reached.
        self.ensure_row(cursor.1);
        Frame {
            rows: self.rows,
            cursor,
            prompt_rows,
            right_prompt: None,
            menu_row: None,
            status_row: None,
            status_rows: 0,
            marks: self.marks,
        }
    }
}

/// A piece of text as the terminal reads it.
#[derive(Debug, PartialEq, Eq)]
enum Token<'a> {
    Escape(&'a str),
    Grapheme(&'a str),
}

/// Split `text` into escape sequences and graphemes, in order.
fn tokens(text: &str) -> impl Iterator<Item = Token<'_>> {
    let mut rest = text;
    let mut graphemes: Option<unicode_segmentation::Graphemes<'_>> = None;
    std::iter::from_fn(move || loop {
        if let Some(iter) = graphemes.as_mut() {
            if let Some(grapheme) = iter.next() {
                return Some(Token::Grapheme(grapheme));
            }
            graphemes = None;
        }
        if rest.is_empty() {
            return None;
        }
        match rest.find('\x1b') {
            Some(0) => {
                let len = escape_len(rest);
                let (seq, tail) = rest.split_at(len);
                rest = tail;
                return Some(Token::Escape(seq));
            }
            Some(at) => {
                let (plain, tail) = rest.split_at(at);
                rest = tail;
                graphemes = Some(plain.graphemes(true));
            }
            None => {
                let plain = rest;
                rest = "";
                graphemes = Some(plain.graphemes(true));
            }
        }
    })
}

/// Length in bytes of the escape sequence `text` starts with. `text` must
/// start with ESC. A malformed or truncated sequence ends at the end of the
/// text, or before the first non-ASCII byte, so the split stays on a char
/// boundary.
fn escape_len(text: &str) -> usize {
    let bytes = text.as_bytes();
    debug_assert_eq!(bytes.first(), Some(&0x1b));
    let len = bytes.len();
    match bytes.get(1) {
        // CSI: parameter and intermediate bytes, then one final byte.
        Some(b'[') => {
            let mut i = 2;
            while i < len && (0x20..=0x3f).contains(&bytes[i]) {
                i += 1;
            }
            if i < len && bytes[i].is_ascii() {
                i + 1
            } else {
                i
            }
        }
        // OSC, DCS, PM, APC: a string ended by BEL or ST.
        Some(b']' | b'P' | b'^' | b'_') => {
            let mut i = 2;
            while i < len {
                match bytes[i] {
                    0x07 => return i + 1,
                    0x1b if bytes.get(i + 1) == Some(&b'\\') => return i + 2,
                    _ => i += 1,
                }
            }
            len
        }
        // Two-byte escapes, with optional intermediates (`ESC ( B`).
        Some(_) => {
            let mut i = 1;
            while i < len && (0x20..=0x2f).contains(&bytes[i]) {
                i += 1;
            }
            if i < len && bytes[i].is_ascii() {
                i + 1
            } else {
                i
            }
        }
        None => 1,
    }
}

fn is_link(seq: &str) -> bool {
    seq.starts_with("\x1b]8;")
}

/// `ESC ] 8 ; params ; ST` with no URI ends the hyperlink.
fn closes_link(seq: &str) -> bool {
    let body = seq["\x1b]8;".len()..].trim_end_matches(['\x07', '\x1b', '\\']);
    body.split_once(';').is_none_or(|(_, uri)| uri.is_empty())
}

/// The sequence that ends a hyperlink.
pub(crate) const LINK_END: &str = "\x1b]8;;\x1b\\";

fn is_sgr(seq: &str) -> bool {
    seq.starts_with("\x1b[") && seq.ends_with('m')
}

/// `ESC [ m`, `ESC [ 0 m` and a parameter list starting with `0;`: the
/// state before them no longer applies.
fn is_sgr_reset(seq: &str) -> bool {
    let params = &seq[2..seq.len() - 1];
    params.is_empty() || params == "0" || params.starts_with("0;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn input<'a>(prompt: &'a str, before: &'a str, after: &'a str, width: u16) -> LayoutInput<'a> {
        LayoutInput {
            prompt_left: prompt,
            prompt_indicator: "",
            prompt_right: "",
            right_prompt_on_last_line: false,
            before_cursor: before,
            after_cursor: after,
            hint: "",
            colors: None,
            markers: None,
            menu: None,
            status: "",
            width,
        }
    }

    fn texts(frame: &Frame) -> Vec<String> {
        frame.rows.iter().map(Row::text).collect()
    }

    #[rstest]
    #[case("\x1b[31m", 5)]
    #[case("\x1b[1;32;40mrest", 10)]
    #[case("\x1b]133;A;k=i\x07text", 12)]
    #[case("\x1b]8;;http://x\x1b\\link", 15)]
    #[case("\x1b(B", 3)]
    #[case("\x1b7", 2)]
    #[case("\x1b[", 2)]
    #[case("\x1b", 1)]
    // A non-ASCII byte where the final byte should be ends the escape before it.
    #[case("\x1b[1ä", 3)]
    fn escape_len_covers_the_sequence(#[case] text: &str, #[case] expected: usize) {
        assert_eq!(escape_len(text), expected);
        assert!(text.is_char_boundary(expected));
    }

    #[test]
    fn tokens_split_escapes_from_graphemes() {
        let got: Vec<_> = tokens("a\x1b[31mb\u{301}\x1b[0m").collect();
        assert_eq!(
            got,
            vec![
                Token::Grapheme("a"),
                Token::Escape("\x1b[31m"),
                Token::Grapheme("b\u{301}"),
                Token::Escape("\x1b[0m"),
            ]
        );
    }

    #[test]
    fn layout_places_the_cursor_between_before_and_after() {
        let frame = layout(&input("> ", "hel", "lo", 20));
        assert_eq!(texts(&frame), vec!["> hello"]);
        assert_eq!(frame.cursor, (5, 0));
        assert_eq!(frame.prompt_rows, 1);
    }

    #[test]
    fn layout_wraps_at_the_width_and_keeps_the_cursor_row() {
        let frame = layout(&input("> ", "abcdefgh", "", 10));
        assert_eq!(texts(&frame), vec!["> abcdefgh", ""]);
        assert_eq!(frame.cursor, (0, 1));
    }

    #[test]
    fn layout_breaks_rows_at_crlf_and_counts_it_as_one_grapheme() {
        let frame = layout(&input("> ", "a\r\nb", "", 20));
        assert_eq!(texts(&frame), vec!["> a", "b"]);
        assert_eq!(frame.cursor, (1, 1));
        assert_eq!(frame.index_at(2, 0), Some(0));
        assert_eq!(frame.index_at(0, 1), Some(2));
    }

    #[test]
    fn prompt_ending_in_newline_puts_the_buffer_on_its_own_row() {
        let frame = layout(&input("abc\n", "hi", "", 20));
        assert_eq!(texts(&frame), vec!["abc", "hi"]);
        assert_eq!(frame.cursor, (2, 1));
        assert_eq!(frame.prompt_rows, 2);
    }

    #[test]
    fn right_prompt_lands_on_the_last_prompt_row_when_it_fits() {
        let mut input = input("abc\n", "hi", "", 20);
        input.prompt_right = "R";
        input.right_prompt_on_last_line = true;
        let frame = layout(&input);
        assert_eq!(
            texts(&frame),
            vec!["abc".to_owned(), format!("hi{}R", " ".repeat(17))]
        );
        assert_eq!(
            frame.right_prompt,
            Some(RightPromptAt {
                row: 1,
                start_col: 19,
                end_col: 20
            })
        );
        // The padding is not buffer text.
        assert_eq!(frame.index_at(10, 1), None);
    }

    #[test]
    fn right_prompt_is_dropped_when_the_row_is_full() {
        let mut input = input("> ", "0123456789abcdefgh", "", 20);
        input.prompt_right = "RR";
        let frame = layout(&input);
        assert_eq!(frame.right_prompt, None);
        // The full row leaves the cursor on an empty row of its own.
        assert_eq!(texts(&frame), vec!["> 0123456789abcdefgh", ""]);
    }

    fn sgr(frame: &Frame, row: usize, cell: usize) -> &str {
        &frame.rows[row].cells[cell].style.sgr
    }

    #[test]
    fn each_cell_records_the_style_in_effect_across_a_wrap() {
        let frame = layout(&input("\x1b[31m> ", "abcdefghi\x1b[0mjk", "", 10));
        assert_eq!(texts(&frame), vec!["> abcdefgh", "ijk"]);
        assert_eq!(sgr(&frame, 0, 0), "\x1b[31m");
        // Red is still on when the second row starts; the reset lands on `j`.
        assert_eq!(sgr(&frame, 1, 0), "\x1b[31m");
        assert_eq!(sgr(&frame, 1, 1), "");
    }

    #[rstest]
    #[case::layers("\x1b[1m\x1b[31ma", "\x1b[1m\x1b[31m")]
    #[case::reset("\x1b[1m\x1b[0ma", "")]
    #[case::bare_reset("\x1b[1m\x1b[ma", "")]
    #[case::reset_then_set("\x1b[1m\x1b[0;31ma", "\x1b[0;31m")]
    fn the_sgr_state_stops_at_a_reset(#[case] before: &str, #[case] expected: &str) {
        assert_eq!(sgr(&layout(&input("", before, "", 10)), 0, 0), expected);
    }

    #[test]
    fn placement_of_a_reset_does_not_change_a_cell() {
        let plain = layout(&input("", "\x1b[32mab", "", 10));
        let chatty = layout(&input("", "\x1b[32ma\x1b[0m\x1b[32mb", "", 10));
        assert_eq!(plain.rows[0].common_prefix(&chatty.rows[0]), 2);
    }

    #[rstest]
    #[case::bel("\x1b]8;;http://x\x07a\x1b]8;;\x07b")]
    #[case::st("\x1b]8;;http://x\x1b\\a\x1b]8;;\x1b\\b")]
    fn a_hyperlink_is_part_of_the_cell_style(#[case] before: &str) {
        let frame = layout(&input("", before, "", 10));
        assert!(frame.rows[0].cells[0].style.link.contains("http://x"));
        assert_eq!(&*frame.rows[0].cells[1].style.link, "");
        assert_eq!(texts(&frame), vec!["ab"]);
    }

    #[test]
    fn an_unrecognized_sequence_stays_in_place_at_no_width() {
        let frame = layout(&input("", "a\x1b]1337;x\x07b", "", 10));
        assert_eq!(frame.rows[0].cells[1].grapheme, "\x1b]1337;x\x07");
        assert_eq!(frame.rows[0].cells[1].width, 0);
        assert_eq!(frame.rows[0].width(), 2);
        assert_eq!(frame.cursor, (2, 0));
    }

    #[test]
    fn a_style_with_no_glyph_after_it_takes_no_cell() {
        let mut input = input("> ", "", "", 10);
        input.colors = Some(Colors {
            prompt: "\x1b[32m".into(),
            indicator: "\x1b[33m".into(),
            right: String::new(),
        });
        let frame = layout(&input);
        assert_eq!(frame.rows[0].cells.len(), 2);
        assert_eq!(frame.cursor, (2, 0));
    }

    #[test]
    fn marks_sit_at_the_start_of_what_they_mark() {
        let mut input = input("abc\n> ", "hi", "", 20);
        input.prompt_right = "R";
        input.markers = Some(Markers {
            prompt_start: "A".into(),
            input_start: "B".into(),
            right_start: "P".into(),
        });
        let frame = layout(&input);
        let at = |kind| {
            let mark = frame.marks.iter().find(|m| m.kind == kind).unwrap();
            (mark.col, mark.row)
        };
        assert_eq!(at(MarkKind::Prompt), (0, 0));
        assert_eq!(at(MarkKind::Input), (2, 1));
        assert_eq!(at(MarkKind::RightPrompt), (19, 0));
        // None of it reached a cell.
        assert_eq!(frame.rows[0].cells[0].grapheme, "a");
    }

    #[test]
    fn menu_replaces_the_rows_after_the_cursor_row() {
        let mut input = input("> ", "ab", "cd\r\nef", 20);
        input.menu = Some("one\ntwo");
        let frame = layout(&input);
        assert_eq!(texts(&frame), vec!["> abcd", "one", "two"]);
        assert_eq!(frame.menu_row, Some(1));
        assert_eq!(frame.cursor, (4, 0));
    }

    #[test]
    fn common_prefix_ignores_the_buffer_index() {
        // Inserting on the row above renumbers this row's cells; the glyphs
        // still match.
        let old = layout(&input("> ", "a\r\nbc", "", 20)).rows.remove(1);
        let new = layout(&input("> ", "ab\r\nbc", "", 20)).rows.remove(1);
        assert_ne!(new.cells[0].index, old.cells[0].index);
        assert_eq!(new.common_prefix(&old), 2);
        let new = layout(&input("> ", "ab\r\nbd", "", 20)).rows.remove(1);
        assert_eq!(new.common_prefix(&old), 1);
        assert_eq!(new.column_of(1), 1);
        assert_eq!(new.column_of(99), 2);
    }

    #[test]
    fn status_follows_the_hint_and_sits_above_a_menu() {
        let mut input = input("> ", "ab", "", 20);
        input.hint = "cd";
        input.status = "s1\r\ns2";
        let frame = layout(&input);
        assert_eq!(texts(&frame), vec!["> abcd", "s1", "s2"]);
        assert_eq!(frame.status_row, Some(1));
        assert_eq!(frame.status_rows, 2);

        input.menu = Some("m1\nm2");
        let frame = layout(&input);
        assert_eq!(texts(&frame), vec!["> ab", "s1", "s2", "m1", "m2"]);
        assert_eq!(frame.status_row, Some(1));
        assert_eq!(frame.menu_row, Some(3));

        input.status = "";
        let frame = layout(&input);
        assert_eq!(frame.status_row, None);
        assert_eq!(frame.status_rows, 0);
        assert_eq!(frame.menu_row, Some(1));
    }

    /// Text filling the row puts the cursor on the next one; the menu must
    /// open below the cursor, not under it.
    #[test]
    fn menu_opens_below_the_cursor_row_after_a_filled_row() {
        let mut input = input("> ", "abcdefgh", "", 10);
        input.menu = Some("one");
        let frame = layout(&input);
        assert_eq!(texts(&frame), vec!["> abcdefgh", "", "one"]);
        assert_eq!(frame.cursor, (0, 1));
        assert_eq!(frame.menu_row, Some(2));
    }

    #[test]
    fn viewport_keeps_the_cursor_row_on_screen() {
        let frame = layout(&input("> ", &"x".repeat(400), "", 80));
        assert_eq!(frame.height(), 6);
        assert_eq!(frame.cursor, (2, 5));
        assert_eq!(frame.viewport_top(5), 1);
        // A cursor near the top wins over filling the screen from the bottom.
        let frame = layout(&input("> ", "a", &"x".repeat(400), 80));
        assert_eq!(frame.cursor, (3, 0));
        assert_eq!(frame.viewport_top(5), 0);
    }

    #[test]
    fn wide_graphemes_wrap_early_rather_than_split() {
        let frame = layout(&input("", &"あ".repeat(3), "", 5));
        assert_eq!(texts(&frame), vec!["ああ", "あ"]);
        assert_eq!(frame.rows[0].width(), 4);
        assert_eq!(frame.index_at(3, 0), Some(1));
        assert_eq!(frame.index_at(4, 0), None);
    }
}
