use super::command::{parse_command, Command};
use super::motion::{parse_motion, Motion};
use crate::{EditCommand, ReedlineEvent, Vi};
use std::iter::Peekable;

#[derive(Debug, Clone)]
pub enum ReedlineOption {
    Event(ReedlineEvent),
    Edit(EditCommand),
    Incomplete,
}

impl ReedlineOption {
    pub fn into_reedline_event(self) -> Option<ReedlineEvent> {
        match self {
            ReedlineOption::Event(event) => Some(event),
            ReedlineOption::Edit(edit) => Some(ReedlineEvent::Edit(vec![edit])),
            ReedlineOption::Incomplete => None,
        }
    }
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

    /// Combine `multiplier` and `count` as vim only considers the product
    ///
    /// Default return value: 1
    ///
    /// ### Note:
    ///
    /// https://github.com/vim/vim/blob/140f6d0eda7921f2f0b057ec38ed501240903fc3/runtime/doc/motion.txt#L64-L70
    fn total_multiplier(&self) -> usize {
        self.multiplier.unwrap_or(1) * self.count.unwrap_or(1)
    }

    fn apply_multiplier(&self, raw_events: Option<Vec<ReedlineOption>>) -> ReedlineEvent {
        if let Some(raw_events) = raw_events {
            let events = std::iter::repeat(raw_events)
                .take(self.total_multiplier())
                .flatten()
                .filter_map(ReedlineOption::into_reedline_event)
                .collect::<Vec<ReedlineEvent>>();

            if events.is_empty() || events.contains(&ReedlineEvent::None) {
                // TODO: Clarify if the `contains(ReedlineEvent::None)` path is relevant
                ReedlineEvent::None
            } else {
                ReedlineEvent::Multiple(events)
            }
        } else {
            ReedlineEvent::None
        }
    }

    pub fn enter_insert_mode(&self) -> bool {
        matches!(
            (&self.command, &self.motion),
            (Some(Command::EnterViInsert), None)
                | (Some(Command::EnterViAppend), None)
                | (Some(Command::ChangeToLineEnd), None)
                | (Some(Command::AppendToEnd), None)
                | (Some(Command::PrependToStart), None)
                | (Some(Command::RewriteCurrentLine), None)
                | (Some(Command::SubstituteCharWithInsert), None)
                | (Some(Command::HistorySearch), None)
                | (Some(Command::Change), Some(_))
        )
    }

    pub fn to_reedline_event(&self) -> ReedlineEvent {
        match (&self.multiplier, &self.command, &self.count, &self.motion) {
            (_, Some(command), None, None) => self.apply_multiplier(Some(command.to_reedline())),
            // This case handles all combinations of commands and motions that could exist
            (_, Some(command), _, Some(motion)) => {
                self.apply_multiplier(command.to_reedline_with_motion(motion))
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

pub fn parse<'iter, I>(vi: &Vi, input: &mut Peekable<I>) -> ParseResult
where
    I: Iterator<Item = &'iter char>,
{
    let multiplier = parse_number(input);
    let command = parse_command(vi, input);
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
        let vi = Vi::default();
        parse(&vi, &mut input.iter().peekable())
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
                motion: Some(Motion::NextWord),
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
                motion: Some(Motion::NextWord),
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
                motion: Some(Motion::NextWord),
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
                motion: Some(Motion::NextWord),
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
    fn test_find_action() {
        let input = ['d', 't', 'd'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParseResult {
                multiplier: None,
                command: Some(Command::Delete),
                count: None,
                motion: Some(Motion::RightBefore('d')),
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
    fn test_find_motion() {
        let input = ['2', 'f', 'f'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParseResult {
                multiplier: Some(2),
                command: Some(Command::MoveRightUntil('f')),
                count: None,
                motion: None,
                valid: true
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
    #[case(&['w'],
        ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::MoveWordRightStart])]))]
    #[case(&['W'],
        ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::MoveBigWordRightStart])]))]
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
    #[case(&['d', 'w'], ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::CutWordRightToNext])]))]
    #[case(&['d', 'W'], ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::CutBigWordRightToNext])]))]
    #[case(&['d', 'e'], ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::CutWordRight])]))]
    #[case(&['d', 'b'], ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::CutWordLeft])]))]
    #[case(&['d', 'B'], ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::CutBigWordLeft])]))]
    fn test_reedline_move(#[case] input: &[char], #[case] expected: ReedlineEvent) {
        let res = vi_parse(input);
        let output = res.to_reedline_event();

        assert_eq!(output, expected);
    }
}
