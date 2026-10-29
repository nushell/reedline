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
    tty: std::os::fd::OwnedFd,
    termios: rustix::termios::Termios,
}

#[cfg(unix)]
impl TtyModeGuard {
    pub fn save() -> Option<Self> {
        use std::os::fd::AsFd;
        // Stdin if it is a terminal, else /dev/tty, the same choice crossterm makes
        let stdin = std::io::stdin();
        let tty = if rustix::termios::isatty(&stdin) {
            stdin.as_fd().try_clone_to_owned().ok()?
        } else {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("/dev/tty")
                .ok()?
                .into()
        };
        let termios = rustix::termios::tcgetattr(&tty).ok()?;
        Some(Self { tty, termios })
    }
}

#[cfg(unix)]
impl Drop for TtyModeGuard {
    fn drop(&mut self) {
        use rustix::termios::{tcsetattr, OptionalActions};
        let _ = tcsetattr(&self.tty, OptionalActions::Now, &self.termios);
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
