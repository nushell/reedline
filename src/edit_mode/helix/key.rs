use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use keybindings::InputKey;

/// A simple `InputKey` implementation around `crossterm` types.
///
/// This avoids pulling in the `crossterm` types used by `modalkit` (which can be a different
/// version than the one used by `reedline`).
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub(super) struct HelixKey {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl HelixKey {
    pub(super) fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }
}

impl From<KeyEvent> for HelixKey {
    fn from(event: KeyEvent) -> Self {
        Self::new(event.code, event.modifiers)
    }
}

impl InputKey for HelixKey {
    type Error = std::convert::Infallible;

    fn decompose(&mut self) -> Option<Self> {
        None
    }

    fn from_macro_str(mstr: &str) -> Result<Vec<Self>, Self::Error> {
        Ok(mstr
            .chars()
            .map(|c| HelixKey::new(KeyCode::Char(c), KeyModifiers::NONE))
            .collect())
    }

    fn get_char(&self) -> Option<char> {
        match self.code {
            KeyCode::Char(c) => Some(c),
            _ => None,
        }
    }
}
