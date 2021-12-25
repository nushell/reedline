use super::motion::Motion;
use super::parser::ReedlineOption;
use crate::{EditCommand, ReedlineEvent};
use std::iter::Peekable;

pub fn parse_command<'iter, I>(input: &mut Peekable<I>) -> Option<Command>
where
    I: Iterator<Item = &'iter char>,
{
    match input.peek() {
        Some('d') => {
            let _ = input.next();
            Some(Command::Delete)
        }
        Some('p') => {
            let _ = input.next();
            Some(Command::Paste)
        }
        Some('h') => {
            let _ = input.next();
            Some(Command::MoveLeft)
        }
        Some('l') => {
            let _ = input.next();
            Some(Command::MoveRight)
        }
        Some('j') => {
            let _ = input.next();
            Some(Command::MoveDown)
        }
        Some('k') => {
            let _ = input.next();
            Some(Command::MoveUp)
        }
        Some('w') => {
            let _ = input.next();
            Some(Command::MoveWordRight)
        }
        Some('b') => {
            let _ = input.next();
            Some(Command::MoveWordLeft)
        }
        Some('i') => {
            let _ = input.next();
            Some(Command::EnterViInsert)
        }
        Some('0') => {
            let _ = input.next();
            Some(Command::MoveToStart)
        }
        Some('$') => {
            let _ = input.next();
            Some(Command::MoveToEnd)
        }
        Some('u') => {
            let _ = input.next();
            Some(Command::Undo)
        }
        Some('c') => {
            let _ = input.next();
            Some(Command::Change)
        }
        Some('x') => {
            let _ = input.next();
            Some(Command::DeleteChar)
        }
        Some('D') => {
            let _ = input.next();
            Some(Command::DeleteToEnd)
        }
        Some('A') => {
            let _ = input.next();
            Some(Command::AppendToEnd)
        }
        Some('f') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Command::MoveRightUntil(**c)),
                None => Some(Command::Incomplete),
            }
        }
        Some('t') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Command::MoveRightBefore(**c)),
                None => Some(Command::Incomplete),
            }
        }
        Some('F') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Command::MoveLeftUntil(**c)),
                None => Some(Command::Incomplete),
            }
        }
        Some('T') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Command::MoveLeftBefore(**c)),
                None => Some(Command::Incomplete),
            }
        }
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Incomplete,
    Delete,
    DeleteChar,
    Paste,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveWordRight,
    MoveWordLeft,
    MoveToStart,
    MoveToEnd,
    EnterViInsert,
    Undo,
    DeleteToEnd,
    AppendToEnd,
    Change,
    MoveRightUntil(char),
    MoveRightBefore(char),
    MoveLeftUntil(char),
    MoveLeftBefore(char),
}

impl Command {
    pub fn to_reedline(&self) -> ReedlineOption {
        match self {
            Self::MoveUp => ReedlineOption::Event(ReedlineEvent::Up),
            Self::MoveDown => ReedlineOption::Event(ReedlineEvent::Down),
            Self::MoveLeft => ReedlineOption::Edit(EditCommand::MoveLeft),
            Self::MoveRight => ReedlineOption::Edit(EditCommand::MoveRight),
            Self::MoveToStart => ReedlineOption::Edit(EditCommand::MoveToStart),
            Self::MoveToEnd => ReedlineOption::Edit(EditCommand::MoveToEnd),
            Self::MoveWordLeft => ReedlineOption::Edit(EditCommand::MoveWordLeft),
            Self::MoveWordRight => ReedlineOption::Edit(EditCommand::MoveWordRight),
            Self::EnterViInsert => ReedlineOption::Event(ReedlineEvent::Repaint),
            Self::Paste => ReedlineOption::Edit(EditCommand::PasteCutBuffer),
            Self::Undo => ReedlineOption::Edit(EditCommand::Undo),
            Self::DeleteToEnd => ReedlineOption::Edit(EditCommand::CutToEnd),
            Self::AppendToEnd => ReedlineOption::Edit(EditCommand::MoveToEnd),
            Self::MoveRightUntil(c) => ReedlineOption::Edit(EditCommand::MoveRightUntil(*c)),
            Self::MoveRightBefore(c) => ReedlineOption::Edit(EditCommand::MoveRightBefore(*c)),
            Self::MoveLeftUntil(c) => ReedlineOption::Edit(EditCommand::MoveLeftUntil(*c)),
            Self::MoveLeftBefore(c) => ReedlineOption::Edit(EditCommand::MoveLeftBefore(*c)),
            Self::DeleteChar => ReedlineOption::Edit(EditCommand::Delete),
            Self::Delete | Self::Change | Self::Incomplete => ReedlineOption::Incomplete,
        }
    }

    pub fn to_reedline_with_motion(
        &self,
        motion: &Motion,
        count: &Option<usize>,
    ) -> Option<Vec<ReedlineOption>> {
        let edits = match self {
            Self::Delete => match motion {
                Motion::End => Some(vec![ReedlineOption::Edit(EditCommand::CutToEnd)]),
                Motion::Line => Some(vec![
                    ReedlineOption::Edit(EditCommand::MoveToStart),
                    ReedlineOption::Edit(EditCommand::CutToEnd),
                ]),
                Motion::Word => Some(vec![ReedlineOption::Edit(EditCommand::CutWordRight)]),
                Motion::RightUntil(c) => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CutRightUntil(*c))])
                }
                Motion::RightBefore(c) => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CutRightBefore(*c))])
                }
                Motion::LeftUntil(c) => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CutLeftUntil(*c))])
                }
                Motion::LeftBefore(c) => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CutLeftBefore(*c))])
                }
                _ => None,
            },
            Self::Change => match motion {
                Motion::End => Some(vec![
                    ReedlineOption::Edit(EditCommand::CutToEnd),
                    ReedlineOption::Event(ReedlineEvent::Repaint),
                ]),
                Motion::Line => Some(vec![
                    ReedlineOption::Edit(EditCommand::MoveToStart),
                    ReedlineOption::Edit(EditCommand::CutToEnd),
                    ReedlineOption::Event(ReedlineEvent::Repaint),
                ]),
                Motion::Word => Some(vec![
                    ReedlineOption::Edit(EditCommand::CutWordRight),
                    ReedlineOption::Event(ReedlineEvent::Repaint),
                ]),
                Motion::RightUntil(c) => Some(vec![
                    ReedlineOption::Edit(EditCommand::CutRightUntil(*c)),
                    ReedlineOption::Event(ReedlineEvent::Repaint),
                ]),
                Motion::RightBefore(c) => Some(vec![
                    ReedlineOption::Edit(EditCommand::CutRightBefore(*c)),
                    ReedlineOption::Event(ReedlineEvent::Repaint),
                ]),
                Motion::LeftUntil(c) => Some(vec![
                    ReedlineOption::Edit(EditCommand::CutLeftUntil(*c)),
                    ReedlineOption::Event(ReedlineEvent::Repaint),
                ]),
                Motion::LeftBefore(c) => Some(vec![
                    ReedlineOption::Edit(EditCommand::CutLeftBefore(*c)),
                    ReedlineOption::Event(ReedlineEvent::Repaint),
                ]),
                _ => None,
            },
            _ => None,
        };

        match count {
            Some(count) => edits.map(|edits| {
                std::iter::repeat(edits)
                    .take(*count)
                    .flatten()
                    .collect::<Vec<ReedlineOption>>()
            }),
            None => edits,
        }
    }
}
