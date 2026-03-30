use crate::PromptViMode;
use modalkit::keybindings::{EmptyKeyState, InputKey, ModalMachine, Mode, ModeKeys};

use super::{action::HelixAction, key::HelixKey};

#[derive(Clone, Copy, Debug, Default, Hash, Eq, PartialEq)]
pub(super) enum HelixMode {
    #[default]
    Insert,
    Normal,
}

impl Mode<HelixAction, EmptyKeyState> for HelixMode {}

impl From<PromptViMode> for HelixMode {
    fn from(mode: PromptViMode) -> Self {
        match mode {
            PromptViMode::Insert => HelixMode::Insert,
            PromptViMode::Normal => HelixMode::Normal,
        }
    }
}

impl ModeKeys<HelixKey, HelixAction, EmptyKeyState> for HelixMode {
    fn unmapped(
        &self,
        key: &HelixKey,
        _: &mut EmptyKeyState,
    ) -> (Vec<HelixAction>, Option<HelixMode>) {
        match self {
            HelixMode::Normal => (vec![], None),
            HelixMode::Insert => {
                if let Some(c) = key.get_char() {
                    return (vec![HelixAction::Type(c)], None);
                }

                (vec![], None)
            }
        }
    }
}

pub(super) type HelixStep = (Option<HelixAction>, Option<HelixMode>);

pub(super) type HelixMachine = ModalMachine<HelixKey, HelixStep>;
