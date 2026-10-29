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
}

impl FixedPrompt {
    fn new(left: &'static str, indicator: &'static str) -> Self {
        Self {
            left,
            indicator,
            ..Self::default()
        }
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
        "".into()
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
