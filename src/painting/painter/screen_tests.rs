//! Painter tests asserting on what reaches the screen, through
//! [`TestScreen`], rather than on the bytes the painter emits.

use super::*;
use crate::painting::test_screen::TestScreen;
use crate::PromptHistorySearch;
use pretty_assertions::assert_eq;
use std::borrow::Cow;

/// A prompt drawing fixed strings. Lines are built through
/// [`PromptLines::new`] from it, as the engine builds them, so a new field on
/// `PromptLines` does not reach these tests.
#[derive(Default)]
struct FixedPrompt {
    left: &'static str,
    indicator: &'static str,
    right: &'static str,
    right_on_last_line: bool,
    multiline: &'static str,
}

impl FixedPrompt {
    fn new(left: &'static str, indicator: &'static str) -> Self {
        Self {
            left,
            indicator,
            ..Self::default()
        }
    }

    fn with_multiline(mut self, indicator: &'static str) -> Self {
        self.multiline = indicator;
        self
    }

    fn with_right(mut self, right: &'static str, on_last_line: bool) -> Self {
        self.right = right;
        self.right_on_last_line = on_last_line;
        self
    }
}

impl Prompt for FixedPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        self.left.into()
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        self.right.into()
    }

    fn render_prompt_indicator(&self, _prompt_mode: PromptEditMode) -> Cow<'_, str> {
        self.indicator.into()
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        self.multiline.into()
    }

    fn render_prompt_history_search_indicator(
        &self,
        _history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        "".into()
    }

    fn right_prompt_on_last_line(&self) -> bool {
        self.right_on_last_line
    }
}

/// A painter wired to a [`TestScreen`] of the same size, with its prompt
/// anchored on `anchor_row` as if `initialize_prompt_position` had measured it.
///
/// `screen` shows everything the painter emitted. The painter's cursor
/// queries are answered by the [`W::screen`] writer, which has seen only what
/// was flushed, as a real terminal would have.
struct Session {
    painter: Painter,
    screen: TestScreen,
}

impl Session {
    fn new(width: u16, height: u16, anchor_row: u16) -> Self {
        let mut painter = Painter::new(W::screen(width, height));
        painter.terminal_size = (width, height);
        // Pinned rather than read from `TERM`, which differs between a
        // developer's shell and CI.
        painter.term_is_dumb = false;
        painter.prompt_start_row.mark_verified(anchor_row);
        Self {
            painter,
            screen: TestScreen::new(width, height),
        }
    }

    /// As [`Session::new`], with `output` already on screen above the anchor,
    /// left there by the host before the prompt.
    fn after_output(width: u16, height: u16, output: &[&str]) -> Self {
        let rows = u16::try_from(output.len()).expect("a test prints a few rows");
        let mut session = Self::new(width, height, rows);
        for line in output {
            session.host_output(line.as_bytes());
            session.host_output(b"\r\n");
        }
        session
    }

    /// Bytes the host writes to the terminal itself, past the painter, as an
    /// idle callback or a command run between prompts does.
    fn host_output(&mut self, bytes: &[u8]) {
        self.screen.feed(bytes);
        self.painter.stdout.terminal_screen().feed(bytes);
    }

    /// Paint `prompt` with the cursor between `before` and `after`.
    fn paint(&mut self, prompt: &FixedPrompt, before: &str, after: &str) {
        let lines = Self::lines(prompt, before, after);
        self.painter
            .repaint_buffer(prompt, &lines, PromptEditMode::Default, None, false, &None)
            .expect("repaint_buffer failed");
        self.screen.sync(self.painter.stdout.captured());
    }

    /// As [`Session::paint`], with a hint, a status line and a menu.
    fn paint_with(
        &mut self,
        prompt: &FixedPrompt,
        before: &str,
        hint: &str,
        status: &str,
        menu: Option<&ReedlineMenu>,
    ) {
        let lines = PromptLines::new(prompt, PromptEditMode::Default, None, before, "", hint)
            .with_status(status, false);
        super::tests::reserve_status(&mut self.painter, &lines);
        self.painter
            .repaint_buffer(prompt, &lines, PromptEditMode::Default, menu, false, &None)
            .expect("repaint_buffer failed");
        self.screen.sync(self.painter.stdout.captured());
    }

    /// The lines [`Session::paint`] paints for the same arguments.
    fn lines<'p>(prompt: &'p FixedPrompt, before: &'p str, after: &'p str) -> PromptLines<'p> {
        PromptLines::new(prompt, PromptEditMode::Default, None, before, after, "")
    }

    /// The buffer offset the painter maps a click on `(column, row)` to, from
    /// the layout of the last paint, which must have had the same arguments.
    fn click(
        &self,
        prompt: &FixedPrompt,
        before: &str,
        after: &str,
        column: u16,
        row: u16,
    ) -> Option<usize> {
        let lines = Self::lines(prompt, before, after);
        let layout = self.painter.last_layout.as_ref().expect("painted before");
        let snapshot = self
            .painter
            .render_snapshot(&lines, None, before, after, layout);
        self.painter.screen_to_buffer_offset(&snapshot, column, row)
    }

    /// Commit `messages` above the prompt block, as the external printer
    /// does. The engine repaints right after; a test does that with
    /// [`Session::paint`].
    fn commit(&mut self, messages: &[&str]) {
        self.painter
            .print_external_message(messages.iter().map(|m| (*m).to_owned()).collect())
            .expect("print_external_message failed");
        self.screen.sync(self.painter.stdout.captured());
    }

    /// Visible rows and scrollback together, oldest first, trailing blank
    /// rows dropped: everything the user can still scroll back to.
    fn history(&mut self) -> Vec<String> {
        let mut rows = self.screen.scrollback_rows();
        rows.extend(self.screen.rows());
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        rows
    }
}

#[test]
fn paints_below_the_host_output_without_touching_it() {
    let mut session = Session::after_output(20, 6, &["out1", "out2"]);

    session.paint(&FixedPrompt::new("> ", ""), "hello", "");

    assert_eq!(session.screen.row(0), "out1");
    assert_eq!(session.screen.row(1), "out2");
    assert_eq!(session.screen.row(2), "> hello");
    assert_eq!(session.screen.cursor(), (7, 2));
    assert!(session.screen.cursor_visible());
    assert_eq!(session.screen.scrolled(), 0);
}

#[test]
fn cursor_sits_between_the_text_before_and_after_it() {
    let mut session = Session::new(20, 6, 1);

    session.paint(&FixedPrompt::new("> ", ""), "hel", "lo");

    assert_eq!(session.screen.row(1), "> hello");
    assert_eq!(session.screen.cursor(), (5, 1));
}

#[test]
fn repaint_draws_over_the_previous_frame() {
    let mut session = Session::new(20, 6, 1);
    let prompt = FixedPrompt::new("> ", "");

    session.paint(&prompt, "a longer line", "");
    session.paint(&prompt, "short", "");

    assert_eq!(session.screen.row(1), "> short");
    assert_eq!(session.screen.row(2), "");
    assert_eq!(session.screen.scrolled(), 0);
}

// #1141: text ending exactly on the margin leaves the terminal's wrap pending,
// and terminals disagree about restoring that state. The cursor belongs on the
// next row's home column, placed there absolutely.
#[test]
fn text_filling_the_width_puts_the_cursor_on_the_next_row() {
    let mut session = Session::new(10, 6, 1);

    session.paint(&FixedPrompt::new("> ", ""), "abcdefgh", "");

    assert_eq!(session.screen.row(1), "> abcdefgh");
    assert_eq!(session.screen.cursor(), (0, 2));
}

#[test]
fn right_prompt_on_last_line_of_a_multiline_prompt() {
    let mut session = Session::new(20, 6, 1);
    let prompt = FixedPrompt::new("abc\n", "> ").with_right("R", true);

    session.paint(&prompt, "hi", "");

    assert_eq!(session.screen.row(1), "abc");
    assert_eq!(session.screen.row(2), format!("> hi{}R", " ".repeat(15)));
    assert_eq!(session.screen.cursor(), (4, 2));
}

// The commit starts where the painter says the block starts, erases from
// there, and prints the messages in its place, so nothing of the old block
// survives under them.
#[test]
fn external_messages_replace_the_block_at_the_anchor() {
    let mut session = Session::new(20, 10, 5);

    session.paint(&FixedPrompt::new("> ", ""), "old", "");
    session.commit(&["one", "two"]);

    assert_eq!(session.screen.rows()[4..8], ["", "one", "two", ""]);
    assert_eq!(
        session.painter.prompt_start_row,
        PromptStartRow::Verified(7)
    );
}

// The case from a real terminal: a left prompt long enough that the first row
// wraps, and a message landing mid-edit. The old path moved up from the cursor
// by a byte count and left a copy of the prompt row above each batch. The
// message commits above the block, and exactly one prompt is drawn under it.
#[test]
fn external_messages_commit_above_the_prompt_and_leave_nothing_behind() {
    let mut session = Session::new(20, 6, 0);
    let prompt = FixedPrompt::new("~/project>", "");

    session.paint(&prompt, "some things!", "");
    session.commit(&["msg"]);
    session.paint(&prompt, "some things!", "");

    // The prompt and buffer wrap at 20 columns onto a second row.
    assert_eq!(
        session.screen.rows()[..4],
        ["msg", "~/project>some thing", "s!", ""]
    );
    assert_eq!(
        session.painter.prompt_start_row,
        PromptStartRow::Verified(1)
    );
}

// An idle callback had the tty and scrolled the prompt up, leaving the cursor
// on its new row. Messages arriving next start where the repaint will find
// the block, so the scrolled copy is replaced, not left above them.
#[test]
fn external_messages_after_an_idle_callback_scrolled_the_prompt() {
    let mut session = Session::new(20, 10, 5);
    let prompt = FixedPrompt::new("> ", "");
    session.paint(&prompt, "abc", "");

    // Three rows up, the cursor on the prompt's row 2.
    session.host_output(b"\x1b[3S\x1b[3;1H");
    // What the engine does after every idle callback.
    session.painter.invalidate_prompt_start_row();
    session.commit(&["msg"]);
    session.paint(&prompt, "abc", "");

    assert_eq!(session.screen.rows()[..5], ["", "", "msg", "> abc", ""]);
    assert_eq!(
        session.painter.prompt_start_row,
        PromptStartRow::Verified(3)
    );
}

// A message wider than the screen takes the rows it wraps onto, and the
// prompt goes below all of them.
#[test]
fn a_wrapped_external_message_pushes_the_prompt_below_its_rows() {
    let mut session = Session::new(20, 6, 2);
    let prompt = FixedPrompt::new("> ", "");

    session.paint(&prompt, "", "");
    session.commit(&[&"x".repeat(30)]);
    session.paint(&prompt, "", "");

    assert_eq!(session.screen.row(2), "x".repeat(20));
    assert_eq!(session.screen.row(3), "x".repeat(10));
    assert_eq!(session.screen.row(4), ">");
    assert_eq!(
        session.painter.prompt_start_row,
        PromptStartRow::Verified(4)
    );
}

// A message holding newlines, as a custom `ExternalOutput` may hand over,
// prints one row per line from the left edge, with no extra row for a
// trailing newline or a `\r` left in the text.
#[test]
fn a_multi_line_external_message_prints_each_line_from_the_left() {
    let mut session = Session::new(20, 8, 1);
    let prompt = FixedPrompt::new("> ", "");

    session.paint(&prompt, "", "");
    session.commit(&["aaa\nbbb", "ccc\r\nddd\n"]);
    session.paint(&prompt, "", "");

    assert_eq!(
        session.screen.rows()[1..6],
        ["aaa", "bbb", "ccc", "ddd", ">"]
    );
    assert_eq!(
        session.painter.prompt_start_row,
        PromptStartRow::Verified(5)
    );
}

// Messages arriving at the bottom of the screen scroll the earlier ones into
// history in order, and the prompt never does.
#[test]
fn external_messages_at_the_bottom_scroll_into_history_in_order() {
    let mut session = Session::new(20, 4, 1);
    session.painter.bottom_confirmed_at = Some(4);
    let prompt = FixedPrompt::new("> ", "");

    session.paint(&prompt, "", "");
    for msg in ["m1", "m2", "m3", "m4", "m5"] {
        session.commit(&[msg]);
        session.paint(&prompt, "", "");
    }

    assert_eq!(session.history(), ["", "m1", "m2", "m3", "m4", "m5", ">"]);
}

// As above with the indicator folded into the left prompt. The row math
// counts rows with `str::lines()`, which yields no row after a trailing
// newline, so the prompt reads as one row: the right prompt lands a row too
// high, and the cursor column and buffer start the painter derives (used for
// menu placement and click mapping) are measured from `abc`, not the empty
// row the buffer is on.
#[test]
#[ignore = "row math reads a trailing newline as no row"]
fn right_prompt_on_last_line_after_a_prompt_ending_in_newline() {
    let mut session = Session::new(20, 6, 1);
    let prompt = FixedPrompt::new("abc\n", "").with_right("R", true);

    session.paint(&prompt, "hi", "");

    assert_eq!(session.screen.row(1), "abc");
    assert_eq!(session.screen.row(2), format!("hi{}R", " ".repeat(17)));
    assert_eq!(session.screen.cursor(), (2, 2));
    let painter_column = Session::lines(&prompt, "hi", "").cursor_pos(20).0;
    assert_eq!(painter_column, session.screen.cursor().0);
    // The `i` on screen.
    assert_eq!(session.click(&prompt, "hi", "", 1, 2), Some(1));
}

// A buffer taller than the screen is drawn as a viewport onto its tail. Which
// rows scroll off is decided by `compute_layout` counting `str::lines()`, so a
// single line wrapping past the bottom reads as fitting: the paint emits one
// row too many and the terminal scrolls, on every paint.
#[test]
#[ignore = "compute_layout counts logical lines, not wrapped rows"]
fn large_buffer_of_one_wrapped_line_stays_on_screen() {
    let mut session = Session::new(80, 5, 0);
    let prompt = FixedPrompt::new("> ", "");
    let buffer = "x".repeat(400);

    session.paint(&prompt, &buffer, "");
    let scrolled_by_first_paint = session.screen.scrolled();
    session.paint(&prompt, &buffer, "");

    // The first paint may scroll to make room; a repaint of the same buffer
    // has nothing new to make room for.
    assert_eq!(session.screen.scrolled(), scrolled_by_first_paint);
    for row in 0..4 {
        assert_eq!(session.screen.row(row), "x".repeat(80));
    }
    assert_eq!(session.screen.row(4), "xx");
    assert_eq!(session.screen.cursor(), (2, 4));
}

#[test]
fn status_line_draws_below_the_buffer() {
    let mut session = Session::new(20, 6, 1);

    session.paint_with(&FixedPrompt::new("> ", ""), "pwd", "", "status", None);

    assert_eq!(session.screen.rows()[1..4], ["> pwd", "status", ""]);
    assert_eq!(session.screen.cursor(), (5, 1));
}

#[test]
fn status_line_follows_the_hint_and_a_wrapped_buffer() {
    let mut session = Session::new(10, 6, 0);

    session.paint_with(
        &FixedPrompt::new("> ", ""),
        "abcdefgh",
        "ij",
        "status",
        None,
    );

    assert_eq!(
        session.screen.rows()[..4],
        ["> abcdefgh", "ij", "status", ""]
    );
    assert_eq!(session.screen.cursor(), (0, 1));
}

// A hint running onto more lines gets the multiline indicator on each, as the
// buffer's own lines do, and leaves the cursor where the buffer ends (#530).
#[test]
fn a_multi_line_hint_gets_the_multiline_indicator() {
    let prompt = FixedPrompt::new("> ", "").with_multiline("::: ");
    let hint = nu_ansi_term::Style::new()
        .dimmed()
        .paint("\n  foo: S,\n}")
        .to_string();
    let hint = crate::painting::continue_hint_lines(hint, &prompt, true, None);
    let mut session = Session::new(20, 6, 0);

    session.paint_with(&prompt, "struct E {", &hint, "", None);

    assert_eq!(
        session.screen.rows()[..4],
        ["> struct E {", ":::   foo: S,", "::: }", ""]
    );
    assert_eq!(session.screen.cursor(), (12, 0));
}

// The rows are reserved like any others, so a prompt on the last row scrolls
// up far enough for a wrapped status line to land on screen.
#[test]
fn status_line_at_the_bottom_scrolls_into_view() {
    let mut session = Session::after_output(10, 4, &["o1", "o2", "o3"]);

    session.paint_with(&FixedPrompt::new("> ", ""), "pwd", "", "0123456789ab", None);

    assert_eq!(session.screen.rows(), ["o3", "> pwd", "0123456789", "ab"]);
    assert_eq!(session.screen.cursor(), (5, 1));
}

#[test]
fn repaint_without_a_status_line_erases_it() {
    let mut session = Session::new(20, 6, 1);
    let prompt = FixedPrompt::new("> ", "");

    session.paint_with(&prompt, "pwd", "", "status", None);
    session.paint_with(&prompt, "pwd", "", "", None);

    assert_eq!(session.screen.rows()[1..3], ["> pwd", ""]);
}

// A menu opens below the cursor's row. The status line takes that row and the
// menu moves down past it, so both stay visible.
#[test]
fn status_line_sits_between_the_buffer_and_a_menu() {
    let mut session = Session::new(20, 8, 1);
    let menu = ReedlineMenu::EngineCompleter(Box::new(super::tests::TestMenu::new("m1\r\nm2")));

    session.paint_with(
        &FixedPrompt::new("> ", ""),
        "pwd",
        "",
        "status",
        Some(&menu),
    );

    assert_eq!(
        session.screen.rows()[1..6],
        ["> pwd", "status", "m1", "m2", ""]
    );
    assert_eq!(session.screen.cursor(), (5, 1));
    assert_eq!(
        session
            .painter
            .last_layout
            .as_ref()
            .map(|l| l.menu_start_row),
        Some(Some(3))
    );
}

// The status line gives way to the buffer: a block that only fits on the
// screen without it is drawn without it, rather than as a large buffer.
#[test]
fn buffer_too_tall_for_the_status_line_draws_without_it() {
    let mut session = Session::new(20, 3, 0);

    session.paint_with(&FixedPrompt::new("> ", ""), "a\r\nb", "", "status", None);

    assert_eq!(session.screen.rows(), ["> a", "b", ""]);
    assert!(!session.painter.large_buffer);
    assert_eq!(session.screen.scrolled(), 0);
}

// Every menu sizes itself from one of the two, so none draws over the rows.
#[test]
fn menus_size_themselves_around_the_reserved_status_rows() {
    let mut session = Session::new(20, 10, 2);
    session.painter.prompt_height = 1;
    let real = session.painter.remaining_lines_real();
    let remaining = session.painter.remaining_lines();

    session.painter.reserve_status_rows(2, 2);

    assert_eq!(session.painter.remaining_lines_real(), real - 2);
    assert_eq!(session.painter.remaining_lines(), remaining - 2);
    assert_eq!(session.painter.reserved_status_rows(), 2);
}

#[test]
fn status_line_without_room_reserves_no_rows() {
    let mut session = Session::new(20, 10, 2);

    session.painter.reserve_status_rows(3, 2);

    assert_eq!(session.painter.reserved_status_rows(), 0);
}

// A reservation is for the paint after it. History search paints without
// making one, and must not lay out the rows of the status line before it.
#[test]
fn a_paint_that_reserves_nothing_draws_no_status_rows() {
    let mut session = Session::new(20, 6, 1);
    let prompt = FixedPrompt::new("> ", "");
    session.paint_with(&prompt, "pwd", "", "status", None);

    session.paint(&prompt, "pwd", "");

    assert_eq!(session.screen.rows()[1..3], ["> pwd", ""]);
    let layout = session.painter.last_layout.as_ref().expect("painted");
    assert_eq!(layout.status_start_row, None);
    assert_eq!(session.painter.last_required_lines, 1);
}

// The rows are set aside before the menu sizes itself. A menu that cannot
// shrink to leave them free takes them back, rather than push the block onto
// the large-buffer path.
#[test]
fn a_menu_too_tall_for_the_status_line_draws_without_it() {
    let mut session = Session::new(20, 5, 0);
    let menu =
        ReedlineMenu::EngineCompleter(Box::new(super::tests::TestMenu::new("m1\r\nm2\r\nm3")));

    session.paint_with(
        &FixedPrompt::new("> ", ""),
        "pwd",
        "",
        "status",
        Some(&menu),
    );

    assert_eq!(session.screen.rows(), ["> pwd", "m1", "m2", "m3", ""]);
    assert!(!session.painter.large_buffer);
}

// A status ending in a line break would print it after the last row it was
// given, scrolling the screen when that row is the bottom one and leaving the
// prompt below the row the painter believes it is on.
#[test]
fn status_line_ending_in_a_newline_at_the_bottom_scrolls_once() {
    let mut session = Session::after_output(10, 4, &["o1", "o2", "o3"]);
    let prompt = FixedPrompt::new("> ", "");

    session.paint_with(&prompt, "pwd", "", "error: x\n", None);
    session.paint_with(&prompt, "pwd", "", "error: x\n", None);

    assert_eq!(session.screen.rows(), ["o2", "o3", "> pwd", "error: x"]);
    assert_eq!(session.screen.cursor(), (5, 2));
}

// With a menu open, the status line takes the row where the text after the
// cursor would continue, and that text is erased. A click there lands nowhere.
#[test]
fn click_on_the_status_line_above_a_menu_lands_nowhere() {
    let mut session = Session::new(20, 8, 1);
    let prompt = FixedPrompt::new("> ", "");
    let menu = ReedlineMenu::EngineCompleter(Box::new(super::tests::TestMenu::new("m1")));
    let lines = PromptLines::new(&prompt, PromptEditMode::Default, None, "a", "\r\nb", "")
        .with_status("status", false);
    super::tests::reserve_status(&mut session.painter, &lines);
    session
        .painter
        .repaint_buffer(
            &prompt,
            &lines,
            PromptEditMode::Default,
            Some(&menu),
            false,
            &None,
        )
        .expect("repaint_buffer failed");
    session.screen.sync(session.painter.stdout.captured());
    let layout = session.painter.last_layout.as_ref().expect("painted");
    let snapshot = session
        .painter
        .render_snapshot(&lines, Some(&menu), "a", "\nb", layout);

    assert_eq!(session.screen.row(2), "status");
    assert_eq!(
        session.painter.screen_to_buffer_offset(&snapshot, 0, 2),
        None
    );
}

/// One paint, for the tests that draw one over another.
type Paint<'a> = &'a dyn Fn(&mut Session);

/// Drawing `second` over `first` must leave what `second` leaves on a blank
/// screen: a paint that no longer erases first has to remove whatever of the
/// old one it does not cover, or it stays for good.
fn assert_nothing_left_over(size: (u16, u16), first: Paint, second: Paint) {
    let mut over = Session::new(size.0, size.1, 1);
    first(&mut over);
    second(&mut over);
    let mut blank = Session::new(size.0, size.1, 1);
    second(&mut blank);

    assert_eq!(over.screen.rows(), blank.screen.rows());
    assert_eq!(over.screen.cursor(), blank.screen.cursor());
    assert!(over.screen.cursor_visible());
}

/// A terminal may draw after any byte of a paint (#1302). Whenever it does,
/// every cell must hold what it held before `second` or what it holds after:
/// cut anywhere, the screen shows text that was there or is about to be,
/// never a blank where both had some.
///
/// The cursor is not part of this. It is hidden for the length of a paint, so
/// a screen drawn halfway still lacks it.
fn assert_no_tear(size: (u16, u16), first: Paint, second: Paint) {
    let mut session = Session::new(size.0, size.1, 1);
    first(&mut session);
    let old = session.screen.cells();
    let start = session.painter.stdout.captured().len();
    second(&mut session);
    let new = session.screen.cells();
    let captured = session.painter.stdout.captured().to_vec();

    let mut replay = TestScreen::new(size.0, size.1);
    replay.feed(&captured[..start]);
    for end in start..captured.len() {
        replay.feed(&captured[end..=end]);
        for (row, cells) in replay.cells().iter().enumerate() {
            for (col, cell) in cells.iter().enumerate() {
                assert!(
                    *cell == old[row][col] || *cell == new[row][col],
                    "after {:?} cell ({col}, {row}) holds {cell:?}, was {:?}, becomes {:?}",
                    String::from_utf8_lossy(&captured[start..=end]),
                    old[row][col],
                    new[row][col],
                );
            }
        }
    }
}

/// Both of the above, for a pair of paints the painter draws in place.
fn assert_drawn_in_place(size: (u16, u16), first: Paint, second: Paint) {
    assert_nothing_left_over(size, first, second);
    assert_no_tear(size, first, second);
}

#[test]
fn typing_draws_in_place() {
    let prompt = FixedPrompt::new("> ", "");
    assert_drawn_in_place((20, 6), &|s| s.paint(&prompt, "ab", ""), &|s| {
        s.paint(&prompt, "abc", "")
    });
}

// The prompt of the recording in #1302: two rows, the buffer on the second.
#[test]
fn typing_under_a_two_row_prompt_draws_in_place() {
    let prompt = FixedPrompt::new("⬢ ~\n", "$ ");
    assert_drawn_in_place((20, 6), &|s| s.paint(&prompt, "jfa", ""), &|s| {
        s.paint(&prompt, "jfav", "")
    });
}

#[test]
fn a_shorter_line_draws_in_place() {
    let prompt = FixedPrompt::new("> ", "");
    assert_drawn_in_place((20, 6), &|s| s.paint(&prompt, "a longer line", ""), &|s| {
        s.paint(&prompt, "short", "")
    });
}

#[test]
fn fewer_lines_draw_in_place() {
    let prompt = FixedPrompt::new("> ", "").with_multiline("::: ");
    assert_drawn_in_place(
        (20, 8),
        &|s| s.paint(&prompt, "first\nsecond line\nthird", ""),
        &|s| s.paint(&prompt, "one", "\ntwo"),
    );
}

#[test]
fn a_hint_going_away_draws_in_place() {
    let prompt = FixedPrompt::new("> ", "");
    assert_drawn_in_place(
        (20, 6),
        &|s| s.paint_with(&prompt, "gi", "t status\n--short", "", None),
        &|s| s.paint_with(&prompt, "gix", "", "", None),
    );
}

#[test]
fn a_closed_menu_draws_in_place() {
    let prompt = FixedPrompt::new("> ", "");
    let menu = ReedlineMenu::EngineCompleter(Box::new(super::tests::TestMenu::new("m1\r\nm2")));
    assert_drawn_in_place(
        (20, 8),
        &|s| s.paint_with(&prompt, "pw", "", "", Some(&menu)),
        &|s| s.paint_with(&prompt, "pwd", "", "", None),
    );
}

#[rstest::rstest]
#[case::on_the_first_row(false)]
#[case::on_the_last_row(true)]
fn a_right_prompt_draws_in_place(#[case] on_last_line: bool) {
    let prompt = FixedPrompt::new("abc\n", "> ").with_right("RIGHT", on_last_line);
    assert_drawn_in_place((20, 6), &|s| s.paint(&prompt, "longer", ""), &|s| {
        s.paint(&prompt, "hi", "")
    });
}

// The buffer reaching the right prompt hides it, and a shorter one brings it
// back: its cells change hands between the two.
#[rstest::rstest]
#[case::hidden("hi", "0123456789abcd")]
#[case::shown_again("0123456789abcd", "hi")]
fn a_right_prompt_hidden_and_shown_draws_in_place(#[case] first: &str, #[case] second: &str) {
    let prompt = FixedPrompt::new("> ", "").with_right("RIGHT", false);
    assert_drawn_in_place((20, 6), &|s| s.paint(&prompt, first, ""), &|s| {
        s.paint(&prompt, second, "")
    });
}

// Not drawn in place, since the painter cannot be sure where the text ends:
// a line reaching the right margin leaves the wrap pending, where an erase
// takes the last cell on some terminals, and terminals disagree on how wide
// an ambiguous-width character is. The right prompt counts too: it is placed
// by its width and nothing is erased behind it. Such a paint erases first as
// before, and still has to leave nothing behind.
#[rstest::rstest]
#[case::a_line_reaching_the_margin("", "abcdefgh")]
#[case::an_ambiguous_width("", "\u{a7} 1")]
#[case::an_uncertain_right_prompt("\u{e0a0}", "hi")]
fn a_paint_that_erases_first_leaves_nothing_behind(
    #[case] right: &'static str,
    #[case] second: &str,
) {
    let prompt = FixedPrompt::new("> ", "").with_right(right, false);
    let first: Paint = &|s| s.paint(&prompt, "one\ntwo\nthree", "");
    let second: Paint = &|s| s.paint(&prompt, second, "");
    assert_nothing_left_over((10, 6), first, second);

    let mut session = Session::new(10, 6, 1);
    first(&mut session);
    let start = session.painter.stdout.captured().len();
    second(&mut session);
    let paint = String::from_utf8_lossy(&session.painter.stdout.captured()[start..]).into_owned();
    assert!(paint.contains("\x1b[2;1H\x1b[J"), "{paint:?}");
}

fn test_menu(contents: &str) -> ReedlineMenu {
    ReedlineMenu::EngineCompleter(Box::new(super::tests::TestMenu::new(contents)))
}

impl Session {
    /// As [`Session::paint_with`], returning the bytes that paint emitted.
    fn paint_menu(&mut self, before: &str, status: &str, menu: &ReedlineMenu) -> String {
        let sent = self.painter.stdout.captured().len();
        self.paint_with(&FixedPrompt::new("> ", ""), before, "", status, Some(menu));
        String::from_utf8_lossy(&self.painter.stdout.captured()[sent..]).into_owned()
    }
}

// On a slow serial line every resent byte is felt: a repaint under a menu
// whose rows are unchanged sends the buffer, never the menu again.
#[test]
fn a_repaint_under_an_unchanged_menu_resends_only_the_buffer() {
    let mut session = Session::new(30, 10, 1);
    let menu = test_menu("Ethernet0  Ethernet4\r\nEthernet8  Ethernet12\r\n");

    session.paint_menu("E", "", &menu);
    let sent = session.paint_menu("Et", "", &menu);

    assert!(!sent.contains("Ethernet"), "menu resent: {sent:?}");
    assert_eq!(
        session.screen.rows()[1..5],
        ["> Et", "Ethernet0  Ethernet4", "Ethernet8  Ethernet12", ""]
    );
    assert_eq!(session.screen.cursor(), (4, 1));
}

// Moving the selection changes two rows; only those are sent.
#[test]
fn a_changed_menu_row_is_redrawn_alone() {
    let mut session = Session::new(30, 10, 1);

    session.paint_menu("x", "", &test_menu("row-a\r\nrow-b\r\nrow-c\r\n"));
    let sent = session.paint_menu("x", "", &test_menu("row-a\r\nROW-B\r\nrow-c\r\n"));

    assert!(sent.contains("ROW-B"));
    assert!(
        !sent.contains("row-a") && !sent.contains("row-c"),
        "{sent:?}"
    );
    assert_eq!(
        session.screen.rows()[1..6],
        ["> x", "row-a", "ROW-B", "row-c", ""]
    );
}

// A row that grows shorter loses the cells it no longer covers.
#[test]
fn a_shorter_menu_row_erases_its_old_tail() {
    let mut session = Session::new(30, 10, 1);

    session.paint_menu("x", "", &test_menu("a long first row\r\nsecond\r\n"));
    session.paint_menu("x", "", &test_menu("short\r\nsecond\r\n"));

    assert_eq!(session.screen.rows()[1..4], ["> x", "short", "second"]);
}

// A menu that filters down to fewer rows leaves none of the old ones behind.
#[test]
fn a_menu_with_fewer_rows_erases_the_rows_it_left() {
    let mut session = Session::new(30, 10, 1);

    session.paint_menu("E", "", &test_menu("one\r\ntwo\r\nthree\r\n"));
    session.paint_menu("Et", "", &test_menu("one\r\n"));

    assert_eq!(session.screen.rows()[1..6], ["> Et", "one", "", "", ""]);
}

// The status line sits between the buffer and the menu, and is redrawn with
// the buffer while the menu stays.
#[test]
fn status_line_redraws_above_a_kept_menu() {
    let mut session = Session::new(30, 10, 1);
    let menu = test_menu("m1\r\nm2\r\n");

    session.paint_menu("a", "status one", &menu);
    let sent = session.paint_menu("ab", "status two", &menu);

    assert!(!sent.contains("m1") && !sent.contains("m2"), "{sent:?}");
    assert_eq!(
        session.screen.rows()[1..6],
        ["> ab", "status two", "m1", "m2", ""]
    );
}

// Output committed above the prompt moves the block, so the menu is drawn
// whole again rather than trusted where it was.
#[test]
fn a_menu_is_redrawn_whole_after_external_output() {
    let mut session = Session::new(30, 10, 1);
    let menu = test_menu("m1\r\nm2\r\n");

    session.paint_menu("a", "", &menu);
    session.commit(&["printed"]);
    let sent = session.paint_menu("a", "", &menu);

    assert!(sent.contains("m1") && sent.contains("m2"), "{sent:?}");
    assert_eq!(
        session.screen.rows()[1..6],
        ["printed", "> a", "m1", "m2", ""]
    );
}

// A row wider than the screen wraps and shifts the rows below it, so such a
// menu is drawn whole every time.
#[test]
fn a_menu_with_a_wrapping_row_is_redrawn_whole() {
    let mut session = Session::new(10, 10, 1);
    let menu = test_menu("0123456789abc\r\nnext\r\n");

    session.paint_menu("a", "", &menu);
    let sent = session.paint_menu("ab", "", &menu);

    assert!(sent.contains("next"), "{sent:?}");
    assert_eq!(
        session.screen.rows()[1..5],
        ["> ab", "0123456789", "abc", "next"]
    );
}

// Text after the cursor that runs onto the menu's rows draws over them, so
// the menu is drawn whole to repair them.
#[test]
fn text_after_the_cursor_reaching_the_menu_redraws_it_whole() {
    let mut session = Session::new(30, 10, 1);
    let menu = test_menu("m1\r\nm2\r\n");
    let prompt = FixedPrompt::new("> ", "");

    session.paint_with(&prompt, "a", "", "", Some(&menu));
    let lines = PromptLines::new(&prompt, PromptEditMode::Default, None, "a", "\nb", "");
    let sent = session.painter.stdout.captured().len();
    session
        .painter
        .repaint_buffer(
            &prompt,
            &lines,
            PromptEditMode::Default,
            Some(&menu),
            false,
            &None,
        )
        .expect("repaint_buffer failed");
    session.screen.sync(session.painter.stdout.captured());
    let sent = String::from_utf8_lossy(&session.painter.stdout.captured()[sent..]).into_owned();

    assert!(sent.contains("m1"), "{sent:?}");
    assert_eq!(session.screen.row(2), "m1");
}

// A row exactly as wide as the screen is kept like any other, and redrawn
// without the erase that would take its last cell.
#[test]
fn a_full_width_menu_row_keeps_its_last_cell() {
    let mut session = Session::new(10, 10, 1);

    session.paint_menu("a", "", &test_menu("0123456789\r\nrow\r\n"));
    session.paint_menu("a", "", &test_menu("abcdefghij\r\nrow\r\n"));

    assert_eq!(session.screen.rows()[1..4], ["> a", "abcdefghij", "row"]);
}

// Once most rows change, the menu is drawn whole, and the screen shows the
// same as row by row.
#[test]
fn a_menu_whose_rows_all_change_is_drawn_whole() {
    let mut session = Session::new(30, 10, 1);

    session.paint_menu("a", "", &test_menu("one\r\ntwo\r\nthree\r\n"));
    let sent = session.paint_menu("a", "", &test_menu("uno\r\ndos\r\n"));

    assert!(sent.contains("uno\r\ndos"), "drawn row by row: {sent:?}");
    assert_eq!(session.screen.rows()[1..5], ["> a", "uno", "dos", ""]);
}

/// One paint of a menu test: the buffer before and after the cursor, the
/// status line, and the menu's text.
type MenuStep = (&'static str, &'static str, &'static str, &'static str);

fn paint_step(session: &mut Session, prompt: &FixedPrompt, step: &MenuStep, ansi: bool) {
    let (before, after, status, menu) = *step;
    let menu = test_menu(menu);
    let lines = PromptLines::new(prompt, PromptEditMode::Default, None, before, after, "")
        .with_status(status, false);
    super::tests::reserve_status(&mut session.painter, &lines);
    session
        .painter
        .repaint_buffer(
            prompt,
            &lines,
            PromptEditMode::Default,
            Some(&menu),
            ansi,
            &None,
        )
        .expect("repaint_buffer failed");
    session.screen.sync(session.painter.stdout.captured());
}

/// Paints `steps` in turn, keeping menu rows where it can, and asserts the
/// screen looks as it does when the last step is drawn whole, cell by cell
/// with attributes, cursor included.
fn assert_kept_rows_match_a_full_paint(
    (width, height, anchor): (u16, u16, u16),
    prompt: &FixedPrompt,
    steps: &[MenuStep],
    ansi: bool,
) {
    let mut kept = Session::new(width, height, anchor);
    let mut whole = Session::new(width, height, anchor);
    for (index, step) in steps.iter().enumerate() {
        paint_step(&mut kept, prompt, step, ansi);
        if index + 1 == steps.len() {
            whole.painter.painted_menu = None;
        }
        paint_step(&mut whole, prompt, step, ansi);
    }
    assert_eq!(
        kept.screen.styled_rows(),
        whole.screen.styled_rows(),
        "{steps:?}"
    );
    assert_eq!(kept.screen.cursor(), whole.screen.cursor(), "{steps:?}");
}

#[rstest::rstest]
#[case::right_prompt((30, 10, 1), FixedPrompt::new("> ", "").with_right("RP", false), &[("a", "", "", "alpha  beta\r\ngamma\r\n"), ("ab", "", "", "alpha  BETA\r\ngamma\r\nextra\r\n")])]
#[case::right_prompt_on_last_line((30, 10, 1), FixedPrompt::new("> ", "").with_right("RP", true), &[("a", "", "", "alpha\r\ngamma\r\n"), ("ab", "", "", "alpha\r\ngamma\r\n")])]
#[case::anchor_on_row_0((30, 10, 0), FixedPrompt::new("> ", ""), &[("a", "", "", "alpha\r\ngamma\r\n"), ("ab", "", "", "alpha\r\nGAMMA\r\nextra\r\n")])]
#[case::status_line((30, 10, 0), FixedPrompt::new("> ", ""), &[("a", "", "st", "alpha\r\ngamma\r\n"), ("ab", "", "st2 longer", "alpha\r\nGAMMA\r\n")])]
#[case::multiline_prompt((30, 10, 1), FixedPrompt::new("line1\n> ", ""), &[("a", "", "", "alpha\r\ngamma\r\n"), ("ab", "", "", "ALPHA\r\ngamma\r\n")])]
#[case::multiline_buffer((30, 10, 1), FixedPrompt::new("> ", "").with_multiline("::: "), &[("a\nb", "", "", "alpha\r\ngamma\r\n"), ("a\nbc", "", "", "alpha\r\nGAMMA\r\n")])]
#[case::text_after_the_cursor((30, 10, 1), FixedPrompt::new("> ", ""), &[("a", "zz", "", "alpha\r\ngamma\r\n"), ("a", "z", "", "alpha\r\nGAMMA\r\n")])]
#[case::menu_grows_and_shrinks((30, 10, 1), FixedPrompt::new("> ", ""), &[("a", "", "", "1\r\n2\r\n3\r\n"), ("a", "", "", "1\r\n"), ("a", "", "", "1\r\n2\r\n3\r\n")])]
#[case::full_width_rows((10, 10, 1), FixedPrompt::new("> ", ""), &[("a", "", "", "0123456789\r\nxx\r\n"), ("a", "", "", "abc\r\n0123456789\r\n")])]
#[case::wide_characters((10, 10, 1), FixedPrompt::new("> ", ""), &[("a", "", "", "日本語日本\r\nx\r\n"), ("a", "", "", "日本\r\nx\r\n")])]
#[case::buffer_filling_the_width((10, 10, 1), FixedPrompt::new("> ", ""), &[("abcdefgh", "", "", "alpha\r\ngamma\r\n"), ("abcdefgh", "", "", "alpha\r\nGAMMA\r\n")])]
#[case::menu_ending_on_the_last_row((20, 6, 1), FixedPrompt::new("> ", ""), &[("a", "", "", "1\r\n2\r\n3\r\n4\r\n"), ("a", "", "", "1\r\nX\r\n3\r\n4\r\n")])]
#[case::empty_menu_between((20, 6, 1), FixedPrompt::new("> ", ""), &[("a", "", "", "1\r\n2\r\n"), ("a", "", "", ""), ("a", "", "", "1\r\n2\r\n")])]
#[case::trailing_whitespace((20, 6, 1), FixedPrompt::new("> ", ""), &[("a", "", "", "abc   \r\nd\r\n"), ("a", "", "", "abc\r\nd  \r\n")])]
#[case::bare_line_feed((30, 10, 1), FixedPrompt::new("> ", ""), &[("a", "", "", "abc\r\nxyz\nrow\r\nend\r\n"), ("a", "", "", "abc\r\nxyz\nROW\r\nend\r\n")])]
#[case::tab_widening_a_row_past_the_screen((10, 10, 1), FixedPrompt::new("> ", ""), &[("a", "", "", "abcdefg\tXYZ\r\nrow\r\nr3\r\n"), ("a", "", "", "abcdefg\tXYZ\r\nROW\r\nr3\r\n")])]
#[case::tab_over_old_text((30, 10, 1), FixedPrompt::new("> ", ""), &[("x", "", "", "abcdefghijkl\r\nrow2\r\nrow3\r\nrow4\r\n"), ("x", "", "", "a\tb\r\nrow2\r\nrow3\r\nrow4\r\n")])]
// As a description menu draws one: a style opened on one row and reset rows
// later, so the rows between carry it.
#[case::style_spanning_rows((40, 12, 1), FixedPrompt::new("> ", ""), &[("x", "", "", "h1\r\nh2\r\nh3\r\n\x1b[31mdesc 1\r\ndesc 2a\x1b[0m\r\n"), ("x", "", "", "h1\r\nh2\r\nh3\r\n\x1b[31mdesc 1\r\ndesc 2b\x1b[0m\r\n")])]
#[case::style_changes_on_an_unchanged_row((40, 12, 1), FixedPrompt::new("> ", ""), &[("x", "", "", "h1\r\nh2\r\nh3\r\n\x1b[31mdesc 1\r\ndesc 2\x1b[0m\r\n"), ("x", "", "", "h1\r\nh2\r\nh3\r\n\x1b[32mdesc 1\r\ndesc 2\x1b[0m\r\n")])]
// A zero that is a color argument, a palette index or an RGB component, is no
// reset: the bold opened rows above stays in effect.
#[case::palette_index_0_after_bold((40, 12, 1), FixedPrompt::new("> ", ""), &[("x", "", "", "h1\r\n\x1b[1mbold\r\n\x1b[38;5;0mblack\r\nrow3a\x1b[0m\r\n"), ("x", "", "", "h1\r\n\x1b[1mbold\r\n\x1b[38;5;0mblack\r\nrow3b\x1b[0m\r\n")])]
#[case::rgb_with_a_zero_after_bold((40, 12, 1), FixedPrompt::new("> ", ""), &[("x", "", "", "h1\r\n\x1b[1mbold\r\n\x1b[38;2;255;0;0mred\r\nrow3a\x1b[0m\r\n"), ("x", "", "", "h1\r\n\x1b[1mbold\r\n\x1b[38;2;255;0;0mred\r\nrow3b\x1b[0m\r\n")])]
#[case::reset_among_parameters((40, 12, 1), FixedPrompt::new("> ", ""), &[("x", "", "", "h1\r\n\x1b[1mbold\r\n\x1b[1;0;31mred\r\nrow3a\x1b[0m\r\n"), ("x", "", "", "h1\r\n\x1b[1mbold\r\n\x1b[1;0;31mred\r\nrow3b\x1b[0m\r\n")])]
#[case::highlighted_entry_moves((30, 10, 1), FixedPrompt::new("> ", ""), &[("x", "", "", "\x1b[7m alpha\x1b[0m\r\nbeta\r\ngamma\r\ndelta\r\n"), ("x", "", "", " alpha\r\n\x1b[7m beta\x1b[0m\r\ngamma\r\ndelta\r\n")])]
fn kept_menu_rows_look_like_a_full_paint(
    #[case] size: (u16, u16, u16),
    #[case] prompt: FixedPrompt,
    #[case] steps: &[MenuStep],
    #[values(false, true)] ansi: bool,
) {
    assert_kept_rows_match_a_full_paint(size, &prompt, steps, ansi);
}

// A dumb terminal gets a single stream, so every paint is drawn whole and the
// cursor ends where a full paint leaves it.
#[test]
fn a_dumb_terminal_draws_the_menu_whole() {
    let menu = test_menu("m1\r\nm2\r\n");
    let mut session = Session::new(30, 10, 1);
    session.painter.term_is_dumb = true;

    session.paint_menu("a", "", &menu);
    let sent = session.paint_menu("ab", "", &menu);

    assert!(sent.contains("m1\r\nm2"), "{sent:?}");
}
