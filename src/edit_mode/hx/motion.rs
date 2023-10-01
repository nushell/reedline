use std::iter::Peekable;

use crate::{EditCommand, Hx, ReedlineEvent};

use super::parser::{ParseResult, ReedlineOption};

pub fn parse_motion<'iter, I>(
    input: &mut Peekable<I>,
    command_char: Option<char>,
) -> ParseResult<Motion>
where
    I: Iterator<Item = &'iter char>,
{
    match input.peek() {
        Some('h') => {
            let _ = input.next();
            ParseResult::Valid(Motion::Left)
        }
        Some('l') => {
            let _ = input.next();
            ParseResult::Valid(Motion::Right)
        }
        Some('j') => {
            let _ = input.next();
            ParseResult::Valid(Motion::Down)
        }
        Some('k') => {
            let _ = input.next();
            ParseResult::Valid(Motion::Up)
        }
        Some('b') => {
            let _ = input.next();
            ParseResult::Valid(Motion::PreviousWord)
        }
        Some('B') => {
            let _ = input.next();
            ParseResult::Valid(Motion::PreviousBigWord)
        }
        Some('w') => {
            let _ = input.next();
            ParseResult::Valid(Motion::NextWord)
        }
        Some('W') => {
            let _ = input.next();
            ParseResult::Valid(Motion::NextBigWord)
        }
        Some('e') => {
            let _ = input.next();
            ParseResult::Valid(Motion::NextWordEnd)
        }
        Some('E') => {
            let _ = input.next();
            ParseResult::Valid(Motion::NextBigWordEnd)
        }
        Some('f') => {
            let _ = input.next();
            match input.peek() {
                Some(&x) => {
                    input.next();
                    ParseResult::Valid(Motion::RightUntil(*x))
                }
                None => ParseResult::Incomplete,
            }
        }
        Some('t') => {
            let _ = input.next();
            match input.peek() {
                Some(&x) => {
                    input.next();
                    ParseResult::Valid(Motion::RightBefore(*x))
                }
                None => ParseResult::Incomplete,
            }
        }
        Some('F') => {
            let _ = input.next();
            match input.peek() {
                Some(&x) => {
                    input.next();
                    ParseResult::Valid(Motion::LeftUntil(*x))
                }
                None => ParseResult::Incomplete,
            }
        }
        Some('T') => {
            let _ = input.next();
            match input.peek() {
                Some(&x) => {
                    input.next();
                    ParseResult::Valid(Motion::LeftBefore(*x))
                }
                None => ParseResult::Incomplete,
            }
        }
        ch if ch == command_char.as_ref().as_ref() && command_char.is_some() => {
            let _ = input.next();
            ParseResult::Valid(Motion::Line)
        }
        None => ParseResult::Incomplete,
        _ => ParseResult::Invalid,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    NextWord,
    NextBigWord,
    NextWordEnd,
    NextBigWordEnd,
    PreviousWord,
    PreviousBigWord,
    Line,
    Start,
    End,
    RightUntil(char),
    RightBefore(char),
    LeftUntil(char),
    LeftBefore(char),
}

impl Motion {
    pub fn to_reedline(&self, hx_state: &mut Hx) -> Vec<ReedlineOption> {
        match self {
            Motion::Left => vec![ReedlineOption::Event(ReedlineEvent::UntilFound(vec![
                ReedlineEvent::MenuLeft,
                ReedlineEvent::Left,
            ]))],
            Motion::Right => vec![ReedlineOption::Event(ReedlineEvent::UntilFound(vec![
                ReedlineEvent::HistoryHintComplete,
                ReedlineEvent::MenuRight,
                ReedlineEvent::Right,
            ]))],
            Motion::Up => vec![ReedlineOption::Event(ReedlineEvent::UntilFound(vec![
                ReedlineEvent::MenuUp,
                ReedlineEvent::Up,
            ]))],
            Motion::Down => vec![ReedlineOption::Event(ReedlineEvent::UntilFound(vec![
                ReedlineEvent::MenuDown,
                ReedlineEvent::Down,
            ]))],
            Motion::NextWord => vec![ReedlineOption::Edit(EditCommand::MoveWordRightStart)],
            Motion::NextBigWord => vec![ReedlineOption::Edit(EditCommand::MoveBigWordRightStart)],
            Motion::NextWordEnd => vec![ReedlineOption::Edit(EditCommand::MoveWordRightEnd)],
            Motion::NextBigWordEnd => vec![ReedlineOption::Edit(EditCommand::MoveBigWordRightEnd)],
            Motion::PreviousWord => vec![ReedlineOption::Edit(EditCommand::MoveWordLeft)],
            Motion::PreviousBigWord => vec![ReedlineOption::Edit(EditCommand::MoveBigWordLeft)],
            Motion::Line => vec![], // Placeholder as unusable standalone motion
            Motion::Start => vec![ReedlineOption::Edit(EditCommand::MoveToLineStart)],
            Motion::End => vec![ReedlineOption::Edit(EditCommand::MoveToLineEnd)],
            Motion::RightUntil(ch) => {
                hx_state.last_char_search = Some(HxCharSearch::ToRight(*ch));
                vec![ReedlineOption::Edit(EditCommand::MoveRightUntil(*ch))]
            }
            Motion::RightBefore(ch) => {
                hx_state.last_char_search = Some(HxCharSearch::TillRight(*ch));
                vec![ReedlineOption::Edit(EditCommand::MoveRightBefore(*ch))]
            }
            Motion::LeftUntil(ch) => {
                hx_state.last_char_search = Some(HxCharSearch::ToLeft(*ch));
                vec![ReedlineOption::Edit(EditCommand::MoveLeftUntil(*ch))]
            }
            Motion::LeftBefore(ch) => {
                hx_state.last_char_search = Some(HxCharSearch::TillLeft(*ch));
                vec![ReedlineOption::Edit(EditCommand::MoveLeftBefore(*ch))]
            }
        }
    }
}

/// Vi left-right motions to or till a character.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum HxCharSearch {
    /// f
    ToRight(char),
    /// F
    ToLeft(char),
    /// t
    TillRight(char),
    /// T
    TillLeft(char),
}

impl HxCharSearch {
    /// Swap the direction of the to or till for ','
    pub fn reverse(&self) -> Self {
        match self {
            HxCharSearch::ToRight(c) => HxCharSearch::ToLeft(*c),
            HxCharSearch::ToLeft(c) => HxCharSearch::ToRight(*c),
            HxCharSearch::TillRight(c) => HxCharSearch::TillLeft(*c),
            HxCharSearch::TillLeft(c) => HxCharSearch::TillRight(*c),
        }
    }

    pub fn to_move(&self) -> EditCommand {
        match self {
            HxCharSearch::ToRight(c) => EditCommand::MoveRightUntil(*c),
            HxCharSearch::ToLeft(c) => EditCommand::MoveLeftUntil(*c),
            HxCharSearch::TillRight(c) => EditCommand::MoveRightBefore(*c),
            HxCharSearch::TillLeft(c) => EditCommand::MoveLeftBefore(*c),
        }
    }

    pub fn to_cut(&self) -> EditCommand {
        match self {
            HxCharSearch::ToRight(c) => EditCommand::CutRightUntil(*c),
            HxCharSearch::ToLeft(c) => EditCommand::CutLeftUntil(*c),
            HxCharSearch::TillRight(c) => EditCommand::CutRightBefore(*c),
            HxCharSearch::TillLeft(c) => EditCommand::CutLeftBefore(*c),
        }
    }
}
