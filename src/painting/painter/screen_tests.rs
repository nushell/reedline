//! Painter tests asserting on what reaches the screen, through
//! [`TestScreen`], rather than on the bytes the painter emits.
//!
//! Each test runs against both painters unless it pins one: the streaming
//! painter in `painter.rs` and the frame painter in `frame.rs`.

use super::*;
use crate::painting::test_screen::TestScreen;
use crate::terminal_extensions::semantic_prompt::Osc133Markers;
use crate::PromptHistorySearch;
use pretty_assertions::assert_eq;
use rstest::rstest;
use std::borrow::Cow;

/// Which paint path a [`Session`] drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Engine {
    Streaming,
    Frame,
}

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
    engine: Engine,
    painter: Painter,
    screen: TestScreen,
}

impl Session {
    fn new(engine: Engine, width: u16, height: u16, anchor_row: u16) -> Self {
        let mut painter = Painter::new(W::screen(width, height));
        painter.terminal_size = (width, height);
        // Pinned rather than read from `TERM`, which differs between a
        // developer's shell and CI.
        painter.term_is_dumb = false;
        if engine == Engine::Frame {
            painter.use_frame_painter_for_test();
        }
        painter.prompt_start_row.mark_verified(anchor_row);
        Self {
            engine,
            painter,
            screen: TestScreen::new(width, height),
        }
    }

    /// As [`Session::new`], with `output` already on screen above the anchor,
    /// left there by the host before the prompt.
    fn after_output(engine: Engine, width: u16, height: u16, output: &[&str]) -> Self {
        let rows = u16::try_from(output.len()).expect("a test prints a few rows");
        let mut session = Self::new(engine, width, height, rows);
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

    /// As [`Session::paint`], with coloring on, so the prompt colors and any
    /// SGR in the text reach the screen.
    fn paint_colored(&mut self, prompt: &FixedPrompt, before: &str, after: &str) {
        let lines = Self::lines(prompt, before, after);
        self.painter
            .repaint_buffer(prompt, &lines, PromptEditMode::Default, None, true, &None)
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

    /// The bytes the painter has emitted so far, to measure one paint's worth.
    fn emitted(&self) -> usize {
        self.painter.stdout.captured().len()
    }

    /// The bytes emitted since `since`, lossily as text.
    fn emitted_since(&self, since: usize) -> String {
        String::from_utf8_lossy(&self.painter.stdout.captured()[since..]).into_owned()
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
        match self.engine {
            Engine::Streaming => {
                let lines = Self::lines(prompt, before, after);
                let layout = self.painter.last_layout.as_ref().expect("painted before");
                let snapshot = self
                    .painter
                    .render_snapshot(&lines, None, before, after, layout);
                self.painter.screen_to_buffer_offset(&snapshot, column, row)
            }
            Engine::Frame => {
                // The frame records grapheme indices; the engine turns them into
                // byte offsets against the raw buffer, as done here.
                let index = self.painter.frame_grapheme_at(column, row)?;
                let buffer = format!("{before}{after}");
                buffer
                    .grapheme_indices(true)
                    .nth(index)
                    .map(|(offset, _)| offset)
            }
        }
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

#[rstest]
fn paints_below_the_host_output_without_touching_it(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::after_output(engine, 20, 6, &["out1", "out2"]);

    session.paint(&FixedPrompt::new("> ", ""), "hello", "");

    assert_eq!(session.screen.row(0), "out1");
    assert_eq!(session.screen.row(1), "out2");
    assert_eq!(session.screen.row(2), "> hello");
    assert_eq!(session.screen.cursor(), (7, 2));
    assert!(session.screen.cursor_visible());
    assert_eq!(session.screen.scrolled(), 0);
}

#[rstest]
fn cursor_sits_between_the_text_before_and_after_it(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 20, 6, 1);

    session.paint(&FixedPrompt::new("> ", ""), "hel", "lo");

    assert_eq!(session.screen.row(1), "> hello");
    assert_eq!(session.screen.cursor(), (5, 1));
}

#[rstest]
fn repaint_draws_over_the_previous_frame(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 20, 6, 1);
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
#[rstest]
fn text_filling_the_width_puts_the_cursor_on_the_next_row(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 10, 6, 1);

    session.paint(&FixedPrompt::new("> ", ""), "abcdefgh", "");

    assert_eq!(session.screen.row(1), "> abcdefgh");
    assert_eq!(session.screen.cursor(), (0, 2));
}

#[rstest]
fn right_prompt_on_last_line_of_a_multiline_prompt(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 20, 6, 1);
    let prompt = FixedPrompt::new("abc\n", "> ").with_right("R", true);

    session.paint(&prompt, "hi", "");

    assert_eq!(session.screen.row(1), "abc");
    assert_eq!(session.screen.row(2), format!("> hi{}R", " ".repeat(15)));
    assert_eq!(session.screen.cursor(), (4, 2));
}

// The commit starts where the painter says the block starts, erases from
// there, and prints the messages in its place, so nothing of the old block
// survives under them.
#[rstest]
fn external_messages_replace_the_block_at_the_anchor(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 20, 10, 5);

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
#[rstest]
fn external_messages_commit_above_the_prompt_and_leave_nothing_behind(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 20, 6, 0);
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
#[rstest]
fn external_messages_after_an_idle_callback_scrolled_the_prompt(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 20, 10, 5);
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
#[rstest]
fn a_wrapped_external_message_pushes_the_prompt_below_its_rows(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 20, 6, 2);
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
#[rstest]
fn a_multi_line_external_message_prints_each_line_from_the_left(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 20, 8, 1);
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
#[rstest]
fn external_messages_at_the_bottom_scroll_into_history_in_order(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 20, 4, 1);
    session.painter.bottom_confirmed_at = Some(4);
    let prompt = FixedPrompt::new("> ", "");

    session.paint(&prompt, "", "");
    for msg in ["m1", "m2", "m3", "m4", "m5"] {
        session.commit(&[msg]);
        session.paint(&prompt, "", "");
    }

    assert_eq!(session.history(), ["", "m1", "m2", "m3", "m4", "m5", ">"]);
}

// As above with the indicator folded into the left prompt. The streaming
// painter's row math counts rows with `str::lines()`, which yields no row
// after a trailing newline, so the prompt reads as one row: the right prompt
// lands a row too high, and the cursor column and buffer start the painter
// derives (used for menu placement and click mapping) are measured from
// `abc`, not the empty row the buffer is on.
#[test]
#[ignore = "row math reads a trailing newline as no row"]
fn right_prompt_on_last_line_after_a_prompt_ending_in_newline() {
    let mut session = Session::new(Engine::Streaming, 20, 6, 1);
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

// The frame has one row model, the grapheme walk, so the trailing newline is
// a row like any other. `PromptLines::cursor_pos` is not asserted: it is the
// streaming painter's helper and still feeds the IDE menu's column.
#[test]
fn frame_right_prompt_on_last_line_after_a_prompt_ending_in_newline() {
    let mut session = Session::new(Engine::Frame, 20, 6, 1);
    let prompt = FixedPrompt::new("abc\n", "").with_right("R", true);

    session.paint(&prompt, "hi", "");

    assert_eq!(session.screen.row(1), "abc");
    assert_eq!(session.screen.row(2), format!("hi{}R", " ".repeat(17)));
    assert_eq!(session.screen.cursor(), (2, 2));
    // The `i` on screen.
    assert_eq!(session.click(&prompt, "hi", "", 1, 2), Some(1));
    // The padding before the right prompt is nobody's.
    assert_eq!(session.click(&prompt, "hi", "", 10, 2), None);
}

// A buffer taller than the screen is drawn as a viewport onto its tail. Which
// rows scroll off is decided by `compute_layout` counting `str::lines()`, so a
// single line wrapping past the bottom reads as fitting: the paint emits one
// row too many and the terminal scrolls, on every paint.
#[test]
#[ignore = "compute_layout counts logical lines, not wrapped rows"]
fn large_buffer_of_one_wrapped_line_stays_on_screen() {
    large_buffer_of_one_wrapped_line_stays_on_screen_with(Engine::Streaming);
}

#[test]
fn frame_large_buffer_of_one_wrapped_line_stays_on_screen() {
    large_buffer_of_one_wrapped_line_stays_on_screen_with(Engine::Frame);
}

fn large_buffer_of_one_wrapped_line_stays_on_screen_with(engine: Engine) {
    let mut session = Session::new(engine, 80, 5, 0);
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

#[rstest]
fn status_line_draws_below_the_buffer(#[values(Engine::Streaming, Engine::Frame)] engine: Engine) {
    let mut session = Session::new(engine, 20, 6, 1);

    session.paint_with(&FixedPrompt::new("> ", ""), "pwd", "", "status", None);

    assert_eq!(session.screen.rows()[1..4], ["> pwd", "status", ""]);
    assert_eq!(session.screen.cursor(), (5, 1));
}

#[rstest]
fn status_line_follows_the_hint_and_a_wrapped_buffer(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 10, 6, 0);

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
#[rstest]
fn a_multi_line_hint_gets_the_multiline_indicator(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let prompt = FixedPrompt::new("> ", "").with_multiline("::: ");
    let hint = nu_ansi_term::Style::new()
        .dimmed()
        .paint("\n  foo: S,\n}")
        .to_string();
    let hint = crate::painting::continue_hint_lines(hint, &prompt, true, None);
    let mut session = Session::new(engine, 20, 6, 0);

    session.paint_with(&prompt, "struct E {", &hint, "", None);

    assert_eq!(
        session.screen.rows()[..4],
        ["> struct E {", ":::   foo: S,", "::: }", ""]
    );
    assert_eq!(session.screen.cursor(), (12, 0));
}

// The rows are reserved like any others, so a prompt on the last row scrolls
// up far enough for a wrapped status line to land on screen.
#[rstest]
fn status_line_at_the_bottom_scrolls_into_view(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::after_output(engine, 10, 4, &["o1", "o2", "o3"]);

    session.paint_with(&FixedPrompt::new("> ", ""), "pwd", "", "0123456789ab", None);

    assert_eq!(session.screen.rows(), ["o3", "> pwd", "0123456789", "ab"]);
    assert_eq!(session.screen.cursor(), (5, 1));
}

#[rstest]
fn repaint_without_a_status_line_erases_it(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 20, 6, 1);
    let prompt = FixedPrompt::new("> ", "");

    session.paint_with(&prompt, "pwd", "", "status", None);
    session.paint_with(&prompt, "pwd", "", "", None);

    assert_eq!(session.screen.rows()[1..3], ["> pwd", ""]);
}

// A menu opens below the cursor's row. The status line takes that row and the
// menu moves down past it, so both stay visible.
#[rstest]
fn status_line_sits_between_the_buffer_and_a_menu(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 20, 8, 1);
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
    if engine == Engine::Streaming {
        assert_eq!(
            session
                .painter
                .last_layout
                .as_ref()
                .map(|l| l.menu_start_row),
            Some(Some(3))
        );
    }
}

// The status line gives way to the buffer: a block that only fits on the
// screen without it is drawn without it, rather than as a large buffer.
#[rstest]
fn buffer_too_tall_for_the_status_line_draws_without_it(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 20, 3, 0);

    session.paint_with(&FixedPrompt::new("> ", ""), "a\r\nb", "", "status", None);

    assert_eq!(session.screen.rows(), ["> a", "b", ""]);
    assert!(!session.painter.large_buffer);
    assert_eq!(session.screen.scrolled(), 0);
}

// Every menu sizes itself from one of the two, so none draws over the rows.
#[test]
fn menus_size_themselves_around_the_reserved_status_rows() {
    let mut session = Session::new(Engine::Streaming, 20, 10, 2);
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
    let mut session = Session::new(Engine::Streaming, 20, 10, 2);

    session.painter.reserve_status_rows(3, 2);

    assert_eq!(session.painter.reserved_status_rows(), 0);
}

// A reservation is for the paint after it. History search paints without
// making one, and must not lay out the rows of the status line before it.
#[rstest]
fn a_paint_that_reserves_nothing_draws_no_status_rows(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 20, 6, 1);
    let prompt = FixedPrompt::new("> ", "");
    session.paint_with(&prompt, "pwd", "", "status", None);

    session.paint(&prompt, "pwd", "");

    assert_eq!(session.screen.rows()[1..3], ["> pwd", ""]);
    if engine == Engine::Streaming {
        let layout = session.painter.last_layout.as_ref().expect("painted");
        assert_eq!(layout.status_start_row, None);
    }
    assert_eq!(session.painter.last_required_lines, 1);
}

// The rows are set aside before the menu sizes itself. A menu that cannot
// shrink to leave them free takes them back, rather than push the block onto
// the large-buffer path.
#[rstest]
fn a_menu_too_tall_for_the_status_line_draws_without_it(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 20, 5, 0);
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
#[rstest]
fn status_line_ending_in_a_newline_at_the_bottom_scrolls_once(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::after_output(engine, 10, 4, &["o1", "o2", "o3"]);
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
    let mut session = Session::new(Engine::Streaming, 20, 8, 1);
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

// The frame painter's own cases: what it draws differently, or what the
// streaming painter never had to get right.

/// The frame's reason to exist: a keystroke touching one row costs that row
/// from the changed cell on, and the rows above it nothing.
#[test]
fn frame_repaint_rewrites_only_the_cells_that_changed() {
    let mut session = Session::new(Engine::Frame, 20, 6, 1);
    let prompt = FixedPrompt::new("> ", "");

    session.paint(&prompt, "one\ntwo", "");
    let before_second_paint = session.emitted();
    session.paint(&prompt, "one\ntwx", "");

    assert_eq!(session.screen.row(1), "> one");
    assert_eq!(session.screen.row(2), "twx");
    assert_eq!(session.screen.cursor(), (3, 2));
    let second_paint = session.emitted_since(before_second_paint);
    assert!(
        !second_paint.contains("one"),
        "the unchanged row was redrawn: {second_paint:?}"
    );
    assert!(
        !second_paint.contains("tw"),
        "the unchanged cells were redrawn: {second_paint:?}"
    );
    assert!(second_paint.contains('x'));
}

/// Typing at the end of the first line leaves the prompt cells alone.
#[test]
fn frame_typing_after_the_prompt_does_not_redraw_the_prompt() {
    let mut session = Session::new(Engine::Frame, 20, 6, 1);
    let prompt = FixedPrompt::new("> ", "");

    session.paint(&prompt, "on", "");
    let before_second_paint = session.emitted();
    session.paint(&prompt, "one", "");

    assert_eq!(session.screen.row(1), "> one");
    assert_eq!(session.screen.cursor(), (5, 1));
    let second_paint = session.emitted_since(before_second_paint);
    assert!(
        !second_paint.contains("> ") && !second_paint.contains("on"),
        "cells before the change were redrawn: {second_paint:?}"
    );
    assert!(second_paint.contains('e'));
}

/// A tail redrawn from mid-row inherits the style the skipped cells set, or
/// the new character comes out in the default color.
#[test]
fn frame_tail_redraw_keeps_the_style_the_skipped_cells_set() {
    let mut session = Session::new(Engine::Frame, 20, 6, 0);
    let prompt = FixedPrompt::new("> ", "");
    let green = vt100::Color::Idx(2);

    session.paint_colored(&prompt, "\x1b[32mabc", "");
    let before_second_paint = session.emitted();
    session.paint_colored(&prompt, "\x1b[32mabcd", "");

    let second_paint = session.emitted_since(before_second_paint);
    assert!(!second_paint.contains("abc"));
    assert_eq!(session.screen.row(0), "> abcd");
    assert_eq!(session.screen.fg(5, 0), green);
    assert_eq!(session.screen.fg(6, 0), vt100::Color::Default);
}

/// A highlighter closing and reopening the same color between tokens changes
/// nothing on screen, so it costs nothing.
#[test]
fn frame_moving_a_reset_redraws_nothing() {
    let mut session = Session::new(Engine::Frame, 20, 6, 0);
    let prompt = FixedPrompt::new("> ", "");

    session.paint_colored(&prompt, "\x1b[32mxy", "");
    let before_second_paint = session.emitted();
    session.paint_colored(&prompt, "\x1b[32mx\x1b[0m\x1b[32my", "");

    let second_paint = session.emitted_since(before_second_paint);
    assert!(
        !second_paint.contains('x') && !second_paint.contains('y'),
        "an unchanged look was redrawn: {second_paint:?}"
    );
}

/// A tail redrawn inside a hyperlink is still part of it, and the row does
/// not leave the link open behind it.
#[test]
fn frame_tail_redraw_stays_inside_a_hyperlink() {
    let mut session = Session::new(Engine::Frame, 30, 6, 0);
    let prompt = FixedPrompt::new("> ", "");

    session.paint_colored(&prompt, "\x1b]8;;http://x\x1b\\ab", "");
    let before_second_paint = session.emitted();
    session.paint_colored(&prompt, "\x1b]8;;http://x\x1b\\abc", "");

    let second_paint = session.emitted_since(before_second_paint);
    let open = second_paint.find("http://x").expect("the link is reopened");
    let glyph = second_paint.find('c').expect("the new glyph is drawn");
    let close = second_paint
        .find("\x1b]8;;\x1b\\")
        .expect("the link is closed");
    assert!(open < glyph && glyph < close, "{second_paint:?}");
    assert!(!second_paint.contains("ab"));
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// The marks go out with the prompt and are not repeated by the keystrokes
/// that follow. Anything that loses the frame sends them again.
#[test]
fn frame_marks_are_sent_once_per_prompt() {
    let mut session = Session::new(Engine::Frame, 20, 6, 1);
    session
        .painter
        .set_semantic_markers(Some(Osc133Markers::boxed()));
    let prompt = FixedPrompt::new("> ", "");

    session.paint(&prompt, "a", "");
    let first_paint = session.emitted_since(0);
    assert_eq!(count(&first_paint, "133;A"), 1);
    assert_eq!(count(&first_paint, "133;B"), 1);
    assert!(first_paint.find("133;A") < first_paint.find('>'));
    assert!(first_paint.find("133;B") < first_paint.find('a'));

    let before_keystroke = session.emitted();
    session.paint(&prompt, "ab", "");
    assert_eq!(count(&session.emitted_since(before_keystroke), "133;"), 0);

    session.painter.invalidate_prompt_start_row();
    let before_repaint = session.emitted();
    session.paint(&prompt, "ab", "");
    let repaint = session.emitted_since(before_repaint);
    assert_eq!(count(&repaint, "133;A"), 1);
    assert_eq!(count(&repaint, "133;B"), 1);
}

/// A prompt whose width changes moves the start of the input, and only that
/// mark follows it.
#[test]
fn frame_a_moved_mark_is_sent_again() {
    let mut session = Session::new(Engine::Frame, 20, 6, 1);
    session
        .painter
        .set_semantic_markers(Some(Osc133Markers::boxed()));

    session.paint(&FixedPrompt::new("> ", ""), "a", "");
    let before_second_paint = session.emitted();
    session.paint(&FixedPrompt::new(">> ", ""), "a", "");

    let second_paint = session.emitted_since(before_second_paint);
    assert_eq!(count(&second_paint, "133;A"), 0);
    assert_eq!(count(&second_paint, "133;B"), 1);
}

/// The right prompt is drawn after the input's start on the same row, so the
/// input's mark has to be repeated to be the last one the terminal saw.
#[test]
fn frame_the_input_mark_is_the_last_one_sent() {
    let mut session = Session::new(Engine::Frame, 20, 6, 1);
    session
        .painter
        .set_semantic_markers(Some(Osc133Markers::boxed()));
    let prompt = FixedPrompt::new("> ", "").with_right("R", false);

    session.paint(&prompt, "a", "");

    let paint = session.emitted_since(0);
    assert!(paint.rfind("133;P") < paint.rfind("133;B"), "{paint:?}");
    assert_eq!(session.screen.cursor(), (3, 1));
}

/// Deleting from the end leaves nothing to draw, only cells to erase.
#[test]
fn frame_shortening_a_row_erases_its_tail() {
    let mut session = Session::new(Engine::Frame, 20, 6, 1);
    let prompt = FixedPrompt::new("> ", "");

    session.paint(&prompt, "one", "");
    let before_second_paint = session.emitted();
    session.paint(&prompt, "on", "");

    assert_eq!(session.screen.row(1), "> on");
    assert_eq!(session.screen.cursor(), (4, 1));
    assert!(!session.emitted_since(before_second_paint).contains("on"));
}

/// A row redrawn alone starts from the SGR state the rows above left it in,
/// or a style opened on one row and closed on a later one is lost on the
/// first partial repaint. Every other test here paints without color, so
/// only this one sees the defect.
#[test]
fn frame_rows_keep_their_color_when_redrawn_alone() {
    let mut session = Session::new(Engine::Frame, 20, 6, 0);
    let prompt = FixedPrompt::new("> ", "");
    let green = vt100::Color::Idx(2);

    // Green opens on the first row and stays on through the second.
    session.paint_colored(&prompt, "\x1b[32mabc\ndef", "");
    assert_eq!(session.screen.fg(2, 0), green);
    assert_eq!(session.screen.fg(0, 1), green);

    // Only the second row changes, so only it is redrawn.
    let before_second_paint = session.emitted();
    session.paint_colored(&prompt, "\x1b[32mabc\ndeg", "");
    let second_paint = session.emitted_since(before_second_paint);
    assert!(!second_paint.contains("abc"));

    assert_eq!(session.screen.row(1), "deg");
    assert_eq!(session.screen.fg(0, 1), green);
    assert_eq!(session.screen.fg(2, 1), green);
    // The reset that ends the row keeps the style off the rest of the screen.
    assert_eq!(session.screen.fg(3, 1), vt100::Color::Default);
}

/// Shrinking by a row clears the row the taller frame left behind.
#[test]
fn frame_clears_the_rows_a_shorter_frame_no_longer_uses() {
    let mut session = Session::new(Engine::Frame, 20, 6, 1);
    let prompt = FixedPrompt::new("> ", "");

    session.paint(&prompt, "one\ntwo\nthree", "");
    assert_eq!(session.screen.row(3), "three");
    session.paint(&prompt, "one", "");

    assert_eq!(session.screen.row(1), "> one");
    assert_eq!(session.screen.row(2), "");
    assert_eq!(session.screen.row(3), "");
    assert_eq!(session.screen.cursor(), (5, 1));
}

/// Growing past the bottom scrolls just enough, and the anchor follows.
#[test]
fn frame_scrolls_to_make_room_and_keeps_the_anchor_aligned() {
    let mut session = Session::after_output(Engine::Frame, 20, 4, &["out1", "out2", "out3"]);
    let prompt = FixedPrompt::new("> ", "");

    session.paint(&prompt, "one", "");
    assert_eq!(session.screen.row(3), "> one");
    assert_eq!(session.screen.scrolled(), 0);

    session.paint(&prompt, "one\ntwo", "");
    assert_eq!(session.screen.scrolled(), 1);
    assert_eq!(session.screen.row(1), "out3");
    assert_eq!(session.screen.row(2), "> one");
    assert_eq!(session.screen.row(3), "two");
    assert_eq!(session.screen.cursor(), (3, 3));
    assert!(session.painter.prompt_anchor_is_verified_for_test());
    assert_eq!(session.painter.prompt_start_row.last_known_row(), 2);
}

// #974: a one-line hint that soft-wraps past the bottom of the screen. The
// streaming painter's large-buffer path clips the hint by counting `\n`, so
// a single wrapped line goes out whole, the terminal scrolls and the prompt
// goes with it. In the frame the hint is rows like any other and the
// viewport keeps the cursor row, so the prompt stays and the hint is clipped.
#[test]
fn frame_long_hint_clips_instead_of_hiding_the_prompt() {
    let mut session = Session::new(Engine::Frame, 20, 4, 0);
    let prompt = FixedPrompt::new("> ", "");
    let hint = "h".repeat(100);
    let lines = PromptLines::new(&prompt, PromptEditMode::Default, None, "ls", "", &hint);

    for _ in 0..2 {
        session
            .painter
            .repaint_buffer(&prompt, &lines, PromptEditMode::Default, None, false, &None)
            .expect("repaint_buffer failed");
        session.screen.sync(session.painter.stdout.captured());
    }

    assert_eq!(session.screen.row(0), format!("> ls{}", "h".repeat(16)));
    assert_eq!(session.screen.row(3), "h".repeat(20));
    assert_eq!(session.screen.cursor(), (4, 0));
    assert_eq!(session.screen.scrolled(), 0);
}

/// A menu opened near the bottom gets the room the paint can scroll into,
/// not the room below the anchor before the scroll.
#[test]
fn frame_menu_gets_the_room_the_scroll_makes() {
    let mut session = Session::new(Engine::Frame, 20, 5, 3);
    let prompt = FixedPrompt::new("> ", "");
    let menu = ReedlineMenu::EngineCompleter(Box::new(super::tests::TestMenu::filling(2)));
    let lines = Session::lines(&prompt, "ab", "");

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

    assert_eq!(session.screen.scrolled(), 3);
    assert_eq!(session.screen.row(0), "> ab");
    assert_eq!(session.screen.row(1), "m1");
    assert_eq!(session.screen.row(4), "m4");
    assert_eq!(session.screen.cursor(), (4, 0));
}

// #477 and #897: the right prompt has to follow the right edge through a
// horizontal resize, with a multiline left prompt and the right prompt on
// its first row.
#[rstest]
fn right_prompt_follows_the_edge_after_a_horizontal_resize(
    #[values(Engine::Streaming, Engine::Frame)] engine: Engine,
) {
    let mut session = Session::new(engine, 40, 6, 1);
    let prompt = FixedPrompt::new("abc\n", "> ").with_right("R", false);

    session.paint(&prompt, "hi", "");
    assert_eq!(session.screen.row(1), format!("abc{}R", " ".repeat(36)));

    session.screen.resize(30, 6);
    session.painter.stdout.terminal_screen().resize(30, 6);
    session.painter.handle_resize(30, 6);
    session.paint(&prompt, "hi", "");

    assert_eq!(session.screen.row(1), format!("abc{}R", " ".repeat(26)));
    assert_eq!(session.screen.row(2), "> hi");
    assert_eq!(session.screen.cursor(), (4, 2));
}
