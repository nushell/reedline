use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use {
    crossterm::{
        event::{poll, Event, KeyCode, KeyEvent},
        terminal,
    },
    std::{
        io::{stdout, Result, Write},
        time::Duration,
    },
};

fn main() -> Result<()> {
    println!("Ready to print events (Abort with ESC):");
    print_events()?;
    println!();
    Ok(())
}

/// **For debugging purposes only:** Track the terminal events observed by [`Reedline`] and print them.
pub fn print_events() -> Result<()> {
    stdout().flush()?;
    terminal::enable_raw_mode()?;
    // enable kitty protocol
    //
    // Note that, currently, only the following support this protocol:
    // * [kitty terminal](https://sw.kovidgoyal.net/kitty/)
    // * [foot terminal](https://codeberg.org/dnkl/foot/issues/319)
    // * [WezTerm terminal](https://wezfurlong.org/wezterm/config/lua/config/enable_kitty_keyboard.html)
    // * [notcurses library](https://github.com/dankamongmen/notcurses/issues/2131)
    // * [neovim text editor](https://github.com/neovim/neovim/pull/18181)
    // * [kakoune text editor](https://github.com/mawww/kakoune/issues/4103)
    // * [dte text editor](https://gitlab.com/craigbarnes/dte/-/issues/138)
    //
    // Refer to https://sw.kovidgoyal.net/kitty/keyboard-protocol/ if you're curious.
    execute!(
        stdout(),
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        )
    )
    .unwrap();
    let result = print_events_helper();
    execute!(stdout(), PopKeyboardEnhancementFlags).unwrap();
    terminal::disable_raw_mode()?;

    result
}

// this fn is totally ripped off from crossterm's examples
// it's really a diagnostic routine to see if crossterm is
// even seeing the events. if you press a key and no events
// are printed, it's a good chance your terminal is eating
// those events.
fn print_events_helper() -> Result<()> {
    loop {
        // Wait up to 5s for another event
        if poll(Duration::from_millis(5_000))? {
            // It's guaranteed that read() wont block if `poll` returns `Ok(true)`
            let event = crossterm::event::read()?;

            if let Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                state,
            }) = event
            {
                match code {
                    KeyCode::Char(c) => {
                        println!(
                            "Char: {} code: {:#08x}; Modifier {:?}; Flags {:#08b}; Kind {kind:?}; state {state:?}\r",
                            c,
                            u32::from(c),
                            modifiers,
                            modifiers
                        );
                    }
                    _ => {
                        println!(
                            "Keycode: {code:?}; Modifier {modifiers:?}; Flags {modifiers:#08b}; Kind {kind:?}; state {state:?}\r"
                        );
                    }
                }
            } else {
                println!("Event::{event:?}\r");
            }

            // hit the esc key to git out
            if event == Event::Key(KeyCode::Esc.into()) {
                break;
            }
        } else {
            // Timeout expired, no event for 5s
            println!("Waiting for you to type...\r");
        }
    }

    Ok(())
}
