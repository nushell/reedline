//! A frame painter, in the shape of fish's `screen.cpp`.
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
//! and rewrites only the rows that changed, each positioned absolutely from
//! the prompt's anchor row.
//!
//! Escape sequences are carried raw. A starship prompt or a menu arrives as a
//! finished string, and parsing its styling into a model would mean owning an
//! SGR parser; instead each cell keeps the escapes that preceded its grapheme
//! as an opaque prefix, and each row records the SGR state it starts in so
//! that it can be redrawn on its own.

use super::utils::advance_grapheme;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// One grapheme on screen, with the escape sequences that precede it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Cell {
    /// Escape sequences emitted before `grapheme`, raw.
    pub(crate) prefix: String,
    /// The grapheme itself. Empty for a carrier of escapes with no glyph
    /// after them, such as the reset that ends a prompt.
    pub(crate) grapheme: String,
    /// Columns the grapheme occupies.
    pub(crate) width: u16,
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
    /// SGR sequences in effect when the row begins, so redrawing this row
    /// alone starts from the right style.
    pub(crate) leading: String,
    pub(crate) cells: Vec<Cell>,
}

impl Cell {
    /// Whether this cell draws the same as `other`. The buffer index is not
    /// drawn, so it is not compared: an insert renumbers every later cell
    /// without changing what any of them shows.
    pub(crate) fn same_glyph(&self, other: &Cell) -> bool {
        self.prefix == other.prefix && self.grapheme == other.grapheme && self.width == other.width
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

    /// The SGR state in effect when the cell at `index` is drawn: the row's
    /// `leading` plus the SGR sequences in the prefixes before it, with a
    /// reset dropping what came before. What a redraw starting at `index`
    /// must emit first.
    pub(crate) fn sgr_state_through(&self, index: usize) -> String {
        let mut state: Vec<&str> = if self.leading.is_empty() {
            Vec::new()
        } else {
            vec![self.leading.as_str()]
        };
        for cell in self.cells.iter().take(index) {
            for seq in escapes(&cell.prefix) {
                if !is_sgr(seq) {
                    continue;
                }
                if is_sgr_reset(seq) {
                    state.clear();
                } else {
                    state.push(seq);
                }
            }
        }
        state.concat()
    }

    /// The row's text with the escapes removed, for tests and debugging.
    #[cfg(test)]
    pub(crate) fn text(&self) -> String {
        self.cells
            .iter()
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
    fn push_unwrapped(&mut self, text: &str) {
        let mut prefix = String::new();
        for token in tokens(text) {
            match token {
                Token::Escape(seq) => prefix.push_str(seq),
                Token::Grapheme(grapheme) => self.cells.push(Cell {
                    prefix: std::mem::take(&mut prefix),
                    grapheme: grapheme.to_owned(),
                    width: grapheme.width() as u16,
                    index: None,
                }),
            }
        }
        if !prefix.is_empty() {
            self.cells.push(Cell {
                prefix,
                ..Cell::default()
            });
        }
    }
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

const RESET: &str = "\x1b[0m";

/// Lay `input` out into a frame `input.width` columns wide.
pub(crate) fn layout(input: &LayoutInput<'_>) -> Frame {
    // A zero-width terminal has no layout; one column at least gives every
    // grapheme a row rather than a panic.
    let width = input.width.max(1);
    let mut builder = Builder::new(width);

    if let Some(markers) = &input.markers {
        builder.escape(&markers.prompt_start);
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
        builder.escape(RESET);
    }
    let prompt_end = builder.position();
    let prompt_rows = prompt_end.1 + 1;

    if let Some(markers) = &input.markers {
        builder.escape(&markers.input_start);
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
        builder.finish();
        let mut frame = builder.frame(cursor, prompt_rows);
        frame.menu_row = menu_row;
        frame.set_status(status);
        place_right_prompt(&mut frame, input, prompt_end.1, width);
        return frame;
    }

    builder.put(input.after_cursor, Some(&mut index));
    builder.put(input.hint, None);
    let status = builder.put_block(input.status);
    builder.finish();
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
    if let Some(markers) = &input.markers {
        target.push_unwrapped(&markers.right_start);
    }
    if let Some(colors) = &input.colors {
        target.push_unwrapped(&colors.right);
    }
    target.push_unwrapped(input.prompt_right);
    if input.colors.is_some() {
        target.push_unwrapped(RESET);
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
    /// Escapes seen since the last grapheme, waiting to prefix the next one.
    pending: String,
    /// SGR sequences in effect, cleared by a reset.
    sgr: Vec<String>,
}

impl Builder {
    fn new(width: u16) -> Self {
        Self {
            width,
            rows: vec![Row::default()],
            row: 0,
            col: 0,
            pending: String::new(),
            sgr: Vec::new(),
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
        if seq.is_empty() {
            return;
        }
        self.pending.push_str(seq);
        if is_sgr(seq) {
            if is_sgr_reset(seq) {
                self.sgr.clear();
            } else {
                self.sgr.push(seq.to_owned());
            }
        }
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
        self.put(text, None);
        Some((start, self.row.saturating_sub(start).saturating_add(1)))
    }

    fn ensure_row(&mut self, row: u16) {
        while self.rows.len() <= row as usize {
            self.rows.push(Row {
                leading: self.sgr.concat(),
                cells: Vec::new(),
            });
        }
    }

    fn glyph(&mut self, grapheme: &str, index: Option<usize>) {
        let ((row, col), width) = advance_grapheme((self.row, self.col), grapheme, self.width);
        self.ensure_row(row);
        self.rows[row as usize].cells.push(Cell {
            prefix: std::mem::take(&mut self.pending),
            grapheme: grapheme.to_owned(),
            width,
            index,
        });
        self.row = row;
        self.col = col.saturating_add(width);
    }

    /// Flush escapes that no grapheme followed into a carrier cell.
    fn finish(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let row = self.row;
        self.ensure_row(row);
        self.rows[row as usize].cells.push(Cell {
            prefix: std::mem::take(&mut self.pending),
            ..Cell::default()
        });
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

/// The escape sequences a cell prefix is made of, in order.
fn escapes(prefix: &str) -> impl Iterator<Item = &str> {
    tokens(prefix).filter_map(|token| match token {
        Token::Escape(seq) => Some(seq),
        Token::Grapheme(_) => None,
    })
}

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

    #[test]
    fn escapes_ride_as_prefixes_and_rows_record_their_sgr_state() {
        let frame = layout(&input("\x1b[31m> ", "abcdefghi\x1b[0mjk", "", 10));
        assert_eq!(texts(&frame), vec!["> abcdefgh", "ijk"]);
        assert_eq!(frame.rows[0].cells[0].prefix, "\x1b[31m");
        assert_eq!(frame.rows[0].leading, "");
        // Red is still on when the second row starts; the reset lands on `j`.
        assert_eq!(frame.rows[1].leading, "\x1b[31m");
        assert_eq!(frame.rows[1].cells[0].prefix, "");
        assert_eq!(frame.rows[1].cells[1].prefix, "\x1b[0m");
    }

    #[test]
    fn trailing_escapes_get_a_carrier_cell() {
        let mut input = input("> ", "", "", 10);
        input.colors = Some(Colors {
            prompt: "\x1b[32m".into(),
            indicator: "\x1b[33m".into(),
            right: String::new(),
        });
        let frame = layout(&input);
        let last = frame.rows[0].cells.last().unwrap();
        assert_eq!(last.grapheme, "");
        assert_eq!(last.prefix, "\x1b[33m\x1b[0m");
        assert_eq!(frame.cursor, (2, 0));
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
    fn sgr_state_replays_the_skipped_prefixes_and_stops_at_a_reset() {
        let row = Row {
            leading: "\x1b[1m".into(),
            cells: vec![
                Cell {
                    prefix: "\x1b[31m\x1b]133;B\x07".into(),
                    grapheme: "a".into(),
                    width: 1,
                    index: None,
                },
                Cell {
                    prefix: "\x1b[0m\x1b[32m".into(),
                    grapheme: "b".into(),
                    width: 1,
                    index: None,
                },
                Cell {
                    prefix: String::new(),
                    grapheme: "c".into(),
                    width: 1,
                    index: None,
                },
            ],
        };
        assert_eq!(row.sgr_state_through(0), "\x1b[1m");
        // The OSC marker is not style and is not replayed.
        assert_eq!(row.sgr_state_through(1), "\x1b[1m\x1b[31m");
        assert_eq!(row.sgr_state_through(3), "\x1b[32m");
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
