//! To print messages while editing a line
//!
//! See example:
//!
//! ``` shell
//! cargo run --example external_printer --features=external_printer
//! ```
#[cfg(feature = "external_printer")]
use std::{
    fmt::Display,
    sync::mpsc::{sync_channel, Receiver, SendError, SyncSender},
};

#[cfg(feature = "external_printer")]
pub const EXTERNAL_PRINTER_DEFAULT_CAPACITY: usize = 20;

/// An ExternalPrinter allows to print messages of text while editing a line.
/// The message is printed as a new line, the line-edit will continue below the
/// output.
///
/// ## Required feature:
/// `external_printer`
#[cfg(feature = "external_printer")]
#[derive(Debug)]
pub struct ExternalPrinter<T>
where
    T: Display,
{
    sender: SyncSender<T>,
    receiver: Receiver<T>,
}

#[cfg(feature = "external_printer")]
impl<T> ExternalPrinter<T>
where
    T: Display,
{
    /// Creates an ExternalPrinter to store lines with a max_cap
    pub fn new(max_cap: usize) -> Self {
        let (sender, receiver) = sync_channel::<T>(max_cap);
        Self { sender, receiver }
    }
    /// Gets a `SyncSender` to use the printer externally by sending lines to it
    pub fn sender(&self) -> SyncSender<T> {
        self.sender.clone()
    }
    /// Receiver to get messages if any
    pub fn receiver(&self) -> &Receiver<T> {
        &self.receiver
    }

    /// Send a line through the printer's own sender; blocks if `max_cap` is reached.
    pub fn print(&self, line: T) -> Result<(), SendError<T>> {
        self.sender.send(line)
    }

    /// Convenience method to get a line if any, doesn't block.
    pub fn get_line(&self) -> Option<T> {
        self.receiver.try_recv().ok()
    }
}

#[cfg(feature = "external_printer")]
impl<T> Default for ExternalPrinter<T>
where
    T: Display,
{
    fn default() -> Self {
        Self::new(EXTERNAL_PRINTER_DEFAULT_CAPACITY)
    }
}

#[cfg(all(test, feature = "external_printer"))]
mod tests {
    use super::*;

    #[test]
    fn line_sent_from_another_thread_is_received() {
        let printer = ExternalPrinter::<String>::new(2);
        let sender = printer.sender();
        std::thread::spawn(move || sender.send("hello".to_string()).unwrap())
            .join()
            .unwrap();
        assert_eq!(printer.get_line().as_deref(), Some("hello"));
        assert_eq!(printer.get_line(), None);
    }

    #[test]
    fn print_goes_through_the_same_channel() {
        let printer = ExternalPrinter::<String>::new(1);
        printer.print("via print".to_string()).unwrap();
        assert_eq!(printer.get_line().as_deref(), Some("via print"));
    }
}
