//! A virtual terminal for painting tests.
//!
//! [`W::capture`](super::W::capture) records the bytes the painter emits, but
//! asserting on escape sequences says nothing about what reaches the screen,
//! and the painter's own layout math cannot serve as the oracle for itself.
//! This feeds those bytes through an independent terminal emulator, so a test
//! can assert on rows, the cursor and scrolling instead.
//!
//! Where real terminals disagree, `vt100` is just one opinion: it keeps a
//! deferred wrap across DECSC/DECRC, and it wraps a wide grapheme early when
//! one column is left (tmux's behavior). A test should assert that the painter
//! stays out of such a case, not on how this emulator resolves it.
//!
//! Tests talk to [`TestScreen`] rather than to `vt100` directly, so the
//! emulator can be swapped by changing this file alone.

/// Rows kept above the screen; far more than any test scrolls, so
/// [`TestScreen::scrolled`] never undercounts.
const SCROLLBACK: usize = 1000;

pub(crate) struct TestScreen {
    parser: vt100::Parser,
    /// How much of the painter's capture has been fed already, so
    /// [`TestScreen::sync`] can be called after every paint.
    consumed: usize,
}

impl TestScreen {
    pub(crate) fn new(width: u16, height: u16) -> Self {
        Self {
            parser: vt100::Parser::new(height, width, SCROLLBACK),
            consumed: 0,
        }
    }

    /// Feed bytes that did not come from the painter, such as output a host
    /// printed before the prompt.
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    /// Feed whatever `captured` holds beyond what was fed last time. Pass the
    /// whole of [`W::captured`](super::W::captured) after each paint.
    pub(crate) fn sync(&mut self, captured: &[u8]) {
        self.parser.process(&captured[self.consumed..]);
        self.consumed = captured.len();
    }

    /// The text on `row`, trailing blanks trimmed.
    pub(crate) fn row(&self, row: u16) -> String {
        self.rows()
            .into_iter()
            .nth(row.into())
            .unwrap_or_else(|| panic!("row {row} is off the screen"))
    }

    /// Every visible row, trailing blanks trimmed.
    pub(crate) fn rows(&self) -> Vec<String> {
        let (_, width) = self.parser.screen().size();
        self.parser
            .screen()
            .rows(0, width)
            .map(|row| row.trim_end().to_owned())
            .collect()
    }

    /// Where the cursor is, as `(column, row)` like the painter and crossterm
    /// count it. The column equals the screen width while a wrap is pending.
    pub(crate) fn cursor(&self) -> (u16, u16) {
        let (row, column) = self.parser.screen().cursor_position();
        (column, row)
    }

    pub(crate) fn cursor_visible(&self) -> bool {
        !self.parser.screen().hide_cursor()
    }

    /// Rows pushed off the top into scrollback since the screen was created.
    pub(crate) fn scrolled(&mut self) -> usize {
        let screen = self.parser.screen_mut();
        // `vt100` only exposes the scrollback length through the view offset,
        // which it clamps to the rows it holds.
        screen.set_scrollback(usize::MAX);
        let rows = screen.scrollback();
        screen.set_scrollback(0);
        rows
    }
}
