//! Parsing for the GNU readline / inputrc key syntax.
//!
//! reedline's own tables are written in Rust (`KeyModifiers`, `KeyCode`), but
//! frontends that mirror `bind`/`inputrc` entries receive the key as text:
//! `"\C-x"`, `"\e[1~"`, `"^A"`, `"\M-f"`. This module turns that spelling into
//! the pair a [`Keybindings`](crate::Keybindings) table stores, and splits a
//! multi-key ("chord") sequence into its units.
//!
//! Scope is deliberately the classic readline dialect, not every terminal:
//! named keys, single control/meta characters, the VT100/xterm editing-key
//! sequences GNU binds by default, and literal characters.

use crossterm::event::{KeyCode, KeyModifiers};

/// Parse one readline key sequence into the single key it names.
///
/// Returns `None` for a multi-key sequence (use [`parse_readline_chord`]),
/// an empty string, or a spelling this dialect does not define.
pub fn parse_readline_keyseq(seq: &str) -> Option<(KeyModifiers, KeyCode)> {
    let seq = seq.trim();
    if seq.is_empty() {
        return None;
    }
    if let Some(named) = parse_named_key(seq) {
        return Some(named);
    }
    match seq {
        "^[[A" | "\\e[A" | "\\eOA" => Some((KeyModifiers::NONE, KeyCode::Up)),
        "^[[B" | "\\e[B" | "\\eOB" => Some((KeyModifiers::NONE, KeyCode::Down)),
        "^[[C" | "\\e[C" | "\\eOC" => Some((KeyModifiers::NONE, KeyCode::Right)),
        "^[[D" | "\\e[D" | "\\eOD" => Some((KeyModifiers::NONE, KeyCode::Left)),
        // Home / End / Insert / Delete / PgUp / PgDn spellings: the numbered
        // `~` forms and the `O`/`[` letter forms, including rxvt's
        // `\e[7~` / `\e[8~`.
        "\\e[1~" | "^[[1~" | "\\e[7~" | "^[[7~" | "\\e[H" | "^[[H" | "\\eOH" => {
            Some((KeyModifiers::NONE, KeyCode::Home))
        }
        "\\e[4~" | "^[[4~" | "\\e[8~" | "^[[8~" | "\\e[F" | "^[[F" | "\\eOF" => {
            Some((KeyModifiers::NONE, KeyCode::End))
        }
        "\\e[2~" | "^[[2~" => Some((KeyModifiers::NONE, KeyCode::Insert)),
        "\\e[3~" | "^[[3~" => Some((KeyModifiers::NONE, KeyCode::Delete)),
        "\\e[5~" | "^[[5~" | "\\e[I" | "^[[I" => Some((KeyModifiers::NONE, KeyCode::PageUp)),
        "\\e[6~" | "^[[6~" | "\\e[G" | "^[[G" => Some((KeyModifiers::NONE, KeyCode::PageDown)),
        // A bare ESC is one key on its own.
        "\\e" | "^[" => Some((KeyModifiers::NONE, KeyCode::Esc)),
        "^?" => Some((KeyModifiers::NONE, KeyCode::Backspace)),
        _ => parse_control_or_meta(seq).or_else(|| {
            // A single literal character.
            let mut chars = seq.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => Some((KeyModifiers::NONE, KeyCode::Char(ch))),
                _ => None,
            }
        }),
    }
}

/// Split a readline key sequence into its units.
///
/// `"\C-x\C-e"` is two units, `"\C-\e("` is three (`\C-\`, literal `e`,
/// literal `(`  — readline consumes the escape after `\C-\`), `"\e[1~\C-a"`
/// is two. Returns `None` on a dangling unit (`"\C-"`).
pub fn split_readline_units(seq: &str) -> Option<Vec<&str>> {
    let mut rest = seq;
    let mut units = Vec::new();
    while !rest.is_empty() {
        let (unit, tail) = if let Some(tail) = rest
            .strip_prefix("\\C-")
            .or_else(|| rest.strip_prefix("\\M-"))
        {
            let ch = tail.chars().next()?;
            let end = rest.len() - tail.len() + ch.len_utf8();
            (&rest[..end], &rest[end..])
        } else if rest.starts_with("\\e[") {
            let end = rest
                .char_indices()
                .skip(2)
                .find(|(_, c)| c.is_ascii_alphabetic() || *c == '~')
                .map(|(i, c)| i + c.len_utf8())?;
            (&rest[..end], &rest[end..])
        } else if let Some(tail) = rest.strip_prefix("\\e") {
            let end = rest.len() - tail.len();
            (&rest[..end], tail)
        } else if let Some(tail) = rest.strip_prefix('^') {
            let ch = tail.chars().next()?;
            let end = 1 + ch.len_utf8();
            (&rest[..end], &rest[end..])
        } else {
            let ch = rest.chars().next()?;
            (&rest[..ch.len_utf8()], &rest[ch.len_utf8()..])
        };
        units.push(unit);
        rest = tail;
    }
    Some(units)
}

/// Parse a multi-key sequence into its units.
///
/// Returns `None` for a single-key sequence (the flat keymap's business) or
/// when any unit does not parse.
pub fn parse_readline_chord(seq: &str) -> Option<Vec<(KeyModifiers, KeyCode)>> {
    let units = split_readline_units(seq)?;
    if units.len() < 2 {
        return None;
    }
    units
        .into_iter()
        .map(parse_readline_keyseq)
        .collect::<Option<Vec<_>>>()
}

fn parse_named_key(seq: &str) -> Option<(KeyModifiers, KeyCode)> {
    let lowered = seq.to_ascii_lowercase();
    let name = lowered
        .strip_prefix("control+")
        .map(|rest| (KeyModifiers::CONTROL, rest))
        .or_else(|| {
            lowered
                .strip_prefix("ctrl+")
                .map(|rest| (KeyModifiers::CONTROL, rest))
        })
        .or_else(|| {
            lowered
                .strip_prefix("alt+")
                .map(|rest| (KeyModifiers::ALT, rest))
        })
        .or_else(|| {
            lowered
                .strip_prefix("meta+")
                .map(|rest| (KeyModifiers::ALT, rest))
        });
    let (modifiers, name) = name.unwrap_or((KeyModifiers::NONE, lowered.as_str()));
    let code = match name {
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "prior" => KeyCode::PageUp,
        "pagedown" | "next" => KeyCode::PageDown,
        "insert" | "ins" => KeyCode::Insert,
        "delete" | "del" => KeyCode::Delete,
        "backspace" | "bs" => KeyCode::Backspace,
        "tab" => KeyCode::Tab,
        "enter" | "return" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        _ => return None,
    };
    Some((modifiers, code))
}

fn parse_control_or_meta(seq: &str) -> Option<(KeyModifiers, KeyCode)> {
    if let Some(tail) = seq.strip_prefix("\\C-") {
        let mut chars = tail.chars();
        let ch = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        return Some((
            KeyModifiers::CONTROL,
            KeyCode::Char(ch.to_ascii_lowercase()),
        ));
    }
    if let Some(tail) = seq.strip_prefix("\\M-") {
        let mut chars = tail.chars();
        let ch = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        return Some((KeyModifiers::ALT, KeyCode::Char(ch)));
    }
    // `^X` spelling: the character after `^`, upper case folded.
    if let Some(tail) = seq.strip_prefix('^') {
        let mut chars = tail.chars();
        let ch = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        return Some((
            KeyModifiers::CONTROL,
            KeyCode::Char(ch.to_ascii_lowercase()),
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_control_meta_and_caret_spellings() {
        assert_eq!(
            parse_readline_keyseq(r"\C-x"),
            Some((KeyModifiers::CONTROL, KeyCode::Char('x')))
        );
        assert_eq!(
            parse_readline_keyseq(r"\M-f"),
            Some((KeyModifiers::ALT, KeyCode::Char('f')))
        );
        assert_eq!(
            parse_readline_keyseq("^A"),
            Some((KeyModifiers::CONTROL, KeyCode::Char('a')))
        );
        assert_eq!(
            parse_readline_keyseq("^?"),
            Some((KeyModifiers::NONE, KeyCode::Backspace))
        );
    }

    #[test]
    fn parses_terminal_editing_key_spellings() {
        // bash 5.3's own defaults: `"\e[1~": beginning-of-line`,
        // `"\e[4~": end-of-line`, `"\e[3~": delete-char`.
        assert_eq!(
            parse_readline_keyseq(r"\e[1~"),
            Some((KeyModifiers::NONE, KeyCode::Home))
        );
        assert_eq!(
            parse_readline_keyseq(r"\e[4~"),
            Some((KeyModifiers::NONE, KeyCode::End))
        );
        assert_eq!(
            parse_readline_keyseq(r"\e[3~"),
            Some((KeyModifiers::NONE, KeyCode::Delete))
        );
        assert_eq!(
            parse_readline_keyseq(r"\e[5~"),
            Some((KeyModifiers::NONE, KeyCode::PageUp))
        );
        assert_eq!(
            parse_readline_keyseq(r"\e[6~"),
            Some((KeyModifiers::NONE, KeyCode::PageDown))
        );
        // rxvt spellings and a bare ESC.
        assert_eq!(
            parse_readline_keyseq(r"\e[7~"),
            Some((KeyModifiers::NONE, KeyCode::Home))
        );
        assert_eq!(
            parse_readline_keyseq(r"\e"),
            Some((KeyModifiers::NONE, KeyCode::Esc))
        );
    }

    #[test]
    fn parses_named_keys_with_modifiers() {
        assert_eq!(
            parse_readline_keyseq("Home"),
            Some((KeyModifiers::NONE, KeyCode::Home))
        );
        assert_eq!(
            parse_readline_keyseq("Ctrl+Left"),
            Some((KeyModifiers::CONTROL, KeyCode::Left))
        );
        assert_eq!(
            parse_readline_keyseq("Alt+Enter"),
            Some((KeyModifiers::ALT, KeyCode::Enter))
        );
    }

    #[test]
    fn splits_and_parses_chords() {
        assert_eq!(
            parse_readline_chord(r"\C-x\C-e"),
            Some(vec![
                (KeyModifiers::CONTROL, KeyCode::Char('x')),
                (KeyModifiers::CONTROL, KeyCode::Char('e')),
            ])
        );
        // `\C-\` consumes the escape: the rest is literal `e` and `(`.
        assert_eq!(
            parse_readline_chord(r"\C-\e("),
            Some(vec![
                (KeyModifiers::CONTROL, KeyCode::Char('\\')),
                (KeyModifiers::NONE, KeyCode::Char('e')),
                (KeyModifiers::NONE, KeyCode::Char('(')),
            ])
        );
        assert_eq!(
            parse_readline_chord(r"\e[1~\C-a"),
            Some(vec![
                (KeyModifiers::NONE, KeyCode::Home),
                (KeyModifiers::CONTROL, KeyCode::Char('a')),
            ])
        );
        // Single keys and dangling units are not chords.
        assert_eq!(parse_readline_chord(r"\C-x"), None);
        assert_eq!(parse_readline_chord(r"\C-x\C-"), None);
        assert_eq!(parse_readline_chord(""), None);
    }
}
