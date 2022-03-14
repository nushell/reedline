/// Defines an interface to interact with a Clipboard for cut and paste.
///
/// Mutable reference requirements are stricter than always necessary, but the currently used system clipboard API demands them for exclusive access.
pub trait Clipboard: Send {
    fn set(&mut self, content: &str, mode: ClipboardMode);

    fn get(&mut self) -> (String, ClipboardMode);

    fn clear(&mut self) {
        self.set("", ClipboardMode::Normal);
    }

    fn len(&mut self) -> usize {
        self.get().0.len()
    }
}

/// Determines how the content in the clipboard should be inserted
#[derive(Copy, Clone, Debug)]
pub enum ClipboardMode {
    /// As direct content at the current cursor position
    Normal,
    /// As new lines below or above
    Lines,
}

impl Default for ClipboardMode {
    fn default() -> Self {
        ClipboardMode::Normal
    }
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
        self.content = content.to_owned();
        self.mode = mode;
    }

    fn get(&mut self) -> (String, ClipboardMode) {
        (self.content.clone(), self.mode)
    }
}

#[cfg(feature = "system_clipboard")]
pub use system_clipboard::SystemClipboard;

#[cfg(feature = "system_clipboard")]
/// Helper to get a clipboard based on the `system_clipboard` feature flag:
///
/// Enabled -> [`SystemClipboard`], which talks to the system
///
/// Disabled -> [`LocalClipboard`], which supports cutting and pasting limited to the [`crate::Reedline`] instance
pub fn get_default_clipboard() -> SystemClipboard {
    SystemClipboard::new()
}

#[cfg(not(feature = "system_clipboard"))]
/// Helper to get a clipboard based on the `system_clipboard` feature flag:
///
/// Enabled -> `SystemClipboard`, which talks to the system
///
/// Disabled -> [`LocalClipboard`], which supports cutting and pasting limited to the [`crate::Reedline`] instance
pub fn get_default_clipboard() -> LocalClipboard {
    LocalClipboard::new()
}

#[cfg(feature = "system_clipboard")]
mod system_clipboard {
    use super::*;
    use clipboard::{ClipboardContext, ClipboardProvider};

    /// Wrapper around [`clipboard`](https://docs.rs/clipboard) crate
    ///
    /// Requires that the feature `system_clipboard` is enabled
    pub struct SystemClipboard {
        cb: ClipboardContext,
        local_copy: String,
        mode: ClipboardMode,
    }

    impl SystemClipboard {
        pub fn new() -> Self {
            let cb = ClipboardProvider::new().unwrap();
            SystemClipboard {
                cb,
                local_copy: String::new(),
                mode: ClipboardMode::Normal,
            }
        }
    }

    impl Clipboard for SystemClipboard {
        fn set(&mut self, content: &str, mode: ClipboardMode) {
            self.local_copy = content.to_owned();
            let _ = self.cb.set_contents(content.to_owned());
            self.mode = mode;
        }

        fn get(&mut self) -> (String, ClipboardMode) {
            let system_content = self.cb.get_contents().unwrap_or_default();
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
    use super::{get_default_clipboard, Clipboard, ClipboardMode};
    #[test]
    fn reads_back() {
        let mut cb = get_default_clipboard();
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
