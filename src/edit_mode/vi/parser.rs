use super::command::{parse_command, Command};
use super::motion::{parse_motion, Motion};
use crate::{EditCommand, ReedlineEvent};
use std::iter::Peekable;

#[derive(Debug, Clone)]
pub enum ReedlineOption {
    Event(ReedlineEvent),
    Edit(EditCommand),
    Incomplete,
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
