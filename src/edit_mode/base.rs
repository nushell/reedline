use crate::{
    enums::{EventStatus, ReedlineEvent, ReedlineRawEvent},
    EditCommand, PromptEditMode,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use super::{keybindings, KeyCombination};

/// Define the style of parsing for the edit events
/// Available default options:
/// - Emacs
/// - Vi
pub trait EditMode: Send {
    /// Translate the given user input event into what the `LineEditor` understands
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        match event.into() {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => self.parse_key_event(modifiers, code),
            other => self.parse_non_key_event(other),
        }
    }

    /// Translate key events into what the `LineEditor` understands
    fn parse_key_event(&mut self, modifiers: KeyModifiers, code: KeyCode) -> ReedlineEvent;

    /// Resolve a key combination using keybindings with a fallback to insertable characters.
    fn default_key_event(
        &self,
        keybindings: &keybindings::Keybindings,
        combo: KeyCombination,
    ) -> ReedlineEvent {
        match combo.key_code {
            KeyCode::Char(c) => keybindings
                .find_binding(combo.modifier, KeyCode::Char(c))
                .unwrap_or_else(|| {
                    if combo.modifier == KeyModifiers::NONE
                        || combo.modifier == KeyModifiers::SHIFT
                        || combo.modifier == KeyModifiers::CONTROL | KeyModifiers::ALT
                        || combo.modifier
                            == KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
                    {
                        ReedlineEvent::Edit(vec![EditCommand::InsertChar(
                            if combo.modifier == KeyModifiers::SHIFT {
                                c.to_ascii_uppercase()
                            } else {
                                c
                            },
                        )])
                    } else {
                        ReedlineEvent::None
                    }
                }),
            code => keybindings
                .find_binding(combo.modifier, code)
                .unwrap_or_else(|| {
                    if combo.modifier == KeyModifiers::NONE && code == KeyCode::Enter {
                        ReedlineEvent::Enter
                    } else {
                        ReedlineEvent::None
                    }
                }),
        }
    }

    /// What to display in the prompt indicator
    fn edit_mode(&self) -> PromptEditMode;

    /// Handles events that apply only to specific edit modes (e.g changing vi mode)
    fn handle_mode_specific_event(&mut self, _event: ReedlineEvent) -> EventStatus {
        EventStatus::Inapplicable
    }

    /// Translate non-key events into what the `LineEditor` understands
    fn parse_non_key_event(&mut self, event: Event) -> ReedlineEvent {
        match event {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => self.parse_key_event(modifiers, code),
            Event::Mouse(_) => self.with_flushed_sequence(ReedlineEvent::Mouse),
            Event::Resize(width, height) => {
                self.with_flushed_sequence(ReedlineEvent::Resize(width, height))
            }
            Event::FocusGained => self.with_flushed_sequence(ReedlineEvent::None),
            Event::FocusLost => self.with_flushed_sequence(ReedlineEvent::None),
            Event::Paste(body) => {
                self.with_flushed_sequence(ReedlineEvent::Edit(vec![EditCommand::InsertString(
                    body.replace("\r\n", "\n").replace('\r', "\n"),
                )]))
            }
        }
    }

    /// Flush pending sequences and combine them with an incoming event.
    fn with_flushed_sequence(&mut self, event: ReedlineEvent) -> ReedlineEvent {
        let Some(flush_event) = self.flush_pending_sequence() else {
            return event;
        };

        if matches!(event, ReedlineEvent::None) {
            return flush_event;
        }

        match flush_event {
            ReedlineEvent::Multiple(mut events) => {
                events.push(event);
                ReedlineEvent::Multiple(events)
            }
            other => ReedlineEvent::Multiple(vec![other, event]),
        }
    }

    /// Whether a key sequence is currently pending
    fn has_pending_sequence(&self) -> bool {
        false
    }

    /// Flush any pending key sequence and return the resulting event
    fn flush_pending_sequence(&mut self) -> Option<ReedlineEvent> {
        None
    }
}
