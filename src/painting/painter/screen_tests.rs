//! Painter tests asserting on what reaches the screen, through
//! [`TestScreen`], rather than on the bytes the painter emits.
//!
//! Each test runs against both painters unless it pins one: the streaming
//! painter in `painter.rs` and the frame painter in `frame.rs`.

use super::*;
use crate::painting::test_screen::TestScreen;
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
struct Session {
    engine: Engine,
    painter: Painter,
    screen: TestScreen,
}

impl Session {
    fn new(engine: Engine, width: u16, height: u16, anchor_row: u16) -> Self {
        let mut painter = Painter::new(W::capture());
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
            session.screen.feed(line.as_bytes());
            session.screen.feed(b"\r\n");
        }
        session
    }

    /// Paint `prompt` with the cursor between `before` and `after`.
    fn paint(&mut self, prompt: &FixedPrompt, before: &str, after: &str) {
        self.paint_with(prompt, before, after, false);
    }

    fn paint_with(&mut self, prompt: &FixedPrompt, before: &str, after: &str, colored: bool) {
        let lines = Self::lines(prompt, before, after);
        self.painter
            .repaint_buffer(
                prompt,
                &lines,
                PromptEditMode::Default,
                None,
                colored,
                &None,
            )
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

/// The frame's reason to exist: a keystroke touching one row costs one row.
#[test]
fn frame_repaint_rewrites_only_the_rows_that_changed() {
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
    assert!(second_paint.contains("twx"));
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
    session.paint_with(&prompt, "\x1b[32mabc\ndef", "", true);
    assert_eq!(session.screen.fg(2, 0), green);
    assert_eq!(session.screen.fg(0, 1), green);

    // Only the second row changes, so only it is redrawn.
    let before_second_paint = session.emitted();
    session.paint_with(&prompt, "\x1b[32mabc\ndeg", "", true);
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
