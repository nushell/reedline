use crate::{
    enums::{EditCommand, ReedlineEvent},
};

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(super) enum HelixAction {
    Type(char),
    MoveCharRight,
    MoveCharLeft,
    #[default]
    NoOp,
}

impl HelixAction {
    pub(super) fn into_reedline_event(self) -> Option<ReedlineEvent> {
        match self {
            HelixAction::Type(c) => Some(ReedlineEvent::Edit(vec![EditCommand::InsertChar(c)])),
            HelixAction::MoveCharLeft => Some(ReedlineEvent::Edit(vec![
                EditCommand::MoveLeft { select: false },
            ])),
            HelixAction::MoveCharRight => Some(ReedlineEvent::Edit(vec![
                EditCommand::MoveRight { select: false },
            ])),
            HelixAction::NoOp => None,
        }
    }
}