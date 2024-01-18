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
pub enum ParseResult<T> {
    Valid(T),
    Incomplete,
    Invalid,
}

impl<T> ParseResult<T> {
    fn is_invalid(&self) -> bool {
        match self {
            ParseResult::Valid(_) => false,
            ParseResult::Incomplete => false,
            ParseResult::Invalid => true,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParsedViSequence {
    multiplier: Option<usize>,
    command: Option<Command>,
    count: Option<usize>,
    motion: ParseResult<Motion>,
}

impl ParsedViSequence {
    pub fn is_valid(&self) -> bool {
        !self.motion.is_invalid()
    }

    pub fn is_complete(&self) -> bool {
        match (&self.command, &self.motion) {
            (None, ParseResult::Valid(_)) => true,
            (Some(Command::Incomplete), _) => false,
            (Some(cmd), ParseResult::Incomplete) if !cmd.requires_motion() => true,
            (Some(_), ParseResult::Valid(_)) => true,
            (Some(cmd), ParseResult::Incomplete) if cmd.requires_motion() => false,
            _ => false,
        }
    }

    /// Combine `multiplier` and `count` as vim only considers the product
    ///
    /// Default return value: 1
    ///
    /// ### Note:
    ///
    /// <https://github.com/vim/vim/blob/140f6d0eda7921f2f0b057ec38ed501240903fc3/runtime/doc/motion.txt#L64-L70>
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

    pub fn enters_insert_mode(&self) -> bool {
        matches!(
            (&self.command, &self.motion),
            (Some(Command::EnterViInsert), ParseResult::Incomplete)
                | (Some(Command::EnterViAppend), ParseResult::Incomplete)
                | (Some(Command::ChangeToLineEnd), ParseResult::Incomplete)
                | (Some(Command::AppendToEnd), ParseResult::Incomplete)
                | (Some(Command::PrependToStart), ParseResult::Incomplete)
                | (Some(Command::RewriteCurrentLine), ParseResult::Incomplete)
                | (
                    Some(Command::SubstituteCharWithInsert),
                    ParseResult::Incomplete
                )
                | (Some(Command::HistorySearch), ParseResult::Incomplete)
                | (Some(Command::Change), ParseResult::Valid(_))
        )
    }

    pub fn to_reedline_event(&self, vi_state: &mut Vi) -> ReedlineEvent {
        match (&self.multiplier, &self.command, &self.count, &self.motion) {
            (_, Some(command), None, ParseResult::Incomplete) => {
                let events = self.apply_multiplier(Some(command.to_reedline(vi_state)));
                match &events {
                    ReedlineEvent::None => {}
                    event => vi_state.previous = Some(event.clone()),
                }
                events
            }
            // This case handles all combinations of commands and motions that could exist
            (_, Some(command), _, ParseResult::Valid(motion)) => {
                let events =
                    self.apply_multiplier(command.to_reedline_with_motion(motion, vi_state));
                match &events {
                    ReedlineEvent::None => {}
                    event => vi_state.previous = Some(event.clone()),
                }
                events
            }
            (_, None, _, ParseResult::Valid(motion)) => {
                self.apply_multiplier(Some(motion.to_reedline(vi_state)))
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

pub fn parse<'iter, I>(input: &mut Peekable<I>) -> ParsedViSequence
where
    I: Iterator<Item = &'iter char>,
{
    let multiplier = parse_number(input);
    let command = parse_command(input);
    let count = parse_number(input);
    let motion = parse_motion(input, command.as_ref().and_then(Command::whole_line_char));

    ParsedViSequence {
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

    fn vi_parse(input: &[char]) -> ParsedViSequence {
        parse(&mut input.iter().peekable())
    }

    #[test]
    fn test_delete_word() {
        let input = ['d', 'w'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParsedViSequence {
                multiplier: None,
                command: Some(Command::Delete),
                count: None,
                motion: ParseResult::Valid(Motion::NextWord),
            }
        );
        assert_eq!(output.is_valid(), true);
        assert_eq!(output.is_complete(), true);
    }

    #[test]
    fn test_two_delete_word() {
        let input = ['2', 'd', 'w'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParsedViSequence {
                multiplier: Some(2),
                command: Some(Command::Delete),
                count: None,
                motion: ParseResult::Valid(Motion::NextWord),
            }
        );
        assert_eq!(output.is_valid(), true);
        assert_eq!(output.is_complete(), true);
    }

    #[test]
    fn test_two_delete_two_word() {
        let input = ['2', 'd', '2', 'w'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParsedViSequence {
                multiplier: Some(2),
                command: Some(Command::Delete),
                count: Some(2),
                motion: ParseResult::Valid(Motion::NextWord),
            }
        );
        assert_eq!(output.is_valid(), true);
        assert_eq!(output.is_complete(), true);
    }

    #[test]
    fn test_two_delete_twenty_word() {
        let input = ['2', 'd', '2', '0', 'w'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParsedViSequence {
                multiplier: Some(2),
                command: Some(Command::Delete),
                count: Some(20),
                motion: ParseResult::Valid(Motion::NextWord),
            }
        );
        assert_eq!(output.is_valid(), true);
        assert_eq!(output.is_complete(), true);
    }

    #[test]
    fn test_two_delete_two_lines() {
        let input = ['2', 'd', 'd'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParsedViSequence {
                multiplier: Some(2),
                command: Some(Command::Delete),
                count: None,
                motion: ParseResult::Valid(Motion::Line),
            }
        );
        assert_eq!(output.is_valid(), true);
        assert_eq!(output.is_complete(), true);
    }

    #[test]
    fn test_find_action() {
        let input = ['d', 't', 'd'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParsedViSequence {
                multiplier: None,
                command: Some(Command::Delete),
                count: None,
                motion: ParseResult::Valid(Motion::RightBefore('d')),
            }
        );
        assert_eq!(output.is_valid(), true);
        assert_eq!(output.is_complete(), true);
    }

    #[test]
    fn test_has_garbage() {
        let input = ['2', 'd', 'm'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParsedViSequence {
                multiplier: Some(2),
                command: Some(Command::Delete),
                count: None,
                motion: ParseResult::Invalid,
            }
        );
        assert_eq!(output.is_valid(), false);
    }

    #[test]
    fn test_partial_action() {
        let input = ['r'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParsedViSequence {
                multiplier: None,
                command: Some(Command::Incomplete),
                count: None,
                motion: ParseResult::Incomplete,
            }
        );

        assert_eq!(output.is_valid(), true);
        assert_eq!(output.is_complete(), false);
    }

    #[test]
    fn test_partial_motion() {
        let input = ['f'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParsedViSequence {
                multiplier: None,
                command: None,
                count: None,
                motion: ParseResult::Incomplete,
            }
        );
        assert_eq!(output.is_valid(), true);
        assert_eq!(output.is_complete(), false);
    }

    #[test]
    fn test_two_char_action_replace() {
        let input = ['r', 'k'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParsedViSequence {
                multiplier: None,
                command: Some(Command::ReplaceChar('k')),
                count: None,
                motion: ParseResult::Incomplete,
            }
        );

        assert_eq!(output.is_valid(), true);
        assert_eq!(output.is_complete(), true);
    }

    #[test]
    fn test_find_motion() {
        let input = ['2', 'f', 'f'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParsedViSequence {
                multiplier: Some(2),
                command: None,
                count: None,
                motion: ParseResult::Valid(Motion::RightUntil('f')),
            }
        );
        assert_eq!(output.is_valid(), true);
        assert_eq!(output.is_complete(), true);
    }

    #[test]
    fn test_two_up() {
        let input = ['2', 'k'];
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParsedViSequence {
                multiplier: Some(2),
                command: None,
                count: None,
                motion: ParseResult::Valid(Motion::Up),
            }
        );
        assert_eq!(output.is_valid(), true);
        assert_eq!(output.is_complete(), true);
    }

    #[rstest]
    #[case(&['2', 'k'], ReedlineEvent::Multiple(vec![ReedlineEvent::UntilFound(vec![
                ReedlineEvent::MenuUp,
                ReedlineEvent::Up,
            ]), ReedlineEvent::UntilFound(vec![
                ReedlineEvent::MenuUp,
                ReedlineEvent::Up,
            ])]))]
    #[case(&['k'], ReedlineEvent::Multiple(vec![ReedlineEvent::UntilFound(vec![
                ReedlineEvent::MenuUp,
                ReedlineEvent::Up,
            ])]))]
    #[case(&['w'],
        ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::MoveWordRightStart{select:false}])]))]
    #[case(&['W'],
        ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::MoveBigWordRightStart{select:false}])]))]
    #[case(&['2', 'l'], ReedlineEvent::Multiple(vec![
        ReedlineEvent::UntilFound(vec![
                ReedlineEvent::HistoryHintComplete,
                ReedlineEvent::MenuRight,
                ReedlineEvent::Right,
            ]),ReedlineEvent::UntilFound(vec![
                ReedlineEvent::HistoryHintComplete,
                ReedlineEvent::MenuRight,
                ReedlineEvent::Right,
            ]) ]))]
    #[case(&['l'], ReedlineEvent::Multiple(vec![ReedlineEvent::UntilFound(vec![
                ReedlineEvent::HistoryHintComplete,
                ReedlineEvent::MenuRight,
                ReedlineEvent::Right,
            ])]))]
    #[case(&['0'], ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::MoveToLineStart{select:false}])]))]
    #[case(&['$'], ReedlineEvent::Multiple(vec![ReedlineEvent::Edit(vec![EditCommand::MoveToLineEnd{select:false}])]))]
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
        let mut vi = Vi::default();
        let res = vi_parse(input);
        let output = res.to_reedline_event(&mut vi);

        assert_eq!(output, expected);
    }
}
