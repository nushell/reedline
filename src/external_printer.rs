use std::sync::mpsc::{self, Receiver, SyncSender};

/// An external printer allows one to print messages of text while editing a line.
/// The message is printed as a new line, and the line-edit will continue below the output.
///
/// Use [`sender`](Self::sender) to receive a [`SyncSender`] for use in other threads.
///
/// ## Required feature:
/// `external_printer`
#[derive(Debug)]
pub struct ExternalPrinter {
    sender: SyncSender<Vec<u8>>,
    receiver: Receiver<Vec<u8>>,
}

impl ExternalPrinter {
    /// The default maximum number of lines that can be queued up for printing
    pub const DEFAULT_CAPACITY: usize = 20;

    /// Create a new `ExternalPrinter` with the [default capacity](Self::DEFAULT_CAPACITY)
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new `ExternalPrinter` with the given capacity
    ///
    /// The capacity determines the maximum number of lines that can be queued up for printing
    /// before subsequent [`send`](SyncSender::send) calls on the [`sender`](Self::sender) will block.
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::sync_channel(capacity);
        Self { sender, receiver }
    }

    /// Returns a new [`SyncSender`] which can be used in other threads to queue messages to print
    pub fn sender(&self) -> SyncSender<Vec<u8>> {
        self.sender.clone()
    }

    pub(crate) fn receiver(&self) -> &Receiver<Vec<u8>> {
        &self.receiver
    }
}

impl Default for ExternalPrinter {
    fn default() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impls_send() {
        fn impls_send<T: Send>(_: &T) {}

        let printer = ExternalPrinter::new();
        impls_send(&printer);
        impls_send(&printer.sender())
    }

    #[test]
    fn receives_message() {
        let printer = ExternalPrinter::new();
        let sender = printer.sender();
        assert!(sender.send(b"some text".into()).is_ok());
        assert_eq!(printer.receiver().recv(), Ok(b"some text".into()))
    }
}
