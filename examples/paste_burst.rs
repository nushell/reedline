// Coalesce a paste that arrives as a rapid stream of key events instead of a
// single bracketed-paste event.
// cargo run --example paste_burst
//
// A paste the terminal injects itself (right click, Ctrl+Shift+V, Cmd+V) is
// delivered as ordinary key events unless bracketed paste is on and the
// terminal supports it, and such a stream cannot be told apart from very fast
// typing by content alone. A `PasteBurstHook` supplies the missing signal — the
// arrival timing — and answers the two questions the read loop asks: is this
// `Enter` a newline inside a paste, and is a burst still coalescing?
//
// Like `Reedline::create()` in general, this example leaves bracketed paste off
// (the default), so every paste takes the key-event path and the hook always
// has something to do. A host that enables `use_bracketed_paste` still wants
// this hook for the terminals that ignore the request.
//
// This example deliberately binds nothing: Ctrl+V does nothing here, because
// the hook is about the paste the terminal injects on its own, not about
// reading the clipboard on a keypress. For that path, see the
// `paste_interceptor` example.
//
// How to try it:
//   1. Copy two or more lines of text.
//   2. Paste them with the terminal's own paste shortcut (right click,
//      Ctrl+Shift+V, Cmd+V). Without the hook each pasted newline submits a
//      line; with it the whole paste lands on one multi-line prompt, its
//      newlines inserted as newlines.
//   3. Press Enter yourself. That one still submits, because it arrives after
//      the burst settled, in a batch of its own.
//
// The detector below is deliberately naive — "characters closer together than
// a human can type" is all it knows. A real host will want to tune the
// thresholds, and where bracketed paste works it should be preferred.
//
// Abort with Ctrl-C or Ctrl-D.

use reedline::{DefaultPrompt, PasteBurstHook, Reedline, Signal};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Two characters arriving closer together than this are machine-fast. Only
/// used to declare a burst; see `enter_is_newline` for why the classification
/// of a newline inside a declared burst must not go back to the clock.
const BURST_GAP: Duration = Duration::from_millis(10);
/// A burst is declared once this many machine-fast characters arrive in a row.
const BURST_MIN_CHARS: usize = 6;
/// How long the read loop waits for more input before calling a burst settled.
const BURST_IDLE: Duration = Duration::from_millis(20);

#[derive(Default)]
struct DetectorState {
    /// When the last character of the current fast run arrived.
    last_char: Option<Instant>,
    /// How many machine-fast characters that run holds so far.
    fast_chars: usize,
    /// Latched once a burst is declared; cleared by `settle`.
    burst: bool,
    /// Text of the last coalesced burst, for the report printed after submit.
    last_burst: Option<String>,
}

#[derive(Default)]
struct TimingBurstDetector {
    state: Mutex<DetectorState>,
}

impl TimingBurstDetector {
    /// True if `now` continues a machine-fast run of characters.
    fn is_fast(state: &DetectorState, now: Instant) -> bool {
        state
            .last_char
            .map_or(false, |last| now.duration_since(last) < BURST_GAP)
    }

    fn take_last_burst(&self) -> Option<String> {
        self.state
            .lock()
            .expect("detector poisoned")
            .last_burst
            .take()
    }
}

impl PasteBurstHook for TimingBurstDetector {
    fn on_char(&self, _c: char) {
        let mut state = self.state.lock().expect("detector poisoned");
        let now = Instant::now();
        if Self::is_fast(&state, now) {
            state.fast_chars += 1;
        } else {
            state.fast_chars = 1;
        }
        state.last_char = Some(now);
        if state.fast_chars >= BURST_MIN_CHARS {
            state.burst = true;
        }
    }

    fn enter_is_newline(&self) -> bool {
        let mut state = self.state.lock().expect("detector poisoned");

        // A declared burst answers this on its own, without consulting the
        // clock. The engine asks the question while parsing a batch it stopped
        // draining because `poll_timeout` found the input idle, so the newest
        // pasted character is always at least that old by now: a freshness test
        // here would answer "not a paste" for every burst, whatever the
        // threshold, and the paste would submit at its first newline.
        //
        // Every Enter in this batch belongs to the paste by construction, since
        // an Enter arriving after the idle window lands in the next batch, and
        // `settle` has cleared the latch by then. That is what keeps an Enter
        // the user presses a moment later a submit.
        if state.burst {
            return true;
        }

        // No burst declared: a short paste such as "aa\nbb", whose lines never
        // reach `BURST_MIN_CHARS`, is not drained, so this question does arrive
        // right after the characters were read and the timing test holds. Keep
        // the run alive across the newline, because the next pasted line
        // continues it.
        let now = Instant::now();
        let embedded = Self::is_fast(&state, now);
        if embedded {
            state.last_char = Some(now);
        }
        embedded
    }

    fn is_burst_active(&self) -> bool {
        // Latched: the read loop asks twice for one burst, once to keep
        // draining and once after the idle flush, and both must agree.
        self.state.lock().expect("detector poisoned").burst
    }

    fn poll_timeout(&self) -> Duration {
        BURST_IDLE
    }

    fn settle(&self) {
        let mut state = self.state.lock().expect("detector poisoned");
        state.last_char = None;
        state.fast_chars = 0;
        state.burst = false;
    }

    fn resolve_burst(&self, coalesced: &str) -> Option<String> {
        // Record the burst so `main` can report it, and return `None` to insert
        // the pasted text as it was. A host that would rather show a compact
        // reference here returns `Some("[Pasted text #1 +12 lines]".into())`
        // and stashes `coalesced` itself — see the `paste_interceptor` example.
        self.state.lock().expect("detector poisoned").last_burst = Some(coalesced.to_string());
        None
    }
}

fn main() -> io::Result<()> {
    println!("Paste two or more lines with your terminal's paste shortcut.");
    println!("Newlines inside the paste are inserted instead of submitting;");
    println!("an Enter you press yourself still submits.");
    println!("Abort with Ctrl-C or Ctrl-D.");

    let detector = Arc::new(TimingBurstDetector::default());
    let mut line_editor = Reedline::create().with_paste_burst(detector.clone());
    let prompt = DefaultPrompt::default();

    loop {
        match line_editor.read_line(&prompt)? {
            Signal::Success(buffer) => {
                if let Some(burst) = detector.take_last_burst() {
                    println!(
                        "Coalesced a paste burst of {} characters over {} lines into one insert.",
                        burst.chars().count(),
                        burst.lines().count()
                    );
                }
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
