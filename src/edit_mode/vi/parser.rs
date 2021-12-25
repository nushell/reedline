use crate::{EditCommand, ReedlineEvent};
use std::iter::Peekable;

#[derive(Debug, PartialEq, Eq)]
enum Motion {
    Word,
    Line,
    Start,
    End,
    RightUntil(char),
    RightBefore(char),
    LeftUntil(char),
    LeftBefore(char),
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
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
            Self::MoveLeftUntil(c) => ReedlineOption::Edit(EditCommand::MoveLeftUntil(*c)),
            Self::MoveLeftBefore(c) => ReedlineOption::Edit(EditCommand::MoveLeftBefore(*c)),
            Self::DeleteChar => ReedlineOption::Edit(EditCommand::Delete),
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

fn parse_motion<'iter, I>(input: &mut Peekable<I>) -> Option<Motion>
where
    I: Iterator<Item = &'iter char>,
{
    match input.peek() {
        Some('w') => {
            let _ = input.next();
            Some(Motion::Word)
        }
        Some('d') => {
            let _ = input.next();
            Some(Motion::Line)
        }
        Some('0') => {
            let _ = input.next();
            Some(Motion::Start)
        }
        Some('$') => {
            let _ = input.next();
            Some(Motion::End)
        }
        Some('f') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Motion::RightUntil(**c)),
                None => None,
            }
        }
        Some('t') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Motion::RightBefore(**c)),
                None => None,
            }
        }
        Some('F') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Motion::LeftUntil(**c)),
                None => None,
            }
        }
        Some('T') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Motion::LeftBefore(**c)),
                None => None,
            }
        }
        _ => None,
    }
}

fn parse_command<'iter, I>(input: &mut Peekable<I>) -> Option<Command>
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

fn parse_number<'iter, I>(input: &mut Peekable<I>) -> Option<usize>
where
    I: Iterator<Item = &'iter char>,
{
    match input.peek() {
        Some('0') => return None,
        Some(x) if x.is_ascii_digit() => {
            let mut count: usize = 0;
            while let Some(&c) = input.peek() {
                if c.is_ascii_digit() {
                    let c = c.to_digit(10).expect("already checked if is a digit");
                    let _ = input.next();
                    count *= 10;
                    count += c as usize;
                } else {
                    return Some(count);
                }
            }
            Some(count)
        }
        _ => None,
    }
}

pub fn parse<'iter, I>(input: &mut Peekable<I>) -> ParseResult
where
    I: Iterator<Item = &'iter char>,
{
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

    fn vi_parse(input: &[char]) -> ParseResult {
        parse(&mut input.iter().peekable())
    }

    #[test]
    fn test_delete_word() {
        let input = ['d', 'w'];
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
        let input = ['2', 'd', 'w'];
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
        let input = ['2', 'd', '2', 'w'];
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
    fn test_two_delete_twenty_word() {
        let input = ['2', 'd', '2', '0', 'w'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParseResult {
                multiplier: Some(2),
                command: Some(Command::Delete),
                count: Some(20),
                motion: Some(Motion::Word)
            }
        );
    }

    #[test]
    fn test_two_delete_two_lines() {
        let input = ['2', 'd', 'd'];
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
        let input = ['2', 'k'];
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
    #[case(&['2', 'k'], ReedlineEvent::Multiple(vec![ReedlineEvent::Up, ReedlineEvent::Up]))]
    #[case(&['k'], ReedlineEvent::Up)]
    #[case(&['2', 'j'], ReedlineEvent::Multiple(vec![ReedlineEvent::Down, ReedlineEvent::Down]))]
    #[case(&['j'], ReedlineEvent::Down)]
    #[case(&['2', 'l'], ReedlineEvent::Edit(vec![EditCommand::MoveRight, EditCommand::MoveRight]))]
    #[case(&['l'], ReedlineEvent::Edit(vec![EditCommand::MoveRight]))]
    #[case(&['2', 'h'], ReedlineEvent::Edit(vec![EditCommand::MoveLeft, EditCommand::MoveLeft]))]
    #[case(&['h'], ReedlineEvent::Edit(vec![EditCommand::MoveLeft]))]
    #[case(&['0'], ReedlineEvent::Edit(vec![EditCommand::MoveToStart]))]
    #[case(&['$'], ReedlineEvent::Edit(vec![EditCommand::MoveToEnd]))]
    #[case(&['i'], ReedlineEvent::Repaint)]
    #[case(&['p'], ReedlineEvent::Edit(vec![EditCommand::PasteCutBuffer]))]
    #[case(&['2', 'p'], ReedlineEvent::Edit(vec![EditCommand::PasteCutBuffer, EditCommand::PasteCutBuffer]))]
    #[case(&['u'], ReedlineEvent::Edit(vec![EditCommand::Undo]))]
    #[case(&['2', 'u'], ReedlineEvent::Edit(vec![EditCommand::Undo, EditCommand::Undo]))]
    #[case(&['d', 'd'], ReedlineEvent::Multiple(vec![ ReedlineEvent::Edit(vec![EditCommand::MoveToStart]), ReedlineEvent::Edit(vec![EditCommand::CutToEnd]) ]))]
    #[case(&['d', 'w'], ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::CutWordRight])]))]
    fn test_reedline_move(#[case] input: &[char], #[case] expected: ReedlineEvent) {
        let res = vi_parse(input);
        let output = res.to_reedline_event();

        assert_eq!(output, expected);
    }
}
