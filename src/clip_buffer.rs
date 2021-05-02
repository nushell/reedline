/// Defines an interface to interact with a Clipboard for cut and paste.
///
/// Mutable reference requirements are stricter than always necessary, but the currently used system clipboard API demands them for exclusive access.
pub trait Clipboard {
    fn set(&mut self, content: &str);

    fn get(&mut self) -> String;

    fn clear(&mut self) {
        self.set("");
    }

    fn len(&mut self) -> usize {
        self.get().len()
    }

    fn is_empty(&mut self) -> bool {
        self.get().is_empty()
    }
}

/// Simple buffer that provides a clipboard only usable within the application/library.
#[derive(Default)]
pub struct LocalClipboard {
    content: String,
}

impl LocalClipboard {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Clipboard for LocalClipboard {
    fn set(&mut self, content: &str) {
        self.content = content.to_owned();
    }

    fn get(&mut self) -> String {
        self.content.clone()
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
    }

    impl SystemClipboard {
        pub fn new() -> Self {
            let cb = ClipboardProvider::new().unwrap();
            SystemClipboard { cb }
        }
    }

    impl Clipboard for SystemClipboard {
        fn set(&mut self, content: &str) {
            let _ = self.cb.set_contents(content.to_owned());
        }

        fn get(&mut self) -> String {
            self.cb.get_contents().unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{get_default_clipboard, Clipboard};
    #[test]
    fn reads_back() {
        let mut cb = get_default_clipboard();
        // If the system clipboard is used we want to persist it for the user
        let previous_state = cb.get();

        // Actual test
        cb.set("test");
        assert_eq!(cb.len(), 4);
        assert_eq!(cb.get(), "test".to_owned());
        cb.clear();
        assert!(cb.is_empty());

        // Restore!

        cb.set(&previous_state);
    }
}
