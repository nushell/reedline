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
    valid: bool,
}

impl ParseResult {
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    pub fn enter_insert_mode(&self) -> bool {
        matches!(
            (&self.command, &self.motion),
            (Some(Command::EnterViInsert), None)
                | (Some(Command::EnterViAppend), None)
                | (Some(Command::AppendToEnd), None)
                | (Some(Command::HistorySearch), None)
                | (Some(Command::Change), Some(_))
        )
    }

    pub fn to_reedline_event(&self) -> ReedlineEvent {
        match (&self.multiplier, &self.command, &self.count, &self.motion) {
            // Movements with h,j,k,l are always single char or a number followed
            // by a single command (char)
            (multiplier, Some(command), None, None) => {
                let events = command.to_reedline().into_iter().map(|event| match event {
                    ReedlineOption::Edit(e) => ReedlineEvent::Edit(vec![e]),
                    ReedlineOption::Event(e) => e,
                    ReedlineOption::Incomplete => ReedlineEvent::None,
                });

                let multiplier = multiplier.unwrap_or(1);
                let events = std::iter::repeat(events)
                    .take(multiplier)
                    .flatten()
                    .collect::<Vec<ReedlineEvent>>();

                if events.contains(&ReedlineEvent::None) {
                    ReedlineEvent::None
                } else {
                    ReedlineEvent::Multiple(events)
                }
            }
            // This case handles all combinations of commands and motions that could exist
            // The option count is used to multiply the actions that should be done with the motion
            // and the multiplier repeats the whole chain x number of time
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
        Some('0') => None,
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

    let valid =
        { multiplier.is_some() || command.is_some() || count.is_some() || motion.is_some() };

    // If after parsing all the input characters there is a remainder,
    // then there is garbage in the input. Having unrecognized characters will get
    // the user stuck in normal mode until the cache is clear, specially with
    // commands that could be incomplete until a motion is introduced (e.g. delete or change)
    // Better mark it as invalid for the cache to be cleared
    let has_garbage = input.next().is_some();

    ParseResult {
        multiplier,
        command,
        count,
        motion,
        valid: valid && !has_garbage,
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
                motion: Some(Motion::Word),
                valid: true
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
                motion: Some(Motion::Word),
                valid: true
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
                motion: Some(Motion::Word),
                valid: true
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
                motion: Some(Motion::Word),
                valid: true
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
                valid: true
            }
        );
    }

    #[test]
    fn test_has_garbage() {
        let input = ['2', 'd', 'm'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParseResult {
                multiplier: Some(2),
                command: Some(Command::Delete),
                count: None,
                motion: None,
                valid: false
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
                valid: true
            }
        );
    }

    #[rstest]
    #[case(&['2', 'k'], ReedlineEvent::Multiple(vec![ReedlineEvent::Up, ReedlineEvent::Up]))]
    #[case(&['k'], ReedlineEvent::Multiple(vec![ReedlineEvent::Up]))]
    #[case(&['2', 'j'], ReedlineEvent::Multiple(vec![ReedlineEvent::Down, ReedlineEvent::Down]))]
    #[case(&['j'], ReedlineEvent::Multiple(vec![ReedlineEvent::Down]))]
    #[case(&['2', 'l'], ReedlineEvent::Multiple(vec![
        ReedlineEvent::Right,
        ReedlineEvent::Right
        ]))]
    #[case(&['l'], ReedlineEvent::Multiple(vec![ReedlineEvent::Right]))]
    #[case(&['2', 'h'], ReedlineEvent::Multiple(vec![
        ReedlineEvent::Left,
        ReedlineEvent::Left,
        ]))]
    #[case(&['h'], ReedlineEvent::Multiple(vec![ReedlineEvent::Left]))]
    #[case(&['0'], ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::MoveToLineStart])]))]
    #[case(&['$'], ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::MoveToLineEnd])]))]
    #[case(&['i'], ReedlineEvent::Multiple(vec![ReedlineEvent::Repaint]))]
    #[case(&['p'], ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::PasteCutBufferAfter])]))]
    #[case(&['2', 'p'], ReedlineEvent::Multiple(vec![
        ReedlineEvent::Edit(vec![EditCommand::PasteCutBufferAfter]),
        ReedlineEvent::Edit(vec![EditCommand::PasteCutBufferAfter])
        ]))]
    #[case(&['u'], ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::Undo])]))]
    #[case(&['2', 'u'], ReedlineEvent::Multiple(vec![
        ReedlineEvent::Edit(vec![EditCommand::Undo]),
        ReedlineEvent::Edit(vec![EditCommand::Undo])
        ]))]
    #[case(&['d', 'd'], ReedlineEvent::Multiple(vec![
        ReedlineEvent::Edit(vec![EditCommand::CutCurrentLine])]))]
    #[case(&['d', 'w'], ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::CutWordRight])]))]
    fn test_reedline_move(#[case] input: &[char], #[case] expected: ReedlineEvent) {
        let res = vi_parse(input);
        let output = res.to_reedline_event();

        assert_eq!(output, expected);
    }
}
