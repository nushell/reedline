#[cfg(feature = "helix")]
use std::collections::HashMap;

use crossterm::cursor::SetCursorStyle;

/// Maps cursor shapes to each edit mode (emacs, vi normal & vi insert).
/// If any of the fields is `None`, the cursor won't get changed by Reedline for that mode.
///
/// When the `hx` feature is enabled, the [`custom`](Self::custom) map
/// provides cursor shapes for Helix modes and any `PromptEditMode::Custom`
/// modes, keyed by the string representation of the mode name.
///
/// The [`Default`] implementation leaves all fields as `None` (no cursor
/// changes).  For Helix mode, use
/// [`CursorConfig::with_hx_defaults()`](Self::with_hx_defaults) which
/// pre-populates cursor shapes for Normal, Insert, and Select modes.
#[derive(Default)]
pub struct CursorConfig {
    /// The cursor to be used when in vi insert mode
    pub vi_insert: Option<SetCursorStyle>,
    /// The cursor to be used when in vi normal mode
    pub vi_normal: Option<SetCursorStyle>,
    /// The cursor to be used when in emacs mode
    pub emacs: Option<SetCursorStyle>,
    /// Cursor shapes for Helix and custom edit modes, keyed by mode name.
    /// Helix modes use keys [`HX_CURSOR_NORMAL`], [`HX_CURSOR_INSERT`],
    /// [`HX_CURSOR_SELECT`].
    #[cfg(feature = "helix")]
    pub custom: HashMap<String, SetCursorStyle>,
}

#[cfg(feature = "helix")]
impl CursorConfig {
    /// Create a default config with Helix cursor shapes pre-populated.
    pub fn with_hx_defaults() -> Self {
        let mut custom = HashMap::new();
        custom.insert(HX_CURSOR_NORMAL.to_string(), SetCursorStyle::SteadyBlock);
        custom.insert(HX_CURSOR_INSERT.to_string(), SetCursorStyle::SteadyBar);
        custom.insert(
            HX_CURSOR_SELECT.to_string(),
            SetCursorStyle::SteadyUnderScore,
        );

        Self {
            vi_insert: None,
            vi_normal: None,
            emacs: None,
            custom,
        }
    }
}

/// Cursor config key for Helix Normal mode.
#[cfg(feature = "helix")]
pub const HX_CURSOR_NORMAL: &str = "HX_NORMAL";
/// Cursor config key for Helix Insert mode.
#[cfg(feature = "helix")]
pub const HX_CURSOR_INSERT: &str = "HX_INSERT";
/// Cursor config key for Helix Select mode.
#[cfg(feature = "helix")]
pub const HX_CURSOR_SELECT: &str = "HX_SELECT";

/// Methods available when the `hx` feature is enabled.
#[cfg(feature = "helix")]
impl CursorConfig {
    /// Register a cursor shape for a custom edit mode name.
    ///
    /// Helix modes use the keys [`HX_CURSOR_NORMAL`], [`HX_CURSOR_INSERT`],
    /// and [`HX_CURSOR_SELECT`] (populated by
    /// [`with_hx_defaults`](Self::with_hx_defaults)).
    pub fn with_custom_cursor(mut self, mode_name: &str, style: SetCursorStyle) -> Self {
        self.custom.insert(mode_name.to_string(), style);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_vi_cursors_are_none() {
        let config = CursorConfig::default();
        assert!(config.vi_insert.is_none());
        assert!(config.vi_normal.is_none());
        assert!(config.emacs.is_none());
    }

    #[cfg(feature = "helix")]
    #[test]
    fn hx_defaults_are_set() {
        let config = CursorConfig::with_hx_defaults();
        assert_eq!(
            config.custom.get(HX_CURSOR_NORMAL),
            Some(&SetCursorStyle::SteadyBlock)
        );
        assert_eq!(
            config.custom.get(HX_CURSOR_INSERT),
            Some(&SetCursorStyle::SteadyBar)
        );
        assert_eq!(
            config.custom.get(HX_CURSOR_SELECT),
            Some(&SetCursorStyle::SteadyUnderScore)
        );
    }

    #[cfg(feature = "helix")]
    #[test]
    fn hx_builders_override_defaults() {
        let config = CursorConfig::with_hx_defaults()
            .with_custom_cursor(HX_CURSOR_NORMAL, SetCursorStyle::BlinkingBlock)
            .with_custom_cursor(HX_CURSOR_SELECT, SetCursorStyle::BlinkingBar);
        assert_eq!(
            config.custom.get(HX_CURSOR_NORMAL),
            Some(&SetCursorStyle::BlinkingBlock)
        );
        assert_eq!(
            config.custom.get(HX_CURSOR_SELECT),
            Some(&SetCursorStyle::BlinkingBar)
        );
    }
}
