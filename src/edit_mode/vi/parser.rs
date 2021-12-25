use crate::{EditCommand, ReedlineEvent};
use std::{iter::Peekable, str::Bytes};

#[derive(Debug, PartialEq, Eq)]
enum Motion {
    Word,
    Line,
    Start,
    End,
    Until(char),
    Before(char),
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Incomplete,
    Delete,
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
}

#[derive(Debug, Clone)]
enum ReedlineOption {
    Event(ReedlineEvent),
    Edit(EditCommand),
    Incomplete,
}

impl Command {
    fn to_reedline(&self) -> ReedlineOption {
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
            Self::Delete | Self::Change | Self::Incomplete => ReedlineOption::Incomplete,
        }
    }

    fn to_reedline_with_motion(
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
                Motion::Until(c) => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CutRightUntil(*c))])
                }
                Motion::Before(c) => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CutRightBefore(*c))])
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
                Motion::Until(c) => Some(vec![
                    ReedlineOption::Edit(EditCommand::CutRightUntil(*c)),
                    ReedlineOption::Event(ReedlineEvent::Repaint),
                ]),
                Motion::Before(c) => Some(vec![
                    ReedlineOption::Edit(EditCommand::CutRightBefore(*c)),
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

#[derive(Debug, PartialEq, Eq)]
pub struct ParseResult {
    multiplier: Option<usize>,
    command: Option<Command>,
    count: Option<usize>,
    motion: Option<Motion>,
}

impl ParseResult {
    pub fn is_valid(&self) -> bool {
        self.multiplier.is_some()
            || self.command.is_some()
            || self.count.is_some()
            || self.motion.is_some()
    }

    pub fn enter_insert_mode(&self) -> bool {
        match (&self.command, &self.motion) {
            (Some(Command::EnterViInsert), None) => true,
            (Some(Command::AppendToEnd), None) => true,
            (Some(Command::Change), Some(_)) => true,
            _ => false,
        }
    }

    pub fn to_reedline_event(&self) -> ReedlineEvent {
        match (&self.multiplier, &self.command, &self.count, &self.motion) {
            // Movements with h,j,k,l are always single char or a number followed
            // by a single command (char)
            (None, Some(command), None, None) => match command.to_reedline() {
                ReedlineOption::Edit(e) => ReedlineEvent::Edit(vec![e]),
                ReedlineOption::Event(e) => e,
                ReedlineOption::Incomplete => ReedlineEvent::None,
            },
            (Some(multiplier), Some(command), None, None) => match command.to_reedline() {
                ReedlineOption::Edit(e) => {
                    let edits = std::iter::repeat(e)
                        .take(*multiplier)
                        .collect::<Vec<EditCommand>>();

                    ReedlineEvent::Edit(edits)
                }
                ReedlineOption::Event(e) => {
                    let moves = std::iter::repeat(e)
                        .take(*multiplier)
                        .collect::<Vec<ReedlineEvent>>();

                    ReedlineEvent::Multiple(moves)
                }
                ReedlineOption::Incomplete => ReedlineEvent::None,
            },
            (multiplier, Some(command), count, Some(motion)) => {
                match command.to_reedline_with_motion(motion, count) {
                    Some(events) => {
                        let multiplier = multiplier.unwrap_or(1);
                        let events = std::iter::repeat(events)
                            .take(multiplier)
                            .flatten()
                            .map(|option| match option {
                                ReedlineOption::Edit(edit) => ReedlineEvent::Edit(vec![edit]),
                                ReedlineOption::Event(event) => event,
                                ReedlineOption::Incomplete => ReedlineEvent::None,
                            })
                            .collect::<Vec<ReedlineEvent>>();

                        ReedlineEvent::Multiple(events)
                    }
                    None => ReedlineEvent::None,
                }
            }
            _ => ReedlineEvent::None,
        }
    }
}

type InputIterator<'a> = Peekable<Bytes<'a>>;

fn parse_motion(input: &mut InputIterator) -> Option<Motion> {
    match input.peek() {
        Some(b'w') => {
            let _ = input.next();
            Some(Motion::Word)
        }
        Some(b'd') => {
            let _ = input.next();
            Some(Motion::Line)
        }
        Some(b'0') => {
            let _ = input.next();
            Some(Motion::Start)
        }
        Some(b'$') => {
            let _ = input.next();
            Some(Motion::End)
        }
        Some(b'f') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Motion::Until(*c as char)),
                None => None,
            }
        }
        Some(b't') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Motion::Before(*c as char)),
                None => None,
            }
        }
        _ => None,
    }
}

fn parse_command(input: &mut InputIterator) -> Option<Command> {
    match input.peek() {
        Some(b'd') => {
            let _ = input.next();
            Some(Command::Delete)
        }
        Some(b'p') => {
            let _ = input.next();
            Some(Command::Paste)
        }
        Some(b'h') => {
            let _ = input.next();
            Some(Command::MoveLeft)
        }
        Some(b'l') => {
            let _ = input.next();
            Some(Command::MoveRight)
        }
        Some(b'j') => {
            let _ = input.next();
            Some(Command::MoveDown)
        }
        Some(b'k') => {
            let _ = input.next();
            Some(Command::MoveUp)
        }
        Some(b'w') => {
            let _ = input.next();
            Some(Command::MoveWordRight)
        }
        Some(b'b') => {
            let _ = input.next();
            Some(Command::MoveWordLeft)
        }
        Some(b'i') => {
            let _ = input.next();
            Some(Command::EnterViInsert)
        }
        Some(b'0') => {
            let _ = input.next();
            Some(Command::MoveToStart)
        }
        Some(b'$') => {
            let _ = input.next();
            Some(Command::MoveToEnd)
        }
        Some(b'u') => {
            let _ = input.next();
            Some(Command::Undo)
        }
        Some(b'c') => {
            let _ = input.next();
            Some(Command::Change)
        }
        Some(b'D') => {
            let _ = input.next();
            Some(Command::DeleteToEnd)
        }
        Some(b'A') => {
            let _ = input.next();
            Some(Command::AppendToEnd)
        }
        Some(b'f') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Command::MoveRightUntil(*c as char)),
                None => Some(Command::Incomplete),
            }
        }
        Some(b't') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Command::MoveRightBefore(*c as char)),
                None => Some(Command::Incomplete),
            }
        }
        _ => None,
    }
}

fn parse_number(input: &mut InputIterator) -> Option<usize> {
    match input.peek() {
        Some(b'0') => return None,
        Some(x) if x.is_ascii_digit() => {
            let mut count: usize = 0;
            while let Some(&c) = input.peek() {
                if c.is_ascii_digit() {
                    let _ = input.next();
                    count *= 10;
                    count += (c - b'0') as usize;
                } else {
                    return Some(count);
                }
            }
            Some(count)
        }
        _ => None,
    }
}

pub fn parse(input: &mut InputIterator) -> ParseResult {
    let multiplier = parse_number(input);
    let command = parse_command(input);
    let count = parse_number(input);
    let motion = parse_motion(input);

    // validate input here

    ParseResult {
        multiplier,
        command,
        count,
        motion,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn vi_parse(input: &str) -> ParseResult {
        let mut bytes = input.bytes().peekable();

        parse(&mut bytes)
    }

    #[test]
    fn test_delete_word() {
        let input = "dw";
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParseResult {
                multiplier: None,
                command: Some(Command::Delete),
                count: None,
                motion: Some(Motion::Word)
            }
        );
    }

    #[test]
    fn test_two_delete_word() {
        let input = "2dw";
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParseResult {
                multiplier: Some(2),
                command: Some(Command::Delete),
                count: None,
                motion: Some(Motion::Word)
            }
        );
    }

    #[test]
    fn test_two_delete_two_word() {
        let input = "2d2w";
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParseResult {
                multiplier: Some(2),
                command: Some(Command::Delete),
                count: Some(2),
                motion: Some(Motion::Word)
            }
        );
    }

    #[test]
    fn test_two_delete_two_lines() {
        let input = "2dd";
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParseResult {
                multiplier: Some(2),
                command: Some(Command::Delete),
                count: None,
                motion: Some(Motion::Line),
            }
        );
    }

    #[test]
    fn test_two_up() {
        let input = "2k";
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParseResult {
                multiplier: Some(2),
                command: Some(Command::MoveUp),
                count: None,
                motion: None,
            }
        );
    }

    #[rstest]
    #[case("2k", ReedlineEvent::Multiple(vec![ReedlineEvent::Up, ReedlineEvent::Up]))]
    #[case("k", ReedlineEvent::Up)]
    #[case("2j", ReedlineEvent::Multiple(vec![ReedlineEvent::Down, ReedlineEvent::Down]))]
    #[case("j", ReedlineEvent::Down)]
    #[case("2l", ReedlineEvent::Edit(vec![EditCommand::MoveRight, EditCommand::MoveRight]))]
    #[case("l", ReedlineEvent::Edit(vec![EditCommand::MoveRight]))]
    #[case("2h", ReedlineEvent::Edit(vec![EditCommand::MoveLeft, EditCommand::MoveLeft]))]
    #[case("h", ReedlineEvent::Edit(vec![EditCommand::MoveLeft]))]
    #[case("0", ReedlineEvent::Edit(vec![EditCommand::MoveToStart]))]
    #[case("$", ReedlineEvent::Edit(vec![EditCommand::MoveToEnd]))]
    #[case("i", ReedlineEvent::Repaint)]
    #[case("p", ReedlineEvent::Edit(vec![EditCommand::PasteCutBuffer]))]
    #[case("2p", ReedlineEvent::Edit(vec![EditCommand::PasteCutBuffer, EditCommand::PasteCutBuffer]))]
    #[case("u", ReedlineEvent::Edit(vec![EditCommand::Undo]))]
    #[case("2u", ReedlineEvent::Edit(vec![EditCommand::Undo, EditCommand::Undo]))]
    #[case("dd", ReedlineEvent::Edit(vec![EditCommand::MoveToStart, EditCommand::CutToEnd]))]
    #[case("dw", ReedlineEvent::Edit(vec![EditCommand::CutWordRight]))]
    fn test_reedline_move(#[case] input: &str, #[case] expected: ReedlineEvent) {
        let res = vi_parse(input);
        let output = res.to_reedline_event();

        assert_eq!(output, expected);
    }
}
