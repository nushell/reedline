use std::iter::Peekable;

use crate::{edit_mode::vi::ViMode, EditCommand, ReedlineEvent, Vi};

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
        Some('0' | '^') => {
            let _ = input.next();
            ParseResult::Valid(Motion::Start)
        }
        Some('$') => {
            let _ = input.next();
            ParseResult::Valid(Motion::End)
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
        Some(';') => {
            let _ = input.next();
            ParseResult::Valid(Motion::ReplayCharSearch)
        }
        Some(',') => {
            let _ = input.next();
            ParseResult::Valid(Motion::ReverseCharSearch)
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
    ReplayCharSearch,
    ReverseCharSearch,
}

impl Motion {
    pub fn to_reedline(&self, vi_state: &mut Vi) -> Vec<ReedlineOption> {
        let select_mode = vi_state.mode == ViMode::Visual;
        match self {
            Motion::Left => vec![ReedlineOption::Event(ReedlineEvent::UntilFound(vec![
                ReedlineEvent::MenuLeft,
                ReedlineEvent::Edit(vec![EditCommand::MoveLeft {
                    select: select_mode,
                }]),
            ]))],
            Motion::Right => vec![ReedlineOption::Event(ReedlineEvent::UntilFound(vec![
                ReedlineEvent::HistoryHintComplete,
                ReedlineEvent::MenuRight,
                ReedlineEvent::Edit(vec![EditCommand::MoveRight {
                    select: select_mode,
                }]),
            ]))],
            Motion::Up => vec![ReedlineOption::Event(ReedlineEvent::UntilFound(vec![
                ReedlineEvent::MenuUp,
                ReedlineEvent::Up,
                // todo: add EditCommand::MoveLineUp
            ]))],
            Motion::Down => vec![ReedlineOption::Event(ReedlineEvent::UntilFound(vec![
                ReedlineEvent::MenuDown,
                ReedlineEvent::Down,
                // todo: add EditCommand::MoveLineDown
            ]))],
            Motion::NextWord => vec![ReedlineOption::Edit(EditCommand::MoveWordRightStart {
                select: select_mode,
            })],
            Motion::NextBigWord => vec![ReedlineOption::Edit(EditCommand::MoveBigWordRightStart {
                select: select_mode,
            })],
            Motion::NextWordEnd => vec![ReedlineOption::Edit(EditCommand::MoveWordRightEnd {
                select: select_mode,
            })],
            Motion::NextBigWordEnd => {
                vec![ReedlineOption::Edit(EditCommand::MoveBigWordRightEnd {
                    select: select_mode,
                })]
            }
            Motion::PreviousWord => vec![ReedlineOption::Edit(EditCommand::MoveWordLeft {
                select: select_mode,
            })],
            Motion::PreviousBigWord => vec![ReedlineOption::Edit(EditCommand::MoveBigWordLeft {
                select: select_mode,
            })],
            Motion::Line => vec![], // Placeholder as unusable standalone motion
            Motion::Start => vec![ReedlineOption::Edit(EditCommand::MoveToLineStart {
                select: select_mode,
            })],
            Motion::End => vec![ReedlineOption::Edit(EditCommand::MoveToLineEnd {
                select: select_mode,
            })],
            Motion::RightUntil(ch) => {
                vi_state.last_char_search = Some(ViCharSearch::ToRight(*ch));
                vec![ReedlineOption::Edit(EditCommand::MoveRightUntil {
                    c: *ch,
                    select: select_mode,
                })]
            }
            Motion::RightBefore(ch) => {
                vi_state.last_char_search = Some(ViCharSearch::TillRight(*ch));
                vec![ReedlineOption::Edit(EditCommand::MoveRightBefore {
                    c: *ch,
                    select: select_mode,
                })]
            }
            Motion::LeftUntil(ch) => {
                vi_state.last_char_search = Some(ViCharSearch::ToLeft(*ch));
                vec![ReedlineOption::Edit(EditCommand::MoveLeftUntil {
                    c: *ch,
                    select: select_mode,
                })]
            }
            Motion::LeftBefore(ch) => {
                vi_state.last_char_search = Some(ViCharSearch::TillLeft(*ch));
                vec![ReedlineOption::Edit(EditCommand::MoveLeftBefore {
                    c: *ch,
                    select: select_mode,
                })]
            }
            Motion::ReplayCharSearch => {
                if let Some(char_search) = vi_state.last_char_search.as_ref() {
                    vec![ReedlineOption::Edit(char_search.to_move(select_mode))]
                } else {
                    vec![]
                }
            }
            Motion::ReverseCharSearch => {
                if let Some(char_search) = vi_state.last_char_search.as_ref() {
                    vec![ReedlineOption::Edit(
                        char_search.reverse().to_move(select_mode),
                    )]
                } else {
                    vec![]
                }
            }
        }
    }
}

/// Vi left-right motions to or till a character.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ViCharSearch {
    /// f
    ToRight(char),
    /// F
    ToLeft(char),
    /// t
    TillRight(char),
    /// T
    TillLeft(char),
}

impl ViCharSearch {
    /// Swap the direction of the to or till for ','
    pub fn reverse(&self) -> Self {
        match self {
            ViCharSearch::ToRight(c) => ViCharSearch::ToLeft(*c),
            ViCharSearch::ToLeft(c) => ViCharSearch::ToRight(*c),
            ViCharSearch::TillRight(c) => ViCharSearch::TillLeft(*c),
            ViCharSearch::TillLeft(c) => ViCharSearch::TillRight(*c),
        }
    }

    pub fn to_move(&self, select_mode: bool) -> EditCommand {
        match self {
            ViCharSearch::ToRight(c) => EditCommand::MoveRightUntil {
                c: *c,
                select: select_mode,
            },
            ViCharSearch::ToLeft(c) => EditCommand::MoveLeftUntil {
                c: *c,
                select: select_mode,
            },
            ViCharSearch::TillRight(c) => EditCommand::MoveRightBefore {
                c: *c,
                select: select_mode,
            },
            ViCharSearch::TillLeft(c) => EditCommand::MoveLeftBefore {
                c: *c,
                select: select_mode,
            },
        }
    }

    pub fn to_cut(&self) -> EditCommand {
        match self {
            ViCharSearch::ToRight(c) => EditCommand::CutRightUntil(*c),
            ViCharSearch::ToLeft(c) => EditCommand::CutLeftUntil(*c),
            ViCharSearch::TillRight(c) => EditCommand::CutRightBefore(*c),
            ViCharSearch::TillLeft(c) => EditCommand::CutLeftBefore(*c),
        }
    }

    pub fn to_copy(&self) -> EditCommand {
        match self {
            ViCharSearch::ToRight(c) => EditCommand::CopyRightUntil(*c),
            ViCharSearch::TillRight(c) => EditCommand::CopyRightBefore(*c),
            ViCharSearch::ToLeft(c) => EditCommand::CopyLeftUntil(*c),
            ViCharSearch::TillLeft(c) => EditCommand::CopyLeftBefore(*c),
        }
    }
}
