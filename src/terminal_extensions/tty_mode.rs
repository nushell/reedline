//! Restore the terminal mode after an external program borrowed the terminal
//!
//! An editor is supposed to hand the terminal back as it found it, but not all
//! of them do: on Windows, crossterm and libuv set fixed console flags on exit
//! instead of restoring the previous mode, and on any platform an editor that
//! crashes leaves its own mode behind. Going through crossterm's
//! `enable_raw_mode` does not help, since on Unix it keeps its own record of
//! whether raw mode is on and skips the switch.

/// Saves the terminal mode on creation and writes it back on drop
#[cfg(unix)]
pub(crate) struct TtyModeGuard {
    /// `None` stands for stdin, the same choice crossterm makes
    tty: Option<std::fs::File>,
    termios: rustix::termios::Termios,
}

#[cfg(unix)]
impl TtyModeGuard {
    pub fn save() -> Option<Self> {
        let tty = if rustix::termios::isatty(std::io::stdin()) {
            None
        } else {
            Some(
                std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open("/dev/tty")
                    .ok()?,
            )
        };
        let termios = match &tty {
            Some(file) => rustix::termios::tcgetattr(file),
            None => rustix::termios::tcgetattr(std::io::stdin()),
        }
        .ok()?;
        Some(Self { tty, termios })
    }
}

#[cfg(unix)]
impl Drop for TtyModeGuard {
    fn drop(&mut self) {
        use rustix::termios::{tcsetattr, OptionalActions};
        let _ = match &self.tty {
            Some(file) => tcsetattr(file, OptionalActions::Now, &self.termios),
            None => tcsetattr(std::io::stdin(), OptionalActions::Now, &self.termios),
        };
    }
}

/// Saves the console input mode on creation and writes it back on drop
#[cfg(windows)]
pub(crate) struct TtyModeGuard {
    console: crossterm_winapi::ConsoleMode,
    mode: u32,
}

#[cfg(windows)]
impl TtyModeGuard {
    pub fn save() -> Option<Self> {
        let handle = crossterm_winapi::Handle::current_in_handle().ok()?;
        let console = crossterm_winapi::ConsoleMode::from(handle);
        let mode = console.mode().ok()?;
        Some(Self { console, mode })
    }
}

#[cfg(windows)]
impl Drop for TtyModeGuard {
    fn drop(&mut self) {
        let _ = self.console.set_mode(self.mode);
    }
}

#[cfg(not(any(unix, windows)))]
pub(crate) struct TtyModeGuard;

#[cfg(not(any(unix, windows)))]
impl TtyModeGuard {
    pub fn save() -> Option<Self> {
        None
    }
}
