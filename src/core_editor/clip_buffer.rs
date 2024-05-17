/// Defines an interface to interact with a Clipboard for cut and paste.
///
/// Mutable reference requirements are stricter than always necessary, but the currently used system clipboard API demands them for exclusive access.
pub trait Clipboard: Send {
    fn set(&mut self, content: &str, mode: ClipboardMode);

    fn get(&mut self) -> (String, ClipboardMode);

    #[allow(dead_code)]
    fn clear(&mut self) {
        self.set("", ClipboardMode::Normal);
    }

    #[allow(dead_code)]
    fn len(&mut self) -> usize {
        self.get().0.len()
    }
}

/// Determines how the content in the clipboard should be inserted
#[derive(Copy, Clone, Debug, Default)]
pub enum ClipboardMode {
    /// As direct content at the current cursor position
    #[default]
    Normal,
    /// As new lines below or above
    Lines,
}

/// Simple buffer that provides a clipboard only usable within the application/library.
#[derive(Default)]
pub struct LocalClipboard {
    content: String,
    mode: ClipboardMode,
}

impl LocalClipboard {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Clipboard for LocalClipboard {
    fn set(&mut self, content: &str, mode: ClipboardMode) {
        content.clone_into(&mut self.content);
        self.mode = mode;
    }

    fn get(&mut self) -> (String, ClipboardMode) {
        (self.content.clone(), self.mode)
    }
}

/// Creates a local clipboard
pub fn get_local_clipboard() -> Box<dyn Clipboard> {
    Box::new(LocalClipboard::new())
}

#[cfg(feature = "system_clipboard")]
pub use system_clipboard::SystemClipboard;

/// Creates a handle for the OS clipboard
#[cfg(feature = "system_clipboard")]
pub fn get_system_clipboard() -> Box<dyn Clipboard> {
    SystemClipboard::new().map_or_else(
        |_e| Box::new(LocalClipboard::new()) as Box<dyn Clipboard>,
        |cb| Box::new(cb),
    )
}

#[cfg(feature = "system_clipboard")]
mod system_clipboard {
    use super::*;
    use arboard::Clipboard as Arboard;

    /// Wrapper around [`arboard`](https://docs.rs/arboard) crate
    ///
    /// Requires that the feature `system_clipboard` is enabled
    pub struct SystemClipboard {
        cb: Arboard,
        local_copy: String,
        mode: ClipboardMode,
    }

    impl SystemClipboard {
        pub fn new() -> Result<Self, arboard::Error> {
            Ok(SystemClipboard {
                cb: Arboard::new()?,
                local_copy: String::new(),
                mode: ClipboardMode::Normal,
            })
        }
    }

    impl Clipboard for SystemClipboard {
        fn set(&mut self, content: &str, mode: ClipboardMode) {
            self.local_copy = content.to_owned();
            let _ = self.cb.set_text(content);
            self.mode = mode;
        }

        fn get(&mut self) -> (String, ClipboardMode) {
            let system_content = self.cb.get_text().unwrap_or_default();
            if system_content == self.local_copy {
                // We assume the content was yanked inside the line editor and the last yank determined the mode.
                (system_content, self.mode)
            } else {
                // Content has changed, default to direct insertion.
                (system_content, ClipboardMode::Normal)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "system_clipboard")]
    use super::get_system_clipboard;
    use super::{get_local_clipboard, ClipboardMode};
    #[test]
    fn reads_back_local() {
        let mut cb = get_local_clipboard();
        // If the system clipboard is used we want to persist it for the user
        let previous_state = cb.get().0;

        // Actual test
        cb.set("test", ClipboardMode::Normal);
        assert_eq!(cb.len(), 4);
        assert_eq!(cb.get().0, "test".to_owned());
        cb.clear();
        assert_eq!(cb.get().0, String::new());

        // Restore!

        cb.set(&previous_state, ClipboardMode::Normal);
    }

    #[cfg(feature = "system_clipboard")]
    #[test]
    fn reads_back_system() {
        let mut cb = get_system_clipboard();
        // If the system clipboard is used we want to persist it for the user
        let previous_state = cb.get().0;

        // Actual test
        cb.set("test", ClipboardMode::Normal);
        assert_eq!(cb.len(), 4);
        assert_eq!(cb.get().0, "test".to_owned());
        cb.clear();
        assert_eq!(cb.get().0, String::new());

        // Restore!

        cb.set(&previous_state, ClipboardMode::Normal);
    }
}
