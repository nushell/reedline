pub(crate) mod bracketed_paste;
pub(crate) mod kitty;

/// Return if the terminal supports the kitty keyboard enhancement protocol
///
/// Read more: <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>
///
/// SIDE EFFECT: Touches the terminal file descriptors
pub fn kitty_protocol_available() -> bool {
    crossterm::terminal::supports_keyboard_enhancement().unwrap_or_default()
}
