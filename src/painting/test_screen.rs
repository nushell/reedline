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
//!
//! [`W::screen`](super::W::screen) also answers the painter's cursor queries
//! from one of these, fed only what the painter has flushed, as a terminal
//! behind a buffered writer sees it.

/// Rows kept above the screen; far more than any test scrolls, so
/// [`TestScreen::scrolled`] never undercounts.
const SCROLLBACK: usize = 1000;

/// Mode 2026, synchronized update. `vt100` ignores it, so [`TestScreen`]
/// tracks it itself.
const BEGIN_FRAME: &[u8] = b"\x1b[?2026h";
const END_FRAME: &[u8] = b"\x1b[?2026l";

pub(crate) struct TestScreen {
    parser: vt100::Parser,
    /// How much of the painter's capture has been fed already, so
    /// [`TestScreen::sync`] can be called after every paint.
    consumed: usize,
    /// Whether the bytes fed so far leave a synchronized update open.
    frame_open: bool,
}

impl TestScreen {
    pub(crate) fn new(width: u16, height: u16) -> Self {
        Self {
            parser: vt100::Parser::new(height, width, SCROLLBACK),
            consumed: 0,
            frame_open: false,
        }
    }

    /// Feed bytes that did not come from the painter, such as output a host
    /// printed before the prompt.
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        self.process(bytes);
    }

    /// Feed whatever `captured` holds beyond what was fed last time. Pass the
    /// whole of [`W::captured`](super::W::captured) after each paint.
    pub(crate) fn sync(&mut self, captured: &[u8]) {
        let consumed = std::mem::replace(&mut self.consumed, captured.len());
        self.process(&captured[consumed..]);
    }

    fn process(&mut self, bytes: &[u8]) {
        let last = |marker: &[u8]| bytes.windows(marker.len()).rposition(|w| w == marker);
        match (last(BEGIN_FRAME), last(END_FRAME)) {
            (Some(begin), Some(end)) => self.frame_open = begin > end,
            (Some(_), None) => self.frame_open = true,
            (None, Some(_)) => self.frame_open = false,
            (None, None) => {}
        }
        self.parser.process(bytes);
    }

    /// Whether a synchronized update is open: a terminal that honors it shows
    /// nothing fed since, and Alacritty and Rio also hold back their input,
    /// so a cursor query goes unanswered until their timeout.
    pub(crate) fn frame_open(&self) -> bool {
        self.frame_open
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

    /// The cursor as a terminal reports it to a position query: a pending
    /// wrap reads as the last column, since the report has no column past it.
    pub(crate) fn reported_cursor(&self) -> (u16, u16) {
        let (_, width) = self.parser.screen().size();
        let (column, row) = self.cursor();
        (column.min(width.saturating_sub(1)), row)
    }

    /// The rows pushed off the top into scrollback, oldest first, trailing
    /// blanks trimmed.
    pub(crate) fn scrollback_rows(&mut self) -> Vec<String> {
        let total = self.scrolled();
        let (_, width) = self.parser.screen().size();
        let screen = self.parser.screen_mut();
        // With the view `offset` rows up, its top row is the one `offset`
        // rows above the screen.
        let rows = (1..=total)
            .rev()
            .map(|offset| {
                screen.set_scrollback(offset);
                screen
                    .rows(0, width)
                    .next()
                    .unwrap_or_default()
                    .trim_end()
                    .to_owned()
            })
            .collect();
        screen.set_scrollback(0);
        rows
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
