use std::fmt::Display;

use crossbeam::channel::{bounded, Receiver, SendError, Sender, TryRecvError};

/// The channel to which external messages can be sent
///
/// Use the [`sender`](Self::sender) to create [`ExternalPrinter`]s for use in other threads.
///
/// An external printer allows one to print messages of text while editing a line.
/// The message is printed as a new line, and the line-edit will continue below the output.
///
/// ## Required feature:
/// `external_printer`
#[derive(Debug)]
pub struct ExternalPrinterChannel {
    sender: Sender<String>,
    receiver: Receiver<String>,
}

impl ExternalPrinterChannel {
    /// The default maximum number of lines that can be queued up for printing
    ///
    /// If the channel is full, calls to [`ExternalPrinter::print`] will block
    /// and wait for the channel to have spare capacity again.
    pub const DEFAULT_CAPACITY: usize = 20;

    /// Create a new `ExternalPrinterChannel` with default capacity
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new `ExternalPrinterChannel` with the given capacity
    ///
    /// The capacity determines the maximum number of lines that can be queued up for printing
    /// before subsequent calls to [`ExternalPrinter::print`] will block.
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, receiver) = bounded(capacity);
        Self { sender, receiver }
    }

    /// Returns a new [`ExternalPrinter`] which can be used in other threads to queue messages to print
    pub fn sender(&self) -> ExternalPrinter {
        ExternalPrinter(self.sender.clone())
    }

    pub(crate) fn messages(&self) -> Vec<String> {
        // TODO: refactor to use `self.receiver.try_iter()`
        let mut messages = Vec::new();
        loop {
            match self.receiver.try_recv() {
                Ok(string) => {
                    messages.extend(string.lines().map(String::from));
                }
                Err(TryRecvError::Empty) => {
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    debug_assert!(false); // there is always one sender in `self`.
                    break;
                }
            }
        }
        messages
    }
}

impl Default for ExternalPrinterChannel {
    fn default() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }
}

/// An [`ExternalPrinter`] queue messages for printing by sending them to an [`ExternalPrinterChannel`]
///
/// [`ExternalPrinter`] are created through [`ExternalPrinterChannel::sender`]
/// or by cloning an existing [`ExternalPrinter`].
///
/// An external printer allows one to print messages of text while editing a line.
/// The message is printed as a new line, and the line-edit will continue below the output.
///
/// ## Required feature:
/// `external_printer`
#[derive(Debug, Clone)]
pub struct ExternalPrinter(Sender<String>);

impl ExternalPrinter {
    /// Queues a string message to be printed
    ///
    /// This will block if the corresponding [`ExternalPrinterChannel`] is full.
    /// Also, if the channel has been dropped,
    /// then the `String` message that would have been sent is returned as an `Err`.
    ///
    /// To print any type that implements [`Display`] use [`display`](Self::display).
    pub fn print(&self, string: impl Into<String>) -> Result<(), String> {
        self.0.send(string.into()).map_err(|SendError(s)| s)
    }

    /// Queues a value to be printed, using the result of its [`Display`] implementation as the message
    ///
    /// This will block if the corresponding [`ExternalPrinterChannel`] is full.
    /// Also, if the channel has been dropped,
    /// then the `String` message that would have been sent is returned as an `Err`.
    ///
    /// If `T` is a string-like type, use [`print`](Self::display) instead,
    /// since it can be more efficient.
    pub fn display<T: Display>(&self, value: &T) -> Result<(), String> {
        self.print(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_safe() {
        fn send_sync<T: Send + Sync>(_: &T) {}

        let channel = ExternalPrinterChannel::new();
        send_sync(&channel);
        send_sync(&channel.sender())
    }
}
