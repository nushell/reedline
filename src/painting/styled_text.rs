use nu_ansi_term::Style;

use crate::core_editor::{ensure_grapheme_boundary_next, ensure_grapheme_boundary_prev};
use crate::terminal_extensions::semantic_prompt::{PromptKind, SemanticPromptMarkers};
use crate::Prompt;

use super::utils::strip_ansi;

/// A representation of a buffer with styling, used for doing syntax highlighting
#[derive(Clone)]
pub struct StyledText {
    /// The component, styled parts of the text
    pub buffer: Vec<(Style, String)>,
}

impl Default for StyledText {
    fn default() -> Self {
        Self::new()
    }
}

impl StyledText {
    /// Construct a new `StyledText`
    pub const fn new() -> Self {
        Self { buffer: vec![] }
    }

    /// Add a new styled string to the buffer
    pub fn push(&mut self, styled_string: (Style, String)) {
        self.buffer.push(styled_string);
    }

    /// Restyle the byte range `from..to` of the whole buffer.
    ///
    /// The bounds are snapped outward to grapheme boundaries, so a range that
    /// lands inside a multi-byte character or a combining sequence styles the
    /// whole grapheme instead of splitting it. Pairs the range does not touch
    /// are kept whole; no zero-length pair is ever produced.
    pub fn style_range(&mut self, from: usize, to: usize, new_style: Style) {
        let (from, to) = (from.min(to), from.max(to));
        let mut rebuilt = Vec::with_capacity(self.buffer.len() + 2);
        let mut start = 0;
        for (style, text) in std::mem::take(&mut self.buffer) {
            let end = start + text.len();
            // Where the range meets this pair, in the pair's own byte offsets.
            let lo = ensure_grapheme_boundary_prev(&text, from.clamp(start, end) - start);
            let hi = ensure_grapheme_boundary_next(&text, to.clamp(start, end) - start);

            match (text.get(..lo), text.get(lo..hi), text.get(hi..)) {
                (Some(before), Some(styled), Some(after)) if lo < hi => {
                    let pieces = [(style, before), (new_style, styled), (style, after)];
                    rebuilt.extend(
                        pieces
                            .into_iter()
                            .filter(|(_, piece)| !piece.is_empty())
                            .map(|(style, piece)| (style, piece.to_string())),
                    );
                }
                // The range does not touch this pair, or (after snapping, which
                // makes this unreachable) a bound is not a char boundary: keep
                // the pair whole either way.
                _ => rebuilt.push((style, text)),
            }
            start = end;
        }
        self.buffer = rebuilt;
    }

    /// Render the styled string. We use the insertion point to render around so that
    /// we can properly write out the styled string to the screen and find the correct
    /// place to put the cursor. This assumes a logic that prints the first part of the
    /// string, saves the cursor position, prints the second half, and then restores
    /// the cursor position
    ///
    /// Also inserts the multiline continuation prompt with optional semantic markers
    pub fn render_around_insertion_point(
        &self,
        insertion_point: usize,
        prompt: &dyn Prompt,
        use_ansi_coloring: bool,
        semantic_markers: Option<&dyn SemanticPromptMarkers>,
    ) -> (String, String) {
        let mut current_idx = 0;
        let mut left_string = String::new();
        let mut right_string = String::new();

        let multiline_prompt = prompt.render_prompt_multiline_indicator();
        let prompt_style = Style::new().fg(prompt.get_prompt_multiline_color());

        for pair in &self.buffer {
            if current_idx >= insertion_point {
                right_string.push_str(&render_as_string(
                    pair,
                    &prompt_style,
                    &multiline_prompt,
                    semantic_markers,
                ));
            } else if pair.1.len() + current_idx <= insertion_point {
                left_string.push_str(&render_as_string(
                    pair,
                    &prompt_style,
                    &multiline_prompt,
                    semantic_markers,
                ));
            } else if pair.1.len() + current_idx > insertion_point {
                let offset = insertion_point - current_idx;

                let left_side = pair.1[..offset].to_string();
                let right_side = pair.1[offset..].to_string();

                left_string.push_str(&render_as_string(
                    &(pair.0, left_side),
                    &prompt_style,
                    &multiline_prompt,
                    semantic_markers,
                ));
                right_string.push_str(&render_as_string(
                    &(pair.0, right_side),
                    &prompt_style,
                    &multiline_prompt,
                    semantic_markers,
                ));
            }
            current_idx += pair.1.len();
        }

        if use_ansi_coloring {
            (left_string, right_string)
        } else {
            (strip_ansi(&left_string), strip_ansi(&right_string))
        }
    }

    /// Apply the ANSI style formatting to the full string.
    pub fn render_simple(&self) -> String {
        self.buffer
            .iter()
            .map(|(style, text)| style.paint(text).to_string())
            .collect()
    }

    /// Get the unformatted text as a single continuous string.
    pub fn raw_string(&self) -> String {
        self.buffer.iter().map(|(_, str)| str.as_str()).collect()
    }
}

/// Whether `style` would make a blank cell look different from unstyled text.
///
/// A space has no glyph, so a foreground colour and every weight-ish attribute
/// (bold, dim, italic, blink) have nothing to act on. A background paints the
/// cell, reversed colours paint the foreground *as* the background, and a rule
/// through or under the cell is drawn regardless of what is in it.
///
/// `Style::reverse()` is the one that matters in practice: it is the natural way
/// to write a selection style and it leaves `background` at `None`, so a check
/// for a background alone silently excludes it.
fn shows_on_a_blank_cell(style: &Style) -> bool {
    style.background.is_some() || style.is_reverse || style.is_underline || style.is_strikethrough
}

fn render_as_string(
    renderable: &(Style, String),
    prompt_style: &Style,
    multiline_prompt: &str,
    semantic_markers: Option<&dyn SemanticPromptMarkers>,
) -> String {
    let mut rendered = String::new();

    // Build the formatted multiline prompt with optional semantic markers
    let formatted_multiline_prompt = if let Some(markers) = semantic_markers {
        // Wrap multiline indicator with secondary prompt markers:
        // \n + A;k=s + multiline_prompt + B
        format!(
            "\n{}{}{}",
            markers.prompt_start(PromptKind::Secondary),
            multiline_prompt,
            markers.command_input_start()
        )
    } else {
        format!("\n{multiline_prompt}")
    };

    // `split` consumes the `\n`, so a terminator has no glyph left to carry the
    // style and a selection covering it paints nothing. Every piece but the last
    // had one consumed after it: give those a cell, and the `\n` itself comes
    // back with the multiline prompt.
    //
    // Only when the style would actually show on a blank cell, or a highlighter
    // that emits the buffer as one plain chunk would gain a stray space per line
    // in every mode. The `\r` of a CRLF is the terminator's own, thus the cell
    // replaces it; a trailing `\r` on the *last* piece is buffer content, stays.
    //
    // The cell is a real column, so `required_lines` and `cursor_pos` count it.
    // That shows up only on a selected line which exactly fills the terminal.
    let mut lines = renderable.1.split('\n').peekable();
    while let Some(line) = lines.next() {
        let terminated = lines.peek().is_some();
        let line = if terminated {
            line.strip_suffix('\r').unwrap_or(line)
        } else {
            line
        };

        // One `paint` for line and cell together: two would wrap each in its own
        // escape pair, and the rendered string is asserted verbatim.
        if terminated && shows_on_a_blank_cell(&renderable.0) {
            rendered.push_str(&renderable.0.paint(format!("{line} ")).to_string());
        } else {
            rendered.push_str(&renderable.0.paint(line).to_string());
        }

        if terminated {
            rendered.push_str(&prompt_style.paint(&formatted_multiline_prompt).to_string());
        }
    }
    rendered
}

#[cfg(test)]
mod test {
    use nu_ansi_term::{Color, Style};

    use super::strip_ansi;
    use crate::StyledText;
    use rstest::rstest;

    fn get_styled_text_template() -> (super::StyledText, Style, Style) {
        let before_style = Style::new().on(Color::Black);
        let after_style = Style::new().on(Color::Red);
        (
            super::StyledText {
                buffer: vec![
                    (before_style, "aaa".into()),
                    (before_style, "bbb".into()),
                    (before_style, "ccc".into()),
                ],
            },
            before_style,
            after_style,
        )
    }
    #[test]
    fn style_range_partial_update_one_part() {
        let (styled_text_template, before_style, after_style) = get_styled_text_template();
        let mut styled_text = styled_text_template.clone();
        styled_text.style_range(0, 1, after_style);
        assert_eq!(styled_text.buffer[0], (after_style, "a".into()));
        assert_eq!(styled_text.buffer[1], (before_style, "aa".into()));
        assert_eq!(styled_text.buffer[2], (before_style, "bbb".into()));
        assert_eq!(styled_text.buffer[3], (before_style, "ccc".into()));
    }
    #[test]
    fn style_range_complete_update_one_part() {
        let (styled_text_template, before_style, after_style) = get_styled_text_template();
        let mut styled_text = styled_text_template.clone();
        styled_text.style_range(0, 3, after_style);
        assert_eq!(styled_text.buffer[0], (after_style, "aaa".into()));
        assert_eq!(styled_text.buffer[1], (before_style, "bbb".into()));
        assert_eq!(styled_text.buffer[2], (before_style, "ccc".into()));
        assert_eq!(styled_text.buffer.len(), 3);
    }
    #[test]
    fn style_range_update_over_boundary() {
        let (styled_text_template, before_style, after_style) = get_styled_text_template();
        let mut styled_text = styled_text_template;
        styled_text.style_range(0, 5, after_style);
        assert_eq!(styled_text.buffer[0], (after_style, "aaa".into()));
        assert_eq!(styled_text.buffer[1], (after_style, "bb".into()));
        assert_eq!(styled_text.buffer[2], (before_style, "b".into()));
        assert_eq!(styled_text.buffer[3], (before_style, "ccc".into()));
    }
    #[test]
    fn style_range_update_over_part() {
        let (styled_text_template, before_style, after_style) = get_styled_text_template();
        let mut styled_text = styled_text_template;
        styled_text.style_range(1, 7, after_style);
        assert_eq!(styled_text.buffer[0], (before_style, "a".into()));
        assert_eq!(styled_text.buffer[1], (after_style, "aa".into()));
        assert_eq!(styled_text.buffer[2], (after_style, "bbb".into()));
        assert_eq!(styled_text.buffer[3], (after_style, "c".into()));
        assert_eq!(styled_text.buffer[4], (before_style, "cc".into()));
    }
    #[test]
    fn style_range_last_letter() {
        let (_, before_style, after_style) = get_styled_text_template();
        let mut styled_text = StyledText {
            buffer: vec![(before_style, "asdf".into())],
        };
        styled_text.style_range(3, 4, after_style);
        assert_eq!(styled_text.buffer[0], (before_style, "asd".into()));
        assert_eq!(styled_text.buffer[1], (after_style, "f".into()));
    }
    #[test]
    fn style_range_from_second_to_last() {
        let (_, before_style, after_style) = get_styled_text_template();
        let mut styled_text = StyledText {
            buffer: vec![(before_style, "asdf".into())],
        };
        styled_text.style_range(2, 3, after_style);
        assert_eq!(styled_text.buffer[0], (before_style, "as".into()));
        assert_eq!(styled_text.buffer[1], (after_style, "d".into()));
        assert_eq!(styled_text.buffer[2], (before_style, "f".into()));
    }
    /// The styled flag and text of each pair, so a case can state the whole
    /// segmentation on one line.
    fn segments(text: &StyledText, styled: Style) -> Vec<(bool, &str)> {
        text.buffer
            .iter()
            .map(|(style, s)| (*style == styled, s.as_str()))
            .collect()
    }

    // Byte offsets `from`/`to` against the "aaa" "bbb" "ccc" template. Cases
    // where the range starts or ends on a pair boundary produce no
    // zero-length pairs; the split-and-insert version used to leave them.
    #[rstest]
    #[case::empty_range_mid_pair(4, 4, &[(false, "aaa"), (false, "bbb"), (false, "ccc")])]
    #[case::empty_range_on_boundary(3, 3, &[(false, "aaa"), (false, "bbb"), (false, "ccc")])]
    #[case::empty_range_at_zero(0, 0, &[(false, "aaa"), (false, "bbb"), (false, "ccc")])]
    #[case::starts_on_boundary(3, 5, &[(false, "aaa"), (true, "bb"), (false, "b"), (false, "ccc")])]
    #[case::ends_on_boundary(1, 6, &[(false, "a"), (true, "aa"), (true, "bbb"), (false, "ccc")])]
    #[case::exactly_one_pair(3, 6, &[(false, "aaa"), (true, "bbb"), (false, "ccc")])]
    #[case::whole_buffer(0, 9, &[(true, "aaa"), (true, "bbb"), (true, "ccc")])]
    #[case::runs_past_the_end(7, 20, &[(false, "aaa"), (false, "bbb"), (false, "c"), (true, "cc")])]
    #[case::entirely_past_the_end(12, 20, &[(false, "aaa"), (false, "bbb"), (false, "ccc")])]
    #[case::spans_all_three(1, 8, &[(false, "a"), (true, "aa"), (true, "bbb"), (true, "cc"), (false, "c")])]
    #[case::reversed_bounds_are_swapped(8, 1, &[(false, "a"), (true, "aa"), (true, "bbb"), (true, "cc"), (false, "c")])]
    fn style_range_segmentation(
        #[case] from: usize,
        #[case] to: usize,
        #[case] expected: &[(bool, &str)],
    ) {
        let (mut text, _, after_style) = get_styled_text_template();
        text.style_range(from, to, after_style);
        assert_eq!(segments(&text, after_style), expected);
    }

    /// `from`/`to` are byte offsets; a highlighter that derives them from
    /// chars or graphemes can land inside a multi-byte character. That must
    /// not panic the paint path.
    #[rstest]
    #[case::end_inside_a_char("café", 0, 4)]
    #[case::start_inside_a_char("café", 4, 5)]
    #[case::both_inside_chars("ééé", 1, 3)]
    fn style_range_inside_a_multibyte_char_does_not_panic(
        #[case] text: &str,
        #[case] from: usize,
        #[case] to: usize,
    ) {
        let (_, before_style, after_style) = get_styled_text_template();
        let mut styled = StyledText {
            buffer: vec![(before_style, text.into())],
        };
        styled.style_range(from, to, after_style);
        // Whatever the split policy, the text itself must survive intact.
        assert_eq!(styled.raw_string(), text);
    }

    #[test]
    fn regression_style_range_cargo_run() {
        let (_, before_style, after_style) = get_styled_text_template();
        let mut styled_text = StyledText {
            buffer: vec![
                (before_style, "cargo".into()),
                (before_style, " ".into()),
                (before_style, "run".into()),
            ],
        };
        styled_text.style_range(8, 7, after_style);
        assert_eq!(styled_text.buffer[0], (before_style, "cargo".into()));
        assert_eq!(styled_text.buffer[1], (before_style, " ".into()));
        assert_eq!(styled_text.buffer[2], (before_style, "r".into()));
        assert_eq!(styled_text.buffer[3], (after_style, "u".into()));
        assert_eq!(styled_text.buffer[4], (before_style, "n".into()));
    }

    #[test]
    fn test_render_multiline_without_semantic_markers() {
        let style = Style::new();
        let renderable = (style, "line1\nline2".to_string());
        let prompt_style = Style::new();
        let multiline_prompt = "::: ";

        // Without semantic markers, just get newline + multiline prompt
        let result = super::render_as_string(&renderable, &prompt_style, multiline_prompt, None);
        assert!(result.contains("\n::: "));
        assert!(!result.contains("\x1b]133;A;k=s"));
    }

    #[test]
    fn test_render_multiline_with_semantic_markers() {
        use crate::terminal_extensions::semantic_prompt::Osc133Markers;
        let style = Style::new();
        let renderable = (style, "line1\nline2".to_string());
        let prompt_style = Style::new();
        let multiline_prompt = "::: ";
        let markers = Osc133Markers;

        // With semantic markers, should wrap multiline prompt with A;k=s and B
        let result =
            super::render_as_string(&renderable, &prompt_style, multiline_prompt, Some(&markers));
        // The result should contain the secondary prompt marker before ::: and B after
        assert!(result.contains("\x1b]133;A;k=s\x1b\\"));
        assert!(result.contains("\x1b]133;B\x1b\\"));
    }

    #[test]
    fn test_render_single_line_no_markers_emitted() {
        use crate::terminal_extensions::semantic_prompt::Osc133Markers;
        let style = Style::new();
        let renderable = (style, "single line".to_string());
        let prompt_style = Style::new();
        let multiline_prompt = "::: ";
        let markers = Osc133Markers;

        // Single line should not emit any markers
        let result =
            super::render_as_string(&renderable, &prompt_style, multiline_prompt, Some(&markers));
        assert!(!result.contains("\x1b]133;A;k=s"));
        assert!(!result.contains("\x1b]133;B"));
    }

    // --- the terminator cell (helix block cursor on a `\n`) ---

    #[test]
    fn lone_terminator_chunk_gets_a_painted_cell() {
        let style = Style::new().on(Color::LightGray);
        for text in ["\n", "\r\n"] {
            let result =
                super::render_as_string(&(style, text.to_string()), &Style::new(), "::: ", None);
            assert!(
                strip_ansi(&result).starts_with(' '),
                "{text:?} rendered {result:?}"
            );
            assert_eq!(
                result,
                format!("{}\n::: {}", style.paint(" "), style.paint(""))
            );
        }
    }

    #[test]
    fn the_cell_survives_the_split_at_the_caret() {
        // The real path. Helix rests the caret at the *start* of the covered
        // grapheme, so the insertion point lands exactly on the chunk boundary
        // and the terminator stays whole rather than being split in half.
        let sel = Style::new().on(Color::LightGray);
        let mut styled = StyledText {
            buffer: vec![(Style::new(), "ab\ncd".into())],
        };
        styled.style_range(2, 3, sel);
        let (left, right) =
            styled.render_around_insertion_point(2, &crate::DefaultPrompt::default(), false, None);
        assert_eq!(left, "ab");
        assert!(right.starts_with(' '), "right was {right:?}");
    }

    #[rstest]
    #[case::reverse(Style::new().reverse(), true)]
    #[case::background(Style::new().on(Color::LightGray), true)]
    #[case::underline(Style::new().underline(), true)]
    #[case::strikethrough(Style::new().strikethrough(), true)]
    #[case::foreground(Style::new().fg(Color::Green), false)]
    #[case::bold(Style::new().bold(), false)]
    #[case::plain(Style::new(), false)]
    fn only_a_style_that_shows_on_a_blank_cell_gets_one(
        #[case] style: Style,
        #[case] expected: bool,
    ) {
        // `examples/helix.rs` styles its selection with `Style::new().reverse()`,
        // which leaves `background` at `None` — a background-only check paints
        // nothing there, which is the whole selection invisible on a terminator.
        // A foreground or a weight has no glyph to act on in a space, so those
        // still get no cell.
        let mut styled = StyledText {
            buffer: vec![(Style::new(), "ab\ncd".into())],
        };
        styled.style_range(1, 4, style);
        let rendered: String = styled
            .buffer
            .iter()
            .map(|p| super::render_as_string(p, &Style::new(), "> ", None))
            .collect();
        let expected = if expected { "ab \n> cd" } else { "ab\n> cd" };
        assert_eq!(strip_ansi(&rendered), expected);
    }

    #[test]
    fn a_terminator_inside_a_wider_chunk_gets_a_cell_too() {
        // A word motion or any multi-grapheme selection crossing a line hands
        // the terminator over *inside* a wider chunk, which is the common case:
        // the terminator-only chunk above only happens stepping onto a `\n` from
        // an empty line. Every piece but the last had a `\n` consumed after it.
        let style = Style::new().on(Color::LightGray);
        let result =
            super::render_as_string(&(style, "a\nb".to_string()), &Style::new(), "::: ", None);
        assert_eq!(strip_ansi(&result), "a \n::: b");
    }

    #[test]
    fn a_trailing_terminator_in_a_wider_chunk_gets_a_cell() {
        // The shape a selection through the end of a line produces: content and
        // terminator in one chunk, nothing after it.
        let style = Style::new().on(Color::LightGray);
        for text in ["bar\n", "o bar\n"] {
            let result =
                super::render_as_string(&(style, text.to_string()), &Style::new(), "::: ", None);
            let first = strip_ansi(&result);
            let first = first.split('\n').next().unwrap().to_string();
            assert!(
                first.ends_with(' '),
                "{text:?} rendered first line {first:?}"
            );
        }
    }

    #[test]
    fn a_crlf_terminator_paints_a_cell_and_drops_the_carriage_return() {
        // `split('\n')` leaves the `\r` on the piece, so painting it raw emits a
        // real carriage return mid-line and snaps the terminal to column 0. The
        // `\r` belongs to the terminator, thus the cell replaces it.
        //
        // Asserted on the *raw* string: `strip_ansi` discards `\r` as a control
        // character, so a stripped assertion here would hold either way.
        let style = Style::new().on(Color::LightGray);
        let result =
            super::render_as_string(&(style, "bar\r\n".to_string()), &Style::new(), "::: ", None);
        assert!(!result.contains('\r'), "leaked a CR: {result:?}");
        assert_eq!(strip_ansi(&result).split('\n').next().unwrap(), "bar ");
    }

    #[test]
    fn a_trailing_carriage_return_is_not_a_terminator() {
        // The contrast with the case above: with no `\n` after it the `\r` is
        // buffer content on the last piece, so it survives and gains no cell.
        // Raw again, for the same reason.
        let style = Style::new().on(Color::LightGray);
        let result =
            super::render_as_string(&(style, "bar\r".to_string()), &Style::new(), "::: ", None);
        assert!(result.contains('\r'), "ate buffer content: {result:?}");
        assert_eq!(strip_ansi(&result), "bar");
    }

    #[test]
    fn an_unstyled_wider_terminator_chunk_gains_nothing() {
        // The guard `008df78` cared about, at the generalized predicate: a
        // highlighter emitting the whole buffer as one unstyled chunk must not
        // gain a space on every line in every mode.
        let result = super::render_as_string(
            &(Style::new(), "a\nb\nc".to_string()),
            &Style::new(),
            "::: ",
            None,
        );
        assert_eq!(strip_ansi(&result), "a\n::: b\n::: c");
    }

    #[test]
    fn an_unstyled_terminator_chunk_gains_nothing() {
        // The non-helix exposure: a highlighter that pushes the whole buffer as
        // one chunk emits a bare `"\n"` for a newline-only buffer, with no
        // `style_range` involved. Every mode would gain a stray space.
        use crate::{Highlighter, SimpleMatchHighlighter};
        let styled = SimpleMatchHighlighter::default().highlight("\n", 0);
        assert_eq!(styled.buffer[0].0.background, None, "premise of this test");
        let (left, right) =
            styled.render_around_insertion_point(0, &crate::DefaultPrompt::default(), false, None);
        assert_eq!((left.as_str(), right.starts_with(' ')), ("", false));

        // A foreground-only style is the same case: nothing to show on a blank
        // cell, so no cell.
        let fg = Style::new().fg(Color::Green);
        let out = super::render_as_string(&(fg, "\n".to_string()), &Style::new(), "> ", None);
        assert_eq!(strip_ansi(&out), "\n> ");
    }

    #[test]
    fn an_empty_chunk_is_not_given_a_cell() {
        // `render_around_insertion_point` splits pairs and can hand us an empty
        // string; that must not gain a stray space.
        let style = Style::new().on(Color::LightGray);
        let result = super::render_as_string(&(style, String::new()), &Style::new(), "::: ", None);
        assert_eq!(strip_ansi(&result), "");
    }

    #[test]
    fn selecting_only_the_newline_paints_a_cell_end_to_end() {
        let sel = Style::new().on(Color::LightGray);
        let mut styled = StyledText {
            buffer: vec![(Style::new(), "ab\ncd".into())],
        };
        styled.style_range(2, 3, sel);
        assert_eq!(styled.buffer[1], (sel, "\n".into()));

        let rendered: String = styled
            .buffer
            .iter()
            .map(|p| super::render_as_string(p, &Style::new(), "> ", None))
            .collect();
        assert_eq!(strip_ansi(&rendered), "ab \n> cd");
    }
}
