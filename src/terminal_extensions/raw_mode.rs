//! Raw mode that leaves output processing on
//!
//! crossterm's raw mode on Unix is `cfmakeraw`, which also clears `OPOST`, the
//! terminal setting that turns `\n` into `\r\n`. Anything another thread or
//! process prints while the editor waits for input, such as output from a
//! background job of the host shell, then starts each line where the last one
//! ended. readline and zle keep output processing on in raw mode, and so does
//! this. Reedline's own output already ends its lines with `\r\n`, which looks
//! the same either way.
//!
//! Leaving raw mode needs nothing extra: crossterm writes back the whole
//! terminal mode it saved on entry.

use std::io;

/// Enables raw mode with `OPOST` and `ONLCR` on, so a bare `\n` still starts
/// a new line
pub(crate) fn enable() -> io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    #[cfg(unix)]
    if let Some(tty) = super::tty_mode::tty() {
        // Without it the editor still works; only stray output staircases.
        let _ = keep_output_processing(&tty);
    }
    Ok(())
}

/// Turns output processing back on for `tty`, with `\n` written as `\r\n`
#[cfg(unix)]
fn keep_output_processing(tty: impl std::os::fd::AsFd) -> io::Result<()> {
    use rustix::termios::{tcgetattr, tcsetattr, OptionalActions, OutputModes};

    let mut termios = tcgetattr(&tty)?;
    termios.output_modes |= OutputModes::OPOST | OutputModes::ONLCR;
    // A `\r` from the painter must stay a carriage return.
    termios.output_modes -= OutputModes::OCRNL;
    tcsetattr(&tty, OptionalActions::Now, &termios)?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::keep_output_processing;
    use rustix::{
        fs::{Mode, OFlags},
        pty::{grantpt, openpt, ptsname, unlockpt, OpenptFlags},
        termios::{tcgetattr, tcsetattr, OptionalActions},
    };
    use std::{
        fs::File,
        io::{Read, Write},
    };

    /// What a newline written to a raw pseudo-terminal comes out as, with or
    /// without output processing turned back on
    fn newline_through_raw_pty(keep_processing: bool) -> Vec<u8> {
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).unwrap();
        grantpt(&master).unwrap();
        unlockpt(&master).unwrap();
        let name = ptsname(&master, Vec::new()).unwrap();
        let slave = rustix::fs::open(
            name.as_c_str(),
            OFlags::RDWR | OFlags::NOCTTY,
            Mode::empty(),
        )
        .unwrap();

        // What crossterm's enable_raw_mode does on Unix
        let mut termios = tcgetattr(&slave).unwrap();
        termios.make_raw();
        tcsetattr(&slave, OptionalActions::Now, &termios).unwrap();

        if keep_processing {
            keep_output_processing(&slave).unwrap();
        }

        // Keep the slave open until the master is read: once its last
        // descriptor closes, Linux may answer the read with EIO.
        let mut slave = File::from(slave);
        slave.write_all(b"a\nb").unwrap();
        let mut out = [0; 16];
        let read = File::from(master).read(&mut out).unwrap();
        out[..read].to_vec()
    }

    #[test]
    fn raw_mode_alone_writes_a_bare_line_feed() {
        assert_eq!(newline_through_raw_pty(false), b"a\nb");
    }

    #[test]
    fn kept_output_processing_writes_a_newline_as_crlf() {
        assert_eq!(newline_through_raw_pty(true), b"a\r\nb");
    }
}
