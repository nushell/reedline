use crate::engine::print_message;
use crossterm::event::{poll, read, Event, KeyCode};
use std::{io::Stdout, time::Duration};

// this fn is totally ripped off from crossterm's examples
// it's really a diagnostic routine to see if crossterm is
// even seeing the events. if you press a key and no events
// are printed, it's a good chance your terminal is eating
// those events.
pub fn print_events(stdout: &mut Stdout) -> Result<(), crossterm::ErrorKind> {
    loop {
        // Wait up to 5s for another event
        if poll(Duration::from_millis(5_000))? {
            // It's guaranteed that read() wont block if `poll` returns `Ok(true)`
            let event = read()?;

            // just reuse the print_message fn to show events
            print_message(stdout, &format!("Event::{:?}", event))?;

            // hit the esc key to git out
            if event == Event::Key(KeyCode::Esc.into()) {
                break;
            }
        } else {
            // Timeout expired, no event for 5s
            print_message(stdout, "Waiting for you to type...")?;
        }
    }

    Ok(())
}
