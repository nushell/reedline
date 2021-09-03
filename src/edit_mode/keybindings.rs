use crate::enums::ReedlineEvent;

use {
    crate::EditCommand,
    crossterm::event::{KeyCode, KeyModifiers},
    serde::{Deserialize, Serialize},
};

#[derive(Serialize, Deserialize, Clone)]
pub struct Keybinding {
    modifier: KeyModifiers,
    key_code: KeyCode,
    command: ReedlineEvent,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Keybindings {
    pub bindings: Vec<Keybinding>,
}

impl Default for Keybindings {
    fn default() -> Self {
        Self::new()
    }
}

impl Keybindings {
    pub fn new() -> Self {
        Self { bindings: vec![] }
    }

    pub fn add_binding(
        &mut self,
        modifier: KeyModifiers,
        key_code: KeyCode,
        command: ReedlineEvent,
    ) {
        self.bindings.push(Keybinding {
            modifier,
            key_code,
            command,
        });
    }

    pub fn find_binding(&self, modifier: KeyModifiers, key_code: KeyCode) -> Option<ReedlineEvent> {
        for binding in &self.bindings {
            if binding.modifier == modifier && binding.key_code == key_code {
                return Some(binding.command.clone());
            }
        }

        None
    }
}

fn edit_bind(command: EditCommand) -> ReedlineEvent {
    ReedlineEvent::Edit(vec![command])
}

/// Returns the current default emacs keybindings
pub fn default_emacs_keybindings() -> Keybindings {
    use EditCommand as EC;
    use KeyCode as KC;
    use KeyModifiers as KM;

    let mut kb = Keybindings::new();

    // CTRL
    kb.add_binding(KM::CONTROL, KC::Left, edit_bind(EC::MoveWordLeft));
    kb.add_binding(KM::CONTROL, KC::Right, edit_bind(EC::MoveWordRight));
    kb.add_binding(KM::CONTROL, KC::Delete, edit_bind(EC::DeleteWord));
    kb.add_binding(KM::CONTROL, KC::Backspace, edit_bind(EC::BackspaceWord));
    kb.add_binding(KM::CONTROL, KC::Char('g'), edit_bind(EC::Redo));
    kb.add_binding(KM::CONTROL, KC::Char('z'), edit_bind(EC::Undo));
    kb.add_binding(KM::CONTROL, KC::Char('d'), edit_bind(EC::Delete));
    kb.add_binding(KM::CONTROL, KC::Char('a'), edit_bind(EC::MoveToStart));
    kb.add_binding(KM::CONTROL, KC::Char('e'), edit_bind(EC::MoveToEnd));
    kb.add_binding(KM::CONTROL, KC::Char('k'), edit_bind(EC::CutToEnd));
    kb.add_binding(KM::CONTROL, KC::Char('u'), edit_bind(EC::CutFromStart));
    kb.add_binding(KM::CONTROL, KC::Char('y'), edit_bind(EC::PasteCutBuffer));
    kb.add_binding(KM::CONTROL, KC::Char('b'), edit_bind(EC::MoveLeft));
    kb.add_binding(KM::CONTROL, KC::Char('f'), edit_bind(EC::MoveRight));
    kb.add_binding(KM::CONTROL, KC::Char('c'), edit_bind(EC::Clear));
    kb.add_binding(KM::CONTROL, KC::Char('h'), edit_bind(EC::Backspace));
    kb.add_binding(KM::CONTROL, KC::Char('w'), edit_bind(EC::CutWordLeft));
    kb.add_binding(KM::CONTROL, KC::Char('p'), ReedlineEvent::PreviousHistory);
    kb.add_binding(KM::CONTROL, KC::Char('n'), ReedlineEvent::NextHistory);
    kb.add_binding(KM::CONTROL, KC::Char('r'), edit_bind(EC::SearchHistory));
    kb.add_binding(KM::CONTROL, KC::Char('t'), edit_bind(EC::SwapGraphemes));
    kb.add_binding(KM::ALT, KC::Char('b'), edit_bind(EC::MoveWordLeft));
    kb.add_binding(KM::ALT, KC::Char('f'), edit_bind(EC::MoveWordRight));
    kb.add_binding(KM::ALT, KC::Char('d'), edit_bind(EC::CutWordRight));
    kb.add_binding(KM::ALT, KC::Char('u'), edit_bind(EC::UppercaseWord));
    kb.add_binding(KM::ALT, KC::Char('l'), edit_bind(EC::LowercaseWord));
    kb.add_binding(KM::ALT, KC::Char('c'), edit_bind(EC::CapitalizeChar));
    kb.add_binding(KM::ALT, KC::Left, edit_bind(EC::MoveWordLeft));
    kb.add_binding(KM::ALT, KC::Right, edit_bind(EC::MoveWordRight));
    kb.add_binding(KM::ALT, KC::Enter, edit_bind(EC::InsertChar('\n')));
    kb.add_binding(KM::ALT, KC::Delete, edit_bind(EC::DeleteWord));
    kb.add_binding(KM::ALT, KC::Backspace, edit_bind(EC::BackspaceWord));
    kb.add_binding(KM::NONE, KC::Up, ReedlineEvent::Up);
    kb.add_binding(KM::NONE, KC::End, edit_bind(EC::MoveToEnd));
    kb.add_binding(KM::NONE, KC::Home, edit_bind(EC::MoveToStart));
    kb.add_binding(KM::NONE, KC::Down, ReedlineEvent::Down);
    kb.add_binding(KM::NONE, KC::Left, edit_bind(EC::MoveLeft));
    kb.add_binding(KM::NONE, KC::Right, edit_bind(EC::MoveRight));
    kb.add_binding(KM::NONE, KC::Delete, edit_bind(EC::Delete));
    kb.add_binding(KM::NONE, KC::Backspace, edit_bind(EC::Backspace));

    kb
}
