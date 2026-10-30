mod command;
mod motion;
mod parser;
mod vi_keybindings;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
pub use vi_keybindings::{
    default_vi_insert_keybindings, default_vi_normal_keybindings, default_vi_visual_keybindings,
};

use super::{
    combine_events, is_meta_char, is_plain_char, is_text_char, normalized_key_combination,
    parse_non_key_event, EditMode, SequenceResolution, SequenceResolver,
};
use crate::{
    edit_mode::{
        keybindings::{KeyCombination, Keybindings},
        vi::parser::parse,
    },
    enums::{EditCommand, EventStatus, ReedlineEvent, ReedlineRawEvent},
    Direction, MotionTarget, PromptEditMode, PromptViMode,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ViMode {
    Normal,
    Insert,
    Visual,
}

impl From<PromptViMode> for ViMode {
    fn from(mode: PromptViMode) -> Self {
        match mode {
            PromptViMode::Normal => ViMode::Normal,
            PromptViMode::Insert => ViMode::Insert,
            PromptViMode::Visual => ViMode::Visual,
        }
    }
}

/// This parses incoming input `Event`s like a Vi-Style editor
///
/// In every mode an unbound `Alt-<char>` is read as `Esc` followed by
/// `<char>`, the meta convention of readline and zsh: `Alt-k` from insert
/// recalls the previous line, `Alt-I` jumps to the line start and stays in
/// insert. Binding the Alt chord itself takes precedence and is the per-key
/// opt-out.
pub struct Vi {
    cache: Vec<char>,
    insert_keybindings: Keybindings,
    normal_keybindings: Keybindings,
    visual_keybindings: Keybindings,
    mode: ViMode,
    previous: Option<ReedlineEvent>,
    // last f, F, t, T motion for ; and ,
    last_char_search: Option<MotionTarget>,
    // half-typed explicit multi-key binding
    resolver: SequenceResolver,
}

impl Default for Vi {
    fn default() -> Self {
        Self::new(
            default_vi_insert_keybindings(),
            default_vi_normal_keybindings(),
            default_vi_visual_keybindings(),
        )
    }
}

impl Vi {
    /// Creates Vi editor using defined keybindings, one table per mode, in the
    /// order insert, normal, visual.
    ///
    /// Visual reads only its own table, so a binding meant for both goes into
    /// both. [`default_vi_visual_keybindings`] is the normal table with its
    /// navigation keys rebound to extend the selection, so build a custom
    /// visual table on top of it rather than on the normal one.
    pub fn new(
        insert_keybindings: Keybindings,
        normal_keybindings: Keybindings,
        visual_keybindings: Keybindings,
    ) -> Self {
        // Every field is spelled out, since `Default` is built through here.
        Self {
            insert_keybindings,
            normal_keybindings,
            visual_keybindings,
            cache: Vec::new(),
            mode: ViMode::Insert,
            previous: None,
            last_char_search: None,
            resolver: SequenceResolver::new(),
        }
    }

    /// The keybinding table for the mode the machine is in.
    fn keybindings(&self) -> &Keybindings {
        match self.mode {
            ViMode::Normal => &self.normal_keybindings,
            ViMode::Insert => &self.insert_keybindings,
            ViMode::Visual => &self.visual_keybindings,
        }
    }
}

impl EditMode for Vi {
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        match event.into() {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => self.dispatch_key(modifiers, code),
            event => parse_non_key_event(event),
        }
    }

    fn edit_mode(&self) -> PromptEditMode {
        match self.mode {
            ViMode::Normal => PromptEditMode::Vi(PromptViMode::Normal),
            // Visual maps to its own policy (min-width-1 `Block`) so the commit
            // boundary widens the cursor into a selection on entry.
            ViMode::Visual => PromptEditMode::Vi(PromptViMode::Visual),
            ViMode::Insert => PromptEditMode::Vi(PromptViMode::Insert),
        }
    }

    fn handle_mode_specific_event(&mut self, event: ReedlineEvent) -> EventStatus {
        match event {
            ReedlineEvent::SwitchMode(PromptEditMode::Vi(target)) => {
                // Abandon a half-typed sequence (`3f`), or the next key would
                // be parsed as its argument in the new mode.
                self.cache.clear();
                self.resolver.reset();
                self.mode = ViMode::from(target);
                EventStatus::Handled
            }
            _ => EventStatus::Inapplicable,
        }
    }

    fn has_pending_sequence(&self) -> bool {
        self.resolver.is_pending()
    }

    fn reset_pending_sequence(&mut self) {
        self.resolver.reset();
    }

    fn flush_pending_sequence(&mut self) -> ReedlineEvent {
        let pending = self.resolver.take_pending();
        if pending.is_empty() {
            return ReedlineEvent::None;
        }
        combine_events(self.flush_keys(&pending))
    }
}

/// Append `ReedlineEvent::Repaint` unless the event already ends with one,
/// flattening into an existing `Multiple` rather than nesting it.
fn with_trailing_repaint(event: ReedlineEvent) -> ReedlineEvent {
    match event {
        ReedlineEvent::Repaint => event,
        ReedlineEvent::Multiple(mut events) => {
            if events.last() != Some(&ReedlineEvent::Repaint) {
                events.push(ReedlineEvent::Repaint);
            }
            ReedlineEvent::Multiple(events)
        }
        event => ReedlineEvent::Multiple(vec![event, ReedlineEvent::Repaint]),
    }
}

impl Vi {
    /// Route one keypress: the meta prefix first, then an explicit chord if
    /// one is live (or may start here), then the mode's own grammar and
    /// tables.
    ///
    /// **Vi grammar wins.** A chord may only start where an explicit binding
    /// would fire: a key that is bound, or a non-character key. An unbound
    /// plain character belongs to the vi parser, so `dd`, `3f` and `g g`
    /// cannot be shadowed by a sequence. Once a chord *is* pending it owns the
    /// next keys, so the second `j` of `j j` is not parsed as a motion.
    fn dispatch_key(&mut self, modifiers: KeyModifiers, code: KeyCode) -> ReedlineEvent {
        if let Some(event) = self.meta_as_escape(modifiers, code) {
            return event;
        }

        // Esc always cancels a half-typed chord: in insert it must leave the
        // mode, not insert the pending characters first.
        if self.resolver.is_pending() && code == KeyCode::Esc {
            self.resolver.reset();
        }

        if self.resolver.is_pending() || self.chord_may_start(modifiers, code) {
            if let Some(event) = self.dispatch_chord(modifiers, code) {
                return event;
            }
        }

        self.dispatch_plain(modifiers, code)
    }

    /// Whether this key may begin an explicit chord.
    fn chord_may_start(&self, modifiers: KeyModifiers, code: KeyCode) -> bool {
        if !self.cache.is_empty() || code == KeyCode::Esc {
            return false;
        }
        let key = self.chord_combination(modifiers, code);
        if !self
            .keybindings()
            .is_sequence_prefix(std::slice::from_ref(&key))
        {
            return false;
        }
        let has_binding = self
            .keybindings()
            .find_binding(key.modifier, key.key_code)
            .is_some();
        // Esc and Enter carry a vi/default action of their own. A chord may
        // only take them when the user bound the key explicitly.
        if !has_binding && code == KeyCode::Enter {
            return false;
        }
        match self.mode {
            // Insert has no grammar to defer to.
            ViMode::Insert => true,
            ViMode::Normal | ViMode::Visual => match code {
                KeyCode::Char(_) if is_plain_char(modifiers) => has_binding,
                _ => true,
            },
        }
    }

    /// The key spelling the mode's table uses.
    fn chord_combination(&self, modifiers: KeyModifiers, code: KeyCode) -> KeyCombination {
        match (self.mode, code) {
            // Normal/visual look bindings up by the lowercased character even
            // without a modifier, so their chords use the same spelling.
            (ViMode::Normal | ViMode::Visual, KeyCode::Char(c)) => KeyCombination {
                modifier: modifiers,
                key_code: KeyCode::Char(c.to_ascii_lowercase()),
            },
            _ => normalized_key_combination(modifiers, code),
        }
    }

    /// Feed one key to the live chord. `None` means the chord does not claim
    /// it and the ordinary dispatch should run.
    fn dispatch_chord(&mut self, modifiers: KeyModifiers, code: KeyCode) -> Option<ReedlineEvent> {
        let key = self.chord_combination(modifiers, code);
        let resolution = match self.mode {
            ViMode::Normal => self.resolver.resolve(&self.normal_keybindings, key),
            ViMode::Insert => self.resolver.resolve(&self.insert_keybindings, key),
            ViMode::Visual => self.resolver.resolve(&self.visual_keybindings, key),
        };
        match resolution {
            SequenceResolution::Pending => Some(ReedlineEvent::None),
            SequenceResolution::Complete(event) => Some(event),
            SequenceResolution::NoMatch { flush } if flush.is_empty() => None,
            SequenceResolution::NoMatch { flush } => {
                let mut events = self.flush_keys(&flush);
                let current = self.dispatch_key(modifiers, code);
                if current != ReedlineEvent::None {
                    events.push(current);
                }
                Some(combine_events(events))
            }
        }
    }

    /// Resolve keys that were pending when a sequence was interrupted or timed
    /// out. A complete sequence inside the prefix wins; the rest is replayed
    /// through the mode's single-key fallback.
    fn flush_keys(&mut self, keys: &[KeyCombination]) -> Vec<ReedlineEvent> {
        let mut events = Vec::new();
        let mut rest = keys;
        loop {
            let exact = match self.mode {
                ViMode::Normal => self.normal_keybindings.longest_sequence_prefix(rest),
                ViMode::Insert => self.insert_keybindings.longest_sequence_prefix(rest),
                ViMode::Visual => self.visual_keybindings.longest_sequence_prefix(rest),
            };
            match exact {
                Some((len, event)) => {
                    events.push(event);
                    rest = &rest[len..];
                }
                None => break,
            }
        }
        for key in rest {
            let event = self.fallback_for(key);
            if event != ReedlineEvent::None {
                events.push(event);
            }
        }
        events
    }

    /// One flushed key, handled as if it had never started a chord.
    fn fallback_for(&self, key: &KeyCombination) -> ReedlineEvent {
        match self.mode {
            ViMode::Insert => self.insert_single_key(key.modifier, key.key_code),
            ViMode::Normal | ViMode::Visual => self
                .keybindings()
                .find_binding(key.modifier, key.key_code)
                .unwrap_or(ReedlineEvent::None),
        }
    }

    /// The insert-mode table plus self-insert, shared by ordinary dispatch and
    /// chord fallback.
    fn insert_single_key(&self, modifier: KeyModifiers, code: KeyCode) -> ReedlineEvent {
        let KeyCode::Char(c) = code else {
            return self
                .keybindings()
                .find_binding(modifier, code)
                .unwrap_or(ReedlineEvent::None);
        };
        // Note. The modifier can also be a combination of modifiers, for
        // example:
        //     KeyModifiers::CONTROL | KeyModifiers::ALT
        //     KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
        //
        // Mixed modifiers are used by non american keyboards that have extra
        // keys like 'alt gr'. Keep this in mind if in the future there are
        // cases where an event is not being captured
        let c = match modifier {
            KeyModifiers::NONE => c,
            _ => c.to_ascii_lowercase(),
        };

        self.keybindings()
            .find_binding(modifier, KeyCode::Char(c))
            .unwrap_or_else(|| {
                if is_text_char(modifier) {
                    ReedlineEvent::Edit(vec![EditCommand::InsertChar(
                        if modifier == KeyModifiers::SHIFT {
                            c.to_ascii_uppercase()
                        } else {
                            c
                        },
                    )])
                } else {
                    ReedlineEvent::None
                }
            })
    }

    /// Route one keypress through the mode's grammar and tables.
    fn dispatch_plain(&mut self, modifiers: KeyModifiers, code: KeyCode) -> ReedlineEvent {
        match (self.mode, modifiers, code) {
            // TODO: This guard changes `2v`: the pending count keeps `cache`
            // non-empty, so `v` no longer enters Visual mode. Decide how
            // count-prefixed Visual entry should behave before broadening this
            // special case.
            (ViMode::Normal, KeyModifiers::NONE, KeyCode::Char('v')) if self.cache.is_empty() => {
                self.mode = ViMode::Visual;
                // Entering Visual switches the rest policy to `Block`; the
                // pre-paint commit then widens the cursor into its min-width-1
                // selection. Just repaint — do *not* clear the selection here
                // (e.g. by emitting `Esc`), which would defeat starting one.
                ReedlineEvent::Repaint
            }
            (ViMode::Normal | ViMode::Visual, modifier, KeyCode::Char(c)) => {
                let c = c.to_ascii_lowercase();

                let binding = self.keybindings().find_binding(modifiers, KeyCode::Char(c));
                let is_typeable = is_plain_char(modifier);

                // A pending multi-key motion (e.g. `f<char>`) must be completed
                // before a custom keybinding can claim the next key; otherwise a
                // binding on that second key would hijack the sequence.
                if !self.cache.is_empty() || (binding.is_none() && is_typeable) {
                    self.cache.push(if modifier == KeyModifiers::SHIFT {
                        c.to_ascii_uppercase()
                    } else {
                        c
                    });

                    let res = parse(self.mode, &mut self.cache.iter().peekable());

                    if !res.is_valid() {
                        self.cache.clear();
                        ReedlineEvent::None
                    } else if res.is_complete(self.mode) {
                        let event = res.to_reedline_event(self);
                        let previous_mode = self.mode;
                        let mode_changed = match res.changes_mode(previous_mode) {
                            Some(mode) => {
                                self.mode = mode;
                                mode != previous_mode
                            }
                            None => false,
                        };
                        self.cache.clear();
                        // A mode change must repaint even when the edit is a
                        // no-op (e.g. `a` at line end), mirroring helix's
                        // `Multiple([event, Repaint])` on a mode switch.
                        // `changes_mode` can also report the current mode
                        // (`dw`/`yy` report `Normal` from `Normal`), so only
                        // an actual change gains the trailing `Repaint`.
                        if mode_changed {
                            with_trailing_repaint(event)
                        } else {
                            event
                        }
                    } else {
                        ReedlineEvent::None
                    }
                } else if let Some(event) = binding {
                    event
                } else {
                    ReedlineEvent::None
                }
            }
            (ViMode::Insert, modifier, KeyCode::Char(_)) => self.insert_single_key(modifier, code),
            (_, KeyModifiers::NONE, KeyCode::Esc) => ReedlineEvent::Multiple(self.escape()),
            (ViMode::Normal | ViMode::Visual, _, _) => {
                match self.keybindings().find_binding(modifiers, code) {
                    // A key the sequence has no use for: the binding fires and
                    // abandons what was half typed, as a `SwitchMode` does, so
                    // the next character is not read as a stale argument.
                    Some(event) => {
                        self.cache.clear();
                        event
                    }
                    None => {
                        // Default Enter behavior when no custom binding
                        if modifiers == KeyModifiers::NONE && code == KeyCode::Enter {
                            self.mode = ViMode::Insert;
                            // The normal/visual block caret rests *on* a grapheme;
                            // submitting (or inserting a newline on incomplete input)
                            // acts past it. Release the caret forward like `a`/append
                            // — under the now-`Between` policy — so the trailing edit
                            // (abbreviation expansion or the newline) lands at the line
                            // end, not one grapheme short, which otherwise split the
                            // last word and dropped submit-time abbreviation expansion.
                            ReedlineEvent::Multiple(vec![
                                ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: false }]),
                                ReedlineEvent::Enter,
                            ])
                        } else {
                            ReedlineEvent::None
                        }
                    }
                }
            }
            (ViMode::Insert, _, _) => self
                .keybindings()
                .find_binding(modifiers, code)
                .unwrap_or_else(|| {
                    // Default Enter behavior when no custom binding
                    if modifiers == KeyModifiers::NONE && code == KeyCode::Enter {
                        ReedlineEvent::Enter
                    } else {
                        ReedlineEvent::None
                    }
                }),
        }
    }

    /// Leave for normal mode the way `Esc` does: drop any pending sequence,
    /// let the engine close menus and selections, and when leaving insert step
    /// the caret back onto the last typed grapheme, as vim does.
    fn escape(&mut self) -> Vec<ReedlineEvent> {
        self.cache.clear();
        self.resolver.reset();
        let leaving_insert = self.mode == ViMode::Insert;
        self.mode = ViMode::Normal;
        let mut events = vec![ReedlineEvent::Esc];
        if leaving_insert {
            events.push(ReedlineEvent::Edit(vec![EditCommand::Move(
                MotionTarget::Grapheme(Direction::Backward),
            )]));
        }
        events.push(ReedlineEvent::Repaint);
        events
    }

    /// `Alt-<char>` as `Esc` then `<char>`, the meta convention of readline
    /// and zsh's vi mode. Terminals without the kitty protocol send the same
    /// bytes for both spellings, so this only makes the two agree.
    ///
    /// Returns `None` where the convention does not apply: a binding on the
    /// Alt chord wins (the per-key opt-out), Alt on a non-character key is
    /// left to the tables, and the Ctrl-Alt pairs some terminals report for
    /// AltGr stay typed text via [`is_text_char`].
    fn meta_as_escape(&mut self, modifiers: KeyModifiers, code: KeyCode) -> Option<ReedlineEvent> {
        let KeyCode::Char(c) = code else {
            return None;
        };
        if !is_meta_char(modifiers) {
            return None;
        }

        // Both mode arms look bindings up by the lowercased char, Shift
        // riding in the modifiers; match that so a user's `Alt-Shift-x`
        // binding is found the same way `Shift-x` is. A sequence starting with
        // the Alt chord is an explicit opt-out too, so its first key can open
        // the chord instead of escaping.
        let key = self.chord_combination(modifiers, code);
        if self
            .keybindings()
            .find_binding(key.modifier, key.key_code)
            .is_some()
            || self
                .keybindings()
                .is_sequence_prefix(std::slice::from_ref(&key))
        {
            return None;
        }

        let mut events = self.escape();
        // The stripped key can no longer satisfy `is_meta_char`, so this
        // re-dispatch cannot loop back here.
        match self.dispatch_key(modifiers.difference(KeyModifiers::ALT), code) {
            ReedlineEvent::None => {}
            event => events.push(event),
        }
        Some(ReedlineEvent::Multiple(events))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{Direction, Granularity, MotionTarget, WordEdge, WordKind};
    use crossterm::event::{MouseEvent, MouseEventKind};
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> ReedlineRawEvent {
        ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(code, modifiers))).unwrap()
    }

    fn combo(c: char) -> KeyCombination {
        KeyCombination {
            modifier: KeyModifiers::NONE,
            key_code: KeyCode::Char(c),
        }
    }

    #[test]
    fn esc_leads_to_normal_mode_test() {
        // `Vi::default()` starts in insert, so this also covers the
        // leaving-insert cursor step-back.
        let mut vi = Vi::default();
        let esc =
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
                .unwrap();
        let result = vi.parse_event(esc);

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Esc,
                ReedlineEvent::Edit(vec![EditCommand::Move(MotionTarget::Grapheme(
                    Direction::Backward
                ))]),
                ReedlineEvent::Repaint,
            ])
        );
        assert!(matches!(vi.mode, ViMode::Normal));
    }

    #[test]
    fn esc_from_normal_does_not_step_cursor() {
        // Esc in normal mode only cancels a pending sequence; it must not
        // walk the cursor left like leaving insert does.
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Esc, KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint])
        );
        assert!(matches!(vi.mode, ViMode::Normal));
    }

    /// What `code` with `modifiers` does in a fresh `Vi` sitting in `mode`.
    fn in_mode(mode: ViMode, code: KeyCode, modifiers: KeyModifiers) -> ReedlineEvent {
        let mut vi = Vi {
            mode,
            ..Default::default()
        };
        vi.parse_event(key(code, modifiers))
    }

    /// [`in_mode`] for normal, for comparing against the tail of a
    /// meta-prefixed dispatch.
    fn in_normal(code: KeyCode, modifiers: KeyModifiers) -> ReedlineEvent {
        in_mode(ViMode::Normal, code, modifiers)
    }

    #[test]
    fn alt_char_in_insert_is_esc_then_the_normal_command() {
        // The readline/zsh meta convention: `Alt-j` from insert leaves for
        // normal mode exactly like `Esc` does, then runs `j` there.
        let mut vi = Vi::default();
        let result = vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::ALT));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Esc,
                ReedlineEvent::Edit(vec![EditCommand::Move(MotionTarget::Grapheme(
                    Direction::Backward
                ))]),
                ReedlineEvent::Repaint,
                in_normal(KeyCode::Char('j'), KeyModifiers::NONE),
            ])
        );
        assert!(matches!(vi.mode, ViMode::Normal));
    }

    #[test]
    fn alt_shift_char_in_insert_reaches_the_uppercase_command() {
        // `Alt-I` is `Esc` then `I`: caret to the line start, back in insert.
        let mut vi = Vi::default();
        let result = vi.parse_event(key(
            KeyCode::Char('i'),
            KeyModifiers::ALT | KeyModifiers::SHIFT,
        ));

        let prepend = in_normal(KeyCode::Char('i'), KeyModifiers::SHIFT);
        assert_ne!(prepend, ReedlineEvent::None);
        match result {
            ReedlineEvent::Multiple(events) => assert_eq!(events.last(), Some(&prepend)),
            other => panic!("expected the escape bundle, got {other:?}"),
        }
        assert!(matches!(vi.mode, ViMode::Insert));
    }

    #[test]
    fn alt_char_in_normal_aborts_the_pending_sequence() {
        // `d` then `Alt-w` is `d`, `Esc`, `w`: the operator is dropped and `w`
        // only moves, where `dw` would have cut a word.
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );

        let result = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::ALT));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Esc,
                ReedlineEvent::Repaint,
                in_normal(KeyCode::Char('w'), KeyModifiers::NONE),
            ])
        );
        assert!(vi.cache.is_empty());
    }

    #[test]
    fn alt_char_in_visual_returns_to_normal_first() {
        let mut vi = Vi {
            mode: ViMode::Visual,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::ALT));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Esc,
                ReedlineEvent::Repaint,
                in_normal(KeyCode::Char('j'), KeyModifiers::NONE),
            ])
        );
        assert!(matches!(vi.mode, ViMode::Normal));
    }

    #[test]
    fn altgr_char_in_insert_is_still_typed() {
        // Some terminals report AltGr as Ctrl-Alt. That is text, not a meta
        // chord, so it must not escape to normal mode.
        let mut vi = Vi::default();
        let result = vi.parse_event(key(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('q')])
        );
        assert!(matches!(vi.mode, ViMode::Insert));
    }

    #[test]
    fn binding_on_the_alt_chord_beats_the_escape() {
        // Binding `Alt-j` is also how a user opts out of the convention for
        // that one key.
        let mut keybindings = default_vi_insert_keybindings();
        keybindings.add_binding(
            KeyModifiers::ALT,
            KeyCode::Char('j'),
            ReedlineEvent::ClearScreen,
        );
        let mut vi = Vi {
            insert_keybindings: keybindings,
            ..Default::default()
        };

        let result = vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::ALT));

        assert_eq!(result, ReedlineEvent::ClearScreen);
        assert!(matches!(vi.mode, ViMode::Insert));
    }

    #[test]
    fn the_alt_chord_opt_out_reads_the_visual_table() {
        // Visual owns a table now, so its per-key opt-out lives there rather
        // than in normal's, which is where it would have gone when the two
        // shared one table.
        let mut visual = default_vi_visual_keybindings();
        visual.add_binding(
            KeyModifiers::ALT,
            KeyCode::Char('j'),
            ReedlineEvent::ClearScreen,
        );
        let mut vi = Vi {
            mode: ViMode::Visual,
            ..Vi::new(
                default_vi_insert_keybindings(),
                default_vi_normal_keybindings(),
                visual,
            )
        };

        let result = vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::ALT));

        assert_eq!(result, ReedlineEvent::ClearScreen);
        assert!(
            matches!(vi.mode, ViMode::Visual),
            "the binding wins, so nothing escapes to normal"
        );
    }

    #[test]
    fn alt_on_a_non_character_key_is_left_to_the_tables() {
        let mut vi = Vi::default();
        let result = vi.parse_event(key(KeyCode::Left, KeyModifiers::ALT));

        assert_eq!(result, ReedlineEvent::None);
        assert!(matches!(vi.mode, ViMode::Insert));
    }

    #[test]
    fn keybinding_without_modifier_test() {
        let mut keybindings = default_vi_normal_keybindings();
        keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Char('e'),
            ReedlineEvent::ClearScreen,
        );

        let mut vi = Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: keybindings,
            mode: ViMode::Normal,
            ..Default::default()
        };

        let esc = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('e'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = vi.parse_event(esc);

        assert_eq!(result, ReedlineEvent::ClearScreen);
    }

    #[test]
    fn keybinding_with_shift_modifier_test() {
        let mut keybindings = default_vi_normal_keybindings();
        keybindings.add_binding(
            KeyModifiers::SHIFT,
            KeyCode::Char('$'),
            ReedlineEvent::CtrlD,
        );

        let mut vi = Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: keybindings,
            mode: ViMode::Normal,
            ..Default::default()
        };

        let esc = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('$'),
            KeyModifiers::SHIFT,
        )))
        .unwrap();
        let result = vi.parse_event(esc);

        assert_eq!(result, ReedlineEvent::CtrlD);
    }

    #[test]
    fn pending_motion_beats_custom_keybinding() {
        // A custom binding on `B` must not hijack the second key of an
        // in-progress `f<char>` motion: `fB` should find `B`, not fire the
        // binding. Regression test for nushell/reedline#693.
        let mut keybindings = default_vi_normal_keybindings();
        keybindings.add_binding(
            KeyModifiers::SHIFT,
            KeyCode::Char('b'),
            ReedlineEvent::ClearScreen,
        );

        let mut vi = Vi {
            normal_keybindings: keybindings,
            mode: ViMode::Normal,
            ..Default::default()
        };

        // `f` opens a pending find-motion...
        let pending = vi.parse_event(key(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(pending, ReedlineEvent::None);

        // ...so `B` completes `fB` instead of triggering the custom binding.
        let res = vi.parse_event(key(KeyCode::Char('b'), KeyModifiers::SHIFT));

        assert_eq!(
            res,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::Move(
                MotionTarget::Find {
                    ch: 'B',
                    direction: Direction::Forward,
                    stop: crate::FindStop::On,
                }
            )])])
        );
    }

    #[test]
    fn binding_fires_right_after_aborted_find() {
        // A custom binding on `B` must not hijack the second key of an
        // in-progress `f<char>` motion: `fB` should find `B`, not fire the
        // binding. Regression test for nushell/reedline#693.
        let mut keybindings = default_vi_normal_keybindings();
        keybindings.add_binding(
            KeyModifiers::SHIFT,
            KeyCode::Char('z'),
            ReedlineEvent::ClearScreen,
        );

        let mut vi = Vi {
            normal_keybindings: keybindings,
            mode: ViMode::Normal,
            ..Default::default()
        };

        // `f` opens a pending find-motion
        let pending = vi.parse_event(key(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(pending, ReedlineEvent::None);

        // `ESC` aborts the pending find-motion
        vi.parse_event(key(KeyCode::Esc, KeyModifiers::NONE));

        let res = vi.parse_event(key(KeyCode::Char('z'), KeyModifiers::SHIFT));

        assert_eq!(res, ReedlineEvent::ClearScreen);
    }

    #[test]
    fn keybinding_with_super_modifier_test() {
        let mut keybindings = default_vi_normal_keybindings();
        keybindings.add_binding(
            KeyModifiers::SUPER,
            KeyCode::Char('$'),
            ReedlineEvent::CtrlD,
        );

        let mut vi = Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: keybindings,
            mode: ViMode::Normal,
            ..Default::default()
        };

        let esc = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('$'),
            KeyModifiers::SUPER,
        )))
        .unwrap();
        let result = vi.parse_event(esc);

        assert_eq!(result, ReedlineEvent::CtrlD);
    }

    #[test]
    fn non_register_modifier_test() {
        let keybindings = default_vi_normal_keybindings();
        let mut vi = Vi {
            insert_keybindings: default_vi_insert_keybindings(),
            normal_keybindings: keybindings,
            mode: ViMode::Normal,
            ..Default::default()
        };

        let esc = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = vi.parse_event(esc);

        assert_eq!(result, ReedlineEvent::None);
    }

    #[test]
    fn v_completes_pending_replace_char() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };

        let pending = vi.parse_event(key(KeyCode::Char('r'), KeyModifiers::NONE));
        assert_eq!(pending, ReedlineEvent::None);

        let result = vi.parse_event(key(KeyCode::Char('v'), KeyModifiers::NONE));

        assert!(matches!(vi.mode, ViMode::Normal));
        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::ReplaceChar(
                'v'
            )])])
        );
    }

    #[test]
    fn v_completes_pending_find_motion() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };

        let pending = vi.parse_event(key(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(pending, ReedlineEvent::None);

        let result = vi.parse_event(key(KeyCode::Char('v'), KeyModifiers::NONE));

        assert!(matches!(vi.mode, ViMode::Normal));
        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::Move(
                MotionTarget::Find {
                    ch: 'v',
                    direction: Direction::Forward,
                    stop: crate::FindStop::On,
                }
            )])])
        );
    }

    #[test]
    fn v_completes_pending_till_motion() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };

        let pending = vi.parse_event(key(KeyCode::Char('t'), KeyModifiers::NONE));
        assert_eq!(pending, ReedlineEvent::None);

        let result = vi.parse_event(key(KeyCode::Char('v'), KeyModifiers::NONE));

        assert!(matches!(vi.mode, ViMode::Normal));
        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::Move(
                MotionTarget::Find {
                    ch: 'v',
                    direction: Direction::Forward,
                    stop: crate::FindStop::Before,
                }
            )])])
        );
    }

    #[test]
    fn v_in_normal_enters_visual() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('v'), KeyModifiers::NONE));

        assert!(matches!(vi.mode, ViMode::Visual));
        // `v` only enters Visual + repaints; it must NOT emit `Esc` (which would
        // clear the selection). The `Block` rest policy materializes the block.
        assert_eq!(result, ReedlineEvent::Repaint);
    }

    #[test]
    fn esc_from_visual_returns_to_normal() {
        let mut vi = Vi {
            mode: ViMode::Visual,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(vi.mode, ViMode::Normal));
    }

    #[test]
    fn esc_clears_cache() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(
            !vi.cache.is_empty(),
            "cache should hold the partial sequence"
        );

        let _ = vi.parse_event(key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(vi.cache.is_empty(), "Esc should clear the cache");
    }

    #[test]
    fn unbound_char_in_normal_feeds_parser() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };

        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        let result = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::Cut {
                target: MotionTarget::Word {
                    kind: WordKind::Word,
                    edge: WordEdge::Start,
                    direction: Direction::Forward,
                },
                granularity: Granularity::CharWise
            }])]),
        );
        assert!(
            vi.cache.is_empty(),
            "cache should be cleared after a complete sequence"
        );
    }

    #[test]
    fn incomplete_sequence_returns_none_and_holds_cache() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));

        assert_eq!(result, ReedlineEvent::None);
        assert_eq!(vi.cache, vec!['d']);
    }

    #[test]
    fn shift_char_pushed_uppercase_into_cache() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::SHIFT));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::Move(
                MotionTarget::Word {
                    kind: WordKind::LongWord,
                    edge: WordEdge::Start,
                    direction: Direction::Forward,
                }
            )])]),
        );
    }

    #[test]
    fn d_in_visual_emits_cut_selection_and_returns_to_normal() {
        let mut vi = Vi {
            mode: ViMode::Visual,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::CutSelection {
                    granularity: Granularity::CharWise
                }]),
                // Leaving Visual for Normal is an actual mode change.
                ReedlineEvent::Repaint,
            ]),
        );
        assert!(matches!(vi.mode, ViMode::Normal));
    }

    #[test]
    fn non_char_key_in_normal_uses_keybindings() {
        let mut kb = default_vi_normal_keybindings();
        kb.add_binding(KeyModifiers::NONE, KeyCode::Up, ReedlineEvent::Up);

        let mut vi = Vi {
            normal_keybindings: kb,
            mode: ViMode::Normal,
            ..Default::default()
        };

        let result = vi.parse_event(key(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(result, ReedlineEvent::Up);
    }

    #[test]
    fn enter_in_normal_with_no_binding_submits_and_enters_insert() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Enter, KeyModifiers::NONE));

        // Releases the block caret forward (like `a`) before submitting, so a
        // trailing abbreviation / newline acts past the resting grapheme.
        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: false }]),
                ReedlineEvent::Enter,
            ])
        );
        assert!(matches!(vi.mode, ViMode::Insert));
    }

    #[test]
    fn unbound_char_in_insert_inserts_char() {
        let mut vi = Vi {
            mode: ViMode::Insert,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('x'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('x')]),
        );
    }

    #[test]
    fn shift_char_in_insert_inserts_uppercase() {
        let mut vi = Vi {
            mode: ViMode::Insert,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('a'), KeyModifiers::SHIFT));

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('A')]),
        );
    }

    #[test]
    fn ctrl_char_in_insert_with_no_binding_returns_none() {
        let mut vi = Vi {
            mode: ViMode::Insert,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('z'), KeyModifiers::CONTROL));

        assert_eq!(result, ReedlineEvent::None);
    }

    #[test]
    fn i_in_normal_transitions_to_insert() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('i'), KeyModifiers::NONE));
        assert!(matches!(vi.mode, ViMode::Insert));
    }

    /// Entering Insert from Normal repaints at the dispatch layer, even
    /// though the parser/command layer now emits only the cursor move.
    #[rstest]
    #[case::append('a', KeyModifiers::NONE, EditCommand::MoveRight { select: false })]
    #[case::append_to_end('a', KeyModifiers::SHIFT, EditCommand::MoveToLineEnd { select: false })]
    #[case::prepend_to_start('i', KeyModifiers::SHIFT, EditCommand::MoveToLineStart { select: false })]
    fn entering_insert_repaints_after_the_cursor_move(
        #[case] c: char,
        #[case] modifiers: KeyModifiers,
        #[case] edit: EditCommand,
    ) {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char(c), modifiers));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![edit]),
                ReedlineEvent::Repaint,
            ])
        );
        assert!(matches!(vi.mode, ViMode::Insert));
    }

    #[test]
    fn i_in_normal_repaints_without_duplicating_its_bare_repaint() {
        // `i` already yields `Multiple([Repaint])` from the command layer;
        // the dispatch-layer wrap must not add a second one.
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('i'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Repaint])
        );
        assert!(matches!(vi.mode, ViMode::Insert));
    }

    #[test]
    fn cw_repaints_without_duplicating_the_change_semihack_repaint() {
        // `c<motion>` already ends with `Repaint` (the semihack in
        // `command.rs`); entering Insert must not add another.
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('c'), KeyModifiers::NONE));
        let result = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::Cut {
                    target: MotionTarget::Word {
                        kind: WordKind::Word,
                        edge: WordEdge::End,
                        direction: Direction::Forward,
                    },
                    granularity: Granularity::CharWise
                }]),
                ReedlineEvent::Repaint,
            ])
        );
        assert!(matches!(vi.mode, ViMode::Insert));
    }

    #[test]
    fn commands_staying_in_normal_do_not_gain_a_repaint() {
        // `dw`/`yy` report `Some(Normal)` from `changes_mode` while already
        // in Normal, and a plain move reports no mode at all; none of them
        // may gain a spurious trailing `Repaint`.
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        let dw = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::NONE));
        assert_eq!(
            dw,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::Cut {
                target: MotionTarget::Word {
                    kind: WordKind::Word,
                    edge: WordEdge::Start,
                    direction: Direction::Forward,
                },
                granularity: Granularity::CharWise
            }])]),
        );
        assert!(matches!(vi.mode, ViMode::Normal));

        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('y'), KeyModifiers::NONE));
        let yy = vi.parse_event(key(KeyCode::Char('y'), KeyModifiers::NONE));
        assert_eq!(
            yy,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::Copy {
                target: MotionTarget::LineEdge(Direction::Forward),
                granularity: Granularity::LineWise
            }])]),
        );
        assert!(matches!(vi.mode, ViMode::Normal));

        assert_eq!(
            in_normal(KeyCode::Char('w'), KeyModifiers::NONE),
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::Move(
                MotionTarget::Word {
                    kind: WordKind::Word,
                    edge: WordEdge::Start,
                    direction: Direction::Forward,
                }
            )])]),
        );
    }

    #[test]
    fn previous_set_after_complete_command() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        let _ = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::NONE));
        assert!(
            vi.previous.is_some(),
            "previous should track the last complete command"
        );
    }

    #[test]
    fn paste_event_produces_insert_string() {
        let mut vi = Vi::default();
        let paste = ReedlineRawEvent::try_from(Event::Paste("hello".to_string())).unwrap();
        let result = vi.parse_event(paste);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::InsertString("hello".to_string())]),
        );
    }

    #[test]
    fn resize_event_passes_through() {
        let mut vi = Vi::default();
        let resize = ReedlineRawEvent::try_from(Event::Resize(80, 24)).unwrap();
        let result = vi.parse_event(resize);
        assert_eq!(result, ReedlineEvent::Resize(80, 24));
    }

    #[test]
    fn focus_gained_returns_none() {
        let mut vi = Vi::default();
        let ev = ReedlineRawEvent::try_from(Event::FocusGained).unwrap();
        assert_eq!(vi.parse_event(ev), ReedlineEvent::None);
    }

    #[test]
    fn mouse_down_event_produces_mouse_event() {
        let mut vi = Vi::default();
        let ev = ReedlineRawEvent::try_from(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 5,
            row: 10,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();

        assert_eq!(
            vi.parse_event(ev),
            ReedlineEvent::Mouse {
                column: 5,
                row: 10,
                button: crate::enums::MouseButton::Left,
            },
        );
    }

    #[test]
    fn multiplier_repeats_operator_motion() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('2'), KeyModifiers::NONE));
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        let result = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::NONE));

        let cut_word = ReedlineEvent::Edit(vec![EditCommand::Cut {
            target: MotionTarget::Word {
                kind: WordKind::Word,
                edge: WordEdge::Start,
                direction: Direction::Forward,
            },
            granularity: Granularity::CharWise,
        }]);
        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![cut_word.clone(), cut_word]),
        );
        assert!(vi.cache.is_empty());
    }

    #[test]
    fn multiplier_alone_repeats_motion() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('3'), KeyModifiers::NONE));
        let result = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::NONE));

        let mv = ReedlineEvent::Edit(vec![EditCommand::Move(MotionTarget::Word {
            kind: WordKind::Word,
            edge: WordEdge::Start,
            direction: Direction::Forward,
        })]);
        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![mv.clone(), mv.clone(), mv]),
        );
        assert!(vi.cache.is_empty());
    }

    #[test]
    fn partial_multiplier_holds_cache() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let result = vi.parse_event(key(KeyCode::Char('2'), KeyModifiers::NONE));

        assert_eq!(result, ReedlineEvent::None);
        assert_eq!(vi.cache, vec!['2']);
    }

    #[test]
    fn invalid_motion_after_operator_clears_cache() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        assert_eq!(vi.cache, vec!['d']);

        let result = vi.parse_event(key(KeyCode::Char('z'), KeyModifiers::NONE));

        assert_eq!(result, ReedlineEvent::None);
        assert!(
            vi.cache.is_empty(),
            "an invalid motion should drop the cached operator",
        );
    }

    #[test]
    fn linewise_dd_emits_linewise_cut() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        let result = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));

        assert_eq!(
            result,
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::Cut {
                target: MotionTarget::LineEdge(Direction::Forward),
                granularity: Granularity::LineWise,
            }])]),
        );
        assert!(vi.cache.is_empty());
    }

    #[test]
    fn repeated_dot_doesnt_accumulates_nesting_in_previous() {
        // `.` replays the last change; it must not record itself
        fn depth(ev: &ReedlineEvent) -> usize {
            match ev {
                ReedlineEvent::Multiple(v) if v.len() == 1 => 1 + depth(&v[0]),
                _ => 0,
            }
        }
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        let _ = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::NONE));
        assert_eq!(depth(vi.previous.as_ref().unwrap()), 1);

        let _ = vi.parse_event(key(KeyCode::Char('.'), KeyModifiers::NONE));
        assert_eq!(depth(vi.previous.as_ref().unwrap()), 1);

        let _ = vi.parse_event(key(KeyCode::Char('.'), KeyModifiers::NONE));
        assert_eq!(depth(vi.previous.as_ref().unwrap()), 1);

        let _ = vi.parse_event(key(KeyCode::Char('.'), KeyModifiers::NONE));
        assert_eq!(depth(vi.previous.as_ref().unwrap()), 1);
    }

    #[test]
    fn dot_replays_previous_wrapped_in_outer_multiple() {
        // `.` produces Multiple([previous]) and writes it back to
        // `previous`. See `repeated_dot_accumulates_nesting_in_previous`
        // for the consequences — this assertion just pins the shape.
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        let _ = vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE));
        let dw = vi.parse_event(key(KeyCode::Char('w'), KeyModifiers::NONE));
        assert!(vi.previous.is_some());

        let dot = vi.parse_event(key(KeyCode::Char('.'), KeyModifiers::NONE));
        assert_eq!(dot, ReedlineEvent::Multiple(vec![dw]));
    }

    // --- SwitchMode ---

    #[rstest]
    #[case(PromptViMode::Insert, ViMode::Insert)]
    #[case(PromptViMode::Normal, ViMode::Normal)]
    #[case(PromptViMode::Visual, ViMode::Visual)]
    fn switch_mode_event_switches_the_machine(
        #[case] target: PromptViMode,
        #[case] expected: ViMode,
    ) {
        let mut vi = Vi::default();
        let status =
            vi.handle_mode_specific_event(ReedlineEvent::SwitchMode(PromptEditMode::Vi(target)));
        assert!(matches!(status, EventStatus::Handled));
        assert_eq!(vi.mode, expected);
    }

    #[test]
    fn switch_mode_event_declines_another_machine() {
        let mut vi = Vi::default();
        let status = vi.handle_mode_specific_event(ReedlineEvent::SwitchMode(
            PromptEditMode::Helix(crate::PromptHelixMode::Normal),
        ));
        assert!(matches!(status, EventStatus::Inapplicable));
        assert_eq!(vi.mode, ViMode::Insert);
    }

    #[test]
    fn switch_mode_event_abandons_a_half_typed_sequence() {
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Default::default()
        };
        // Arm a counted find; the switch must clear the cache so the next key
        // is not parsed as the find argument in the new mode.
        let _ = vi.parse_event(key(KeyCode::Char('3'), KeyModifiers::NONE));
        let _ = vi.parse_event(key(KeyCode::Char('f'), KeyModifiers::NONE));
        assert!(!vi.cache.is_empty(), "setup: sequence is armed");

        vi.handle_mode_specific_event(ReedlineEvent::SwitchMode(PromptEditMode::Vi(
            PromptViMode::Insert,
        )));
        assert!(vi.cache.is_empty());
        assert_eq!(vi.mode, ViMode::Insert);
    }

    /// A key the sequence has no use for still reaches the table, and the
    /// binding found there abandons the half-typed sequence, as in helix: the
    /// character after it starts fresh instead of serving as `f`'s argument.
    #[test]
    fn a_bound_key_fires_during_a_half_typed_sequence_and_abandons_it() {
        let mut normal = crate::default_vi_normal_keybindings();
        normal.add_binding(
            KeyModifiers::NONE,
            KeyCode::F(5),
            ReedlineEvent::ClearScreen,
        );
        let mut vi = Vi::new(
            crate::default_vi_insert_keybindings(),
            normal,
            crate::default_vi_visual_keybindings(),
        );
        vi.mode = ViMode::Normal;
        let _ = vi.parse_event(key(KeyCode::Char('f'), KeyModifiers::NONE));
        assert!(!vi.cache.is_empty(), "setup: sequence is armed");

        assert_eq!(
            vi.parse_event(key(KeyCode::F(5), KeyModifiers::NONE)),
            ReedlineEvent::ClearScreen
        );
        assert!(vi.cache.is_empty());
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('x'), KeyModifiers::NONE)),
            in_normal(KeyCode::Char('x'), KeyModifiers::NONE)
        );
    }

    // --- visual keybinding table ---

    /// Visual has its own table: a binding placed there fires after `v`, and a
    /// binding placed only in the normal table does not.
    #[test]
    fn visual_mode_reads_its_own_keybinding_table() {
        let mut normal = default_vi_normal_keybindings();
        normal.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('t'),
            ReedlineEvent::ClearScrollback,
        );
        let mut visual = default_vi_visual_keybindings();
        visual.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('t'),
            ReedlineEvent::ClearScreen,
        );
        let mut vi = Vi {
            mode: ViMode::Normal,
            ..Vi::new(default_vi_insert_keybindings(), normal, visual)
        };

        assert_eq!(
            vi.parse_event(key(KeyCode::Char('t'), KeyModifiers::CONTROL)),
            ReedlineEvent::ClearScrollback
        );
        let _ = vi.parse_event(key(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(vi.mode, ViMode::Visual, "setup: `v` enters visual");
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('t'), KeyModifiers::CONTROL)),
            ReedlineEvent::ClearScreen
        );
    }

    fn in_visual(code: KeyCode, modifiers: KeyModifiers) -> ReedlineEvent {
        in_mode(ViMode::Visual, code, modifiers)
    }

    fn word_start(direction: Direction) -> MotionTarget {
        MotionTarget::Word {
            kind: WordKind::Word,
            edge: WordEdge::Start,
            direction,
        }
    }

    /// A navigation key in visual extends like the modal key it stands for,
    /// where the normal table's would collapse the selection and restart it.
    #[rstest]
    #[case::left_is_h(KeyCode::Left, KeyModifiers::NONE, EditCommand::MoveLeft { select: true })]
    #[case::right_is_l(KeyCode::Right, KeyModifiers::NONE, EditCommand::MoveRight { select: true })]
    #[case::backspace_is_h(KeyCode::Backspace, KeyModifiers::NONE, EditCommand::MoveLeft { select: true })]
    #[case::up_is_k(KeyCode::Up, KeyModifiers::NONE, EditCommand::MoveLineUp { select: true })]
    #[case::down_is_j(KeyCode::Down, KeyModifiers::NONE, EditCommand::MoveLineDown { select: true })]
    #[case::ctrl_p_is_k(KeyCode::Char('p'), KeyModifiers::CONTROL, EditCommand::MoveLineUp { select: true })]
    #[case::ctrl_n_is_j(KeyCode::Char('n'), KeyModifiers::CONTROL, EditCommand::MoveLineDown { select: true })]
    #[case::ctrl_left_is_b(
        KeyCode::Left,
        KeyModifiers::CONTROL,
        EditCommand::Extend(word_start(Direction::Backward))
    )]
    #[case::ctrl_right_is_w(
        KeyCode::Right,
        KeyModifiers::CONTROL,
        EditCommand::Extend(word_start(Direction::Forward))
    )]
    #[case::home_is_0(
        KeyCode::Home,
        KeyModifiers::NONE,
        EditCommand::Extend(MotionTarget::LineEdge(Direction::Backward))
    )]
    #[case::end_is_dollar(
        KeyCode::End,
        KeyModifiers::NONE,
        EditCommand::Extend(MotionTarget::LineEdge(Direction::Forward))
    )]
    #[case::ctrl_a_is_0(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL,
        EditCommand::Extend(MotionTarget::LineEdge(Direction::Backward))
    )]
    #[case::ctrl_e_is_dollar(
        KeyCode::Char('e'),
        KeyModifiers::CONTROL,
        EditCommand::Extend(MotionTarget::LineEdge(Direction::Forward))
    )]
    #[case::ctrl_home_is_gg(
        KeyCode::Home,
        KeyModifiers::CONTROL,
        EditCommand::Extend(MotionTarget::BufferEdge(Direction::Backward))
    )]
    #[case::ctrl_end_is_g(
        KeyCode::End,
        KeyModifiers::CONTROL,
        EditCommand::Extend(MotionTarget::BufferEdge(Direction::Forward))
    )]
    fn visual_navigation_keys_extend_the_selection(
        #[case] code: KeyCode,
        #[case] modifiers: KeyModifiers,
        #[case] expected: EditCommand,
    ) {
        assert_eq!(
            in_visual(code, modifiers),
            ReedlineEvent::Edit(vec![expected])
        );
    }

    /// `Delete` is `d`: it takes the selection and returns to normal, which a
    /// table binding can only do through `SwitchMode`.
    #[test]
    fn visual_delete_cuts_the_selection_and_leaves_visual() {
        assert_eq!(
            in_visual(KeyCode::Delete, KeyModifiers::NONE),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::CutSelection {
                    granularity: Granularity::CharWise
                }]),
                ReedlineEvent::SwitchMode(PromptEditMode::Vi(PromptViMode::Normal)),
            ])
        );
    }

    /// The visual table must not leak into normal, where an open menu takes
    /// the arrows first and `Right` accepts a history hint.
    #[test]
    fn normal_mode_arrows_keep_menu_navigation() {
        assert_eq!(
            in_normal(KeyCode::Left, KeyModifiers::NONE),
            ReedlineEvent::UntilFound(vec![ReedlineEvent::MenuLeft, ReedlineEvent::Left])
        );
    }

    // --- explicit multi-key sequences (chords) ---

    fn vi_with(insert: Keybindings, normal: Keybindings, visual: Keybindings, mode: ViMode) -> Vi {
        Vi {
            mode,
            ..Vi::new(insert, normal, visual)
        }
    }

    #[test]
    fn a_chord_completes_in_insert_mode() {
        let mut insert = default_vi_insert_keybindings();
        insert.add_binding_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);
        let mut vi = vi_with(
            insert,
            default_vi_normal_keybindings(),
            default_vi_visual_keybindings(),
            ViMode::Insert,
        );

        assert_eq!(
            vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert!(vi.has_pending_sequence());
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::Esc
        );
        assert!(!vi.has_pending_sequence());
    }

    #[test]
    fn an_insert_strict_prefix_waits_for_more_keys() {
        let mut insert = default_vi_insert_keybindings();
        insert.add_binding_sequence(
            &[combo('a'), combo('b'), combo('c')],
            ReedlineEvent::ClearScreen,
        );
        let mut vi = vi_with(
            insert,
            default_vi_normal_keybindings(),
            default_vi_visual_keybindings(),
            ViMode::Insert,
        );

        assert_eq!(
            vi.parse_event(key(KeyCode::Char('a'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('b'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert!(vi.has_pending_sequence());
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('c'), KeyModifiers::NONE)),
            ReedlineEvent::ClearScreen
        );
    }

    #[test]
    fn an_alt_chord_can_start_a_sequence() {
        let alt_j = KeyCombination {
            modifier: KeyModifiers::ALT,
            key_code: KeyCode::Char('j'),
        };
        let mut insert = default_vi_insert_keybindings();
        insert.add_binding_sequence(&[alt_j.clone(), alt_j], ReedlineEvent::Esc);
        let mut vi = vi_with(
            insert,
            default_vi_normal_keybindings(),
            default_vi_visual_keybindings(),
            ViMode::Insert,
        );

        // An Alt key with no binding is normally `Esc` + key; an explicit
        // sequence starting there opts out of that.
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::ALT)),
            ReedlineEvent::None
        );
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::ALT)),
            ReedlineEvent::Esc
        );
    }

    #[test]
    fn an_insert_chord_timeout_self_inserts_the_prefix() {
        let mut insert = default_vi_insert_keybindings();
        insert.add_binding_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);
        let mut vi = vi_with(
            insert,
            default_vi_normal_keybindings(),
            default_vi_visual_keybindings(),
            ViMode::Insert,
        );

        assert_eq!(
            vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(
            vi.flush_pending_sequence(),
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('j')])
        );
        assert!(!vi.has_pending_sequence());
    }

    #[test]
    fn an_insert_chord_mismatch_flushes_then_self_inserts_the_new_key() {
        let mut insert = default_vi_insert_keybindings();
        insert.add_binding_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);
        let mut vi = vi_with(
            insert,
            default_vi_normal_keybindings(),
            default_vi_visual_keybindings(),
            ViMode::Insert,
        );

        assert_eq!(
            vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('x'), KeyModifiers::NONE)),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::InsertChar('j')]),
                ReedlineEvent::Edit(vec![EditCommand::InsertChar('x')]),
            ])
        );
    }

    #[test]
    fn a_normal_key_that_is_both_binding_and_prefix_waits() {
        let mut normal = default_vi_normal_keybindings();
        normal.add_binding(KeyModifiers::NONE, KeyCode::Char('j'), ReedlineEvent::Down);
        normal.add_binding_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);
        let mut vi = vi_with(
            default_vi_insert_keybindings(),
            normal,
            default_vi_visual_keybindings(),
            ViMode::Normal,
        );

        // The binding is a prefix, so the first `j` must not fire Down yet.
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::Esc
        );

        // On its own, the shorter binding is the timeout fallback.
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(vi.flush_pending_sequence(), ReedlineEvent::Down);
    }

    #[test]
    fn a_normal_chord_mismatch_flushes_the_binding_first() {
        let mut normal = default_vi_normal_keybindings();
        normal.add_binding(KeyModifiers::NONE, KeyCode::Char('j'), ReedlineEvent::Down);
        normal.add_binding_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);
        let mut vi = vi_with(
            default_vi_insert_keybindings(),
            normal,
            default_vi_visual_keybindings(),
            ViMode::Normal,
        );

        assert_eq!(
            vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('l'), KeyModifiers::CONTROL)),
            ReedlineEvent::Multiple(vec![ReedlineEvent::Down, ReedlineEvent::ClearScreen])
        );
    }

    /// Vi's own `dd` wins over an explicit `d d` chord: `d` is unbound, so the
    /// grammar claims both keys and the chord never starts.
    #[test]
    fn vi_grammar_dd_wins_over_a_chord() {
        let mut normal = default_vi_normal_keybindings();
        normal.add_binding_sequence(&[combo('d'), combo('d')], ReedlineEvent::ClearScreen);
        let mut vi = vi_with(
            default_vi_insert_keybindings(),
            normal,
            default_vi_visual_keybindings(),
            ViMode::Normal,
        );

        assert_eq!(
            vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('d'), KeyModifiers::NONE)),
            ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::Cut {
                target: MotionTarget::LineEdge(Direction::Forward),
                granularity: Granularity::LineWise,
            }])])
        );
    }

    /// A live count/operator sequence keeps priority: `3f` takes its argument
    /// even when `f f` is a chord.
    #[test]
    fn vi_grammar_counted_find_wins_over_a_chord() {
        let mut normal = default_vi_normal_keybindings();
        normal.add_binding_sequence(&[combo('f'), combo('f')], ReedlineEvent::ClearScreen);
        let mut vi = vi_with(
            default_vi_insert_keybindings(),
            normal,
            default_vi_visual_keybindings(),
            ViMode::Normal,
        );

        assert_eq!(
            vi.parse_event(key(KeyCode::Char('3'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('f'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(
            vi.parse_event(key(KeyCode::Char('x'), KeyModifiers::NONE)),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::Move(MotionTarget::Find {
                    ch: 'x',
                    direction: Direction::Forward,
                    stop: crate::FindStop::On,
                })]),
                ReedlineEvent::Edit(vec![EditCommand::Move(MotionTarget::Find {
                    ch: 'x',
                    direction: Direction::Forward,
                    stop: crate::FindStop::On,
                })]),
                ReedlineEvent::Edit(vec![EditCommand::Move(MotionTarget::Find {
                    ch: 'x',
                    direction: Direction::Forward,
                    stop: crate::FindStop::On,
                })]),
            ])
        );
    }

    #[test]
    fn esc_cancels_a_pending_chord_without_flushing_it() {
        let mut normal = default_vi_normal_keybindings();
        normal.add_binding(KeyModifiers::NONE, KeyCode::Char('j'), ReedlineEvent::Down);
        normal.add_binding_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);
        let mut vi = vi_with(
            default_vi_insert_keybindings(),
            normal,
            default_vi_visual_keybindings(),
            ViMode::Normal,
        );

        assert_eq!(
            vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(
            vi.parse_event(key(KeyCode::Esc, KeyModifiers::NONE)),
            ReedlineEvent::Multiple(vec![ReedlineEvent::Esc, ReedlineEvent::Repaint])
        );
        assert!(!vi.has_pending_sequence());
    }

    #[test]
    fn switching_mode_abandons_a_pending_chord() {
        let mut normal = default_vi_normal_keybindings();
        normal.add_binding(KeyModifiers::NONE, KeyCode::Char('j'), ReedlineEvent::Down);
        normal.add_binding_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);
        let mut vi = vi_with(
            default_vi_insert_keybindings(),
            normal,
            default_vi_visual_keybindings(),
            ViMode::Normal,
        );

        assert_eq!(
            vi.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert!(vi.has_pending_sequence());

        vi.handle_mode_specific_event(ReedlineEvent::SwitchMode(PromptEditMode::Vi(
            PromptViMode::Insert,
        )));
        assert!(!vi.has_pending_sequence());
    }
}
