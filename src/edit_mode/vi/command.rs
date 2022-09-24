use super::{motion::Motion, motion::ViToTill, parser::ReedlineOption};
use crate::{EditCommand, ReedlineEvent, Vi};
use std::iter::Peekable;

pub fn parse_command<'iter, I>(vi: &Vi, input: &mut Peekable<I>) -> Option<Command>
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
            Some(Command::MoveWordRightStart)
        }
        Some('W') => {
            let _ = input.next();
            Some(Command::MoveBigWordRightStart)
        }
        Some('e') => {
            let _ = input.next();
            Some(Command::MoveWordRightEnd)
        }
        Some('E') => {
            let _ = input.next();
            Some(Command::MoveBigWordRightEnd)
        }
        Some('b') => {
            let _ = input.next();
            Some(Command::MoveWordLeft)
        }
        Some('B') => {
            let _ = input.next();
            Some(Command::MoveBigWordLeft)
        }
        Some('i') => {
            let _ = input.next();
            Some(Command::EnterViInsert)
        }
        Some('a') => {
            let _ = input.next();
            Some(Command::EnterViAppend)
        }
        Some('0' | '^') => {
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
        Some('r') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Command::ReplaceChar(**c)),
                None => Some(Command::Incomplete),
            }
        }
        Some('s') => {
            let _ = input.next();
            Some(Command::SubstituteCharWithInsert)
        }
        Some('?') => {
            let _ = input.next();
            Some(Command::HistorySearch)
        }
        Some('C') => {
            let _ = input.next();
            Some(Command::ChangeToLineEnd)
        }
        Some('D') => {
            let _ = input.next();
            Some(Command::DeleteToEnd)
        }
        Some('I') => {
            let _ = input.next();
            Some(Command::PrependToStart)
        }
        Some('A') => {
            let _ = input.next();
            Some(Command::AppendToEnd)
        }
        Some('S') => {
            let _ = input.next();
            Some(Command::RewriteCurrentLine)
        }
        Some('f') => {
            let _ = input.next();
            match input.peek() {
                Some(&c) => {
                    input.next();
                    Some(Command::MoveRightUntil(*c))
                }
                None => Some(Command::Incomplete),
            }
        }
        Some('t') => {
            let _ = input.next();
            match input.peek() {
                Some(&c) => {
                    input.next();
                    Some(Command::MoveRightBefore(*c))
                }
                None => Some(Command::Incomplete),
            }
        }
        Some('F') => {
            let _ = input.next();
            match input.peek() {
                Some(&c) => {
                    input.next();
                    Some(Command::MoveLeftUntil(*c))
                }
                None => Some(Command::Incomplete),
            }
        }
        Some('T') => {
            let _ = input.next();
            match input.peek() {
                Some(&c) => {
                    input.next();
                    Some(Command::MoveLeftBefore(*c))
                }
                None => Some(Command::Incomplete),
            }
        }
        Some(';') => {
            let _ = input.next();
            vi.last_to_till
                .as_ref()
                .map(|to_till| Command::ReplayToTill(to_till.clone()))
        }
        Some(',') => {
            let _ = input.next();
            vi.last_to_till
                .as_ref()
                .map(|to_till| Command::ReverseToTill(to_till.clone()))
        }
        Some('~') => {
            let _ = input.next();
            Some(Command::Switchcase)
        }
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Incomplete,
    Delete,
    DeleteChar,
    ReplaceChar(char),
    SubstituteCharWithInsert,
    PasteAfter,
    PasteBefore,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveWordRightStart,
    MoveBigWordRightStart,
    MoveWordRightEnd,
    MoveBigWordRightEnd,
    MoveWordLeft,
    MoveBigWordLeft,
    MoveToLineStart,
    MoveToLineEnd,
    EnterViAppend,
    EnterViInsert,
    Undo,
    ChangeToLineEnd,
    DeleteToEnd,
    AppendToEnd,
    PrependToStart,
    RewriteCurrentLine,
    Change,
    MoveRightUntil(char),
    MoveRightBefore(char),
    MoveLeftUntil(char),
    MoveLeftBefore(char),
    ReplayToTill(ViToTill),
    ReverseToTill(ViToTill),
    HistorySearch,
    Switchcase,
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
            Self::MoveBigWordLeft => vec![ReedlineOption::Edit(EditCommand::MoveBigWordLeft)],
            Self::MoveWordRightStart => vec![ReedlineOption::Edit(EditCommand::MoveWordRightStart)],
            Self::MoveBigWordRightStart => {
                vec![ReedlineOption::Edit(EditCommand::MoveBigWordRightStart)]
            }
            Self::MoveWordRightEnd => vec![ReedlineOption::Edit(EditCommand::MoveWordRightEnd)],
            Self::MoveBigWordRightEnd => {
                vec![ReedlineOption::Edit(EditCommand::MoveBigWordRightEnd)]
            }
            Self::EnterViInsert => vec![ReedlineOption::Event(ReedlineEvent::Repaint)],
            Self::EnterViAppend => vec![ReedlineOption::Edit(EditCommand::MoveRight)],
            Self::PasteAfter => vec![ReedlineOption::Edit(EditCommand::PasteCutBufferAfter)],
            Self::PasteBefore => vec![ReedlineOption::Edit(EditCommand::PasteCutBufferBefore)],
            Self::Undo => vec![ReedlineOption::Edit(EditCommand::Undo)],
            Self::ChangeToLineEnd => vec![ReedlineOption::Edit(EditCommand::ClearToLineEnd)],
            Self::DeleteToEnd => vec![ReedlineOption::Edit(EditCommand::CutToLineEnd)],
            Self::AppendToEnd => vec![ReedlineOption::Edit(EditCommand::MoveToLineEnd)],
            Self::PrependToStart => vec![ReedlineOption::Edit(EditCommand::MoveToLineStart)],
            Self::RewriteCurrentLine => vec![ReedlineOption::Edit(EditCommand::CutCurrentLine)],
            Self::MoveRightUntil(c) => vec![
                ReedlineOption::Event(ReedlineEvent::RecordToTill),
                ReedlineOption::Edit(EditCommand::MoveRightUntil(*c)),
            ],
            Self::MoveRightBefore(c) => {
                vec![
                    ReedlineOption::Event(ReedlineEvent::RecordToTill),
                    ReedlineOption::Edit(EditCommand::MoveRightBefore(*c)),
                ]
            }
            Self::MoveLeftUntil(c) => vec![
                ReedlineOption::Event(ReedlineEvent::RecordToTill),
                ReedlineOption::Edit(EditCommand::MoveLeftUntil(*c)),
            ],
            Self::MoveLeftBefore(c) => vec![
                ReedlineOption::Event(ReedlineEvent::RecordToTill),
                ReedlineOption::Edit(EditCommand::MoveLeftBefore(*c)),
            ],
            Self::ReplayToTill(to_till) => vec![ReedlineOption::Edit(to_till.into())],
            Self::ReverseToTill(to_till) => vec![ReedlineOption::Edit(to_till.reverse().into())],
            Self::DeleteChar => vec![ReedlineOption::Edit(EditCommand::CutChar)],
            Self::ReplaceChar(c) => {
                vec![ReedlineOption::Edit(EditCommand::ReplaceChar(*c))]
            }
            Self::SubstituteCharWithInsert => vec![ReedlineOption::Edit(EditCommand::CutChar)],
            Self::HistorySearch => vec![ReedlineOption::Event(ReedlineEvent::SearchHistory)],
            Self::Switchcase => vec![ReedlineOption::Edit(EditCommand::SwitchcaseChar)],
            // Mark a command as incomplete whenever a motion is required to finish the command
            Self::Delete | Self::Change | Self::Incomplete => vec![ReedlineOption::Incomplete],
        }
    }

    pub fn to_reedline_with_motion(&self, motion: &Motion) -> Option<Vec<ReedlineOption>> {
        match self {
            Self::Delete => match motion {
                Motion::End => Some(vec![ReedlineOption::Edit(EditCommand::CutToLineEnd)]),
                Motion::Line => Some(vec![ReedlineOption::Edit(EditCommand::CutCurrentLine)]),
                Motion::NextWord => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CutWordRightToNext)])
                }
                Motion::NextBigWord => Some(vec![ReedlineOption::Edit(
                    EditCommand::CutBigWordRightToNext,
                )]),
                Motion::NextWordEnd => Some(vec![ReedlineOption::Edit(EditCommand::CutWordRight)]),
                Motion::NextBigWordEnd => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CutBigWordRight)])
                }
                Motion::PreviousWord => Some(vec![ReedlineOption::Edit(EditCommand::CutWordLeft)]),
                Motion::PreviousBigWord => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CutBigWordLeft)])
                }
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
                Motion::Start => Some(vec![ReedlineOption::Edit(EditCommand::CutFromLineStart)]),
            },
            Self::Change => {
                let op = match motion {
                    Motion::End => Some(vec![ReedlineOption::Edit(EditCommand::ClearToLineEnd)]),
                    Motion::Line => Some(vec![
                        ReedlineOption::Edit(EditCommand::MoveToStart),
                        ReedlineOption::Edit(EditCommand::ClearToLineEnd),
                    ]),
                    Motion::NextWord => {
                        Some(vec![ReedlineOption::Edit(EditCommand::CutWordRightToNext)])
                    }
                    Motion::NextBigWord => Some(vec![ReedlineOption::Edit(
                        EditCommand::CutBigWordRightToNext,
                    )]),
                    Motion::NextWordEnd => {
                        Some(vec![ReedlineOption::Edit(EditCommand::CutWordRight)])
                    }
                    Motion::NextBigWordEnd => {
                        Some(vec![ReedlineOption::Edit(EditCommand::CutBigWordRight)])
                    }
                    Motion::PreviousWord => {
                        Some(vec![ReedlineOption::Edit(EditCommand::CutWordLeft)])
                    }
                    Motion::PreviousBigWord => {
                        Some(vec![ReedlineOption::Edit(EditCommand::CutBigWordLeft)])
                    }
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
                    Motion::Start => {
                        Some(vec![ReedlineOption::Edit(EditCommand::CutFromLineStart)])
                    }
                };
                // Semihack: Append `Repaint` to ensure the mode change gets displayed
                op.map(|mut vec| {
                    vec.push(ReedlineOption::Event(ReedlineEvent::Repaint));
                    vec
                })
            }
            _ => None,
        }
    }
}

impl From<ViToTill> for EditCommand {
    fn from(val: ViToTill) -> Self {
        EditCommand::from(&val)
    }
}

impl From<&ViToTill> for EditCommand {
    fn from(val: &ViToTill) -> Self {
        match val {
            ViToTill::TillLeft(c) => EditCommand::MoveLeftBefore(*c),
            ViToTill::ToLeft(c) => EditCommand::MoveLeftUntil(*c),
            ViToTill::TillRight(c) => EditCommand::MoveRightBefore(*c),
            ViToTill::ToRight(c) => EditCommand::MoveRightUntil(*c),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    #[rstest]
    #[case(';', None, None)]
    #[case(',', None, None)]
    #[case(
        ';',
        Some(ViToTill::ToRight('X')),
        Some(Command::ReplayToTill(ViToTill::ToRight('X')))
    )]
    #[case(
        ',',
        Some(ViToTill::ToRight('X')),
        Some(Command::ReverseToTill(ViToTill::ToRight('X')))
    )]
    fn repeat_to_till(
        #[case] input: char,
        #[case] last_to_till: Option<ViToTill>,
        #[case] expected: Option<Command>,
    ) {
        let vi = Vi {
            last_to_till,
            ..Vi::default()
        };

        let input = vec![input];

        let result = parse_command(&vi, &mut input.iter().peekable());

        assert_eq!(result, expected);
    }
}
