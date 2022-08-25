#[cfg(feature = "external_printer")]
use crossbeam::channel::{bounded, Receiver, SendError, Sender};

/// An ExternalPrinter allows to print messages of text while editing a line.
/// The message is printed as a new line, the line-edit will continue below the
/// output.
#[cfg(feature = "external_printer")]
#[derive(Debug, Clone)]
pub struct ExternalPrinter {
    sender: Sender<String>,
    receiver: Receiver<String>,
}

#[cfg(feature = "external_printer")]
impl ExternalPrinter {
    /// Creates an ExternalPrinter to store lines with a max_cap
    pub fn new(max_cap: usize) -> Self {
        let (sender, receiver) = bounded::<String>(max_cap);
        Self { sender, receiver }
    }
    /// Gets a Sender to use the printer externally by sending lines to it
    pub fn sender(&self) -> Sender<String> {
        self.sender.clone()
    }
    /// Receiver to get messages if any
    pub fn receiver(&self) -> &Receiver<String> {
        &self.receiver
    }

    /// Convenience method if the whole Printer is cloned, blocks if max_cap is reached.
    ///
    pub fn print(&self, line: &str) -> Result<(), SendError<String>> {
        self.sender.send(line.to_string())
    }

    /// Convenience method to get a line if any, doesn´t block.
    pub fn get_line(&self) -> Option<String> {
        self.receiver.try_recv().ok()
    }
}
