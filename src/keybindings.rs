use {
    crate::EditCommand,
    crossterm::event::{KeyCode, KeyModifiers},
    serde::{Deserialize, Serialize},
};

#[derive(Serialize, Deserialize, Clone)]
pub struct Keybinding {
    modifier: KeyModifiers,
    key_code: KeyCode,
    edit_commands: Vec<EditCommand>,
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
        edit_commands: Vec<EditCommand>,
    ) {
        self.bindings.push(Keybinding {
            modifier,
            key_code,
            edit_commands,
        });
    }

    pub fn find_binding(
        &self,
        modifier: KeyModifiers,
        key_code: KeyCode,
    ) -> Option<Vec<EditCommand>> {
        for binding in &self.bindings {
            if binding.modifier == modifier && binding.key_code == key_code {
                return Some(binding.edit_commands.clone());
            }
        }

        None
    }
}

pub fn default_vi_normal_keybindings() -> Keybindings {
    use KeyCode::*;

    let mut keybindings = Keybindings::new();

    keybindings.add_binding(
        KeyModifiers::NONE,
        Up,
        vec![EditCommand::ViCommandFragment('k')],
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        Down,
        vec![EditCommand::ViCommandFragment('j')],
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        Left,
        vec![EditCommand::ViCommandFragment('h')],
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        Right,
        vec![EditCommand::ViCommandFragment('l')],
    );

    keybindings
}

pub fn default_vi_insert_keybindings() -> Keybindings {
    use KeyCode::*;

    let mut keybindings = Keybindings::new();

    keybindings.add_binding(KeyModifiers::NONE, Esc, vec![EditCommand::EnterViNormal]);
    keybindings.add_binding(KeyModifiers::NONE, Up, vec![EditCommand::PreviousHistory]);
    keybindings.add_binding(KeyModifiers::NONE, Down, vec![EditCommand::NextHistory]);
    keybindings.add_binding(KeyModifiers::NONE, Left, vec![EditCommand::MoveLeft]);
    keybindings.add_binding(KeyModifiers::NONE, Right, vec![EditCommand::MoveRight]);
    keybindings.add_binding(KeyModifiers::NONE, Backspace, vec![EditCommand::Backspace]);
    keybindings.add_binding(KeyModifiers::NONE, Delete, vec![EditCommand::Delete]);

    keybindings
}

/// Returns the current default emacs keybindings
pub fn default_emacs_keybindings() -> Keybindings {
    use KeyCode::*;

    let mut keybindings = Keybindings::new();

    // CTRL
    keybindings.add_binding(KeyModifiers::CONTROL, Char('g'), vec![EditCommand::Redo]);
    keybindings.add_binding(KeyModifiers::CONTROL, Char('z'), vec![EditCommand::Undo]);
    keybindings.add_binding(KeyModifiers::CONTROL, Char('d'), vec![EditCommand::Delete]);
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Char('a'),
        vec![EditCommand::MoveToStart],
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Char('e'),
        vec![EditCommand::MoveToEnd],
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Char('k'),
        vec![EditCommand::CutToEnd],
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Char('u'),
        vec![EditCommand::CutFromStart],
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Char('y'),
        vec![EditCommand::PasteCutBuffer],
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Char('b'),
        vec![EditCommand::MoveLeft],
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Char('f'),
        vec![EditCommand::MoveRight],
    );
    keybindings.add_binding(KeyModifiers::CONTROL, Char('c'), vec![EditCommand::Clear]);
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Char('h'),
        vec![EditCommand::Backspace],
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Char('w'),
        vec![EditCommand::CutWordLeft],
    );
    keybindings.add_binding(KeyModifiers::CONTROL, Left, vec![EditCommand::MoveWordLeft]);
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Right,
        vec![EditCommand::MoveWordRight],
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Char('p'),
        vec![EditCommand::PreviousHistory],
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Char('n'),
        vec![EditCommand::NextHistory],
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Char('r'),
        vec![EditCommand::SearchHistory],
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Char('t'),
        vec![EditCommand::SwapGraphemes],
    );
    keybindings.add_binding(
        KeyModifiers::ALT,
        Enter,
        vec![EditCommand::InsertChar('\n')],
    );
    keybindings.add_binding(
        KeyModifiers::ALT,
        Char('b'),
        vec![EditCommand::MoveWordLeft],
    );
    keybindings.add_binding(
        KeyModifiers::ALT,
        Char('f'),
        vec![EditCommand::MoveWordRight],
    );
    keybindings.add_binding(
        KeyModifiers::ALT,
        Char('d'),
        vec![EditCommand::CutWordRight],
    );
    keybindings.add_binding(KeyModifiers::ALT, Left, vec![EditCommand::MoveWordLeft]);
    keybindings.add_binding(KeyModifiers::ALT, Right, vec![EditCommand::MoveWordRight]);
    keybindings.add_binding(
        KeyModifiers::ALT,
        Backspace,
        vec![EditCommand::BackspaceWord],
    );
    keybindings.add_binding(KeyModifiers::ALT, Delete, vec![EditCommand::DeleteWord]);
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        Backspace,
        vec![EditCommand::BackspaceWord],
    );
    keybindings.add_binding(KeyModifiers::CONTROL, Delete, vec![EditCommand::DeleteWord]);
    keybindings.add_binding(
        KeyModifiers::ALT,
        Char('u'),
        vec![EditCommand::UppercaseWord],
    );
    keybindings.add_binding(
        KeyModifiers::ALT,
        Char('l'),
        vec![EditCommand::LowercaseWord],
    );
    keybindings.add_binding(
        KeyModifiers::ALT,
        Char('c'),
        vec![EditCommand::CapitalizeChar],
    );
    keybindings.add_binding(KeyModifiers::NONE, Backspace, vec![EditCommand::Backspace]);
    keybindings.add_binding(KeyModifiers::NONE, Delete, vec![EditCommand::Delete]);
    keybindings.add_binding(KeyModifiers::NONE, Home, vec![EditCommand::MoveToStart]);
    keybindings.add_binding(KeyModifiers::NONE, End, vec![EditCommand::MoveToEnd]);
    keybindings.add_binding(KeyModifiers::NONE, Up, vec![EditCommand::Up]);
    keybindings.add_binding(KeyModifiers::NONE, Down, vec![EditCommand::Down]);
    keybindings.add_binding(KeyModifiers::NONE, Left, vec![EditCommand::MoveLeft]);
    keybindings.add_binding(KeyModifiers::NONE, Right, vec![EditCommand::MoveRight]);

    keybindings
}
