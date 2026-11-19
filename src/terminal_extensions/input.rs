//! Wait for terminal input without spinning on a terminal that went away
//!
//! When the far end of a pty closes, the tty stays readable for good while
//! `read` returns 0 or EIO. crossterm's unix event source retries that read
//! without end (crossterm-rs/crossterm#793), so a `crossterm::event::read`
//! already blocked when the terminal disappears never returns and pins a core.
//! A process that ignores SIGHUP, or never receives it, then runs forever.
//!
//! On unix reedline therefore does the waiting itself. It polls the tty next to
//! a SIGWINCH self-pipe, reports `POLLHUP`/`POLLERR` as `UnexpectedEof`, and
//! calls into crossterm only once poll says the tty is alive. The self-pipe
//! wakes the wait on a resize, which crossterm learns about through its own
//! signal pipe and reports on the zero-timeout poll that follows.
//!
//! Elsewhere, and on unix when the tty cannot be polled (macOS `poll` rejects
//! some devices with `POLLNVAL`), these are crossterm's own functions.

use std::{io, time::Duration};

use crossterm::event::{self, Event};

/// [`crossterm::event::poll`], but fails instead of spinning on a dead tty
pub(crate) fn poll(timeout: Duration) -> io::Result<bool> {
    #[cfg(unix)]
    if let Some(watch) = unix::watch() {
        return watch.poll(timeout);
    }
    event::poll(timeout)
}

/// [`crossterm::event::read`], but fails instead of spinning on a dead tty
pub(crate) fn read() -> io::Result<Event> {
    #[cfg(unix)]
    if let Some(watch) = unix::watch() {
        while !watch.poll(Duration::MAX)? {}
    }
    event::read()
}

/// Fail if the tty is already gone, before anything asks it for the cursor
/// position: `crossterm::cursor::position` spins on a dead tty as well
pub(crate) fn ensure_tty_alive() -> io::Result<()> {
    #[cfg(unix)]
    if let Some(watch) = unix::watch() {
        watch.wait(Some(Duration::ZERO))?;
    }
    Ok(())
}

#[cfg(unix)]
mod unix {
    use std::{
        io::{self, Read},
        os::{
            fd::{AsFd, OwnedFd},
            unix::net::UnixStream,
        },
        sync::OnceLock,
        time::{Duration, Instant},
    };

    use crossterm::event;
    use rustix::event::{PollFd, PollFlags, Timespec};

    /// Wakes after this many readable-but-no-event rounds hand over to crossterm
    ///
    /// Each round should drain the tty, so repeats mean crossterm reads some
    /// other fd than the one watched here, and waiting would spin.
    const MAX_IDLE_WAKEUPS: u32 = 3;

    /// How long crossterm gets to look for an event once the wait has woken
    ///
    /// `event::poll(Duration::ZERO)` skips what crossterm's `use-dev-tty`
    /// source, which nushell enables, has already read and parsed but not yet
    /// handed out, and it does not read the tty either. Any nonzero timeout
    /// gets past that, and an event that is ready returns at once.
    const CROSSTERM_LOOK: Duration = Duration::from_millis(1);

    pub(super) struct Watch {
        tty: OwnedFd,
        winch: UnixStream,
    }

    static WATCH: OnceLock<Option<Watch>> = OnceLock::new();

    /// The watch, set up on first use while the tty is still alive
    pub(super) fn watch() -> Option<&'static Watch> {
        WATCH.get_or_init(Watch::new).as_ref()
    }

    impl Watch {
        fn new() -> Option<Self> {
            // Stdin if it is a terminal, else /dev/tty, the same choice crossterm
            // makes. Decided once: `isatty` fails on a hung up tty.
            let stdin = io::stdin();
            let tty: OwnedFd = if rustix::termios::isatty(&stdin) {
                stdin.as_fd().try_clone_to_owned().ok()?
            } else {
                std::fs::File::open("/dev/tty").ok()?.into()
            };

            // signal-hook runs SIGWINCH actions in the order they were added.
            // With crossterm's first, its pipe holds the resize by the time this
            // one wakes the wait, so the poll that follows cannot miss it.
            // Starting crossterm's reader adds its action; skip that on a tty
            // already gone, where crossterm would spin.
            if probe(&tty).is_some_and(|tty| !tty.intersects(PollFlags::HUP | PollFlags::ERR)) {
                let _ = event::poll(Duration::ZERO);
            }

            Self::from_tty(tty)
        }

        /// Watch `tty`, unless poll cannot
        pub(super) fn from_tty(tty: OwnedFd) -> Option<Self> {
            // A tty poll cannot watch would wake the wait at once, every time
            if probe(&tty)?.contains(PollFlags::NVAL) {
                return None;
            }

            let (winch, notify) = UnixStream::pair().ok()?;
            winch.set_nonblocking(true).ok()?;
            signal_hook::low_level::pipe::register(signal_hook::consts::SIGWINCH, notify).ok()?;

            Some(Self { tty, winch })
        }

        pub(super) fn poll(&self, timeout: Duration) -> io::Result<bool> {
            let deadline = Instant::now().checked_add(timeout);
            // Probe first, so crossterm is never entered on a dead tty
            self.wait(Some(Duration::ZERO))?;
            let mut idle_wakeups = 0;
            loop {
                if event::poll(CROSSTERM_LOOK)? {
                    return Ok(true);
                }
                let remaining = match deadline {
                    Some(deadline) => match deadline.checked_duration_since(Instant::now()) {
                        Some(left) if !left.is_zero() => Some(left),
                        _ => return Ok(false),
                    },
                    None => None,
                };
                if self.wait(remaining)? {
                    idle_wakeups += 1;
                    if idle_wakeups >= MAX_IDLE_WAKEUPS {
                        return event::poll(remaining.unwrap_or(Duration::MAX));
                    }
                } else {
                    idle_wakeups = 0;
                }
            }
        }

        /// Wait for input or a resize, `None` meaning no timeout. Returns whether
        /// the tty is readable, and fails once the tty has hung up.
        pub(super) fn wait(&self, timeout: Option<Duration>) -> io::Result<bool> {
            let timeout = timeout.and_then(|t| Timespec::try_from(t).ok());
            let mut fds = [
                PollFd::new(&self.tty, PollFlags::IN),
                PollFd::new(&self.winch, PollFlags::IN),
            ];
            match rustix::event::poll(&mut fds, timeout.as_ref()) {
                Ok(_) => {}
                Err(rustix::io::Errno::INTR) => return Ok(false),
                Err(err) => return Err(err.into()),
            }

            let tty = fds[0].revents();
            if tty.intersects(PollFlags::HUP | PollFlags::ERR) {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the terminal went away",
                ));
            }
            if !fds[1].revents().is_empty() {
                let mut buf = [0u8; 64];
                while matches!((&self.winch).read(&mut buf), Ok(n) if n > 0) {}
            }
            Ok(tty.contains(PollFlags::IN))
        }
    }

    /// What poll reports for `tty` right now, `None` if poll fails
    fn probe(tty: &OwnedFd) -> Option<PollFlags> {
        let mut fds = [PollFd::new(tty, PollFlags::IN)];
        rustix::event::poll(&mut fds, Some(&Timespec::default())).ok()?;
        Some(fds[0].revents())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::{
        io::{ErrorKind, Write},
        os::fd::OwnedFd,
        time::{Duration, Instant},
    };

    use rustix::{
        fs::{Mode, OFlags},
        pty::{grantpt, openpt, ptsname, unlockpt, OpenptFlags},
    };

    use super::unix::Watch;

    /// A pty pair: the master, played by the terminal, and the watched slave
    fn pty() -> (std::fs::File, Watch) {
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).unwrap();
        grantpt(&master).unwrap();
        unlockpt(&master).unwrap();
        let path = ptsname(&master, Vec::new()).unwrap();
        let slave: OwnedFd = rustix::fs::open(
            path.as_c_str(),
            OFlags::RDWR | OFlags::NOCTTY,
            Mode::empty(),
        )
        .unwrap();
        // Raw, as under reedline: in canonical mode a key is not readable
        // until a newline completes the line
        let mut termios = rustix::termios::tcgetattr(&slave).unwrap();
        termios.make_raw();
        rustix::termios::tcsetattr(&slave, rustix::termios::OptionalActions::Now, &termios)
            .unwrap();
        let watch = Watch::from_tty(slave).expect("poll can watch a pty");
        (master.into(), watch)
    }

    #[test]
    fn live_idle_tty_is_neither_readable_nor_an_error() {
        let (_master, watch) = pty();
        assert!(!watch.wait(Some(Duration::ZERO)).unwrap());
    }

    /// Wait until `watch` reports the tty readable or fails, sitting out
    /// wakeups from SIGWINCH, which tests running in parallel raise
    fn wait_past_resizes(watch: &Watch) -> std::io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !watch.wait(Some(Duration::from_secs(5)))? {
            assert!(Instant::now() < deadline, "the wait never woke");
        }
        Ok(())
    }

    #[test]
    fn typed_input_makes_the_tty_readable() {
        let (mut master, watch) = pty();
        master.write_all(b"x").unwrap();
        wait_past_resizes(&watch).unwrap();
    }

    #[test]
    fn closed_terminal_is_unexpected_eof() {
        let (master, watch) = pty();
        drop(master);
        let err = watch.wait(Some(Duration::ZERO)).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
    }

    #[test]
    fn closing_the_terminal_wakes_a_blocked_wait() {
        let (master, watch) = pty();
        let closer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(master);
        });
        let err = wait_past_resizes(&watch).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
        closer.join().unwrap();
    }

    #[test]
    fn resize_wakes_a_blocked_wait() {
        let (_master, watch) = pty();
        let start = Instant::now();
        let raiser = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            signal_hook::low_level::raise(signal_hook::consts::SIGWINCH).unwrap();
        });
        assert!(!watch.wait(Some(Duration::from_secs(5))).unwrap());
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "woke on the timeout"
        );
        raiser.join().unwrap();
    }
}
