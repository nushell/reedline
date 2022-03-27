use super::{motion::Motion, parser::ReedlineOption};
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
            Some(Command::PasteAfter)
        }
        Some('P') => {
            let _ = input.next();
            Some(Command::PasteBefore)
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
        Some('a') => {
            let _ = input.next();
            Some(Command::EnterViAppend)
        }
        Some('0') => {
            let _ = input.next();
            Some(Command::MoveToLineStart)
        }
        Some('^') => {
            let _ = input.next();
            Some(Command::MoveToLineStart)
        }
        Some('$') => {
            let _ = input.next();
            Some(Command::MoveToLineEnd)
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
        Some('s') => {
            let _ = input.next();
            Some(Command::HistorySearch)
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
    PasteAfter,
    PasteBefore,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveWordRight,
    MoveWordLeft,
    MoveToLineStart,
    MoveToLineEnd,
    EnterViAppend,
    EnterViInsert,
    Undo,
    DeleteToEnd,
    AppendToEnd,
    Change,
    MoveRightUntil(char),
    MoveRightBefore(char),
    MoveLeftUntil(char),
    MoveLeftBefore(char),
    HistorySearch,
}

impl Command {
    pub fn to_reedline(&self) -> Vec<ReedlineOption> {
        match self {
            Self::MoveUp => vec![ReedlineOption::Event(ReedlineEvent::Up)],
            Self::MoveDown => vec![ReedlineOption::Event(ReedlineEvent::Down)],
            Self::MoveLeft => vec![ReedlineOption::Event(ReedlineEvent::Left)],
            Self::MoveRight => vec![ReedlineOption::Event(ReedlineEvent::Right)],
            Self::MoveToLineStart => vec![ReedlineOption::Edit(EditCommand::MoveToLineStart)],
            Self::MoveToLineEnd => vec![ReedlineOption::Edit(EditCommand::MoveToLineEnd)],
            Self::MoveWordLeft => vec![ReedlineOption::Edit(EditCommand::MoveWordLeft)],
            Self::MoveWordRight => vec![ReedlineOption::Edit(EditCommand::MoveWordRight)],
            Self::EnterViInsert => vec![ReedlineOption::Event(ReedlineEvent::Repaint)],
            Self::EnterViAppend => vec![ReedlineOption::Edit(EditCommand::MoveRight)],
            Self::PasteAfter => vec![ReedlineOption::Edit(EditCommand::PasteCutBufferAfter)],
            Self::PasteBefore => vec![ReedlineOption::Edit(EditCommand::PasteCutBufferBefore)],
            Self::Undo => vec![ReedlineOption::Edit(EditCommand::Undo)],
            Self::DeleteToEnd => vec![ReedlineOption::Edit(EditCommand::CutToLineEnd)],
            Self::AppendToEnd => vec![ReedlineOption::Edit(EditCommand::MoveToEnd)],
            Self::MoveRightUntil(c) => vec![ReedlineOption::Edit(EditCommand::MoveRightUntil(*c))],
            Self::MoveRightBefore(c) => {
                vec![ReedlineOption::Edit(EditCommand::MoveRightBefore(*c))]
            }
            Self::MoveLeftUntil(c) => vec![ReedlineOption::Edit(EditCommand::MoveLeftUntil(*c))],
            Self::MoveLeftBefore(c) => vec![ReedlineOption::Edit(EditCommand::MoveLeftBefore(*c))],
            Self::DeleteChar => vec![ReedlineOption::Edit(EditCommand::Delete)],
            Self::HistorySearch => vec![ReedlineOption::Event(ReedlineEvent::SearchHistory)],
            // Mark a command as incomplete whenever a motion is required to finish the command
            Self::Delete | Self::Change | Self::Incomplete => vec![ReedlineOption::Incomplete],
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
                Motion::Line => Some(vec![ReedlineOption::Edit(EditCommand::CutCurrentLine)]),
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
                Motion::Start => None,
            },
            Self::Change => match motion {
                Motion::End => Some(vec![
                    ReedlineOption::Edit(EditCommand::ClearToLineEnd),
                    ReedlineOption::Event(ReedlineEvent::Repaint),
                ]),
                Motion::Line => Some(vec![
                    ReedlineOption::Edit(EditCommand::MoveToStart),
                    ReedlineOption::Edit(EditCommand::ClearToLineEnd),
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
                Motion::Start => None,
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
