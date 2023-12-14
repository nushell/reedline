use crossterm::{event, execute};

/// Helper managing proper setup and teardown of bracketed paste mode
///
/// <https://en.wikipedia.org/wiki/Bracketed-paste>
#[derive(Default)]
pub(crate) struct BracketedPasteGuard {
    enabled: bool,
    active: bool,
}

impl BracketedPasteGuard {
    pub fn set(&mut self, enable: bool) {
        self.enabled = enable;
    }
    pub fn enter(&mut self) {
        if self.enabled && !self.active {
            let _ = execute!(std::io::stdout(), event::EnableBracketedPaste);
            self.active = true;
        }
    }
    pub fn exit(&mut self) {
        if self.active {
            let _ = execute!(std::io::stdout(), event::DisableBracketedPaste);
            self.active = false;
        }
    }
}

impl Drop for BracketedPasteGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = execute!(std::io::stdout(), event::DisableBracketedPaste);
        }
    }
}
