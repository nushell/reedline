// Create a reedline object with automatic pairs, plus a `Highlighter` that vetoes
// the auto-pairing in a couple of syntactic positions.
// cargo run --example auto_pairs

use reedline::{
    AutoPairAction, AutoPairContext, AutoPairs, DefaultPrompt, Highlighter, Reedline, Signal,
    StyledText,
};
use std::io;

/// Characters that, when auto-pairs would insert `(open, close)` right at the
/// cursor, allow the pair to actually be typed. This is an illustrative,
/// context-sensitive policy; applications should normally use the parser or
/// language state they already maintain.
///
/// 1. **Same-quote containment (quotes only)**: inside an *unclosed* string of
///    the same quote kind, typing that quote character closes the string
///    instead of opening a new pair. Brackets are not subject to this rule:
///    `(` and `[` still pair normally *inside* a string, e.g. typing `(` after
///    `"foo` produces `"foo()`. Only same-kind quote nesting is special-cased,
///    and quote kinds don't interfere with each other: e.g. in `'foo"`, the
///    `"` is plain text inside the still-open single-quoted string, not a
///    string delimiter, so typing `"` there still opens a new double-quote
///    pair.
struct ContextAwareHighlighter;

impl Highlighter for ContextAwareHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        // This example only cares about `should_auto_pair`; return the buffer
        // unstyled.
        let mut styled_text = StyledText::new();
        styled_text.push((nu_ansi_term::Style::new(), line.to_string()));
        styled_text
    }

    fn should_auto_pair(&self, context: &AutoPairContext<'_>) -> bool {
        // `SkipExistingCloser` (typing a closer that's already sitting under the
        // cursor) and `BackspacePair` (deleting an empty pair) are left at their
        // default behaviour here; only the insertion of a *new* pair is vetoed.
        if context.action() != AutoPairAction::Open {
            return true;
        }

        let buffer = context.buffer();
        let insertion_point = context.insertion_point();

        // An active selection is a deliberate wrapping request.
        if context.selection().is_some() {
            return true;
        }

        // Same-quote containment only applies when both halves of the
        // pair are the same character (i.e. a quote, not a bracket).
        let (open, close) = context.pair();
        if open == close && is_inside_unclosed_quote(buffer, insertion_point, open) {
            return false;
        }

        true
    }
}

/// Returns `true` if the cursor at `insertion_point` sits inside an unclosed
/// string of the given `quote` kind (`'` or `"`).
///
/// Both quote kinds are tracked simultaneously so that a quote of one kind
/// found *inside* an unclosed string of the other kind is not mistaken for a
/// delimiter: once inside a single-quoted string, `"` characters are just
/// text until the single quote closes, and vice versa. Backslash-escaped
/// quotes never toggle either state.
///
/// This scanner is deliberately minimal: it exists to keep the example
/// self-contained, and it is not covered by tests. A real consumer should
/// answer this question with the parser it already owns — which is the point
/// of putting the veto on `Highlighter`, where that parser normally lives.
fn is_inside_unclosed_quote(buffer: &str, insertion_point: usize, quote: char) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for ch in buffer[..insertion_point].chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            _ => {}
        }
    }
    match quote {
        '\'' => in_single,
        '"' => in_double,
        _ => false,
    }
}

fn main() -> io::Result<()> {
    let auto_pairs = AutoPairs::new([('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\'')]);
    let mut line_editor = Reedline::create()
        .with_auto_pairs(auto_pairs)
        .with_highlighter(Box::new(ContextAwareHighlighter))
        // Auto-pairs and pasting don't mix well without bracketed paste: without
        // it, pasted text is indistinguishable from typing and gets auto-paired
        // character by character (see `Reedline::with_auto_pairs` for details).
        // This keeps pasted input from being interpreted as a stream of typed
        // characters.
        .use_bracketed_paste(cfg!(not(target_os = "windows")));
    let prompt = DefaultPrompt::default();

    loop {
        let sig = line_editor.read_line(&prompt)?;
        match sig {
            Signal::Success(buffer) => {
                println!("We processed: {buffer}");
            }
            Signal::CtrlD | Signal::CtrlC => {
                println!("\nAborted!");
                break Ok(());
            }
            _ => {}
        }
    }
}
