use crate::enums::{ReedlineEvent, ReedlineRawEvent};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use modalkit::keybindings::BindingMachine;

use super::{action::HelixAction, mode::HelixMachine};

pub(super) fn parse_event(machine: &mut HelixMachine, event: ReedlineRawEvent) -> ReedlineEvent {
    let Some(key_event) = KeyEvent::try_from(event).ok() else {
        return ReedlineEvent::None;
    };

    handle_key_event(machine, key_event)
}

fn is_interrupt_event(key_event: &KeyEvent) -> bool {
    matches!(
        key_event,
        KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }
    )
}

fn handle_key_event(machine: &mut HelixMachine, key_event: KeyEvent) -> ReedlineEvent {
    if is_interrupt_event(&key_event) {
        return ReedlineEvent::CtrlC;
    }

    let (action, mode_changed) = apply_key_event(machine, key_event);

    action
        .and_then(HelixAction::into_reedline_event)
        .unwrap_or_else(|| mode_change_event(mode_changed))
}

fn apply_key_event(machine: &mut HelixMachine, key_event: KeyEvent) -> (Option<HelixAction>, bool) {
    let previous_mode = machine.mode();
    machine.input_key(key_event.into());

    let mode_changed = machine.mode() != previous_mode;
    let action = machine.pop().map(|(action, _ctx)| action);

    (action, mode_changed)
}

fn mode_change_event(mode_changed: bool) -> ReedlineEvent {
    if mode_changed {
        ReedlineEvent::Repaint
    } else {
        ReedlineEvent::None
    }
}
