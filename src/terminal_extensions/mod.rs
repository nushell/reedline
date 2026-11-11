pub(crate) mod bracketed_paste;
pub(crate) mod kitty;
pub(crate) mod raw_mode;
pub mod semantic_prompt;
pub(crate) mod tty_mode;

/// Return if the terminal supports the kitty keyboard enhancement protocol
///
/// Read more: <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>
///
/// SIDE EFFECT: Touches the terminal file descriptors
pub fn kitty_protocol_available() -> bool {
    // The probe needs raw mode for its round trip to the terminal. Entering it
    // here, rather than letting crossterm do it, keeps output processing on
    // while the probe waits for the answer.
    let entered =
        !crossterm::terminal::is_raw_mode_enabled().unwrap_or(false) && raw_mode::enable().is_ok();
    let available = crossterm::terminal::supports_keyboard_enhancement().unwrap_or_default();
    if entered {
        let _ = crossterm::terminal::disable_raw_mode();
    }
    available
}
