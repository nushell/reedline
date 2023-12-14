use crossterm::{event, execute};

/// Helper managing proper setup and teardown of the kitty keyboard enhancement protocol
///
/// Note that, currently, only the following support this protocol:
/// * [kitty terminal](https://sw.kovidgoyal.net/kitty/)
/// * [foot terminal](https://codeberg.org/dnkl/foot/issues/319)
/// * [WezTerm terminal](https://wezfurlong.org/wezterm/config/lua/config/enable_kitty_keyboard.html)
/// * [notcurses library](https://github.com/dankamongmen/notcurses/issues/2131)
/// * [neovim text editor](https://github.com/neovim/neovim/pull/18181)
/// * [kakoune text editor](https://github.com/mawww/kakoune/issues/4103)
/// * [dte text editor](https://gitlab.com/craigbarnes/dte/-/issues/138)
///
/// Refer to <https://sw.kovidgoyal.net/kitty/keyboard-protocol/> if you're curious.
#[derive(Default)]
pub(crate) struct KittyProtocolGuard {
    enabled: bool,
    active: bool,
}

impl KittyProtocolGuard {
    pub fn set(&mut self, enable: bool) {
        self.enabled = enable && super::kitty_protocol_available();
    }
    pub fn enter(&mut self) {
        if self.enabled && !self.active {
            let _ = execute!(
                std::io::stdout(),
                event::PushKeyboardEnhancementFlags(
                    event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                )
            );

            self.active = true;
        }
    }
    pub fn exit(&mut self) {
        if self.active {
            let _ = execute!(std::io::stdout(), event::PopKeyboardEnhancementFlags);
            self.active = false;
        }
    }
}

impl Drop for KittyProtocolGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = execute!(std::io::stdout(), event::PopKeyboardEnhancementFlags);
        }
    }
}
