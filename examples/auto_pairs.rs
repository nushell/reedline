// Create a reedline object with automatic pairs, plus a `Highlighter` that vetoes
// the auto-pairing in a couple of syntactic positions.
// cargo run --example auto_pairs

use reedline::{
    AutoPairAction, AutoPairContext, AutoPairs, DefaultPrompt, Highlighter, Reedline, Signal,
    StyledText,
};
use std::io;

/// Characters that, when auto-pairs would insert `(open, close)` right at the
/// cursor, allow the pair to actually be typed. These are the same rules an R
/// console (radian, arf) applies:
///
/// 1. **Position**: only pair when the cursor is at the end of the buffer, or
///    immediately before one of these closing characters. Typing `(` in the
///    middle of a word (e.g. between `fo|o`) inserts just `(`, not `()`.
/// 2. **Same-quote containment (quotes only)**: inside an *unclosed* string of
///    the same quote kind, typing that quote character closes the string
///    instead of opening a new pair. Brackets are not subject to this rule:
///    `(` and `[` still pair normally *inside* a string, e.g. typing `(` after
///    `"foo` produces `"foo()`. Only same-kind quote nesting is special-cased.
const FOLLOWING_CLOSERS: [char; 5] = [')', ']', '}', '"', '\''];

struct RConsoleHighlighter;

impl Highlighter for RConsoleHighlighter {
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

        // Rule 1: position. Pair only at the end of the buffer, or right
        // before one of the recognized closing characters.
        let positionally_ok = insertion_point == buffer.len()
            || matches!(
                buffer[insertion_point..].chars().next(),
                Some(next) if FOLLOWING_CLOSERS.contains(&next)
            );
        if !positionally_ok {
            return false;
        }

        // Rule 2: same-quote containment. Only applies when both halves of the
        // pair are the same character (i.e. a quote, not a bracket).
        let (open, close) = context.pair();
        if open == close && is_inside_unclosed_quote(buffer, insertion_point, open) {
            return false;
        }

        true
    }
}

/// Returns `true` if `buffer[..insertion_point]` contains an odd number of
/// unescaped `quote` characters, i.e. the cursor sits inside a string of that
/// quote kind which hasn't been closed yet.
fn is_inside_unclosed_quote(buffer: &str, insertion_point: usize, quote: char) -> bool {
    let mut in_quote = false;
    let mut escaped = false;
    for ch in buffer[..insertion_point].chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            c if c == quote => in_quote = !in_quote,
            _ => {}
        }
    }
    in_quote
}

fn main() -> io::Result<()> {
    let auto_pairs = AutoPairs::new([('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\'')]);
    let mut line_editor = Reedline::create()
        .with_auto_pairs(auto_pairs)
        .with_highlighter(Box::new(RConsoleHighlighter))
        // Auto-pairs and pasting don't mix well without bracketed paste: without
        // it, pasted text is indistinguishable from typing and gets auto-paired
        // character by character (see `Reedline::with_auto_pairs` for details).
        // This mirrors what nushell itself does.
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
