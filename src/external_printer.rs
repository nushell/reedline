use std::fmt::Display;

use crossbeam::channel::{bounded, Receiver, SendError, Sender, TryRecvError};

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
    pub const DEFAULT_CAPACITY: usize = 20;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, receiver) = bounded(capacity);
        Self { sender, receiver }
    }

    pub fn sender(&self) -> ExternalPrinter {
        ExternalPrinter(self.sender.clone())
    }

    pub(crate) fn messages(&self) -> Vec<String> {
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
    /// This function blocks if the underlying channel is full.
    pub fn print(&self, string: impl Into<String>) -> Result<(), String> {
        self.0.send(string.into()).map_err(|SendError(s)| s)
    }

    /// Queues a value to be printed via its [`Display`] implementation
    ///
    /// This function blocks if the underlying channel is full.
    pub fn display<T: Display>(&self, value: &T) -> Result<(), String> {
        self.print(value.to_string())
    }
}
